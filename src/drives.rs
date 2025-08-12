use std::{process::Command, sync::atomic::AtomicU64};

use ratatui::layout::Constraint;
use serde_json::Value;

use crate::widget::TableWidget;

static NEXT_PART_ID: AtomicU64 = AtomicU64::new(1);

pub fn get_entry_id() -> u64 {
  NEXT_PART_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Convert 'bytes used' into a Disko-compatible size string
///
/// Disko (NixOS disk partitioning tool) expects sizes in specific formats:
/// - Exact byte counts like "1024B", "500M", "50G"
/// - "100%" for remaining space
///
/// If we're near the end of available space, return "100%" to avoid
/// Disko calculation errors due to rounding or alignment issues
pub fn bytes_disko_cfg(
  bytes: u64,
  total_used_sectors: u64,
  sector_size: u64,
  total_size: u64,
) -> String {
  let requested_sectors = bytes.div_ceil(sector_size);
  // Check if this partition would use most/all remaining space
  // Reserve 2048 sectors (1MB at 512 bytes/sector) for disk alignment
  let is_rest_of_space =
    (requested_sectors + total_used_sectors) >= (total_size.saturating_sub(2048));
  if is_rest_of_space {
    log::debug!(
      "bytes_disko_cfg: using 100% for bytes {bytes}, total_used_sectors {total_used_sectors}, sector_size {sector_size}, total_size {total_size}"
    );
    return "100%".into();
  }
  // Use decimal units (powers of 1000) as expected by Disko
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

/// Simple byte size formatter
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

/// Parse human-readable size strings into sector counts
/// Supports various formats: "50 MiB", "500MB", "25%", "1024B"
/// Returns the equivalent number of sectors for the given sector size
pub fn parse_sectors(s: &str, sector_size: u64, total_sectors: u64) -> Option<u64> {
  let s = s.trim().to_lowercase();

  // Define multipliers for both binary (1024-based) and decimal (1000-based) units
  let units: [(&str, f64); 10] = [
    ("tib", (1u64 << 40) as f64),  // 2^40 bytes (binary terabyte)
    ("tb", 1_000_000_000_000.0),   // 10^12 bytes (decimal terabyte)
    ("gib", (1u64 << 30) as f64),  // 2^30 bytes (binary gigabyte)
    ("gb", 1_000_000_000.0),       // 10^9 bytes (decimal gigabyte)
    ("mib", (1u64 << 20) as f64),  // 2^20 bytes (binary megabyte)
    ("mb", 1_000_000.0),           // 10^6 bytes (decimal megabyte)
    ("kib", (1u64 << 10) as f64),  // 2^10 bytes (binary kilobyte)
    ("kb", 1_000.0),               // 10^3 bytes (decimal kilobyte)
    ("b", 1.0),                    // bytes
    ("%", 0.0),                    // percentage (handled separately)
  ];

  for (unit, multiplier) in units.iter() {
    if s.ends_with(unit) {
      let num_str = s.trim_end_matches(unit).trim();

      if *unit == "%" {
        // Convert percentage to sectors (e.g., "50%" = half of total_sectors)
        return num_str
          .parse::<f64>()
          .ok()
          .map(|v| ((v / 100.0) * total_sectors as f64).round() as u64);
      } else {
        // Convert bytes to sectors by dividing by sector size
        return num_str
          .parse::<f64>()
          .ok()
          .map(|v| ((v * multiplier) / sector_size as f64).round() as u64);
      }
    }
  }

  // If no unit suffix found, interpret as raw sector count
  s.parse::<u64>().ok()
}

/// Convert number of megabytes into sectors
pub fn mb_to_sectors(mb: u64, sector_size: u64) -> u64 {
  let bytes = mb * 1024 * 1024;
  bytes.div_ceil(sector_size) // round up to nearest sector
}

/// Discover available disk drives using the `lsblk` command
///
/// This function safely identifies disk drives that can be used for installation:
/// - Uses `lsblk` to get comprehensive disk information in JSON format
/// - Filters out the drive hosting the current live system (mounted at "/" or "/iso")
/// - Returns structured disk information suitable for partitioning
///
/// The installer assumes `lsblk` is available (provided by the Nix environment)
pub fn lsblk() -> anyhow::Result<Vec<Disk>> {
  /// Check if a device is safe to use for installation
  /// 
  /// A device is considered unsafe if it or any of its partitions
  /// are currently being used by the live system
  fn is_safe_device(dev: &serde_json::Value) -> bool {
    // Check if this device is mounted at critical mount points
    if let Some(mount) = dev.get("mountpoint").and_then(|m| m.as_str()) {
      if mount == "/" || mount == "/iso" {
        // "/" is the root filesystem, "/iso" is common in live environments
        return false;
      }
    }

    // Recursively check all child partitions
    if let Some(children) = dev.get("children").and_then(|c| c.as_array()) {
      for child in children {
        if !is_safe_device(child) {
          return false;
        }
      }
    }

    true
  }
  // Execute lsblk with specific options:
  // --json: JSON output format
  // -o: specify columns (name, size, type, mount, filesystem, label, start, physical sector size)
  // -b: output sizes in bytes (not human-readable)
  let output = Command::new("lsblk")
    .args([
      "--json",
      "-o",
      "NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,LABEL,START,PHY-SEC",
      "-b",
    ])
    .output()?;

  if !output.status.success() {
    return Err(anyhow::anyhow!(
      "lsblk command failed with status: {}",
      output.status
    ));
  }

  let lsblk_json: Value = serde_json::from_slice(&output.stdout)
    .map_err(|e| anyhow::anyhow!("Failed to parse lsblk output as JSON: {}", e))?;

  // Extract and filter block devices from lsblk output
  let blockdevices = lsblk_json
    .get("blockdevices")
    .and_then(|v| v.as_array())
    .ok_or_else(|| anyhow::anyhow!("lsblk output missing 'blockdevices' array"))?
    .iter()
    .filter(|dev| is_safe_device(dev))  // Only include devices safe for partitioning
    .collect::<Vec<_>>();
  // Parse each block device, but only include actual disks (not partitions, LVM, etc.)
  let mut disks = vec![];
  for device in blockdevices {
    let dev_type = device
      .get("type")
      .and_then(|v| v.as_str())
      .ok_or_else(|| anyhow::anyhow!("Device entry missing TYPE"))?;
    
    // Only process devices of type "disk" (physical drives)
    if dev_type == "disk" {
      let disk = parse_disk(device.clone())?;
      disks.push(disk);
    }
  }
  Ok(disks)
}

/// Parse a single disk entry from lsblk JSON output into our Disk structure
/// 
/// Extracts disk metadata (name, size, sector size) and recursively parses
/// any existing partitions as child objects
pub fn parse_disk(disk: Value) -> anyhow::Result<Disk> {
  let obj = disk
    .as_object()
    .ok_or_else(|| anyhow::anyhow!("Disk entry is not an object"))?;

  let name = obj
    .get("name")
    .and_then(|v| v.as_str())
    .ok_or_else(|| anyhow::anyhow!("Disk entry missing NAME"))?
    .to_string();

  let size = obj
    .get("size")
    .and_then(|v| v.as_u64())
    .ok_or_else(|| anyhow::anyhow!("Disk entry missing or invalid SIZE: {:?}", obj.clone()))?;

  // Get physical sector size, defaulting to 512 bytes (standard for most drives)
  let sector_size = obj.get("phy-sec").and_then(|v| v.as_u64()).unwrap_or(512);

  // Parse existing partitions on this disk
  let mut layout = Vec::new();
  if let Some(children) = obj.get("children").and_then(|v| v.as_array()) {
    for part in children {
      let partition = parse_partition(part)?;
      layout.push(partition);
    }
  }

  // Convert byte size to sector count and create disk object
  let mut disk = Disk::new(name, size / sector_size, sector_size, layout);
  disk.calculate_free_space(); // Calculate available free space between partitions
  Ok(disk)
}

/// Parse a single partition entry from lsblk JSON output
/// 
/// Converts lsblk partition data into our DiskItem::Partition structure
pub fn parse_partition(part: &Value) -> anyhow::Result<DiskItem> {
  let obj = part
    .as_object()
    .ok_or_else(|| anyhow::anyhow!("Partition entry is not an object"))?;

  let start = obj
    .get("start")
    .and_then(|v| v.as_u64())
    .ok_or_else(|| anyhow::anyhow!("Partition entry missing or invalid START"))?;

  let size = obj
    .get("size")
    .and_then(|v| v.as_u64())
    .ok_or_else(|| anyhow::anyhow!("Partition entry missing or invalid SIZE"))?;

  // Get sector size (should match parent disk, but we'll be safe)
  let sector_size = obj.get("phy-sec").and_then(|v| v.as_u64()).unwrap_or(512);

  let name = obj
    .get("name")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());
  let fs_type = obj
    .get("fstype")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());
  let mount_point = obj
    .get("mountpoint")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());
  let label = obj
    .get("label")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());

  // Note: lsblk doesn't provide read-only status in our query
  let ro = false;

  // Partition flags (like "boot", "esp") would need additional detection
  let flags = vec![];

  // Existing partitions discovered by lsblk are marked as "Exists"
  let status = PartStatus::Exists;

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
    flags,
  )))
}

/// Return a table showing available disk devices
pub fn disk_table(disks: &[Disk]) -> TableWidget {
  let (headers, widths): (Vec<String>, Vec<Constraint>) = DiskTableHeader::disk_table_header_info()
    .into_iter()
    .unzip();
  let rows: Vec<Vec<String>> = disks
    .iter()
    .map(|d| d.as_table_row(&DiskTableHeader::disk_table_headers()))
    .collect();
  TableWidget::new("Disks", widths, headers, rows)
}

/// Return a table showing available partitions for a disk device
pub fn part_table(disk_items: &[DiskItem], sector_size: u64) -> TableWidget {
  let (headers, widths): (Vec<String>, Vec<Constraint>) =
    DiskTableHeader::partition_table_header_info()
      .into_iter()
      .unzip();
  let rows: Vec<Vec<String>> = disk_items
    .iter()
    .map(|item| item.as_table_row(sector_size, &DiskTableHeader::partition_table_headers()))
    .collect();
  TableWidget::new("Partitions", widths, headers, rows)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
/// Represents a physical disk drive and its partition layout
/// 
/// Tracks both the current partition layout and the original layout
/// discovered at startup, allowing users to revert changes
pub struct Disk {
  name: String,
  size: u64, // sectors
  sector_size: u64,

  initial_layout: Vec<DiskItem>,
  total_used_sectors: u64,
  /// Current partition layout including partitions and free space
  /// 
  /// Partitions use half-open ranges: [start, start+size)
  /// This means start sector is included, end sector is excluded
  layout: Vec<DiskItem>,
}

impl Disk {
  pub fn new(name: String, size: u64, sector_size: u64, layout: Vec<DiskItem>) -> Self {
    let mut new = Self {
      name,
      size,
      sector_size,
      initial_layout: layout.clone(),
      total_used_sectors: 0,
      layout,
    };
    new.calculate_free_space();
    new
  }
  /// Get info as a table row, based on the given field names (`headers`)
  pub fn as_table_row(&self, headers: &[DiskTableHeader]) -> Vec<String> {
    headers
      .iter()
      .map(|h| {
        match h {
          DiskTableHeader::Status => "".into(),
          DiskTableHeader::Device => self.name.clone(),
          DiskTableHeader::Label => "".into(),
          DiskTableHeader::Start => "".into(), // Disk does not have a start sector in this context
          DiskTableHeader::End => "".into(),   // Disk does not have an end sector in this context
          DiskTableHeader::Size => bytes_readable(self.size_bytes()),
          DiskTableHeader::FSType => "".into(),
          DiskTableHeader::MountPoint => "".into(),
          DiskTableHeader::Flags => "".into(),
          DiskTableHeader::ReadOnly => "no".into(),
        }
      })
      .collect()
  }
  /// Convert the disk into a `disko` config
  pub fn as_disko_cfg(&mut self) -> serde_json::Value {
    let mut partitions = serde_json::Map::new();
    for item in &self.layout {
      if let DiskItem::Partition(p) = item {
        if *p.status() == PartStatus::Delete {
          continue;
        }
        let name = p
          .label()
          .map(|s| s.to_string())
          .unwrap_or_else(|| format!("part{}", p.id()));
        let size = bytes_disko_cfg(
          p.size_bytes(p.sector_size),
          self.total_used_sectors,
          p.sector_size,
          self.size,
        );

        if p.flags.contains(&"esp".to_string()) {
          partitions.insert(
            name,
            serde_json::json!({
              "size": size,
              "type": p.fs_gpt_code(p.flags.contains(&"esp".to_string())),
              "format": p.disko_fs_type(),
              "mountpoint": p.mount_point(),
            }),
          );
        } else {
          partitions.insert(
            name,
            serde_json::json!({
              "size": size,
              "format": p.disko_fs_type(),
              "mountpoint": p.mount_point(),
            }),
          );
        }
        self.total_used_sectors += p.size();
      }
    }
    self.total_used_sectors = 0;

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
      if let DiskItem::Partition(p) = item {
        Some(p)
      } else {
        None
      }
    })
  }
  pub fn partitions_mut(&mut self) -> impl Iterator<Item = &mut Partition> {
    self.layout.iter_mut().filter_map(|item| {
      if let DiskItem::Partition(p) = item {
        Some(p)
      } else {
        None
      }
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
      if let DiskItem::FreeSpace { start, size, .. } = *item {
        Some((start, size))
      } else {
        None
      }
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
    let Some(part_idx) = self.layout.iter().position(|item| item.id() == id) else {
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
    log::debug!("Adding new partition: {part:#?}");
    log::debug!("Current layout: {:#?}", self.layout);
    let new_start = part.start();
    let new_end = part.end();
    for item in &self.layout {
      if let DiskItem::Partition(p) = item {
        if p.status == PartStatus::Delete {
          // We do not care about deleted partitions
          continue;
        }
        let existing_start = p.start();
        let existing_end = p.end();
        if (new_start < existing_end) && (new_end > existing_start) {
          return Err(anyhow::anyhow!(
            "New partition overlaps with existing partition"
          ));
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
    self
      .layout
      .retain(|item| !matches!(item, DiskItem::FreeSpace { .. }));
    self.normalize_layout();
  }

  /// Recalculate free space gaps between partitions
  /// 
  /// This function rebuilds the layout by:
  /// 1. Keeping deleted partitions at the beginning (for UI visibility)
  /// 2. Finding gaps between existing partitions
  /// 3. Adding FreeSpace entries for gaps larger than 5MB
  pub fn calculate_free_space(&mut self) {
    // Separate deleted partitions from active ones
    // Deleted partitions are kept for UI display but don't affect free space calculation
    let (deleted, mut rest) = self.layout.iter().cloned().partition::<Vec<_>, _>(
      |item| matches!(item, DiskItem::Partition(p) if p.status == PartStatus::Delete),
    );

    // Sort remaining partitions by their start position on disk
    rest.sort_by_key(|p| p.start());

    let mut gaps = vec![];
    // Start at sector 2048 (1MB) to leave space for disk alignment and boot sectors
    let mut cursor = 2048u64;

    // Walk through partitions in order, identifying gaps
    for p in rest.iter() {
      let DiskItem::Partition(p) = p else {
        continue;  // Skip non-partition items
      };
      
      // Check if there's a gap before this partition
      if p.start() > cursor {
        let size = p.start() - cursor;
        
        // Only create FreeSpace entries for gaps larger than 5MB
        // Smaller gaps are typically unusable due to alignment requirements
        if size > mb_to_sectors(5, self.sector_size) {
          gaps.push(DiskItem::FreeSpace {
            id: get_entry_id(),
            start: cursor,
            size,
          });
        }
      }

      // Move cursor past this partition
      cursor = p.start() + p.size();
    }

    // Check for free space at the end of the disk
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

  /// Clean up the disk layout by sorting and merging adjacent free space
  /// 
  /// This ensures:
  /// - Deleted partitions appear first (for UI visibility)
  /// - Adjacent free space regions are merged into single entries
  pub fn normalize_layout(&mut self) {
    // Separate deleted partitions and put them at the beginning for UI organization
    let (mut new_layout, others): (Vec<_>, Vec<_>) = self
      .layout()
      .to_vec()
      .into_iter()
      .partition(|item| matches!(item, DiskItem::Partition(p) if p.status == PartStatus::Delete));
    let mut last_free: Option<(u64, u64)> = None; // Track adjacent free space: (start, size)

    new_layout.extend(others);
    let mut new_new_layout = vec![];

    // Merge adjacent free space while preserving partition order
    for item in &new_layout {
      match item {
        DiskItem::FreeSpace { start, size, .. } => {
          if let Some((last_start, last_size)) = last_free {
            // Extend the current free space region
            last_free = Some((last_start, last_size + size));
          } else {
            // Start tracking a new free space region
            last_free = Some((*start, *size));
          }
        }
        DiskItem::Partition(p) => {
          // If we have accumulated free space, add it to the layout
          if let Some((start, size)) = last_free.take() {
            new_new_layout.push(DiskItem::FreeSpace {
              id: get_entry_id(),
              start,
              size,
            });
          }
          // Add the partition
          new_new_layout.push(DiskItem::Partition(p.clone()));
        }
      }
    }
    // Add any remaining free space at the end
    if let Some((start, size)) = last_free.take() {
      new_new_layout.push(DiskItem::FreeSpace {
        id: get_entry_id(),
        start,
        size,
      });
    }

    self.layout = new_new_layout;
  }

  /// Apply the default NixOS partitioning scheme to this disk
  /// 
  /// Creates a standard two-partition layout:
  /// - 500MB FAT32 boot partition (ESP) at the beginning
  /// - Remaining space for root filesystem (specified fs_type or default)
  /// 
  /// All existing partitions are marked for deletion
  pub fn use_default_layout(&mut self, fs_type: Option<String>) {
    // Remove all free space and newly created partitions
    // Keep existing partitions so user can see what will be deleted
    self.layout.retain(|item| match item {
      DiskItem::FreeSpace { .. } => false,  // Remove all free space
      DiskItem::Partition(part) => part.status != PartStatus::Create,  // Remove created partitions
    });
    // Mark all existing partitions for deletion
    for part in self.layout.iter_mut() {
      let DiskItem::Partition(part) = part else {
        continue;
      };
      part.status = PartStatus::Delete
    }
    // Create 500MB FAT32 boot partition starting at sector 2048 (1MB aligned)
    // This serves as the EFI System Partition (ESP)
    let boot_part = Partition::new(
      2048,  // Start at 1MB boundary
      mb_to_sectors(500, self.sector_size),  // 500MB size
      self.sector_size,
      PartStatus::Create,
      None,
      Some("fat32".into()),                 // FAT32 filesystem
      Some("/boot".into()),                 // Mount at /boot
      Some("BOOT".into()),                  // Partition label
      false,
      vec!["boot".into(), "esp".into()],    // Mark as bootable ESP
    );
    // Create root partition using all remaining space
    let root_part = Partition::new(
      boot_part.end(),               // Start immediately after boot partition
      self.size - (boot_part.end()), // Use all remaining disk space
      self.sector_size,
      PartStatus::Create,
      None,
      fs_type,                       // User-specified or default filesystem
      Some("/".into()),              // Mount as root filesystem
      Some("ROOT".into()),           // Partition label
      false,
      vec![],                        // No special flags
    );
    // Add the new partitions to the layout
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
        headers
          .iter()
          .map(|h| {
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
          })
          .collect()
      }
      DiskItem::FreeSpace { start, size, .. } => {
        headers
          .iter()
          .map(|h| {
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
          })
          .collect()
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
  Unknown,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Partition {
  id: u64,
  start: u64,       // sectors
  size: u64,        // also sectors
  sector_size: u64, // bytes
  status: PartStatus,
  name: Option<String>,
  fs_type: Option<String>,
  mount_point: Option<String>,
  ro: bool,
  label: Option<String>,
  flags: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
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
    flags: Vec<String>,
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
      flags,
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
  /// Disko expects `vfat` for any fat fs types
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
        if is_esp {
          Some("EF00")
        } else {
          Some("0700")
        }
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
  flags: Vec<String>,
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
      flags: vec![],
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
    let start = self
      .start
      .ok_or_else(|| anyhow::anyhow!("start is required"))?;
    let size = self
      .size
      .ok_or_else(|| anyhow::anyhow!("size is required"))?;
    let sector_size = self.sector_size.unwrap_or(512); // default to 512 if not specified
    let mount_point = self
      .mount_point
      .ok_or_else(|| anyhow::anyhow!("mount_point is required"))?;
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
      flags: self.flags,
    })
  }
}

impl Default for PartitionBuilder {
  fn default() -> Self {
    Self::new()
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
  ReadOnly,
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
    Self::disk_table_headers()
      .iter()
      .map(|h| h.header_info())
      .collect()
  }
  pub fn partition_table_header_info() -> Vec<(String, Constraint)> {
    Self::partition_table_headers()
      .iter()
      .map(|h| h.header_info())
      .collect()
  }
  pub fn all_header_info() -> Vec<(String, Constraint)> {
    Self::all_headers()
      .iter()
      .map(|h| h.header_info())
      .collect()
  }
}
