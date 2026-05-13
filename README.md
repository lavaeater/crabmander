# crabmander

[![CI](https://github.com/tommie-nygren/crabmander/workflows/CI/badge.svg)](https://github.com/tommie-nygren/crabmander/actions)

A twin-pane TUI file manager written in Rust using Ratatui, inspired by Norton Commander and Midnight Commander.

```
┌─ /home/user/documents ───────────────┬─ /home/user/downloads ───────────────┐
│ Name↑              Size   Date       │ Name↑              Size   Date        │
│ ..                 <DIR>             │ ..                 <DIR>              │
│ projects/          <DIR>  6d ago     │*archive.tar.gz    45.3M  2d ago      │
│▶ notes/            <DIR>  1d ago     │ image.png          2.1M  5d ago      │
│ todo.md            1.1K   3h ago     │ video.mp4          1.2G  6d ago      │
├──────────────────────────────────────┴──────────────────────────────────────┤
│ notes/  <DIR>                                          1 marked (45.3 MB)   │
├─────────────────────────────────────────────────────────────────────────────┤
│ F1 QuickCD  F2 Menu  F3 Nano  F4 Sizes  F5 Copy  F6 Move  F7 MkDir  F8 Del │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Installation

Requires the **nightly** Rust toolchain (pinned automatically via `rust-toolchain.toml`).

**From GitHub:**
```sh
cargo install --git https://github.com/tommie-nygren/crabmander
```

**A specific release tag:**
```sh
cargo install --git https://github.com/tommie-nygren/crabmander --tag v0.1.0
```

**Update to latest:**
```sh
cargo install --git https://github.com/tommie-nygren/crabmander --force
```

## Desktop shortcut

After installing, run once to add crabmander to your application menu:

```sh
crabmander --install-desktop-entry
```

This writes `~/.local/share/applications/crabmander.desktop` and calls
`update-desktop-database` automatically. It auto-detects your terminal emulator
(tries `alacritty`, `kitty`, `foot`, `xterm` in that order). Edit the file
afterward to hardcode your preferred terminal if needed.

## Keyboard reference

### Navigation

| Key               | Action                                                        |
|-------------------|---------------------------------------------------------------|
| `↑` / `k`         | Move cursor up                                                |
| `↓` / `j`         | Move cursor down                                              |
| `Page Up`         | Page up                                                       |
| `Page Down`       | Page down                                                     |
| `Home`            | Jump to first entry                                           |
| `End`             | Jump to last entry                                            |
| `Enter` / `→`     | Enter directory; open file with `xdg-open`                    |
| `Backspace` / `←` | Go to parent directory                                        |
| `Tab`             | Switch active panel                                           |
| `Shift+Tab`       | Make the other panel navigate to the active panel's directory |

### Marking files

Marks determine which files are acted on by Copy, Move, and Delete. If nothing
is marked, those operations act on the entry under the cursor.

| Key                | Action                                         |
|--------------------|------------------------------------------------|
| `Space` / `Insert` | Toggle mark on cursor entry and advance cursor |
| `*`                | Toggle mark on all entries in the panel        |

Marked files are shown in yellow with a `*` prefix. The status bar shows the
marked count and cumulative size.

### Function keys

| Key         | Action                                                                           |
|-------------|----------------------------------------------------------------------------------|
| `F1`        | **Quick CD** — incremental directory navigator (see below)                       |
| `F2`        | **Context menu** — file-type-aware actions (see below)                           |
| `F3`        | **Nano** — open a file in nano                                                   |
| `F4`        | **Sizes** — recursively calculate directory sizes; auto-sorts by size descending |
| `F5`        | **Copy** — copy marked/cursor files to the other panel's directory               |
| `F6`        | **Move** — move marked/cursor files to the other panel's directory               |
| `F7`        | **MkDir** — create a new directory in the active panel                           |
| `F8`        | **Delete** — delete marked/cursor files (with confirmation)                      |
| `F9`        | **Sort** — cycle sort column: Name → Size → Modified                             |
| `Shift+F9`  | **Invert sort** — toggle ascending ↑ / descending ↓                              |
| `F10` / `q` | Quit                                                                             |
| `Ctrl+Z`    | Suspend to background                                                            |

### Quick CD (F1)

Opens an incremental directory navigator. Start typing and the list filters in
real time.

| Key        | Action                                                      |
|------------|-------------------------------------------------------------|
| _any text_ | Filter directories; `~` expands to your home directory      |
| `↑` / `↓`  | Navigate the filtered list                                  |
| `Tab`      | Complete the selected entry into the input and drill deeper |
| `Enter`    | Navigate the active panel to the selected directory         |
| `Esc`      | Cancel                                                      |

**Examples:**

| Input            | Lists                                             |
|------------------|---------------------------------------------------|
| _(empty)_        | Subdirectories of the current panel               |
| `doc`            | Subdirectories whose name contains `doc`          |
| `~/pro`          | Subdirectories of `$HOME` containing `pro`        |
| `/usr/lo`        | Subdirectories of `/usr` containing `lo`          |
| `/home/` + `Tab` | Drills into `/home/` and lists its subdirectories |

### Context menu (F2)

Shows context-aware actions for the file under the cursor. Navigate with
`↑` / `↓`, confirm with `Enter`, cancel with `Esc`.

| File type                                     | Available actions                                       |
|-----------------------------------------------|---------------------------------------------------------|
| Any file                                      | Open with OS (`xdg-open`), Run VS Code here             |
| Archives (`.zip`, `.tar.*`, `.7z`, `.rar`, …) | Extract here, Extract to other panel                    |
| Executable files                              | Execute… (prompts for arguments, then runs in terminal) |

### Auto-filter

Typing any character that is not a keyboard shortcut activates a live filter
on the active panel. The panel narrows to entries whose names contain the typed
text (case-insensitive). The active filter is shown in a yellow bar inside the
panel border.

| Key             | Action                       |
|-----------------|------------------------------|
| _printable key_ | Append to filter             |
| `Backspace`     | Remove last filter character |
| `Esc`           | Clear filter                 |

The filter clears automatically when you navigate into a new directory.

### Sorting (F9 / Shift+F9)

The active sort column is indicated by an arrow in the column header
(`↑` ascending, `↓` descending). Directories are always listed before files.

- **F4** (Sizes) automatically switches to Size ↓ (largest first) and updates
  the sort live as directory sizes are computed in the background.
- Directories whose sizes are not yet known (shown as `···`) always appear at
  the bottom of the directory group until computed.

## Configuration

The default configuration is compiled into the binary. To customise keybindings
or styles, create a config file in your platform config directory (shown in
`crabmander --version`) with one of these names:

```
config.json5 | config.json | config.yaml | config.toml | config.ini
```

Override the config or data directory with environment variables:

```sh
CRABMANDER_CONFIG=~/.config/crabmander crabmander
CRABMANDER_DATA=~/.local/share/crabmander crabmander
```

### Key sequence syntax

| Syntax       | Meaning               |
|--------------|-----------------------|
| `<q>`        | The `q` key           |
| `<ctrl-a>`   | Ctrl+A                |
| `<shift-f9>` | Shift+F9              |
| `<g><g>`     | Two-key sequence `gg` |
