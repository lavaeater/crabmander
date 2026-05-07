# Crabmander — Design Document

Crabmander is a dual-pane TUI file manager inspired by Norton Commander and Midnight Commander. This document describes the target UI, keyboard model, component architecture, and implementation plan.

---

## Layout

```
┌─ /home/user/documents ───────────────┬─ /home/user/downloads ───────────────┐
│ Name               Size   Date       │ Name               Size   Date        │
│ ..                 <DIR>             │ ..                 <DIR>              │
│ projects/          <DIR>  Jan 15     │*archive.tar.gz    45.3M  Jan 16      │
│▶README.md          4.5K   Jan 14     │ image.png          2.1M  Jan 14      │
│*notes.txt           892B  Jan 13     │ video.mp4          1.2G  Jan 12      │
│ todo.md            1.1K   Jan 12     │                                       │
│                                      │                                       │
├──────────────────────────────────────┴───────────────────────────────────────┤
│ README.md  4,608 bytes                           2 marked (5,500 bytes)      │
├──────────────────────────────────────────────────────────────────────────────┤
│ F2 Menu  F3 View  F4 Edit  F5 Copy  F6 Move  F7 MkDir  F8 Delete  F10 Quit  │
└──────────────────────────────────────────────────────────────────────────────┘
```

- The two panels split the terminal width 50/50 with a vertical divider.
- The active panel's border is highlighted; the inactive panel's is dimmed.
- `▶` marks the cursor row. `*` marks a file selected for bulk operations.
- The status bar (one row) shows the cursor file's name and size on the left, and the marked-file count and cumulative size on the right. When nothing is marked it shows only cursor info.
- The function bar (one row) is always visible. Unavailable actions (F2, F3, F4 for now) are rendered dimmed.

---

## Keyboard Shortcuts

### Navigation

| Key                  | Action                          |
|----------------------|---------------------------------|
| `↑` / `k`            | Move cursor up                  |
| `↓` / `j`            | Move cursor down                |
| `PgUp`               | Page up                         |
| `PgDn`               | Page down                       |
| `Home` / `g`         | Jump to first entry             |
| `End` / `G`          | Jump to last entry              |
| `Enter` / `→`        | Enter directory / open file     |
| `Backspace` / `←`    | Go to parent directory          |
| `Tab`                | Switch active panel             |

### Marking

| Key              | Action                                       |
|------------------|----------------------------------------------|
| `Space` / `Ins`  | Toggle mark on cursor entry, cursor moves down |
| `*`              | Toggle mark on all entries in panel          |

Marking applies to files only; directories cannot be marked. When an operation is triggered with no marks, it acts on the cursor entry.

### Operations (function keys)

| Key   | Action  | Scope         | Status      |
|-------|---------|---------------|-------------|
| `F2`  | Menu    | —             | Later       |
| `F3`  | View    | cursor / mark | Later       |
| `F4`  | Edit    | cursor        | Later       |
| `F5`  | Copy    | cursor / mark | **Now**     |
| `F6`  | Move    | cursor / mark | **Now**     |
| `F7`  | MkDir   | —             | **Now**     |
| `F8`  | Delete  | cursor / mark | **Now**     |
| `F10` | Quit    | —             | **Now**     |

`F5` and `F6` copy/move into the directory shown in the **opposite panel**. The destination path is shown pre-filled in the dialog and can be edited before confirming.

---

## Dialogs

All dialogs are modal overlays centred on screen. Keyboard input is captured by the dialog; panels are greyed out.

### Confirm dialog (Delete)

```
┌─────────────── Delete ────────────────┐
│                                       │
│  Delete 3 marked files?               │
│                                       │
│         [ Yes ]    [ No ]             │
└───────────────────────────────────────┘
```

`Enter` / `y` confirms. `Esc` / `n` cancels.

### Input dialog (Copy, Move, MkDir)

```
┌──────────────── Copy ─────────────────┐
│                                       │
│  Copy to:                             │
│  ┌───────────────────────────────┐    │
│  │ /home/user/downloads          │    │
│  └───────────────────────────────┘    │
│                                       │
│         [ OK ]    [ Cancel ]          │
└───────────────────────────────────────┘
```

Editable text field pre-filled with the destination path. `Enter` confirms, `Esc` cancels.

---

## Architecture

### Modes (`app::Mode`)

```rust
pub enum Mode {
    Normal,   // navigating panels (replaces Home)
    Dialog,   // a modal is open; panel keys are suppressed
}
```

### Actions (`action::Action`)

New variants to add alongside the existing set:

```rust
// Navigation
NavUp, NavDown, NavPageUp, NavPageDown, NavTop, NavBottom,
NavEnter,   // Enter / →  — descend into dir
NavParent,  // Backspace / ← — ascend

// Panel
SwitchPanel,   // Tab

// Marking
ToggleMark,    // Space / Ins — toggle + advance cursor
ToggleMarkAll, // * — toggle all

// Operations
Copy, Move, Mkdir, Delete,

// Dialog lifecycle
DialogInputChar(char),
DialogInputBackspace,
DialogConfirm,
DialogCancel,
```

### Components

Replace the placeholder `Home` component with:

| Component     | File                          | Responsibility                                          |
|---------------|-------------------------------|---------------------------------------------------------|
| `Panel`       | `src/components/panel.rs`     | Renders one file-browser pane; owns `PanelState`        |
| `FunctionBar` | `src/components/func_bar.rs`  | Renders the F-key hint row at the bottom                |
| `Dialog`      | `src/components/dialog.rs`    | Renders modal overlays; owns `Option<DialogState>`      |

`App` holds two `Panel` instances (left, right) plus `FunctionBar` and `Dialog`. The active panel side is stored in `App` and broadcast to panels via a new `SetActivePanel(Side)` action.

### State model

```rust
// In src/components/panel.rs
pub struct PanelState {
    pub path: PathBuf,
    pub entries: Vec<Entry>,   // sorted: dirs first, then files, both alpha
    pub cursor: usize,
    pub scroll_offset: usize,
    pub marked: HashSet<PathBuf>,
}

pub struct Entry {
    pub name: OsString,
    pub is_dir: bool,
    pub size: u64,           // 0 for dirs
    pub modified: SystemTime,
}
```

```rust
// In src/components/dialog.rs
pub enum DialogKind {
    Confirm { message: String, on_confirm: Action },
    Input   { title: String, prompt: String, value: String, on_confirm: Box<dyn Fn(String) -> Action + Send> },
}

pub struct DialogState {
    pub kind: DialogKind,
}
```

### Operation targeting

When an operation is triggered the **effective target set** is resolved once:

1. If the active panel has any marked files → operate on all marked files.
2. Otherwise → operate on the entry under the cursor (unless it is `..`).

`F5 Copy` and `F6 Move` open an Input dialog pre-filled with the **opposite panel's current path**. The user can edit the destination before confirming.

`F7 Mkdir` opens an Input dialog (empty) for the new directory name; the directory is created inside the active panel's current path.

`F8 Delete` opens a Confirm dialog. On confirm, marked/cursor files are removed. Directories are deleted recursively.

After any successful mutating operation the active panel (and for Copy/Move the opposite panel) reloads its directory listing.

### Directory loading

Directory I/O is synchronous for now (`std::fs::read_dir`). The panel rebuilds its `entries` list on:

- Initial `init()` call
- `NavEnter` / `NavParent`
- Post-operation reload via a `ReloadPanel(Side)` action

---

## Implementation Order

1. **`PanelState` + `Panel` component** — render a file list, handle all navigation keys, `SwitchPanel`.
2. **`FunctionBar` component** — static row of F-key hints, dims unavailable ones.
3. **File marking** — `ToggleMark`, `ToggleMarkAll`, status bar mark count.
4. **`Dialog` component + `F10` quit** — modal infrastructure; wire Quit through a Confirm dialog.
5. **`F7 Mkdir`** — Input dialog → `std::fs::create_dir` → reload.
6. **`F8 Delete`** — Confirm dialog → `std::fs::remove_file` / `remove_dir_all` → reload.
7. **`F5 Copy`** — Input dialog (destination) → `std::fs::copy` loop → reload both panels.
8. **`F6 Move`** — Input dialog (destination) → `std::fs::rename` (fallback copy+delete) → reload both panels.
