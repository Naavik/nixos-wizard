#![warn(clippy::unwrap_used)]
#![warn(clippy::expect_used)]
use std::{env, fs::OpenOptions, io, path::PathBuf};

use log::debug;
use ratatui::{crossterm::{execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}}, layout::{Alignment, Constraint, Direction, Layout}, prelude::CrosstermBackend, style::{Color, Modifier, Style}, text::Line, widgets::Paragraph, Terminal};
use ratatui::crossterm::event::{self, Event, KeyCode};
use serde_json::Value;
use std::time::{Duration, Instant};

use crate::installer::{systempkgs::init_nixpkgs, Installer, Menu, Page, Signal};

pub mod installer;
pub mod widget;
pub mod drives;
pub mod nix;

pub fn styled_block<'a>(lines: Vec<Vec<(Option<(Color,Modifier)>, impl ToString)>>) -> Vec<Line<'a>> {
	lines.into_iter().map(|line| {
		let spans = line.into_iter().map(|(style_opt, text)| {
			let mut span = ratatui::text::Span::raw(text.to_string());
			if let Some((color, modifier)) = style_opt {
				span.style = Style::default().fg(color).add_modifier(modifier);
			}
			span
		}).collect::<Vec<_>>();
		Line::from(spans)
	}).collect()
}

#[macro_export]
macro_rules! attrset {
	{$($key:tt = $val:expr);+ ;} => {{
		let mut parts = vec![];
		$(
			parts.push(format!("{} = {};", stringify!($key).trim_matches('"'), $val));
		)*
		format!("{{ {} }}", parts.join(" "))
  }};
}

#[macro_export]
macro_rules! merge_attrs {
	($($set:expr),* $(,)?) => {{
		let mut merged = String::new();
		$(
			if !$set.starts_with('{') || !$set.ends_with('}') {
				panic!("attrset must be a valid attribute set");
			}
			let inner = $set
			.strip_prefix('{')
			.and_then(|s| s.strip_suffix('}'))
			.unwrap_or("")
			.trim();
			merged.push_str(inner);
		)*
			format!("{{ {merged} }}")
	}};
}

#[macro_export]
macro_rules! list {
	($($item:expr),* $(,)?) => {
		{
			let items = vec![$(format!("{}", $item)),*];
			format!("[{}]", items.join(" "))
		}
	};
}

struct RawModeGuard;

impl RawModeGuard {
	fn new(stdout: &mut io::Stdout) -> anyhow::Result<Self> {
		enable_raw_mode()?;
		execute!(stdout, EnterAlternateScreen)?;
		Ok(Self)
	}
}

impl Drop for RawModeGuard {
	fn drop(&mut self) {
		let _ = disable_raw_mode();
		let _ = execute!(io::stdout(), LeaveAlternateScreen);
	}
}

fn main() -> anyhow::Result<()> {
	unsafe {
		env::set_var("RUST_LOG", "debug");
		env::set_var("RUST_LOG_STYLE", "never");
	};
	let log = Box::new(OpenOptions::new().append(true).create(true).open("tui-debug.log")?);
	env_logger::Builder::from_default_env()
		.format_timestamp(None)
		.target(env_logger::Target::Pipe(log))
		.init();
	debug!("Logger initialized");
	init_nixpkgs();

	let mut stdout = io::stdout();
	let res = {
		let _raw_guard = RawModeGuard::new(&mut stdout)?;
		let backend = CrosstermBackend::new(stdout);
		let mut terminal = Terminal::new(backend)?;
		debug!("Running TUI");
		run_app(&mut terminal)
	};


	if let Err(err) = res {
		eprintln!("Error: {err:?}");
	}

	Ok(())
}

pub fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {

	let mut installer = Installer::new();
	let mut page_stack: Vec<Box<dyn Page>> = vec![];
	page_stack.push(Box::new(Menu::new()));

	let tick_rate = Duration::from_millis(250);
	let mut last_tick = Instant::now();

	loop {
		terminal.draw(|f| {
			let chunks = Layout::default()
				.direction(Direction::Vertical)
				.constraints([
					Constraint::Length(1),           // Header height
					Constraint::Min(0),              // Rest of screen
				])
				.split(f.area());

			// Draw header
			let header = Paragraph::new("Install NixOS")
				.style(Style::default().add_modifier(Modifier::BOLD))
				.alignment(Alignment::Center);
			f.render_widget(header, chunks[0]);

			// Draw current page in the remaining area
			if let Some(page) = page_stack.last_mut() {
				page.render(&mut installer, f, chunks[1]);
			}
		})?;

		let timeout = tick_rate
			.checked_sub(last_tick.elapsed())
			.unwrap_or_else(|| Duration::from_secs(0));

		if event::poll(timeout)? {
			if let Event::Key(key) = event::read()? {
				if let Some(page) = page_stack.last_mut() {
					match page.handle_input(&mut installer, key) {
						Signal::Wait => {
							// Do nothing
						}
						Signal::Push(new_page) => {
							page_stack.push(new_page);
						}
						Signal::Pop => {
							page_stack.pop();
						}
						Signal::PopCount(n) => {
							for _ in 0..n {
								if page_stack.len() > 1 {
									page_stack.pop();
								}
							}
						}
						Signal::Unwind => {
							// Used to return to the main menu
							while page_stack.len() > 1 {
								page_stack.pop();
							}
						}
						Signal::Quit => {
							debug!("Quit signal received");
							return Ok(());
						}
						Signal::WriteCfg => {
							debug!("WriteCfg signal received");

							// Generate JSON configuration
							let config_json = installer.to_json()?;
							debug!("Generated config JSON: {}", serde_json::to_string_pretty(&config_json)?);

							// Create NixSerializer and generate Nix configs
							let output_dir = std::path::PathBuf::from("./nixos-config");
							let use_flake = installer.enable_flakes;
							let serializer = crate::nix::NixWriter::new(config_json, output_dir, use_flake);

							match serializer.write_configs() {
								Ok(cfg) => {
									debug!("system config: {}", cfg.system);
									debug!("disko config: {}", cfg.disko);
									debug!("flake_path: {:?}", cfg.flake_path);
								}
								Err(e) => {
									debug!("Failed to write configuration files: {e}");
									return Err(anyhow::anyhow!("Configuration write failed: {e}"));
								}
							}
						}
						Signal::Error(err) => {
							return Err(anyhow::anyhow!("{}", err));
						}
					}
				} else {
					// No pages, push the initial page
					page_stack.push(Box::new(Menu::new()));
				}
			}
		}

		if last_tick.elapsed() >= tick_rate {
			last_tick = Instant::now();
		}
	}

	/*
		 let _ = disable_raw_mode();
		 execute!(io::stdout(), LeaveAlternateScreen)?;

		 Ok(())
		 */
}
