use std::{fmt::{Debug, Display}, str::FromStr};

use indoc::indoc;
use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Direction, Layout, Rect}, style::{Color, Modifier}, Frame};
use serde_json::Value;

use crate::{drives::{get_entry_id, DiskEntry, DiskPlan, DiskPlanBuilder, DiskSize, DiskTable, DiskTableHeader, EntryType, PartStatus}, styled_block, widget::{Button, CheckBox, ConfigWidget, InfoBox, LineEditor, StrList, TableWidget, WidgetBox, WidgetBoxBuilder}};



#[derive(Default)]
pub struct Installer {

	pub selected_drive_info: Option<DiskEntry>,
	pub drive_config_builder: DiskPlanBuilder,
	pub use_auto_drive_config: bool,
	pub drive_config: Option<DiskPlan>,


	pub drive_config_display: Option<TableWidget>,

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
	pub fn info_box_for_item(&self, installer: &mut Installer, idx: usize) -> WidgetBox {
		let mut display_widget: Option<Box<dyn ConfigWidget>> = None;
		let (title, content) = match idx {
			0 => (
				"Source Flake",
				styled_block(vec![
					vec![(None, "Choose a flake output to use as your system configuration.")],
					vec![(None, "This can be used in place of manual configuration using this installer. You will still need to set up a disk partitioning plan, however.")],
					vec![(None, "This can be "), (Some((Color::Reset, Modifier::ITALIC)), "any valid path"), (None, " to a flake output that produces a "), (Some((Color::Cyan, Modifier::BOLD)), "'nixosConfiguration'")],
					vec![(None, " attribute.")],
					vec![(None, "Examples include:")],
					vec![(None, " - A local flake: "), (Some((Color::Yellow, Modifier::BOLD)), "'/path/to/flake#my-host'")],
					vec![(None, " - A GitHub flake: "), (Some((Color::Yellow, Modifier::BOLD)), "'github:user/repo#my-host'")],
					vec![(None, "")],
					vec![(Some((Color::DarkGray,Modifier::ITALIC)), "Don't forget to double check your config's hardware configuration :)")]
				])
			),
			1 => (
				"Language",
				styled_block(vec![
					vec![(None, "Select the language to be used for your system.")],
				])
			),
			2 => (
				"Keyboard Layout",
				styled_block(vec![
					vec![(None, "Choose the keyboard layout that matches your physical keyboard.")],
				])
			),
			3 => (
				"Locale",
				styled_block(vec![
					vec![(None, "Set the locale for your system, which determines language and regional settings.")],
				])
			),
			4 => (
				"Use Flakes",
				styled_block(vec![
					vec![(None, "Decide whether to use Nix Flakes for package management.")],
					vec![(None, "Will write 'nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ];' to your generated configuration.")],
				])
			),
			5 => {
				display_widget = installer.drive_config_display.as_ref().map(|w| Box::new(w.clone()) as Box<dyn ConfigWidget>);
				(
					"Drives",
					styled_block(vec![
						vec![(None, "Select and configure the drives for your NixOS installation.")],
						vec![(None, "This includes partitioning, formatting, and mount points.")],
						vec![(None, "If you have already configured a drive, its current configuration will be shown below.")],
					])
				)
			}
			6 => (
				"Bootloader",
				styled_block(vec![
					vec![(None, "Choose a bootloader to install for your system.")],
				])
			),
			7 => (
				"Swap",
				styled_block(vec![
					vec![(None, "Configure swap space for your system.")],
				])
			),
			8 => (
				"Hostname",
				styled_block(vec![
					vec![(None, "Set the hostname for your system.")],
				])
			),
			9 => (
				"Root Password",
				styled_block(vec![
					vec![(None, "Set the root password for your system.")],
				])
			),
			10 => (
				"User Accounts",
				styled_block(vec![
					vec![(None, "Create and manage user accounts for your system.")],
				])
			),
			11 => (
				"Profile",
				styled_block(vec![
					vec![(None, "Select a NixOS profile to use for your system.")],
				])
			),
			12 => (
				"Greeter",
				styled_block(vec![
					vec![(None, "Choose a greeter (login screen) for your system.")],
				])
			),
			13 => (
				"Desktop Environment",
				styled_block(vec![
					vec![(None, "Select a desktop environment for your system.")],
				])
			),
			14 => (
				"Audio",
				styled_block(vec![
					vec![(None, "Configure audio settings and select audio backends.")],
				])
			),
			15 => (
				"Kernels",
				styled_block(vec![
					vec![(None, "Choose which kernel(s) to install for your system.")],
				])
			),
			16 => (
				"Virtualization",
				styled_block(vec![
					vec![(None, "Configure virtualization settings and select virtualization backends.")],
				])
			),
			17 => (
				"System Packages",
				styled_block(vec![
					vec![(None, "Select additional system packages to install.")],
				])
			),
			18 => (
				"Network",
				styled_block(vec![
					vec![(None, "Configure network settings for your system.")],
				])
			),
			19 => (
				"Timezone",
				styled_block(vec![
					vec![(None, "Set the timezone for your system.")],
				])
			),
			_ => (
				"Unknown Option",
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

pub struct Drives<'a> {
	pub buttons: WidgetBox,
	pub info_box: InfoBox<'a>
}

impl<'a> Drives<'a> {
	pub fn new() -> Self {
		let buttons = vec![
			Box::new(Button::new("Use a best-effort default partition layout")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("Configure partitions manually")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("Back")) as Box<dyn ConfigWidget>,
		];
		let mut button_row = WidgetBox::button_menu(buttons);
		button_row.focus();
		let info_box = InfoBox::new(
			"Drive Configuration",
			styled_block(vec![
				vec![(None, "Select how you would like to configure your drives for the NixOS installation.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Use a best-effort default partition layout'"), (None, " will attempt to automatically partition and format your selected drive(s) with sensible defaults. "), (None, "This is recommended for most users.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Configure partitions manually'"), (None, " will allow you to specify exactly how your drives should be partitioned and formatted. "), (None, "This is recommended for advanced users who have specific requirements.")],
				vec![(Some((Color::Red, Modifier::BOLD)), "NOTE: "), (None, "When the installer is run, "), (Some((Color::Red, Modifier::BOLD | Modifier::ITALIC)), " any and all"), (None, " data on the selected drive will be wiped. Make sure you've backed up any important data.")],
			])
		);

		Self { buttons: button_row, info_box }
	}
}

impl<'a> Default for Drives<'a> {
	fn default() -> Self {
		Self::new()
	}
}

impl<'a> Page for Drives<'a> {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
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
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
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
					let dev_id = row.fields.last().and_then(|f| f.parse::<u64>().ok());
					let Some(id) = dev_id else {
						log::error!("Failed to find drive info'");
						return Signal::Wait;
					};

					let drive_info = DiskTable::from_lsblk().unwrap();
					let parent = drive_info.filter_by(|d| d.id == id).entries().first().cloned().unwrap();

					installer.selected_drive_info = Some(parent.clone());
					installer.drive_config_builder.set_device(parent.clone());
					let children = DiskTable::from_lsblk().unwrap()
						.filter_by(|d| d.parent.as_deref() == Some(&parent.device));
					installer.drive_config_builder.set_layout(children.entries().to_vec());

					if installer.use_auto_drive_config {
						Signal::Push(Box::new(SelectFilesystem::new(id)))
					} else {
						Signal::Push(Box::new(ManualPartition::new()))
					}
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
	pub dev_id: u64,
}

impl SelectFilesystem {
	pub fn new(dev_id: u64) -> Self {
		let buttons = vec![
			Box::new(Button::new("ext4")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("ext3")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("ext2")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("btrfs")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("xfs")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("fat12")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("fat16")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("fat32")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("ntfs")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("Back")) as Box<dyn ConfigWidget>,
		];
		let mut button_row = WidgetBox::button_menu(buttons);
		button_row.focus();
		Self { buttons: button_row, dev_id }
	}
	pub fn get_fs_info<'a>(idx: usize) -> InfoBox<'a> {
		match idx {
			0 => InfoBox::new(
				"ext4",
				styled_block(vec![
					vec![
						(Some((Color::Yellow, Modifier::BOLD)), "ext4"),
						(None, " is a"),
						(Some((Color::Yellow, Modifier::BOLD)), " widely used and stable filesystem"),
						(None, " known for its "),
						(Some((Color::Yellow, Modifier::BOLD)), "reliability and performance.")
					],
					vec![
						(None, "It supports journaling, which helps protect against data corruption in case of crashes.")
					],
					vec![
						(None, "It's a good choice for"),
						(Some((Color::Yellow,Modifier::BOLD)), " general-purpose"),
						(None, " use and is"),
						(Some((Color::Yellow,Modifier::BOLD)), " well-supported across various Linux distributions.")
					],
				])
			),
			1 => InfoBox::new(
				"ext3",
				styled_block(vec![
					vec![
						(Some((Color::Yellow, Modifier::BOLD)), "ext3"),
						(None, " is an older journaling filesystem that builds upon ext2."),
					],
					vec![
						(None, "It provides "),
						(Some((Color::Yellow, Modifier::BOLD)), "journaling"),
						(None, " capabilities to improve data integrity and recovery after crashes."),
					],
					vec![
						(None, "While it is reliable and stable, it lacks some of the performance and features of ext4."),
					],
				])
			),
			2 => InfoBox::new(
				"ext2",
				styled_block(vec![
					vec![
						(Some((Color::Yellow, Modifier::BOLD)), "ext2"),
						(None, " is a non-journaling filesystem that is simple and efficient."),
					],
					vec![
						(None, "It is suitable for use cases where journaling is not required, such as "),
						(Some((Color::Yellow, Modifier::BOLD)), "flash drives"),
						(None, " or "),
						(Some((Color::Yellow, Modifier::BOLD)), "small partitions"),
						(None, "."),
					],
					vec![
						(None, "However, it is more prone to data corruption in case of crashes compared to ext3 and ext4."),
					],
				])
			),
			3 => InfoBox::new(
				"btrfs",
				styled_block(vec![
					vec![
						(Some((Color::Yellow, Modifier::BOLD)), "btrfs"),
						(None, " ("),
						(Some((Color::Reset, Modifier::ITALIC)), "B-tree filesystem"),
						(None, ") is a "),
						(Some((Color::Yellow, Modifier::BOLD)), "modern filesystem"),
						(None, " that offers advanced features like "),
						(Some((Color::Yellow, Modifier::BOLD)), "snapshots"),
						(None, ", "),
						(Some((Color::Yellow, Modifier::BOLD)), "subvolumes"),
						(None, ", and "),
						(Some((Color::Yellow, Modifier::BOLD)), "built-in RAID support"),
						(None, "."),
					],
					vec![
						(None, "It is designed for "),
						(Some((Color::Yellow, Modifier::BOLD)), "scalability"),
						(None, " and "),
						(Some((Color::Yellow, Modifier::BOLD)), "flexibility"),
						(None, ", making it suitable for systems that require complex storage solutions."),
					],
					vec![
						(None, "However, it may not be as mature as "),
						(Some((Color::Yellow, Modifier::BOLD)), "ext4"),
						(None, " in terms of "),
						(Some((Color::Yellow, Modifier::BOLD)), "stability"),
						(None, " for all use cases."),
					],
				])
			),
			4 => InfoBox::new(
				"xfs",
				styled_block(vec![
					vec![
						(Some((Color::Yellow, Modifier::BOLD)), "XFS"),
						(None, " is a "),
						(Some((Color::Yellow, Modifier::BOLD)), "high-performance journaling filesystem"),
						(None, " that excels in handling "),
						(Some((Color::Yellow, Modifier::BOLD)), "large files"),
						(None, " and "),
						(Some((Color::Yellow, Modifier::BOLD)), "high I/O workloads"),
						(None, "."),
					],
					vec![
						(None, "It is known for its "),
						(Some((Color::Yellow, Modifier::BOLD)), "scalability"),
						(None, " and "),
						(Some((Color::Yellow, Modifier::BOLD)), "robustness"),
						(None, ", making it a popular choice for "),
						(Some((Color::Yellow, Modifier::BOLD)), "enterprise environments"),
						(None, "."),
					],
					vec![
						(Some((Color::Yellow, Modifier::BOLD)), "XFS"),
						(None, " is particularly well-suited for systems that require efficient handling of "),
						(Some((Color::Yellow, Modifier::BOLD)), "large datasets"),
						(None, "."),
					],
				])
			),
			5 => InfoBox::new(
				"fat12",
				styled_block(vec![
					vec![
						(Some((Color::Yellow, Modifier::BOLD)), "FAT12"),
						(None, " is a simple and widely supported filesystem primarily used for "),
						(Some((Color::Yellow, Modifier::BOLD)), "small storage devices"),
						(None, " like floppy disks."),
					],
					vec![
						(None, "It has limitations in terms of maximum partition size and file size, making it less suitable for modern systems."),
					],
				])
			),
			6 => InfoBox::new(
				"fat16",
				styled_block(vec![
					vec![
						(Some((Color::Yellow, Modifier::BOLD)), "FAT16"),
						(None, " is an older filesystem that extends FAT12 to support larger partitions and files."),
					],
					vec![
						(None, "It is still used in some embedded systems and older devices but has limitations compared to more modern filesystems."),
					],
				])
			),
			7 => InfoBox::new(
				"fat32",
				styled_block(vec![
					vec![
					(Some((Color::Yellow, Modifier::BOLD)), "FAT32"),
					(None, " is a widely supported filesystem that can handle larger partitions and files than FAT16."),
					],
					vec![
					(None, "It is commonly used for USB drives and memory cards due to its broad "),
					(Some((Color::Yellow, Modifier::BOLD)), "cross-platform compatibility"),
					(None, "."),
					],
					vec![
					(None, "FAT32 is also commonly used for "),
					(Some((Color::Yellow, Modifier::BOLD)), "EFI System Partitions (ESP)"),
					(None, " on UEFI systems, allowing the firmware to load the bootloader."),
					],
					vec![
					(None, "However, it has limitations such as a maximum file size of 4GB and lack of modern journaling features."),
					],
				])
			),
			8 => InfoBox::new(
				"ntfs",
				styled_block(vec![
					vec![
						(Some((Color::Yellow, Modifier::BOLD)), "NTFS"),
						(None, " is a robust and feature-rich filesystem developed by Microsoft."),
					],
					vec![
						(None, "It supports large files, advanced permissions, encryption, and journaling."),
					],
					vec![
						(None, "While it is primarily used in Windows environments, Linux has good support for NTFS through the "),
						(Some((Color::Yellow, Modifier::BOLD)), "ntfs-3g"),
						(None, " driver."),
					],
					vec![
						(None, "NTFS is a good choice if you need to share data between Windows and Linux systems or if you require features like file compression and encryption."),
					],
				])
			),
			_ => InfoBox::new(
				"Unknown Filesystem",
				styled_block(vec![
					vec![(None, "No information available for this filesystem.")],
				])
			),
		}
	}
}

impl Page for SelectFilesystem {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
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


		let idx = self.buttons.selected_child().unwrap_or(9);
		let info_box = Self::get_fs_info(self.buttons.selected_child().unwrap_or(9));
		self.buttons.render(f, hor_chunks[1]);
		if idx < 9 {
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
					1 => "ext3",
					2 => "ext2",
					3 => "btrfs",
					4 => "xfs",
					5 => "fat12",
					6 => "fat16",
					7 => "fat32",
					8 => "ntfs",
					9 => return Signal::Pop,
					_ => return Signal::Wait,
				}.to_string();

				if installer.use_auto_drive_config {
					installer.drive_config_builder.set_part_table("gpt");
					installer.drive_config_builder.set_fs(&fs);
					let Ok(config_ir) = installer.drive_config_builder.clone().build_default() else {
						log::error!("Failed to build auto drive config");
						return Signal::Pop;
					};
					installer.drive_config = Some(config_ir);
					installer.make_drive_config_display();
				} else {
					let drive_info = installer.drive_config_builder.find_by_id(self.dev_id);
					if drive_info.is_some() {
						installer.drive_config_builder.set_part_fs_type(self.dev_id, &fs);
						return Signal::PopCount(2)
					} else {
						log::error!("Failed to find drive info for id {}", self.dev_id);
						return Signal::PopCount(2);
					}
				}


				Signal::Unwind
			}
			_ => Signal::Wait,
		}
	}
}

pub struct ManualPartition {
	disk_config: TableWidget,
	buttons: WidgetBox,
	confirming_reset: bool,
}

impl ManualPartition {
	pub fn new() -> Self {
		let mut disk_config = DiskTable::empty().as_widget(Some(DiskTableHeader::all_headers()));
		let buttons = vec![
			Box::new(Button::new("Suggest Partition Layout")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("Confirm and Exit")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("Reset Partition Layout")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("Abort")) as Box<dyn ConfigWidget>,
		];
		let buttons = WidgetBox::button_menu(buttons);
		disk_config.focus();
		Self { disk_config, buttons, confirming_reset: false }
	}
}

impl Default for ManualPartition {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for ManualPartition {
	fn render(&mut self, installer: &mut Installer, f: &mut Frame, area: Rect) {
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
		if self.confirming_reset && event.code != KeyCode::Enter {
			self.confirming_reset = false;
			self.buttons.set_children_inplace(vec![
				Box::new(Button::new("Suggest Partition Layout")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("Confirm and Exit")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("Reset Partition Layout")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("Abort")) as Box<dyn ConfigWidget>,
			]);
		}
		if self.disk_config.is_focused() {
			match event.code {
				KeyCode::Char('q') | KeyCode::Esc => Signal::PopCount(2),
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
					let dev_id = row.fields.last().and_then(|id| id.parse::<u64>().ok());
					let status = row.get_field("status");
					let kind = row.get_field("type");
					let flags = row.get_field("flags").unwrap().split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>();
					if let Some(dev_id) = dev_id {
						if let Some(status) = status {
							if status == "create" && kind == Some(&"free".to_string()) {
								let fs_entry = installer.drive_config_builder.find_by_id(dev_id);
								if let Some(entry) = fs_entry {
									let DiskSize::Literal(sector_size) = entry.sector_size.unwrap_or(DiskSize::Literal(512)) else { unreachable!() };
									Signal::Push(Box::new(NewPartition::new(entry.start as u64, sector_size, entry.size, Some(entry.id))))
								} else {
									// Safe to assume that we are working with a device that has no partitions
									let Some(ref dev_info) = installer.selected_drive_info else {
										log::error!("No selected drive info available");
										return Signal::Wait;
									};
									let start = 2048u64; // Default start sector for new partitions
									let sector_size = dev_info.sector_size.unwrap_or(DiskSize::Literal(512));
									let size = dev_info.size;
									Signal::Push(Box::new(NewPartition::new(start, sector_size.as_bytes(DiskSize::Literal(u64::MAX)), size, None)))
								}
							} else {
								Signal::Push(Box::new(AlterPartition::new(dev_id, status.clone(), flags.clone())))
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
				KeyCode::Char('q') | KeyCode::Esc => Signal::PopCount(2),
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
								installer.drive_config = Some(config_ir);
								installer.make_drive_config_display();
								return Signal::Unwind;
							}
							Signal::Wait
						}
						2 => {
							if !self.confirming_reset {
								self.confirming_reset = true;
								let new_buttons = vec![
									Box::new(Button::new("Suggest Partition Layout")) as Box<dyn ConfigWidget>,
									Box::new(Button::new("Confirm and Exit")) as Box<dyn ConfigWidget>,
									Box::new(Button::new("Really?")) as Box<dyn ConfigWidget>,
									Box::new(Button::new("Abort")) as Box<dyn ConfigWidget>,
								];
								self.buttons.set_children_inplace(new_buttons);
								Signal::Wait
							} else {
								let Some(ref device) = installer.selected_drive_info else {
									return Signal::Wait;
								};
								let device = device.device.clone();
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
								self.buttons.unfocus();
								self.disk_config.first_row();
								self.disk_config.focus();
								self.confirming_reset = false;
								self.buttons.set_children_inplace(vec![
									Box::new(Button::new("Suggest Partition Layout")) as Box<dyn ConfigWidget>,
									Box::new(Button::new("Confirm and Exit")) as Box<dyn ConfigWidget>,
									Box::new(Button::new("Reset Partition Layout")) as Box<dyn ConfigWidget>,
									Box::new(Button::new("Abort")) as Box<dyn ConfigWidget>,
								]);
								Signal::Wait
							}
						}
						3 => {
							// Abort
							return Signal::PopCount(2);
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
			Box::new(Button::new("Yes")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("No")) as Box<dyn ConfigWidget>,
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
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
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
			styled_block(vec![
				vec![
					(None, "Would you like to use a "),
					(Some((Color::Yellow,Modifier::BOLD)), "suggested partition layout "),
					(None, "for your selected drive?")
				],
				vec![
					(None, "This will create a standard layout with a "),
					(Some((Color::Yellow,Modifier::BOLD)), "boot partition "),
					(None, "and a "),
					(Some((Color::Yellow,Modifier::BOLD)), "root partition."),
				],
				vec![
					(None, "Any existing manual configuration will be overwritten, and when the installer is run, "),
					(Some((Color::Red, Modifier::ITALIC | Modifier::BOLD)), "all existing data on the drive will be erased."),
				],
				vec![(None, "")],
				vec![(None, "Do you wish to proceed?")],
			])
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

pub struct NewPartition {
	pub free_space_id: Option<u64>,
	pub part_start: u64,
	pub part_end: u64,
	pub sector_size: u64,
	pub total_size: DiskSize,

	pub new_part_size: Option<DiskSize>,
	pub size_input: LineEditor,

	pub new_part_fs: Option<String>,
	pub fs_buttons: WidgetBox,

	pub new_part_mount_point: Option<String>,
	pub mount_input: LineEditor,
}

impl NewPartition {
	pub fn new(part_start: u64, sector_size: u64, total_size: DiskSize, free_space_id: Option<u64>) -> Self {
		let bytes = total_size.as_bytes(DiskSize::Literal(u64::MAX));
		let sectors = bytes.div_ceil(sector_size); // round up
		let part_end = part_start + sectors - 1;
		let fs_buttons = {
			let buttons = vec![
				Box::new(Button::new("ext4")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("ext3")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("ext2")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("btrfs")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("xfs")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("fat12")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("fat16")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("fat32")) as Box<dyn ConfigWidget>,
				Box::new(Button::new("ntfs")) as Box<dyn ConfigWidget>,
			];
			let mut button_row = WidgetBox::button_menu(buttons);
			button_row.focus();
			button_row
		};
		let mount_input = LineEditor::new("New Partition Mount Point", None::<&str>);
		let mut size_input = LineEditor::new("New Partition Size", None::<&str>);
		size_input.focus();
		Self {
			free_space_id,
			part_start,
			sector_size,
			total_size,
			part_end,

			new_part_size: None,
			size_input,

			new_part_fs: None,
			fs_buttons,

			new_part_mount_point: None,
			mount_input
		}
	}
	pub fn render_size_input(&mut self, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
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
			.constraints(
				[
					Constraint::Percentage(33),
					Constraint::Percentage(34),
					Constraint::Percentage(33),
				]
				.as_ref(),
			)
			.split(chunks[1]);

		let info_box = InfoBox::new("Free Space Info", styled_block(vec![
				vec![(Some((Color::Yellow, Modifier::BOLD)), "Sector Size: "), (None, &format!("{}", self.sector_size))],
				vec![(Some((Color::Yellow, Modifier::BOLD)), "Partition Start Sector: "), (None, &format!("{}", self.part_start))],
				vec![(Some((Color::Yellow, Modifier::BOLD)), "Partition End Sector: "), (None, &format!("{}", self.part_end))],
				vec![(Some((Color::Yellow, Modifier::BOLD)), "Total Free Space: "), (None, &format!("{}", self.total_size))],
				vec![(None, "")],
				vec![(None, "Enter the desired size for the new partition. You can specify sizes in bytes (B), kilobytes (KB), megabytes (MB), gigabytes (GB), terabytes (TB), or as a percentage of the total free space (e.g., 50%). A number given without a unit is counted in sectors.")],
				vec![(None, "Examples: "), (Some((Color::Green, Modifier::BOLD)), "10GB"), (None, ", "), (Some((Color::Green, Modifier::BOLD)), "500MiB"), (None, ", "), (Some((Color::Green, Modifier::BOLD)), "100%")],
		]));
		info_box.render(f, chunks[0]);
		self.size_input.render(f, hor_chunks[1]);
	}
	pub fn handle_input_size(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Enter => {
				let input = self.size_input.get_value().unwrap();
				let input = input.as_str().unwrap().trim(); // TODO: handle these unwraps
				if input.is_empty() {
					return Signal::Wait;
				}
				match DiskSize::from_str(input) {
					Ok(size) => {
						self.new_part_size = Some(size);
						self.size_input.unfocus();
						self.fs_buttons.focus();
						Signal::Wait
					}
					Err(_) => {
						self.size_input.error("Invalid size input");
						Signal::Wait
					}
				}
			}
			_ => self.size_input.handle_input(event),
		}
	}
	pub fn render_fs_select(&mut self, f: &mut Frame, area: Rect) {
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

		let idx = self.fs_buttons.selected_child().unwrap_or(9);
		let info_box = SelectFilesystem::get_fs_info(self.fs_buttons.selected_child().unwrap_or(9));
		self.fs_buttons.render(f, hor_chunks[1]);
		if idx < 9 {
			info_box.render(f, vert_chunks[1]);
		}
	}
	pub fn handle_input_fs_select(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('q') | KeyCode::Esc => Signal::Pop,
			KeyCode::Up | KeyCode::Char('k') => {
				self.fs_buttons.prev_child();
				Signal::Wait
			}
			KeyCode::Down | KeyCode::Char('j') => {
				self.fs_buttons.next_child();
				Signal::Wait
			}
			KeyCode::Enter => {
				let Some(idx) = self.fs_buttons.selected_child() else {
					return Signal::Wait;
				};
				let fs =  match idx {
					0 => "ext4",
					1 => "ext3",
					2 => "ext2",
					3 => "btrfs",
					4 => "xfs",
					5 => "fat12",
					6 => "fat16",
					7 => "fat32",
					8 => "ntfs",
					9 => {
						self.new_part_size = None;
						self.size_input.focus();
						self.fs_buttons.unfocus();
						return Signal::Wait;
					}
					_ => return Signal::Wait,
				}.to_string();

				self.new_part_fs = Some(fs);
				self.fs_buttons.unfocus();
				self.mount_input.focus();
				Signal::Wait
			}
			_ => Signal::Wait,
		}
	}
	pub fn render_mount_point_input(&mut self, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints(
				[
					Constraint::Percentage(70),
					Constraint::Length(5),
					Constraint::Percentage(25),
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

		let info_box = InfoBox::new("Mount Point Info", styled_block(vec![
				vec![(None, "Enter the mount point for the new partition. This is the directory where the partition will be mounted in the filesystem.")],
				vec![(None, "Common mount points include "), (Some((Color::Green, Modifier::BOLD)), "/"), (None, " for root, "), (Some((Color::Green, Modifier::BOLD)), "/home"), (None, " for user data, "), (Some((Color::Green, Modifier::BOLD)), "/boot"), (None, " for boot files, and "), (Some((Color::Green, Modifier::BOLD)), "/var"), (None, " for variable data.")],
				vec![(None, "You can also specify other mount points as needed.")],
				vec![(None, "")],
				vec![(None, "Examples: "), (Some((Color::Green, Modifier::BOLD)), "/"), (None, ", "), (Some((Color::Green, Modifier::BOLD)), "/home"), (None, ", "), (Some((Color::Green, Modifier::BOLD)), "/mnt/data")],
		]));
		info_box.render(f, chunks[0]);
		self.mount_input.render(f, hor_chunks[1]);
	}
	pub fn handle_input_mount_point(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => {
				self.new_part_fs = None;
				self.fs_buttons.focus();
				self.mount_input.unfocus();
				Signal::Wait
			}
			KeyCode::Enter => {
				let input = self.mount_input.get_value().unwrap();
				let input = input.as_str().unwrap().trim(); // TODO: handle these unwraps
				let taken_mounts: Vec<String> = installer
						.drive_config_builder
						.layout
						.iter()
						.filter_map(|d| d.mount_point.clone())
						.collect();

				if let Err(err) = SetMountPoint::validate_mount_point(input, &taken_mounts) {
					self.mount_input.error(&err);
					return Signal::Wait;
				}
				self.new_part_mount_point = Some(input.to_string());
				self.mount_input.unfocus();

				let flags = if self.new_part_mount_point.as_deref() == Some("/boot") {
					vec!["boot".to_string(), "esp".to_string()]
				} else {
					vec![]
				};

				let new_disk_entry_name = "-".to_string();
				let parent = installer.selected_drive_info.as_ref().map(|d| d.device.clone());
				let new_disk_entry = DiskEntry {
					id: get_entry_id(new_disk_entry_name.clone(), self.part_start, self.new_part_size.unwrap_or(self.total_size).as_bytes(DiskSize::Literal(u64::MAX)).div_ceil(self.sector_size), None),
					parent,
					device: new_disk_entry_name,
					entry_type: EntryType::Partition,
					read_only: false,
					start: self.part_start as usize,
					size: self.new_part_size.unwrap(),
					fs_type: self.new_part_fs.clone(),
					mount_point: self.new_part_mount_point.clone(),
					label: None,
					flags,
					status: PartStatus::Create,
					sector_size: Some(DiskSize::Literal(self.sector_size)),
				};
				installer.drive_config_builder.insert_new_entry(new_disk_entry);

				if let Some(id) = self.free_space_id {
					if let Some(free_space) = installer.drive_config_builder.find_by_id_mut(id) {
						let used_sectors = self.new_part_size.unwrap().as_sectors(self.sector_size as usize);
						if free_space.size.as_bytes(DiskSize::Literal(u64::MAX)) > self.new_part_size.unwrap().as_bytes(DiskSize::Literal(u64::MAX)) {
							free_space.start += used_sectors as usize;
							let new_size_bytes = free_space.size.as_bytes(DiskSize::Literal(u64::MAX)).saturating_sub(self.new_part_size.unwrap().as_bytes(DiskSize::Literal(u64::MAX)));
							free_space.size = DiskSize::Literal(new_size_bytes);
						} else {
						}
					} else {
						log::warn!("Failed to find free space entry with id {id}");
					}
				}

				Signal::Pop
			}
			_ => self.mount_input.handle_input(event),
		}
	}
}

impl Page for NewPartition {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		if self.new_part_size.is_none() {
			self.render_size_input(f, area);

		} else if self.new_part_fs.is_none() {
			self.render_fs_select(f, area);

		} else if self.new_part_mount_point.is_none() {
			self.render_mount_point_input(f, area);
		}
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		if self.new_part_size.is_none() {
			self.handle_input_size(installer, event)

		} else if self.new_part_fs.is_none() {
			self.handle_input_fs_select(installer, event)

		} else if self.new_part_mount_point.is_none() {
			self.handle_input_mount_point(installer, event)

		} else {
			Signal::Pop
		}
	}
}

pub struct AlterPartition {
	pub buttons: WidgetBox,
	pub dev_id: u64,
	pub part_status: PartStatus,
}

impl AlterPartition {
	pub fn new(dev_id: u64, part_status: String, flags: Vec<String>) -> Self {
		let part_status = if part_status == PartStatus::Exists.to_string() {
			PartStatus::Exists
		} else if part_status == PartStatus::Modify.to_string() {
			PartStatus::Modify
		} else if part_status == PartStatus::Create.to_string() {
			PartStatus::Create
		} else if part_status == PartStatus::Delete.to_string() {
			PartStatus::Delete
		} else {
			PartStatus::Unknown
		};
		let buttons = Self::buttons_by_status(part_status, flags);
		let mut button_row = WidgetBox::button_menu(buttons);
		button_row.focus();
		Self { buttons: button_row, dev_id, part_status }
	}
	pub fn buttons_by_status(status: PartStatus, flags: Vec<String>) -> Vec<Box<dyn ConfigWidget>> {
		match status {
			PartStatus::Exists => vec![
				Box::new(Button::new("Set Mount Point")),
				Box::new(Button::new("Mark For Modification (data will be wiped on install)")),
				Box::new(Button::new("Delete Partition")),
				Box::new(Button::new("Back")),
			],
			PartStatus::Modify => vec![
				Box::new(Button::new("Set Mount Point")),
				Box::new(CheckBox::new("Mark as bootable partition", flags.contains(&"boot".into()))),
				Box::new(CheckBox::new("Mark as ESP partition", flags.contains(&"esp".into()))),
				Box::new(CheckBox::new("Mark as XBOOTLDR partition", flags.contains(&"bls_boot".into()))),
				Box::new(Button::new("Change Filesystem")),
				Box::new(Button::new("Set Label")),
				Box::new(Button::new("Unmark for modification")),
				Box::new(Button::new("Delete Partition")),
				Box::new(Button::new("Back")),
			],
			PartStatus::Create => vec![
				Box::new(Button::new("Set Mount Point")),
				Box::new(CheckBox::new("Mark as bootable partition", flags.contains(&"boot".into()))),
				Box::new(CheckBox::new("Mark as ESP partition", flags.contains(&"esp".into()))),
				Box::new(CheckBox::new("Mark as XBOOTLDR partition", flags.contains(&"bls_boot".into()))),
				Box::new(Button::new("Change Filesystem")),
				Box::new(Button::new("Set Label")),
				Box::new(Button::new("Delete Partition")),
				Box::new(Button::new("Back")),
			],
			_ => vec![
				Box::new(Button::new("Back")),
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
			styled_block(vec![
				vec![(None, "Choose an action to perform on the selected partition.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Set Mount Point'"), (None, " allows you to specify where this partition will be mounted in the filesystem.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Mark For Modification'"), (None, " will flag this partition to be reformatted during installation (all data will be lost on installation). Partitions marked for modification have more options available in this menu.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Delete Partition'"), (None, " Mark this existing partition for deletion. The space it occupies will be freed for replacement.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Back'"), (None, " return to the previous menu without making changes.")],
			])
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
			styled_block(vec![
				vec![(None, "This partition is marked for modification. You can change its mount point or delete it.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Set Mount Point'"), (None, " allows you to specify where this partition will be mounted in the filesystem.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Delete Partition'"), (None, " will remove this partition from the configuration.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Back'"), (None, " return to the previous menu without making changes.")],
			])
		);
		info_box.render(f, chunks[0]);
		self.buttons.render(f, chunks[1]);
	}
	pub fn render_delete_part(&self, f: &mut Frame, area: Rect) {
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
			"Deleted Partition",
			styled_block(vec![
				vec![(None,"This partition has been marked for deletion.")],
				vec![(None,"Reclaiming the freed space can cause unpredictable behavior, so if you wish to reclaim the space freed by marking this partition for deletion, please return to the previous menu and reset the partition layout.")],
			])
		);
		info_box.render(f, chunks[0]);
		self.buttons.render(f, chunks[1]);
	}
}

impl Page for AlterPartition {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		match &self.part_status {
			PartStatus::Exists => {
				self.render_existing_part(f, area);
			}
			PartStatus::Modify | PartStatus::Create => {
				self.render_modify_part(f, area);
			}
			PartStatus::Delete => {
				self.render_delete_part(f, area);
			}
			_ => {
				let info_box = InfoBox::new(
					"Alter Partition",
					styled_block(vec![
						vec![(None, "The partition status is unknown. No actions can be performed on this partition.")],
					])
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
				if self.part_status == PartStatus::Delete {
					return Signal::Pop
				}
				let Some(idx) = self.buttons.selected_child() else {
					return Signal::Wait;
				};
				match self.part_status {
					PartStatus::Exists => {
						match idx {
							0 => {
								// Set Mount Point
								Signal::Push(Box::new(SetMountPoint::new(self.dev_id)))
							}
							1 => {
								// Mark For Modification
								installer.drive_config_builder.mark_part_as_modify(self.dev_id);
								Signal::Pop
							}
							2 => {
								// Delete Partition
								installer.drive_config_builder.delete_partition(self.dev_id);
								Signal::Pop
							}
							3 => {
								// Back
								Signal::Pop
							}
							_ => Signal::Wait,
						}
					}
					PartStatus::Modify => {
						match idx {
							0 => {
								// Set Mount Point
								Signal::Push(Box::new(SetMountPoint::new(self.dev_id)))
							}
							1 => {
								if let Some(child) = self.buttons.focused_child_mut() {
									child.interact();
									if let Some(value) = child.get_value() {
										let Value::Bool(checked) = value else {
											return Signal::Wait;
										};
										installer.drive_config_builder.set_part_flag(self.dev_id, "boot", checked);
									}
								}
								Signal::Wait
							}
							2 => {
								if let Some(child) = self.buttons.focused_child_mut() {
									child.interact();
									if let Some(value) = child.get_value() {
										let Value::Bool(checked) = value else {
											return Signal::Wait;
										};
										installer.drive_config_builder.set_part_flag(self.dev_id, "esp", checked);
									}
								}
								Signal::Wait
							}
							3 => {
								if let Some(child) = self.buttons.focused_child_mut() {
									child.interact();
									if let Some(value) = child.get_value() {
										let Value::Bool(checked) = value else {
											return Signal::Wait;
										};
										installer.drive_config_builder.set_part_flag(self.dev_id, "bls_boot", checked);
									}
								}
								Signal::Wait
							}
							4 => {
								// Change Filesystem
								Signal::Push(Box::new(SelectFilesystem::new(self.dev_id)))
							}
							5 => {
								// Set Label
								Signal::Push(Box::new(SetLabel::new(self.dev_id)))
							}
							6 => {
								// Unmark for modification
								installer.drive_config_builder.unmark_part_as_modify(self.dev_id);
								Signal::Pop
							}
							7 => {
								// Delete Partition
								installer.drive_config_builder.delete_partition(self.dev_id);
								Signal::Pop
							}
							8 => {
								// Back
								Signal::Pop
							}
							_ => Signal::Wait,
						}
					}
					PartStatus::Create => {
						match idx {
							0 => {
								// Set Mount Point
								Signal::Push(Box::new(SetMountPoint::new(self.dev_id)))
							}
							1 => {
								if let Some(child) = self.buttons.focused_child_mut() {
									child.interact();
									if let Some(value) = child.get_value() {
										let Value::Bool(checked) = value else {
											return Signal::Wait;
										};
										installer.drive_config_builder.set_part_flag(self.dev_id, "boot", checked);
									}
								}
								Signal::Wait
							}
							2 => {
								if let Some(child) = self.buttons.focused_child_mut() {
									child.interact();
									if let Some(value) = child.get_value() {
										let Value::Bool(checked) = value else {
											return Signal::Wait;
										};
										installer.drive_config_builder.set_part_flag(self.dev_id, "esp", checked);
									}
								}
								Signal::Wait
							}
							3 => {
								if let Some(child) = self.buttons.focused_child_mut() {
									child.interact();
									if let Some(value) = child.get_value() {
										let Value::Bool(checked) = value else {
											return Signal::Wait;
										};
										installer.drive_config_builder.set_part_flag(self.dev_id, "bls_boot", checked);
									}
								}
								Signal::Wait
							}
							4 => {
								// Change Filesystem
								Signal::Push(Box::new(SelectFilesystem::new(self.dev_id)))
							}
							5 => {
								// Set Label
								Signal::Push(Box::new(SetLabel::new(self.dev_id)))
							}
							6 => {
								// Delete Partition
								installer.drive_config_builder.delete_partition(self.dev_id);
								Signal::Pop
							}
							7 => {
								// Back
								Signal::Pop
							}
							_ => Signal::Wait,
						}
					}
					_ => Signal::Wait
				}
			}
			_ => Signal::Wait,
		}
	}
}

pub struct SetMountPoint {
	editor: LineEditor,
	dev_id: u64
}

impl SetMountPoint {
	pub fn new(dev_id: u64) -> Self {
		let mut editor = LineEditor::new("Mount Point", Some("Enter a mount point..."));
		editor.focus();
		Self { editor, dev_id }
	}
	fn validate_mount_point(mount_point: &str, taken: &[String]) -> Result<(), String> {
		if mount_point.is_empty() {
			return Err("Mount point cannot be empty.".to_string());
		}
		if !mount_point.starts_with('/') {
			return Err("Mount point must be an absolute path starting with '/'.".to_string());
		}
		if mount_point != "/" && mount_point.ends_with('/') {
			return Err("Mount point cannot end with '/' unless it is root '/'.".to_string());
		}
		if taken.contains(&mount_point.to_string()) {
			return Err(format!("Mount point '{}' is already taken by another partition.", mount_point));
		}
		Ok(())
	}
}

impl Page for SetMountPoint {
	fn render(&mut self, installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
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
			.constraints(
				[
					Constraint::Percentage(15),
					Constraint::Percentage(70),
					Constraint::Percentage(15),
				]
				.as_ref(),
			)
			.split(chunks[1]);

		let info_box = InfoBox::new(
			"Set Mount Point",
			styled_block(vec![
				vec![(None, "Specify the mount point for the selected partition.")],
				vec![(None, "Examples of valid mount points include:")],
				vec![(None, "- "), (Some((Color::Yellow, Modifier::BOLD)), "/")],
				vec![(None, "- "), (Some((Color::Yellow, Modifier::BOLD)), "/home")],
				vec![(None, "- "), (Some((Color::Yellow, Modifier::BOLD)), "/boot")],
				vec![(None, "Mount points must be absolute paths.")],
			])
		);
		info_box.render(f, chunks[0]);
		self.editor.render(f, hor_chunks[1]);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Enter => {
				let mount_point = self.editor.get_value().unwrap().as_str().unwrap().trim().to_string();
				let current_mount = installer
						.drive_config_builder
						.layout
						.iter()
						.find(|p| p.id == self.dev_id)
						.and_then(|p| p.mount_point.clone());

				let mut taken_mounts: Vec<String> = installer
						.drive_config_builder
						.layout
						.iter()
						.filter_map(|d| d.mount_point.clone())
						.collect();

				if let Some(current_mount) = current_mount {
						taken_mounts.retain(|mp| mp != &current_mount);
				}
				if let Err(err) = Self::validate_mount_point(&mount_point, &taken_mounts) {
					self.editor.error(&err);
					return Signal::Wait;
				}
				installer.drive_config_builder.set_part_mount_point(self.dev_id, &mount_point);
				Signal::PopCount(2)
			}
			_ => self.editor.handle_input(event)
		}
	}
}

pub struct SetLabel {
	editor: LineEditor,
	dev_id: u64
}

impl SetLabel {
	pub fn new(dev_id: u64) -> Self {
		let mut editor = LineEditor::new("Partition Label", Some("Enter a label..."));
		editor.focus();
		Self { editor, dev_id }
	}
}

impl Page for SetLabel {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
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
			.constraints(
				[
					Constraint::Percentage(15),
					Constraint::Percentage(70),
					Constraint::Percentage(15),
				]
				.as_ref()
			)
			.split(chunks[1]);

		let info_box = InfoBox::new(
			"Set Partition Label",
			styled_block(vec![
				vec![(None, "Specify a label for the selected partition.")],
				vec![(None, "Partition labels can help identify partitions in the system.")],
				vec![(None, "")],
				vec![(Some((Color::Yellow,Modifier::BOLD)), "NOTE: If possible, you should make sure that your labels are all uppercase letters.")],
				vec![(None, "Labels with lowercase letters may break certain tools, and they also cannot be used with vfat filesystems.")],
			])
		);
		info_box.render(f, chunks[0]);
		self.editor.render(f, hor_chunks[1]);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Esc => Signal::Pop,
			KeyCode::Enter => {
				let label = self.editor.get_value().unwrap().as_str().unwrap().trim().to_string();
				if label.is_empty() {
					self.editor.error("Label cannot be empty.");
					return Signal::Wait;
				}
				if label.len() > 36 {
					self.editor.error("Label cannot exceed 36 characters.");
					return Signal::Wait;
				}
				if label.contains(' ') {
					self.editor.error("Label cannot contain spaces.");
					return Signal::Wait;
				}
				installer.drive_config_builder.set_part_label(self.dev_id, &label);
				Signal::PopCount(2)
			}
			_ => self.editor.handle_input(event)
		}
	}
}
