use ratatui::{prelude::*, widgets::Paragraph};

use crate::palette::Palette;

const NORMAL_KEYS: &[(&str, &str)] = &[
    ("F1", "QuickCD"),
    ("⇧F1", "Recent"),
    ("F2/⇧X", "Menu"),
    ("F3/⇧E", "View"),
    ("F4/⇧Z", "Sizes"),
    ("F5/⇧Y", "Copy"),
    ("F6/⇧R", "Move"),
    ("⇧F6", "Rename"),
    ("F7/⇧K", "MkDir"),
    ("F8/⇧D", "Delete"),
    ("F9/⇧S", "Sort"),
    ("F10", "Quit"),
    ("F11/⇧T", "Theme"),
    ("F12/⇧G", "GitRepos"),
];

const GIT_KEYS: &[(&str, &str)] = &[
    ("a", "Stage"),
    ("u", "Unstage"),
    ("c", "Commit"),
    ("S", "SuperCommit"),
    ("p", "Push"),
    ("P", "Pull"),
    ("b", "Branches"),
    ("n", "Branch"),
    ("A", "All+Commit"),
    ("t/T", "Tag"),
    ("Tab", "Pane"),
    ("Spc", "Mark"),
    ("Esc", "Normal"),
];

const CARGO_KEYS: &[(&str, &str)] = &[
    ("↑↓", "Navigate"),
    ("j/k", "Navigate"),
    ("Enter", "Run"),
    ("^B/Esc", "Exit"),
];

pub enum FuncBarMode {
    Normal,
    GlobalGit,
    Git,
    Cargo,
}

const GLOBAL_GIT_KEYS: &[(&str, &str)] = &[
    ("↑↓", "Navigate"),
    ("j/k", "Navigate"),
    ("Enter", "Open"),
    ("d/Del", "Remove"),
    ("Esc/q", "Back"),
];

pub fn draw(frame: &mut Frame, area: Rect, mode: FuncBarMode, palette: &Palette) {
    let (keys, key_style) = match mode {
        FuncBarMode::Normal => (NORMAL_KEYS, palette.funcbar_normal),
        FuncBarMode::GlobalGit => (GLOBAL_GIT_KEYS, palette.funcbar_git),
        FuncBarMode::Git => (GIT_KEYS, palette.funcbar_git),
        FuncBarMode::Cargo => (CARGO_KEYS, palette.funcbar_cargo),
    };
    let mut spans = Vec::new();
    for (key, label) in keys {
        spans.push(Span::styled(*key, key_style));
        spans.push(Span::raw(format!("{} ", label)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
