use std::{collections::VecDeque, io::{BufRead, BufReader, Read}, os::fd::{FromRawFd, IntoRawFd, OwnedFd}, process::{Child, ChildStderr, ChildStdout, Command, Stdio}};
use portable_pty::{PtySize, CommandBuilder};
use throbber_widgets_tui::{ThrobberState, BOX_DRAWING, BRAILLE_EIGHT};

use ansi_to_tui::IntoText;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Alignment, Constraint, Layout, Rect}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Table, TableState}, Frame};
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

impl Default for WidgetBoxBuilder {
    fn default() -> Self {
        Self::new()
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
		self.widgets[self.focused_child.unwrap_or(0)].handle_input(key)
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
	fn handle_input(&mut self, _key: KeyEvent) -> Signal {
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
	fn get_placeholder_line(&self, focused: bool) -> Line<'_> {
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
	fn render_line(&self) -> Line<'_> {
		if !self.focused {
			if self.is_secret {
				let masked = "*".repeat(self.value.chars().count());
				let span = Span::raw(masked);
				return Line::from(span);
			} else if !self.value.is_empty() {
				let span = Span::raw(self.value.clone());
				return Line::from(span);
			} else {
				return Line::from(Span::raw(" "));
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
	fn as_widget(&self) -> Paragraph<'_> {
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

/// Optimized list widget that works with pre-sorted data and avoids expensive operations
pub struct OptimizedStrList {
	pub focused: bool,
	pub title: String,
	pub items: Vec<String>,
	pub filter: Option<String>,
	pub selected_idx: usize,
}

impl OptimizedStrList {
	pub fn new(title: impl Into<String>, items: Vec<String>) -> Self {
		Self {
			focused: false,
			title: title.into(),
			items,
			filter: None,
			selected_idx: 0,
		}
	}

	pub fn set_items(&mut self, items: Vec<String>) {
		self.items = items;
		if self.selected_idx >= self.items.len() {
			self.selected_idx = self.items.len().saturating_sub(1);
		}
	}

	pub fn selected_item(&self) -> Option<&String> {
		self.items.get(self.selected_idx)
	}

	pub fn next_item(&mut self) -> bool {
		if self.selected_idx + 1 < self.items.len() {
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

	pub fn len(&self) -> usize {
		self.items.len()
	}

	pub fn is_empty(&self) -> bool {
		self.items.is_empty()
	}

	pub fn focus(&mut self) {
		self.focused = true;
	}

	pub fn unfocus(&mut self) {
		self.focused = false;
	}

	pub fn is_focused(&self) -> bool {
		self.focused
	}
}

impl ConfigWidget for OptimizedStrList {
	fn render(&self, f: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
		use ratatui::{
			prelude::*,
			widgets::{Block, Borders, List, ListItem, ListState},
		};

		let items: Vec<ListItem> = self.items
			.iter()
			.map(|item| ListItem::new(item.as_str()))
			.collect();

		let border_color = if self.focused {
			Color::Yellow
		} else {
			Color::Gray
		};

		let list = List::new(items)
			.block(Block::default()
				.title(self.title.as_str())
				.borders(Borders::ALL)
				.border_style(Style::default().fg(border_color)))
			.highlight_style(Style::default().bg(Color::Blue).fg(Color::White));

		let mut state = ListState::default();
		state.select(Some(self.selected_idx));

		f.render_stateful_widget(list, area, &mut state);
	}

	fn handle_input(&mut self, _key: ratatui::crossterm::event::KeyEvent) -> super::Signal {
		super::Signal::Wait
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
	pub content: Vec<Line<'a>>,
	pub highlighted: bool
}

impl<'a> InfoBox<'a> {
	pub fn new(title: impl Into<String>, content: Vec<Line<'a>>) -> Self {
		Self {
			title: title.into(),
			content,
			highlighted: false,
		}
	}
	pub fn highlighted(&mut self, highlighted: bool) {
		self.highlighted = highlighted;
	}
}

impl<'a> ConfigWidget for InfoBox<'a> {
	fn handle_input(&mut self, _key: KeyEvent) -> Signal {
		Signal::Wait
	}
	fn render(&self, f: &mut Frame, area: Rect) {
		let block = if self.highlighted {
			Block::default()
				.title(self.title.clone())
				.borders(Borders::ALL)
				.border_style(Style::default().fg(Color::Yellow))
		} else {
			Block::default()
				.title(self.title.clone())
				.borders(Borders::ALL)
		};
		let paragraph = Paragraph::new(self.content.clone())
			.block(block)
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

impl From<Vec<Command>> for ShellCommand {
	fn from(cmds: Vec<Command>) -> Self {
		ShellCommand::new(cmds).unwrap()
	}
}

pub enum ChildProcess {
	Standard(Child),
	Pty(Box<dyn portable_pty::Child + Send + Sync>),
}

pub struct ShellCommand {
	pub commands: Option<Vec<Command>>,
	pub child_procs: Option<Vec<ChildProcess>>,
	pub pty_pair: Option<portable_pty::PtyPair>,
}

impl ShellCommand {
	pub fn new(commands: Vec<Command>) -> anyhow::Result<Self> {
		Ok(Self {
			commands: Some(commands),
			child_procs: None,
			pty_pair: None,
		})
	}
	pub fn run_pipeline(&mut self) -> anyhow::Result<()> {
		let Some(cmds) = self.commands.take() else {
			return Err(anyhow::anyhow!("No commands to run"));
		};

		// Create PTY for terminal isolation
		let pty_system = portable_pty::native_pty_system();
		let pty_pair = pty_system.openpty(PtySize {
			rows: 24,
			cols: 80,
			pixel_width: 0,
			pixel_height: 0,
		})?;

		let mut child_procs = vec![];
		let len = cmds.len();

		if len == 1 {
			// Single command - use PTY directly
			let cmd = cmds.into_iter().next().unwrap();
			let mut cmd_builder = CommandBuilder::new(cmd.get_program());
			cmd_builder.args(cmd.get_args());
			let child = pty_pair.slave.spawn_command(cmd_builder)?;
			child_procs.push(ChildProcess::Pty(child));
		} else {
			// Multiple commands - still need pipes between them, but last one uses PTY
			let mut pipes: Vec<(Option<OwnedFd>, Option<OwnedFd>)> = vec![];

			for _ in 0..len - 1 {
				let (r,w) = nix::unistd::pipe()?;
				pipes.push((Some(r),Some(w)))
			}

			for (i, mut cmd) in cmds.into_iter().enumerate() {
				let stdin = if i == 0 {
					Stdio::piped()
				} else {
					let Some(pipe) = pipes[i - 1].0.take() else {
						unreachable!()
					};
					unsafe { Stdio::from_raw_fd(pipe.into_raw_fd()) }
				};

				if i == len - 1 {
					// Last command in pipeline - use PTY
					let mut cmd_builder = CommandBuilder::new(cmd.get_program());
					cmd_builder.args(cmd.get_args());
					// Set up stdin for PTY command - need to convert from Stdio to proper fd
					let child = pty_pair.slave.spawn_command(cmd_builder)?;
					child_procs.push(ChildProcess::Pty(child));
				} else {
					// Intermediate command - use regular pipe
					let stdout = {
						let Some(pipe) = pipes[i].1.take() else {
							unreachable!()
						};
						unsafe { Stdio::from_raw_fd(pipe.into_raw_fd()) }
					};
					let child = cmd.stdin(stdin).stdout(stdout).spawn()?;
					child_procs.push(ChildProcess::Standard(child));
				}
			}
		}

		self.pty_pair = Some(pty_pair);
		self.child_procs = Some(child_procs);
		Ok(())
	}

	pub fn single(cmd: Command) -> anyhow::Result<Self> {
		Self::new(vec![cmd])
	}

	pub fn nom(cmd: Command) -> anyhow::Result<Self> {
		let nom = Command::new("nom");
		Self::new(vec![cmd, nom])
	}

	pub fn stdin(&mut self) -> Option<&mut std::process::ChildStdin> {
		match self.child_procs.as_mut()?.first_mut()? {
			ChildProcess::Standard(child) => child.stdin.as_mut(),
			ChildProcess::Pty(_) => None, // PTY handles stdin differently
		}
	}
	pub fn take_stdin(&mut self) -> Option<std::process::ChildStdin> {
		match self.child_procs.as_mut()?.first_mut()? {
			ChildProcess::Standard(child) => child.stdin.take(),
			ChildProcess::Pty(_) => None, // PTY handles stdin differently
		}
	}

	pub fn stdout(&mut self) -> Option<&mut std::process::ChildStdout> {
		match self.child_procs.as_mut()?.last_mut()? {
			ChildProcess::Standard(child) => child.stdout.as_mut(),
			ChildProcess::Pty(_) => None, // PTY handles this through the master
		}
	}
	pub fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
		match self.child_procs.as_mut()?.last_mut()? {
			ChildProcess::Standard(child) => child.stdout.take(),
			ChildProcess::Pty(_) => None, // PTY handles this through the master
		}
	}

	pub fn stderr(&mut self) -> Option<&mut std::process::ChildStderr> {
		match self.child_procs.as_mut()?.last_mut()? {
			ChildProcess::Standard(child) => child.stderr.as_mut(),
			ChildProcess::Pty(_) => None, // PTY handles this through the master
		}
	}
	pub fn take_stderr(&mut self) -> Option<std::process::ChildStderr> {
		match self.child_procs.as_mut()?.last_mut()? {
			ChildProcess::Standard(child) => child.stderr.take(),
			ChildProcess::Pty(_) => None, // PTY handles this through the master
		}
	}

	pub fn take_pty_reader(&mut self) -> Option<Box<dyn Read + Send>> {
		self.pty_pair.take().map(|pty| pty.master.try_clone_reader().unwrap())
	}

	pub fn wait_all(&mut self) -> anyhow::Result<Vec<std::process::ExitStatus>> {
		let Some(children) = self.child_procs.as_mut() else {
			return Err(anyhow::anyhow!("No child processes to wait for"));
		};
		let mut results = Vec::with_capacity(children.len());
		for child in children {
			match child {
				ChildProcess::Standard(child) => {
					results.push(child.wait()?);
				}
				ChildProcess::Pty(child) => {
					// portable_pty::Child has different interface - convert to ExitStatus
					let exit_status = child.wait()?;
					// Convert portable_pty exit status to std::process::ExitStatus
					// This is tricky since ExitStatus can't be constructed directly
					// For now, let's use a workaround
					if exit_status.success() {
						// Create a successful status by running "true"
						let status = std::process::Command::new("true").status()?;
						results.push(status);
					} else {
						// Create a failed status by running "false"
						let status = std::process::Command::new("false").status()?;
						results.push(status);
					}
				}
			}
		}
		Ok(results)
	}
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepStatus {
	Inactive,
	Running,
	Completed,
	Failed,
}

pub struct InstallSteps<'a> {
	pub title: String,
	pub commands: VecDeque<(Line<'a>, VecDeque<Command>)>,
	pub steps: Vec<(Line<'a>, StepStatus)>,
	pub num_steps: usize,
	pub current_step_index: usize,
	pub throbber_state: ThrobberState,
	pub running: bool,
	pub error: bool,
	current_step_commands: Option<VecDeque<Command>>,
	current_command: Option<Child>,
}

impl<'a> InstallSteps<'a> {
	pub fn new(title: impl Into<String>, commands: impl IntoIterator<Item = (Line<'a>, VecDeque<Command>)>) -> Self {
		let commands = commands.into_iter().collect::<VecDeque<_>>();
		let steps = commands.iter().map(|(line, _)| (line.clone(), StepStatus::Inactive)).collect();
		let num_steps = commands.len();

		Self {
			title: title.into(),
			commands,
			steps,
			num_steps,
			current_step_index: 0,
			throbber_state: ThrobberState::default(),
			running: false,
			error: false,
			current_step_commands: None,
			current_command: None,
		}
	}

	pub fn progress(&self) -> f64 {
		if self.num_steps == 0 {
			1.0
		} else {
			let num_completed = self.steps
				.iter()
				.filter(|step| step.1 == StepStatus::Completed)
				.count();

			num_completed as f64 / self.num_steps as f64
		}
	}

	pub fn start_next_step(&mut self) -> anyhow::Result<()> {
		// If we have a current step still running, don't start a new one
		if self.current_step_commands.is_some() {
			return Ok(());
		}

		// Get the next step
		if let Some((_line, commands)) = self.commands.pop_front() {
			// Update step status
			if self.current_step_index < self.steps.len() {
				self.steps[self.current_step_index].1 = StepStatus::Running;
			}

			// Store the commands for this step
			self.current_step_commands = Some(commands);
		}
		Ok(())
	}

	pub fn start_next_command(&mut self) -> anyhow::Result<()> {
		// Get the next command from the current step
		if let Some(commands) = self.current_step_commands.as_mut() {
			if let Some(mut cmd) = commands.pop_front() {
				// Redirect all output to /dev/null
				let null = std::fs::File::create("/dev/null")?;
				cmd.stdout(Stdio::from(null.try_clone()?))
				   .stderr(Stdio::from(null))
				   .stdin(Stdio::null());

				let child = cmd.spawn()?;
				self.current_command = Some(child);
			}
		}
		Ok(())
	}

	pub fn tick(&mut self) -> anyhow::Result<()> {
		if !self.running && !self.error {
			self.start_next_step()?;
			self.running = true;
		}

		if self.running {
			self.throbber_state.calc_next();
		}

		// If no command is currently running, try to start the next one
		if self.current_command.is_none() && self.current_step_commands.is_some() {
			self.start_next_command()?;
		}

		if let Some(child) = &mut self.current_command {
			if let Ok(Some(status)) = child.try_wait() {
				self.current_command = None;

				if !status.success() {
					// Command failed - mark current step as failed
					if self.current_step_index < self.steps.len() {
						self.steps[self.current_step_index].1 = StepStatus::Failed;
					}
					self.error = true;
					self.running = false;
					return Ok(());
				}

				// Command succeeded - check if there are more commands in this step
				if let Some(commands) = &self.current_step_commands {
					if commands.is_empty() {
						// Step completed successfully
						if self.current_step_index < self.steps.len() {
							self.steps[self.current_step_index].1 = StepStatus::Completed;
						}
						self.current_step_commands = None;
						self.current_step_index += 1;
						self.running = false;

						// Check if we're completely done
						if self.commands.is_empty() {
							self.running = false;
						}
					}
					// If there are more commands in this step, they'll be started on next tick
				}
			}
		}
		Ok(())
	}

	pub fn is_complete(&self) -> bool {
		!self.running && !self.error && self.commands.is_empty() && self.current_step_commands.is_none()
	}

	pub fn has_error(&self) -> bool {
		self.error
	}
}

impl<'a> ConfigWidget for InstallSteps<'a> {
	fn handle_input(&mut self, _key: KeyEvent) -> Signal {
		Signal::Wait
	}

	fn render(&self, f: &mut Frame, area: Rect) {
		let mut lines = Vec::new();

		for (step_line, status) in self.steps.iter() {
			let (prefix, style) = match status {
				StepStatus::Inactive => ("  ", Style::default().fg(Color::DarkGray)),
				StepStatus::Running => {
					let idx = (self.throbber_state.index() % 4) as usize;
					let throbber_symbol = BOX_DRAWING.symbols[idx];
					(throbber_symbol, Style::default().fg(Color::Cyan))
				},
				StepStatus::Completed => ("✓ ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
				StepStatus::Failed => ("✗ ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
			};

			let mut step_spans = vec![Span::styled(prefix, style)];
			step_spans.extend(step_line.spans.iter().cloned().map(|mut span| {
				if *status == StepStatus::Inactive {
					span.style = span.style.fg(Color::DarkGray);
				}
				span
			}));

			lines.push(Line::from(step_spans));
		}

		let paragraph = Paragraph::new(lines)
			.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
			.wrap(ratatui::widgets::Wrap { trim: true });

		f.render_widget(paragraph, area);
	}

	fn focus(&mut self) {
		// InstallSteps does not need focus
	}

	fn unfocus(&mut self) {
		// InstallSteps does not need focus
	}

	fn is_focused(&self) -> bool {
		false
	}
}

/// Like infobox, except it runs a bunch of commands
/// and the content is the lines produced by the output of those commands
/// commands is a vecdeque of unspawned std::process::Commands
pub struct ShellBox<'a> {
	pub title: String,
	pub commands: VecDeque<Command>,
	pub num_cmds: usize,
	pub content: Vec<Line<'a>>,
	pub running: bool,
	pub error: bool,
	current_command: Option<Child>,
	stdout_reader: Option<BufReader<ChildStdout>>,
	stderr_reader: Option<BufReader<ChildStderr>>,
}

impl<'a> ShellBox<'a> {
	pub fn new(title: impl Into<String>, commands: impl IntoIterator<Item = Command>) -> Self {
		let commands = commands.into_iter().collect::<VecDeque<_>>();
		Self {
			title: title.into(),
			num_cmds: commands.len(),
			commands,
			content: vec![],
			running: false,
			error: false,
			current_command: None,
			stdout_reader: None,
			stderr_reader: None,
		}
	}
	pub fn progress(&self) -> u32 {
		// return 0-100 based on commands.len() vs self.num_cmds
		if self.num_cmds == 0 {
			100
		} else {
			let completed = self.num_cmds - self.commands.len();
			((completed as f64 / self.num_cmds as f64) * 100.0).round() as u32
		}
	}
	pub fn start_next_command(&mut self) -> anyhow::Result<()> {
		if let Some(mut cmd) = self.commands.pop_front() {
			let mut child = cmd.spawn()?;
			// Use PTY reader if available, otherwise fall back to stdout/stderr
			self.stdout_reader = child.stdout.take().map(BufReader::new);
			self.stderr_reader = child.stderr.take().map(BufReader::new);
			self.current_command = Some(child);
		}
		Ok(())
	}
	pub fn read_output(&mut self) -> anyhow::Result<()> {
		let mut line = String::new();

		// Read from PTY if available (preferred)
		// Fall back to separate stdout/stderr readers
		if let Some(stdout) = self.stdout_reader.as_mut() {
			while stdout.read_line(&mut line)? > 0 {
				let text = format!("| {line}").into_text()?;
				self.content.extend(text.lines);
				line.clear();
			}
		}

		if let Some(stderr) = self.stderr_reader.as_mut() {
			while stderr.read_line(&mut line)? > 0 {
				let text = format!("| {line}").into_text()?;
				self.content.extend(text.lines);
				line.clear();
			}
		}

		Ok(())
	}
	pub fn tick(&mut self) -> anyhow::Result<()> {
		if !self.running && !self.error {
			self.start_next_command()?;
			self.running = true;
		}

		if self.current_command.is_some() {
			self.read_output()?;
		}

		if let Some(child) = &mut self.current_command {

			if let Ok(status) = child.wait() {
				self.current_command = None;
				self.stdout_reader = None;
				self.stderr_reader = None;
				self.running = false;
				if !status.success() {
					self.content.push(Line::from(Span::styled(
								"Installation failed".to_string(),
								Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
					)));
					self.error = true;
				}
			}
		}
		Ok(())
	}
}

impl<'a> ConfigWidget for ShellBox<'a> {
	fn handle_input(&mut self, _key: KeyEvent) -> Signal {
		Signal::Wait
	}
	fn render(&self, f: &mut Frame, area: Rect) {
		let height = area.height as usize;
		let content_len = self.content.len();
		let mut lines = vec![];

		if content_len < height {
			let padding_lines = height - (content_len + 2);
			lines.extend(std::iter::repeat_n(Line::from(""), padding_lines));
		}

		lines.extend(self.content.iter().cloned());

		let paragraph = Paragraph::new(lines)
			.block(Block::default().title(self.title.clone()).borders(Borders::ALL))
			.wrap(ratatui::widgets::Wrap { trim: true });

		f.render_widget(paragraph, area);
	}
	fn focus(&mut self) {
		// ShellBox does not need focus
	}
	fn unfocus(&mut self) {
		// ShellBox does not need focus
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
		if let Some(_idx) = self.selected_row.as_mut() {
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

pub struct ProgressBar {
	message: String,
	progress: u32, // 0-100
}

impl ProgressBar {
	pub fn new(message: impl Into<String>, progress: u32) -> Self {
		Self {
			message: message.into(),
			progress,
		}
	}
	pub fn set_progress(&mut self, progress: u32) {
		self.progress = progress.clamp(0, 100);
	}
	pub fn set_message(&mut self, message: impl Into<String>) {
		self.message = message.into();
	}
}

impl ConfigWidget for ProgressBar {
	fn handle_input(&mut self, _key: KeyEvent) -> Signal {
		Signal::Wait
	}
	fn render(&self, f: &mut Frame, area: Rect) {
		let gauge = Gauge::default()
			.block(Block::default().title(self.message.clone()).borders(Borders::ALL))
			.gauge_style(
				Style::default()
				.fg(Color::Green)
				.bg(Color::Black)
				.add_modifier(Modifier::BOLD),
			)
			.percent(self.progress as u16);
		f.render_widget(gauge, area);
	}
	fn focus(&mut self) {
		// ProgressBar does not need focus
	}
	fn unfocus(&mut self) {
		// ProgressBar does not need focus
	}
	fn is_focused(&self) -> bool {
		false
	}
}
