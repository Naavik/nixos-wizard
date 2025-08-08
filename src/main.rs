use std::{env, fs::OpenOptions, io};

use log::debug;
use ratatui::{crossterm::{execute, terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen}}, layout::{Alignment, Constraint, Direction, Layout}, prelude::CrosstermBackend, style::{Color, Modifier, Style}, text::Line, widgets::Paragraph, Terminal};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use std::time::{Duration, Instant};

use crate::installer::{systempkgs::init_nixpkgs, InstallProgress, Installer, Menu, Page, Signal};

pub mod installer;
pub mod widget;
pub mod drives;
pub mod nixgen;

type LineStyle = Option<(Color, Modifier)>;
pub fn styled_block<'a>(lines: Vec<Vec<(LineStyle, impl ToString)>>) -> Vec<Line<'a>> {
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
macro_rules! command {
    ($cmd:expr, $($arg:expr),* $(,)?) => {{
			use std::process::Command;
			let mut c = Command::new($cmd);
				c.args(&[$($arg.to_string()),*]);
				c
		}};
    ($cmd:expr) => {{
			use std::process::Command;
			let c = Command::new($cmd);
				c
		}};
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
		if let Ok("linux") = env::var("TERM").as_deref() {
			// we are in a dumb terminal
			// so we have to explicitly clear the terminal before we start rendering stuff
			// because in this context, entering an alternate screen doesn't do that for us
			execute!(stdout, Clear(ClearType::All))?;
		}
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
	let uid = nix::unistd::getuid();
	log::debug!("UID: {uid}");
	if uid.as_raw() != 0 {
		eprintln!("nixos-wizard: This installer must be run as root.");
		std::process::exit(1);
	}
	// Set up panic handler to clean up terminal state
	std::panic::set_hook(Box::new(|panic_info| {
		use ratatui::crossterm::{execute, terminal::{disable_raw_mode, LeaveAlternateScreen}};
		use std::io::{self, Write};

		// Try to clean up terminal state
		let _ = disable_raw_mode();
		let _ = execute!(io::stdout(), LeaveAlternateScreen);

		// Print panic info to stderr
		eprintln!("==================================================");
		eprintln!("NIXOS INSTALLER PANIC - Terminal state restored!");
		eprintln!("==================================================");
		eprintln!("Panic occurred: {panic_info}");
		eprintln!("==================================================");

		// Also try to write to log file
		if let Ok(mut file) = OpenOptions::new().append(true).create(true).open("tui-debug.log") {
			let _ = writeln!(file, "PANIC: {panic_info}");
		}
	}));

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

	debug!("Exiting TUI");


	if let Err(err) = res {
		log::error!("{err}");
		eprintln!("Error: {err:?}");
	}

	Ok(())
}

pub fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {

	let mut installer = Installer::new();
	let mut page_stack: Vec<Box<dyn Page>> = vec![];
	page_stack.push(Box::new(Menu::new()));

	let tick_rate = Duration::from_millis(100);
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

			// Draw header with three columns: empty, title, help text
			let header_chunks = Layout::default()
				.direction(Direction::Horizontal)
				.constraints([
					Constraint::Percentage(33),  // Left section (empty)
					Constraint::Percentage(34),  // Middle section (title)
					Constraint::Percentage(33),  // Right section (help)
				])
				.split(chunks[0]);

			// Title in center
			let title = Paragraph::new("Install NixOS")
				.style(Style::default().add_modifier(Modifier::BOLD))
				.alignment(Alignment::Center);
			f.render_widget(title, header_chunks[1]);

			// Help text on right
			let help_text = Paragraph::new("Press '?' for help")
				.style(Style::default().fg(Color::Gray))
				.alignment(Alignment::Center);
			f.render_widget(help_text, header_chunks[0]);

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
				if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
				    panic!("Test panic - this should show in terminal after cleanup!");
				}

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
							let serializer = crate::nixgen::NixWriter::new(config_json, output_dir, use_flake);

							match serializer.write_configs() {
								Ok(cfg) => {
									debug!("system config: {}", cfg.system);
									debug!("disko config: {}", cfg.disko);
									debug!("flake_path: {:?}", cfg.flake_path);
									std::fs::write("/tmp/configuration.nix", cfg.system)?;
									std::fs::write("/tmp/disko.nix", cfg.disko)?;
									page_stack.push(Box::new(InstallProgress::new(installer.clone())?));
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
