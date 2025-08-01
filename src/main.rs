#![warn(clippy::unwrap_used)]
#![warn(clippy::expect_used)]
use std::{env, fs::OpenOptions, io, path::PathBuf};

use log::debug;
use ratatui::{crossterm::{execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}}, prelude::CrosstermBackend, Terminal};
use ratatui::crossterm::event::{self, Event, KeyCode};
use serde_json::Value;
use std::time::{Duration, Instant};

use crate::{drive::{DiskPlan, DiskPlanIR}, nix::{fmt_nix, NixSerializer}, widget::ConfigWidget};
use crate::widget::ConfigMenu;


pub mod page;
pub mod widget;
pub mod nix;
pub mod drive;

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
	let mut app = ConfigMenu::new();
	let tick_rate = Duration::from_millis(250);
	let mut last_tick = Instant::now();

	let config: Option<Value> = loop {
		terminal.draw(|f| {
			let size = f.area();
			app.render(f, size);
		})?;

		let timeout = tick_rate
			.checked_sub(last_tick.elapsed())
			.unwrap_or_else(|| Duration::from_secs(0));

		if event::poll(timeout)? {
			if let Event::Key(key) = event::read()? {
				match key.code {
					KeyCode::Char('q') => {
						if app.titles.is_focused() {
							break None;
						} else {
							app.handle_input(key);
						}
					}
					_ => {
						if let Some(value) = app.handle_input(key) {
							break Some(value);
						}
					}
				}
			}
		}

		if last_tick.elapsed() >= tick_rate {
			last_tick = Instant::now();
		}
	};

	let _ = disable_raw_mode();
	execute!(io::stdout(), LeaveAlternateScreen)?;

	if let Some(config) = config {
		let serializer = NixSerializer::new(config["config"].clone(), PathBuf::from("./."), false);
		let serialized = serializer.mk_nix_config()?;
		println!("Generated Nix Configuration:\n\n{serialized}");
	}

	Ok(())
}
