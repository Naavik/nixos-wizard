use ratatui::{crossterm::event::KeyCode, layout::Constraint, text::Line};

use crate::{
  installer::{HIGHLIGHT, Installer, Page, Signal, systempkgs::get_available_pkgs},
  split_hor, split_vert, styled_block, ui_back, ui_close, ui_down, ui_enter, ui_up,
  widget::{
    Button, ConfigWidget, HelpModal, InfoBox, LineEditor, PackagePicker, StrList, TableWidget,
    WidgetBox,
  },
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct User {
  pub username: String,
  pub password_hash: String,
  pub groups: Vec<String>,
  pub home_manager_cfg: Option<HomeManagerCfg>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HomeManagerCfg {
  pub packages: Vec<String>,
}

impl User {
  pub fn as_table_row(&self) -> Vec<String> {
    let groups = if self.groups.is_empty() {
      "<none>".to_string()
    } else {
      self.groups.join(", ")
    };
    let use_hm = if self.home_manager_cfg.is_some() {
      "yes"
    } else {
      "no"
    }
    .to_string();
    vec![self.username.clone(), groups, use_hm]
  }
}

pub struct UserAccounts {
  pub user_table: TableWidget,
  pub buttons: WidgetBox,
  help_modal: HelpModal<'static>,
}

impl UserAccounts {
  pub fn new(users: Vec<User>) -> Self {
    let buttons = vec![Box::new(Button::new("Back")) as Box<dyn ConfigWidget>];
    let buttons = WidgetBox::button_menu(buttons);
    let widths = vec![
      Constraint::Percentage(33),
      Constraint::Percentage(33),
      Constraint::Percentage(33),
    ];
    let headers = vec![
      "Username".to_string(),
      "Groups".to_string(),
      "Use Home Manager".to_string(),
    ];
    let mut rows: Vec<Vec<String>> = users.into_iter().map(|u| u.as_table_row()).collect();
    rows.insert(0, vec!["Add a new user".into(), "".into()]);
    let mut user_table = TableWidget::new("Users", widths, headers, rows);
    user_table.focus();
    let help_content = styled_block(vec![
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "↑/↓, j/k",
        ),
        (None, " - Navigate user list"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Enter →, l",
        ),
        (None, " - Add new user or edit selected user"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Tab",
        ),
        (None, " - Switch between user list and buttons"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Esc, q, ←, h",
        ),
        (None, " - Return to main menu"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "?",
        ),
        (None, " - Show this help"),
      ],
      vec![(None, "")],
      vec![(None, "Create user accounts for your NixOS system.")],
      vec![(
        None,
        "Select 'Add a new user' to create accounts, or select",
      )],
      vec![(None, "an existing user to modify their settings.")],
    ]);
    let help_modal = HelpModal::new("User Accounts", help_content);
    Self {
      user_table,
      buttons,
      help_modal,
    }
  }
  pub fn display_widget(installer: &mut Installer) -> Option<Box<dyn ConfigWidget>> {
    let users = installer.users.clone();
    if users.is_empty() {
      return None;
    }
    Some(Box::new(TableWidget::new(
      "Users",
      vec![
        Constraint::Percentage(33),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
      ],
      vec![
        "Username".to_string(),
        "Groups".to_string(),
        "Use Home Manager".to_string(),
      ],
      users.into_iter().map(|u| u.as_table_row()).collect(),
    )))
  }
  pub fn page_info<'a>() -> (String, Vec<Line<'a>>) {
    let title = "User Accounts".to_string();
    let description = vec![Line::from("Manage user accounts for the system.")];
    (title, description)
  }
}

impl Page for UserAccounts {
  fn render(
    &mut self,
    installer: &mut super::Installer,
    f: &mut ratatui::Frame,
    area: ratatui::prelude::Rect,
  ) {
    let chunks = split_vert!(
      area,
      1,
      [Constraint::Percentage(60), Constraint::Percentage(40),]
    );
    let mut rows: Vec<Vec<String>> = installer
      .users
      .clone()
      .into_iter()
      .map(|u| u.as_table_row())
      .collect();
    rows.insert(0, vec!["Add a new user".into(), "".into()]);
    self.user_table.set_rows(rows);
    self.user_table.fix_selection();
    self.user_table.render(f, chunks[0]);
    self.buttons.render(f, chunks[1]);

    // Render help modal on top
    self.help_modal.render(f, area);
  }

  fn handle_input(
    &mut self,
    installer: &mut super::Installer,
    event: ratatui::crossterm::event::KeyEvent,
  ) -> Signal {
    match event.code {
      KeyCode::Char('?') => {
        self.help_modal.toggle();
        return Signal::Wait;
      }
      ui_close!() if self.help_modal.visible => {
        self.help_modal.hide();
        return Signal::Wait;
      }
      _ if self.help_modal.visible => {
        return Signal::Wait;
      }
      _ => {}
    }

    if self.user_table.is_focused() {
      match event.code {
        ui_down!() => {
          if !self.user_table.next_row() {
            self.user_table.unfocus();
            self.buttons.focus();
            self.buttons.first_child();
          }
          Signal::Wait
        }
        ui_up!() => {
          if !self.user_table.previous_row() {
            self.user_table.unfocus();
            self.buttons.focus();
            self.buttons.last_child();
          }
          Signal::Wait
        }
        ui_enter!() => {
          let Some(selected_user) = self.user_table.selected_row() else {
            return Signal::Error(anyhow::anyhow!("No user selected"));
          };
          if selected_user == 0 {
            // Add a new user
            Signal::Push(Box::new(AddUser::new()))
          } else {
            let groups = installer
              .users
              .get(selected_user - 1)
              .map(|u| u.groups.clone())
              .unwrap_or_default();
            Signal::Push(Box::new(AlterUser::new(selected_user - 1, groups)))
          }
        }
        ui_back!() => Signal::Pop,
        _ => Signal::Wait,
      }
    } else if self.buttons.is_focused() {
      match event.code {
        ui_down!() => {
          if !self.buttons.next_child() {
            self.buttons.unfocus();
            self.user_table.focus();
            self.user_table.first_row();
          }
          Signal::Wait
        }
        ui_up!() => {
          if !self.buttons.prev_child() {
            self.buttons.unfocus();
            self.user_table.focus();
            self.user_table.last_row();
          }
          Signal::Wait
        }
        ui_enter!() => {
          match self.buttons.selected_child() {
            Some(0) => {
              // Back
              Signal::Pop
            }
            _ => Signal::Wait,
          }
        }
        ui_back!() => Signal::Pop,
        _ => Signal::Wait,
      }
    } else {
      self.buttons.focus();
      Signal::Wait
    }
  }

  fn get_help_content(&self) -> (String, Vec<Line<'_>>) {
    let help_content = styled_block(vec![
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "↑/↓, j/k",
        ),
        (None, " - Navigate user list"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Enter, →, j",
        ),
        (None, " - Add new user or edit selected user"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Tab",
        ),
        (None, " - Switch between user list and buttons"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Esc, q, ←, h",
        ),
        (None, " - Return to main menu"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "?",
        ),
        (None, " - Show this help"),
      ],
      vec![(None, "")],
      vec![(None, "Create user accounts for your NixOS system.")],
      vec![(
        None,
        "Select 'Add a new user' to create accounts, or select",
      )],
      vec![(None, "an existing user to modify their settings.")],
    ]);
    ("User Accounts".to_string(), help_content)
  }
}

pub struct AddUser {
  name_input: LineEditor,
  pass_input: LineEditor,
  pass_confirm: LineEditor,
  help_modal: HelpModal<'static>,

  username: Option<String>,
}

impl AddUser {
  pub fn new() -> Self {
    let mut name_input = LineEditor::new("Username", None::<&str>);
    name_input.focus();
    let help_content = styled_block(vec![
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Tab",
        ),
        (None, " - Move to next field"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Shift+Tab",
        ),
        (None, " - Move to previous field"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Enter",
        ),
        (None, " - Create user account"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Esc",
        ),
        (None, " - Cancel and return"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "←/→",
        ),
        (None, " - Move cursor in text field"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Home/End",
        ),
        (None, " - Jump to field beginning/end"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "?",
        ),
        (None, " - Show this help"),
      ],
      vec![(None, "")],
      vec![(None, "Create a new user account for your NixOS system.")],
      vec![(None, "Enter username, password, and confirm password.")],
      vec![(None, "Passwords are hidden during entry for security.")],
    ]);
    let help_modal = HelpModal::new("Add User", help_content);
    Self {
      name_input,
      pass_input: LineEditor::new("Password", None::<&str>).secret(true),
      pass_confirm: LineEditor::new("Confirm Password", None::<&str>).secret(true),
      help_modal,
      username: None,
    }
  }
  pub fn cycle_forward(&mut self) {
    // Tab was pressed
    if self.name_input.is_focused() {
      self.name_input.unfocus();
      self.pass_input.focus();
    } else if self.pass_input.is_focused() {
      self.pass_input.unfocus();
      self.pass_confirm.focus();
    } else if self.pass_confirm.is_focused() {
      self.pass_confirm.unfocus();
      self.name_input.focus();
    } else {
      self.name_input.focus();
    }
  }
  pub fn cycle_backward(&mut self) {
    // Shift+Tab was pressed
    if self.name_input.is_focused() {
      self.name_input.unfocus();
      self.pass_confirm.focus();
    } else if self.pass_input.is_focused() {
      self.pass_input.unfocus();
      self.name_input.focus();
    } else if self.pass_confirm.is_focused() {
      self.pass_confirm.unfocus();
      self.pass_input.focus();
    } else {
      self.name_input.focus();
    }
  }
}

impl Default for AddUser {
  fn default() -> Self {
    Self::new()
  }
}

impl Page for AddUser {
  fn render(
    &mut self,
    _installer: &mut super::Installer,
    f: &mut ratatui::Frame,
    area: ratatui::prelude::Rect,
  ) {
    let hor_chunks = split_hor!(
      area,
      1,
      [
        Constraint::Percentage(25),
        Constraint::Percentage(50),
        Constraint::Percentage(25),
      ]
    );
    let chunks = split_vert!(
      hor_chunks[1],
      1,
      [
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(0),
      ]
    );
    self.name_input.render(f, chunks[0]);
    self.pass_input.render(f, chunks[1]);
    self.pass_confirm.render(f, chunks[2]);

    // Render help modal on top
    self.help_modal.render(f, area);
  }

  fn handle_input(
    &mut self,
    installer: &mut super::Installer,
    event: ratatui::crossterm::event::KeyEvent,
  ) -> Signal {
    match event.code {
      KeyCode::Char('?') => {
        self.help_modal.toggle();
        return Signal::Wait;
      }
      ui_close!() if self.help_modal.visible => {
        self.help_modal.hide();
        return Signal::Wait;
      }
      _ if self.help_modal.visible => {
        return Signal::Wait;
      }
      _ => {}
    }

    if event.code == KeyCode::Tab {
      self.cycle_forward();
      return Signal::Wait;
    } else if event.code == KeyCode::BackTab {
      self.cycle_backward();
      return Signal::Wait;
    }

    if self.name_input.is_focused() {
      match event.code {
        KeyCode::Enter => {
          if let Some(name) = self.name_input.get_value() {
            let Some(name) = name.as_str() else {
              self.name_input.error("Username cannot be empty");
              return Signal::Wait;
            };
            if name.is_empty() {
              self.name_input.error("Username cannot be empty");
              return Signal::Wait;
            }
            self.username = Some(name.to_string());
            self.name_input.unfocus();
            self.pass_input.focus();
            Signal::Wait
          } else {
            self.name_input.error("Username cannot be empty");
            Signal::Wait
          }
        }
        KeyCode::Esc => Signal::Pop,
        _ => self.name_input.handle_input(event),
      }
    } else if self.pass_input.is_focused() {
      match event.code {
        KeyCode::Enter => {
          if let Some(pass) = self.pass_input.get_value() {
            let Some(pass) = pass.as_str() else {
              self.pass_input.error("Password cannot be empty");
              return Signal::Wait;
            };
            if pass.is_empty() {
              self.pass_input.error("Password cannot be empty");
              return Signal::Wait;
            }
            self.pass_input.unfocus();
            self.pass_confirm.focus();
            Signal::Wait
          } else {
            self.pass_input.error("Password cannot be empty");
            Signal::Wait
          }
        }
        _ => self.pass_input.handle_input(event),
      }
    } else if self.pass_confirm.is_focused() {
      match event.code {
        KeyCode::Enter => {
          if let Some(pass) = self.pass_input.get_value() {
            let Some(pass) = pass.as_str() else {
              self.pass_input.error("Password cannot be empty");
              return Signal::Wait;
            };
            if let Some(confirm) = self.pass_confirm.get_value() {
              let Some(confirm) = confirm.as_str() else {
                self
                  .pass_confirm
                  .error("Password confirmation cannot be empty");
                return Signal::Wait;
              };
              if pass != confirm {
                self.pass_confirm.error("Passwords do not match");
                self.pass_input.clear();
                self.pass_confirm.clear();
                return Signal::Wait;
              }
              let hashed = match super::RootPassword::mkpasswd(pass.to_string()) {
                Ok(h) => h,
                Err(e) => {
                  return Signal::Error(anyhow::anyhow!("Failed to hash password: {}", e));
                }
              };

              installer.users.push(User {
                username: self.username.clone().unwrap_or_default(),
                password_hash: hashed,
                groups: vec![],
                home_manager_cfg: None,
              });
              Signal::Pop
            } else {
              self
                .pass_confirm
                .error("Password confirmation cannot be empty");
              return Signal::Wait;
            }
          } else {
            self.pass_input.error("Password cannot be empty");
            return Signal::Wait;
          }
        }
        _ => self.pass_confirm.handle_input(event),
      }
    } else {
      self.name_input.focus();
      Signal::Wait
    }
  }

  fn get_help_content(&self) -> (String, Vec<Line<'_>>) {
    let help_content = styled_block(vec![
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Tab",
        ),
        (None, " - Move to next field"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Shift+Tab",
        ),
        (None, " - Move to previous field"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Enter",
        ),
        (None, " - Create user account"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Esc",
        ),
        (None, " - Cancel and return"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "←/→",
        ),
        (None, " - Move cursor in text field"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Home/End",
        ),
        (None, " - Jump to field beginning/end"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "?",
        ),
        (None, " - Show this help"),
      ],
      vec![(None, "")],
      vec![(None, "Create a new user account for your NixOS system.")],
      vec![(None, "Enter username, password, and confirm password.")],
      vec![(None, "Passwords are hidden during entry for security.")],
    ]);
    ("Add User".to_string(), help_content)
  }
}

pub struct AlterUser {
  pub selected_user: usize,

  /// Main menu options
  pub buttons: WidgetBox,

  /// Name change
  pub name_input: LineEditor,

  /// Password change
  pub pass_input: LineEditor,
  pub pass_confirm: LineEditor,

  /// Group Editor
  pub group_name_input: LineEditor,
  pub group_list: StrList,
  help_modal: HelpModal<'static>,
  confirming_delete: bool,
}

impl AlterUser {
  pub fn new(selected_user_idx: usize, groups: Vec<String>) -> Self {
    let buttons = vec![
      Box::new(Button::new("Change username")) as Box<dyn ConfigWidget>,
      Box::new(Button::new("Change password")) as Box<dyn ConfigWidget>,
      Box::new(Button::new("Edit Groups")) as Box<dyn ConfigWidget>,
      Box::new(Button::new("Configure Home Manager")) as Box<dyn ConfigWidget>,
      Box::new(Button::new("Delete user")) as Box<dyn ConfigWidget>,
    ];
    let mut buttons = WidgetBox::button_menu(buttons);
    buttons.focus();
    let help_content = styled_block(vec![
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "↑/↓, j/k",
        ),
        (None, " - Navigate menu options"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Enter, →, l",
        ),
        (None, " - Select option"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Tab",
        ),
        (None, " - Navigate between fields"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Esc, q, ←, h",
        ),
        (None, " - Return to previous menu"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "?",
        ),
        (None, " - Show this help"),
      ],
      vec![(None, "")],
      vec![(
        None,
        "Modify an existing user account. Choose from changing",
      )],
      vec![(None, "username, password, groups, or deleting the user.")],
    ]);
    let help_modal = HelpModal::new("Alter User", help_content);
    Self {
      selected_user: selected_user_idx,
      buttons,
      name_input: LineEditor::new("New username", None::<&str>),
      pass_input: LineEditor::new("New password", None::<&str>).secret(true),
      pass_confirm: LineEditor::new("Confirm password", None::<&str>).secret(true),
      group_name_input: LineEditor::new("Add group", None::<&str>),
      group_list: StrList::new("Groups", groups),
      help_modal,
      confirming_delete: false,
    }
  }
  pub fn render_main_menu(&mut self, f: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
    let vert_chunks = split_vert!(
      area,
      1,
      [Constraint::Percentage(50), Constraint::Percentage(50)]
    );
    let hor_chunks = split_hor!(
      vert_chunks[0],
      1,
      [
        Constraint::Percentage(40),
        Constraint::Percentage(20),
        Constraint::Percentage(40),
      ]
    );
    self.buttons.render(f, hor_chunks[1]);
  }
  pub fn render_name_change(&mut self, f: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
    let chunks = split_vert!(area, 1, [Constraint::Length(5), Constraint::Min(0)]);
    let hor_chunks = split_hor!(
      chunks[0],
      1,
      [
        Constraint::Percentage(25),
        Constraint::Percentage(50),
        Constraint::Percentage(25),
      ]
    );
    self.name_input.render(f, hor_chunks[1]);
  }
  pub fn render_pass_change(&mut self, f: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
    let chunks = split_vert!(
      area,
      1,
      [
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Min(0),
      ]
    );
    let hor_chunks1 = split_hor!(
      chunks[0],
      1,
      [
        Constraint::Percentage(25),
        Constraint::Percentage(50),
        Constraint::Percentage(25),
      ]
    );
    let hor_chunks2 = split_hor!(
      chunks[1],
      1,
      [
        Constraint::Percentage(25),
        Constraint::Percentage(50),
        Constraint::Percentage(25),
      ]
    );
    self.pass_input.render(f, hor_chunks1[1]);
    self.pass_confirm.render(f, hor_chunks2[1]);
  }
  pub fn render_edit_groups(
    &mut self,
    _installer: &mut super::Installer,
    f: &mut ratatui::Frame,
    area: ratatui::prelude::Rect,
  ) {
    let hor_chunks = split_hor!(
      area,
      1,
      [Constraint::Percentage(50), Constraint::Percentage(50)]
    );
    let line_editor_chunks = split_vert!(
      hor_chunks[0],
      1,
      [
        Constraint::Length(5),
        Constraint::Percentage(80),
        Constraint::Min(7),
      ]
    );
    let help_box = InfoBox::new(
      "Help",
      styled_block(vec![
        vec![
          (None, "Use "),
          (HIGHLIGHT, "tab "),
          (None, "to switch between new group input and group list"),
        ],
        vec![
          (None, "Pressing "),
          (HIGHLIGHT, "enter "),
          (None, "on an existing group will delete it."),
        ],
        vec![
          (None, "Adding the '"),
          (HIGHLIGHT, "wheel"),
          (None, "' group enables the use of "),
          (HIGHLIGHT, "sudo"),
          (None, "."),
        ],
      ]),
    );
    self.group_name_input.render(f, line_editor_chunks[0]);
    help_box.render(f, line_editor_chunks[2]);
    self.group_list.render(f, hor_chunks[1]);
  }
  pub fn handle_input_main_menu(
    &mut self,
    installer: &mut super::Installer,
    event: ratatui::crossterm::event::KeyEvent,
  ) -> Signal {
    if self.confirming_delete && event.code != KeyCode::Enter {
      self.confirming_delete = false;
      let buttons = vec![
        Box::new(Button::new("Change username")) as Box<dyn ConfigWidget>,
        Box::new(Button::new("Change password")) as Box<dyn ConfigWidget>,
        Box::new(Button::new("Edit Groups")) as Box<dyn ConfigWidget>,
        Box::new(Button::new("Configure Home Manager")) as Box<dyn ConfigWidget>,
        Box::new(Button::new("Delete user")) as Box<dyn ConfigWidget>,
      ];
      self.buttons.set_children_inplace(buttons);
    }
    match event.code {
      ui_down!() => {
        if !self.buttons.next_child() {
          self.buttons.first_child();
        }
        Signal::Wait
      }
      ui_up!() => {
        if !self.buttons.prev_child() {
          self.buttons.last_child();
        }
        Signal::Wait
      }
      KeyCode::Enter => {
        match self.buttons.selected_child() {
          Some(0) => {
            // Change username
            self.buttons.unfocus();
            self.name_input.focus();
            Signal::Wait
          }
          Some(1) => {
            // Change password
            self.buttons.unfocus();
            self.pass_input.focus();
            Signal::Wait
          }
          Some(2) => {
            // Edit groups
            self.buttons.unfocus();
            self.group_name_input.focus();
            Signal::Wait
          }
          Some(3) => {
            let existing_config = installer
              .users
              .get(self.selected_user)
              .and_then(|user| user.home_manager_cfg.clone());
            Signal::Push(Box::new(ConfigureHomeManager::new(
              self.selected_user,
              existing_config,
            )))
          }
          Some(4) => {
            // Delete user
            if !self.confirming_delete {
              self.confirming_delete = true;
              let buttons = vec![
                Box::new(Button::new("Change username")) as Box<dyn ConfigWidget>,
                Box::new(Button::new("Change password")) as Box<dyn ConfigWidget>,
                Box::new(Button::new("Edit Groups")) as Box<dyn ConfigWidget>,
                Box::new(Button::new("Configure Home Manager")) as Box<dyn ConfigWidget>,
                Box::new(Button::new("Really?")) as Box<dyn ConfigWidget>,
              ];
              self.buttons.set_children_inplace(buttons);
              Signal::Wait
            } else {
              if self.selected_user < installer.users.len() {
                installer.users.remove(self.selected_user);
              }
              Signal::Pop
            }
          }
          _ => Signal::Wait,
        }
      }
      ui_back!() => Signal::Pop,
      _ => Signal::Wait,
    }
  }
  pub fn handle_input_name_change(
    &mut self,
    installer: &mut super::Installer,
    event: ratatui::crossterm::event::KeyEvent,
  ) -> Signal {
    match event.code {
      KeyCode::Enter => {
        if let Some(name) = self.name_input.get_value() {
          let Some(name) = name.as_str() else {
            self.name_input.error("Username cannot be empty");
            return Signal::Wait;
          };
          if name.is_empty() {
            self.name_input.error("Username cannot be empty");
            return Signal::Wait;
          }
          if self.selected_user < installer.users.len() {
            installer.users[self.selected_user].username = name.to_string();
          }
          self.name_input.unfocus();
          self.buttons.focus();
          Signal::Wait
        } else {
          self.name_input.error("Username cannot be empty");
          Signal::Wait
        }
      }
      ui_close!() => {
        self.name_input.unfocus();
        self.buttons.focus();
        Signal::Wait
      }
      _ => self.name_input.handle_input(event),
    }
  }
  pub fn handle_input_pass_change(
    &mut self,
    installer: &mut super::Installer,
    event: ratatui::crossterm::event::KeyEvent,
  ) -> Signal {
    if self.pass_input.is_focused() {
      match event.code {
        KeyCode::Tab => {
          self.pass_input.unfocus();
          self.pass_confirm.focus();
          Signal::Wait
        }
        KeyCode::Enter => {
          if let Some(pass) = self.pass_input.get_value() {
            let Some(pass) = pass.as_str() else {
              self.pass_input.error("Password cannot be empty");
              return Signal::Wait;
            };
            if pass.is_empty() {
              self.pass_input.error("Password cannot be empty");
              return Signal::Wait;
            }
            self.pass_input.unfocus();
            self.pass_confirm.focus();
            Signal::Wait
          } else {
            self.pass_input.error("Password cannot be empty");
            Signal::Wait
          }
        }
        KeyCode::Esc => {
          self.pass_input.unfocus();
          self.buttons.focus();
          Signal::Wait
        }
        _ => self.pass_input.handle_input(event),
      }
    } else if self.pass_confirm.is_focused() {
      match event.code {
        KeyCode::Tab => {
          self.pass_confirm.unfocus();
          self.pass_input.focus();
          Signal::Wait
        }
        KeyCode::Esc => {
          self.pass_confirm.unfocus();
          self.buttons.focus();
          Signal::Wait
        }
        KeyCode::Enter => {
          if let Some(pass) = self.pass_input.get_value() {
            let Some(pass) = pass.as_str() else {
              self.pass_input.error("Password cannot be empty");
              return Signal::Wait;
            };
            if let Some(confirm) = self.pass_confirm.get_value() {
              let Some(confirm) = confirm.as_str() else {
                self
                  .pass_confirm
                  .error("Password confirmation cannot be empty");
                return Signal::Wait;
              };
              if pass != confirm {
                self.pass_confirm.error("Passwords do not match");
                self.pass_input.clear();
                self.pass_confirm.clear();
                return Signal::Wait;
              }
              let hashed = match super::RootPassword::mkpasswd(pass.to_string()) {
                Ok(h) => h,
                Err(e) => {
                  return Signal::Error(anyhow::anyhow!("Failed to hash password: {}", e));
                }
              };
              if self.selected_user < installer.users.len() {
                installer.users[self.selected_user].password_hash = hashed;
              }
              self.pass_confirm.unfocus();
              self.buttons.focus();
              Signal::Wait
            } else {
              self
                .pass_confirm
                .error("Password confirmation cannot be empty");
              return Signal::Wait;
            }
          } else {
            self.pass_input.error("Password cannot be empty");
            return Signal::Wait;
          }
        }
        _ => self.pass_confirm.handle_input(event),
      }
    } else {
      self.pass_input.focus();
      Signal::Wait
    }
  }
  pub fn handle_input_edit_groups(
    &mut self,
    installer: &mut super::Installer,
    event: ratatui::crossterm::event::KeyEvent,
  ) -> Signal {
    if self.group_name_input.is_focused() {
      match event.code {
        KeyCode::Enter => {
          if let Some(group) = self.group_name_input.get_value() {
            let Some(group) = group.as_str() else {
              self.group_name_input.error("Group name cannot be empty");
              return Signal::Wait;
            };
            if group.is_empty() {
              self.group_name_input.error("Group name cannot be empty");
              return Signal::Wait;
            }
            if self.selected_user < installer.users.len() {
              let user = &mut installer.users[self.selected_user];
              if !user.groups.contains(&group.to_string()) {
                user.groups.push(group.to_string());
              } else {
                self.group_name_input.error("User already in group");
                return Signal::Wait;
              }
            }
            self.group_name_input.clear();
            self
              .group_list
              .set_items(installer.users[self.selected_user].groups.clone());
            Signal::Wait
          } else {
            self.group_name_input.error("Group name cannot be empty");
            Signal::Wait
          }
        }
        KeyCode::Tab => {
          if !self.group_list.is_empty() {
            self.group_name_input.unfocus();
            self.group_list.focus();
          }
          Signal::Wait
        }
        KeyCode::Esc => {
          self.group_name_input.unfocus();
          self.buttons.focus();
          Signal::Wait
        }
        _ => self.group_name_input.handle_input(event),
      }
    } else if self.group_list.is_focused() {
      // Enter deletes items from the list
      match event.code {
        ui_down!() => {
          if !self.group_list.next_item() {
            self.group_list.first_item();
          }
          Signal::Wait
        }
        ui_up!() => {
          if !self.group_list.previous_item() {
            self.group_list.last_item();
          }
          Signal::Wait
        }
        KeyCode::Enter => {
          if let Some(selected) = self.group_list.selected_item() {
            if self.selected_user < installer.users.len() {
              let user = &mut installer.users[self.selected_user];
              user.groups.retain(|g| g != selected);
              self.group_list.set_items(user.groups.clone());
            }
          }

          if self.group_list.is_empty() {
            self.group_list.unfocus();
            self.group_name_input.focus();
          }
          Signal::Wait
        }
        KeyCode::Tab => {
          self.group_list.unfocus();
          self.group_name_input.focus();
          Signal::Wait
        }
        ui_close!() => {
          self.group_list.unfocus();
          self.buttons.focus();
          Signal::Wait
        }
        _ => Signal::Wait,
      }
    } else {
      self.group_name_input.focus();
      Signal::Wait
    }
  }
}

impl Page for AlterUser {
  fn render(
    &mut self,
    installer: &mut super::Installer,
    f: &mut ratatui::Frame,
    area: ratatui::prelude::Rect,
  ) {
    if self.buttons.is_focused() {
      self.render_main_menu(f, area);
    } else if self.name_input.is_focused() {
      self.render_name_change(f, area);
    } else if self.pass_input.is_focused() || self.pass_confirm.is_focused() {
      self.render_pass_change(f, area);
    } else if self.group_name_input.is_focused() || self.group_list.is_focused() {
      self.render_edit_groups(installer, f, area);
    } else {
      self.buttons.focus();
      self.render_main_menu(f, area);
    }

    // Render help modal on top
    self.help_modal.render(f, area);
  }

  fn handle_input(
    &mut self,
    installer: &mut super::Installer,
    event: ratatui::crossterm::event::KeyEvent,
  ) -> Signal {
    match event.code {
      KeyCode::Char('?') => {
        self.help_modal.toggle();
        return Signal::Wait;
      }
      ui_close!() if self.help_modal.visible => {
        self.help_modal.hide();
        return Signal::Wait;
      }
      _ if self.help_modal.visible => {
        return Signal::Wait;
      }
      _ => {}
    }

    if self.buttons.is_focused() {
      self.handle_input_main_menu(installer, event)
    } else if self.name_input.is_focused() {
      self.handle_input_name_change(installer, event)
    } else if self.pass_input.is_focused() || self.pass_confirm.is_focused() {
      self.handle_input_pass_change(installer, event)
    } else if self.group_name_input.is_focused() || self.group_list.is_focused() {
      self.handle_input_edit_groups(installer, event)
    } else {
      self.buttons.focus();
      Signal::Wait
    }
  }

  fn get_help_content(&self) -> (String, Vec<Line<'_>>) {
    let help_content = styled_block(vec![
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "↑/↓, j/k",
        ),
        (None, " - Navigate menu options"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Enter, →, l",
        ),
        (None, " - Select option"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Tab",
        ),
        (None, " - Navigate between fields"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "Esc, q, ←, h",
        ),
        (None, " - Return to previous menu"),
      ],
      vec![
        (
          Some((
            ratatui::style::Color::Yellow,
            ratatui::style::Modifier::BOLD,
          )),
          "?",
        ),
        (None, " - Show this help"),
      ],
      vec![(None, "")],
      vec![(
        None,
        "Modify an existing user account. Choose from changing",
      )],
      vec![(None, "username, password, groups, or deleting the user.")],
    ]);
    ("Alter User".to_string(), help_content)
  }
}

pub struct ConfigureHomeManager {
  pub confirmed: bool,
  pub picking_pkgs: bool,
  pub confirm_buttons: WidgetBox,
  pub configuration_options: WidgetBox,
  pub package_picker: PackagePicker,
  pub selected_user: usize,
  pub confirming_disable: bool,
}

impl ConfigureHomeManager {
  pub fn new(selected_user: usize, existing_config: Option<HomeManagerCfg>) -> Self {
    let buttons = vec![
      Box::new(Button::new("Yes")) as Box<dyn ConfigWidget>,
      Box::new(Button::new("No")) as Box<dyn ConfigWidget>,
    ];
    let config_options = vec![
      Box::new(Button::new("Configure User Packages")) as Box<dyn ConfigWidget>,
      Box::new(Button::new("Disable Home Manager")) as Box<dyn ConfigWidget>,
    ];

    let mut confirm_buttons = WidgetBox::button_menu(buttons);
    let mut configuration_options = WidgetBox::button_menu(config_options);
    if let Some(cfg) = existing_config {
      configuration_options.focus();
      let pkgs = get_available_pkgs().unwrap_or_default();
      let selected_pkgs = cfg.packages.clone();
      let package_picker = PackagePicker::new(
        "Selected User Packages",
        "Available Packages",
        selected_pkgs,
        pkgs,
      );
      Self {
        confirmed: true,
        picking_pkgs: false,
        confirm_buttons,
        configuration_options,
        package_picker,
        selected_user,
        confirming_disable: false,
      }
    } else {
      confirm_buttons.focus();
      let pkgs = get_available_pkgs().unwrap_or_default();
      let package_picker =
        PackagePicker::new("Selected User Packages", "Available Packages", vec![], pkgs);
      Self {
        confirmed: false,
        picking_pkgs: false,
        confirm_buttons,
        configuration_options,
        package_picker,
        selected_user,
        confirming_disable: false,
      }
    }
  }
}

impl Page for ConfigureHomeManager {
  fn render(
    &mut self,
    installer: &mut Installer,
    f: &mut ratatui::Frame,
    area: ratatui::prelude::Rect,
  ) {
    if !self.confirmed {
      let info_box = InfoBox::new(
        "Configure Home Manager",
        styled_block(vec![
          vec![(None, "Configure Home Manager for this user?")],
          vec![
            (None, "This will set up a "),
            (HIGHLIGHT, "home manager "),
            (None, "configuration for the user in the "),
            (HIGHLIGHT, "configuration.nix "),
            (None, "generated by this installer."),
          ],
          vec![
            (HIGHLIGHT, "Home Manager"),
            (
              None,
              " allows you to declaratively manage your user environment using ",
            ),
            (HIGHLIGHT, "Nix"),
            (None, "."),
          ],
        ]),
      );
      let vert_chunks = split_vert!(
        area,
        1,
        [Constraint::Percentage(70), Constraint::Percentage(30)]
      );
      let hor_chunks = split_hor!(
        vert_chunks[1],
        1,
        [
          Constraint::Percentage(40),
          Constraint::Percentage(20),
          Constraint::Percentage(40),
        ]
      );

      info_box.render(f, vert_chunks[0]);
      self.confirm_buttons.render(f, hor_chunks[1]);
    } else if self.picking_pkgs {
      self.package_picker.render(f, area);
    } else {
      let table = installer.users.get(self.selected_user).map(|user| {
        let pkgs = user
          .home_manager_cfg
          .as_ref()
          .map(|cfg| cfg.packages.clone())
          .unwrap_or_default()
          .into_iter()
          .map(|pkg| vec![pkg])
          .collect();
        TableWidget::new(
          "",
          vec![Constraint::Percentage(100)],
          vec!["User Packages".into()],
          pkgs,
        )
      });
      let vert_chunks = split_vert!(
        area,
        1,
        [Constraint::Percentage(50), Constraint::Percentage(50)]
      );
      let hor_chunks = split_hor!(
        vert_chunks[0],
        1,
        [
          Constraint::Percentage(40),
          Constraint::Percentage(20),
          Constraint::Percentage(40),
        ]
      );
      self.configuration_options.render(f, hor_chunks[1]);
      table.unwrap().render(f, vert_chunks[1]);
    }
  }
  fn handle_input(
    &mut self,
    installer: &mut Installer,
    event: ratatui::crossterm::event::KeyEvent,
  ) -> Signal {
    if !self.confirmed {
      match event.code {
        ui_down!() => {
          if !self.confirm_buttons.next_child() {
            self.confirm_buttons.first_child();
          }
          Signal::Wait
        }
        ui_up!() => {
          if !self.confirm_buttons.prev_child() {
            self.confirm_buttons.last_child();
          }
          Signal::Wait
        }
        KeyCode::Enter => {
          match self.confirm_buttons.selected_child() {
            Some(0) => {
              // Yes
              self.confirmed = true;
              self.configuration_options.focus();
              if self.selected_user < installer.users.len()
                && installer.users[self.selected_user]
                  .home_manager_cfg
                  .is_none()
              {
                installer.users[self.selected_user].home_manager_cfg =
                  Some(HomeManagerCfg { packages: vec![] });
              }
              Signal::Wait
            }
            Some(1) => {
              // No
              if self.selected_user < installer.users.len() {
                installer.users[self.selected_user].home_manager_cfg = None;
              }
              Signal::Pop
            }
            _ => Signal::Wait,
          }
        }
        _ => Signal::Wait,
      }
    } else if self.picking_pkgs {
      match event.code {
        ui_close!() => {
          if self.package_picker.search_bar.is_focused() {
            self.package_picker.handle_input(event)
          } else {
            let selected = self.package_picker.get_selected_packages();
            if let Some(user) = installer.users.get_mut(self.selected_user) {
              if let Some(cfg) = user.home_manager_cfg.as_mut() {
                cfg.packages = selected;
              }
            }
            self.picking_pkgs = false;
            Signal::Wait
          }
        }
        _ => self.package_picker.handle_input(event),
      }
    } else {
      if self.confirming_disable && event.code != KeyCode::Enter {
        self.confirming_disable = false;
        let config_options = vec![
          Box::new(Button::new("Configure User Packages")) as Box<dyn ConfigWidget>,
          Box::new(Button::new("Disable Home Manager")) as Box<dyn ConfigWidget>,
        ];
        self
          .configuration_options
          .set_children_inplace(config_options);
      }
      match event.code {
        ui_down!() => {
          if !self.configuration_options.next_child() {
            self.configuration_options.first_child();
          }
          Signal::Wait
        }
        ui_up!() => {
          if !self.configuration_options.prev_child() {
            self.configuration_options.last_child();
          }
          Signal::Wait
        }
        ui_close!() => Signal::Pop,
        KeyCode::Enter => {
          match self.configuration_options.selected_child() {
            Some(0) => {
              // Configure User Packages
              self.picking_pkgs = true;
              Signal::Wait
            }
            Some(1) => {
              // Disable Home Manager
              if !self.confirming_disable {
                self.confirming_disable = true;
                let config_options = vec![
                  Box::new(Button::new("Configure User Packages")) as Box<dyn ConfigWidget>,
                  Box::new(Button::new("Really?")) as Box<dyn ConfigWidget>,
                ];
                self
                  .configuration_options
                  .set_children_inplace(config_options);
                Signal::Wait
              } else {
                if self.selected_user < installer.users.len() {
                  installer.users[self.selected_user].home_manager_cfg = None;
                }
                Signal::Pop
              }
            }
            _ => Signal::Wait,
          }
        }
        _ => Signal::Wait,
      }
    }
  }
}
