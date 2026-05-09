use std::path::PathBuf;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

// --- Deferred operations resolved on dialog confirmation ---

#[derive(Debug, Clone)]
pub enum DeferredOp {
    Delete(Vec<PathBuf>),
    Copy { sources: Vec<PathBuf> },
    Move { sources: Vec<PathBuf> },
    Mkdir { base: PathBuf },
    Execute { path: PathBuf },
    OpenInNano { base: PathBuf },
    ChownFiles { paths: Vec<PathBuf>, reload_sides: Vec<crate::action::Side> },
}

// --- Context menu ---

#[derive(Debug, Clone)]
pub enum MenuAction {
    OpenWithOs(PathBuf),
    RunCodeHere(PathBuf),
    RequestExecute(PathBuf), // opens an args Input dialog, then runs
    ExtractHere { archive: PathBuf, dest: PathBuf },
    MountDevice { device: String },
    UnmountDevice { device: String },
    Chown { paths: Vec<PathBuf>, current_owner: String, reload_sides: Vec<crate::action::Side> },
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub action: MenuAction,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, action: MenuAction) -> Self {
        Self {
            label: label.into(),
            action,
        }
    }
}

// --- Dialog state ---

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
    ContextMenu {
        title: String,
        items: Vec<MenuItem>,
        selected: usize,
    },
    QuickCd {
        input: String,
        matches: Vec<String>, // sorted dir names in the current parent
        selected: usize,
        base_path: PathBuf, // active panel's path at open time (for relative resolution)
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

    pub fn context_menu(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self::ContextMenu {
            title: title.into(),
            items,
            selected: 0,
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

    pub fn nav_up(&mut self) {
        match self {
            Self::ContextMenu { selected, .. } | Self::QuickCd { selected, .. }
                if *selected > 0 =>
            {
                *selected -= 1;
            }
            _ => {}
        }
    }

    pub fn nav_down(&mut self) {
        match self {
            Self::ContextMenu {
                selected, items, ..
            } if *selected + 1 < items.len() => {
                *selected += 1;
            }
            Self::QuickCd {
                selected, matches, ..
            } if *selected + 1 < matches.len() => {
                *selected += 1;
            }
            _ => {}
        }
    }

    pub fn is_quick_cd(&self) -> bool {
        matches!(self, Self::QuickCd { .. })
    }
}

// --- Rendering ---

pub fn draw(frame: &mut Frame, dialog: &DialogState, area: Rect) {
    match dialog {
        DialogState::Confirm { title, message, .. } => {
            let popup = centered_rect(64, 8, area);
            frame.render_widget(Clear, popup);
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
            let popup = centered_rect(64, 8, area);
            frame.render_widget(Clear, popup);
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
                Paragraph::new(prompt.as_str())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                prompt_area,
            );
            frame.render_widget(
                Paragraph::new(format!("{}_", value)).style(Style::default().fg(Color::White)),
                value_area,
            );
            frame.render_widget(
                Paragraph::new("[ Enter ] OK   [ Esc ] Cancel")
                    .alignment(Alignment::Center)
                    .style(Style::default().add_modifier(Modifier::DIM)),
                hint_area,
            );
        }

        DialogState::QuickCd {
            input,
            matches,
            selected,
            ..
        } => {
            let height = (matches.len() as u16 + 6)
                .clamp(10, 24)
                .min(area.height.saturating_sub(2));
            let popup = centered_rect(66, height, area);
            frame.render_widget(Clear, popup);

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta))
                .title(" Quick CD ");
            let inner = block.inner(popup);
            frame.render_widget(block, popup);

            let [input_area, sep_area, list_area, hint_area] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .areas(inner);

            // Input line
            frame.render_widget(
                Paragraph::new(format!("{}_", input)).style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                input_area,
            );

            // Separator
            frame.render_widget(
                Paragraph::new("─".repeat(inner.width as usize))
                    .style(Style::default().add_modifier(Modifier::DIM)),
                sep_area,
            );

            // Match list
            let list_items: Vec<ListItem> = matches
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let style = if i == *selected {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::Cyan)
                    };
                    ListItem::new(format!(" {}/", name)).style(style)
                })
                .collect();

            let mut list_state = ListState::default();
            if !matches.is_empty() {
                list_state.select(Some(*selected));
            }
            frame.render_stateful_widget(List::new(list_items), list_area, &mut list_state);

            // Hint
            frame.render_widget(
                Paragraph::new("↑↓ select   Tab complete   ↵ cd   Esc cancel")
                    .alignment(Alignment::Center)
                    .style(Style::default().add_modifier(Modifier::DIM)),
                hint_area,
            );
        }

        DialogState::ContextMenu {
            title,
            items,
            selected,
        } => {
            let height = (items.len() as u16 + 4).min(area.height.saturating_sub(2));
            let popup = centered_rect(50, height, area);
            frame.render_widget(Clear, popup);

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green))
                .title(format!(" {} ", title));
            let inner = block.inner(popup);
            frame.render_widget(block, popup);

            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let style = if i == *selected {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!(" {} ", item.label)).style(style)
                })
                .collect();

            let mut state = ListState::default();
            state.select(Some(*selected));

            frame.render_stateful_widget(List::new(list_items), inner, &mut state);
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
