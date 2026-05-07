use ratatui::{prelude::*, widgets::Paragraph};

const KEYS: &[(&str, &str, bool)] = &[
    ("F1", "QuickCD", true),
    ("F2", "Menu", true),
    ("F3", "Nano", true),
    ("F4", "Sizes", true),
    ("F5", "Copy", true),
    ("F6", "Move", true),
    ("F7", "MkDir", true),
    ("F8", "Delete", true),
    ("F9", "Sort", true),
    ("F10", "Quit", true),
];

pub fn draw(frame: &mut Frame, area: Rect) {
    let mut spans = Vec::new();
    for (key, label, enabled) in KEYS {
        let key_style = if *enabled {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default()
                .fg(Color::Black)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::DIM)
        };
        let label_style = if *enabled {
            Style::default()
        } else {
            Style::default().add_modifier(Modifier::DIM)
        };
        spans.push(Span::styled(*key, key_style));
        spans.push(Span::styled(format!("{} ", label), label_style));
    }
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}
