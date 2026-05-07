use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use strum::Display;

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
    pub size: u64,
    pub modified: u64, // unix seconds
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
    },
    #[strum(to_string = "DirSizeResult")]
    DirSizeResult {
        side: Side,
        panel_path: PathBuf, // guards against stale results after navigation
        name: String,
        size: u64,
    },
    #[strum(to_string = "OpCompleted")]
    OpCompleted(Vec<Side>),
    OpError(String),

    // Dialog lifecycle
    DialogInputChar(char),
    DialogInputBackspace,
    DialogConfirm,
    DialogCancel,
}
