use std::{fmt::Display, process::Command, str::FromStr, sync::atomic::AtomicU64};

use ratatui::layout::Constraint;
use serde_json::{Map, Value};

use crate::{attrset, merge_attrs, nix::{fmt_nix, nixstr}, widget::TableWidget};

static NEXT_PART_ID: AtomicU64 = AtomicU64::new(1);

/// Hash attributes into a stable id
///
/// Using this function to obtain ids for each DiskEntry
/// ensures that we can always identify an entry, even with incomplete data
/// we hash using name, start sector, size, and parent as these are generally unchanging attributes
pub fn get_entry_id() -> u64 {
	NEXT_PART_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub fn bytes_disko_cfg(bytes: u64) -> String {
	const K: f64 = 1000.0;
	const M: f64 = 1000.0 * K;
	const G: f64 = 1000.0 * M;
	const T: f64 = 1000.0 * G;

	let bytes_f = bytes as f64;
	if bytes_f >= T {
		format!("{:.0}T", bytes_f / T)
	} else if bytes_f >= G {
		format!("{:.0}G", bytes_f / G)
	} else if bytes_f >= M {
		format!("{:.0}M", bytes_f / M)
	} else if bytes_f >= K {
		format!("{:.0}K", bytes_f / K)
	} else {
		format!("{bytes}B")
	}
}

pub fn bytes_readable(bytes: u64) -> String {
	const KIB: u64 = 1 << 10;
	const MIB: u64 = 1 << 20;
	const GIB: u64 = 1 << 30;
	const TIB: u64 = 1 << 40;

	if bytes >= 1 << 40 {
		format!("{:.2} TiB", bytes as f64 / TIB as f64)
	} else if bytes >= 1 << 30 {
		format!("{:.2} GiB", bytes as f64 / GIB as f64)
	} else if bytes >= 1 << 20 {
		format!("{:.2} MiB", bytes as f64 / MIB as f64)
	} else if bytes >= 1 << 10 {
		format!("{:.2} KiB", bytes as f64 / KIB as f64)
	} else {
		bytes.to_string()
	}
}

pub fn parse_sectors(s: &str, sector_size: u64, total_sectors: u64) -> Option<u64> {
	let s = s.trim().to_lowercase();

	// Define multipliers for binary and decimal units
	let units: [(&str, f64); 10] = [
		("tib", (1u64 << 40) as f64),
		("tb",  1_000_000_000_000.0),
		("gib", (1u64 << 30) as f64),
		("gb",  1_000_000_000.0),
		("mib", (1u64 << 20) as f64),
		("mb",  1_000_000.0),
		("kib", (1u64 << 10) as f64),
		("kb",  1_000.0),
		("b",   1.0),
		("%",   0.0), // handled separately
	];

	for (unit, multiplier) in units.iter() {
		if s.ends_with(unit) {
			let num_str = s.trim_end_matches(unit).trim();

			if *unit == "%" {
				return num_str.parse::<f64>().ok()
					.map(|v| ((v / 100.0) * total_sectors as f64).round() as u64);
				} else {
					return num_str.parse::<f64>().ok()
						.map(|v| ((v * multiplier) / sector_size as f64).round() as u64);
			}
		}
	}

	// If no suffix, assume sectors directly
	s.parse::<u64>().ok()
}

pub fn mb_to_sectors(mb: u64, sector_size: u64) -> u64 {
	let bytes = mb * 1024 * 1024;
	(bytes + sector_size - 1) / sector_size // round up to nearest sector
}

/// We are going to be using the `lsblk` command to get disk information.
///
/// This is a reasonable approach for accurate data collection, and since Nix is a contender for the title of "greatest thing ever created", we can actually make the assumption that lsblk exists in this environment
/// The installer is intended to be ran using the flake like `nix run github:km-clay/nixos-wizard`, and it runs in a wrapped environment that includes `lsblk`.
/// So if lsblk is somehow not available, that is user error.
pub fn lsblk() -> anyhow::Result<Vec<Disk>> {
	let output = Command::new("lsblk")
		.args([
			"--json",
			"-o",
			"NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,LABEL,START,PHY-SEC",
			"-b"
		])
		.output()?;

	if !output.status.success() {
		return Err(anyhow::anyhow!("lsblk command failed with status: {}", output.status));
	}

	let lsblk_json: Value = serde_json::from_slice(&output.stdout)
		.map_err(|e| anyhow::anyhow!("Failed to parse lsblk output as JSON: {}", e))?;

	let blockdevices = lsblk_json.get("blockdevices")
		.and_then(|v| v.as_array())
		.ok_or_else(|| anyhow::anyhow!("lsblk output missing 'blockdevices' array"))?;
	let mut disks = Vec::new();
	for device in blockdevices {
		let dev_type = device.get("type")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Device entry missing TYPE"))?;
		if dev_type == "disk" {
			let disk = parse_disk(device.clone())?;
			disks.push(disk);
		}
	}
	Ok(disks)
}

pub fn parse_disk(disk: Value) -> anyhow::Result<Disk> {
	// disk is a JSON object with fields like NAME, SIZE, TYPE, MOUNTPOINT, FSTYPE, LABEL, START, PHY-SEC
	let obj = disk.as_object().ok_or_else(|| anyhow::anyhow!("Disk entry is not an object"))?;

	let name = obj.get("name")
		.and_then(|v| v.as_str())
		.ok_or_else(|| anyhow::anyhow!("Disk entry missing NAME"))?.to_string();

	let size = obj.get("size")
		.and_then(|v| v.as_u64())
		.ok_or_else(|| anyhow::anyhow!("Disk entry missing or invalid SIZE: {:?}", obj.clone()))?;

	let sector_size = obj.get("phy-sec")
		.and_then(|v| v.as_u64())
		.unwrap_or(512); // default to 512 if missing

	// Parse partitions
	let mut layout = Vec::new();
	if let Some(children) = obj.get("children").and_then(|v| v.as_array()) {
		for part in children {
			let partition = parse_partition(part)?;
			layout.push(partition);
		}
	}

	let mut disk = Disk::new(name, size / sector_size, sector_size, layout);
	disk.calculate_free_space(); // Ensure free space is calculated
	Ok(disk)
}

pub fn parse_partition(part: &Value) -> anyhow::Result<DiskItem> {
	let obj = part.as_object().ok_or_else(|| anyhow::anyhow!("Partition entry is not an object"))?;

	let start = obj.get("start")
		.and_then(|v| v.as_u64())
		.ok_or_else(|| anyhow::anyhow!("Partition entry missing or invalid START"))?;

	let size = obj.get("size")
		.and_then(|v| v.as_u64())
		.ok_or_else(|| anyhow::anyhow!("Partition entry missing or invalid SIZE"))?;

	let sector_size = obj.get("phy-sec")
		.and_then(|v| v.as_u64())
		.unwrap_or(512); // default to 512 if missing

	let name = obj.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
	let fs_type = obj.get("fstype").and_then(|v| v.as_str()).map(|s| s.to_string());
	let mount_point = obj.get("mountpoint").and_then(|v| v.as_str()).map(|s| s.to_string());
	let label = obj.get("label").and_then(|v| v.as_str()).map(|s| s.to_string());

	let ro = false; // lsblk does not provide read-only info directly

	let flags = vec![]; // Could be populated based on other attributes if needed

	let status = PartStatus::Exists; // Default to existing, could be modified based on other criteria

	Ok(DiskItem::Partition(Partition::new(
		start,
		size / sector_size,
		sector_size,
		status,
		name,
		fs_type,
		mount_point,
		label,
		ro,
		flags
	)))
}

pub fn disk_table(disks: &[Disk]) -> TableWidget {
	let (headers, widths): (Vec<String>, Vec<Constraint>) = DiskTableHeader::disk_table_header_info().into_iter().unzip();
	let rows: Vec<Vec<String>> = disks.iter().map(|d| d.as_table_row(&DiskTableHeader::disk_table_headers())).collect();
	TableWidget::new("Disks", widths, headers, rows)
}

pub fn part_table(disk_items: &[DiskItem], sector_size: u64) -> TableWidget {
	let (headers, widths): (Vec<String>, Vec<Constraint>) = DiskTableHeader::partition_table_header_info().into_iter().unzip();
	let rows: Vec<Vec<String>> = disk_items.iter().map(|item| item.as_table_row(sector_size, &DiskTableHeader::partition_table_headers())).collect();
	TableWidget::new("Partitions", widths, headers, rows)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Disk {
	name: String,
	size: u64, // sectors
	sector_size: u64,

	initial_layout: Vec<DiskItem>,
	/// Model of the disk's sector usage
	/// The sector spans are `half-open ranges`
	/// This means the start value is inclusive, and the end (start + size) is exclusive.
	layout: Vec<DiskItem>
}

impl Disk {
	pub fn new(
		name: String,
		size: u64,
		sector_size: u64,
		layout: Vec<DiskItem>
	) -> Self {
		let mut new = Self {
			name,
			size,
			sector_size,
			initial_layout: layout.clone(),
			layout
		};
		new.calculate_free_space();
		new
	}
	pub fn as_table_row(&self, headers: &[DiskTableHeader]) -> Vec<String> {
		headers.iter().map(|h| {
			match h {
				DiskTableHeader::Status => "".into(),
				DiskTableHeader::Device => self.name.clone(),
				DiskTableHeader::Label => "".into(),
				DiskTableHeader::Start => "".into(), // Disk does not have a start sector in this context
				DiskTableHeader::End => "".into(), // Disk does not have an end sector in this context
				DiskTableHeader::Size => bytes_readable(self.size_bytes()),
				DiskTableHeader::FSType => "".into(),
				DiskTableHeader::MountPoint => "".into(),
				DiskTableHeader::Flags => "".into(),
				DiskTableHeader::ReadOnly => "no".into(),
			}
		}).collect()
	}
	pub fn as_disko_cfg(&self) -> serde_json::Value {
		let mut partitions = serde_json::Map::new();
		for item in &self.layout {
			if let DiskItem::Partition(p) = item {
				if *p.status() == PartStatus::Delete {
					continue;
				}
				let name = p.label()
					.map(|s| s.to_string())
					.unwrap_or_else(|| format!("part{}", p.id()));

				partitions.insert(name, serde_json::json!({
					"size": bytes_disko_cfg(p.size_bytes(p.sector_size)),
					"type": p.fs_gpt_code(p.flags.contains(&"esp".to_string())),
					"format": p.disko_fs_type(),
					"mountpoint": p.mount_point(),
				}));
			}
		}

		serde_json::json!({
			"device": format!("/dev/{}", self.name),
			"type": "disk",
			"content": {
				"type": "gpt",
				"partitions": partitions
			}
		})
	}
	pub fn name(&self) -> &str {
		&self.name
	}
	pub fn set_name<S: Into<String>>(&mut self, name: S) {
		self.name = name.into();
	}
	pub fn size(&self) -> u64 {
		self.size
	}
	pub fn set_size(&mut self, size: u64) {
		self.size = size;
	}
	pub fn sector_size(&self) -> u64 {
		self.sector_size
	}
	pub fn set_sector_size(&mut self, sector_size: u64) {
		self.sector_size = sector_size;
	}
	pub fn layout(&self) -> &[DiskItem] {
		&self.layout
	}
	pub fn partitions(&self) -> impl Iterator<Item = &Partition> {
		self.layout.iter().filter_map(|item| {
			if let DiskItem::Partition(p) = item { Some(p) } else { None }
		})
	}
	pub fn partitions_mut(&mut self) -> impl Iterator<Item = &mut Partition> {
		self.layout.iter_mut().filter_map(|item| {
			if let DiskItem::Partition(p) = item { Some(p) } else { None }
		})
	}
	pub fn partition_by_id(&self, id: u64) -> Option<&Partition> {
		self.partitions().find(|p| p.id() == id)
	}
	pub fn partition_by_id_mut(&mut self, id: u64) -> Option<&mut Partition> {
		self.partitions_mut().find(|p| p.id() == id)
	}
	pub fn free_spaces(&self) -> impl Iterator<Item = (u64, u64)> {
		self.layout.iter().filter_map(|item| {
			if let DiskItem::FreeSpace { start, size, .. } = *item { Some((start, size)) } else { None }
		})
	}
	pub fn reset_layout(&mut self) {
		self.layout = self.initial_layout.clone();
		self.calculate_free_space();
	}
	pub fn size_bytes(&self) -> u64 {
		self.size * self.sector_size
	}
	pub fn remove_partition(&mut self, id: u64) -> anyhow::Result<()> {
		let Some(part_idx) = self.layout.iter().position(|item| { item.id() == id }) else {
			return Err(anyhow::anyhow!("No item with id {}", id));
		};
		let DiskItem::Partition(_) = &mut self.layout[part_idx] else {
			return Err(anyhow::anyhow!("Item with id {} is not a partition", id));
		};
		self.layout.remove(part_idx);

		self.calculate_free_space();
		Ok(())
	}
	pub fn new_partition(&mut self, part: Partition) -> anyhow::Result<()> {
		// Ensure the new partition does not overlap existing partitions
		self.clear_free_space();
		log::debug!("Adding new partition: {:#?}", part);
		log::debug!("Current layout: {:#?}", self.layout);
		let new_start = part.start();
		let new_end = part.end();
		for item in &self.layout {
			if let DiskItem::Partition(p) = item {
				if p.status == PartStatus::Delete {
					continue;
				}
				let existing_start = p.start();
				let existing_end = p.end();
				if (new_start < existing_end) && (new_end > existing_start) {
					return Err(anyhow::anyhow!("New partition overlaps with existing partition"));
				}
			}
		}
		self.layout.push(DiskItem::Partition(part));
		log::debug!("Updated layout: {:#?}", self.layout);
		self.calculate_free_space();
		log::debug!("After calculating free space: {:#?}", self.layout);
		Ok(())
	}

	pub fn clear_free_space(&mut self) {
		self.layout.retain(|item| {
			!matches!(item, DiskItem::FreeSpace { .. })
		});
		self.normalize_layout();
	}

	/// Recomputes FreeSpace entries based on current Partitions.
	pub fn calculate_free_space(&mut self) {
		// 1. Retain only partitions
		let (deleted, mut rest) = self.layout
			.iter()
			.cloned()
			.partition::<Vec<_>, _>(|item| matches!(item, DiskItem::Partition(p) if p.status == PartStatus::Delete));

		// 2. Sort partitions by start sector
		rest.sort_by_key(|p| p.start());

		let mut gaps = vec![];
		let mut cursor = 2048u64; // track the current sector

		// 3. Walk through partitions, inserting FreeSpace where gaps exist
		for p in rest.iter() {
			let DiskItem::Partition(p) = p else { continue; };
			// 1. Handle gap before this partition
			if p.start() > cursor {
				let size = p.start() - cursor;
				if size > mb_to_sectors(5, self.sector_size) {
					gaps.push(DiskItem::FreeSpace {
						id: get_entry_id(),
						start: cursor,
						size,
					});
				}
			}

			// 3. Advance cursor past the partition
			cursor = p.start() + p.size();
		}

		// 4. Check for free space at the end
		if cursor < self.size {
			let size = self.size - cursor;
			if size > mb_to_sectors(5, self.sector_size) {
				gaps.push(DiskItem::FreeSpace {
					id: get_entry_id(),
					start: cursor,
					size: self.size - cursor,
				});
			}
		}

		let mut rest_with_gaps = rest.into_iter().chain(gaps).collect::<Vec<_>>();
		rest_with_gaps.sort_by_key(|item| item.start());
		let new_layout = deleted.into_iter().chain(rest_with_gaps).collect();
		self.layout = new_layout;
		self.normalize_layout();
	}

	/// Sort the layout, and merge adjacent free space
	pub fn normalize_layout(&mut self) {
		// Now we move all of the deleted entries to the start, for visual organization
		let (mut new_layout, others): (Vec<_>, Vec<_>) = self
			.layout()
			.to_vec()
			.into_iter()
			.partition(|item| matches!(item, DiskItem::Partition(p) if p.status == PartStatus::Delete));
		let mut last_free: Option<(u64, u64)> = None; // (start, size)

		new_layout.extend(others);
		let mut new_new_layout = vec![];

		for item in &new_layout {
			match item {
				DiskItem::FreeSpace { start, size, .. } => {
					if let Some((last_start, last_size)) = last_free {
						// Merge with previous free space
						last_free = Some((last_start, last_size + size));
					} else {
						last_free = Some((*start, *size));
					}
				},
				DiskItem::Partition(p) => {
					if let Some((start, size)) = last_free.take() {
						new_new_layout.push(DiskItem::FreeSpace { id: get_entry_id(), start, size });
					}
					new_new_layout.push(DiskItem::Partition(p.clone()));
				}
			}
		}
		if let Some((start, size)) = last_free.take() {
			new_new_layout.push(DiskItem::FreeSpace { id: get_entry_id(), start, size });
		}

		self.layout = new_new_layout;
	}

	pub fn use_default_layout(&mut self) {
		// 1. Remove all free space
		// 2. Set all existing/modified partitions to deleted
		// 3. Create a boot and root partition
		self.layout.retain(|item| {
			match item {
				DiskItem::FreeSpace { .. } => false,
				DiskItem::Partition(part) => part.status != PartStatus::Create,
			}
		});
		for part in self.layout.iter_mut() {
			let DiskItem::Partition(part) = part else { continue; };
			part.status = PartStatus::Delete
		}
		let boot_part = Partition::new(
			2048,
			mb_to_sectors(500, self.sector_size),
			self.sector_size,
			PartStatus::Create,
			None,
			Some("fat32".into()),
			Some("/boot".into()),
			Some("BOOT".into()),
			false,
			vec!["boot".into(), "esp".into()]
		);
		let root_part = Partition::new(
			boot_part.end(), // start at the end of boot partition
			self.size - (boot_part.end()), // all remaining sectors
			self.sector_size,
			PartStatus::Create,
			None,
			Some("ext4".into()),
			Some("/".into()),
			Some("ROOT".into()),
			false,
			vec![]
		);
		self.layout.push(DiskItem::Partition(boot_part));
		self.layout.push(DiskItem::Partition(root_part));
	}
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum DiskItem {
	Partition(Partition),
	FreeSpace { id: u64, start: u64, size: u64 }, // size in sectors
}

impl DiskItem {
	pub fn start(&self) -> u64 {
		match self {
			DiskItem::Partition(p) => p.start,
			DiskItem::FreeSpace { start, .. } => *start,
		}
	}
	pub fn id(&self) -> u64 {
		match self {
			DiskItem::Partition(p) => p.id(),
			DiskItem::FreeSpace { id, .. } => *id,
		}
	}
	pub fn mount_point(&self) -> Option<&str> {
		match self {
			DiskItem::Partition(p) => p.mount_point(),
			DiskItem::FreeSpace { .. } => None,
		}
	}
	pub fn as_table_row(&self, sector_size: u64, headers: &[DiskTableHeader]) -> Vec<String> {
		match self {
			DiskItem::Partition(p) => {
				headers.iter().map(|h| {
					match h {
						DiskTableHeader::Status => match p.status() {
							PartStatus::Delete => "delete".into(),
							PartStatus::Modify => "modify".into(),
							PartStatus::Exists => "existing".into(),
							PartStatus::Create => "create".into(),
							PartStatus::Unknown => "unknown".into(),
						},
						DiskTableHeader::Device => p.name().unwrap_or("").into(),
						DiskTableHeader::Label => p.label().unwrap_or("").into(),
						DiskTableHeader::Start => p.start().to_string(),
						DiskTableHeader::End => (p.end() - 1).to_string(),
						DiskTableHeader::Size => bytes_readable(p.size_bytes(sector_size)),
						DiskTableHeader::FSType => p.fs_type().unwrap_or("").into(),
						DiskTableHeader::MountPoint => p.mount_point().unwrap_or("").into(),
						DiskTableHeader::Flags => p.flags().join(","),
						DiskTableHeader::ReadOnly => "".into(), // Not applicable for partitions
					}
				}).collect()
			},
			DiskItem::FreeSpace { start, size, .. } => {
				headers.iter().map(|h| {
					match h {
						DiskTableHeader::Status => "free".into(),
						DiskTableHeader::Device => "".into(),
						DiskTableHeader::Label => "".into(),
						DiskTableHeader::Start => start.to_string(),
						DiskTableHeader::End => ((start + size) - 1).to_string(),
						DiskTableHeader::Size => bytes_readable(size * sector_size),
						DiskTableHeader::FSType => "".into(),
						DiskTableHeader::MountPoint => "".into(),
						DiskTableHeader::Flags => "".into(),
						DiskTableHeader::ReadOnly => "".into(), // Not applicable for free space
					}
				}).collect()
			}
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PartStatus {
	Delete,
	Modify,
	Create,
	Exists,
	Unknown
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Partition {
	id: u64,
	start: u64, // sectors
	size: u64, // also sectors
	sector_size: u64, // bytes
	status: PartStatus,
	name: Option<String>,
	fs_type: Option<String>,
	mount_point: Option<String>,
	ro: bool,
	label: Option<String>,
	flags: Vec<String>
}

impl Partition {
	pub fn new(
		start: u64,
		size: u64,
		sector_size: u64,
		status: PartStatus,
		name: Option<String>,
		fs_type: Option<String>,
		mount_point: Option<String>,
		label: Option<String>,
		ro: bool,
		flags: Vec<String>
	) -> Self {
		Self {
			id: get_entry_id(),
			start,
			sector_size,
			size,
			status,
			name,
			fs_type,
			mount_point,
			label,
			ro,
			flags
		}
	}
	pub fn id(&self) -> u64 {
		self.id
	}
	pub fn name(&self) -> Option<&str> {
		self.name.as_deref()
	}
	pub fn set_name<S: Into<String>>(&mut self, name: S) {
		self.name = Some(name.into());
	}
	pub fn start(&self) -> u64 {
		self.start
	}
	pub fn end(&self) -> u64 {
		self.start + self.size
	}
	pub fn set_start(&mut self, start: u64) {
		self.start = start;
	}
	pub fn size(&self) -> u64 {
		self.size
	}
	pub fn set_size(&mut self, size: u64) {
		self.size = size;
	}
	pub fn status(&self) -> &PartStatus {
		&self.status
	}
	pub fn set_status(&mut self, status: PartStatus) {
		self.status = status;
	}
	pub fn fs_type(&self) -> Option<&str> {
		self.fs_type.as_deref()
	}
	pub fn disko_fs_type(&self) -> Option<&'static str> {
		match self.fs_type.as_deref()? {
			"ext4" => Some("ext4"),
			"ext3" => Some("ext3"),
			"ext2" => Some("ext2"),
			"btrfs" => Some("btrfs"),
			"xfs" => Some("xfs"),
			"fat12" => Some("vfat"),
			"fat16" => Some("vfat"),
			"fat32" => Some("vfat"),
			"ntfs" => Some("ntfs"),
			"swap" => Some("swap"),
			_ => None,
		}
	}
	pub fn fs_gpt_code(&self, is_esp: bool) -> Option<&'static str> {
		match self.fs_type.as_deref()? {
			"ext4" | "ext3" | "ext2" | "btrfs" | "xfs" => Some("8300"),
			"fat12" | "fat16" | "fat32" => {
				if is_esp { Some("EF00") } else { Some("0700") }
			}
			"ntfs" => Some("0700"),
			"swap" => Some("8200"),
			_ => None,
		}
	}
	pub fn set_fs_type<S: Into<String>>(&mut self, fs_type: S) {
		self.fs_type = Some(fs_type.into());
	}
	pub fn mount_point(&self) -> Option<&str> {
		self.mount_point.as_deref()
	}
	pub fn set_mount_point<S: Into<String>>(&mut self, mount_point: S) {
		self.mount_point = Some(mount_point.into());
	}
	pub fn label(&self) -> Option<&str> {
		self.label.as_deref()
	}
	pub fn set_label<S: Into<String>>(&mut self, label: S) {
		self.label = Some(label.into());
	}
	pub fn flags(&self) -> &[String] {
		&self.flags
	}
	pub fn add_flag<S: Into<String>>(&mut self, flag: S) {
		let flag_str = flag.into();
		if !self.flags.contains(&flag_str) {
			self.flags.push(flag_str);
		}
	}
	pub fn add_flags(&mut self, flags: impl Iterator<Item = impl Into<String>>) {
		for flag in flags {
			let flag = flag.into();
			if !self.flags.contains(&flag) {
				self.flags.push(flag);
			}
		}
	}
	pub fn remove_flag<S: AsRef<str>>(&mut self, flag: S) {
		self.flags.retain(|f| f != flag.as_ref());
	}
	pub fn remove_flags<S: AsRef<str>>(&mut self, flags: impl Iterator<Item = S>) {
		let flag_set: Vec<String> = flags.map(|f| f.as_ref().to_string()).collect();
		self.flags.retain(|f| !flag_set.contains(f));
	}
	pub fn size_bytes(&self, sector_size: u64) -> u64 {
		self.size * sector_size
	}
}

pub struct PartitionBuilder {
	start: Option<u64>,
	size: Option<u64>,
	sector_size: Option<u64>,
	status: PartStatus,
	name: Option<String>,
	fs_type: Option<String>,
	mount_point: Option<String>,
	label: Option<String>,
	ro: Option<bool>,
	flags: Vec<String>
}

impl PartitionBuilder {
	pub fn new() -> Self {
		Self {
			start: None,
			size: None,
			sector_size: None,
			status: PartStatus::Unknown,
			name: None,
			fs_type: None,
			mount_point: None,
			label: None,
			ro: None,
			flags: vec![]
		}
	}
	pub fn start(mut self, start: u64) -> Self {
		self.start = Some(start);
		self
	}
	pub fn size(mut self, size: u64) -> Self {
		self.size = Some(size);
		self
	}
	pub fn sector_size(mut self, sector_size: u64) -> Self {
		self.sector_size = Some(sector_size);
		self
	}
	pub fn status(mut self, status: PartStatus) -> Self {
		self.status = status;
		self
	}
	pub fn fs_type<S: Into<String>>(mut self, fs_type: S) -> Self {
		self.fs_type = Some(fs_type.into());
		self
	}
	pub fn mount_point<S: Into<String>>(mut self, mount_point: S) -> Self {
		self.mount_point = Some(mount_point.into());
		self
	}
	pub fn read_only(mut self, ro: bool) -> Self {
		self.ro = Some(ro);
		self
	}
	pub fn label<S: Into<String>>(mut self, label: S) -> Self {
		self.label = Some(label.into());
		self
	}
	pub fn add_flag<S: Into<String>>(mut self, flag: S) -> Self {
		let flag_str = flag.into();
		if !self.flags.contains(&flag_str) {
			self.flags.push(flag_str);
		}
		self
	}
	pub fn build(self) -> anyhow::Result<Partition> {
		let start = self.start.ok_or_else(|| anyhow::anyhow!("start is required"))?;
		let size = self.size.ok_or_else(|| anyhow::anyhow!("size is required"))?;
		let sector_size = self.sector_size.unwrap_or(512); // default to 512 if not specified
		let mount_point = self.mount_point.ok_or_else(|| anyhow::anyhow!("mount_point is required"))?;
		let ro = self.ro.unwrap_or(false);
		if size == 0 {
			return Err(anyhow::anyhow!("size must be greater than zero"));
		}
		let id = get_entry_id();
		Ok(Partition {
			id,
			start,
			size,
			sector_size,
			status: self.status,
			name: self.name,
			fs_type: self.fs_type,
			mount_point: Some(mount_point),
			label: self.label,
			ro,
			flags: self.flags
		})
	}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum DiskTableHeader {
	Status,
	Device,
	Start,
	End,
	Label,
	Size,
	FSType,
	MountPoint,
	Flags,
	ReadOnly
}

impl DiskTableHeader {
	pub fn header_info(&self) -> (String, Constraint) {
		match self {
			DiskTableHeader::Status => ("Status".into(), Constraint::Min(10)),
			DiskTableHeader::Device => ("Device".into(), Constraint::Min(11)),
			DiskTableHeader::Label => ("Label".into(), Constraint::Min(15)),
			DiskTableHeader::Start => ("Start".into(), Constraint::Min(22)),
			DiskTableHeader::End => ("End".into(), Constraint::Min(22)),
			DiskTableHeader::Size => ("Size".into(), Constraint::Min(11)),
			DiskTableHeader::FSType => ("FS Type".into(), Constraint::Min(7)),
			DiskTableHeader::MountPoint => ("Mount Point".into(), Constraint::Min(15)),
			DiskTableHeader::Flags => ("Flags".into(), Constraint::Min(20)),
			DiskTableHeader::ReadOnly => ("Read Only".into(), Constraint::Min(21)),
		}
	}
	pub fn all_headers() -> Vec<Self> {
		vec![
			DiskTableHeader::Status,
			DiskTableHeader::Device,
			DiskTableHeader::Label,
			DiskTableHeader::Start,
			DiskTableHeader::End,
			DiskTableHeader::Size,
			DiskTableHeader::FSType,
			DiskTableHeader::MountPoint,
			DiskTableHeader::Flags,
			DiskTableHeader::ReadOnly,
		]
	}
	pub fn partition_table_headers() -> Vec<Self> {
		vec![
			DiskTableHeader::Status,
			DiskTableHeader::Device,
			DiskTableHeader::Label,
			DiskTableHeader::Start,
			DiskTableHeader::End,
			DiskTableHeader::Size,
			DiskTableHeader::FSType,
			DiskTableHeader::MountPoint,
			DiskTableHeader::Flags,
		]
	}
	pub fn disk_table_headers() -> Vec<Self> {
		vec![
			DiskTableHeader::Device,
			DiskTableHeader::Size,
			DiskTableHeader::ReadOnly,
		]
	}
	pub fn disk_table_header_info() -> Vec<(String, Constraint)> {
		Self::disk_table_headers().iter().map(|h| h.header_info()).collect()
	}
	pub fn partition_table_header_info() -> Vec<(String, Constraint)> {
		Self::partition_table_headers().iter().map(|h| h.header_info()).collect()
	}
	pub fn all_header_info() -> Vec<(String, Constraint)> {
		Self::all_headers().iter().map(|h| h.header_info()).collect()
	}
}
