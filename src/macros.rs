/// Sets up a new, unspawned std::process::Command
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
/// Creates a Nix attribute set using similar syntax.
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
/// Merges two attribute sets.
macro_rules! merge_attrs {
	($($set:expr),* $(,)?) => {{
		let mut merged = String::new();
		$(
			if !$set.is_empty() {
				if !$set.starts_with('{') || !$set.ends_with('}') {
					panic!("attrset must be a valid attribute set, got: {:?}", $set);
				}
				let inner = $set
				.strip_prefix('{')
				.and_then(|s| s.strip_suffix('}'))
				.unwrap_or("")
				.trim();
				merged.push_str(inner);
			}
		)*
			format!("{{ {merged} }}")
	}};
}

#[macro_export]
/// Creates a Nix list
macro_rules! list {
	($($item:expr),* $(,)?) => {
		{
			let items = vec![$(format!("{}", $item)),*];
			format!("[{}]", items.join(" "))
		}
	};
}

// Ui
#[macro_export]
/// Escape or 'q'
macro_rules! ui_close {
  () => {
    KeyCode::Esc | KeyCode::Char('q')
  };
}

#[macro_export]
/// Escape or 'q' or Left or 'h'
macro_rules! ui_back {
  () => {
    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Left | KeyCode::Char('h')
  };
}

#[macro_export]
/// Enter or Right or 'l'
macro_rules! ui_enter {
  () => {
    KeyCode::Enter | KeyCode::Right | KeyCode::Char('l')
  };
}

#[macro_export]
/// Down or 'j'
macro_rules! ui_down {
  () => {
    KeyCode::Down | KeyCode::Char('j')
  };
}

#[macro_export]
/// Up or 'k'
macro_rules! ui_up {
  () => {
    KeyCode::Up | KeyCode::Char('k')
  };
}

#[macro_export]
/// Left or 'h'
macro_rules! ui_left {
  () => {
    KeyCode::Left | KeyCode::Char('h')
  };
}
#[macro_export]
/// Right or 'l'
macro_rules! ui_right {
  () => {
    KeyCode::Right | KeyCode::Char('l')
  };
}
