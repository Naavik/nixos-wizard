use std::{fmt::{Debug, Display}, io::Write, process::{Command, Stdio}};

use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Direction, Layout, Rect}, style::{Color, Modifier}, text::Line, Frame};
use serde_json::Value;

use crate::{drives::{part_table, Disk, DiskItem}, installer::{systempkgs::NIXPKGS, users::User}, styled_block, widget::{Button, CheckBox, ConfigWidget, InfoBox, LineEditor, StrList, WidgetBox, WidgetBoxBuilder}};

const HIGHLIGHT: Option<(Color,Modifier)> = Some((Color::Yellow, Modifier::BOLD));

pub mod drivepages;
pub mod users;
pub mod systempkgs;
use users::UserAccounts;
use drivepages::Drives;
use systempkgs::{fetch_nixpkgs,SystemPackages};

#[derive(Default)]
pub struct Installer {
	pub flake_path: Option<String>,
	pub language: Option<String>,
	pub keyboard_layout: Option<String>,
	pub locale: Option<String>,
	pub enable_flakes: bool,
	pub bootloader: Option<String>,
	pub use_swap: bool,
	pub root_passwd_hash: Option<String>, // Hashed
	pub users: Vec<User>,
	pub profile: Option<String>,
	pub hostname: Option<String>,
	pub kernels: Option<Vec<String>>,
	pub audio_backend: Option<String>,
	pub greeter: Option<String>,
	pub system_pkgs: Vec<String>,
	pub desktop_environment: Option<String>,
	pub network_backend: Option<String>,
	pub timezone: Option<String>,


	pub drives: Vec<Disk>,
	pub selected_drive: Option<usize>, // drive index
	pub selected_partition: Option<u64>, // partition id

	pub drive_config: Option<Disk>,
	pub use_auto_drive_config: bool,


	pub drive_config_display: Option<Vec<DiskItem>>,

	/// Used as an escape hatch for inter-page communication
	/// If you can't find a good way to pass a value from one page to another
	/// Store it here, and use mem::take() on it in the receiving page
	pub shared_register: Option<Value>,
}

impl Installer {
	pub fn new() -> Self {
		Self::default()
	}
	pub fn make_drive_config_display(&mut self) {
		let Some(drive) = &self.drive_config else {
			self.drive_config_display = None;
			return;
		};
		self.drive_config_display = Some(drive.layout().to_vec())
	}
}

pub enum Signal {
	Wait,
	Push(Box<dyn Page>),
	Pop,
	PopCount(usize),
	Quit,
	WriteCfg,
	Unwind, // Pop until we get back to the menu
	Error(anyhow::Error), // Used for error handling, like when a drive is not selected
}

impl Debug for Signal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Wait => write!(f, "Signal::Wait"),
			Self::Push(_) => write!(f, "Signal::Push"),
			Self::Pop => write!(f, "Signal::Pop"),
			Self::PopCount(n) => write!(f, "Signal::PopCount({n})"),
			Self::Quit => write!(f, "Signal::Quit"),
			Self::WriteCfg => write!(f, "Signal::WriteCfg"),
			Self::Unwind => write!(f, "Signal::Unwind"),
			Self::Error(err) => write!(f, "Signal::Error({err})"),
		}
	}
}

pub trait Page {
	fn render(&mut self, installer: &mut Installer, f: &mut Frame, area: Rect);
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal;
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum MenuPages {
	SourceFlake = 0,
	Language = 1,
	KeyboardLayout = 2,
	Locale = 3,
	EnableFlakes = 4,
	Drives = 5,
	Bootloader = 6,
	Swap = 7,
	Hostname = 8,
	RootPassword = 9,
	UserAccounts = 10,
	Profile = 11,
	Greeter = 12,
	DesktopEnvironment = 13,
	Audio = 14,
	Kernels = 15,
	SystemPackages = 16,
	Network = 17,
	Timezone = 18,
}

impl MenuPages {
	pub fn all_pages() -> &'static [MenuPages] {
		&[
			MenuPages::SourceFlake,
			MenuPages::Language,
			MenuPages::KeyboardLayout,
			MenuPages::Locale,
			MenuPages::EnableFlakes,
			MenuPages::Drives,
			MenuPages::Bootloader,
			MenuPages::Swap,
			MenuPages::Hostname,
			MenuPages::RootPassword,
			MenuPages::UserAccounts,
			MenuPages::Profile,
			MenuPages::Greeter,
			MenuPages::DesktopEnvironment,
			MenuPages::Audio,
			MenuPages::Kernels,
			MenuPages::SystemPackages,
			MenuPages::Network,
			MenuPages::Timezone,
			]
	}
}

impl Display for MenuPages {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let s = match self {
			MenuPages::SourceFlake => "Source Flake",
			MenuPages::Language => "Language",
			MenuPages::KeyboardLayout => "Keyboard Layout",
			MenuPages::Locale => "Locale",
			MenuPages::EnableFlakes => "Enable Flakes",
			MenuPages::Drives => "Drives",
			MenuPages::Bootloader => "Bootloader",
			MenuPages::Swap => "Swap",
			MenuPages::Hostname => "Hostname",
			MenuPages::RootPassword => "Root Password",
			MenuPages::UserAccounts => "User Accounts",
			MenuPages::Profile => "Profile",
			MenuPages::Greeter => "Greeter",
			MenuPages::DesktopEnvironment => "Desktop Environment",
			MenuPages::Audio => "Audio",
			MenuPages::Kernels => "Kernels",
			MenuPages::SystemPackages => "System Packages",
			MenuPages::Network => "Network",
			MenuPages::Timezone => "Timezone",
		};
		write!(f, "{s}")
	}
}

pub struct Menu {
	menu_items: StrList,
	button_row: WidgetBox
}

impl Menu {
	pub fn new() -> Self {
		let items = MenuPages::all_pages().iter().map(|p| p.to_string()).collect::<Vec<_>>();
		let mut menu_items = StrList::new("Main Menu", items);
		let buttons: Vec<Box<dyn ConfigWidget>> = vec![
			Box::new(Button::new("Done")),
			Box::new(Button::new("Abort")),
		];
		let button_row = WidgetBoxBuilder::new()
			.children(buttons)
			.build();
		menu_items.focus();
		Self { menu_items, button_row }
	}
	pub fn info_box_for_item(&self, installer: &mut Installer, idx: usize) -> WidgetBox {
		let mut display_widget: Option<Box<dyn ConfigWidget>> = None;
		let (title, content) = match idx {
			0 => {
				display_widget = SourceFlake::display_widget(installer);
				SourceFlake::page_info()
			},
			1 => {
				display_widget = Language::display_widget(installer);
				Language::page_info()
			},
			2 => {
				display_widget = KeyboardLayout::display_widget(installer);
				KeyboardLayout::page_info()
			},
			3 => {
				display_widget = Locale::display_widget(installer);
				Locale::page_info()
			},
			4 => {
				display_widget = EnableFlakes::display_widget(installer);
				EnableFlakes::page_info()
			},
			5 => {
				let sector_size = installer.drive_config.as_ref().map(|d| d.sector_size()).unwrap_or(512);
				display_widget = installer.drive_config_display.as_deref()
					.map(|d| Box::new(part_table(d, sector_size)) as Box<dyn ConfigWidget>);
				(
					"Drives".to_string(),
					styled_block(vec![
						vec![(None, "Select and configure the drives for your NixOS installation.")],
						vec![(None, "This includes partitioning, formatting, and mount points.")],
						vec![(None, "If you have already configured a drive, its current configuration will be shown below.")],
					])
				)
			}
			6 => {
				display_widget = Bootloader::display_widget(installer);
				Bootloader::page_info()
			},
			7 => {
				display_widget = Swap::display_widget(installer);
				Swap::page_info()
			},
			8 => {
				display_widget = Hostname::display_widget(installer);
				Hostname::page_info()
			},
			9 => {
				display_widget = RootPassword::display_widget(installer);
				RootPassword::page_info()
			},
			10 => {
				display_widget = UserAccounts::display_widget(installer);
				UserAccounts::page_info()
			},
			11 => {
				display_widget = Profile::display_widget(installer);
				Profile::page_info()
			},
			12 => {
				display_widget = Greeter::display_widget(installer);
				Greeter::page_info()
			},
			13 => {
				display_widget = DesktopEnvironment::display_widget(installer);
				DesktopEnvironment::page_info()
			},
			14 => {
				display_widget = Audio::display_widget(installer);
				Audio::page_info()
			},
			15 => {
				display_widget = Kernels::display_widget(installer);
				Kernels::page_info()
			},
			16 => {
				display_widget = SystemPackages::display_widget(installer);
				SystemPackages::page_info()
			},
			17 => {
				display_widget = Network::display_widget(installer);
				Network::page_info()
			},
			18 => {
				display_widget = Timezone::display_widget(installer);
				Timezone::page_info()
			},
			_ => (
				"Unknown Option".to_string(),
				styled_block(vec![
					vec![(None, "No information available for this option.")],
				])
			),
		};
		let info_box = Box::new(InfoBox::new(title, content));
		if let Some(widget) = display_widget {
			WidgetBoxBuilder::new()
				.layout(
					Layout::default()
					.direction(Direction::Vertical)
					.constraints(
						[
							Constraint::Percentage(50),
							Constraint::Percentage(50),
						]
						.as_ref(),
					)
				)
				.children(vec![info_box,widget])
				.build()
		} else {
			WidgetBoxBuilder::new()
				.children(vec![info_box])
				.build()
		}

	}
	pub fn remaining_requirements(&self, installer: &mut Installer) -> InfoBox {
		let mut lines = vec![];
		if installer.root_passwd_hash.is_none() {
			lines.push(vec![(Some((Color::Red, Modifier::BOLD)), " - Root Password")]);
		}
		if installer.drives.is_empty() || installer.selected_drive.is_none() || installer.drive_config.is_none() {
			lines.push(vec![(Some((Color::Red, Modifier::BOLD)), " - Drive Configuration")]);
		}
		if installer.users.is_empty() {
			lines.push(vec![(Some((Color::Red, Modifier::BOLD)), " - At least one User Account")]);
		}
		if lines.is_empty() {
			lines.push(vec![(Some((Color::Green, Modifier::BOLD)), "All required options have been configured!")]);
		} else {
			lines.insert(0, vec![(None, "The following required options are not yet configured:")]);
			lines.push(vec![(None, "Please configure them before proceeding.")]);
		}

		InfoBox::new("Required Config", styled_block(lines))
	}
}

impl Default for Menu {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Menu {
	fn render(&mut self, installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Horizontal)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(20),
				Constraint::Percentage(80),
				]
				.as_ref(),
			)
			.split(area);

		// We use this for both the menu options and info box
		// so that it looks visually consistent :)
		let split_space = |layout: Layout, chunk: Rect| {
			layout
				.direction(Direction::Vertical)
				.constraints(
					[
					Constraint::Percentage(95), // Main content
					Constraint::Percentage(5),  // Footer
					]
					.as_ref(),
				)
				.split(chunk)
		};

		let left_chunks = split_space(Layout::default(),chunks[0]);

		let right_chunks = split_space(Layout::default(),chunks[1]);

		self.menu_items.render(f, left_chunks[0]);
		self.button_row.render(f, left_chunks[1]);
		let info_box: Box<dyn ConfigWidget> = if self.menu_items.is_focused() {
			Box::new(self.info_box_for_item(installer, self.menu_items.selected_idx)) as Box<dyn ConfigWidget>
		} else {
			Box::new(self.remaining_requirements(installer)) as Box<dyn ConfigWidget>
		};
		info_box.render(f, right_chunks[0]);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('q') => Signal::Quit,
			KeyCode::Home | KeyCode::Char('g') => {
				if self.menu_items.is_focused() {
					self.menu_items.first_item();
					Signal::Wait
				} else {
					self.menu_items.first_item();
					self.menu_items.focus();
					self.button_row.unfocus();
					Signal::Wait
				}
			}
			KeyCode::End | KeyCode::Char('G') => {
				if self.menu_items.is_focused() {
					self.button_row.focus();
					self.menu_items.unfocus();
				}
				Signal::Wait
			}
			KeyCode::Up | KeyCode::Char('k') => {
				if self.menu_items.is_focused() {
					if !self.menu_items.previous_item() {
						self.menu_items.unfocus();
						self.button_row.focus();
					}
					Signal::Wait
				} else {
					self.menu_items.last_item();
					self.menu_items.focus();
					self.button_row.unfocus();
					Signal::Wait
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.menu_items.is_focused() {
					if !self.menu_items.next_item() {
						self.menu_items.unfocus();
						self.button_row.focus();
					}
					Signal::Wait
				} else {
					self.menu_items.first_item();
					self.menu_items.focus();
					self.button_row.unfocus();
					Signal::Wait
				}
			}
			KeyCode::Right | KeyCode::Char('l') => {
				if self.button_row.is_focused() {
					self.button_row.next_child();
				}
				Signal::Wait
			}
			KeyCode::Left | KeyCode::Char('h') => {
				if self.button_row.is_focused() {
					self.button_row.prev_child();
				}
				Signal::Wait
			}
			KeyCode::Enter => {
				if self.menu_items.is_focused() {
					let idx = self.menu_items.selected_idx;
					match idx {
						_ if idx == MenuPages::SourceFlake as usize => {
							Signal::Push(Box::new(SourceFlake::new()))
						}
						_ if idx == MenuPages::Language as usize => {
							Signal::Push(Box::new(Language::new()))
						}
						_ if idx == MenuPages::KeyboardLayout as usize => {
							Signal::Push(Box::new(KeyboardLayout::new()))
						}
						_ if idx == MenuPages::Locale as usize => {
							Signal::Push(Box::new(Locale::new()))
						}
						_ if idx == MenuPages::EnableFlakes as usize => {
							Signal::Push(Box::new(EnableFlakes::new(installer.enable_flakes)))
						}
						_ if idx == MenuPages::Drives as usize => {
							Signal::Push(Box::new(Drives::new()))
						}
						_ if idx == MenuPages::Bootloader as usize => {
							Signal::Push(Box::new(Bootloader::new()))
						}
						_ if idx == MenuPages::Swap as usize => {
							Signal::Push(Box::new(Swap::new(installer.use_swap)))
						}
						_ if idx == MenuPages::Hostname as usize => {
							Signal::Push(Box::new(Hostname::new()))
						}
						_ if idx == MenuPages::RootPassword as usize => {
							Signal::Push(Box::new(RootPassword::new()))
						}
						_ if idx == MenuPages::UserAccounts as usize => {
							Signal::Push(Box::new(UserAccounts::new(installer.users.clone())))
						}
						_ if idx == MenuPages::Profile as usize => {
							Signal::Push(Box::new(Profile::new()))
						}
						_ if idx == MenuPages::Greeter as usize => {
							Signal::Push(Box::new(Greeter::new()))
						}
						_ if idx == MenuPages::DesktopEnvironment as usize => {
							Signal::Push(Box::new(DesktopEnvironment::new()))
						}
						_ if idx == MenuPages::Audio as usize => {
							Signal::Push(Box::new(Audio::new()))
						}
						_ if idx == MenuPages::Kernels as usize => {
							Signal::Push(Box::new(Kernels::new()))
						}
						_ if idx == MenuPages::SystemPackages as usize => {
							// we actually need to go ask nixpkgs what packages it has now
							let pkgs = {
								let mut retries = 0;
								loop {
									let guard = NIXPKGS.read().unwrap();
									if let Some(nixpkgs) = guard.as_ref() {
										break nixpkgs.clone();
									}
									drop(guard); // Release lock before sleeping

									if retries >= 5 {
										break fetch_nixpkgs().unwrap_or_default();
									}

									std::thread::sleep(std::time::Duration::from_millis(500));
									retries += 1;
								}
							};
							Signal::Push(Box::new(SystemPackages::new(installer.system_pkgs.clone(),pkgs)))
						}
						_ if idx == MenuPages::Network as usize => {
							Signal::Push(Box::new(Network::new()))
						}
						_ if idx == MenuPages::Timezone as usize => {
							Signal::Push(Box::new(Timezone::new()))
						}
						_ => Signal::Wait,
					}
				} else if self.button_row.is_focused() {
					match self.button_row.selected_child() {
						Some(0) => Signal::WriteCfg, // Done
						Some(1) => Signal::Quit,     // Abort
						_ => Signal::Wait,
					}
				} else {
					self.menu_items.focus();
					Signal::Wait
				}
			}
			_ => Signal::Wait,
		}
	}
}
/*
			MenuPages::SourceFlake,
			MenuPages::Language,
			MenuPages::KeyboardLayout,
			MenuPages::Locale,
			MenuPages::EnableFlakes,
			MenuPages::Drives,
			MenuPages::Bootloader,
			MenuPages::Swap,
			MenuPages::Hostname,
			MenuPages::RootPassword,
			MenuPages::UserAccounts,
			MenuPages::Profile,
			MenuPages::Greeter,
			MenuPages::DesktopEnvironment,
			MenuPages::Audio,
			MenuPages::Kernels,
			MenuPages::Virtualization,
			MenuPages::SystemPackages,
			MenuPages::Network,
			MenuPages::Timezone,
*/

pub struct SourceFlake {
	pub input: LineEditor
}

impl SourceFlake {
	pub fn new() -> Self {
		let mut input = LineEditor::new("Source Config Flake", Some("e.g. '/path/to/flake#my-host' or 'github:user/repo#my-host'"));
		input.focus();
		Self { input  }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.flake_path.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current flake path set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Source Flake".to_string(),
			styled_block(vec![
				vec![(None,"Choose a flake output to use as a source for the system configuration.")],
				vec![(None,"This can be used in place of manual configuration using this installer. You will still need to set up a disk partitioning plan, however.")],
				vec![(None,"This can be "), (Some((Color::Reset, Modifier::ITALIC)), "any valid path"), (None," to a flake output that produces a "), (Some((Color::Cyan, Modifier::BOLD)), "'nixosConfiguration'"), (None," attribute.")],
				vec![(None,"Examples include:")],
				vec![(None," - A local flake: "), (HIGHLIGHT, "'/path/to/flake#my-host'")],
				vec![(None," - A GitHub flake: "), (HIGHLIGHT, "'github:user/repo#my-host'")],
			])
		)
	}
}

impl Default for SourceFlake {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for SourceFlake {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(40),
				Constraint::Length(5),
				Constraint::Percentage(40),
				]
				.as_ref(),
			)
			.split(area);
		let hor_chunks = Layout::default()
			.direction(Direction::Horizontal)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(10),
				Constraint::Percentage(80),
				Constraint::Percentage(10),
				]
				.as_ref(),
			)
			.split(chunks[1]);

		let info_box = InfoBox::new(
			"",
			styled_block(vec![
				vec![(None,"Choose a flake output to use as a source for the system configuration.")],
				vec![(None,"This can be used in place of manual configuration using this installer. You will still need to set up a disk partitioning plan, however.")],
				vec![(None,"This can be "), (Some((Color::Reset, Modifier::ITALIC)), "any valid path"), (None," to a flake output that produces a "), (Some((Color::Cyan, Modifier::BOLD)), "'nixosConfiguration'"), (None," attribute.")],
				vec![(None,"Examples include:")],
				vec![(None," - A local flake: "), (HIGHLIGHT, "'/path/to/flake#my-host'")],
				vec![(None," - A GitHub flake: "), (HIGHLIGHT, "'github:user/repo#my-host'")],
			])
		);

		info_box.render(f, chunks[0]);
		self.input.render(f, hor_chunks[1]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Enter => {
				let flake_path = self.input.get_value().unwrap().as_str().unwrap().trim().to_string();
				installer.flake_path = if flake_path.is_empty() { None } else { Some(flake_path) };
				Signal::PopCount(2)
			}
			_ => self.input.handle_input(event)
		}
	}
}

pub struct Language {
	langs: StrList
}

impl Language {
	pub fn new() -> Self {
		let languages = [
			"English",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut langs = StrList::new("Select Language", languages);
		langs.focus();
		Self { langs }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.language.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current language set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Language".to_string(),
			styled_block(vec![
				vec![(None, "Select the language to be used for your system.")],
			])
		)
	}
}

impl Default for Language {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Language {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.langs.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.language = Some(self.langs.items[self.langs.selected_idx].clone());
				Signal::Pop
			}
			_ => self.langs.handle_input(event)
		}
	}
}

pub struct KeyboardLayout {
	layouts: StrList
}

impl KeyboardLayout {
	pub fn new() -> Self {
		let layouts = vec![
			"us",
			"us(dvorak)",
			"us(colemak)",
			"uk",
			"de",
			"fr",
			"es",
			"it",
			"ru",
			"cn",
			"jp",
			"kr",
			"in",
			"br",
			"nl",
			"se",
			"no",
			"fi",
			"dk",
			"pl",
			"tr",
			"gr",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut layouts = StrList::new("Select Keyboard Layout", layouts);
		layouts.focus();
		Self { layouts }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.keyboard_layout.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current keyboard layout set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Keyboard Layout".to_string(),
			styled_block(vec![
				vec![(None, "Choose the keyboard layout that matches your physical keyboard.")],
			])
		)
	}
}

impl Default for KeyboardLayout {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for KeyboardLayout {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.layouts.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.keyboard_layout = Some(self.layouts.items[self.layouts.selected_idx].clone());
				Signal::Pop
			}
			_ => self.layouts.handle_input(event)
		}
	}
}

pub struct Locale {
	locales: StrList
}

impl Locale {
	pub fn new() -> Self {
		let locales = vec![
			"en_US.UTF-8",
			"en_GB.UTF-8",
			"de_DE.UTF-8",
			"fr_FR.UTF-8",
			"es_ES.UTF-8",
			"it_IT.UTF-8",
			"ru_RU.UTF-8",
			"zh_CN.UTF-8",
			"ja_JP.UTF-8",
			"ko_KR.UTF-8",
			"pt_BR.UTF-8",
			"nl_NL.UTF-8",
			"sv_SE.UTF-8",
			"no_NO.UTF-8",
			"fi_FI.UTF-8",
			"da_DK.UTF-8",
			"pl_PL.UTF-8",
			"tr_TR.UTF-8",
			"el_GR.UTF-8",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut locales = StrList::new("Select Locale", locales);
		locales.focus();
		Self { locales }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.locale.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current locale set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Locale".to_string(),
			styled_block(vec![
				vec![(None, "Set the locale for your system, which determines language and regional settings.")],
			])
		)
	}
}

impl Default for Locale {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Locale {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.locales.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.locale = Some(self.locales.items[self.locales.selected_idx].clone());
				Signal::Pop
			}
			_ => self.locales.handle_input(event)
		}
	}
}

pub struct EnableFlakes {
	buttons: WidgetBox
}

impl EnableFlakes {
	pub fn new(checked: bool) -> Self {
		let toggle = CheckBox::new("Enable Flakes Support", checked);
		let back_btn = Button::new("Back");
		let mut buttons = WidgetBox::button_menu(vec![Box::new(toggle),Box::new(back_btn)]);
		buttons.focus();
		Self { buttons }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		let status = if installer.enable_flakes { "enabled" } else { "disabled" };
		let ib = InfoBox::new("", styled_block(vec![
			vec![(None, "Flakes support is currently:")],
			vec![(HIGHLIGHT, status)],
		]));
		Some(Box::new(ib) as Box<dyn ConfigWidget>)
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Enable Flakes".to_string(),
			styled_block(vec![
				vec![(None,"Nix flakes are an experimental feature of the Nix package manager that provide a new way to manage and distribute Nix packages and configurations.")],
				vec![(None,"Enabling flakes support allows you to use flake-based configurations and take advantage of features like reproducible builds and easier dependency management.")],
				vec![(None,"Note that flakes are still considered experimental and may not be suitable for all users or use cases.")],
			])
		)
	}
}

impl Default for EnableFlakes {
	fn default() -> Self {
		Self::new(false)
	}
}

impl Page for EnableFlakes {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(40),
				Constraint::Percentage(60),
				]
				.as_ref(),
			)
			.split(area);
		let hor_chunks = Layout::default()
			.direction(Direction::Horizontal)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(30),
				Constraint::Percentage(40),
				Constraint::Percentage(30),
				]
				.as_ref(),
			)
			.split(chunks[1]);
		let info_box = InfoBox::new(
			"",
			styled_block(vec![
				vec![(None,"Nix flakes are an experimental feature of the Nix package manager that provide a new way to manage and distribute Nix packages and configurations.")],
				vec![(None,"Enabling flakes support allows you to use flake-based configurations and take advantage of features like reproducible builds and easier dependency management.")],
				vec![(None,"Note that flakes are still considered experimental and may not be suitable for all users or use cases.")],
			])
		);
		info_box.render(f, chunks[0]);
		self.buttons.render(f, hor_chunks[1]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Up | KeyCode::Char('k') => {
				self.buttons.prev_child();
				Signal::Wait
			}
			KeyCode::Down | KeyCode::Char('j') => {
				self.buttons.next_child();
				Signal::Wait
			}
			KeyCode::Enter => {
				match self.buttons.selected_child() {
					Some(0) => {
						let Some(chkbox) = self.buttons.focused_child_mut() else { return Signal::Wait; };
						chkbox.interact();
						let Some(Value::Bool(checked)) = chkbox.get_value() else { return Signal::Wait; };
						installer.enable_flakes = checked;
						Signal::Wait
					}
					Some(1) => Signal::Pop, // Back
					_ => Signal::Wait,
				}
			}
			_ => Signal::Wait
		}
	}
}

pub struct Bootloader {
	loaders: StrList
}

impl Bootloader {
	pub fn new() -> Self {
		let loaders = [
			"GRUB",
			"systemd-boot",
			"rEFInd",
			"None (manual setup)",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut loaders = StrList::new("Select Bootloader", loaders);
		loaders.focus();
		Self { loaders }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.bootloader.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current bootloader set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Bootloader".to_string(),
			styled_block(vec![
				vec![(None, "Select the bootloader to be installed on your system.")],
				vec![(None, "The bootloader is responsible for loading the operating system when the computer starts.")],
			])
		)
	}
}

impl Default for Bootloader {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Bootloader {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.loaders.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.bootloader = Some(self.loaders.items[self.loaders.selected_idx].clone());
				Signal::Pop
			}
			_ => self.loaders.handle_input(event)
		}
	}
}

pub struct Swap {
	buttons: WidgetBox
}

impl Swap {
	pub fn new(checked: bool) -> Self {
		let toggle = CheckBox::new("Enable Swap", checked);
		let back_btn = Button::new("Back");
		let mut buttons = WidgetBox::button_menu(vec![Box::new(toggle),Box::new(back_btn)]);
		buttons.focus();
		Self { buttons }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		let status = if installer.use_swap { "enabled" } else { "disabled" };
		let ib = InfoBox::new("", styled_block(vec![
			vec![(None, "Swap is currently:")],
			vec![(HIGHLIGHT, status)],
		]));
		Some(Box::new(ib) as Box<dyn ConfigWidget>)
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Swap".to_string(),
			styled_block(vec![
				vec![(None,"Swap space is a portion of the hard drive that is used as virtual memory when the system's RAM is full.")],
				vec![(None,"Enabling swap can help improve system performance and stability, especially on systems with limited RAM.")],
				vec![(None,"However, using swap can also lead to slower performance compared to using RAM, as accessing data from the hard drive is slower than accessing data from RAM.")],
				vec![(None,"It's generally recommended to enable swap on systems with less than 8GB of RAM, but the optimal swap size and configuration can vary depending on your specific use case and workload.")],
			])
		)
	}
}

impl Default for Swap {
	fn default() -> Self {
		Self::new(false)
	}
}

impl Page for Swap {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(40),
				Constraint::Percentage(60),
				]
				.as_ref(),
			)
			.split(area);
		let hor_chunks = Layout::default()
			.direction(Direction::Horizontal)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(30),
				Constraint::Percentage(40),
				Constraint::Percentage(30),
				]
				.as_ref(),
			)
			.split(chunks[1]);
		let info_box = InfoBox::new(
			"",
			styled_block(vec![
				vec![(None,"Swap space is a portion of the hard drive that is used as virtual memory when the system's RAM is full.")],
				vec![(None,"Enabling swap can help improve system performance and stability, especially on systems with limited RAM.")],
				vec![(None,"However, using swap can also lead to slower performance compared to using RAM, as accessing data from the hard drive is slower than accessing data from RAM.")],
				vec![(None,"It's generally recommended to enable swap on systems with less than 8GB of RAM, but the optimal swap size and configuration can vary depending on your specific use case and workload.")],
			])
		);
		info_box.render(f, chunks[0]);
		self.buttons.render(f, hor_chunks[1]);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Up | KeyCode::Char('k') => {
				self.buttons.prev_child();
				Signal::Wait
			}
			KeyCode::Down | KeyCode::Char('j') => {
				self.buttons.next_child();
				Signal::Wait
			}
			KeyCode::Enter => {
				match self.buttons.selected_child() {
					Some(0) => {
						let Some(chkbox) = self.buttons.focused_child_mut() else { return Signal::Wait; };
						chkbox.interact();
						let Some(Value::Bool(checked)) = chkbox.get_value() else { return Signal::Wait; };
						installer.use_swap = checked;
						Signal::Wait
					}
					Some(1) => Signal::Pop, // Back
					_ => Signal::Wait,
				}
			}
			_ => Signal::Wait
		}
	}
}

pub struct Hostname {
	input: LineEditor
}

impl Hostname {
	pub fn new() -> Self {
		let mut input = LineEditor::new("Set Hostname", Some("e.g. 'my-computer'"));
		input.focus();
		Self { input  }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.hostname.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current hostname set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Hostname".to_string(),
			styled_block(vec![
				vec![(None,"The hostname is a unique identifier for your computer on a network.")],
				vec![(None,"It is used to distinguish your computer from other devices and can be helpful for network management and troubleshooting.")],
				vec![(None,"Choose a hostname that is easy to remember and reflects the purpose or identity of your computer.")],
			])
		)
	}
}

impl Default for Hostname {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Hostname {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(40),
				Constraint::Length(5),
				Constraint::Percentage(40),
				]
				.as_ref(),
			)
			.split(area);
		let hor_chunks = Layout::default()
			.direction(Direction::Horizontal)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(10),
				Constraint::Percentage(80),
				Constraint::Percentage(10),
				]
				.as_ref(),
			)
			.split(chunks[1]);

		let info_box = InfoBox::new(
			"",
			styled_block(vec![
				vec![(None,"The hostname is a unique identifier for your computer on a network.")],
				vec![(None,"It is used to distinguish your computer from other devices and can be helpful for network management and troubleshooting.")],
				vec![(None,"Choose a hostname that is easy to remember and reflects the purpose or identity of your computer.")],
			])
		);

		info_box.render(f, chunks[0]);
		self.input.render(f, hor_chunks[1]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Enter => {
				let hostname = self.input.get_value().unwrap().as_str().unwrap().trim().to_string();
				if !hostname.is_empty() {
					installer.hostname = Some(hostname);
				}
				Signal::Pop
			}
			_ => self.input.handle_input(event)
		}
	}
}

pub struct RootPassword {
	input: LineEditor,
	confirm: LineEditor,
}

impl RootPassword {
	pub fn new() -> Self {
		let mut input = LineEditor::new("Set Root Password", Some("Password will be hidden")).secret(true);
		let confirm = LineEditor::new("Confirm Password", Some("Password will be hidden")).secret(true);
		input.focus();
		Self { input, confirm  }
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Root Password".to_string(),
			styled_block(vec![
				vec![(None,"The root user is the superuser account on a Unix-like operating system, including Linux.")],
				vec![(None,"It has full administrative privileges and can perform any action on the system, including installing software, modifying system settings, and accessing all files and directories.")],
				vec![(None,"Setting a strong password for the root user is important for system security, as it helps prevent unauthorized access to sensitive system functions and data.")],
				vec![(None,"Choose a password that is difficult to guess and contains a mix of uppercase and lowercase letters, numbers, and special characters.")],
			])
		)
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.root_passwd_hash.as_ref().map(|_| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(HIGHLIGHT, "Root password is set.")]
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn mkpasswd(passwd: String) -> anyhow::Result<String> {
		let mut child = Command::new("mkpasswd")
			.arg("--method=SHA-512")
			.arg("--rounds=4096")
			.arg("--stdin")
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()?;
		{
			let stdin = child.stdin.as_mut().ok_or_else(|| anyhow::anyhow!("Failed to open stdin"))?;
			stdin.write_all(passwd.as_bytes())?;
		}
		let output = child.wait_with_output()?;
		if output.status.success() {
			let hashed = String::from_utf8_lossy(&output.stdout).trim().to_string();
			Ok(hashed)
		} else {
			Err(anyhow::anyhow!("mkpasswd failed: {}", String::from_utf8_lossy(&output.stderr)))
		}
	}
}

impl Default for RootPassword {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for RootPassword {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(40),
				Constraint::Length(10),
				Constraint::Percentage(40),
				]
				.as_ref(),
			)
			.split(area);
		let hor_chunks = Layout::default()
			.direction(Direction::Horizontal)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(10),
				Constraint::Percentage(80),
				Constraint::Percentage(10),
				]
				.as_ref(),
			)
			.split(chunks[1]);
		let vert_chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(0)
			.constraints(
				[
				Constraint::Length(5),
				Constraint::Length(5),
				]
				.as_ref(),
			)
			.split(hor_chunks[1]);

		let info_box = InfoBox::new(
			"",
			styled_block(vec![
				vec![(None,"The root user is the superuser account on a Unix-like operating system, including Linux.")],
				vec![(None,"It has full administrative privileges and can perform any action on the system, including installing software, modifying system settings, and accessing all files and directories.")],
				vec![(None,"Setting a strong password for the root user is important for system security, as it helps prevent unauthorized access to sensitive system functions and data.")],
				vec![(None,"Choose a password that is difficult to guess and contains a mix of uppercase and lowercase letters, numbers, and special characters.")],
			])
		);

		info_box.render(f, chunks[0]);
		self.input.render(f, vert_chunks[0]);
		self.confirm.render(f, vert_chunks[1]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Tab => {
				if self.input.is_focused() {
					self.input.unfocus();
					self.confirm.focus();
				} else {
					self.confirm.unfocus();
					self.input.focus();
				}
				Signal::Wait
			}
			KeyCode::Enter => {
				if self.input.is_focused() {
					self.input.unfocus();
					self.confirm.focus();
					Signal::Wait
				} else {
					let passwd = self.input.get_value().unwrap().as_str().unwrap().trim().to_string();
					let confirm = self.confirm.get_value().unwrap().as_str().unwrap().trim().to_string();
					if passwd.is_empty() {
						Signal::Wait // Ignore empty passwords
					} else if passwd != confirm {
						self.input.clear();
						self.confirm.clear();
						self.confirm.unfocus();
						self.input.focus();
						self.input.error("Passwords do not match");
						Signal::Wait // Passwords do not match
					} else {
						match Self::mkpasswd(passwd) {
							Ok(hashed) => {
								installer.root_passwd_hash = Some(hashed);
								Signal::Pop
							}
							Err(e) => {
								self.input.clear();
								self.confirm.clear();
								self.confirm.unfocus();
								self.input.focus();
								self.input.error(format!("Error hashing password: {e}"));
								Signal::Wait
							}
						}
					}
				}
			}
			_ => {
				if self.input.is_focused() {
					self.input.handle_input(event)
				} else {
					self.confirm.handle_input(event)
				}
			}
		}
	}
}

pub struct Profile {
	profiles: StrList
}

impl Profile {
	pub fn new() -> Self {
		let profiles = [
			"Minimal",
			"Desktop",
			"Server",
			"Custom",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut profiles = StrList::new("Select Profile", profiles);
		profiles.focus();
		Self { profiles }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.profile.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current profile set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Profile".to_string(),
			styled_block(vec![
				vec![(None,"Select a predefined profile that best matches your intended use case for the system.")],
				vec![(None,"Profiles are collections of settings and packages that are tailored for specific use cases, such as desktop or server environments.")],
				vec![(None,"Choosing a profile can help simplify the installation process and ensure that your system is configured appropriately for your needs.")],
				vec![(None,"You can always customize the configuration further after the installation is complete.")],
			])
		)
	}
}

impl Default for Profile {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Profile {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.profiles.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.profile = Some(self.profiles.items[self.profiles.selected_idx].clone());
				Signal::Pop
			}
			_ => self.profiles.handle_input(event)
		}
	}
}

pub struct Greeter {
	greeters: StrList
}

impl Greeter {
	pub fn new() -> Self {
		let greeters = [
			"LightDM",
			"GDM",
			"SDDM",
			"None (auto-login)",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut greeters = StrList::new("Select Greeter", greeters);
		greeters.focus();
		Self { greeters }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.greeter.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current greeter set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Greeter".to_string(),
			styled_block(vec![
				vec![(None, "Select the display manager (greeter) to be installed on your system.")],
				vec![(None, "The display manager is responsible for providing the graphical login screen and managing user sessions.")],
			])
		)
	}
}

impl Default for Greeter {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Greeter {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.greeters.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.greeter = Some(self.greeters.items[self.greeters.selected_idx].clone());
				Signal::Pop
			}
			_ => self.greeters.handle_input(event)
		}
	}
}

pub struct DesktopEnvironment {
	desktops: StrList
}

impl DesktopEnvironment {
	pub fn new() -> Self {
		let desktops = [
			"GNOME",
			"KDE Plasma",
			"Hyprland",
			"XFCE",
			"Cinnamon",
			"MATE",
			"Budgie",
			"i3",
			"None (command line only)",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut desktops = StrList::new("Select Desktop Environment", desktops);
		desktops.focus();
		Self { desktops }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.desktop_environment.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current desktop environment set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Desktop Environment".to_string(),
			styled_block(vec![
				vec![(None, "Select the desktop environment to be installed on your system.")],
				vec![(None, "The desktop environment provides the graphical user interface (GUI) for your system, including the window manager, panels, and application launchers.")],
				vec![(None, "Choosing a desktop environment can help tailor the user experience to your preferences and workflow.")],
			])
		)
	}
}

impl Default for DesktopEnvironment {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for DesktopEnvironment {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.desktops.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.desktop_environment = Some(self.desktops.items[self.desktops.selected_idx].clone());
				Signal::Pop
			}
			_ => self.desktops.handle_input(event)
		}
	}
}

pub struct Kernels {
	kernels: StrList
}

impl Kernels {
	pub fn new() -> Self {
		let kernels = [
			"linux",
			"linux-lts",
			"linux-zen",
			"linux-hardened",
			"None (custom kernel)",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut kernels = StrList::new("Select Kernel", kernels);
		kernels.focus();
		Self { kernels }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.kernels.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Currently selected kernels:".to_string())],
				s.clone().into_iter().map(|k| (HIGHLIGHT, k)).collect::<Vec<_>>(),
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Kernel".to_string(),
			styled_block(vec![
				vec![(None, "Select the Linux kernel to be installed on your system.")],
				vec![(None, "The kernel is the core component of the operating system that manages hardware resources and provides essential services for other software.")],
				vec![(None, "Choosing a kernel can help optimize system performance and compatibility with your hardware.")],
			])
		)
	}
}

impl Default for Kernels {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Kernels {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.kernels.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				// TODO: Implement multi selection for StrList
				installer.kernels = Some(vec![self.kernels.items[self.kernels.selected_idx].clone()]);
				Signal::Pop
			}
			_ => self.kernels.handle_input(event)
		}
	}
}

pub struct Audio {
	backends: StrList
}

impl Audio {
	pub fn new() -> Self {
		let backends = [
			"PipeWire",
			"PulseAudio",
			"ALSA (no sound server)",
			"None (no audio support)",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut backends = StrList::new("Select Audio Backend", backends);
		backends.focus();
		Self { backends }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.audio_backend.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current audio backend set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Audio".to_string(),
			styled_block(vec![
				vec![(None, "Select the audio management backend to be installed on your system.")],
				vec![(None, "The audio backend is responsible for managing sound devices and providing audio services to applications.")],
				vec![(None, "Choosing an audio backend can help ensure that your system is able to handle audio playback and recording effectively.")],
			])
		)
	}
}

impl Default for Audio {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Audio {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.backends.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.audio_backend = Some(self.backends.items[self.backends.selected_idx].clone());
				Signal::Pop
			}
			_ => self.backends.handle_input(event)
		}
	}
}

pub struct Network {
	backends: StrList
}

impl Network {
	pub fn new() -> Self {
		let backends = [
			"NetworkManager",
			"wpa_supplicant",
			"systemd-networkd",
			"None (manual setup)",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut backends = StrList::new("Select Network Backend", backends);
		backends.focus();
		Self { backends }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.network_backend.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current network backend set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Network".to_string(),
			styled_block(vec![
				vec![(None, "Select the network management backend to be installed on your system.")],
				vec![(None, "The network backend is responsible for managing network connections and settings on your system.")],
				vec![(None, "Choosing a network backend can help ensure that your system is able to connect to and manage network interfaces effectively.")],
			])
		)
	}
}

impl Default for Network {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Network {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.backends.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.network_backend = Some(self.backends.items[self.backends.selected_idx].clone());
				Signal::Pop
			}
			_ => self.backends.handle_input(event)
		}
	}
}

pub struct Timezone {
	timezones: StrList
}

impl Timezone {
	pub fn new() -> Self {
		let timezones = vec![
			"UTC",
			"America/New_York",
			"America/Los_Angeles",
			"America/Chicago",
			"America/Denver",
			"Europe/London",
			"Europe/Berlin",
			"Europe/Paris",
			"Europe/Moscow",
			"Asia/Tokyo",
			"Asia/Shanghai",
			"Asia/Kolkata",
			"Asia/Dubai",
			"Australia/Sydney",
			"Australia/Melbourne",
		]
		.iter()
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
		let mut timezones = StrList::new("Select Timezone", timezones);
		timezones.focus();
		Self { timezones }
	}
	pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
		installer.timezone.clone().map(|s| {
			let ib = InfoBox::new("", styled_block(vec![
				vec![(None, "Current timezone set to:")],
				vec![(HIGHLIGHT, &s)],
			]));
			Box::new(ib) as Box<dyn ConfigWidget>
		})
	}
	pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
		(
			"Timezone".to_string(),
			styled_block(vec![
				vec![(None, "Select the timezone for your system.")],
				vec![(None, "The timezone setting determines the local time displayed on your system and is important for scheduling tasks and logging events.")],
				vec![(None, "Choose a timezone that matches your physical location or the location where the system will primarily be used.")],
			])
		)
	}
}

impl Default for Timezone {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Timezone {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(1)
			.constraints(
				[
				Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		self.timezones.render(f, chunks[0]);
	}

	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Char('q') => Signal::Pop,
			KeyCode::Enter => {
				installer.timezone = Some(self.timezones.items[self.timezones.selected_idx].clone());
				Signal::Pop
			}
			_ => self.timezones.handle_input(event)
		}
	}
}
