use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use itertools::Itertools;
use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Alignment, Constraint, Layout, Rect}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Table, TableState, Widget}, Frame};
use serde_json::Value;

use crate::installer::Signal;

pub trait ConfigWidget {
	fn render(&self, f: &mut Frame, area: Rect);
	fn handle_input(&mut self, key: KeyEvent) -> Signal;
	fn interact(&mut self) {}
	fn focus(&mut self);
	fn unfocus(&mut self);
	fn is_focused(&self) -> bool;
	fn get_value(&self) -> Option<Value> {
		None
	}
}

pub struct WidgetBoxBuilder {
	title: Option<String>,
	layout: Option<Layout>,
	widgets: Vec<Box<dyn ConfigWidget>>,
	input_callback: Option<InputCallbackWidget>,
	render_borders: Option<bool>
}

impl WidgetBoxBuilder {
	pub fn new() -> Self {
		Self {
			title: None,
			layout: None,
			widgets: vec![],
			input_callback: None,
			render_borders: None
		}
	}
	pub fn title(mut self, title: impl Into<String>) -> Self {
		self.title = Some(title.into());
		self
	}
	pub fn layout(mut self, layout: Layout) -> Self {
		self.layout = Some(layout);
		self
	}
	pub fn children(mut self, widgets: Vec<Box<dyn ConfigWidget>>) -> Self {
		self.widgets = widgets;
		self
	}
	pub fn input_callback(mut self, callback: InputCallbackWidget) -> Self {
		self.input_callback = Some(callback);
		self
	}
	pub fn render_borders(mut self, render: bool) -> Self {
		self.render_borders = Some(render);
		self
	}
	fn get_default_layout(mut num_widgets: usize) -> Layout {
		if num_widgets == 0 { num_widgets = 1; } // avoid division by zero
		let space_per_widget = 100 / num_widgets;
		let mut constraints = vec![];
		for _ in 0..num_widgets {
			constraints.push(ratatui::layout::Constraint::Percentage(space_per_widget as u16));
		}
		Layout::default().direction(ratatui::layout::Direction::Horizontal).constraints(constraints)
	}
	pub fn build(self) -> WidgetBox {
		let title = self.title.unwrap_or_default();
		let num_widgets = self.widgets.len();
		let layout = self.layout.unwrap_or_else(|| Self::get_default_layout(num_widgets));
		let render_borders = self.render_borders.unwrap_or(false);
		WidgetBox::new(title, layout, self.widgets, self.input_callback, render_borders)
	}
}

pub type InputCallbackWidget = Box<dyn FnMut(&mut dyn ConfigWidget, KeyEvent) -> Signal>;
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
	pub fn new(
		title: String,
		layout: Layout,
		widgets: Vec<Box<dyn ConfigWidget>>,
		input_callback: Option<InputCallbackWidget>,
		render_borders: bool
	) -> Self {
		Self {
			focused: false,
			focused_child: if widgets.is_empty() { None } else { Some(0) },
			title,
			layout,
			widgets,
			input_callback,
			render_borders
		}
	}
	/// Alter the children array in-place, without altering the focus state
	pub fn set_children_inplace(&mut self, widgets: Vec<Box<dyn ConfigWidget>>) {
		self.widgets = widgets;
		if self.focused {
			self.focus(); // refreshes focus state for children
		}
	}
	pub fn first_child(&mut self) {
		self.focused_child = Some(0);
	}
	pub fn last_child(&mut self) {
		self.focused_child = Some(self.widgets.len().saturating_sub(1));
	}
	pub fn next_child(&mut self) -> bool {
		let idx = self.focused_child.unwrap_or(0);
		if idx + 1 < self.widgets.len() {
			let next_idx = idx + 1;
			self.widgets[idx].unfocus();
			self.focused_child = Some(next_idx);
			self.widgets[next_idx].focus();

			true
		} else {
			false
		}
	}
	pub fn prev_child(&mut self) -> bool {
		let idx = self.focused_child.unwrap_or(0);
		if idx > 0 {
			let prev_idx = idx - 1;
			self.widgets[idx].unfocus();
			self.focused_child = Some(prev_idx);
			self.widgets[prev_idx].focus();

			true
		} else {
			false
		}
	}
	pub fn selected_child(&self) -> Option<usize> {
		self.focused_child
	}

	pub fn focused_child_mut(&mut self) -> Option<&mut Box<dyn ConfigWidget>> {
		if let Some(idx) = self.focused_child {
			self.widgets.get_mut(idx)
		} else {
			None
		}
	}

	pub fn button_menu(buttons: Vec<Box<dyn ConfigWidget>>) -> Self {
		let num_btns = buttons.len();
		let mut constraints = vec![];
		for _ in 0..num_btns {
			constraints.push(Constraint::Length(1))
		}
		let layout = Layout::default()
			.direction(ratatui::layout::Direction::Vertical)
			.constraints(constraints);
		WidgetBoxBuilder::new()
			.layout(layout)
			.children(buttons)
			.build()
	}
}

impl ConfigWidget for WidgetBox {
	fn handle_input(&mut self, key: KeyEvent) -> Signal {
		let mut ret = Signal::Wait;
		if let Some(callback) = self.input_callback.as_mut() {
			if let Some(idx) = self.focused_child {
				ret = callback(self.widgets[idx].as_mut(), key);
			}
		}
		match ret {
			_ => {
				self.widgets[self.focused_child.unwrap_or(0)].handle_input(key)
			}
		}
	}

	fn focus(&mut self) {
	  self.focused = true;
		let Some(idx) = self.focused_child else {
			self.focused_child = Some(0);
			self.widgets[0].focus();
			return;
		};
		if idx < self.widgets.len() {
			self.widgets[idx].focus();
		} else if !self.widgets.is_empty() {
			self.focused_child = Some(0);
			self.widgets[0].focus();
		}
	}

	fn is_focused(&self) -> bool {
		self.focused
	}

	fn unfocus(&mut self) {
		self.focused = false;
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

	fn get_value(&self) -> Option<Value> {
		let mut map = serde_json::Map::new();
		for (i, widget) in self.widgets.iter().enumerate() {
			if let Some(value) = widget.get_value() {
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

pub struct CheckBox {
	pub label: String,
	pub checked: bool,
	pub focused: bool
}

impl CheckBox {
	pub fn new(label: impl Into<String>, checked: bool) -> Self {
		Self {
			label: label.into(),
			checked,
			focused: false
		}
	}
	pub fn toggle(&mut self) {
		self.checked = !self.checked;
	}
	pub fn is_checked(&self) -> bool {
		self.checked
	}
}

impl ConfigWidget for CheckBox {
	fn handle_input(&mut self, key: KeyEvent) -> Signal {
		match key.code {
			KeyCode::Char(' ') | KeyCode::Enter => {
				self.toggle();
			}
			_ => {}
		}
		Signal::Wait
	}

	fn interact(&mut self) {
		// Implementation of this method is necessary since it is technically stateful,
		// So we must be able to interact with it through the ConfigWidget interface,
		// so that the widget remains reactive in the case of use with WidgetBox for instance.
		self.toggle();
	}

	fn focus(&mut self) {
		self.focused = true;
	}

	fn is_focused(&self) -> bool {
		self.focused
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

		let checkbox_char = if self.checked { "[x]" } else { "[ ]" };
		let content = Paragraph::new(Span::styled(
			format!("{} {}", checkbox_char, self.label),
			style,
		))
		.alignment(Alignment::Center)
		.block(Block::default().style(style));

		f.render_widget(content, area);
	}

	fn get_value(&self) -> Option<Value> {
		Some(Value::Bool(self.checked))
	}
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
	fn handle_input(&mut self, key: KeyEvent) -> Signal {
		Signal::Wait
	}

	fn focus(&mut self) {
		self.focused = true;
	}

	fn is_focused(&self) -> bool {
		self.focused
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

	fn get_value(&self) -> Option<Value> {
		None // Buttons do not produce a value
	}
}

pub struct LineEditor {
	pub focused: bool,
	pub placeholder: Option<String>,
	pub is_secret: bool,
	pub title: String,
	pub value: String,
	pub error: Option<String>,
	pub cursor: usize
}

impl LineEditor {
	pub fn new(title: impl ToString, placeholder: Option<impl ToString>) -> Self {
		let title = title.to_string();
		let placeholder = placeholder.map(|p| p.to_string());
		Self {
			focused: false,
			placeholder,
			title,
			is_secret: false,
			value: String::new(),
			error: None,
			cursor: 0
		}
	}
	pub fn secret(mut self, is_secret: bool) -> Self {
		self.is_secret = is_secret;
		self
	}
	fn get_placeholder_line(&self, focused: bool) -> Line {
		if let Some(placeholder) = &self.placeholder {
			if placeholder.is_empty() {
				if focused {
					let span = Span::styled(
						" ",
						Style::default().fg(Color::DarkGray).bg(Color::White).add_modifier(Modifier::ITALIC),
					);
					Line::from(span)
				} else {
					let span = Span::styled(
						" ",
						Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
					);
					Line::from(span)
				}
			} else {
				let first_char = placeholder.chars().next().unwrap_or(' ');
				let rest = &placeholder[first_char.len_utf8()..];
				let first_char_span = if focused {
					Span::styled(
						first_char.to_string(),
						Style::default().fg(Color::DarkGray).bg(Color::White).add_modifier(Modifier::ITALIC),
					)
				} else {
					Span::styled(
						first_char.to_string(),
						Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
					)
				};
				let rest_span = Span::styled(
					rest.to_string(),
					Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
				);
				Line::from(vec![first_char_span, rest_span])
			}
		} else {
			let span = Span::styled(
				" ",
				Style::default().fg(Color::DarkGray).bg(Color::White).add_modifier(Modifier::ITALIC),
			);
			Line::from(span)
		}
	}
	fn render_line(&self) -> Line {
		if !self.focused {
			if self.is_secret {
				let masked = "*".repeat(self.value.chars().count());
				let span = Span::raw(masked);
				return Line::from(span);
			} else if !self.value.is_empty() {
				let span = Span::raw(self.value.clone());
				return Line::from(span);
			} else {
				return self.get_placeholder_line(false);
			}
		}

		if self.value.is_empty() {
			return self.get_placeholder_line(true);
		}

		let mut left = String::new();
		let mut cursor_char = None;
		let mut right = String::new();

		for (i, c) in self.value.chars().enumerate() {
			if i == self.cursor {
				if self.is_secret {
					cursor_char = Some('*');
				} else {
					cursor_char = Some(c);
				}
			} else if i < self.cursor {
				if self.is_secret {
					left.push('*');
				} else {
					left.push(c);
				}
			} else if self.is_secret {
				right.push('*');
			} else {
				right.push(c);
			}
		}

		if self.focused {
			Line::from(vec![
				Span::raw(left),
				Span::styled(
					cursor_char.map_or(" ".to_string(), |c| c.to_string()),
					Style::default().add_modifier(Modifier::REVERSED),
				),
				Span::raw(right),
			])
		} else {
			Line::from(vec![
				Span::raw(left),
				Span::raw(cursor_char.map_or(" ".to_string(), |c| c.to_string())),
				Span::raw(right),
			])
		}
	}
	fn as_widget(&self) -> Paragraph {
		Paragraph::new(self.render_line())
			.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
	}
	pub fn clear(&mut self) {
		self.value.clear();
		self.cursor = 0;
		self.error = None;
	}
	pub fn set_value(&mut self, value: impl ToString) {
		self.value = value.to_string();
		if self.cursor > self.value.len() {
			self.cursor = self.value.len();
		}
		self.error = None;
	}
	pub fn error(&mut self, msg: impl ToString) {
		self.error = Some(msg.to_string());
		self.value.clear();
		self.cursor = 0;
	}
}

impl ConfigWidget for LineEditor {
	fn handle_input(&mut self, key: KeyEvent) -> Signal {
		match key.code {
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
			KeyCode::Backspace => {
				if self.cursor > 0 && !self.value.is_empty() {
					self.value.remove(self.cursor - 1);
					self.cursor -= 1;
				}
			}
			KeyCode::Delete => {
				if self.cursor < self.value.len() && !self.value.is_empty() {
					self.value.remove(self.cursor);
				}
			}
			KeyCode::Char(c) => {
				self.value.insert(self.cursor, c);
				self.cursor += 1;
			}
			KeyCode::Home => {
				self.cursor = 0;
			}
			KeyCode::End => {
				self.cursor = self.value.len();
			}
			_ => {}
		}
		if self.cursor > self.value.len() {
			self.cursor = self.value.len();
		}
		Signal::Wait
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(ratatui::layout::Direction::Vertical)
			.constraints(
				vec![
					Constraint::Min(3),
					Constraint::Length(3)
				]
			)
			.split(area);
		if let Some(err) = &self.error {
			let error_paragraph = Paragraph::new(Span::styled(
				err.clone(),
				Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
			))
			.block(Block::default());
			f.render_widget(error_paragraph, chunks[1]);
		}
		let paragraph = self.as_widget();
		f.render_widget(paragraph, chunks[0]);
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

	fn unfocus(&mut self) {
	  self.focused = false;
	}

	fn get_value(&self) -> Option<Value> {
		Some(Value::String(self.value.clone()))
	}
}

pub struct StrListItem {
	pub idx: usize,
}

pub struct StrList {
	pub focused: bool,
	pub title: String,
	pub items: Vec<String>,
	pub filtered_items: Vec<StrListItem>, // after filtering
	pub filter: Option<String>,
	pub selected_idx: usize,
	pub committed_idx: Option<usize>,
	pub committed: Option<String>,
}

impl StrList {
	pub fn new(title: impl Into<String>, items: Vec<String>) -> Self {
		let filtered_items = items.iter().cloned().enumerate().map(|(i,_)| StrListItem { idx: i }).collect();
		Self {
			focused: false,
			title: title.into(),
			filtered_items,
			items,
			filter: None,
			selected_idx: 0,
			committed_idx: None,
			committed: None,
		}
	}
	pub fn selected_item(&self) -> Option<&String> {
		let item_idx = self.filtered_items.get(self.selected_idx)?;
		self.items.get(item_idx.idx)
	}
	pub fn next_item(&mut self) -> bool {
		if self.selected_idx + 1 < self.filtered_items.len() {
			self.selected_idx += 1;
			true
		} else {
			false
		}
	}
	pub fn previous_item(&mut self) -> bool {
		if self.selected_idx > 0 {
			self.selected_idx -= 1;
			true
		} else {
			false
		}
	}
	pub fn first_item(&mut self) {
		self.selected_idx = 0;
	}
	pub fn last_item(&mut self) {
		self.selected_idx = self.items.len().saturating_sub(1);
	}
	pub fn len(&self) -> usize {
		self.items.len()
	}
	pub fn is_empty(&self) -> bool {
		self.items.is_empty()
	}
	pub fn sort(&mut self) {
		self.items.sort();
		self.set_filter(self.filter.clone());
	}
	pub fn sort_by<F>(&mut self, mut compare: F)
	where
		F: FnMut(&String, &String) -> std::cmp::Ordering,
	{
		self.items.sort_by(|a, b| compare(a, b));
		self.set_filter(self.filter.clone());
	}
	pub fn set_items(&mut self, items: Vec<String>) {
		self.items = items;
		if self.selected_idx >= self.items.len() {
			self.selected_idx = self.items.len().saturating_sub(1);
		}
		self.set_filter(self.filter.clone());
	}
	pub fn set_filter(&mut self, filter: Option<impl Into<String>>) {
		let matcher = SkimMatcherV2::default();
		if let Some(f) = filter {
			let f = f.into();
			self.filter = Some(f.clone());
			let mut results: Vec<_> = self.items
				.iter()
				.enumerate()
				.filter_map(|(i,item)| {
					matcher.fuzzy_match(item, &f)
						.map(|score| (i, score))
				})
				.collect();
			results.sort_unstable_by(|a, b| b.1.cmp(&a.1));
			self.filtered_items = results.into_iter().map(|(i,_)| StrListItem { idx: i }).collect();
		} else {
			self.filter = None;
			self.filtered_items = self.items.iter().cloned().enumerate().map(|(i,_)| StrListItem { idx: i }).collect();
		}
		self.selected_idx = 0;
	}
	pub fn push_item(&mut self, item: impl Into<String>) {
		self.items.push(item.into());
	}
	pub fn push_unique(&mut self, item: impl Into<String>) -> bool {
		let item = item.into();
		if !self.items.contains(&item) {
			self.push_item(item);
			true
		} else {
			false
		}
	}
	pub fn push_sort_unique(&mut self, item: impl Into<String>) -> bool {
		let added = self.push_unique(item);
		if added {
			self.sort();
		}
		added
	}
	pub fn push_sort(&mut self, item: impl Into<String>) {
		self.push_item(item);
		self.sort();
	}
	pub fn add_item(&mut self, item: impl Into<String>) {
		self.push_item(item);
		self.set_filter(self.filter.clone());
	}
	pub fn remove_item(&mut self, idx: usize) -> Option<String> {
		let idx = self.filtered_items.get(idx).map(|sli| sli.idx)?;
		if idx < self.items.len() {
			let item = self.items.remove(idx);
			self.set_filter(self.filter.clone());
			if self.selected_idx >= self.filtered_items.len() && !self.filtered_items.is_empty() {
				self.selected_idx = self.filtered_items.len() - 1;
			}
			Some(item)
		} else {
			None
		}
	}
	pub fn remove_selected(&mut self) -> Option<String> {
		self.remove_item(self.selected_idx)
	}
}

impl ConfigWidget for StrList {
	fn handle_input(&mut self, key: KeyEvent) -> Signal {
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
		Signal::Wait
	}
	fn render(&self, f: &mut Frame, area: Rect) {
		let items: Vec<ListItem> = self
			.filtered_items
			.iter()
			.enumerate()
			.map(|(i,item)| {
				let prefix = if Some(i) == self.committed_idx {
					"> "
				} else {
					"  "
				};
				let idx = item.idx;
				let item = &self.items[idx];
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
	fn focus(&mut self) {
		self.focused = true;
	}
	fn unfocus(&mut self) {
		self.focused = false;
	}
	fn is_focused(&self) -> bool {
	  self.focused
	}
}


pub struct InfoBox<'a> {
	pub title: String,
	pub content: Vec<Line<'a>>
}

impl<'a> InfoBox<'a> {
	pub fn new(title: impl Into<String>, content: Vec<Line<'a>>) -> Self {
		Self {
			title: title.into(),
			content
		}
	}
}

impl<'a> ConfigWidget for InfoBox<'a> {
	fn handle_input(&mut self, _key: KeyEvent) -> Signal {
		Signal::Wait
	}
	fn render(&self, f: &mut Frame, area: Rect) {
		let paragraph = Paragraph::new(self.content.clone())
			.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
			.wrap(ratatui::widgets::Wrap { trim: true });
		f.render_widget(paragraph, area);
	}
	fn focus(&mut self) {
		// InfoBox does not need focus
	}
	fn unfocus(&mut self) {
		// InfoBox does not need focus
	}
	fn is_focused(&self) -> bool {
		false
	}
}

#[derive(Debug, Clone)]
pub struct TableRow {
	pub headers: Vec<String>,
	pub fields: Vec<String>,
}

impl TableRow {
	pub fn get_field(&self, header: &str) -> Option<&String> {
		if let Some(idx) = self.headers.iter().position(|h| h.to_lowercase() == header.to_lowercase()) {
			self.fields.get(idx)
		} else {
			None
		}
	}
}

#[derive(Clone,Debug)]
pub struct TableWidget {
	pub focused: bool,
	pub selected_row: Option<usize>,
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
			selected_row: None,
			title: title.into(),
			num_fields,
			headers,
			rows,
			widths
		}
	}
	pub fn set_rows(&mut self, rows: Vec<Vec<String>>) {
		self.rows = rows;
		if let Some(idx) = self.selected_row {
			if idx >= self.rows.len() {
				self.selected_row = None;
			}
		}
	}
	pub fn selected_row(&self) -> Option<usize> {
		self.selected_row
	}
	pub fn last_row(&mut self) {
		if !self.rows.is_empty() {
			self.selected_row = Some(self.rows.len() - 1);
		} else {
			self.selected_row = None;
		}
	}
	pub fn first_row(&mut self) {
		if !self.rows.is_empty() {
			self.selected_row = Some(0);
		} else {
			self.selected_row = None;
		}
	}
	pub fn next_row(&mut self) -> bool {
		let Some(idx) = self.selected_row else {
			self.selected_row = Some(0);
			return self.next_row()
		};
		if idx + 1 < self.rows.len() {
			self.selected_row = Some(idx + 1);
			true
		} else {
			false
		}
	}
	pub fn previous_row(&mut self) -> bool {
		let Some(idx) = self.selected_row else {
			self.selected_row = Some(0);
			return self.previous_row()
		};
		if idx > 0 {
			self.selected_row = Some(idx - 1);
			true
		} else {
			false
		}
	}
	pub fn get_selected_row_info(&self) -> Option<TableRow> {
		if let Some(idx) = self.selected_row {
			self.get_row(idx)
		} else {
			None
		}
	}
	pub fn get_row(&self, idx: usize) -> Option<TableRow> {
		if idx < self.rows.len() {
			Some(TableRow {
				headers: self.headers.clone(),
				fields: self.rows[idx].clone(),
			})
		} else {
			None
		}
	}
	pub fn fix_selection(&mut self) {
		if let Some(idx) = self.selected_row {
			if idx >= self.rows.len() {
				self.selected_row = Some(0);
			}
		} else if !self.rows.is_empty() {
			self.selected_row = Some(0);
		} else {
			self.selected_row = None;
		}
	}
	pub fn rows(&self) -> &Vec<Vec<String>> {
		&self.rows
	}
	pub fn len(&self) -> usize {
		self.rows.len()
	}
	pub fn is_empty(&self) -> bool {
		self.rows.is_empty()
	}
}

impl ConfigWidget for TableWidget {
	fn handle_input(&mut self, key: KeyEvent) -> Signal {
		if let Some(idx) = self.selected_row.as_mut() {
			log::debug!("TableWidget handle_input: key={:?}", key);
			match key.code {
				KeyCode::Up | KeyCode::Char('k') => {
					self.next_row();
				}
				KeyCode::Down | KeyCode::Char('j') => {
					self.previous_row();
				}
				_ => {}
			}
			Signal::Wait
		} else {
			self.selected_row = Some(0);
			self.handle_input(key)
		}
	}

	fn focus(&mut self) {
		self.focused = true;
		if self.selected_row.is_none() {
			self.selected_row = Some(0);
		}
	}
	fn is_focused(&self) -> bool {
		self.focused
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
		if self.selected_row.is_some_and(|idx| idx >= self.rows.len()) {
			state.select(None);
		} else {
			state.select(self.selected_row);
		}

		let hl_style = if self.focused {
			Style::default()
				.bg(Color::Cyan)
				.fg(Color::Black)
				.add_modifier(Modifier::BOLD)
		} else {
			Style::default()
		};

		let table = Table::new(rows, &self.widths)
			.header(header)
			.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
			.widths(&self.widths)
			.column_spacing(1)
			.row_highlight_style(hl_style)
			.highlight_symbol(">> ");

		f.render_stateful_widget(table, area, &mut state);
	}
}

pub struct HelpModal<'a> {
	pub visible: bool,
	pub title: String,
	pub content: Vec<Line<'a>>,
}

impl<'a> HelpModal<'a> {
	pub fn new(title: impl Into<String>, content: Vec<Line<'a>>) -> Self {
		Self {
			visible: false,
			title: title.into(),
			content,
		}
	}

	pub fn show(&mut self) {
		self.visible = true;
	}

	pub fn hide(&mut self) {
		self.visible = false;
	}

	pub fn toggle(&mut self) {
		self.visible = !self.visible;
	}

	pub fn render(&self, f: &mut Frame, area: Rect) {
		if !self.visible {
			return;
		}

		// Calculate popup size - 80% of screen
		let popup_width = (area.width as f32 * 0.8) as u16;
		let popup_height = (area.height as f32 * 0.8) as u16;
		let x = (area.width.saturating_sub(popup_width)) / 2;
		let y = (area.height.saturating_sub(popup_height)) / 2;

		let popup_area = Rect {
			x: area.x + x,
			y: area.y + y,
			width: popup_width,
			height: popup_height,
		};

		// Clear the popup area to remove background content
		f.render_widget(Clear, popup_area);

		// Render the help content
		let help_paragraph = Paragraph::new(self.content.clone())
			.block(
				Block::default()
					.title(format!("Help: {} (Press ? or ESC to close)", self.title))
					.borders(Borders::ALL)
					.border_style(Style::default().fg(Color::Yellow))
					.style(Style::default().bg(Color::Black))
			)
			.style(Style::default().bg(Color::Black).fg(Color::White))
			.wrap(ratatui::widgets::Wrap { trim: true });

		f.render_widget(help_paragraph, popup_area);
	}
}
