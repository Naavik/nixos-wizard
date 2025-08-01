use crate::{attrset, list, merge_attrs, nix::{fmt_nix, nixstr}};

#[derive(Debug)]
pub struct DiskPlanIRBuilder {
	pub device: Option<String>,
	pub part_table: Option<String>,
	pub fs: Option<String>,
	pub layout: Option<Vec<Partition>>,
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
			layout: None,
		}
	}
	pub fn device(mut self, device: &str) -> Self {
		self.device = Some(device.to_string());
		self
	}
	pub fn part_table(mut self, part_table: &str) -> Self {
		self.part_table = Some(part_table.to_string());
		self
	}
	pub fn fs(mut self, fs: &str) -> Self {
		self.fs = Some(fs.to_string());
		self
	}
	pub fn layout(mut self, layout: Vec<Partition>) -> Self {
		self.layout = Some(layout);
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
		let layout = self.layout.ok_or_else(|| anyhow::anyhow!("layout is required for manual plan"))?;
		Ok(DiskPlanIR::Manual { device, part_table, layout })
	}
}

#[derive(Debug)]
pub struct Partition {
	pub name: String,
	pub mountpoint: String,
	pub fs_type: String,
	pub fs_format: String,
	pub size: String,
	pub flags: Vec<String>,
	pub label: Option<String>,
}

#[derive(Debug)]
pub enum DiskPlanIR {
	Auto { device: String, part_table: String, fs: String },
	Manual {
		device: String,
		part_table: String,
		layout: Vec<Partition>
	},
}

impl From<DiskPlanIR> for DiskPlan {
	fn from(val: DiskPlanIR) -> Self {
	  		match val {
			DiskPlanIR::Auto { device, part_table, fs } => {
				let layout = vec![
					Partition {
						name: "boot".to_string(),
						mountpoint: "/boot".to_string(),
						fs_format: "vfat".to_string(),
						fs_type: "filesystem".to_string(),
						size: "512MiB".to_string(),
						flags: vec!["boot".to_string(), "esp".to_string()],
						label: Some("BOOT".to_string()),
					},
					Partition {
						name: "root".to_string(),
						mountpoint: "/".to_string(),
						fs_format: fs,
						fs_type: "filesystem".to_string(),
						size: "100%".to_string(),
						flags: vec![],
						label: Some("ROOT".to_string()),
					},
				];
				DiskPlan { device, part_table, layout }
			}
			DiskPlanIR::Manual { device, part_table, layout } => DiskPlan { device, part_table, layout },
		}
	}
}

#[derive(Debug)]
pub struct DiskPlan {
	pub device: String,
	pub part_table: String,
	pub layout: Vec<Partition>
}

impl DiskPlan {
	pub fn into_disko_config(&self) -> anyhow::Result<String> {
		let raw = self.fmt_disk();
		println!("Generated disko config:\n{raw}");
		fmt_nix(raw)
	}
	fn fmt_disk(&self) -> String {
		let disk_attrs = attrset! {
			type = nixstr("disk");
			device = nixstr(&self.device);
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
			let name = part.name.clone();
			let mut part_attrs = attrset! {
				size = nixstr(&part.size);
				content = attrset! {
					type = nixstr(&part.fs_type);
					format = nixstr(&part.fs_format);
					mountpoint = nixstr(&part.mountpoint);
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
