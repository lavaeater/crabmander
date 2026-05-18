use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use strum::Display;

// --- Git status types ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitIndexStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitWorktreeStatus {
    Modified,
    Deleted,
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitEntry {
    pub path: String,
    pub index: Option<GitIndexStatus>,
    pub worktree: Option<GitWorktreeStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn other(self) -> Self {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryInfo {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: u64, // unix seconds
    /// Hard link count. > 1 means multiple directory entries share this inode.
    pub nlink: u32,
    /// Owner username (empty for non-Unix or unresolvable UIDs).
    pub owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    Error(String),
    Help,

    // Navigation
    NavUp,
    NavDown,
    NavPageUp,
    NavPageDown,
    NavTop,
    NavBottom,
    NavEnter,
    NavParent,
    SwitchPanel,
    SyncPanelDir, // Shift+Tab: sync other panel to active panel's dir

    // Marking
    ToggleMark,
    ToggleMarkAll,

    // Operations (triggered by F-keys)
    Copy,
    Move,
    Mkdir,
    Delete,
    ContextMenu,   // F2
    View,          // F3 — open in nano
    CalcSizes,     // F4 — recursive dir size calculation
    CycleSortMode, // F9 — cycle Name → Size → Modified
    InvertSort,    // Shift+F9 — flip Asc/Desc

    // Quick CD (F1)
    QuickCd,
    QuickCdChar(char),
    QuickCdBackspace,
    QuickCdComplete, // Tab — complete selected match into input

    // Filter (unbound printable keys in Normal mode)
    FilterChar(char),
    FilterBackspace,
    FilterClear,

    // Dialog navigation (arrow keys while a dialog is open)
    DialogNavUp,
    DialogNavDown,

    // Async completions (sent from spawned tasks)
    #[strum(to_string = "DirLoaded")]
    DirLoaded {
        side: Side,
        path: PathBuf,
        entries: Vec<EntryInfo>,
    },
    #[strum(to_string = "ExecuteDelete")]
    ExecuteDelete(Vec<PathBuf>),
    #[strum(to_string = "ExecuteCopy")]
    ExecuteCopy {
        sources: Vec<PathBuf>,
        dest: PathBuf,
    },
    #[strum(to_string = "ExecuteMove")]
    ExecuteMove {
        sources: Vec<PathBuf>,
        dest: PathBuf,
    },
    #[strum(to_string = "ExecuteMkdir")]
    ExecuteMkdir {
        base: PathBuf,
        name: String,
    },
    #[strum(to_string = "ExecuteFile")]
    ExecuteFile {
        cmd: String,
        args: Vec<String>,
        /// Panels to reload after the command exits.
        reload: Vec<Side>,
    },
    #[strum(to_string = "DirSizeResult")]
    DirSizeResult {
        side: Side,
        panel_path: PathBuf, // guards against stale results after navigation
        name: String,
        size: u64,
    },
    #[strum(to_string = "GitInfoLoaded")]
    GitInfoLoaded {
        side: Side,
        path: PathBuf,
        branch: Option<String>, // None = not a git repo
        is_dirty: bool,
    },
    #[strum(to_string = "OpCompleted")]
    OpCompleted(Vec<Side>),
    OpError(String),
    #[strum(to_string = "OpErrors")]
    OpErrors(Vec<String>),

    // Dialog lifecycle
    DialogInputChar(char),
    DialogInputBackspace,
    DialogConfirm,
    DialogCancel,

    SelectTheme,  // F11
    RecentDirs,   // Shift-F1

    // Git mode
    EnterGitMode,
    ExitGitMode,
    GitNavUp,
    GitNavDown,
    GitSwitchPane,
    GitToggleMark,
    GitStage,
    GitUnstage,
    GitCommit,
    GitCommitSubmit,
    GitCommitCancel,
    GitPush,
    GitPull,
    GitReload,
    GitAddAllAndCommit,
    GitAddAllDone,
    GitListBranches,
    GitNewBranch,
    GitBranchNavUp,
    GitBranchNavDown,
    GitBranchConfirm,

    // Tags
    GitCreateTag,
    GitListTags,
    /// Internal: tag name was entered; now prompt for message.
    #[strum(to_string = "GitTagNameChosen")]
    GitTagNameChosen {
        git_root: PathBuf,
        name: String,
    },
    #[strum(to_string = "GitTagsLoaded")]
    GitTagsLoaded {
        tags: Vec<TagInfo>,
    },
    GitTagNavUp,
    GitTagNavDown,
    /// Check out the commit pointed to by the selected tag.
    GitTagConfirm,
    /// Delete the selected tag.
    GitTagDelete,
    GitPushFollowTags,
    #[strum(to_string = "GitStatusLoaded")]
    GitStatusLoaded {
        git_root: PathBuf,
        branch: String,
        entries: Vec<GitEntry>,
    },
    #[strum(to_string = "GitBranchesLoaded")]
    GitBranchesLoaded {
        branches: Vec<BranchInfo>,
    },
    GitOpCompleted,

    // Progress tracking for long-running operations
    #[strum(to_string = "Progress")]
    Progress {
        id: u64,
        label: String,
        done: u64,
        total: u64,
    },
    ProgressDone(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    pub is_annotated: bool,
    /// Short OID of the target commit.
    pub target_id: String,
    /// Tagger message (annotated tags only).
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchInfo {
    /// Short display name (e.g. "main" or "feature/foo").
    pub name: String,
    /// Whether a local branch with this name exists.
    pub is_local: bool,
    /// Whether this is the currently checked-out branch.
    pub is_current: bool,
    /// For remote-only branches: the canonical remote ref (e.g. "origin/foo").
    pub remote_ref: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_other_left_gives_right() {
        assert_eq!(Side::Left.other(), Side::Right);
    }

    #[test]
    fn side_other_right_gives_left() {
        assert_eq!(Side::Right.other(), Side::Left);
    }

    #[test]
    fn side_other_is_involution() {
        assert_eq!(Side::Left.other().other(), Side::Left);
        assert_eq!(Side::Right.other().other(), Side::Right);
    }
}
