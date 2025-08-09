pub mod installer;
pub mod widget;
pub mod drives;
pub mod nixgen;

pub use crate::installer::{Installer, Signal, Page, users::User};

pub fn styled_block<'a>(lines: Vec<Vec<(Option<(ratatui::style::Color, ratatui::style::Modifier)>, impl ToString)>>) -> Vec<ratatui::text::Line<'a>> {
	lines.into_iter().map(|line| {
		let spans = line.into_iter().map(|(style_opt, text)| {
			let mut span = ratatui::text::Span::raw(text.to_string());
			if let Some((color, modifier)) = style_opt {
				span.style = ratatui::style::Style::default().fg(color).add_modifier(modifier);
			}
			span
		}).collect::<Vec<_>>();
		ratatui::text::Line::from(spans)
	}).collect()
}

#[macro_export]
macro_rules! command {
    ($cmd:expr, $($arg:expr),* $(,)?) => {{
			use std::process::Command;
			let mut c = Command::new($cmd);
				c.args(&[$($arg.to_string()),*]);
				c
		}};
    ($cmd:expr) => {{
			use std::process::Command;
			let c = Command::new($cmd);
				c
		}};
}

#[macro_export]
macro_rules! attrset {
	{$($key:tt = $val:expr);+ ;} => {{
		let mut parts = vec![];
		$(
			parts.push(format!("{} = {};", stringify!($key).trim_matches('"'), $val));
		)*
		format!("{{ {} }}", parts.join(" "))
  }};
}

#[macro_export]
macro_rules! merge_attrs {
	($($set:expr),* $(,)?) => {{
		let mut merged = String::new();
		$(
			if !$set.starts_with('{') || !$set.ends_with('}') {
				panic!("attrset must be a valid attribute set");
			}
			let inner = $set
			.strip_prefix('{')
			.and_then(|s| s.strip_suffix('}'))
			.unwrap_or("")
			.trim();
			merged.push_str(inner);
		)*
			format!("{{ {merged} }}")
	}};
}

#[macro_export]
macro_rules! list {
	($($item:expr),* $(,)?) => {
		{
			let items = vec![$(format!("{}", $item)),*];
			format!("[{}]", items.join(" "))
		}
	};
}