# Crabmander — Design Document

Crabmander is a dual-pane TUI file manager inspired by Norton Commander and Midnight Commander. This document describes the current UI, keyboard model, component architecture, and plans for future work.

---

## Layout

```
┌─ /home/user/documents [main*] ───────┬─ /home/user/downloads ───────────────┐
│ Name↑              Size   Date  Owner│ Name↑              Size   Date  Owner │
│ ..                 <DIR>             │ ..                 <DIR>              │
│ projects/          <DIR>  Jan 15     │*archive.tar.gz    45.3M  Jan 16      │
│▶README.md          4.5K   Jan 14  me │ image.png          2.1M  Jan 14      │
│*notes.txt           892B  Jan 13     │ video.mp4          1.2G  Jan 12      │
│ Filter: notes_                       │                                       │
├──────────────────────────────────────┴───────────────────────────────────────┤
│ README.md  4,608 bytes                           2 marked (5,500 bytes)      │
├──────────────────────────────────────────────────────────────────────────────┤
│ F1 QuickCD ⇧F1 Recent F2 Menu F3 Nano F4 Sizes F5 Copy F6 Move F7 MkDir … │
└──────────────────────────────────────────────────────────────────────────────┘
```

- The two panels split the terminal width 50/50 with a vertical divider.
- The active panel's border is highlighted; the inactive panel's is dimmed.
- The panel title shows the (condensed) current path plus `[branch]` or `[branch*]` when inside a git repo.
- `▶` marks the cursor row (reversed style). `*` marks a file selected for bulk operations.
- A filter bar appears between the column header and the file list when the user is typing.
- The status bar shows cursor file name/size on the left, and the active filter + mark count/size on the right.
- The function bar shows context-sensitive key hints (different set in Git mode).

---

## Keyboard Shortcuts

### Navigation (Normal mode)

| Key                  | Action                                     |
|----------------------|--------------------------------------------|
| `↑` / `k`            | Move cursor up                             |
| `↓` / `j`            | Move cursor down                           |
| `PgUp`               | Page up                                    |
| `PgDn`               | Page down                                  |
| `Home` / `g`         | Jump to first entry                        |
| `End` / `G`          | Jump to last entry                         |
| `Enter` / `→`        | Enter directory / open file (xdg-open)     |
| `Backspace` / `←`    | Go to parent directory                     |
| `Tab`                | Switch active panel                        |
| `Shift+Tab`          | Sync other panel to active panel's dir     |

### Marking

| Key              | Action                                              |
|------------------|-----------------------------------------------------|
| `Space` / `Ins`  | Toggle mark on cursor entry; cursor moves down      |
| `*`              | Toggle mark on all entries in the panel             |

Directories can be marked. When an operation is triggered with no marks, it acts on the cursor entry.

### Filter

Typing any printable character that isn't bound as a keybinding activates the inline filter. The panel list narrows in real time (case-insensitive substring match). `Backspace` removes the last character. `Esc` clears the filter. Navigating into a directory also clears the filter.

### Operations (function keys)

| Key       | Action        | Notes                                                  |
|-----------|---------------|--------------------------------------------------------|
| `F1`      | Quick CD      | Typeahead directory navigator with Tab completion      |
| `Shift+F1`| Recent Dirs   | Menu of last 10 visited directories                    |
| `F2`      | Context Menu  | File-type–aware action list (see below)                |
| `F3`      | Open in Nano  | Prompt for filename, suspend TUI, open `nano`          |
| `F4`      | Calc Sizes    | Recursive dir sizes (async, cached), sort by size      |
| `F5`      | Copy          | Input dialog → async recursive copy                    |
| `F6`      | Move          | Input dialog → `rename` (cross-fs error if needed)     |
| `F7`      | MkDir         | Input dialog → `create_dir`                            |
| `F8`      | Delete        | Confirm dialog → async recursive delete                |
| `F9`      | Cycle Sort    | Cycles Name → Size → Modified; column header shows `↑↓`|
| `Shift+F9`| Invert Sort   | Toggle ascending / descending                          |
| `F10`     | Quit          | Immediate quit                                         |
| `F11`     | Theme         | Pick from ~20 opaline builtin themes live              |
| `Ctrl+G`  | Enter Git mode| Available when active panel is inside a git repo       |

### F2 Context Menu items (file-type aware)

- **Open with OS** — `xdg-open` the file/directory
- **Run VS Code here** — `code <dir>` in the background
- **Extract here** / **Extract to →** — for `.zip`, `.tar.*`, `.7z`, `.rar`
- **Execute…** — prompt for arguments, then suspend TUI and run
- **Change owner…** — `sudo chown` with optional `-R` for directories
- **Mount / Unmount** — removable devices discovered via `lsblk --json`

### Git mode (`Ctrl+G`)

| Key          | Action                     |
|--------------|----------------------------|
| `↑` / `k`    | Move cursor up             |
| `↓` / `j`    | Move cursor down           |
| `Tab`         | Switch between working tree / staging pane |
| `Space`/`Ins` | Toggle mark               |
| `F1` / `a`   | Stage                      |
| `F2` / `u`   | Unstage                    |
| `F3` / `c`   | Commit (opens textarea)    |
| `F4` / `p`   | Push to `origin`           |
| `F5` / `P`   | Pull from `origin` (fast-forward only) |
| `F6` / `b`   | List branches / checkout   |
| `n`           | New branch                 |
| `r`           | Reload status              |
| `Esc` / `q`  | Exit git mode              |

Commit message editor: `Ctrl+Enter` or `Alt+Enter` submits; `Esc` cancels.

---

## Dialogs

All dialogs are modal overlays centred on screen. Keyboard input is captured by the dialog.

| Variant      | Trigger             | Description                                        |
|--------------|---------------------|----------------------------------------------------|
| `Confirm`    | F8 Delete           | Yes/No prompt with y/n shortcuts                   |
| `Input`      | F3/F5/F6/F7/Execute | Editable text field with prompt                    |
| `ContextMenu`| F2, F11, Shift+F1   | Scrollable list of items; ↑↓ to navigate           |
| `QuickCd`    | F1                  | Typeahead navigator; Tab completes selected match  |
| `ErrorList`  | after bulk ops      | Scrollable list of per-file error messages         |

---

## Architecture

### Modes (`app::Mode`)

```rust
pub enum Mode {
    Normal,     // navigating panels
    Dialog,     // a modal overlay is open
    Git,        // git status view
    GitCommit,  // textarea commit message editor
    GitBranch,  // branch picker popup
}
```

### Actions (`action::Action`)

The `Action` enum is the single vocabulary that flows through the event loop. Key categories:

| Category       | Variants (examples)                                           |
|----------------|---------------------------------------------------------------|
| App lifecycle  | `Quit`, `Suspend`, `Resume`, `ClearScreen`, `Render`, `Tick` |
| Navigation     | `NavUp/Down/PageUp/PageDown/Top/Bottom/Enter/Parent`          |
| Panel          | `SwitchPanel`, `SyncPanelDir`                                 |
| Filter         | `FilterChar(char)`, `FilterBackspace`, `FilterClear`          |
| Marking        | `ToggleMark`, `ToggleMarkAll`                                 |
| F-key ops      | `Copy`, `Move`, `Mkdir`, `Delete`, `View`, `CalcSizes`, `ContextMenu`, `CycleSortMode`, `InvertSort`, `SelectTheme`, `RecentDirs`, `QuickCd` |
| Async results  | `DirLoaded`, `DirSizeResult`, `GitInfoLoaded`, `OpCompleted`, `OpError`, `OpErrors` |
| Execute        | `ExecuteFile { cmd, args, reload }`, `ExecuteCopy/Move/Delete/Mkdir` |
| Dialog I/O     | `DialogConfirm`, `DialogCancel`, `DialogNavUp/Down`, `DialogInputChar/Backspace` |
| QuickCd        | `QuickCdChar`, `QuickCdBackspace`, `QuickCdComplete`          |
| Git mode       | `EnterGitMode`, `ExitGitMode`, `GitStage`, `GitUnstage`, `GitCommit`, `GitPush`, `GitPull`, `GitListBranches`, `GitNewBranch`, `GitStatusLoaded`, `GitBranchesLoaded`, … |

### Components

| Component     | File                          | Responsibility                                            |
|---------------|-------------------------------|-----------------------------------------------------------|
| `Panel`       | `src/components/panel.rs`     | File list, navigation, filter, sort, dir-size calc, git indicator |
| `FunctionBar` | `src/components/func_bar.rs`  | F-key hint row; mode-sensitive (normal vs. git)           |
| `dialog`      | `src/components/dialog.rs`    | All modal overlays; `DialogState` enum + `draw()`         |
| `GitView`     | `src/components/git_view.rs`  | Working tree / staging split view; branch ops             |

`App` (in `src/app.rs`) owns all components and wires them together. It is not a `Component` implementor itself — it is the event loop.

### Panel state model

```rust
pub struct Panel {
    pub side: Side,
    pub path: PathBuf,
    pub entries: Vec<EntryInfo>,
    /// Indices into `entries` after filter + sort.
    pub view_indices: Vec<usize>,
    pub cursor: usize,           // position within view_indices
    pub offset: usize,           // scroll offset within view_indices
    pub marked: HashSet<String>,
    pub is_active: bool,
    pub filter: String,
    pub dir_sizes: HashMap<String, u64>,  // computed by F4
    pub sort_mode: SortMode,     // Name | Size | Modified
    pub sort_order: SortOrder,   // Asc | Desc
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    // …plus private fields
}

pub struct EntryInfo {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: u64,   // unix seconds
    pub nlink: u32,      // hard-link count; > 1 → entry_hardlink palette style
    pub owner: String,   // resolved via getpwuid_r
}
```

The `view_indices` indirection means filtering and sorting only rebuild an index slice; `entries` is the stable backing store.

### Dialog state model

```rust
pub enum DialogState {
    Confirm   { title, message, op: DeferredOp },
    Input     { title, prompt, value, op: DeferredOp },
    ContextMenu { title, items: Vec<MenuItem>, selected },
    QuickCd   { input, matches: Vec<String>, selected, base_path },
    ErrorList { title, errors: Vec<String>, scroll },
}

pub enum DeferredOp {
    Delete(Vec<PathBuf>),
    Copy   { sources },
    Move   { sources },
    Mkdir  { base },
    Execute { path },
    OpenInNano { base },
    ChownFiles { paths, reload_sides },
    GitCreateBranch { git_root },
}
```

On `DialogConfirm`, `App` calls `execute_op(op, value)` which dispatches the async work.

### Async file operations

All mutating ops run on Tokio tasks and report back via `Action::OpCompleted(Vec<Side>)` or `Action::OpError(String)`. The main loop reloads affected panels on `OpCompleted` and invalidates the session-wide `dir_size_cache` (a `HashMap<PathBuf, u64>` keyed by absolute path).

### Theme system

Themes come from the `opaline` crate (20+ builtins; default: `catppuccin-mocha`). At startup a `Palette` struct is built from the chosen theme — all styles are pre-computed once and passed `&palette` to every draw call.

### Git integration

- **Panel title indicator**: after every `DirLoaded` the panel spawns a blocking `git2::Repository::discover` task. Result: `[branch]` or `[branch*]` appended to the title.
- **Git mode**: entered via `Ctrl+G`. A `GitView` replaces the two file panels. A `notify`-based file watcher (debounced to 200 ms) fires `GitReload` on any filesystem change under `.git/`.
- **Credential helper**: SSH agent → `~/.ssh/id_ed25519|id_rsa|id_ecdsa` fallback.

### Recent directories

Stored in `$CRABMANDER_DATA/recent_dirs.json` (up to 10 entries). Updated whenever Quick CD navigates to a new directory.

### Configuration

Config lives in `$CRABMANDER_CONFIG` or the XDG config dir. Falls back to the embedded `.config/config.json5`. Keybindings are per-`Mode` maps of key-sequence strings to `Action`s; key sequences are matched using a multi-key accumulator in `App::handle_key_event`.

---

## Future Work

### More tests

The codebase currently has no automated tests. Priority areas:

- **`Panel` unit tests**: `rebuild_view` filter + sort invariants; `toggle_mark`/`toggle_mark_all` edge cases; `effective_targets` when marks are empty vs. populated.
- **`dialog` unit tests**: `nav_up`/`nav_down` boundary conditions; `push_char`/`pop_char` on the correct variant.
- **`recent_dirs` tests**: round-trip `push` → `save` → `load`; truncation at `MAX`; deduplication.
- **`condense_path` / `format_size` / `format_age`**: pure functions with well-defined input/output; trivial to test and high coverage value.
- **Integration smoke test**: spin up the app in headless mode (no real terminal) and send a sequence of `Action`s into `action_tx`; assert panel state after each.

### Architecture: data-driven context menu (removing hard-coding from F2)

Today `App::open_context_menu` is one big function that manually builds a `Vec<MenuItem>` by inspecting the current entry. Adding a new menu item means editing that function.

A cleaner model is a **`ContextMenuProvider` registry**:

```rust
/// Implemented by anyone who wants to contribute items to the F2 menu.
pub trait ContextMenuProvider: Send + Sync {
    /// Return items relevant to this entry, or an empty vec if none apply.
    fn items(&self, ctx: &MenuCtx) -> Vec<MenuItem>;
}

pub struct MenuCtx<'a> {
    pub entry: &'a EntryInfo,
    pub entry_path: &'a Path,
    pub panel_dir: &'a Path,
    pub other_dir: &'a Path,
    pub active_side: Side,
}
```

`App` holds a `Vec<Box<dyn ContextMenuProvider>>`. `open_context_menu` iterates the registry and collects all items. Built-in providers (OS open, VS Code, archive extraction, executable run, chown, device mount) each become a small struct in their own module. New commands need no changes to `App` at all.

This is also the seam through which plugins would add menu items (see below).

### Architecture: generalizing `DeferredOp` for extensibility

`DeferredOp` is an enum, so adding a new operation requires touching `dialog.rs`, `app.rs`, and `action.rs`. A trait-based alternative:

```rust
pub trait DeferredOp: Send + Sync + std::fmt::Debug {
    /// Called by App when the dialog is confirmed.
    fn execute(self: Box<Self>, ctx: &OpCtx);
}
```

Each operation becomes its own struct. The dialog carries a `Box<dyn DeferredOp>` instead of an enum variant. The `execute_op` match arm in `App` shrinks to a single `op.execute(&ctx)` call. This also works cleanly for plugin-supplied operations.

### Plugin system (dynamically loaded `.so` / `.dylib`)

The goal is to let third parties ship Rust crates that extend Crabmander's context menu without forking the project.

**How it would work** (based on the dynamically loaded library pattern):

1. **Shared interface crate** (`crabmander-plugin-api`): defines stable, `#[repr(C)]`-compatible traits and a versioned `PluginManifest` that plugins must export. This crate must never break ABI; semver-major bumps are required for any breaking change.

2. **Plugin crate**: compiled as `crate-type = ["cdylib"]`. Exports one `#[no_mangle]` symbol:

   ```rust
   // In the plugin crate:
   #[no_mangle]
   pub extern "C" fn crabmander_plugin_init() -> *mut dyn ContextMenuProvider {
       Box::into_raw(Box::new(MyProvider))
   }
   ```

3. **Host loading** (using the `libloading` crate):

   ```rust
   fn load_plugin(path: &Path) -> color_eyre::Result<Box<dyn ContextMenuProvider>> {
       // Safety: the plugin must be compiled against the same crabmander-plugin-api version.
       let lib = unsafe { libloading::Library::new(path)? };
       let init: libloading::Symbol<unsafe extern "C" fn() -> *mut dyn ContextMenuProvider>
           = unsafe { lib.get(b"crabmander_plugin_init")? };
       let provider = unsafe { Box::from_raw(init()) };
       // Keep `lib` alive as long as `provider` is live — store them together.
       Ok(provider)
   }
   ```

4. **Discovery**: Crabmander scans `$CRABMANDER_CONFIG/plugins/` (and optionally `$XDG_DATA_HOME/crabmander/plugins/`) for `*.so` / `*.dylib` files at startup and loads each one.

5. **Safety constraints**:
   - Plugins **must** be compiled with the same Rust toolchain and the same version of `crabmander-plugin-api`. ABI stability is not guaranteed across nightly toolchains; document this clearly.
   - Because `dylib` plugins share the same process, a crash in a plugin crashes the whole app. Sandboxing (e.g. running plugins in a subprocess over a Unix socket) is a future option but adds significant complexity.
   - Consider keeping a `plugin_api_version: u32` constant in `PluginManifest` and refusing to load plugins that declare a different version.

6. **What a plugin can do**:
   - Add items to the F2 context menu (via `ContextMenuProvider`).
   - In a future extension: add new `DeferredOp` implementations.
   - In a further extension: register new keybindings by returning `(KeyEvent, Action)` pairs.

This is the minimum viable plugin architecture. It intentionally does not expose internal `App` state to plugins; plugins only see the `MenuCtx` snapshot.

### Other future ideas

- **Rename (F2 submenu or dedicated key)**: an Input dialog pre-filled with the current filename, writes via `fs::rename`.
- **Symlink creation**: create a relative or absolute symlink to the cursor entry in the other panel's directory.
- **Bulk rename**: open a textarea pre-filled with one filename per line; user edits; changes are applied as renames.
- **File preview pane (F3 without Nano)**: a third vertical strip (or a horizontal split) showing a hex dump or text preview of the cursor file.
- **Cross-filesystem move**: currently `F6 Move` fails with an EXDEV error. The fix is to fall back to `copy_recursive` + `delete_recursive` when `rename` returns `EXDEV`.
- **Progress indicator**: long copy/move operations give no feedback. A progress bar overlay (driven by a Tokio channel that streams bytes-copied) would help.
- **Bookmarks**: like recent dirs but user-managed (add/remove via a keybinding), persisted in config.
- **Git merge and diff**: in git mode, show a diff of the cursor file (`git diff`); offer a merge tool launcher.
- **SSH/SFTP panel**: one panel browses a remote host via SFTP (using the `ssh2` crate) while the other stays local. F5/F6 transfer between hosts.
