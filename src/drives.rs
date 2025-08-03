use std::{fmt::Display, process::Command, str::FromStr};

use ratatui::layout::Constraint;
use serde_json::{Map, Value};

use crate::{attrset, merge_attrs, nix::{fmt_nix, nixstr}, widget::TableWidget};

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
		(bytes as usize + sector_size - 1) / sector_size // round up
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
				_ => return Err(format!("Invalid character '{}' in disk size '{}'", ch, s)),
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
			_ => return Err(format!("Unrecognized suffix '{}'", suffix)),
		};

		Ok(DiskSize::Literal((num * multiplier as f64).round() as u64))
	}
}

impl std::fmt::Display for DiskSize {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			DiskSize::Percentage(p) => write!(f, "{}%", p),
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
					write!(f, "{:.2}{}", size, unit)
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
		Ok(Self::from_json(value).with_free_space())
	}
	pub fn with_free_space(mut self) -> Self {
		let mut free_spaces = vec![];
		log::debug!("Calculating free space for disks...");

		let disks: Vec<_> = self.0.iter()
			.filter(|e| e.entry_type == EntryType::Disk)
			.cloned()
			.collect();

		for disk in disks {
			let sector_size = disk.sector_size.unwrap_or(DiskSize::Literal(512)).as_bytes(disk.size) as usize;

			// Get children of this disk
			let mut children: Vec<_> = self.0.iter()
				.filter(|e| e.parent.as_deref() == Some(&disk.device) && e.entry_type == EntryType::Partition)
				.cloned()
				.collect();

			children.sort_by_key(|e| e.start);

			let mut cursor = 2048; // assume GPT starts at sector 2048

			for part in &children {
				log::debug!("Checking partition {:?} on disk {:?}", part.device, disk.device);
				if part.start > cursor {
					let gap_size = (part.start - cursor) * sector_size;
					if gap_size >= 10 * 1024 * 1024 { // only add if >10MiB
						free_spaces.push(DiskEntry {
							status: PartStatus::Exists,
							device: "-".to_string(),
							label: None,
							entry_type: EntryType::FreeSpace,
							size: DiskSize::Literal(gap_size as u64),
							start: cursor,
							fs_type: None,
							mount_point: None,
							flags: vec![],
							sector_size: Some(DiskSize::Literal(sector_size as u64)),
							parent: Some(disk.device.clone()),
							read_only: true,
						});
					}
				} else if part.status == PartStatus::Delete {
					// If the partition is marked for deletion, treat its space as free
					let gap_size = part.size.as_sectors(sector_size) * sector_size;
					free_spaces.push(DiskEntry {
						status: PartStatus::Create,
						device: "-".to_string(),
						label: None,
						entry_type: EntryType::FreeSpace,
						size: DiskSize::Literal(gap_size as u64),
						start: part.start,
						fs_type: None,
						mount_point: None,
						flags: vec![],
						sector_size: Some(DiskSize::Literal(sector_size as u64)),
						parent: Some(disk.device.clone()),
						read_only: true,
					});
				}
				cursor = part.start + part.size.as_sectors(sector_size);
			}

			// Add free space at end
			let disk_end = disk.size.as_sectors(sector_size);
			if cursor < disk_end {
				let gap_size = (disk_end - cursor) * sector_size;
				if gap_size >= 10 * 1024 * 1024 { // only add if >10MiB
					free_spaces.push(DiskEntry {
						status: PartStatus::Exists,
						device: "-".to_string(),
						label: None,
						entry_type: EntryType::FreeSpace,
						size: DiskSize::Literal(gap_size as u64),
						start: cursor,
						fs_type: None,
						mount_point: None,
						flags: vec![],
						sector_size: Some(DiskSize::Literal(sector_size as u64)),
						parent: Some(disk.device.clone()),
						read_only: true,
					});
				}
			}
		}

		self.0.extend(free_spaces);
		self.0.sort_by_key(|e| e.start);
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
				log::warn!("Expected blockdevice to be an object, got: {:?}", device);
			}
		}

		Self(entries)
	}
	pub fn flatten_blockdevices(value: &Map<String,Value>, rows: &mut Vec<DiskEntry>, parent: Option<String>) {
		if value.is_empty() {
			return
		}
		let entry = DiskEntry::from_lsblk_entry(&value, parent.clone());
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
		let boot_size = "512MiB".parse().expect("Failed to parse boot partition size");
		let root_size = "100%".parse().expect("Failed to parse root partition size");

		let root_sector_start = (512 * 1024 * 1024) / 512;

		let entries = vec![
			DiskEntry {
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

#[derive(Clone,Debug)]
pub struct DiskEntry {
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
	pub fn from_lsblk_entry(map: &serde_json::Map<String, Value>, parent: Option<String>) -> Self {
		let device = map.get("name").and_then(|v| v.as_str()).unwrap_or("-").to_string();
		let size = map.get("size").and_then(|v| v.as_u64()).map(|s| DiskSize::Literal(s)).unwrap_or(DiskSize::Literal(0));
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
		let sector_size = map.get("phy-sec").and_then(|v| v.as_u64()).map(|s| DiskSize::Literal(s));
		let read_only = map.get("ro").and_then(|v| v.as_bool()).unwrap_or(false);
		let status = PartStatus::Exists; // lsblk does not provide status directly; assume existing

		let flags = vec![]; // lsblk does not provide mount options directly

		Self {
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
pub struct DiskPlanIRBuilder {
	pub device: Option<DiskEntry>,
	pub part_table: Option<String>,
	pub fs: Option<String>,
	pub layout: Vec<DiskEntry>,
}

impl Default for DiskPlanIRBuilder {
	fn default() -> Self {
		Self::new()
	}
}

impl DiskPlanIRBuilder {
	pub fn new() -> Self {
		Self {
			device: None,
			part_table: None,
			fs: None,
			layout: vec![],
		}
	}
	pub fn manual_config_table(&self) -> anyhow::Result<DiskTable> {
		if self.device.is_none() {
			return Err(anyhow::anyhow!("Device must be set to generate manual config table"));
		};
		let mut table_rows = vec![self.device.clone().unwrap()];
		table_rows.extend(self.layout.clone());
		let disk_table = DiskTable::new(table_rows).with_free_space().filter_by(|d| matches!(d.entry_type, EntryType::Partition | EntryType::FreeSpace));
		Ok(disk_table)
	}
	pub fn set_part_mount_point(&mut self, part: &str, mount_point: &str) {
		if let Some(idx) = self.layout.iter().position(|p| p.device == part) {
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
	pub fn mark_part_as_modify(&mut self, part: &str) {
		if let Some(idx) = self.layout.iter().position(|p| p.device == part) {
			if self.layout.get(idx).unwrap().status == PartStatus::Exists {
				self.layout[idx].status = PartStatus::Modify;
			}
		}
	}
	pub fn delete_partition(&mut self, part: &str) {
		if let Some(idx) = self.layout.iter().position(|p| p.device == part) {
			let soft_delete = self.layout.get(idx).unwrap().status == PartStatus::Exists;
			if soft_delete {
				self.layout[idx].status = PartStatus::Delete;
			} else {
				self.layout.remove(idx);
			}
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
	pub fn build_auto(self) -> anyhow::Result<DiskPlanIR> {
		let device = self.device.ok_or_else(|| anyhow::anyhow!("device is required for auto plan"))?;
		let part_table = self.part_table.ok_or_else(|| anyhow::anyhow!("part_table is required for auto plan"))?;
		let fs = self.fs.ok_or_else(|| anyhow::anyhow!("fs is required for auto plan"))?;
		Ok(DiskPlanIR::Auto { device, part_table, fs })
	}
	pub fn build_manual(self) -> anyhow::Result<DiskPlanIR> {
		let device = self.device.ok_or_else(|| anyhow::anyhow!("device is required for manual plan"))?;
		let part_table = self.part_table.ok_or_else(|| anyhow::anyhow!("part_table is required for manual plan"))?;
		let layout = self.layout;
		Ok(DiskPlanIR::Manual { device, part_table, layout })
	}
}

#[derive(Debug)]
pub enum DiskPlanIR {
	Auto {
		device: DiskEntry,
		part_table: String,
		fs: String
	},
	Manual {
		device: DiskEntry,
		part_table: String,
		layout: Vec<DiskEntry>
	},
}

impl From<DiskPlanIR> for DiskPlan {
	fn from(val: DiskPlanIR) -> Self {
	  		match val {
			DiskPlanIR::Auto { device, part_table, fs } => {
				let DiskTable(layout) = DiskTable::default_layout(&device.device, fs);
				DiskPlan { device, part_table, layout }
			}
			DiskPlanIR::Manual { device, part_table, layout } => DiskPlan { device, part_table, layout },
		}
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
					format = nixstr(&part.fs_type.clone().unwrap());
					mountpoint = nixstr(&part.mount_point.clone().unwrap());
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
