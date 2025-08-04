use std::{fmt::Display, process::Command, str::FromStr};

use ratatui::layout::Constraint;
use serde_json::{Map, Value};

use crate::{attrset, merge_attrs, nix::{fmt_nix, nixstr}, widget::TableWidget};

/// Hash attributes into a stable id
///
/// Using this function to obtain ids for each DiskEntry
/// ensures that we can always identify an entry, even with incomplete data
/// we hash using name, start sector, size, and parent as these are generally unchanging attributes
pub fn get_entry_id(name: String, start: u64, size: u64, parent: Option<DiskEntry>) -> u64 {
	use std::hash::{Hash, Hasher};
	use std::collections::hash_map::DefaultHasher;

	let mut hasher = DefaultHasher::new();
	name.hash(&mut hasher);
	start.hash(&mut hasher);
	size.hash(&mut hasher);
	if let Some(parent) = parent {
		parent.device.hash(&mut hasher);
	}
	hasher.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskSize {
	Literal(u64),
	Percentage(u8)
}

impl DiskSize {
	pub fn as_bytes(&self, max_size: DiskSize) -> u64 {
		let DiskSize::Literal(max_size) = max_size else {
			log::error!("max_size must be a literal size in bytes");
			panic!()
		};
		match self {
			DiskSize::Literal(b) => *b,
			DiskSize::Percentage(p) => ((max_size as f64) * (*p as f64) / 100.0).round() as u64,
		}
	}
	pub fn as_sectors(&self, sector_size: usize) -> usize {
		let bytes = self.as_bytes(DiskSize::Literal(u64::MAX));
		(bytes as usize).div_ceil(sector_size) // round up
	}
}

impl FromStr for DiskSize {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let s = s.trim();

		// Handle percentage like "100%" or "25 %"
		if let Some(percent_part) = s.strip_suffix('%') {
			let percent = percent_part.trim().parse::<u8>()
				.map_err(|e| e.to_string())?;

			if percent > 100 {
				return Err("Percentage cannot exceed 100%".into());
			}
			return Ok(DiskSize::Percentage(percent));
		}

		// Otherwise treat it as a size (e.g., 200MiB)
		let mut suffix = String::new();
		let mut num_part = String::new();
		let chars = s.chars().peekable();

		for ch in chars {
			match ch {
				_ if ch.is_alphabetic() => {
					suffix.push(ch);
				}
				'.' => {
					num_part.push(ch);
				}
				_ if ch.is_ascii_digit() => {
					num_part.push(ch);
				}
				_ => return Err(format!("Invalid character '{ch}' in disk size '{s}'")),
			}
		}


		let num: f64 = num_part.trim().parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;

		let multiplier = match suffix.trim().to_lowercase().as_str() {
			"" | "b"      => 1,
			"k" | "kb"    => 1_000,
			"ki" | "kib"  => 1 << 10,
			"m" | "mb"    => 1_000_000,
			"mi" | "mib"  => 1 << 20,
			"g" | "gb"    => 1_000_000_000,
			"gi" | "gib"  => 1 << 30,
			"t" | "tb"    => 1_000_000_000_000i64,
			"ti" | "tib"  => 1 << 40,
			_ => return Err(format!("Unrecognized suffix '{suffix}'")),
		};

		Ok(DiskSize::Literal((num * multiplier as f64).round() as u64))
	}
}

impl std::fmt::Display for DiskSize {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			DiskSize::Percentage(p) => write!(f, "{p}%"),
			DiskSize::Literal(bytes) => {
				let mut size = *bytes as f64;
				let units = ["B", "KB", "MB", "GB", "TB", "PB"];
				let mut unit = "B";

				for next_unit in &units[1..] {
					if size < 1000.0 {
						break;
					}
					size /= 1000.0;
					unit = next_unit;
				}

				if unit == "B" {
					write!(f, "{}{}", size as u64, unit)
				} else {
					write!(f, "{size:.2}{unit}")
				}
			}
		}
	}
}

#[derive(Clone,Debug)]
pub struct DiskTable(Vec<DiskEntry>);

impl DiskTable {
	pub fn new(entries: Vec<DiskEntry>) -> Self {
		Self(entries)
	}
	pub fn entries(&self) -> &[DiskEntry] {
		&self.0
	}
	pub fn rows(&self) -> Vec<Vec<String>> {
		let table = self.as_widget(Some(DiskTableHeader::all_headers()));
		table.rows().to_vec()
	}
	pub fn empty() -> Self {
		Self(vec![])
	}
	pub fn filter_by<F>(&self, f: F) -> Self
	where
		F: Fn(&DiskEntry) -> bool,
	{
		let filtered = self.0.iter().filter(|&x| f(x)).cloned().collect();
		Self(filtered)
	}
	pub fn find_by<F>(&self, f: F) -> Option<DiskEntry>
	where
		F: Fn(&DiskEntry) -> bool,
	{
		self.0.iter().find(|&x| f(x)).cloned()
	}
	pub fn from_lsblk() -> anyhow::Result<Self> {
		let Ok(output) = Command::new("lsblk")
			.args(["--json", "-o", "NAME,MODEL,TYPE,RO,SIZE,PHY-SEC,MOUNTPOINT,FSTYPE,LABEL,START", "-b"])
			.output() else {
				return Err(anyhow::anyhow!("Failed to run lsblk command"));
			};
		if !output.status.success() {
			return Err(anyhow::anyhow!("lsblk command failed with status: {}", output.status));
		}
		let stdout = String::from_utf8_lossy(&output.stdout);
		let value: Value = serde_json::from_str(&stdout).map_err(|e| anyhow::anyhow!("Failed to parse lsblk JSON output: {}", e))?;
		let table = Self::from_json(value);
		let table_with_free_space = table.with_free_space();
		Ok(table_with_free_space)
	}
	fn consolidate_free_space(&mut self) {
		// If any free space entries appear adjacent to one another, they should merge into a single entry
		// Additionally, this merging can cross over entries with the "delete" status
		self.0.sort_by_key(|e| e.start);
		let mut consolidated = vec![];
		let mut entries = std::mem::take(&mut self.0).into_iter();
		let mut current_free = None;

		for entry in entries {
			match entry.entry_type {
					EntryType::Disk => {
						if let Some(free) = current_free.take() {
							consolidated.push(free);
						}
						consolidated.push(entry);
					}
					EntryType::FreeSpace => {
						if let Some(mut free) = current_free.take() {
							// Merge with existing free space
							let sector_size = free.sector_size.unwrap_or(DiskSize::Literal(512)).as_bytes(free.size) as usize;
							let free_end = free.start + free.size.as_sectors(sector_size);
							let entry_start = entry.start;
							if entry_start <= free_end {
								// They overlap or are adjacent
								let entry_size_sectors = entry.size.as_sectors(sector_size);
								let new_end = std::cmp::max(free_end, entry.start + entry_size_sectors);
								let new_size_sectors = new_end - free.start;
								free.size = DiskSize::Literal((new_size_sectors * sector_size) as u64);
								current_free = Some(free);
							} else {
								// Not adjacent, push the old one and start a new one
								consolidated.push(free);
								current_free = Some(entry);
							}
						} else {
							current_free = Some(entry);
						}
					}
					EntryType::Partition => {
						match entry.status {
							PartStatus::Delete => {
								consolidated.push(entry.clone());
								continue
							}
							_ => {
								if let Some(free) = current_free.take() {
									consolidated.push(free);
								}
							}
						}
						consolidated.push(entry.clone());
					}
			}
		}
		if let Some(free) = current_free.take() {
			consolidated.push(free);
		}
		self.0 = consolidated;
	}
	pub fn with_free_space(mut self) -> Self {
		self
	}
	pub fn as_widget(&self, headers: Option<impl IntoIterator<Item = DiskTableHeader>>) -> TableWidget {
		if let Some(headers) = headers {
			let headers = headers.into_iter().collect::<Vec<_>>();
			let mut header_names = vec![];
			let mut header_constraints = vec![];
			for header in &headers {
				let (name, constraint) = header.header_info();
				header_names.push(name);
				header_constraints.push(constraint);
			}
			let mut rows = vec![];
			for entry in &self.0 {
				let mut row = vec![];
				for header in &headers {
					let cell = match header {
						DiskTableHeader::Status => entry.status.to_string(),
						DiskTableHeader::Device => entry.device.clone(),
						DiskTableHeader::Label => entry.label.clone().unwrap_or_else(|| "-".to_string()),
						DiskTableHeader::Type => match entry.entry_type {
							EntryType::Disk => "disk".into(),
							EntryType::Partition => "partition".into(),
							EntryType::FreeSpace => "free".into(),
						},
						DiskTableHeader::Size => entry.size.to_string(),
						DiskTableHeader::FSType => entry.fs_type.clone().unwrap_or_else(|| "-".into()),
						DiskTableHeader::MountPoint => entry.mount_point.clone().unwrap_or_else(|| "-".into()),
						DiskTableHeader::Flags => if entry.flags.is_empty() {
							"-".into()
						} else {
							entry.flags.join(",")
						},
							DiskTableHeader::ReadOnly => if entry.read_only {
								"yes".into()
							} else {
								"no".into()
							},
					};
					row.push(cell);
				}
				// We sneak this in right after the last cell is pushed
				// It isn't rendered in the table, and just acts as metadata for each row
				row.push(entry.id.to_string());

				rows.push(row);
			}

			TableWidget::new("Disks", header_constraints, header_names, rows)
		} else {
			let headers = DiskTableHeader::all_header_info();
			let header_names: Vec<String> = headers.iter().map(|(name, _)| name.clone()).collect();
			let header_constraints: Vec<Constraint> = headers.iter().map(|(_, constraint)| *constraint).collect();
			let mut rows = vec![];
			for entry in &self.0 {
				let row = vec![
					entry.status.to_string(),
					entry.device.clone(),
					entry.label.clone().unwrap_or_else(|| "-".to_string()),
					match entry.entry_type {
						EntryType::Disk => "disk".to_string(),
						EntryType::Partition => "partition".to_string(),
						EntryType::FreeSpace => "free".to_string(),
					},
					entry.size.to_string(),
					entry.fs_type.clone().unwrap_or_else(|| "-".to_string()),
					entry.mount_point.clone().unwrap_or_else(|| "-".to_string()),
					if entry.flags.is_empty() { "-".to_string() } else { entry.flags.join(",") },
					if entry.read_only { "yes".to_string() } else { "no".to_string() },
					entry.id.to_string()
				];
				rows.push(row);
			}
			TableWidget::new("Disks", header_constraints, header_names, rows)
		}
	}
	pub fn from_json(value: Value) -> Self {
		let mut entries = vec![];
		let Value::Object(map) = value else {
			log::error!("Expected JSON object for lsblk output");
			return Self(entries);
		};

		let Some(blockdevices) = map.get("blockdevices").and_then(|v| v.as_array()) else {
			log::error!("Expected 'blockdevices' array in lsblk output");
			return Self(entries);
		};

		for device in blockdevices {
			if let Value::Object(dev_map) = device {
				Self::flatten_blockdevices(dev_map, &mut entries, None);
			} else {
				log::warn!("Expected blockdevice to be an object, got: {device:?}");
			}
		}

		Self(entries)
	}
	pub fn flatten_blockdevices(value: &Map<String,Value>, rows: &mut Vec<DiskEntry>, parent: Option<String>) {
		if value.is_empty() {
			return
		}
		let entry = DiskEntry::from_lsblk_entry(value, parent.clone());
		rows.push(entry.clone());
		if let Some(children) = value.get("children").and_then(|v| v.as_array()) {
			for child in children {
				if let Value::Object(child_map) = child {
					Self::flatten_blockdevices(child_map, rows, Some(entry.device.clone()));
				}
			}
		}
	}
	pub fn default_layout(parent: &str, fs: String) -> Self {
		let boot_size: DiskSize = "512MiB".parse().expect("Failed to parse boot partition size");
		let root_size: DiskSize = "100%".parse().expect("Failed to parse root partition size");

		let root_sector_start = (512 * 1024 * 1024) / 512;
		let boot_sector_start = 2048u64;

		let name = "-".to_string();
		let boot_hash_size = boot_size.as_bytes(DiskSize::Literal(u64::MAX));
		let root_hash_size = root_size.as_bytes(DiskSize::Literal(u64::MAX));
		let entries = vec![
			DiskEntry {
				id: get_entry_id(name.clone(),boot_hash_size,boot_sector_start,None),
				device: "-".to_string(), // placeholder, resolved at apply time
				status: PartStatus::Create,
				label: Some("BOOT".into()),
				entry_type: EntryType::Partition,
				size: boot_size,
				start: 2048, // typically starts at sector 2048
				fs_type: Some("vfat".into()),
				mount_point: Some("/boot".into()),
				flags: vec!["boot".into(), "esp".into()],
				sector_size: Some(DiskSize::Literal(512)),
				parent: Some(parent.to_string()),
				read_only: false,
			},
			DiskEntry {
				id: get_entry_id(name,root_hash_size,root_sector_start as u64,None),
				device: "-".to_string(),
				status: PartStatus::Create,
				label: Some("ROOT".into()),
				entry_type: EntryType::Partition,
				size: root_size,
				start: root_sector_start,
				fs_type: Some(fs),
				mount_point: Some("/".into()),
				flags: vec![],
				sector_size: Some(DiskSize::Literal(512)),
				parent: Some(parent.to_string()),
				read_only: false,
			},
		];

		Self(entries)
	}
}

#[derive(Clone,Debug,PartialEq)]
pub struct DiskEntry {
	pub id: u64,
	pub status: PartStatus,
	pub device: String,
	pub label: Option<String>,
	pub entry_type: EntryType,
	pub size: DiskSize,
	pub start: usize,
	pub fs_type: Option<String>,
	pub mount_point: Option<String>,
	pub flags: Vec<String>,
	pub sector_size: Option<DiskSize>,
	pub parent: Option<String>,
	pub read_only: bool,
}

impl DiskEntry {
	pub fn free(start: u64, size: u64, sector_size: u64, disk: &DiskEntry) -> Self {
		Self {
			id: get_entry_id("-".to_string(), start, size, Some(disk.clone())),
			status: PartStatus::Create,
			device: "-".to_string(),
			label: None,
			entry_type: EntryType::FreeSpace,
			size: DiskSize::Literal(size),
			start: start as usize,
			fs_type: None,
			mount_point: None,
			flags: vec![],
			sector_size: Some(DiskSize::Literal(sector_size)),
			parent: Some(disk.device.clone()),
			read_only: true,
		}
	}
	pub fn from_lsblk_entry(map: &serde_json::Map<String, Value>, parent: Option<String>) -> Self {
		let device = map.get("name").and_then(|v| v.as_str()).unwrap_or("-").to_string();
		let size = map.get("size").and_then(|v| v.as_u64()).map(DiskSize::Literal).unwrap_or(DiskSize::Literal(0));
		let entry_type = match map.get("type").and_then(|v| v.as_str()) {
			Some("disk") => EntryType::Disk,
			Some("part") => EntryType::Partition,
			Some("rom") => EntryType::Disk, // treat rom as disk for now
			Some("loop") => EntryType::Disk, // treat loop as disk for now
			Some("lvm") => EntryType::Disk, // treat lvm as disk for now
			Some("raid") => EntryType::Disk, // treat raid as disk for now
			Some("crypt") => EntryType::Disk, // treat crypt as disk for now
			Some("free") => EntryType::FreeSpace,
			_ => EntryType::Disk,
		};
		let fs_type = map.get("fstype").and_then(|v| v.as_str()).map(|s| s.to_string());
		let mount_point = map.get("mountpoint").and_then(|v| v.as_str()).map(|s| s.to_string());
		let label = map.get("label").and_then(|v| v.as_str()).map(|s| s.to_string());
		let start = map.get("start").and_then(|v| v.as_u64()).map(|s| s as usize).unwrap_or(0);
		let sector_size = map.get("phy-sec").and_then(|v| v.as_u64()).map(DiskSize::Literal);
		let read_only = map.get("ro").and_then(|v| v.as_bool()).unwrap_or(false);
		let status = PartStatus::Exists; // lsblk does not provide status directly; assume existing

		let flags = vec![]; // lsblk does not provide mount options directly

		Self {
			id: get_entry_id(device.clone(), start as u64, size.as_bytes(DiskSize::Literal(u64::MAX)), None),
			status,
			device,
			label,
			entry_type,
			size,
			start,
			fs_type,
			mount_point,
			flags,
			sector_size,
			parent,
			read_only,
		}
	}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum DiskTableHeader {
	Status,
	Device,
	Label,
	Type,
	Size,
	FSType,
	MountPoint,
	Flags,
	ReadOnly
}

impl DiskTableHeader {
	pub fn header_info(&self) -> (String, Constraint) {
		match self {
			DiskTableHeader::Status => ("Status".into(), Constraint::Percentage(7)),
			DiskTableHeader::Device => ("Device".into(), Constraint::Percentage(17)),
			DiskTableHeader::Label => ("Label".into(), Constraint::Percentage(6)),
			DiskTableHeader::Type => ("Type".into(), Constraint::Percentage(10)),
			DiskTableHeader::Size => ("Size".into(), Constraint::Percentage(10)),
			DiskTableHeader::FSType => ("FS Type".into(), Constraint::Percentage(10)),
			DiskTableHeader::MountPoint => ("Mount Point".into(), Constraint::Percentage(10)),
			DiskTableHeader::Flags => ("Flags".into(), Constraint::Percentage(10)),
			DiskTableHeader::ReadOnly => ("Read Only".into(), Constraint::Percentage(10)),
		}
	}
	pub fn all_headers() -> Vec<Self> {
		vec![
			DiskTableHeader::Status,
			DiskTableHeader::Device,
			DiskTableHeader::Label,
			DiskTableHeader::Type,
			DiskTableHeader::Size,
			DiskTableHeader::FSType,
			DiskTableHeader::MountPoint,
			DiskTableHeader::Flags,
			DiskTableHeader::ReadOnly,
		]
	}
	pub fn all_header_info() -> Vec<(String, Constraint)> {
		Self::all_headers().iter().map(|h| h.header_info()).collect()
	}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EntryType {
	Disk,
	Partition,
	FreeSpace
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PartStatus {
	Exists, // Existing partition, no changes
	Modify, // Existing partition, will be modified (e.g. resized, reformatted)
	Delete, // Existing partition, will be deleted
	Create, // New partition
	Unknown // Unknown status
}

impl Display for PartStatus {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			PartStatus::Exists => write!(f, "existing"),
			PartStatus::Modify => write!(f, "modify"),
			PartStatus::Delete => write!(f, "delete"),
			PartStatus::Create => write!(f, "create"),
			PartStatus::Unknown => write!(f, "unknown"),
		}
	}
}

#[derive(Clone,Debug)]
pub struct DiskPlanBuilder {
	pub device: Option<DiskEntry>,
	pub part_table: Option<String>,
	pub fs: Option<String>,
	pub layout: Vec<DiskEntry>,
}

impl Default for DiskPlanBuilder {
	fn default() -> Self {
		Self::new()
	}
}

impl DiskPlanBuilder {
	pub fn new() -> Self {
		Self {
			device: None,
			part_table: None,
			fs: None,
			layout: vec![],
		}
	}
	pub fn manual_config_table(&mut self) -> anyhow::Result<DiskTable> {
		if self.device.is_none() {
			return Err(anyhow::anyhow!("Device must be set to generate manual config table"));
		};
		let mut table_rows = vec![self.device.clone().unwrap()];
		let mut old_rows = self.layout.clone();
		table_rows.extend(old_rows);
		let mut disk_table = DiskTable::new(table_rows).with_free_space().filter_by(|d| matches!(d.entry_type, EntryType::Partition | EntryType::FreeSpace));
		disk_table.consolidate_free_space();
		self.layout = disk_table.entries().to_vec();
		Ok(disk_table)
	}
	pub fn set_part_flag(&mut self, part_id: u64, flag: &str, enabled: bool) {
		if let Some(idx) = self.pos_by_id(part_id) {
			if enabled {
				if !self.layout[idx].flags.contains(&flag.to_string()) {
					self.layout[idx].flags.push(flag.to_string());
				}
			} else {
				self.layout[idx].flags.retain(|f| f != flag);
			}
		}
	}
	pub fn insert_new_entry(&mut self, new_entry: DiskEntry) {
		// We have to find the first entry that has a 'start' field greater than new_entry.start
		// And insert new_entry before that entry
		let pos = self.layout.iter().position(|e| e.start > new_entry.start);
		match pos {
			Some(idx) => self.layout.insert(idx, new_entry),
			None => self.layout.push(new_entry),
		}
	}
	pub fn set_part_mount_point(&mut self, part_id: u64, mount_point: &str) {
		if let Some(idx) = self.pos_by_id(part_id) {
			self.layout[idx].mount_point = Some(mount_point.to_string());
			let flags = [ "boot".to_string(), "esp".to_string() ];
			if mount_point == "/boot" || mount_point == "/boot/" {
				for flag in flags {
					if !self.layout[idx].flags.contains(&flag) {
						self.layout[idx].flags.push(flag);
					}
				}
			} else {
				for flag in flags {
					self.layout[idx].flags.retain(|f| f != &flag);
				}
			}
		}
	}
	pub fn find_by_id(&self, part_id: u64) -> Option<&DiskEntry> {
		self.layout.iter().find(|p| p.id == part_id)
	}
	pub fn find_by_id_mut(&mut self, part_id: u64) -> Option<&mut DiskEntry> {
		self.layout.iter_mut().find(|p| p.id == part_id)
	}
	pub fn pos_by_id(&self, part_id: u64) -> Option<usize> {
		self.layout.iter().position(|p| p.id == part_id)
	}
	pub fn set_part_label(&mut self, part_id: u64, label: &str) {
		if let Some(idx) = self.pos_by_id(part_id) {
			self.layout[idx].label = Some(label.to_string());
		}
	}
	pub fn set_part_fs_type(&mut self, part_id: u64, fs_type: &str) {
		if let Some(idx) = self.pos_by_id(part_id) {
			self.layout[idx].fs_type = Some(fs_type.to_string());
		}
	}
	pub fn mark_part_as_modify(&mut self, part_id: u64) {
		if let Some(idx) = self.pos_by_id(part_id) {
			if self.layout.get(idx).unwrap().status == PartStatus::Exists {
				self.layout[idx].status = PartStatus::Modify;
			}
		}
	}
	pub fn unmark_part_as_modify(&mut self, part_id: u64) {
		// It's not enough to just change the status flag
		// We need to completely roll back any changes that have occurred
		let Some(part_orig_state) = DiskTable::from_lsblk().unwrap()
			.find_by(|d| d.id == part_id) else { return };
		if let Some(idx) = self.pos_by_id(part_id) {
			self.layout[idx] = part_orig_state;
		}
	}
	pub fn delete_partition(&mut self, part_id: u64) {
		if let Some(idx) = self.pos_by_id(part_id) {
			self.layout[idx].mount_point = None;

			self.layout[idx].status = PartStatus::Create;
			self.layout[idx].device = "-".to_string();
			self.layout[idx].fs_type = None;
			self.layout[idx].flags.clear();
			self.layout[idx].label = None;
			self.layout[idx].entry_type = EntryType::FreeSpace;
			self.layout[idx].read_only = true;
		}
	}
	pub fn device(mut self, device: DiskEntry) -> Self {
		self.device = Some(device);
		self
	}
	pub fn set_default_layout(&mut self, fs: String) {
		let DiskTable(layout) = DiskTable::default_layout(&self.device.clone().as_ref().unwrap().device, fs);
		self.layout = layout;
	}
	pub fn set_device(&mut self, device: DiskEntry) -> &mut Self {
		self.device = Some(device);
		self
	}
	pub fn part_table(mut self, part_table: &str) -> Self {
		self.part_table = Some(part_table.to_string());
		self
	}
	pub fn set_part_table(&mut self, part_table: &str) -> &mut Self {
		self.part_table = Some(part_table.to_string());
		self
	}
	pub fn fs(mut self, fs: &str) -> Self {
		self.fs = Some(fs.to_string());
		self
	}
	pub fn set_fs(&mut self, fs: &str) -> &mut Self {
		self.fs = Some(fs.to_string());
		self
	}
	pub fn layout(mut self, layout: Vec<DiskEntry>) -> Self {
		self.layout = layout;
		self
	}
	pub fn set_layout(&mut self, layout: Vec<DiskEntry>) -> &mut Self {
		self.layout = layout;
		self
	}
	pub fn push_partition(mut self, partition: DiskEntry) -> Self {
		self.layout.push(partition);
		self
	}
	pub fn clear_layout(mut self) -> Self {
		self.layout.clear();
		self
	}
	pub fn build_default(mut self) -> anyhow::Result<DiskPlan> {
		let device = self.device.ok_or_else(|| anyhow::anyhow!("device is required for auto plan"))?;
		let part_table = self.part_table.ok_or_else(|| anyhow::anyhow!("part_table is required for auto plan"))?;
		let fs = self.fs.ok_or_else(|| anyhow::anyhow!("fs is required for auto plan"))?;
		for part in self.layout.iter_mut() {
			if part.entry_type == EntryType::Partition {
				part.status = PartStatus::Delete;
			}
		}
		let default = DiskTable::default_layout(&device.device, fs);
		let default_entries = default.entries();
		self.layout.extend(default_entries.iter().cloned());
		let layout = self.layout.clone();
		Ok(DiskPlan { device, part_table, layout })
	}
	pub fn build_manual(self) -> anyhow::Result<DiskPlan> {
		let device = self.device.ok_or_else(|| anyhow::anyhow!("device is required for manual plan"))?;
		let part_table = self.part_table.ok_or_else(|| anyhow::anyhow!("part_table is required for manual plan"))?;
		let layout = self.layout;
		Ok(DiskPlan { device, part_table, layout })
	}
}


#[derive(Debug)]
pub struct DiskPlan {
	pub device: DiskEntry,
	pub part_table: String,
	pub layout: Vec<DiskEntry>
}

impl DiskPlan {
	pub fn into_disko_config(&self) -> anyhow::Result<String> {
		let raw = self.fmt_disk();
		println!("Generated disko config:\n{raw}");
		fmt_nix(raw)
	}
	pub fn as_table(&self) -> anyhow::Result<TableWidget> {
		let mut table_rows = vec![self.device.clone()];
		table_rows.extend(self.layout.clone());
		let disk_table = DiskTable::new(table_rows);
		Ok(disk_table.as_widget(Some(DiskTableHeader::all_headers())))
	}
	fn fmt_disk(&self) -> String {
		let disk_attrs = attrset! {
			type = nixstr("disk");
			device = nixstr(&self.device.device);
			content = self.fmt_content();
		};
		attrset! { disk = disk_attrs; }
	}
	fn fmt_content(&self) -> String {
		attrset! {
			type = nixstr(&self.part_table);
			partitions = self.fmt_partitions();
		}
	}
	fn fmt_partitions(&self) -> String {
		// We unfortunately cannot use attrset! to easily generate this part, since the partition names are dynamic, and there are a varying number of partitions
		// so we have to build this one by hand here
		let mut attrset = "{ ".to_string();
		for part in &self.layout {
			let name = part.device.clone();
			let mut part_attrs = attrset! {
				size = nixstr(part.size);
				content = attrset! {
					type = nixstr("gpt");
					format = nixstr(part.fs_type.clone().unwrap());
					mountpoint = nixstr(part.mount_point.clone().unwrap());
				};
			};
			if !part.flags.is_empty() {
				let mut flags = String::new();
				flags.push_str("[ ");
				for flag in &part.flags {
					flags.push_str(&format!("\"{flag}\" "));
				}
				flags.push(']');
				let flag_attrset = attrset! {
					flags = flags;
				};
				part_attrs = merge_attrs!(part_attrs, flag_attrset);
			}
			if let Some(label) = &part.label {
				let label_attrset = attrset! {
					label = nixstr(label);
				};
				part_attrs = merge_attrs!(part_attrs, label_attrset);
			}
			let attr = format!("{name} = {part_attrs};");
			attrset.push_str(&attr);
		}
		attrset.push_str(" }");
		attrset
	}
}
