#![warn(clippy::unwrap_used)]
#![warn(clippy::expect_used)]
use std::{env, fs::OpenOptions, io, path::PathBuf};

use log::debug;
use ratatui::{crossterm::{execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}}, layout::{Alignment, Constraint, Direction, Layout}, prelude::CrosstermBackend, style::{Color, Modifier, Style}, widgets::Paragraph, Terminal};
use ratatui::crossterm::event::{self, Event, KeyCode};
use serde_json::Value;
use std::time::{Duration, Instant};

use crate::installer::{Installer, Menu, Page, Signal};

pub mod installer;
pub mod widget;
pub mod drives;
pub mod nix;

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
	let mut pages: Vec<Box<dyn Page>> = vec![];
	pages.push(Box::new(Menu::new()));

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
			if let Some(page) = pages.last_mut() {
				page.render(&installer, f, chunks[1]);
			}
		})?;

		let timeout = tick_rate
			.checked_sub(last_tick.elapsed())
			.unwrap_or_else(|| Duration::from_secs(0));

		if event::poll(timeout)? {
			if let Event::Key(key) = event::read()? {
				debug!("Key event: {key:?}");
				if let Some(page) = pages.last_mut() {
					match page.handle_input(&mut installer, key) {
						Signal::Wait => {
							// Wait
						}
						Signal::Push(new_page) => {
							pages.push(new_page);
						}
						Signal::Pop => {
							pages.pop();
						}
						Signal::PopCount(n) => {
							for _ in 0..n {
								if pages.len() > 1 {
									pages.pop();
								}
							}
						}
						Signal::Unwind => {
							while pages.len() > 1 {
								pages.pop();
							}
						}
						Signal::Quit => {
							debug!("Quit signal received");
							return Ok(());
						}
						Signal::WriteCfg => {
							debug!("WriteCfg signal received");
							// Handle configuration writing here
						}
						sig => {
							// Any other signal is meant to be caught and handled by page widgets
							unreachable!("Uncaught widget signal: {:?}", sig)
						}
					}
				} else {
					// No pages, push the initial page
					pages.push(Box::new(Menu::new()));
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
