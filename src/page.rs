use std::process::Command;

use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Direction, Layout, Rect}, style::{Color, Style}, widgets::{Block, Borders, List, ListItem, ListState}, Frame};
use serde_json::{Map, Value};

use crate::{drive::DiskPlan, widget::{make_signal, Button, ConfigWidget, DriveConfig, InfoBox, InputCallbackWidget, LineEditor, StrList, TableWidget, WidgetBox}};

pub const NO_SELECTION: usize = usize::MAX;

#[macro_export]
macro_rules! object_merge {
    ($($val:expr),* $(,)?) => {{
			let mut merged = serde_json::Map::new();
			$(
				if let serde_json::Value::Object(map) = $val {
					merged.extend(map);
				}
			)*
			serde_json::Value::Object(merged)
		}};
}

pub trait Page {
	fn title(&self) -> String;
	fn render(&self, f: &mut Frame, area: Rect);
	fn confirm(&mut self);
	fn focus(&mut self);
	fn unfocus(&mut self);
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value>;
	fn to_config(&self) -> Value;
}

/*
	 SourceConfig,
	 Language,
	 KeyboardLayout,
	 Locale,
	 UseFlakes,
	 Drives,
	 Bootloader,
	 Swap,
	 Hostname,
	 Virtualization,
	 RootPassword,
	 UserAccounts,
	 Profile,
	 Greeter,
	 DesktopEnv,
	 Audio,
	 Kernels,
	 SystemPkgs,
	 Network,
	 Timezone,
	 AutoTimeSync,
	 Overlays,
	 */

pub struct SourceConfig {
	focused: bool,
	path: WidgetBox,
}

impl SourceConfig {
	pub fn new() -> Self {
		let editor = LineEditor::new("Flake Path".to_string());
		let info_box = InfoBox::new("", r#"Here you can source a flake to use for your configuration.
You can use any valid flake path, as long as the given path results in a nixosConfiguration.

Examples include:
github:foobar/nixos-config#sysConfig
path:./flake#sysConfig
etc
"#);
		let path = WidgetBox {
			focused: false,
			focused_child: None,
			title: "Source Configuration".to_string(),
			input_callback: None,
			layout: Layout::default()
				.direction(Direction::Vertical)
				.constraints([
					Constraint::Percentage(10),
					Constraint::Percentage(90)
				].as_ref())
				.margin(1),
				widgets: vec![Box::new(editor), Box::new(info_box)],
				render_borders: true
		};
		Self {
			focused: false,
			path
		}
	}
}

impl Default for SourceConfig {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for SourceConfig {
	fn title(&self) -> String {
		"Source Configuration".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.path.focus();
	}

	fn confirm(&mut self) {
		self.path.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.path.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.path.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		self.path.handle_input(key);
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(value) = self.path.to_value() {
			let Value::Object(val_map) = value else { unreachable!() };
			let path = val_map.get("widget_0").unwrap();
			map.insert("sourceConfigPath".to_string(), path.clone());
		}
		Value::Object(map)
	}
}

pub struct Language {
	focused: bool,
	selected_idx: usize,
	langs: StrList,
}

impl Language {
	pub fn new() -> Self {
		Self {
			focused: false,
			selected_idx: 0,
			langs: StrList::new("Language", vec![
				"en".to_string(),
				"en_US".to_string(),
				"en_GB".to_string(),
				"es".to_string(),
				"de".to_string(),
				"fr".to_string(),
				"ja".to_string(),
			]),
		}
	}
}

impl Default for Language {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Language {
	fn title(&self) -> String {
		"Language".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.langs.focus();
	}

	fn confirm(&mut self) {
		self.langs.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.langs.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.langs.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.langs.selected_idx > 0 {
					self.langs.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.langs.selected_idx + 1 < self.langs.len() {
					self.langs.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.langs.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(lang) = &self.langs.to_value() {
			map.insert("language".to_string(), lang.clone());
		}
		Value::Object(map)
	}
}

pub struct KeyboardLayout {
	focused: bool,
	selected_idx: usize,
	layouts: StrList
}

impl KeyboardLayout {
	pub fn new() -> Self {
		let layouts = vec![
			"us".to_string(),
			"uk".to_string(),
			"de".to_string(),
			"fr".to_string(),
			"es".to_string(),
			"jp".to_string(),
		];
		Self {
			focused: false,
			selected_idx: NO_SELECTION,
			layouts: StrList::new("Keyboard Layout", layouts)
		}
	}
}

impl Default for KeyboardLayout {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for KeyboardLayout {
	fn title(&self) -> String {
		"Keyboard Layout".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.layouts.focus();
	}

	fn confirm(&mut self) {
		self.layouts.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.layouts.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.layouts.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.layouts.selected_idx > 0 {
					self.layouts.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.layouts.selected_idx + 1 < self.layouts.len() {
					self.layouts.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.layouts.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(layout) = self.layouts.to_value() {
			map.insert("keyboardLayout".to_string(), layout);
		}
		Value::Object(map)
	}
}

pub struct Locale {
	focused: bool,
	selected_idx: usize,
	locales: StrList,
}

impl Locale {
	pub fn new() -> Self {
		Self {
			focused: false,
			selected_idx: 0,
			locales: StrList::new("Locale", vec![
				"en_US.UTF-8".to_string(),
				"en_GB.UTF-8".to_string(),
				"de_DE.UTF-8".to_string(),
				"fr_FR.UTF-8".to_string(),
				"es_ES.UTF-8".to_string(),
				"ja_JP.UTF-8".to_string(),
			]),
		}
	}
}

impl Default for Locale {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Locale {
	fn title(&self) -> String {
		"Locale".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.locales.focus();
	}

	fn confirm(&mut self) {
		self.locales.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.locales.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.locales.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.locales.selected_idx > 0 {
					self.locales.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.locales.selected_idx + 1 < self.locales.len() {
					self.locales.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.locales.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(locale) = &self.locales.to_value() {
			map.insert("locale".to_string(), locale.clone());
		}
		Value::Object(map)
	}
}

pub struct UseFlakes {
	focused: bool,
	selected_idx: usize,
	options: StrList,
}

impl UseFlakes {
	pub fn new() -> Self {
		Self {
			focused: false,
			selected_idx: NO_SELECTION,
			options: StrList::new("Use Flakes", vec![
				"Yes".to_string(),
				"No".to_string(),
			])
		}
	}
}

impl Default for UseFlakes {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for UseFlakes {
	fn title(&self) -> String {
		"Use Flakes".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.options.focus();
	}

	fn confirm(&mut self) {
		self.options.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.options.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.options.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.options.selected_idx > 0 {
					self.options.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.options.selected_idx + 1 < self.options.len() {
					self.options.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.options.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(value) = &self.options.to_value() {
			let use_flakes = match value.as_str().unwrap_or("") {
				"Yes" => Value::Bool(true),
				"No" => Value::Bool(false),
				_ => Value::Null,
			};
			map.insert("useFlakes".to_string(), use_flakes);
		}
		Value::Object(map)
	}
}

pub struct Drives {
	focused: bool,
	disk_plan: Value,
	config_menu: DriveConfig,
	partition_table: TableWidget
}

impl Drives {
	pub fn new() -> Self {
		let config_menu = DriveConfig::new();
		let table_headers = vec![
			"Status".into(),
			"Device".into(),
			"Type".into(),
			"Size".into(),
			"FS Type".into(),
			"Mount Point".into(),
			"Mount Options".into(),
			"Flags".into(),
		];
		let widths = vec![
			Constraint::Percentage(10),
			Constraint::Percentage(10),
			Constraint::Percentage(10),
			Constraint::Percentage(10),
			Constraint::Percentage(10),
			Constraint::Percentage(16),
			Constraint::Percentage(16),
			Constraint::Percentage(10),
		];
		let partition_table = TableWidget::new("Disk Configuration", widths, table_headers, vec![
			vec![
				"create".into(),
				"/dev/sda".into(),
				"primary".into(),
				"500MB".into(),
				"fat32".into(),
				"/boot".into(),
				"defaults".into(),
				"boot, esp".into(),
			],
		]);
		Self {
			focused: false,
			disk_plan: Value::Object(Map::new()),
			config_menu,
			partition_table
		}
	}
}

impl Default for Drives {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for Drives {
	fn title(&self) -> String {
		"Disk Configuration".into()
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Percentage(70),
				Constraint::Percentage(30)
			].as_ref())
			.margin(1)
			.split(area);
		self.config_menu.render(f, chunks[0]);
		self.partition_table.render(f, chunks[1]);
	}

	fn confirm(&mut self) {}

	fn focus(&mut self) {
		self.focused = true;
		self.config_menu.focus();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.config_menu.unfocus();
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		self.config_menu.handle_input(key)
	}

	fn to_config(&self) -> Value {
		todo!()
	}
}

/*
 * The following structs are going to be sub-pages used in the drive config widget
 * These aren't used in the outer config menu
 */
pub struct DiskChooseStrategy {
	focused: bool,
	buttons: WidgetBox
}

impl DiskChooseStrategy {
	pub fn new() -> Self {
			let automatic_btn = Button::new("Choose a best-effort default partition layout");
			let manual_btn = Button::new("Manual partitioning");
			let buttons_callback: Option<InputCallbackWidget> = Some(Box::new(|widget, key| {
				match key.code {
					KeyCode::Up | KeyCode::Char('k') => {
						let signal = make_signal("prev_child", "");
						Some(signal)
					}
					KeyCode::Down | KeyCode::Char('j') => {
						let signal = make_signal("next_child", "");
						Some(signal)
					}
					_ => widget.handle_input(key)
				}
			}));
			let buttons = WidgetBox {
				focused: false,
				focused_child: None,
				title: "Choose Partitioning Strategy".to_string(),
				input_callback: buttons_callback,
				layout: Layout::default()
					.direction(Direction::Vertical)
					.constraints([
						Constraint::Percentage(10),
						Constraint::Percentage(10),
						Constraint::Percentage(10)
					].as_ref())
					.margin(1),
				widgets: vec![
					Box::new(automatic_btn),
					Box::new(manual_btn),
				],
				render_borders: true
			};
		Self {
			focused: false,
			buttons
		}
	}
}

impl Default for DiskChooseStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for DiskChooseStrategy {
	fn title(&self) -> String {
		"Disk Partitioning Strategy".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.buttons.focus();
	}

	fn confirm(&mut self) {}

	fn unfocus(&mut self) {
		self.focused = false;
		self.buttons.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.buttons.render(f, area);
	}
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		self.buttons.handle_input(key)
	}
	fn to_config(&self) -> Value {
		todo!()
	}
}

pub struct ChooseDiskDevice {
	focused: bool,
	table: TableWidget
}

impl ChooseDiskDevice {
	pub fn new() -> Self {
		let table_headers = vec![
			"Model".into(),
			"Path".into(),
			"Type".into(),
			"Size".into(),
			"Free Space".into(),
			"Sector Size".into(),
			"Read Only".into(),
		];
		let widths = vec![
			Constraint::Percentage(10),
			Constraint::Percentage(10),
			Constraint::Percentage(10),
			Constraint::Percentage(10),
			Constraint::Percentage(20),
			Constraint::Percentage(20),
			Constraint::Percentage(20),
		];
		let block_devices = Self::get_block_devices();
		let table = TableWidget::new("Available Disks", widths, table_headers, block_devices);
		Self {
			focused: false,
			table
		}
	}
	fn get_block_devices() -> Vec<Vec<String>> {
		let output = Command::new("lsblk")
			.args(["--json", "-o", "NAME,MODEL,TYPE,RO,SIZE,PHY-SEC", "-b"])
			.output()
			.expect("failed to run lsblk");

		let json_raw = String::from_utf8_lossy(&output.stdout);
		let parsed: Value = serde_json::from_str(&json_raw).unwrap_or(Value::Null);
		let Value::Array(devices) = &parsed["blockdevices"] else { return vec![]; };

		let mut result = vec![];

		for dev in devices {
			let Value::Object(map) = dev else { continue };
			let dtype = map.get("type").and_then(|v| v.as_str()).unwrap_or("");
			if dtype != "disk" { continue; }

			let name = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
			let model = map.get("model").and_then(|v| v.as_str()).unwrap_or("");
			let ro = map.get("ro").and_then(|v| v.as_bool()).unwrap_or(false);
			let ro_str = if ro { "Yes" } else { "No" };

			let size_bytes = map.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
			let size_str = format!("{:.1}GB", size_bytes as f64 / 1e9);

			let phy_sec = map.get("phy-sec").and_then(|v| v.as_u64()).map(|s| format!("{s}B")).unwrap_or_default();

			let used = map.get("children")
				.and_then(|c| c.as_array())
				.map(|children| children.iter().filter_map(|c| c.get("size").and_then(|v| v.as_u64())).sum::<u64>())
				.unwrap_or(0);
			let free_bytes = size_bytes.saturating_sub(used);
			let free_str = format!("{:.1}GB", free_bytes as f64 / 1e9);

			result.push(vec![
				model.to_string(),
				format!("/dev/{}", name),
				dtype.to_string(),
				size_str,
				free_str,
				phy_sec,
				ro_str.to_string(),
			]);
		}

		result
	}
}

impl Default for ChooseDiskDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for ChooseDiskDevice {
	fn title(&self) -> String {
		"Choose Disk Device".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.table.focus();
	}

	fn confirm(&mut self) {}

	fn unfocus(&mut self) {
		self.focused = false;
		self.table.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.table.render(f, area);
	}
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		self.table.handle_input(key)
	}
	fn to_config(&self) -> Value {
		todo!()
	}
}

pub struct Bootloader {
	focused: bool,
	selected_idx: usize,
	loaders: StrList,
}

impl Bootloader {
	pub fn new() -> Self {
		let loaders = vec![
			"GRUB".to_string(),
			"systemd-boot".to_string(),
			"rEFInd".to_string(),
			"none".to_string(),
		];
		Self {
			focused: false,
			selected_idx: NO_SELECTION,
			loaders: StrList::new("Bootloader", loaders)
		}
	}
}

impl Default for Bootloader {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Bootloader {
	fn title(&self) -> String {
		"Bootloader".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.loaders.focus();
	}

	fn confirm(&mut self) {
		self.loaders.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.loaders.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.loaders.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.loaders.selected_idx > 0 {
					self.loaders.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.loaders.selected_idx + 1 < self.loaders.len() {
					self.loaders.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.loaders.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(loader) = self.loaders.to_value() {
			map.insert("bootloader".to_string(), loader);
		}
		Value::Object(map)
	}
}

pub struct Swap {
	focused: bool,
	options: WidgetBox
}

impl Swap {
	pub fn new() -> Self {
		let enable = StrList::new("Enable Swap", vec![
			"Yes".to_string(),
			"No".to_string(),
		]);
		let options = WidgetBox {
			focused: false,
			focused_child: None,
			title: "Swap Options".to_string(),
			input_callback: None,
			layout: Layout::default()
				.direction(Direction::Vertical)
				.constraints([
					Constraint::Percentage(20),
					Constraint::Percentage(20)
				].as_ref())
				.margin(1),
			widgets: vec![
				Box::new(enable),
				Box::new(LineEditor::new("Swap Size (e.g., 2G, 500M, etc)".to_string())),
			],
			render_borders: true
		};
		Self {
			focused: false,
			options
		}
	}
}

impl Default for Swap {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Swap {
	fn title(&self) -> String {
		"Swap".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.options.focus();
	}

	fn confirm(&mut self) {
		self.options.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.options.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.options.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		if key.code == KeyCode::Tab {
			self.options.next_child();
		}
		self.options.handle_input(key);
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(value) = self.options.to_value() {
			let Value::Object(val_map) = value else { return Value::Object(map); };
			if let Some(enable) = val_map.get("widget_0") {
				let use_swap = match enable.as_str().unwrap_or("") {
					"Yes" => Value::Bool(true),
					"No" => Value::Bool(false),
					_ => Value::Null,
				};
				map.insert("enableSwap".to_string(), use_swap);
			}
			if let Some(size) = val_map.get("widget_1") {
				map.insert("swapSize".to_string(), size.clone());
			}
		}
		Value::Object(map)
	}
}

pub struct Hostname {
	focused: bool,
	hostname: LineEditor,
}

impl Hostname {
	pub fn new() -> Self {
		Self {
			focused: false,
			hostname: LineEditor::new("Hostname".to_string()),
		}
	}
}

impl Default for Hostname {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Hostname {
	fn title(&self) -> String {
		"Hostname".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.hostname.focus();
	}

	fn confirm(&mut self) {
		self.hostname.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.hostname.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.hostname.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		self.hostname.handle_input(key);
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(value) = self.hostname.to_value() {
			map.insert("hostname".to_string(), value);
		}
		Value::Object(map)
	}
}

pub struct Virtualization {
	use_virtualization: bool,
}

pub struct RootPassword {
	focused: bool,
	password: LineEditor,
	confirm_password: LineEditor,
}

impl RootPassword {
	pub fn new() -> Self {
		Self {
			focused: false,
			password: LineEditor::new("Root Password".to_string()),
			confirm_password: LineEditor::new("Confirm Password".to_string()),
		}
	}
}

impl Default for RootPassword {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for RootPassword {
	fn title(&self) -> String {
		"Root Password".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.password.focus();
	}

	fn confirm(&mut self) {
		self.password.confirm();
		self.confirm_password.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.password.unfocus();
		self.confirm_password.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Percentage(50),
				Constraint::Percentage(50)
			].as_ref())
			.margin(1)
			.split(area);
		self.password.render(f, chunks[0]);
		self.confirm_password.render(f, chunks[1]);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		if key.code == KeyCode::Tab {
			if self.password.focused {
				self.password.unfocus();
				self.confirm_password.focus();
			} else if self.confirm_password.focused {
				self.confirm_password.unfocus();
				self.password.focus();
			}
		} else {
			if self.password.focused {
				self.password.handle_input(key);
			} else if self.confirm_password.focused {
				self.confirm_password.handle_input(key);
			}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(pass) = self.password.to_value() {
			if let Some(confirm) = self.confirm_password.to_value() {
				if pass == confirm {
					map.insert("rootPassword".to_string(), pass);
				}
			}
		}
		Value::Object(map)
	}
}

pub struct UserAccount {
	username: String,
	password: String,
	confirm_password: String,
	is_admin: bool,
	show_password: bool,
}
pub struct UserAccounts {
	users: Vec<UserAccount>,
	selected_idx: usize,
}

pub struct Profile {
	focused: bool,
	selected_idx: usize,
	profiles: StrList,
}

impl Profile {
	pub fn new() -> Self {
		let profiles = vec![
			"Minimal".to_string(),
			"Desktop".to_string(),
			"Server".to_string(),
			"Workstation".to_string(),
		];
		Self {
			focused: false,
			selected_idx: NO_SELECTION,
			profiles: StrList::new("Profile", profiles)
		}
	}
}

impl Default for Profile {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Profile {
	fn title(&self) -> String {
		"Profile".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.profiles.focus();
	}

	fn confirm(&mut self) {
		self.profiles.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.profiles.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.profiles.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.profiles.selected_idx > 0 {
					self.profiles.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.profiles.selected_idx + 1 < self.profiles.len() {
					self.profiles.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.profiles.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(profile) = self.profiles.to_value() {
			map.insert("profile".to_string(), profile);
		}
		Value::Object(map)
	}
}

pub struct Greeter {
	focused: bool,
	selected_idx: usize,
	greeters: StrList,
}

impl Greeter {
	pub fn new() -> Self {
		let greeters = vec![
			"SDDM".to_string(),
			"GDM".to_string(),
			"LightDM".to_string(),
			"none".to_string(),
		];
		Self {
			focused: false,
			selected_idx: NO_SELECTION,
			greeters: StrList::new("Greeter", greeters)
		}
	}
}

impl Default for Greeter {
	fn default() -> Self {
		Self::new()
	}
}
impl Page for Greeter {
	fn title(&self) -> String {
		"Greeter".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.greeters.focus();
	}

	fn confirm(&mut self) {
		self.greeters.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.greeters.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.greeters.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.greeters.selected_idx > 0 {
					self.greeters.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.greeters.selected_idx + 1 < self.greeters.len() {
					self.greeters.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.greeters.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(greeter) = self.greeters.to_value() {
			map.insert("greeter".to_string(), greeter);
		}
		Value::Object(map)
	}
}

pub struct DesktopEnv {
	focused: bool,
	selected_idx: usize,
	desktops: StrList,
}

impl DesktopEnv {
	pub fn new() -> Self {
		let desktops = vec![
			"GNOME".to_string(),
			"KDE Plasma".to_string(),
			"XFCE".to_string(),
			"Cinnamon".to_string(),
			"MATE".to_string(),
			"none".to_string(),
		];
		Self {
			focused: false,
			selected_idx: NO_SELECTION,
			desktops: StrList::new("Desktop Environment", desktops)
		}
	}
}

impl Default for DesktopEnv {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for DesktopEnv {
	fn title(&self) -> String {
		"Desktop Environment".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.desktops.focus();
	}

	fn confirm(&mut self) {
		self.desktops.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.desktops.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.desktops.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.desktops.selected_idx > 0 {
					self.desktops.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.desktops.selected_idx + 1 < self.desktops.len() {
					self.desktops.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.desktops.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(desktop) = self.desktops.to_value() {
			map.insert("desktopEnvironment".to_string(), desktop);
		}
		Value::Object(map)
	}
}

pub struct Audio {
	focused: bool,
	selected_idx: usize,
	backends: StrList,
}

impl Audio {
	pub fn new() -> Self {
		let backends = vec![
			"PipeWire".to_string(),
			"PulseAudio".to_string(),
			"ALSA".to_string(),
			"none".to_string(),
		];
		Self {
			focused: false,
			selected_idx: NO_SELECTION,
			backends: StrList::new("Audio Backend", backends)
		}
	}
}

impl Default for Audio {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Audio {
	fn title(&self) -> String {
		"Audio".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.backends.focus();
	}

	fn confirm(&mut self) {
		self.backends.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.backends.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.backends.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.backends.selected_idx > 0 {
					self.backends.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.backends.selected_idx + 1 < self.backends.len() {
					self.backends.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.backends.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(backend) = self.backends.to_value() {
			map.insert("audioBackend".to_string(), backend);
		}
		Value::Object(map)
	}
}

pub struct Kernels {
	focused: bool,
	selected_idx: usize,
	kernels: StrList,
}

impl Kernels {
	pub fn new() -> Self {
		let kernels = vec![
			"linux".to_string(),
			"linux-lts".to_string(),
			"linux-zen".to_string(),
			"linux-hardened".to_string(),
			"none".to_string(),
		];
		Self {
			focused: false,
			selected_idx: NO_SELECTION,
			kernels: StrList::new("Kernel", kernels)
		}
	}
}

impl Default for Kernels {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Kernels {
	fn title(&self) -> String {
		"Kernels".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.kernels.focus();
	}

	fn confirm(&mut self) {
		self.kernels.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.kernels.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.kernels.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.kernels.selected_idx > 0 {
					self.kernels.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.kernels.selected_idx + 1 < self.kernels.len() {
					self.kernels.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.kernels.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(kernel) = self.kernels.to_value() {
			map.insert("kernel".to_string(), kernel);
		}
		Value::Object(map)
	}
}

pub struct SystemPkgs {
	packages: Vec<String>,
	new_pkg: String,
	selected_idx: usize,
}

pub struct Network {
	focused: bool,
	backend: StrList,
}

impl Network {
	pub fn new() -> Self {
		let backends = vec![
			"NetworkManager".to_string(),
			"wpa_supplicant".to_string(),
			"none".to_string(),
		];
		Self {
			focused: false,
			backend: StrList::new("Network Backend", backends)
		}
	}
}

impl Default for Network {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Network {
	fn title(&self) -> String {
		"Network".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.backend.focus();
	}

	fn confirm(&mut self) {
		self.backend.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.backend.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.backend.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.backend.selected_idx > 0 {
					self.backend.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.backend.selected_idx + 1 < self.backend.len() {
					self.backend.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				// Confirm selection
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(backend) = self.backend.to_value() {
			map.insert("networkBackend".to_string(), backend);
		}
		Value::Object(map)
	}
}

pub struct Timezone {
	focused: bool,
	selected_idx: usize,
	timezones: StrList,
}

impl Timezone {
	pub fn new() -> Self {
		let timezones = vec![
			"UTC".to_string(),
			"America/New_York".to_string(),
			"America/Los_Angeles".to_string(),
			"Europe/London".to_string(),
			"Europe/Berlin".to_string(),
			"Asia/Tokyo".to_string(),
			"Asia/Shanghai".to_string(),
		];
		Self {
			focused: false,
			selected_idx: NO_SELECTION,
			timezones: StrList::new("Timezone", timezones)
		}
	}
}

impl Default for Timezone {
	fn default() -> Self {
		Self::new()
	}
}

impl Page for Timezone {
	fn title(&self) -> String {
		"Timezone".to_string()
	}

	fn focus(&mut self) {
		self.focused = true;
		self.timezones.focus();
	}

	fn confirm(&mut self) {
		self.timezones.confirm();
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.timezones.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.timezones.render(f, area);
	}

	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.timezones.selected_idx > 0 {
					self.timezones.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.timezones.selected_idx + 1 < self.timezones.len() {
					self.timezones.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.selected_idx = self.timezones.selected_idx;
			}
			_ => {}
		}
		None
	}

	fn to_config(&self) -> Value {
		let mut map = Map::new();
		if let Some(tz) = self.timezones.to_value() {
			map.insert("timezone".to_string(), tz);
		}
		Value::Object(map)
	}
}

pub struct AutoTimeSync {
	auto_sync: bool,
}
