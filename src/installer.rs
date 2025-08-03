use std::{fmt::{Debug, Display}, process::Command};

use indoc::indoc;
use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Direction, Layout, Rect}, Frame};
use serde_json::Value;

use std::str::FromStr;
use std::num::ParseFloatError;

use crate::{drives::{DiskEntry, DiskPlan, DiskPlanIRBuilder, DiskSize, DiskTable, DiskTableHeader, PartStatus}, widget::{Button, ConfigWidget, InfoBox, LineEditor, StrList, TableRow, TableWidget, WidgetBox, WidgetBoxBuilder}};



#[derive(Default)]
pub struct Installer {
	pub selected_drive_info: Option<DiskEntry>,
	pub drive_config_builder: DiskPlanIRBuilder,
	pub use_auto_drive_config: bool,
	pub drive_config: Option<DiskPlan>,


	pub drive_config_display: Option<TableWidget>
}

impl Installer {
	pub fn new() -> Self {
		Self::default()
	}
	pub fn make_drive_config_display(&mut self) {
		let Some(drive_config) = &self.drive_config else {
			self.drive_config_display = None;
			return;
		};
		let table = drive_config.as_table().ok();
		self.drive_config_display = table
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
		}
	}
}

pub trait Page {
	fn render(&mut self, installer: &Installer, f: &mut Frame, area: Rect);
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal;
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum MenuPages {
	SourceFlake = 0,
	Language = 1,
	KeyboardLayout = 2,
	Locale = 3,
	UseFlakes = 4,
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
	Virtualization = 16,
	SystemPackages = 17,
	Network = 18,
	Timezone = 19,
}

impl MenuPages {
	pub fn all_pages() -> &'static [MenuPages] {
		&[
			MenuPages::SourceFlake,
			MenuPages::Language,
			MenuPages::KeyboardLayout,
			MenuPages::Locale,
			MenuPages::UseFlakes,
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
			MenuPages::UseFlakes => "Use Flakes",
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
			MenuPages::Virtualization => "Virtualization",
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
	pub fn info_box_for_item(&self, installer: &Installer, idx: usize) -> WidgetBox {
		let mut display_widget: Option<Box<dyn ConfigWidget>> = None;
		let (title, content) = match idx {
			0 => (
				"Source Flake",
				indoc::indoc! {"Choose a flake output to use as your system configuration.
					This can be any valid path to a flake output that produces a 'nixosConfiguration'.

					Examples include:
					github:foobar/nixos#my-host
					path:./my-flake#my-host
				"}
			),
			1 => (
				"Language",
				"Select the system language for your installation."
			),
			2 => (
				"Keyboard Layout",
				"Choose the keyboard layout that matches your hardware."
			),
			3 => (
				"Locale",
				"Set the locale settings for your system."
			),
			4 => (
				"Use Flakes",
				indoc! {"Decide whether to use Nix Flakes for package management.
					Will write 'nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ];' to your generated configuration.
				"}
			),
			5 => {
				display_widget = installer.drive_config_display.as_ref().map(|w| Box::new(w.clone()) as Box<dyn ConfigWidget>);
				(
					"Drives",
					"Select and configure the drives to be used for installation."
				)
			}
			6 => (
				"Bootloader",
				"Choose and configure the bootloader for your system."
			),
			7 => (
				"Swap",
				"Set up swap space for your installation."
			),
			8 => (
				"Hostname",
				"Define the hostname for your system."
			),
			9 => (
				"Root Password",
				"Set the root password for administrative access."
			),
			10 => (
				"User Accounts",
				"Create and manage user accounts on your system."
			),
			11 => (
				"Profile",
				"Select a NixOS profile to customize your installation."
			),
			12 => (
				"Greeter",
				"Choose a greeter for graphical login."
			),
			13 => (
				"Desktop Environment",
				"Select a desktop environment for your system."
			),
			14 => (
				"Audio",
				"Configure audio settings and devices."
			),
			15 => (
				"Kernels",
				"Choose which kernels to install."
			),
			16 => (
				"Virtualization",
				"Set up virtualization options if needed."
			),
			17 => (
				"System Packages",
				"Select additional system packages to install."
			),
			18 => (
				"Network",
				"Configure network settings and interfaces."
			),
			19 => (
				"Timezone",
				"Set the timezone for your system."
			),
			_ => (
				"Unknown Option",
				"No information available for this option."
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
							Constraint::Percentage(70),
							Constraint::Percentage(30),
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
}

impl Default for Menu {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Menu {
	fn render(&mut self, installer: &Installer, f: &mut Frame, area: Rect) {
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
		let info_box = self.info_box_for_item(installer, self.menu_items.selected_idx);
		info_box.render(f, right_chunks[0]);
	}
	fn handle_input(&mut self, _installer: &mut Installer, event: KeyEvent) -> Signal {
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
				let idx = self.menu_items.selected_idx;
				match idx {
					_ if idx == MenuPages::Drives as usize => {
						Signal::Push(Box::new(Drives::new()))
					}
					_ => Signal::Wait,
				}
			}
			_ => Signal::Wait,
		}
	}
}

pub struct Drives {
	pub buttons: WidgetBox,
	pub info_box: InfoBox
}

impl Drives {
	pub fn new() -> Self {
		let buttons = vec![
			Button::new("Use a best-effort default partition layout"),
			Button::new("Configure partitions manually"),
			Button::new("Back"),
		];
		let mut button_row = WidgetBox::button_menu(buttons);
		button_row.focus();
		let info_box = InfoBox::new(
			"Drive Configuration",
			indoc! {"Choose how you would like to configure your drives for the NixOS installation.

				- 'Use a best-effort default partition layout' will attempt to automatically partition and format your selected drive(s) with sensible defaults.
				  This is recommended for most users.

				- 'Configure partitions manually' will allow you to specify exactly how your drives should be partitioned and formatted.
				  This is recommended for advanced users who have specific requirements.

				NOTE: When the installer is run, any and all data on the selected drive will be wiped. Make sure you've backed up any important data.
			"},
		);

		Self { buttons: button_row, info_box }
	}
}

impl Default for Drives {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Drives {
	fn render(&mut self, _installer: &Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(2)
			.constraints(
				[
				Constraint::Percentage(70),
				Constraint::Percentage(30),
				]
				.as_ref(),
			)
			.split(area);

		self.info_box.render(f, chunks[0]);
		self.buttons.render(f, chunks[1]);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('q') | KeyCode::Esc => Signal::Pop,
			KeyCode::Up | KeyCode::Char('k') => {
				self.buttons.prev_child();
				Signal::Wait
			}
			KeyCode::Down | KeyCode::Char('j') => {
				self.buttons.next_child();
				Signal::Wait
			}
			KeyCode::Enter => {
				let Some(idx) = self.buttons.selected_child() else {
					return Signal::Wait;
				};
				match idx {
					0 => {
						installer.use_auto_drive_config = true;
						Signal::Push(Box::new(SelectDrive::new()))
					}
					1 => {
						installer.use_auto_drive_config = false;
						Signal::Push(Box::new(SelectDrive::new()))
					}
					2 => Signal::Pop,
					_ => Signal::Wait,
				}
			}
			_ => Signal::Wait,
		}
	}
}

pub struct SelectDrive {
	drives: TableWidget
}

impl SelectDrive {
	pub fn new() -> Self {
		let mut rows = DiskTable::from_lsblk().unwrap();
		rows = rows.filter_by(|row| row.parent == None);
		let mut drives = rows.as_widget(Some(DiskTableHeader::all_headers()));
		drives.focus();
		Self { drives }
	}
}

impl Default for SelectDrive {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for SelectDrive {
	fn render(&mut self, _installer: &Installer, f: &mut Frame, area: Rect) {
		self.drives.render(f, area);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('q') | KeyCode::Esc => Signal::Pop,
			KeyCode::Up | KeyCode::Char('k') => {
				self.drives.previous_row();
				Signal::Wait
			}
			KeyCode::Down | KeyCode::Char('j') => {
				self.drives.next_row();
				Signal::Wait
			}
			KeyCode::Enter => {
				if let Some(row) = self.drives.selected_row() {
					let Some(row) = self.drives.get_row(row) else {
						return Signal::Wait;
					};
					let device = row.fields[1].clone();
					let drive_info = DiskTable::from_lsblk().unwrap();
					let Some(drive_info) = drive_info.clone().find_by(|d| d.device == device) else {
						log::error!("Failed to find drive info for device '{device}'");
						return Signal::Wait;
					};
					installer.selected_drive_info = Some(drive_info.clone());
					installer.drive_config_builder.set_device(drive_info.clone());
					let children = DiskTable::from_lsblk().unwrap()
						.filter_by(|d| d.parent.as_deref() == Some(&drive_info.device));
					installer.drive_config_builder.set_layout(children.entries().to_vec());
					Signal::Push(Box::new(SelectFilesystem::new()))
				} else {
					Signal::Wait
				}
			}
			_ => Signal::Wait,
		}
	}
}

pub struct SelectFilesystem {
	pub buttons: WidgetBox,
}

impl SelectFilesystem {
	pub fn new() -> Self {
		let buttons = vec![
			Button::new("ext4"),
			Button::new("btrfs"),
			Button::new("xfs"),
			Button::new("Back"),
		];
		let mut button_row = WidgetBox::button_menu(buttons);
		button_row.focus();
		Self { buttons: button_row }
	}
	pub fn get_fs_info(idx: usize) -> InfoBox {
		match idx {
			0 => InfoBox::new(
				"ext4",
				indoc! {"ext4 is a widely used and stable filesystem known for its reliability and performance.
					It supports journaling, which helps protect against data corruption in case of crashes.
					It's a good choice for general-purpose use and is well-supported across various Linux distributions.
				"},
			),
			1 => InfoBox::new(
				"btrfs",
				indoc! {"btrfs (B-tree filesystem) is a modern filesystem that offers advanced features like snapshots, subvolumes, and built-in RAID support.
					It is designed for scalability and flexibility, making it suitable for systems that require complex storage solutions.
					However, it may not be as mature as ext4 in terms of stability for all use cases.
				"},
			),
			2 => InfoBox::new(
				"xfs",
				indoc! {"XFS is a high-performance journaling filesystem that excels in handling large files and high I/O workloads.
					It is particularly well-suited for servers and systems that require efficient data management for large datasets.
					XFS provides robust scalability and is known for its speed, but it may not have as many features as btrfs.
				"},
			),
			_ => InfoBox::new(
				"Unknown Filesystem",
				"No information available for this filesystem type.",
			),
		}
	}
}

impl Default for SelectFilesystem {
	fn default() -> Self {
		Self::new()
	}
}
impl Page for SelectFilesystem {
	fn render(&mut self, _installer: &Installer, f: &mut Frame, area: Rect) {
		let vert_chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints(
				[
					Constraint::Percentage(50),
					Constraint::Percentage(50),
				]
				.as_ref(),
			)
			.split(area);
		let hor_chunks = Layout::default()
			.direction(Direction::Horizontal)
			.margin(2)
			.constraints(
				[
					Constraint::Percentage(40),
					Constraint::Percentage(20),
					Constraint::Percentage(40),
				]
				.as_ref(),
			)
			.split(vert_chunks[0]);


		let idx = self.buttons.selected_child().unwrap_or(3);
		let info_box = Self::get_fs_info(self.buttons.selected_child().unwrap_or(3));
		self.buttons.render(f, hor_chunks[1]);
		if idx < 3 {
			info_box.render(f, vert_chunks[1]);
		}
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('q') | KeyCode::Esc => Signal::Pop,
			KeyCode::Up | KeyCode::Char('k') => {
				self.buttons.prev_child();
				Signal::Wait
			}
			KeyCode::Down | KeyCode::Char('j') => {
				self.buttons.next_child();
				Signal::Wait
			}
			KeyCode::Enter => {
				let Some(idx) = self.buttons.selected_child() else {
					return Signal::Wait;
				};
				let fs =  match idx {
					0 => "ext4",
					1 => "btrfs",
					2 => "xfs",
					3 => return Signal::Pop,
					_ => return Signal::Wait,
				}.to_string();

				if installer.use_auto_drive_config {
					installer.drive_config_builder.set_part_table("gpt");
					installer.drive_config_builder.set_fs(&fs);
					let Ok(config_ir) = installer.drive_config_builder.clone().build_auto() else {
						log::error!("Failed to build auto drive config");
						return Signal::Pop;
					};
					installer.drive_config = Some(config_ir.into());
					installer.make_drive_config_display();
				} else {
					return Signal::Push(Box::new(ManualPartition::new()));
				}


				Signal::Unwind
			}
			_ => Signal::Wait,
		}
	}
}

pub struct ManualPartition {
	disk_config: TableWidget,
	buttons: WidgetBox
}

impl ManualPartition {
	pub fn new() -> Self {
		let mut disk_config = DiskTable::empty().as_widget(Some(DiskTableHeader::all_headers()));
		let buttons = vec![
			Button::new("Suggest Partition Layout"),
			Button::new("Confirm and Exit"),
			Button::new("Abort"),
		];
		let buttons = WidgetBox::button_menu(buttons);
		disk_config.focus();
		Self { disk_config, buttons }
	}
}

impl Default for ManualPartition {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for ManualPartition {
	fn render(&mut self, installer: &Installer, f: &mut Frame, area: Rect) {
		let rows = installer.drive_config_builder.manual_config_table().unwrap().rows();
		self.disk_config.set_rows(rows);
		let len = self.disk_config.len();
		let table_constraint = 20 + (5u16 * len as u16);
		let padding = 70u16.saturating_sub(table_constraint);
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(2)
			.constraints(
				[
					Constraint::Percentage(table_constraint),
					Constraint::Percentage(30),
					Constraint::Percentage(padding),
				]
				.as_ref(),
			)
			.split(area);
		let hor_chunks = Layout::default()
			.direction(Direction::Horizontal)
			.margin(2)
			.constraints(
				[
					Constraint::Percentage(33),
					Constraint::Percentage(33),
					Constraint::Percentage(33),
				]
				.as_ref(),
			)
			.split(chunks[1]);

		self.disk_config.render(f, chunks[0]);
		self.buttons.render(f, hor_chunks[1]);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		if self.disk_config.is_focused() {
			match event.code {
				KeyCode::Char('q') | KeyCode::Esc => Signal::Pop,
				KeyCode::Up | KeyCode::Char('k') => {
					if !self.disk_config.previous_row() {
						self.disk_config.unfocus();
						self.buttons.last_child();
						self.buttons.focus();
					}
					Signal::Wait
				}
				KeyCode::Down | KeyCode::Char('j') => {
					if !self.disk_config.next_row() {
						self.disk_config.unfocus();
						self.buttons.first_child();
						self.buttons.focus();
					}
					Signal::Wait
				}
				KeyCode::Enter => {
					log::debug!("Disk config is focused, handling row selection");
					let Some(row) = self.disk_config.get_selected_row_info() else {
						return Signal::Wait;
					};
					let dev_name = row.get_field("device");
					let status = row.get_field("status");
					if let Some(dev_name) = dev_name {
						if let Some(status) = status {
							if status == "existing" {
								Signal::Push(Box::new(AlterPartition::new(dev_name.clone(), status.clone())))
							} else {
								Signal::Wait
							}
						} else {
							Signal::Wait
						}
					} else {
						Signal::Wait
					}
				}
				_ => Signal::Wait,
			}
		} else if self.buttons.is_focused() {
			match event.code {
				KeyCode::Char('q') | KeyCode::Esc => Signal::Pop,
				KeyCode::Up | KeyCode::Char('k') => {
					if !self.buttons.prev_child() {
						self.buttons.unfocus();
						self.disk_config.last_row();
						self.disk_config.focus();
					}
					Signal::Wait
				}
				KeyCode::Down | KeyCode::Char('j') => {
					if !self.buttons.next_child() {
						self.buttons.unfocus();
						self.disk_config.first_row();
						self.disk_config.focus();
					}
					Signal::Wait
				}
				KeyCode::Enter => {
					let Some(idx) = self.buttons.selected_child() else {
						return Signal::Wait;
					};
					match idx {
						0 => {
							// Suggest Partition Layout
							Signal::Push(Box::new(SuggestPartition::new()))
						}
						1 => {
							// Confirm and Exit
							if let Some(device) = installer.selected_drive_info.clone() {
								installer.drive_config_builder.set_device(device);
								let Ok(config_ir) = installer.drive_config_builder.clone().build_manual() else {
									log::error!("Failed to build manual drive config");
									return Signal::Wait;
								};
								installer.drive_config = Some(config_ir.into());
								installer.make_drive_config_display();
								return Signal::Unwind;
							}
							Signal::Wait
						}
						2 => {
							// Abort
							return Signal::Pop;
						}
						_ => Signal::Wait,
					}
				}
				_ => Signal::Wait,
			}
		} else {
			self.disk_config.focus();
			self.handle_input(installer, event)
		}
	}
}

pub struct SuggestPartition {
	buttons: WidgetBox
}

impl SuggestPartition {
	pub fn new() -> Self {
		let buttons = vec![
			Button::new("Yes"),
			Button::new("No"),
		];
		let mut button_row = WidgetBox::button_menu(buttons);
		button_row.focus();
		Self { buttons: button_row }
	}
}

impl Default for SuggestPartition {
		fn default() -> Self {
				Self::new()
		}
}

impl Page for SuggestPartition {
	fn render(&mut self, _installer: &Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(2)
			.constraints(
				[
					Constraint::Percentage(70),
					Constraint::Percentage(30),
				]
				.as_ref(),
			)
			.split(area);

		let info_box = InfoBox::new(
			"Suggest Partition Layout",
			indoc! {"Would you like to use a suggested partition layout for your selected drive?

				This will create a standard partition layout with a boot partition and a root partition.
				All existing data on the drive will be erased, and any existing manual configuration will be overwritten.

				Do you wish to proceed?
			"},
		);
		info_box.render(f, chunks[0]);
		self.buttons.render(f, chunks[1]);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('q') | KeyCode::Esc => Signal::Pop,
			KeyCode::Up | KeyCode::Char('k') => {
				self.buttons.prev_child();
				Signal::Wait
			}
			KeyCode::Down | KeyCode::Char('j') => {
				self.buttons.next_child();
				Signal::Wait
			}
			KeyCode::Enter => {
				let Some(idx) = self.buttons.selected_child() else {
					return Signal::Wait;
				};
				match idx {
					0 => {
						// Yes
						let fs = installer.drive_config_builder.fs.clone().unwrap_or_else(|| "ext4".to_string());
						installer.drive_config_builder.set_default_layout(fs);
						Signal::Pop
					}
					1 => {
						// No
						Signal::Pop
					}
					_ => Signal::Wait,
				}
			}
			_ => Signal::Wait,
		}
	}
}

pub struct AlterPartition {
	pub buttons: WidgetBox,
	pub dev_name: String,
	pub part_status: PartStatus
}

impl AlterPartition {
	pub fn new(dev_name: String, part_status: String) -> Self {
		let part_status = if part_status == PartStatus::Exists.to_string() {
			PartStatus::Exists
		} else if part_status == PartStatus::Modify.to_string() {
			PartStatus::Modify
		} else {
			PartStatus::Unknown
		};
		let buttons = vec![
			Button::new("Set Mount Point"),
			Button::new("Mark For Modification (data will be wiped on install)"),
			Button::new("Delete Partition"),
			Button::new("Back"),
		];
		let mut button_row = WidgetBox::button_menu(buttons);
		button_row.focus();
		Self { buttons: button_row, dev_name, part_status }
	}
	pub fn buttons_by_status(status: PartStatus) -> Vec<Button> {
		match status {
			PartStatus::Exists => vec![
				Button::new("Set Mount Point"),
				Button::new("Mark For Modification (data will be wiped on install)"),
				Button::new("Delete Partition"),
				Button::new("Back"),
			],
			PartStatus::Modify | PartStatus::Create => vec![
				Button::new("Set Mount Point"),
				Button::new("Mark as bootable partition"),
				Button::new("Mark as ESP partition"),
				Button::new("Mark as XBOOTLDR partition"),
				Button::new("Delete Partition"),
				Button::new("Back"),
			],
			_ => vec![
				Button::new("Back"),
			],
		}
	}
	pub fn render_existing_part(&self, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(2)
			.constraints(
				[
					Constraint::Percentage(70),
					Constraint::Percentage(30),
				]
				.as_ref(),
			)
			.split(area);

		let info_box = InfoBox::new(
			"Alter Existing Partition",
			indoc! {"Choose an action to perform on the selected partition.

				- 'Set Mount Point' allows you to specify where this partition will be mounted in the filesystem.
				- 'Mark For Modification' will flag this partition to be reformatted during installation (all data will be lost on installation). Partitions marked for modification have more options available in this menu.
				- 'Delete Partition' will remove this partition from the configuration.
				- 'Back' will return to the previous menu without making changes.
			"},
		);
		info_box.render(f, chunks[0]);
		self.buttons.render(f, chunks[1]);
	}
	pub fn render_modify_part(&self, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.margin(2)
			.constraints(
				[
					Constraint::Percentage(70),
					Constraint::Percentage(30),
				]
				.as_ref(),
			)
			.split(area);

		let info_box = InfoBox::new(
			"Alter Partition (Marked for Modification)",
			indoc! {"This partition is marked for modification. You can change its mount point or delete it.

				- 'Set Mount Point' allows you to specify where this partition will be mounted in the filesystem.
				- 'Delete Partition' will remove this partition from the configuration.
				- 'Back' will return to the previous menu without making changes.
			"},
		);
		info_box.render(f, chunks[0]);
		self.buttons.render(f, chunks[1]);
	}
}

impl Page for AlterPartition {
	fn render(&mut self, _installer: &Installer, f: &mut Frame, area: Rect) {
		match &self.part_status {
			PartStatus::Exists => {
				self.render_existing_part(f, area);
			}
			PartStatus::Modify | PartStatus::Create => {
				self.render_modify_part(f, area);
			}
			_ => {
				let info_box = InfoBox::new(
					"Alter Partition",
					"Unknown partition status. Cannot perform actions.",
				);
				info_box.render(f, area);
			}
		}
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('q') | KeyCode::Esc => Signal::Pop,
			KeyCode::Up | KeyCode::Char('k') => {
				self.buttons.prev_child();
				Signal::Wait
			}
			KeyCode::Down | KeyCode::Char('j') => {
				self.buttons.next_child();
				Signal::Wait
			}
			KeyCode::Enter => {
				let Some(idx) = self.buttons.selected_child() else {
					return Signal::Wait;
				};
				match idx {
					0 => {
						// Set Mount Point
						Signal::Push(Box::new(SetMountPoint::new(self.dev_name.clone())))
					}
					1 => {
						// Mark For Modification
						installer.drive_config_builder.mark_part_as_modify(&self.dev_name);
						Signal::Pop
					}
					2 => {
						// Delete Partition
						installer.drive_config_builder.delete_partition(&self.dev_name);
						Signal::Pop
					}
					3 => {
						// Back
						Signal::Pop
					}
					_ => Signal::Wait,
				}
			}
			_ => Signal::Wait,
		}
	}
}

pub struct SetMountPoint {
	editor: LineEditor,
	dev_name: String
}

impl SetMountPoint {
	pub fn new(dev_name: String) -> Self {
		let mut editor = LineEditor::new("Mount Point", Some("Enter a mount point..."));
		editor.focus();
		Self { editor, dev_name }
	}
}

impl Page for SetMountPoint {
	fn render(&mut self, _installer: &Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints(
				[
					Constraint::Percentage(40),
					Constraint::Length(3),
					Constraint::Percentage(40),
				]
				.as_ref(),
			)
			.split(area);
		let hor_chunks = Layout::default()
			.direction(Direction::Horizontal)
			.constraints(
				[
					Constraint::Percentage(33),
					Constraint::Percentage(34),
					Constraint::Percentage(33),
				]
				.as_ref(),
			)
			.split(chunks[1]);

		let info_box = InfoBox::new(
			"Set Mount Point",
			indoc! {"Specify the mount point for the selected partition.

				Examples of valid mount points include:
				- /
				- /home
				- /boot
				- /var

				Mount points must be absolute paths.
			"},
		);
		info_box.render(f, chunks[0]);
		self.editor.render(f, hor_chunks[1]);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Enter => {
				let mount_point = self.editor.get_value().unwrap().as_str().unwrap().trim().to_string();
				if mount_point.is_empty() {
					return Signal::Wait;
				}
				installer.drive_config_builder.set_part_mount_point(&self.dev_name, &mount_point);
				Signal::PopCount(2)
			}
			_ => self.editor.handle_input(event)
		}
	}
}
