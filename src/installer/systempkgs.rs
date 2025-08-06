use std::{cell::LazyCell, collections::HashSet, fs, process::Command, sync::LazyLock};

use ratatui::{crossterm::event::KeyCode, layout::Constraint, text::Line};
use serde_json::Value;

use crate::{installer::{Installer, Page, Signal}, styled_block, widget::{ConfigWidget, LineEditor, StrList, TableWidget}};

use std::{
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

pub static NIXPKGS: LazyLock<Arc<RwLock<Option<Vec<String>>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(None)));

pub fn init_nixpkgs() {
	let pkgs_ref = NIXPKGS.clone();
	thread::spawn(move || {
		let pkgs = fetch_nixpkgs().unwrap_or_else(|e| {
			eprintln!("Failed to fetch nixpkgs: {}", e);
			vec![]
		});
		let mut pkgs_lock = pkgs_ref.write().unwrap();
		*pkgs_lock = Some(pkgs);
	});
}

pub fn fetch_nixpkgs() -> anyhow::Result<Vec<String>> {
	let json: Value = {
		let output = Command::new("nix")
			.args(["search", "nixpkgs", "^", "--json"])
			.output()?;


		if !output.status.success() {
			return Err(anyhow::anyhow!(
					"nix-env command failed with status: {}",
					output.status
			));
		}

		serde_json::from_slice(&output.stdout)?
	};
	let pkgs_object = json
		.as_object()
		.ok_or_else(|| anyhow::anyhow!("Expected JSON object"))?;

	let mut pkgs = Vec::with_capacity(pkgs_object.len());

	for key in pkgs_object.keys() {
		let stripped = key.strip_prefix("legacyPackages.x86_64-linux.").unwrap_or(key);
		pkgs.push(stripped.to_string());
	}

	let mut seen = HashSet::new();
	pkgs.retain(|pkg| seen.insert(pkg.clone()));

	Ok(pkgs)

}

pub struct SystemPackages {
	selected: StrList,
	available: StrList,
	search_bar: LineEditor
}

impl SystemPackages {
	pub fn new(selected: Vec<String>, available: Vec<String>) -> Self {
		let mut available = StrList::new("Available Packages", available);
		available.focus();
		let mut selected = StrList::new("Selected Packages", selected);
		let search_bar = LineEditor::new("Search", Some("Enter a package name..."));
		selected.sort();
		available.sort();
		Self {
			selected,
			available,
			search_bar
		}
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		let sys_pkgs: Vec<Vec<String>> = installer.system_pkgs.clone().into_iter().map(|item| vec![item]).collect();
		if sys_pkgs.is_empty() {
			return None;
		}
		Some(Box::new(TableWidget::new(
			"",
			vec![Constraint::Percentage(100)],
			vec!["Packages".into()],
			sys_pkgs
		)) as Box<dyn ConfigWidget>)
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"System Packages".to_string(),
			styled_block(vec![
				vec![(None, "Select extra system packages to include in the configuration")],
			])
		)
	}
	fn focus_bar(&mut self) {
		self.search_bar.focus();
		self.available.unfocus();
		self.selected.unfocus();
	}
	fn focus_available(&mut self) {
		self.available.focus();
		self.search_bar.unfocus();
		self.selected.unfocus();
	}
	fn focus_selected(&mut self) {
		self.selected.focus();
		self.search_bar.unfocus();
		self.available.unfocus();
	}
}

impl Page for SystemPackages {
	fn render(&mut self, installer: &mut super::Installer, f: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
		let hor_chunks = ratatui::layout::Layout::default()
			.direction(ratatui::layout::Direction::Horizontal)
			.constraints([
				ratatui::layout::Constraint::Percentage(50),
				ratatui::layout::Constraint::Percentage(50),
			].as_ref())
			.split(area);
		let vert_chunks_left = ratatui::layout::Layout::default()
			.direction(ratatui::layout::Direction::Vertical)
			.constraints([
				ratatui::layout::Constraint::Length(5),
				ratatui::layout::Constraint::Min(0),
			].as_ref())
			.split(hor_chunks[0]);
		let vert_chunks_right = ratatui::layout::Layout::default()
			.direction(ratatui::layout::Direction::Vertical)
			.constraints([
				ratatui::layout::Constraint::Length(5),
				ratatui::layout::Constraint::Min(0),
			].as_ref())
			.split(hor_chunks[1]);

		self.selected.render(f, vert_chunks_left[1]);
		self.search_bar.render(f, vert_chunks_right[0]);
		self.available.render(f, vert_chunks_right[1]);
	}

	fn handle_input(&mut self, installer: &mut super::Installer, event: ratatui::crossterm::event::KeyEvent) -> super::Signal {
		if event.code == KeyCode::Char('/') && !self.search_bar.is_focused() {
			self.search_bar.focus();
			self.available.unfocus();
			self.selected.unfocus();
			return super::Signal::Wait;
		}
		if self.search_bar.is_focused() {
			match event.code {
				KeyCode::Enter | KeyCode::Tab => {
					let Some(filter) = self.search_bar.get_value() else {
						self.available.set_filter(None::<&str>);
						self.focus_available();
						return super::Signal::Wait;
					};
					self.available.set_filter(Some(filter.as_str().unwrap()));
					self.focus_available();
					self.search_bar.clear();
					Signal::Wait
				}
				KeyCode::Esc => {
					self.search_bar.clear();
					self.available.set_filter(None::<&str>);
					self.focus_available();
					Signal::Wait
				}
				_ => {
					self.search_bar.handle_input(event)
				}
			}
		} else if self.selected.is_focused() {
			match event.code {
				KeyCode::Esc | KeyCode::Char('q') => {
					Signal::Pop
				}
				KeyCode::Down | KeyCode::Char('j') => {
					self.selected.next_item();
					Signal::Wait
				}
				KeyCode::Up | KeyCode::Char('k') => {
					self.selected.previous_item();
					Signal::Wait
				}
				KeyCode::Tab => {
					self.focus_available();
					Signal::Wait
				}
				KeyCode::Enter => {
					let selected_idx = self.selected.selected_idx;
					if let Some(pkg) = self.selected.selected_item() {
						installer.system_pkgs.retain(|p| p != pkg);
						self.available.push_sort_unique(pkg.clone());
						self.selected.remove_selected();
						self.selected.selected_idx = selected_idx.min(self.selected.items.len().saturating_sub(1));
					}
					Signal::Wait
				}
				_ => {
					self.selected.handle_input(event)
				}
			}
		} else if self.available.is_focused() {
			match event.code {
				KeyCode::Esc | KeyCode::Char('q') => {
					Signal::Pop
				}
				KeyCode::Down | KeyCode::Char('j') => {
					self.available.next_item();
					Signal::Wait
				}
				KeyCode::Up | KeyCode::Char('k') => {
					self.available.previous_item();
					Signal::Wait
				}
				KeyCode::Tab => {
					self.focus_selected();
					Signal::Wait
				}
				KeyCode::Enter => {
					let selected_idx = self.available.selected_idx;
					if let Some(pkg) = self.available.selected_item() {
						installer.system_pkgs.push(pkg.clone());
						self.selected.push_sort_unique(pkg.clone());
						self.available.remove_selected();
						self.available.selected_idx = selected_idx.min(self.available.items.len().saturating_sub(1));
					}
					Signal::Wait
				}
				_ => {
					self.available.handle_input(event)
				}
			}
		} else {
			self.focus_available();
			Signal::Wait
		}
	}
}
