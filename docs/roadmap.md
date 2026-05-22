# Crabmander Roadmap

## Global Git Client — Feature Plan

### Overview

Extend crabmander beyond a per-repo staging area into a proper multi-repo git client. Two new layers are added on top of the existing git staging view:

1. **Global git repo registry** — a persistent list of bookmarked git roots, built automatically as you enter git mode across different repos.
2. **Global git view** — a TUI screen listing all bookmarked repos, letting you jump directly to any repo's git view without navigating the file panel first.

Within the git view itself, we add four new tabs that turn it into a complete embedded git client:

| Tab | Equivalent CLI | Key |
|-----|---------------|-----|
| Status | `git status` | `s` |
| Log | `git log --oneline --decorate` | `l` |
| Graph | `git log --oneline --decorate --graph` | `g` |
| Reflog | `git reflog --date=iso --pretty` | `R` |
| Diff | `git diff --word-diff` | `d` |
| Blame | `git blame -C -C -C` | `B` |

---

### 1. Git Repo Bookmarks (`src/git_repos.rs`)

- **Trigger**: every time `Ctrl+G` is pressed in the normal file view, the git root for the active panel is appended (dedup) to the global repo list.
- **Persistence**: `$XDG_DATA_HOME/crabmander/git_repos.json` — a JSON array of absolute paths.
- **API** matches the existing `bookmarks.rs` pattern: `load`, `save`, `add`, `remove`.

### 2. Global Git View (`src/components/global_git_view.rs`)

New `Mode::GlobalGit`. Accessible via `F12` or `Shift+G` from Normal mode.

**Layout**: full-screen list of bookmarked repos. Each row shows:
```
  /home/user/projects/my-app     [main]   3 untracked
```

**Keys**:

| Key | Action |
|-----|--------|
| `↑` / `k` | Move cursor up |
| `↓` / `j` | Move cursor down |
| `Enter` | Enter git view for selected repo |
| `d` / `Delete` | Remove repo from bookmark list |
| `Esc` / `q` | Return to Normal mode |

When `Enter` selects a repo, a flag `git_from_global: bool` is set on App so that pressing `Esc` in git mode returns to the global git view rather than Normal mode.

### 3. Enhanced Git View — Tabs

The git view gains an internal `GitViewTab` enum. Tab indicator shown in the header bar.

#### 3a. Log View (`l`)

Equivalent to `git log --oneline --decorate`.

Display format:
```
abc1234 (HEAD -> main, origin/main) Fix the thing
def5678 Add feature X
ghi9012 (origin/feature/foo) WIP
```

Loaded async via `git2::Repository::revwalk`. Each `GitCommit` record carries:
- 7-char short OID
- Summary line
- Author name
- Unix timestamp (formatted as relative or ISO)
- Vec of ref decorations

Navigation: `↑`/`↓`, `PgUp`/`PgDn`. `Enter` on a commit → enter blame/diff for that commit (future).

#### 3b. Graph View (`g`)

Equivalent to `git log --oneline --decorate --graph`.

Uses a lane-assignment algorithm:
- Maintain a `Vec<Option<String>>` of active OIDs per lane (column)
- For each commit C in topological order:
  - Find lane L where `lanes[L] == Some(C.id)` → that's C's column
  - Set `lanes[L] = C.parents[0]` (first parent inherits the lane)
  - For each additional parent, assign a new or reused lane
  - Emit graph prefix characters (`*`, `|`, `/`, `\`) based on lane delta vs previous row

Rendered with Ratatui `Line` spans, using styled characters for graph lines.

Example output:
```
* 1234567 (HEAD -> main) Latest commit
|\
| * abcdefg (feature/foo) Feature branch commit
* | fedcba0 Another commit on main
|/
* 9876543 Older shared commit
```

#### 3c. Reflog View (`R`)

Equivalent to `git reflog --date=iso --pretty`.

Loaded via `git2::Repository::reflog("HEAD")`. Each entry shows:
```
abc1234 HEAD@{0}  2026-05-21 14:23:01  commit: Fix the thing
def5678 HEAD@{1}  2026-05-20 09:11:44  checkout: moving from feature to main
```

Navigation: `↑`/`↓`. Read-only.

#### 3d. Diff View (`d`)

Equivalent to `git diff --word-diff` for the currently selected file in the status tab, or the working-tree diff when no file is selected.

Uses `git2::Repository::diff_index_to_workdir`. Lines are split at word boundaries and individually compared, with added words shown in green and removed words in red (inline, like `git diff --word-diff=color`).

#### 3e. Blame View (`B`)

Equivalent to `git blame -C -C -C`.

Uses `git2::BlameOptions` with:
- `track_copies_same_file(true)` (equivalent to `-C`)
- `track_copies_same_commit_moves(true)` (equivalent to `-C -C`)
- `track_copies_any_commit_copies(true)` (equivalent to `-C -C -C`)

Display format (columns): `short_oid | author | date | line_no | content`

Scroll vertically. Pressing `Enter` on a line could navigate to the commit (future).

---

### 4. Key Binding Summary (new additions)

#### Normal mode additions

| Key | Action |
|-----|--------|
| `F12` / `Shift+G` | Open Global Git View |

#### Global Git View

| Key | Action |
|-----|--------|
| `↑` / `k` | Cursor up |
| `↓` / `j` | Cursor down |
| `Enter` | Open git view for selected repo |
| `d` / `Delete` | Remove repo from list |
| `Esc` / `q` | Back to Normal mode |

#### Git view — tab switching

| Key | Tab |
|-----|-----|
| `s` | Status (staging view — existing) |
| `l` | Log |
| `g` | Graph |
| `R` | Reflog |
| `d` | Diff (of cursor file in Status tab) |
| `B` | Blame (of cursor file in Status tab) |

All tab views share:
- `↑` / `k`, `↓` / `j` — scroll
- `PgUp` / `PgDn` — page scroll
- `Esc` — exit git mode (or back to global git view if entered from there)

---

### 5. Technical Notes

- **git2 crate** (v0.21.0) used for all git operations. No subprocess calls.
- Log/graph/reflog data loaded on a `tokio::task::spawn_blocking` thread and sent back via `Action` enum, consistent with the existing async pattern.
- Blame and diff loaded lazily when the tab is first opened.
- Graph lane algorithm is pure Rust, no extra dependencies.
- Persistence files live alongside existing data files in `get_data_dir()`.

---

### 6. Implementation Order

1. `git_repos.rs` — persistence module
2. `action.rs` — new action variants and data types
3. `global_git_view.rs` — component
4. `git_view.rs` — add tab enum + log/graph/reflog loading
5. `app.rs` — wire up modes, actions, bookmarking, global<→git navigation
6. `func_bar.rs` + `config.json5` — keybinding and UI updates
7. `README.md` — documentation
