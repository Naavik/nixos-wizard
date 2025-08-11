# Contribution Guidelines

This doc is designed to give a high level overview of how this codebase works, to make contributing to it easier.

## Setting up the dev environment

The flake included in the project root contains a dev shell which will give you all of the tools you need to work on the project. If you're on NixOS or have `nixpkgs` installed on your machine, you can just use
```bash
nix develop
```

If not, make sure you have cargo installed. Also, run cargo fmt before you make any commits please :)

## nixos-wizard Architecture Overview

The program itself has three core components: 

1. The event loop - manages current UI and installer state 
2. The `Page` trait - defines the main UI screens, essentially containers for widgets 
3. The `ConfigWidget` trait - re-usable UI components that make up pages 

### The event loop
The event loop contains a stack of `Box<dyn Page>`, and whenever a page is entered, that page is pushed onto the stack. Whenever a page is exited, that page is popped from the stack. Every iteration of the event loop does two things:
* Calls the `render()` method of the page on top of the stack
* Polls for user input, and if any is received, passes that input to the `handle_input()` method of the page on top of the stack.
The pages communicate with the event loop using the `Signal` enum. `Signal::Pop` makes the event loop pop from the page stack, for instance.

### The `Page` trait
The `Page` trait is the main interface used to define the different pages of the installer. The main methods of this trait are `render()` and `handle_input()`. Each page is itself a collection of widgets, which each implement the `ConfigWidget` trait. Pages are navigated to by returning `Signal::Push(Box::new(<page>))` from the `handle_input()` method, which tells the event loop to push a new page onto the stack. Pages are navigated away from using `Signal::Pop`.

### The `ConfigWidget` trait
The `ConfigWidget` trait is the main interface used to define page components. Like `Page`, the `ConfigWidget` trait exposes `render()` and `handle_input()`. `handle_input()` is useful when input *must* be passed to the widget using the interface, like in the case of said widget being stored as a trait object. `render()` is usually given a chunk of the screen by it's `Page` to try to render inside of.

Generally speaking, inputs are caught and handled at the page level, as delegating all input to the individual widgets ends up fostering more presumptuous or general logic, where page-specific logic is generally more favorable in this case.

The trickiest part of setting up new `Page` or `ConfigWidget` structs is defining how they use the space that they are given in their respective `render()` methods. Take this for example:

```rust
impl Page for EnableFlakes {
  fn render(&mut self, _installer: &mut Installer, f: &mut Frame, area: Rect) {
    let chunks = Layout::default()
      .direction(Direction::Vertical)
      .margin(1)
      .constraints(
				[
					Constraint::Percentage(40),
					Constraint::Percentage(60)
				].as_ref()
			)
      .split(area);

    let hor_chunks = Layout::default()
      .direction(Direction::Horizontal)
      .margin(1)
      .constraints(
        [
          Constraint::Percentage(30),
          Constraint::Percentage(40),
          Constraint::Percentage(30),
        ]
        .as_ref(),
      )
      .split(chunks[1]);

    let info_box = InfoBox::new(
      "",
      ... info box content ...
    );
    info_box.render(f, chunks[0]);
    self.buttons.render(f, hor_chunks[1]);
    self.help_modal.render(f, area);
  }
...
```

This is the `render()` method of the "Enable Flakes" page. It cuts up the space given to it vertically first, and then horizontally.

The method uses Ratatui's `Layout` system to divide the terminal screen area into smaller rectangular chunks. First, it splits the available space vertically into two regions: the top 40% (for the `info_box`) and the bottom 60%. Then it subdivides the bottom 60% horizontally into three parts: 30%, 40%, and 30%. The middle horizontal chunk is used to render the `buttons` widget.

Each widget’s `render()` method is called with the frame and the specific chunk of the terminal space it should draw itself within. This way, each widget knows exactly how much space it has, and where it should be positioned on the screen.

This approach of dividing and subdividing the UI space using Ratatui’s layout tools allows pages to arrange their child widgets precisely and responsively, adapting to terminal size changes.
