use std::{default, io};

use log::debug;
use ratatui::{crossterm::{event::{KeyCode, KeyEvent}, execute, terminal::{disable_raw_mode, LeaveAlternateScreen}}, layout::{Alignment, Constraint, Direction, Layout, Rect}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Table, TableState}, Frame};
use serde_json::{Map, Value};

use crate::{drive::DiskPlanIRBuilder, object_merge, page::{Audio, Bootloader, ChooseDiskDevice, DesktopEnv, DiskChooseStrategy, Drives, Greeter, Hostname, Kernels, KeyboardLayout, Language, Locale, Network, Page, Profile, RootPassword, SourceConfig, Swap, Timezone, UseFlakes, NO_SELECTION}};

pub fn extract_signals(ret: Option<Value>, keys: &[String]) -> Option<(String,String)> {
	if let Some(Value::Object(map)) = ret {
		for key in keys {
			if let Some(Value::String(s)) = map.get(key) {
				return Some((key.clone(), s.clone()));
			}
		}
	}
	None
}

pub fn make_signal(key: &str, value: &str) -> Value {
	let mut map = Map::new();
	map.insert(key.to_string(), Value::String(value.to_string()));
	Value::Object(map)
}

pub trait ConfigWidget {
	fn render(&self, f: &mut Frame, area: Rect);
	fn confirm(&mut self) -> Option<Value>;
	fn is_focused(&self) -> bool;
	fn focus(&mut self);
	fn unfocus(&mut self);
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value>;
	fn to_value(&self) -> Option<Value>;
}

pub struct Button {
	pub label: String,
	pub focused: bool
}

impl Button {
	pub fn new(label: impl Into<String>) -> Self {
		Self {
			label: label.into(),
			focused: false
		}
	}
}

impl ConfigWidget for Button {
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		if key.code == KeyCode::Enter {
			self.confirm()
		} else {
			None
		}
	}

	fn focus(&mut self) {
		self.focused = true;
	}

	fn is_focused(&self) -> bool {
		self.focused
	}

	fn confirm(&mut self) -> Option<Value> {
		let mut map = Map::new();
		map.insert("button_pressed".into(), Value::String(self.label.clone()));
		Some(Value::Object(map))
	}

	fn unfocus(&mut self) {
		self.focused = false;
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		let style = if self.focused {
			Style::default()
				.fg(Color::Black)
				.bg(Color::Cyan)
				.add_modifier(Modifier::BOLD)
		} else {
			Style::default()
				.fg(Color::White)
				.bg(Color::Reset)
		};

		let content = Paragraph::new(Span::styled(
			format!(" {} ", self.label),
			style,
		))
		.alignment(Alignment::Center)
		.block(Block::default().style(style));

		f.render_widget(content, area);
	}

	fn to_value(&self) -> Option<Value> {
		None // Buttons do not produce a value
	}
}

pub struct WidgetBox {
	pub focused: bool,
	pub focused_child: Option<usize>,
	pub title: String,
	pub layout: Layout,
	pub widgets: Vec<Box<dyn ConfigWidget>>,
	pub input_callback: Option<InputCallbackWidget>,
	pub render_borders: bool
}

impl WidgetBox {
	pub fn next_child(&mut self) {
		if let Some(idx) = self.focused_child {
			let next_idx = (idx + 1) % self.widgets.len();
			self.widgets[idx].unfocus();
			self.focused_child = Some(next_idx);
			self.widgets[next_idx].focus();
		}
	}
	pub fn prev_child(&mut self) {
		if let Some(idx) = self.focused_child {
			let prev_idx = if idx == 0 {
				self.widgets.len() - 1
			} else {
				idx - 1
			};
			self.widgets[idx].unfocus();
			self.focused_child = Some(prev_idx);
			self.widgets[prev_idx].focus();
		}
	}
}

impl ConfigWidget for WidgetBox {
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		let mut ret = None;
		if let Some(callback) = self.input_callback.as_mut() {
			if let Some(idx) = self.focused_child {
				if let Some(value) = callback(self.widgets[idx].as_mut(), key) {
					ret = Some(value);
				}
			}
		}
		if let Some(signal) = extract_signals(ret, ["next_child".into(), "prev_child".into()].as_ref()) {
			match signal.0.as_str() {
				"next_child" => {
					self.next_child();
					return None;
				}
				"prev_child" => {
					self.prev_child();
					return None;
				}
				_ => {}
			}
		}
		self.widgets[self.focused_child.unwrap_or(0)].handle_input(key)
	}

	fn focus(&mut self) {
	  self.focused = true;
		self.focused_child = Some(0);
		self.widgets[0].focus();
	}

	fn is_focused(&self) -> bool {
		self.focused
	}

	fn confirm(&mut self) -> Option<Value> {
		if let Some(idx) = self.focused_child {
			self.widgets[idx].confirm();
		}
		None
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.focused_child = None;
		for widget in self.widgets.iter_mut() {
			widget.unfocus();
		}
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		// If render_borders is enabled, draw a bordered block and reduce the area for inner widgets
		let inner_area = if self.render_borders {
			let block = Block::default()
				.title(self.title.clone())
				.borders(Borders::ALL);
			f.render_widget(block, area);
			Rect {
				x: area.x + 1,
				y: area.y + 1,
				width: area.width.saturating_sub(2),
				height: area.height.saturating_sub(2),
			}
		} else {
			area
		};

		let chunks = self.layout.split(inner_area);
		for (i, widget) in self.widgets.iter().enumerate() {
			if let Some(chunk) = chunks.get(i) {
				widget.render(f, *chunk);
			}
		}
	}

	fn to_value(&self) -> Option<Value> {
		let mut map = serde_json::Map::new();
		for (i, widget) in self.widgets.iter().enumerate() {
			if let Some(value) = widget.to_value() {
				map.insert(format!("widget_{i}"), value);
			}
		}
		if map.is_empty() {
			None
		} else {
			Some(Value::Object(map))
		}
	}
}

/// Callback type that allows providing arbitrary logic for handling input
pub type InputCallbackWidget = Box<dyn FnMut(&mut dyn ConfigWidget, KeyEvent) -> Option<Value>>;
pub type InputCallbackPage = Box<dyn FnMut(&mut dyn Page, KeyEvent) -> Option<Value>>;
/// A list of pages, only one of which is active at a time.
pub struct PageList {
	pub focused: bool,
	pub selected_idx: usize,
	pub pages: Vec<Box<dyn Page>>,
	pub input_callback: Option<InputCallbackPage>
}

impl PageList {
	pub fn new(
		pages: Vec<Box<dyn Page>>,
		input_callback: Option<InputCallbackPage>
	) -> Self {
		Self {
			focused: false,
			selected_idx: 0,
			pages,
			input_callback
		}
	}

	pub fn switch_to_page_by_title(&mut self, title: &str) -> bool {
		if let Some((idx, _)) = self.pages.iter().enumerate().find(|(_, p)| p.title() == title) {
			if idx != self.selected_idx {
				self.pages[self.selected_idx].unfocus();
				self.selected_idx = idx;
				if self.focused {
					self.pages[self.selected_idx].focus();
				}
			}
			true
		} else {
			false
		}
	}
}

impl ConfigWidget for PageList {
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		let mut switch_page = None;
		if let Some(callback) = self.input_callback.as_mut() {
			if let Some(page) = self.pages.get_mut(self.selected_idx) {
				let ret = callback(page.as_mut(), key);
				match ret {
					Some(ref val) => {
						let Some((_,title)) = extract_signals(Some(val.clone()), &["switch_page".into()]) else { return ret; };
						switch_page = Some(title.clone());
					}
					None => return ret
				}
			}
		}
		if let Some(title) = switch_page {
			self.switch_to_page_by_title(&title);
			return None;
		}
		if let Some(page) = self.pages.get_mut(self.selected_idx) {
			page.handle_input(key)
		} else {
			None
		}
	}

	fn focus(&mut self) {
		self.focused = true;
		self.pages[self.selected_idx].focus();
	}

	fn is_focused(&self) -> bool {
		self.focused
	}

	fn confirm(&mut self) -> Option<Value> {
		if let Some(page) = self.pages.get_mut(self.selected_idx) {
			page.confirm();
		}
		None
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.pages[self.selected_idx].unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		if let Some(page) = self.pages.get(self.selected_idx) {
			page.render(f, area);
		} else {
			let block = Block::default()
				.title("No Page Selected")
				.borders(Borders::ALL)
				.style(Style::default().fg(Color::Red));
			f.render_widget(block, area);
		}
	}

	fn to_value(&self) -> Option<Value> {
		let mut value_map = Map::new();
		value_map.insert("config".to_string(), Value::Object(Map::new()));
		let mut values = Value::Object(value_map);
		for page in self.pages.iter() {
			let value = page.to_config();
			match page.title().as_str() {
				"Source Configuration" | "Overlays" => {
					values = object_merge!(values, value);
				}
				_ => {
					let config = values["config"].clone();
					let new_config = object_merge!(config, value);
					let Value::Object(ref mut map) = values else {
						continue;
					};
					map.insert("config".to_string(), new_config);
				}
			}
		}
		Some(values)
	}
}

pub struct ConfigMenu {
	pub titles: StrList,
	pub pages: PageList,
	pub button_row: WidgetBox,
	pub selected_idx: usize,
	pub editing_page: bool,
}

impl ConfigMenu {
	pub fn new() -> Self {
		let pages: Vec<String> = vec![
			"Source Configuration".into(),
			"Language".into(),
			"Keyboard Layout".into(),
		  "Locale".into(),
		  "Use Flakes".into(),
		  "Drives".into(),
		  "Bootloader".into(),
		  "Swap".into(),
		  "Hostname".into(),
		  // "Virtualization".into(),
		  "Root Password".into(),
		  // "User Accounts".into(),
		  "Profile".into(),
		  "Greeter".into(),
		  "Desktop Environment".into(),
		  "Audio".into(),
		  "Kernels".into(),
		  // "System Packages".into(),
		  "Network".into(),
		  "Timezone".into(),
		  // "Auto Time Sync".into(),
		  // "Overlays".into()
		];
		let mut page_name_list = StrList::new("", pages);
		page_name_list.focus();
		let page_list: Vec<Box<dyn Page>> = vec![
			Box::new(SourceConfig::new()),
			Box::new(Language::new()),
			Box::new(KeyboardLayout::new()),
			Box::new(Locale::new()),
			Box::new(UseFlakes::new()),
			Box::new(Drives::new()),
			Box::new(Bootloader::new()),
			Box::new(Swap::new()),
			Box::new(Hostname::new()),
			// Box::new(Virtualization::new()),
			Box::new(RootPassword::new()),
			// Box::new(UserAccounts::new()),
			Box::new(Profile::new()),
			Box::new(Greeter::new()),
			Box::new(DesktopEnv::new()),
			Box::new(Audio::new()),
			Box::new(Kernels::new()),
			// Box::new(SystemPackages::new()),
			Box::new(Network::new()),
			Box::new(Timezone::new()),
			// Box::new(AutoTimeSync::new()),
			// Box::new(Overlays::new()),
		];
		let page_list = PageList::new(page_list, None);
		let done_btn = Button { focused: false, label: "Done".into() };
		let abort_btn = Button { focused: false, label: "Abort".into() };
		let button_row = WidgetBox {
			focused: false,
			focused_child: None,
			title: "".into(),
			input_callback: None,
			layout: Layout::default()
				.direction(Direction::Horizontal)
				.constraints([
					Constraint::Percentage(50),
					Constraint::Percentage(50)
				]),
			widgets: vec![
				Box::new(done_btn),
				Box::new(abort_btn)
			],
			render_borders: false
		};
		Self {
			titles: page_name_list,
			pages: page_list,
			button_row,
			selected_idx: 0,
			editing_page: false
		}
	}
}

impl Default for ConfigMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigWidget for ConfigMenu {
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		let key_code = key.code;
		if self.button_row.is_focused() {
			match key_code {
				KeyCode::Up | KeyCode::Char('k') => {
					self.button_row.unfocus();
					self.titles.focus();
				}
				KeyCode::Left | KeyCode::Char('h') => {
					self.button_row.prev_child();
				}
				KeyCode::Right | KeyCode::Char('l') => {
					self.button_row.next_child();
				}
				KeyCode::Enter => {
					if let Some(idx) = self.button_row.focused_child {
						match idx {
							0 => { // Done
								return self.pages.to_value();
							}
							1 => { // Abort
								let _ = disable_raw_mode();
								let _ = execute!(io::stdout(), LeaveAlternateScreen);

								std::process::exit(0);
							}
							_ => {}
						}
					}
				}
				_ => {}
			}
			return None
		}
		if !self.editing_page {
			if key_code == KeyCode::Enter || key_code == KeyCode::Right || key_code == KeyCode::Char('l') {
				self.editing_page = true;
				self.titles.unfocus();
				self.pages.focus();
			}
			else if (key_code == KeyCode::Down || key_code == KeyCode::Char('j'))
				&& self.titles.selected_idx == self.titles.len() - 1
			{
				self.button_row.focus();
				self.titles.unfocus();
			} else {
				self.titles.handle_input(key);
				self.pages.selected_idx = self.titles.selected_idx;
			}
		} else if key_code == KeyCode::Esc {
			self.editing_page = false;
			self.titles.focus();
			self.pages.unfocus();
		} else {
			self.pages.handle_input(key);
		}
		None
	}

	fn focus(&mut self) {}

	fn is_focused(&self) -> bool {
		false
	}

	fn confirm(&mut self) -> Option<Value> {None}

	fn unfocus(&mut self) {}

	fn render(&self, f: &mut Frame, area: Rect) {
		let block = Block::default()
			.title("Configure NixOS")
			.borders(Borders::ALL)
			.style(Style::default().fg(Color::White).bg(Color::Black));
		f.render_widget(block, area);

		let inner_area = Rect {
			x: area.x + 1,
			y: area.y + 1,
			width: area.width.saturating_sub(2),
			height: area.height.saturating_sub(2),
		};

		let chunks = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([
				Constraint::Percentage(20),
				Constraint::Percentage(80)
			])
			.split(inner_area);

		let left_chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Percentage(95),
				Constraint::Percentage(5)
			])
			.split(chunks[0]);

		// Highlight selected title when not editing a page
		self.titles.render(f, left_chunks[0]);


		self.button_row.render(f, left_chunks[1]);

		self.pages.render(f, chunks[1]);
	}

	fn to_value(&self) -> Option<Value> {
		todo!()
	}
}

pub struct DriveConfig {
	pub focused: bool,
	pub pages: PageList,
	pub disk_setup: DiskPlanIRBuilder
}

impl DriveConfig {
	pub fn new() -> Self {
		let pages: Vec<Box<dyn Page>> = vec![
			Box::new(DiskChooseStrategy::new()),
			Box::new(ChooseDiskDevice::new())
		];
		let pages_input_callback: Option<InputCallbackPage> = Some(Box::new(|page, key| {
			debug!("DriveConfig page input: page='{}' key={:?}", page.title(), key);
			match page.title().as_str() {
				"Disk Partitioning Strategy" => {
					let ret = page.handle_input(key);
					debug!("DriveConfig page input callback got ret: {:?}", ret);
					let Some(label) = extract_signals(ret.clone(), &["button_pressed".into()]) else { return ret };
					if label.1.as_str() == "Choose a best-effort default partition layout" {
						let mut new_ret = Map::new();
						new_ret.insert("switch_page".into(), Value::String("Choose Disk Device".into()));
						Some(Value::Object(new_ret))
					} else {
						ret
					}
				}
				_ => page.handle_input(key)
			}
		}));
		Self {
			focused: false,
			pages: PageList::new(pages, pages_input_callback),
			disk_setup: default::Default::default()
		}
	}
}

impl Default for DriveConfig {
	fn default() -> Self {
		Self::new()
	}
}

impl ConfigWidget for DriveConfig {
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		let ret = self.pages.handle_input(key);
		if let Some(title) = extract_signals(ret.clone(), &["switch_page".into()]) {
			self.pages.switch_to_page_by_title(&title.1);
			None
		} else {
			ret
		}
	}

	fn focus(&mut self) {
		self.focused = true;
		self.pages.focus();
	}

	fn is_focused(&self) -> bool {
		self.focused
	}

	fn confirm(&mut self) -> Option<Value> {
		// Do nothing for now
		None
	}

	fn unfocus(&mut self) {
		self.focused = false;
		self.pages.unfocus();
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		self.pages.render(f, area);
	}

	fn to_value(&self) -> Option<Value> {
		None // DriveConfig does not produce a value yet
	}
}

pub struct LineEditor {
	pub focused: bool,
	pub title: String,
	pub value: String,
	pub cursor: usize
}

impl LineEditor {
	pub fn new(title: String) -> Self {
		Self {
			focused: false,
			title,
			value: String::new(),
			cursor: 0
		}
	}
	fn render_line(&self) -> Line {
		if !self.focused {
			let span = Span::raw(self.value.clone());
			return Line::from(span);
		}
		let mut left = String::new();
		let mut cursor_char = None;
		let mut right = String::new();

		for (i, c) in self.value.chars().enumerate() {
			if i == self.cursor {
				cursor_char = Some(c);
			} else if i < self.cursor {
				left.push(c);
			} else {
				right.push(c);
			}
		}

		Line::from(vec![
			Span::raw(left),
			Span::styled(
				cursor_char.map_or(" ".to_string(), |c| c.to_string()),
				Style::default().add_modifier(Modifier::REVERSED),
			),
			Span::raw(right),
		])
	}
	fn as_widget(&self) -> Paragraph {
		Paragraph::new(self.render_line())
			.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
	}
}

impl ConfigWidget for LineEditor {
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Char(c) => {
				self.value.insert(self.cursor, c);
				self.cursor += 1;
			}
			KeyCode::Backspace => {
				if self.cursor > 0 {
					self.cursor -= 1;
					self.value.remove(self.cursor);
				}
			}
			KeyCode::Delete => {
				if self.cursor < self.value.len() {
					self.value.remove(self.cursor);
				}
			}
			KeyCode::Left => {
				if self.cursor > 0 {
					self.cursor -= 1;
				}
			}
			KeyCode::Right => {
				if self.cursor < self.value.len() {
					self.cursor += 1;
				}
			}
			KeyCode::Home => {
				self.cursor = 0;
			}
			KeyCode::End => {
				self.cursor = self.value.len();
			}
			_ => {}
		}
		None
	}
	fn focus(&mut self) {
		self.focused = true;
		if self.cursor > self.value.len() {
			self.cursor = self.value.len();
		}
	}
	fn is_focused(&self) -> bool {
		self.focused
	}
	fn confirm(&mut self) -> Option<Value> {
		// Do nothing for now
		None
	}
	fn unfocus(&mut self) {
		self.focused = false;
	}
	fn render(&self, f: &mut Frame, area: Rect) {
		let widget = self.as_widget();
		f.render_widget(widget, area);
	}
	fn to_value(&self) -> Option<Value> {
		if self.value.is_empty() {
			None
		} else {
			Some(Value::String(self.value.clone()))
		}
	}
}

pub struct StrList {
	pub focused: bool,
	pub title: String,
	pub items: Vec<String>,
	pub selected_idx: usize,
	pub committed_idx: Option<usize>,
	pub committed: Option<String>,
}

impl StrList {
	pub fn new(title: impl Into<String>, items: Vec<String>) -> Self {
		Self {
			focused: false,
			title: title.into(),
			items,
			selected_idx: 0,
			committed_idx: None,
			committed: None,
		}
	}
	pub fn len(&self) -> usize {
		self.items.len()
	}
	pub fn is_empty(&self) -> bool {
		self.items.is_empty()
	}
}
impl ConfigWidget for StrList {
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.selected_idx > 0 {
					self.selected_idx -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.selected_idx + 1 < self.items.len() {
					self.selected_idx += 1;
				}
			}
			KeyCode::Enter => {
				self.committed = Some(self.items[self.selected_idx].clone());
				self.committed_idx = Some(self.selected_idx);
			}
			_ => {}
		}
		None
	}

	fn focus(&mut self) {
		self.focused = true;
		if self.selected_idx >= self.items.len() && !self.items.is_empty() {
			self.selected_idx = self.items.len() - 1;
		}
	}
	fn is_focused(&self) -> bool {
		self.focused
	}
	fn confirm(&mut self) -> Option<Value> {
		if !self.items.is_empty() {
			self.committed = Some(self.items[self.selected_idx].clone());
			self.committed_idx = Some(self.selected_idx);
		}
		None
	}
	fn unfocus(&mut self) {
		self.focused = false;
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		let items: Vec<ListItem> = self
			.items
			.iter()
			.enumerate()
			.map(|(i,item)| {
				let prefix = if Some(i) == self.committed_idx {
					"> "
				} else {
					"  "
				};
				ListItem::new(Span::raw(format!("{prefix}{item}")))
			})
			.collect();

		let mut state = ListState::default();
		state.select(Some(self.selected_idx));

		let list = if self.focused {
			List::new(items)
				.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
				.highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD))
		} else {
			List::new(items)
				.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
				.highlight_style(Style::default())
		};

		f.render_stateful_widget(list, area, &mut state);
	}

	fn to_value(&self) -> Option<Value> {
		self.committed.as_ref().map(|s| Value::String(s.clone()))
	}
}

pub struct TableWidget {
	pub focused: bool,
	pub selected_row: usize,
	pub title: String,
	pub num_fields: usize,
	pub headers: Vec<String>,
	pub rows: Vec<Vec<String>>,
	pub widths: Vec<Constraint>
}

impl TableWidget {
	pub fn new(title: impl Into<String>, widths: Vec<Constraint>, headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
		let num_fields = headers.len();
		Self {
			focused: false,
			selected_row: NO_SELECTION,
			title: title.into(),
			num_fields,
			headers,
			rows,
			widths
		}
	}
}

impl ConfigWidget for TableWidget {
	fn handle_input(&mut self, key: KeyEvent) -> Option<Value> {
		debug!("TableWidget handle_input: key={:?}", key);
		match key.code {
			KeyCode::Up | KeyCode::Char('k') => {
				if self.selected_row > 0 {
					self.selected_row -= 1;
				}
			}
			KeyCode::Down | KeyCode::Char('j') => {
				if self.selected_row + 1 < self.rows.len() {
					self.selected_row += 1;
				}
			}
			KeyCode::Enter => {
				return self.confirm();
			}
			_ => {}
		}
		None
	}

	fn focus(&mut self) {
		self.focused = true;
		if self.selected_row == NO_SELECTION && !self.rows.is_empty() {
			self.selected_row = 0;
		}
	}
	fn is_focused(&self) -> bool {
		self.focused
	}

	fn confirm(&mut self) -> Option<Value> {
		match self.selected_row {
			NO_SELECTION => None,
			idx if idx < self.rows.len() => {
				let row = &self.rows[idx];
				let mut map = Map::new();
				for (i, header) in self.headers.iter().enumerate() {
					if i < row.len() {
						map.insert(header.clone(), Value::String(row[i].clone()));
					} else {
						map.insert(header.clone(), Value::Null);
					}
				}
				Some(Value::Object(map))
			}
			_ => None
		}
	}

	fn unfocus(&mut self) {
		self.focused = false;
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		let header_cells = self.headers.iter().map(|h| {
			Span::styled(
				h.clone(),
				Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
			)
		});
		let header = ratatui::widgets::Row::new(header_cells)
			.style(Style::default().bg(Color::DarkGray))
			.height(1)
			.bottom_margin(1);

		let rows = self.rows.iter().map(|item| {
			let cells = item.iter().map(|c| Span::raw(c.clone()));
			ratatui::widgets::Row::new(cells).height(1)
		});

		let mut state = TableState::default();
		if self.selected_row >= self.rows.len() {
			state.select(None);
		} else {
			state.select(Some(self.selected_row));
		}

		let table = Table::new(rows, &self.widths)
			.header(header)
			.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
			.widths(&self.widths)
			.column_spacing(1)
			.row_highlight_style(
					Style::default()
							.bg(Color::Cyan)
							.fg(Color::Black)
							.add_modifier(Modifier::BOLD),
			)
			.highlight_symbol(">> ");

		f.render_stateful_widget(table, area, &mut state);
	}

	fn to_value(&self) -> Option<Value> {
		None // StaticTable does not produce a value
	}
}

pub struct InfoBox {
	pub title: String,
	pub content: String
}

impl InfoBox {
	pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
		Self {
			title: title.into(),
			content: content.into()
		}
	}
}

impl ConfigWidget for InfoBox {
	fn handle_input(&mut self, _key: KeyEvent) -> Option<Value> {
		// InfoBox does not handle input
		None
	}

	fn focus(&mut self) {
		// InfoBox does not need to focus
	}
	fn is_focused(&self) -> bool {
		false
	}

	fn confirm(&mut self) -> Option<Value> {
		// InfoBox does not need to confirm
		None
	}

	fn unfocus(&mut self) {
		// InfoBox does not need to unfocus
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		let paragraph = Paragraph::new(self.content.clone())
			.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
			.wrap(ratatui::widgets::Wrap { trim: true });
		f.render_widget(paragraph, area);
	}

	fn to_value(&self) -> Option<Value> {
		None // InfoBox does not produce a value
	}
}
