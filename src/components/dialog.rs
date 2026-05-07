use std::path::PathBuf;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

#[derive(Debug, Clone)]
pub enum DeferredOp {
    Delete(Vec<PathBuf>),
    Copy { sources: Vec<PathBuf> },
    Move { sources: Vec<PathBuf> },
    Mkdir { base: PathBuf },
}

#[derive(Debug, Clone)]
pub enum DialogState {
    Confirm {
        title: String,
        message: String,
        op: DeferredOp,
    },
    Input {
        title: String,
        prompt: String,
        value: String,
        op: DeferredOp,
    },
}

impl DialogState {
    pub fn confirm(title: impl Into<String>, message: impl Into<String>, op: DeferredOp) -> Self {
        Self::Confirm {
            title: title.into(),
            message: message.into(),
            op,
        }
    }

    pub fn input(
        title: impl Into<String>,
        prompt: impl Into<String>,
        default: impl Into<String>,
        op: DeferredOp,
    ) -> Self {
        Self::Input {
            title: title.into(),
            prompt: prompt.into(),
            value: default.into(),
            op,
        }
    }

    pub fn push_char(&mut self, c: char) {
        if let Self::Input { value, .. } = self {
            value.push(c);
        }
    }

    pub fn pop_char(&mut self) {
        if let Self::Input { value, .. } = self {
            value.pop();
        }
    }
}

pub fn draw(frame: &mut Frame, dialog: &DialogState, area: Rect) {
    let popup = centered_rect(62, 10, area);
    frame.render_widget(Clear, popup);

    match dialog {
        DialogState::Confirm { title, message, .. } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(format!(" {} ", title));

            let text = format!("\n{}\n\n[ Yes / Enter ]   [ No / Esc ]", message);
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false });
            frame.render_widget(para, popup);
        }

        DialogState::Input {
            title,
            prompt,
            value,
            ..
        } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" {} ", title));

            let inner = block.inner(popup);
            frame.render_widget(block, popup);

            let [prompt_area, value_area, _, hint_area] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .areas(inner);

            frame.render_widget(
                Paragraph::new(prompt.as_str()).style(Style::default().add_modifier(Modifier::BOLD)),
                prompt_area,
            );

            // Value with a trailing cursor block
            let display = format!("{}_", value);
            frame.render_widget(
                Paragraph::new(display).style(Style::default().fg(Color::White)),
                value_area,
            );

            frame.render_widget(
                Paragraph::new("[ Enter ] OK   [ Esc ] Cancel")
                    .alignment(Alignment::Center)
                    .style(Style::default().add_modifier(Modifier::DIM)),
                hint_area,
            );
        }
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
