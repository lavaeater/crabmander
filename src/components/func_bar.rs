use ratatui::{prelude::*, widgets::Paragraph};

use crate::palette::Palette;

const NORMAL_KEYS: &[(&str, &str)] = &[
    ("F1", "QuickCD"),
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
    ("Tab", "Pane"),
    ("Spc", "Mark"),
    ("Esc", "Normal"),
];

pub fn draw(frame: &mut Frame, area: Rect, git_mode: bool, palette: &Palette) {
    let keys = if git_mode { GIT_KEYS } else { NORMAL_KEYS };
    let key_style = if git_mode { palette.funcbar_git } else { palette.funcbar_normal };
    let mut spans = Vec::new();
    for (key, label) in keys {
        spans.push(Span::styled(*key, key_style));
        spans.push(Span::raw(format!("{} ", label)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
