use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Direction, Layout, Rect}, style::{Color, Modifier}, Frame};
use serde_json::Value;

use crate::{drives::{bytes_readable, disk_table, lsblk, parse_sectors, part_table, DiskItem, PartStatus, Partition}, installer::{Installer, Page, Signal}, styled_block, ui_back, ui_close, ui_down, ui_enter, ui_up, widget::{Button, CheckBox, ConfigWidget, HelpModal, InfoBox, LineEditor, TableWidget, WidgetBox}};

const HIGHLIGHT: Option<(Color,Modifier)> = Some((Color::Yellow, Modifier::BOLD));

pub struct Drives<'a> {
	pub buttons: WidgetBox,
	pub info_box: InfoBox<'a>,
	help_modal: HelpModal<'static>,
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
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Use a best-effort default partition layout'"), (None, " will attempt to automatically partition and format your selected drive with sensible defaults. "), (None, "This is recommended for most users.")],
				vec![(None, "- "), (Some((Color::Green, Modifier::BOLD)), "'Configure partitions manually'"), (None, " will allow you to specify exactly how your drive should be partitioned and formatted. "), (None, "This is recommended for advanced users who have specific requirements.")],
				vec![(Some((Color::Red, Modifier::BOLD)), "NOTE: "), (None, "When the installer is run, "), (Some((Color::Red, Modifier::BOLD | Modifier::ITALIC)), " any and all"), (None, " data on the selected drive will be wiped. Make sure you've backed up any important data.")],
			])
		);

		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate options")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Select drive configuration method")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Return to main menu")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Choose how to configure your drive for NixOS installation:")],
			vec![(None, "• Best-effort default - Automatic partitioning (recommended)")],
			vec![(None, "• Manual configuration - Advanced users only")],
			vec![(None, "")],
			vec![(Some((Color::Red, Modifier::BOLD)), "WARNING: "), (None, "All data on the selected drive will be erased!")],
		]);
		let help_modal = HelpModal::new("Drive Configuration", help_content);
		Self { buttons: button_row, info_box, help_modal }
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

		// Render help modal on top
		self.help_modal.render(f, area);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('?') => {
				self.help_modal.toggle();
				Signal::Wait
			}
			ui_close!() if self.help_modal.visible => {
				self.help_modal.hide();
				Signal::Wait
			}
			_ if self.help_modal.visible => {
				Signal::Wait
			}
			ui_back!() => Signal::Pop,
			ui_up!() => {
				self.buttons.prev_child();
				Signal::Wait
			}
			ui_down!() => {
				self.buttons.next_child();
				Signal::Wait
			}
			ui_enter!() => {
				let Some(idx) = self.buttons.selected_child() else {
					return Signal::Wait;
				};
				let disks = match lsblk() {
					Ok(disks) => disks,
					Err(e) => return Signal::Error(anyhow::anyhow!("Failed to list block devices: {e}"))
				};
				let table = disk_table(&disks);
				installer.drives = disks;
				match idx {
					0 => {
						installer.use_auto_drive_config = true;
						Signal::Push(Box::new(SelectDrive::new(table)))
					}
					1 => {
						installer.use_auto_drive_config = false;
						Signal::Push(Box::new(SelectDrive::new(table)))
					}
					2 => Signal::Pop,
					_ => Signal::Wait,
				}
			}
			_ => Signal::Wait,
		}
	}

	fn get_help_content(&self) -> (String, Vec<ratatui::text::Line<'_>>) {
		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate options")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Select drive configuration method")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Return to main menu")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Choose how to configure your drive for NixOS installation:")],
			vec![(None, "• Best-effort default - Automatic partitioning (recommended)")],
			vec![(None, "• Manual configuration - Advanced users only")],
			vec![(None, "")],
			vec![(Some((Color::Red, Modifier::BOLD)), "WARNING: "), (None, "All data on the selected drive will be erased!")],
		]);
		("Drive Configuration".to_string(), help_content)
	}
}

pub struct SelectDrive {
	table: TableWidget,
	help_modal: HelpModal<'static>,
}

impl SelectDrive {
	pub fn new(mut table: TableWidget) -> Self {
		table.focus();
		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate drive list")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Select drive for installation")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Return to previous menu")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Select the drive you want to use for your NixOS installation.")],
			vec![(None, "The selected drive will be used for partitioning and formatting.")],
			vec![(Some((Color::Red, Modifier::BOLD)), "WARNING: "), (None, "All data on the selected drive will be erased!")],
		]);
		let help_modal = HelpModal::new("Select Drive", help_content);
		Self { table, help_modal }
	}
}

impl Page for SelectDrive {
	fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
		self.table.render(f, area);

		// Render help modal on top
		self.help_modal.render(f, area);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('?') => {
				self.help_modal.toggle();
				Signal::Wait
			}
			ui_close!() if self.help_modal.visible => {
				self.help_modal.hide();
				Signal::Wait
			}
			_ if self.help_modal.visible => {
				Signal::Wait
			}
			ui_back!() => Signal::Pop,
			ui_up!() => {
				self.table.previous_row();
				Signal::Wait
			}
			ui_down!() => {
				self.table.next_row();
				Signal::Wait
			}
			ui_enter!() => {
				if let Some(row) = self.table.selected_row() {
					let Some(disk) = installer.drives.get(row) else {
						return Signal::Error(anyhow::anyhow!("Failed to find drive info'"));
					};

					installer.drive_config = Some(disk.clone());
					if installer.use_auto_drive_config {
						Signal::Push(Box::new(SelectFilesystem::new(None)))
					} else {
						let Some(ref drive) = installer.drive_config else {
							return Signal::Error(anyhow::anyhow!("No drive config available"));
						};
						let table = part_table(drive.layout(), drive.sector_size());
						Signal::Push(Box::new(ManualPartition::new(table)))
					}
				} else {
					Signal::Wait
				}
			}
			_ => Signal::Wait,
		}
	}

	fn get_help_content(&self) -> (String, Vec<ratatui::text::Line<'_>>) {
		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate drive list")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Select drive for installation")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Return to previous menu")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Select the drive you want to use for your NixOS installation.")],
			vec![(None, "The selected drive will be used for partitioning and formatting.")],
			vec![(Some((Color::Red, Modifier::BOLD)), "WARNING: "), (None, "All data on the selected drive will be erased!")],
		]);
		("Select Drive".to_string(), help_content)
	}
}

pub struct SelectFilesystem {
	pub buttons: WidgetBox,
	pub dev_id: Option<u64>,
	help_modal: HelpModal<'static>,
}

impl SelectFilesystem {
	pub fn new(dev_id: Option<u64>) -> Self {
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
		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate filesystem options")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Select filesystem type")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Return to previous menu")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Choose the filesystem type for your partition.")],
			vec![(None, "Different filesystems have different features and performance")],
			vec![(None, "characteristics. ext4 is recommended for most users.")],
		]);
		let help_modal = HelpModal::new("Select Filesystem", help_content);
		Self { buttons: button_row, dev_id, help_modal }
	}
	pub fn get_fs_info<'a>(idx: usize) -> InfoBox<'a> {
		match idx {
			0 => InfoBox::new(
				"ext4",
				styled_block(vec![
					vec![
						(HIGHLIGHT, "ext4"),
						(None, " is a"),
						(HIGHLIGHT, " widely used and stable filesystem"),
						(None, " known for its "),
						(HIGHLIGHT, "reliability and performance.")
					],
					vec![
						(None, "It supports "),
						(HIGHLIGHT, "journaling"),
						(None, ", which helps "),
						(HIGHLIGHT, "protect against data corruption "),
						(None, "in case of crashes.")
					],
					vec![
						(None, "It's a good choice for"),
						(HIGHLIGHT, " general-purpose"),
						(None, " use and is"),
						(HIGHLIGHT, " well-supported across various Linux distributions.")
					],
				])
			),
			1 => InfoBox::new(
				"ext3",
				styled_block(vec![
					vec![
						(HIGHLIGHT, "ext3"),
						(None, " is an older journaling filesystem that builds upon ext2."),
					],
					vec![
						(None, "It provides "),
						(HIGHLIGHT, "journaling"),
						(None, " capabilities to improve data integrity and recovery after crashes."),
					],
					vec![
						(None, "While it is "),
						(HIGHLIGHT, "reliable and stable"),
						(None, ", it lacks some of the performance and features of ext4."),
					],
				])
			),
			2 => InfoBox::new(
				"ext2",
				styled_block(vec![
					vec![
						(HIGHLIGHT, "ext2"),
						(None, " is a non-journaling filesystem that is simple and efficient."),
					],
					vec![
						(None, "It is suitable for use cases where "),
						(HIGHLIGHT, "journaling is not required"),
						(None, ", such as "),
						(HIGHLIGHT, "flash drives"),
						(None, " or "),
						(HIGHLIGHT, "small partitions"),
						(None, "."),
					],
					vec![
						(None, "However, it is more "),
						(HIGHLIGHT, "prone to data corruption "),
						(None, "in case of crashes compared to ext3 and ext4."),
					],
				])
			),
			3 => InfoBox::new(
				"btrfs",
				styled_block(vec![
					vec![
						(HIGHLIGHT, "btrfs"),
						(None, " ("),
						(Some((Color::Reset, Modifier::ITALIC)), "B-tree filesystem"),
						(None, ") is a "),
						(HIGHLIGHT, "modern filesystem"),
						(None, " that offers advanced features like "),
						(HIGHLIGHT, "snapshots"),
						(None, ", "),
						(HIGHLIGHT, "subvolumes"),
						(None, ", and "),
						(HIGHLIGHT, "built-in RAID support"),
						(None, "."),
					],
					vec![
						(None, "It is designed for "),
						(HIGHLIGHT, "scalability"),
						(None, " and "),
						(HIGHLIGHT, "flexibility"),
						(None, ", making it suitable for systems that require "),
						(HIGHLIGHT, "complex storage solutions."),
					],
					vec![
						(None, "However, it may not be as mature as "),
						(HIGHLIGHT, "ext4"),
						(None, " in terms of "),
						(HIGHLIGHT, "stability"),
						(None, " for all use cases."),
					],
				])
			),
			4 => InfoBox::new(
				"xfs",
				styled_block(vec![
					vec![
						(HIGHLIGHT, "XFS"),
						(None, " is a "),
						(HIGHLIGHT, "high-performance journaling filesystem"),
						(None, " that excels in handling "),
						(HIGHLIGHT, "large files"),
						(None, " and "),
						(HIGHLIGHT, "high I/O workloads"),
						(None, "."),
					],
					vec![
						(None, "It is known for its "),
						(HIGHLIGHT, "scalability"),
						(None, " and "),
						(HIGHLIGHT, "robustness"),
						(None, ", making it a popular choice for "),
						(HIGHLIGHT, "enterprise environments"),
						(None, "."),
					],
					vec![
						(HIGHLIGHT, "XFS"),
						(None, " is particularly well-suited for systems that require efficient handling of "),
						(HIGHLIGHT, "large datasets"),
						(None, "."),
					],
				])
			),
			5 => InfoBox::new(
				"fat12",
				styled_block(vec![
					vec![
						(HIGHLIGHT, "FAT12"),
						(None, " is a "),
						(HIGHLIGHT, "simple "),
						(None, "and "),
						(HIGHLIGHT, "widely supported "),
						(None, "filesystem primarily used for "),
						(HIGHLIGHT, "small storage devices"),
						(None, " like floppy disks."),
					],
					vec![
						(None, "It has "),
						(HIGHLIGHT, "limitations "),
						(None, "in terms of "),
						(HIGHLIGHT, "maximum partition size "),
						(None, "and file size, making it "),
						(HIGHLIGHT, "less suitable for modern systems"),
						(None, "."),
					],
				])
			),
			6 => InfoBox::new(
				"fat16",
				styled_block(vec![
					vec![
						(HIGHLIGHT, "FAT16"),
						(None, " is an older filesystem that "),
						(HIGHLIGHT, "extends FAT12"),
						(None, " to support "),
						(HIGHLIGHT, "larger partitions and files."),
					],
					vec![
						(None, "It is still used in some "),
						(HIGHLIGHT, "embedded systems "),
						(None, "and "),
						(HIGHLIGHT, "older devices "),
						(None, "but has "),
						(HIGHLIGHT, "limitations compared to more modern filesystems"), (None, "."),
					],
				])
			),
			7 => InfoBox::new(
				"fat32",
				styled_block(vec![
					vec![
					(HIGHLIGHT, "FAT32"),
					(None, " is a widely supported filesystem that can handle"),
					(HIGHLIGHT, " larger partitions and files than FAT16"), (None, "."),
					],
					vec![
					(None, "It is commonly used for USB drives and memory cards due to its broad "),
					(HIGHLIGHT, "cross-platform compatibility"),
					(None, "."),
					],
					vec![
					(None, "FAT32 is also commonly used for "),
					(HIGHLIGHT, "EFI System Partitions (ESP)"),
					(None, " on UEFI systems, allowing the firmware to load the bootloader."),
					],
					vec![
					(None, "However, it has limitations such as a "),
					(HIGHLIGHT, "maximum file size of 4GB"),
					(None, " and"),
					(HIGHLIGHT, " lack of modern journaling features."),
					],
				])
			),
			8 => InfoBox::new(
				"ntfs",
				styled_block(vec![
					vec![
						(HIGHLIGHT, "NTFS"),
						(None, " is a"),
						(HIGHLIGHT, " robust"),
						(None, " and"),
						(HIGHLIGHT, " feature-rich"),
						(None, " filesystem developed by Microsoft."),
					],
					vec![
						(None, "It supports "),
						(HIGHLIGHT, "large files"),
						(None, ", "),
						(HIGHLIGHT, "advanced permissions"),
						(None, ", "),
						(HIGHLIGHT, "encryption"),
						(None, ", and "),
						(HIGHLIGHT, "journaling"),
						(None, "."),
					],
					vec![
						(None, "While it is"),
						(HIGHLIGHT, " primarily used in Windows environments"),
						(None, ", Linux has good support for NTFS through the "),
						(HIGHLIGHT, "ntfs-3g"),
						(None, " driver."),
					],
					vec![
						(None, "NTFS is a good choice if you need to "),
						(HIGHLIGHT, "share data between Windows and Linux systems "),
						(None, "or if you require features like "),
						(HIGHLIGHT, "file compression and encryption"),
						(None, "."),
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

		// Render help modal on top
		self.help_modal.render(f, area);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('?') => {
				self.help_modal.toggle();
				Signal::Wait
			}
			ui_close!() if self.help_modal.visible => {
				self.help_modal.hide();
				Signal::Wait
			}
			_ if self.help_modal.visible => {
				Signal::Wait
			}
			ui_back!() => Signal::Pop,
			ui_up!() => {
				self.buttons.prev_child();
				Signal::Wait
			}
			ui_down!() => {
				self.buttons.next_child();
				Signal::Wait
			}
			ui_enter!() => {
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
					if let Some(config) = installer.drive_config.as_mut() {
						config.use_default_layout(Some(fs));
					}
					installer.make_drive_config_display();
					return Signal::PopCount(3);
				} else {
					let Some(config) = installer.drive_config.as_mut() else {
						return Signal::Error(anyhow::anyhow!("No drive config available"));
					};
					let Some(id) = self.dev_id else {
						return Signal::Error(anyhow::anyhow!("No device id specified for filesystem selection"));
					};
					let Some(partition) = config.partition_by_id_mut(id) else {
						return Signal::Error(anyhow::anyhow!("No partition found with id {:?}", id));
					};
					partition.set_fs_type(&fs);
				}

				Signal::PopCount(2)
			}
			_ => Signal::Wait,
		}
	}

	fn get_help_content(&self) -> (String, Vec<ratatui::text::Line<'_>>) {
		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate filesystem options")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Select filesystem type")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Return to previous menu")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Choose the filesystem type for your partition.")],
			vec![(None, "Different filesystems have different features and performance")],
			vec![(None, "characteristics. ext4 is recommended for most users.")],
		]);
		("Select Filesystem".to_string(), help_content)
	}
}

pub struct ManualPartition {
	disk_config: TableWidget,
	buttons: WidgetBox,
	confirming_reset: bool,
	help_modal: HelpModal<'static>,
}

impl ManualPartition {
	pub fn new(mut disk_config: TableWidget) -> Self {
		let buttons = vec![
			Box::new(Button::new("Suggest Partition Layout")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("Confirm and Exit")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("Reset Partition Layout")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("Abort")) as Box<dyn ConfigWidget>,
		];
		let buttons = WidgetBox::button_menu(buttons);
		disk_config.focus();
		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate partitions and buttons")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Tab"), (None, " - Switch between partition table and buttons")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Select partition or button action")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Return to previous menu")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Manually configure drive partitions. Select partitions to")],
			vec![(None, "modify them or select free space to create new partitions.")],
			vec![(None, "Use buttons at bottom for additional actions.")],
		]);
		let help_modal = HelpModal::new("Manual Partitioning", help_content);
		Self { disk_config, buttons, confirming_reset: false, help_modal }
	}
}

impl Page for ManualPartition {
	fn render(&mut self, installer: &mut Installer, f: &mut Frame, area: Rect) {
		let Some(ref config) = installer.drive_config else {
			log::error!("No drive config available for manual partitioning");
			return;
		};
		let rows = part_table(config.layout(), config.sector_size()).rows().to_vec();
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

		// Render help modal on top
		self.help_modal.render(f, area);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('?') => {
				self.help_modal.toggle();
				return Signal::Wait;
			}
			ui_close!() if self.help_modal.visible => {
				self.help_modal.hide();
				return Signal::Wait;
			}
			_ if self.help_modal.visible => {
				return Signal::Wait;
			}
			_ => {}
		}

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
				ui_back!() => Signal::PopCount(2),
				ui_up!() => {
					if !self.disk_config.previous_row() {
						self.disk_config.unfocus();
						self.buttons.last_child();
						self.buttons.focus();
					}
					Signal::Wait
				}
				ui_down!() => {
					if !self.disk_config.next_row() {
						self.disk_config.unfocus();
						self.buttons.first_child();
						self.buttons.focus();
					}
					Signal::Wait
				}
				KeyCode::Enter => {
					log::debug!("Disk config is focused, handling row selection");
					// we have now selected a row in the table
					// now we need to figure out if we are editing a partition or creating one
					let Some(row) = self.disk_config.get_selected_row_info() else {
						return Signal::Error(anyhow::anyhow!("No row selected in disk config table"));
					};
					let Some(start) = row.get_field("start").and_then(|s| s.parse::<u64>().ok()) else {
						return Signal::Error(anyhow::anyhow!("Failed to parse start sector from row: {:?}", row));
					};
					let Some(ref drive) = installer.drive_config else {
						return Signal::Error(anyhow::anyhow!("No drive config available"));
					};
					let layout = drive.layout();
					let Some(item) = layout.iter().rfind(|i| i.start() == start) else {
						return Signal::Error(anyhow::anyhow!("No partition or free space found at start sector {}", start));
					};
					log::debug!("Selected item: {item:?}");
					match item {
						DiskItem::Partition(part) => {
							Signal::Push(Box::new(AlterPartition::new(part.clone())))
						}
						DiskItem::FreeSpace { id, start, size } => {
							Signal::Push(Box::new(NewPartition::new(*id, *start, drive.sector_size(), *size)))
						}
					}
				}
				_ => Signal::Wait,
			}
		} else if self.buttons.is_focused() {
			match event.code {
				ui_back!() => Signal::PopCount(2),
				ui_up!() => {
					if !self.buttons.prev_child() {
						self.buttons.unfocus();
						self.disk_config.last_row();
						self.disk_config.focus();
					}
					Signal::Wait
				}
				ui_down!() => {
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
							installer.make_drive_config_display();
							return Signal::Unwind;
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
								let Some(ref mut device) = installer.drive_config else {
									return Signal::Wait;
								};
								device.reset_layout();
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

	fn get_help_content(&self) -> (String, Vec<ratatui::text::Line<'_>>) {
		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate partitions and buttons")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Tab"), (None, " - Switch between partition table and buttons")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Select partition or button action")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Return to previous menu")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Manually configure drive partitions. Select partitions to")],
			vec![(None, "modify them or select free space to create new partitions.")],
			vec![(None, "Use buttons at bottom for additional actions.")],
		]);
		("Manual Partitioning".to_string(), help_content)
	}
}

pub struct SuggestPartition {
	buttons: WidgetBox,
	help_modal: HelpModal<'static>,
}

impl SuggestPartition {
	pub fn new() -> Self {
		let buttons = vec![
			Box::new(Button::new("Yes")) as Box<dyn ConfigWidget>,
			Box::new(Button::new("No")) as Box<dyn ConfigWidget>,
		];
		let mut button_row = WidgetBox::button_menu(buttons);
		button_row.focus();
		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate yes/no options")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Confirm selection")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Cancel and return")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Confirm whether to use a suggested partition layout.")],
			vec![(None, "This will create a standard boot and root partition setup.")],
			vec![(Some((Color::Red, Modifier::BOLD)), "WARNING: "), (None, "All existing data will be erased!")],
		]);
		let help_modal = HelpModal::new("Suggest Partition Layout", help_content);
		Self { buttons: button_row, help_modal }
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
					(HIGHLIGHT, "suggested partition layout "),
					(None, "for your selected drive?")
				],
				vec![
					(None, "This will create a standard layout with a "),
					(HIGHLIGHT, "boot partition "),
					(None, "and a "),
					(HIGHLIGHT, "root partition."),
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

		// Render help modal on top
		self.help_modal.render(f, area);
	}
	fn handle_input(&mut self, installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			KeyCode::Char('?') => {
				self.help_modal.toggle();
				Signal::Wait
			}
			ui_close!() if self.help_modal.visible => {
				self.help_modal.hide();
				Signal::Wait
			}
			_ if self.help_modal.visible => {
				Signal::Wait
			}
			ui_back!() => Signal::Pop,
			ui_up!() => {
				self.buttons.prev_child();
				Signal::Wait
			}
			ui_down!() => {
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
						if let Some(ref mut config) = installer.drive_config {
							config.use_default_layout(Some("ext4".into()));
						} else {
							return Signal::Error(anyhow::anyhow!("No drive config available for suggested partition layout"));
						}
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

	fn get_help_content(&self) -> (String, Vec<ratatui::text::Line<'_>>) {
		let help_content = styled_block(vec![
			vec![(Some((Color::Yellow, Modifier::BOLD)), "↑/↓, j/k"), (None, " - Navigate yes/no options")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Enter"), (None, " - Confirm selection")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "Esc"), (None, " - Cancel and return")],
			vec![(Some((Color::Yellow, Modifier::BOLD)), "?"), (None, " - Show this help")],
			vec![(None, "")],
			vec![(None, "Confirm whether to use a suggested partition layout.")],
			vec![(None, "This will create a standard boot and root partition setup.")],
			vec![(Some((Color::Red, Modifier::BOLD)), "WARNING: "), (None, "All existing data will be erased!")],
		]);
		("Suggest Partition Layout".to_string(), help_content)
	}
}

pub struct NewPartition {
	pub fs_id: u64,
	pub part_start: u64,
	pub part_end: u64,
	pub sector_size: u64,
	pub total_size: u64, // sectors

	pub new_part_size: Option<u64>, // sectors
	pub size_input: LineEditor,

	pub new_part_fs: Option<String>,
	pub fs_buttons: WidgetBox,

	pub new_part_mount_point: Option<String>,
	pub mount_input: LineEditor,
}

impl NewPartition {
	pub fn new(fs_id: u64, part_start: u64, sector_size: u64, total_size: u64) -> Self {
		let bytes = total_size * sector_size;
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
		let mut size_input = LineEditor::new("New Partition Size", Some("Empty input uses rest of free space"));
		size_input.focus();
		Self {
			fs_id,
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
	pub fn total_size_bytes(&self) -> u64 {
		self.total_size * self.sector_size
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
				vec![(HIGHLIGHT, "Sector Size: "), (None, &format!("{}", self.sector_size))],
				vec![(HIGHLIGHT, "Partition Start Sector: "), (None, &format!("{}", self.part_start))],
				vec![(HIGHLIGHT, "Partition End Sector: "), (None, &format!("{}", self.part_end))],
				vec![(HIGHLIGHT, "Total Free Space: "), (None, &bytes_readable(self.total_size_bytes()))],
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
				let mut input = input.as_str().unwrap().trim(); // TODO: handle these unwraps
				if input.is_empty() {
					input = "100%";
				}
				let Some(ref device) = installer.drive_config else {
					return Signal::Error(anyhow::anyhow!("No drive config available for new partition size input"));
				};
				match parse_sectors(input, device.sector_size(), self.total_size) {
					Some(size) => {
						self.new_part_size = Some(size);
						self.size_input.unfocus();
						self.fs_buttons.focus();
						Signal::Wait
					}
					None => {
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
	pub fn handle_input_fs_select(&mut self, _installer: &mut Installer, event: KeyEvent) -> Signal {
		match event.code {
			ui_back!() => Signal::Pop,
			ui_up!() => {
				self.fs_buttons.prev_child();
				Signal::Wait
			}
			ui_down!() => {
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
				let Some(ref mut device) = installer.drive_config else {
					return Signal::Error(anyhow::anyhow!("No drive config available for new partition mount point input"));
				};
				let taken_mounts: Vec<String> = device
					.layout()
					.iter()
					.filter(|di| {
						let DiskItem::Partition(p) = di else { return true };
						let PartStatus::Delete = *p.status() else { return true };
						false // The partition is deleted, so we filter it out
					})
					.filter_map(|d| d.mount_point().map(|s| s.to_string()))
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
				let Some(size) = self.new_part_size else {
					return Signal::Error(anyhow::anyhow!("No new partition size specified when finalizing new partition"));
				};

				let new_part = Partition::new(
					self.part_start,
					size,
					self.sector_size,
					PartStatus::Create,
					None,
					self.new_part_fs.clone(),
					self.new_part_mount_point.clone(),
					None,
					false,
					flags
				);
				if let Err(e) = device.new_partition(new_part) {
					return Signal::Error(anyhow::anyhow!("Failed to create new partition: {}", e));
				};

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
	pub part_id: u64,
	pub part_status: PartStatus,
}

impl AlterPartition {
	pub fn new(part: Partition) -> Self {
		let part_status = part.status();
		let buttons = Self::buttons_by_status(*part_status, part.flags());
		let mut button_row = WidgetBox::button_menu(buttons);
		button_row.focus();
		Self { buttons: button_row, part_id: part.id(), part_status: *part_status }
	}
	pub fn buttons_by_status(status: PartStatus, flags: &[String]) -> Vec<Box<dyn ConfigWidget>> {
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
			ui_back!() => Signal::Pop,
			ui_up!() => {
				self.buttons.prev_child();
				Signal::Wait
			}
			ui_down!() => {
				self.buttons.next_child();
				Signal::Wait
			}
			ui_enter!() => {
				if self.part_status == PartStatus::Delete {
					return Signal::Pop
				}
				let Some(idx) = self.buttons.selected_child() else {
					return Signal::Wait;
				};
				let Some(ref mut device) = installer.drive_config else {
					return Signal::Error(anyhow::anyhow!("No drive config available for altering partition"));
				};
				match self.part_status {
					PartStatus::Exists => {
						let Some(part) = device.partition_by_id_mut(self.part_id) else {
							return Signal::Error(anyhow::anyhow!("No partition found with id {}", self.part_id));
						};
						match idx {
							0 => {
								// Set Mount Point
								Signal::Push(Box::new(SetMountPoint::new(self.part_id)))
							}
							1 => {
								// Mark For Modification
								part.set_status(PartStatus::Modify);
								Signal::Pop
							}
							2 => {
								// Delete Partition
								part.set_status(PartStatus::Delete);
								device.calculate_free_space();
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
								Signal::Push(Box::new(SetMountPoint::new(self.part_id)))
							}
							1 => {
								if let Some(child) = self.buttons.focused_child_mut() {
									child.interact();
									if let Some(value) = child.get_value() {
										let Value::Bool(checked) = value else {
											return Signal::Wait;
										};
										if let Some(part) = device.partition_by_id_mut(self.part_id) {
											if checked {
												part.add_flag("boot");
											} else {
												part.remove_flag("boot");
											}
										}
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
										if let Some(part) = device.partition_by_id_mut(self.part_id) {
											if checked {
												part.add_flag("esp");
											} else {
												part.remove_flag("esp");
											}
										}
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
										if let Some(part) = device.partition_by_id_mut(self.part_id) {
											if checked {
												part.add_flag("bls_boot");
											} else {
												part.remove_flag("bls_boot");
											}
										}
									}
								}
								Signal::Wait
							}
							4 => {
								// Change Filesystem
								Signal::Push(Box::new(SelectFilesystem::new(Some(self.part_id))))
							}
							5 => {
								// Set Label
								Signal::Push(Box::new(SetLabel::new(self.part_id)))
							}
							6 => {
								// Unmark for modification
								if let Some(part) = device.partition_by_id_mut(self.part_id) {
									part.set_status(PartStatus::Exists);
								}
								Signal::Pop
							}
							7 => {
								// Delete Partition
								if let Some(part) = device.partition_by_id_mut(self.part_id) {
									part.set_status(PartStatus::Delete);
								}
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
								Signal::Push(Box::new(SetMountPoint::new(self.part_id)))
							}
							1 => {
								if let Some(child) = self.buttons.focused_child_mut() {
									child.interact();
									if let Some(value) = child.get_value() {
										let Value::Bool(checked) = value else {
											return Signal::Wait;
										};
										if let Some(part) = device.partition_by_id_mut(self.part_id) {
											if checked {
												part.add_flag("boot");
											} else {
												part.remove_flag("boot");
											}
										}
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
										if let Some(part) = device.partition_by_id_mut(self.part_id) {
											if checked {
												part.add_flag("esp");
											} else {
												part.remove_flag("esp");
											}
										}
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
										if let Some(part) = device.partition_by_id_mut(self.part_id) {
											if checked {
												part.add_flag("bls_boot");
											} else {
												part.remove_flag("bls_boot");
											}
										}
									}
								}
								Signal::Wait
							}
							4 => {
								// Change Filesystem
								Signal::Push(Box::new(SelectFilesystem::new(Some(self.part_id))))
							}
							5 => {
								// Set Label
								Signal::Push(Box::new(SetLabel::new(self.part_id)))
							}
							6 => {
								// Delete Partition
								if let Some(part) = device.partition_by_id_mut(self.part_id) {
									part.set_status(PartStatus::Delete);
								}
								if let Err(e) = device.remove_partition(self.part_id) {
									return Signal::Error(anyhow::anyhow!("{}", e));
								};
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
			return Err(format!("Mount point '{mount_point}' is already taken by another partition."));
		}
		Ok(())
	}
}

impl Page for SetMountPoint {
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
				.as_ref(),
			)
			.split(chunks[1]);

		let info_box = InfoBox::new(
			"Set Mount Point",
			styled_block(vec![
				vec![(None, "Specify the mount point for the selected partition.")],
				vec![(None, "Examples of valid mount points include:")],
				vec![(None, "- "), (HIGHLIGHT, "/")],
				vec![(None, "- "), (HIGHLIGHT, "/home")],
				vec![(None, "- "), (HIGHLIGHT, "/boot")],
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
				let Some(device) = installer.drive_config.as_mut() else {
					return Signal::Error(anyhow::anyhow!("No drive config available for setting mount point"));
				};
				let current_mount = device
						.partitions()
						.find(|p| p.id() == self.dev_id)
						.and_then(|p| p.mount_point());

				let mut taken_mounts: Vec<String> = device
					.partitions()
					.filter_map(|d| d.mount_point().map(|mp| mp.to_string()))
					.collect();

				if let Some(current_mount) = current_mount {
					taken_mounts.retain(|mp| mp != current_mount);
				}
				if let Err(err) = Self::validate_mount_point(&mount_point, &taken_mounts) {
					self.editor.error(&err);
					return Signal::Wait;
				}

				if let Some(part) = device.partition_by_id_mut(self.dev_id) {
					part.set_mount_point(&mount_point);
				}
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
				vec![(HIGHLIGHT, "NOTE: If possible, you should make sure that your labels are all uppercase letters.")],
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
				let Some(drive_config) = installer.drive_config.as_mut() else {
					return Signal::Error(anyhow::anyhow!("No drive config available for setting partition label"));
				};
				let Some(part) = drive_config.partition_by_id_mut(self.dev_id) else {
					return Signal::Error(anyhow::anyhow!("No partition found with id {}", self.dev_id));
				};

				part.set_label(&label);
				Signal::PopCount(2)
			}
			_ => self.editor.handle_input(event)
		}
	}
}
