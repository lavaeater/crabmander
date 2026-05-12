use ratatui::{prelude::*, widgets::Paragraph};

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

pub fn draw(frame: &mut Frame, area: Rect, git_mode: bool) {
    let keys = if git_mode { GIT_KEYS } else { NORMAL_KEYS };
    let mut spans = Vec::new();
    for (key, label) in keys {
        let key_style = Style::default().fg(Color::Black).bg(if git_mode {
            Color::LightGreen
        } else {
            Color::Cyan
        });
        spans.push(Span::styled(*key, key_style));
        spans.push(Span::raw(format!("{} ", label)));
    }
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}
