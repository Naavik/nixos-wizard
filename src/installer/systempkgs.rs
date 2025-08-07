use std::{collections::{BTreeMap, HashSet}, process::Command, sync::LazyLock};

use ratatui::{crossterm::event::KeyCode, layout::Constraint, text::Line};
use serde_json::Value;

use crate::{installer::{Installer, Page, Signal}, styled_block, widget::{ConfigWidget, HelpModal, LineEditor, OptimizedStrList, TableWidget}};

use std::{
    sync::{Arc, RwLock},
    thread,
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

#[derive(Debug, Clone)]
pub struct PackageManager {
    // Maps package name -> original index in nixpkgs list
    available: BTreeMap<String, usize>,
    selected: BTreeMap<String, usize>,
    // Original ordering from nixpkgs for restoration
    original_order: Vec<String>,
    // Cache for filtered results - maps package name to (original_index, fuzzy_score)
    last_filter: Option<String>,
    cached_filtered: BTreeMap<String, (usize, i64)>, // package_name -> (original_index, fuzzy_score)
}

impl PackageManager {
    pub fn new(all_packages: Vec<String>, selected_packages: Vec<String>) -> Self {
        let mut available = BTreeMap::new();
        let mut selected = BTreeMap::new();
        
        // Build the original order and available map
        for (idx, package) in all_packages.iter().enumerate() {
            available.insert(package.clone(), idx);
        }
        
        // Move pre-selected packages to selected map
        for package in selected_packages {
            if let Some(idx) = available.remove(&package) {
                selected.insert(package, idx);
            }
        }
        
        Self {
            available,
            selected,
            original_order: all_packages,
            last_filter: None,
            cached_filtered: BTreeMap::new(),
        }
    }
    
    pub fn move_to_selected(&mut self, package: &str) -> bool {
        if let Some(idx) = self.available.remove(package) {
            self.selected.insert(package.to_string(), idx);
            // Update cached filtered map by removing the package
            self.cached_filtered.remove(package);
            true
        } else {
            false
        }
    }
    
    pub fn move_to_available(&mut self, package: &str) -> bool {
        if let Some(idx) = self.selected.remove(package) {
            self.available.insert(package.to_string(), idx);
            // If we have a cached filter, check if this package matches and add it back
            if let Some(ref filter) = self.last_filter {
                use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
                let matcher = SkimMatcherV2::default();
                if let Some(score) = matcher.fuzzy_match(package, filter) {
                    // Add to cached filtered map with both original index and fuzzy score
                    self.cached_filtered.insert(package.to_string(), (idx, score));
                }
            }
            true
        } else {
            false
        }
    }
    
    pub fn get_available_packages(&self) -> Vec<String> {
        self.available.keys().cloned().collect()
    }
    
    pub fn get_selected_packages(&self) -> Vec<String> {
        self.selected.keys().cloned().collect()
    }
    
    pub fn get_available_filtered(&mut self, filter: &str) -> Vec<String> {
        // Check if we can reuse cached results
        if let Some(ref last_filter) = self.last_filter {
            if last_filter == filter {
                return self.get_sorted_by_score_from_cache();
            }
        }
        
        // Need to recompute
        use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
        let matcher = SkimMatcherV2::default();
        
        let mut filtered_map = BTreeMap::new();
        for (package, &original_idx) in &self.available {
            if let Some(score) = matcher.fuzzy_match(package, filter) {
                filtered_map.insert(package.clone(), (original_idx, score));
            }
        }
        
        // Cache the results
        self.last_filter = Some(filter.to_string());
        self.cached_filtered = filtered_map;
        
        self.get_sorted_by_score_from_cache()
    }
    
    /// Get packages from cache sorted by fuzzy match score (best matches first)
    fn get_sorted_by_score_from_cache(&self) -> Vec<String> {
        let mut packages_with_score: Vec<_> = self.cached_filtered.iter()
            .map(|(package, &(_, score))| (package.clone(), score))
            .collect();
        packages_with_score.sort_by_key(|(_, score)| -score); // Negative for descending order
        packages_with_score.into_iter().map(|(package, _)| package).collect()
    }
    
    pub fn contains_available(&self, package: &str) -> bool {
        self.available.contains_key(package)
    }
    
    pub fn contains_selected(&self, package: &str) -> bool {
        self.selected.contains_key(package)
    }
    
    /// Get current available list without recomputing if filter hasn't changed
    pub fn get_current_available(&self) -> Vec<String> {
        if self.last_filter.is_some() {
            self.get_sorted_by_score_from_cache()
        } else {
            self.get_available_packages()
        }
    }
}

pub struct SystemPackages {
	package_manager: PackageManager,
	selected: OptimizedStrList,
	available: OptimizedStrList,
	search_bar: LineEditor,
	help_modal: HelpModal<'static>,
	current_filter: Option<String>,
}

impl SystemPackages {
	pub fn new(selected_pkgs: Vec<String>, available_pkgs: Vec<String>) -> Self {
		let package_manager = PackageManager::new(available_pkgs.clone(), selected_pkgs.clone());
		
		let mut available = OptimizedStrList::new("Available Packages", package_manager.get_available_packages());
		available.focus();
		let selected = OptimizedStrList::new("Selected Packages", package_manager.get_selected_packages());
		let search_bar = LineEditor::new("Search", Some("Enter a package name..."));
		
		let help_content = styled_block(vec![
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "Tab"), (None, " - Switch between lists and search")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate package lists")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "Enter"), (None, " - Add/remove package to/from selection")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "/"), (None, " - Focus search bar")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "Esc"), (None, " - Return to main menu")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Search filters packages in real-time as you type.")],
			vec![(None, "Filter persists when adding/removing packages.")],
			vec![(None, "Selected packages will be installed on your NixOS system.")],
		]);
		let help_modal = HelpModal::new("System Packages", help_content);
		
		Self {
			package_manager,
			selected,
			available,
			search_bar,
			help_modal,
			current_filter: None,
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
	
	fn update_available_list(&mut self) {
		// Use the cache-aware method that avoids recomputation
		let items = self.package_manager.get_current_available();
		self.available.set_items(items);
	}
	
	fn set_filter(&mut self, filter: Option<String>) {
		self.current_filter = filter.clone();
		let items = if let Some(filter) = filter {
			self.package_manager.get_available_filtered(&filter)
		} else {
			self.package_manager.get_available_packages()
		};
		self.available.set_items(items);
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
		
		// Render help modal on top
		self.help_modal.render(f, area);
	}

	fn handle_input(&mut self, installer: &mut super::Installer, event: ratatui::crossterm::event::KeyEvent) -> super::Signal {
		match event.code {
			KeyCode::Char('?') => {
				self.help_modal.toggle();
				return super::Signal::Wait;
			}
			KeyCode::Esc if self.help_modal.visible => {
				self.help_modal.hide();
				return super::Signal::Wait;
			}
			_ if self.help_modal.visible => {
				return super::Signal::Wait;
			}
			_ => {}
		}
		
		if event.code == KeyCode::Char('/') && !self.search_bar.is_focused() {
			self.search_bar.focus();
			self.available.unfocus();
			self.selected.unfocus();
			return super::Signal::Wait;
		}
		if self.search_bar.is_focused() {
			match event.code {
				KeyCode::Enter | KeyCode::Tab => {
					self.focus_available();
					Signal::Wait
				}
				KeyCode::Esc => {
					self.search_bar.clear();
					self.set_filter(None);
					self.focus_available();
					Signal::Wait
				}
				_ => {
					let signal = self.search_bar.handle_input(event);
					// Apply real-time filtering on every keystroke
					let filter_text = self.search_bar.get_value()
						.and_then(|v| v.as_str().map(|s| s.to_string()));
					
					if let Some(filter) = filter_text {
						if !filter.is_empty() {
							self.set_filter(Some(filter));
						} else {
							self.set_filter(None);
						}
					} else {
						self.set_filter(None);
					}
					signal
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
						if self.package_manager.move_to_available(pkg) {
							installer.system_pkgs.retain(|p| p != pkg);
							self.selected.set_items(self.package_manager.get_selected_packages());
							self.update_available_list(); // Maintain current filter
							self.selected.selected_idx = selected_idx.min(self.selected.len().saturating_sub(1));
						}
					}
					Signal::Wait
				}
				_ => {
					Signal::Wait
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
						if self.package_manager.move_to_selected(pkg) {
							installer.system_pkgs.push(pkg.clone());
							self.selected.set_items(self.package_manager.get_selected_packages());
							self.update_available_list(); // Maintain current filter
							self.available.selected_idx = selected_idx.min(self.available.len().saturating_sub(1));
						}
					}
					Signal::Wait
				}
				_ => {
					Signal::Wait
				}
			}
		} else {
			self.focus_available();
			Signal::Wait
		}
	}

	fn get_help_content(&self) -> (String, Vec<Line>) {
		let help_content = styled_block(vec![
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "Tab"), (None, " - Switch between lists and search")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate package lists")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "Enter"), (None, " - Add/remove package to/from selection")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "/"), (None, " - Focus search bar")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "Esc"), (None, " - Return to main menu")],
			vec![(Some((ratatui::style::Color::Yellow, ratatui::style::Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Search filters packages in real-time as you type.")],
			vec![(None, "Filter persists when adding/removing packages.")],
			vec![(None, "Selected packages will be installed on your NixOS system.")],
		]);
		("System Packages".to_string(), help_content)
	}
}
