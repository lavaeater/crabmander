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

## Modes

Crabmander has several operating modes:

| Mode              | Description                                                                        |
|-------------------|------------------------------------------------------------------------------------|
| **Normal**        | Twin-pane file manager (default)                                                   |
| **Git**           | Git client view — staging, log, graph, reflog, diff, blame tabs                    |
| **Git Commit**    | Multi-line commit message editor (entered from Git mode)                           |
| **Git Branch**    | Branch list / switcher (entered from Git mode with `F6` / `b`)                    |
| **Git Repos**     | Global bookmarked git repo list (`F12` / `Shift+G` from Normal mode)              |

---

## Keyboard reference — Normal mode

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

Each F-key has a `Shift+letter` fallback for terminals or compositors that intercept
function keys (e.g. COSMIC desktop).

| Key            | Fallback  | Action                                                                          |
|----------------|-----------|---------------------------------------------------------------------------------|
| `F1`           |           | **Quick CD** — incremental directory navigator (see below)                      |
| `Shift+F1`     |           | **Recent dirs** — jump to a previously visited directory                        |
| `F2`           | `Shift+X` | **Context menu** — file-type-aware actions (see below)                          |
| `F3`           | `Shift+E` | **View** — open the cursor file in nano                                         |
| `F4`           | `Shift+Z` | **Sizes** — recursively calculate directory sizes, auto-sorts by size           |
| `F5`           | `Shift+Y` | **Copy** — copy marked/cursor files to the other panel's directory              |
| `F6`           | `Shift+R` | **Move** — move marked/cursor files to the other panel's directory              |
| `F7`           | `Shift+K` | **MkDir** — create a new directory in the active panel                          |
| `F8`           | `Shift+D` | **Delete** — delete marked/cursor files (with confirmation)                     |
| `F9`           | `Shift+S` | **Sort** — cycle sort column: Name → Size → Modified                            |
| `Shift+F9`     |           | **Invert sort** — toggle ascending ↑ / descending ↓                             |
| `F10` / `q`    |           | Quit                                                                            |
| `F11`          | `Shift+T` | **Select theme** — live-preview built-in color themes                           |
| `F12`          | `Shift+G` | **Git Repos** — global bookmarked git repository list (see below)               |
| `Ctrl+G`       |           | **Enter Git mode** — opens git client for the active repo, bookmarks it         |
| `Ctrl+Z`       |           | Suspend to background                                                           |

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

Visited directories are saved and accessible via **Shift+F1** (Recent dirs).

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
| `Esc`           | Clear filter                 |

`Backspace` is bound to "go to parent directory" (see above) rather than
editing the filter, so it exits the current directory instead of removing a
filter character.

The filter clears automatically when you navigate into a new directory.

### Sorting (F9 / Shift+F9)

The active sort column is indicated by an arrow in the column header
(`↑` ascending, `↓` descending). Directories are always listed before files.

- **F4** (Sizes) automatically switches to Size ↓ (largest first) and updates
  the sort live as directory sizes are computed in the background.
- Directories whose sizes are not yet known (shown as `···`) always appear at
  the bottom of the directory group until computed.

---

## Keyboard reference — Git Repos (global git view)

Open with `F12` / `Shift+G` from Normal mode. Shows all git repositories you have
previously visited via `Ctrl+G`, most-recently-used first. Each row shows the path,
current branch, and dirty status.

| Key              | Action                                             |
|------------------|----------------------------------------------------|
| `↑` / `k`        | Move cursor up                                     |
| `↓` / `j`        | Move cursor down                                   |
| `Enter`          | Open git client for the selected repository        |
| `d` / `Delete`   | Remove repo from the list                          |
| `Esc` / `q`      | Return to Normal mode                              |

When you open a repo from this view, pressing `Esc` in the git client returns
you back here rather than to Normal mode.

---

## Keyboard reference — Git mode

Enter Git mode with `Ctrl+G` when the active panel is inside a git repository.
The repo is automatically added to the global git repo list.

The git view has six tabs shown in a tab bar at the top of the screen.
Switch tabs with the letter keys listed below.

File status symbols:

| Symbol | Meaning (working tree) | Meaning (staging area) |
|--------|------------------------|------------------------|
| `M`    | Modified               | Modified               |
| `D`    | Deleted                | Deleted                |
| `?`    | Untracked              | —                      |
| `A`    | —                      | Added                  |
| `R`    | —                      | Renamed                |
| `C`    | —                      | Copied                 |

### Tab switching (all tabs)

| Key         | Tab                                                      |
|-------------|----------------------------------------------------------|
| `s`         | **Status** — staging view (default)                      |
| `l`         | **Log** — `git log --oneline --decorate`                 |
| `g`         | **Graph** — ASCII commit graph                           |
| `R`         | **Reflog** — `git reflog --date=iso --pretty`            |
| `d`         | **Diff** — word-diff of cursor file in Status tab        |
| `B`         | **Blame** — deep blame (`-C -C -C`) of cursor file       |

All tabs: `↑`/`k` up, `↓`/`j` down, `PgUp`/`PgDn` page scroll.

### Status tab keys

| Key              | Action                                                              |
|------------------|---------------------------------------------------------------------|
| `↑` / `k`        | Move cursor up                                                      |
| `↓` / `j`        | Move cursor down                                                    |
| `Tab`            | Switch between Working Tree and Staging Area panes                  |
| `Space` / `Insert` | Toggle mark on cursor entry and advance cursor                    |
| `F1` / `a`       | **Stage** — stage marked/cursor files from the Working Tree         |
| `F2` / `u`       | **Unstage** — unstage marked/cursor files from the Staging Area     |
| `F3` / `c`       | **Commit** — open multi-line commit message editor                  |
| `F4` / `p`       | **Push** — push current branch to remote                            |
| `F5` / `P`       | **Pull** — pull from remote                                         |
| `F6` / `b`       | **Branches** — open branch list / switcher                          |
| `F7` / `A`       | **Add all & commit** — stage everything and open commit editor      |
| `n`              | Create a new branch                                                 |
| `r`              | Reload git status                                                   |
| `Esc` / `q`      | Exit Git mode (or back to Git Repos if entered from there)          |

### Commit editor (Git Commit mode)

A full-screen multi-line text editor for writing commit messages.

| Key              | Action                  |
|------------------|-------------------------|
| `Ctrl+Enter` / `Alt+Enter` | Submit commit   |
| `Esc`            | Cancel                  |

### Branch list (Git Branch mode)

| Key     | Action                            |
|---------|-----------------------------------|
| `↑` / `k` | Move cursor up                  |
| `↓` / `j` | Move cursor down                |
| `Enter` | Check out the selected branch     |
| `n`     | Create a new branch               |
| `Esc`   | Exit back to Git mode             |

---

## Git panel indicator

When a panel is inside a git repository, its title bar shows the current branch
and dirty state:

```
─ /home/user/project [main] ──────
─ /home/user/project [main*] ─────   ← asterisk means uncommitted changes
```

---

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

### Themes

Built-in color themes are provided via the [opaline](https://github.com/opaline-rs/opaline)
crate. Select a theme interactively with `F11`, or set it permanently in your
config file:

```json5
{
  "theme": "catppuccin-mocha"   // default
}
```
