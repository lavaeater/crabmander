# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Crabmander is a twin-pane TUI file manager written in Rust using Ratatui. It is built on the **nightly** Rust toolchain.

## Commands

```sh
cargo build                          # Build the project
cargo run                            # Run the app
cargo test --locked --all-features   # Run all tests
cargo fmt --all                      # Format code
cargo clippy --all-targets --all-features -- -D warnings  # Lint
```

Run a single test by name:
```sh
cargo test test_parse_style_foreground
```

## Architecture

The app follows a message-passing event loop pattern:

- **`tui.rs`** — Drives a background Tokio task that polls crossterm for terminal events (key, mouse, resize, etc.) and emits timed `Tick` and `Render` events. Wraps `ratatui::Terminal`.
- **`action.rs`** — Defines the `Action` enum: the single vocabulary of commands that flow through the app (`Quit`, `Tick`, `Render`, `Resize`, `Suspend`, `Resume`, `ClearScreen`, `Error`, `Help`).
- **`app.rs`** — The main event loop. Owns a `Vec<Box<dyn Component>>`, reads `Event`s from `Tui`, translates them into `Action`s via an `mpsc::unbounded_channel`, dispatches actions to all components, and renders each frame.
- **`components.rs`** — Defines the `Component` trait. Implementors receive events/actions and produce `Action`s in return. `draw()` and `update()` are the required methods; others have default no-op implementations.
- **`config.rs`** — Loads configuration from `~/.config/crabmander/` (XDG). Falls back to `.config/config.json5` baked in at compile time via `include_str!`. Supports json5, json, yaml, toml, and ini. Keybindings are per-`Mode` maps of key sequence strings to `Action`s. Styles are per-`Mode` maps of string keys to human-readable color/modifier strings (e.g. `"bold red on blue"`).
- **`cli.rs`** — CLI argument parsing via Clap (tick rate, frame rate flags).
- **`app::Mode`** — An enum that scopes keybindings and styles. Currently only `Home`.

### Adding a new component

1. Create `src/components/my_component.rs`, implement the `Component` trait.
2. Re-export from `src/components.rs`.
3. Add to the `components` vec in `App::new()`.

### Adding a new action

1. Add variant to `Action` in `action.rs`.
2. Handle it in `App::handle_actions()` and/or in relevant `Component::update()` implementations.

## Configuration

User config lives in `$CRABMANDER_CONFIG` or the XDG config dir (`~/.config/crabmander/`). Data dir is `$CRABMANDER_DATA` or the XDG data dir. Key sequences use the format `<ctrl-a>`, `<shift-esc>`, chained as `<g><g>` for multi-key combos.
