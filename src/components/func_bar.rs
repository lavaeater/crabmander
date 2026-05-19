use ratatui::{prelude::*, widgets::Paragraph};

use crate::palette::Palette;

const NORMAL_KEYS: &[(&str, &str)] = &[
    ("F1", "QuickCD"),
    ("⇧F1", "Recent"),
    ("F2", "Menu"),
    ("F3", "Nano"),
    ("F4", "Sizes"),
    ("F5", "Copy"),
    ("F6", "Move"),
    ("F7", "MkDir"),
    ("F8", "Delete"),
    ("F9", "Sort"),
    ("F10", "Quit"),
    ("F11", "Theme"),
];

const GIT_KEYS: &[(&str, &str)] = &[
    ("F1", "Stage"),
    ("F2", "Unstage"),
    ("F3", "Commit"),
    ("F4", "Push"),
    ("F5", "Pull"),
    ("F6", "Branches"),
    ("F7", "All+Commit"),
    ("t", "NewTag"),
    ("T", "Tags"),
    ("n", "NewBranch"),
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
    Git,
    Cargo,
}

pub fn draw(frame: &mut Frame, area: Rect, mode: FuncBarMode, palette: &Palette) {
    let (keys, key_style) = match mode {
        FuncBarMode::Normal => (NORMAL_KEYS, palette.funcbar_normal),
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
