use std::path::PathBuf;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::palette::Palette;

struct CargoCommand {
    label: &'static str,
    subcommand: &'static str,
    extra_args: &'static str,
}

const COMMANDS: &[CargoCommand] = &[
    CargoCommand { label: "build", subcommand: "build", extra_args: "" },
    CargoCommand { label: "build --release", subcommand: "build", extra_args: "--release" },
    CargoCommand { label: "check", subcommand: "check", extra_args: "" },
    CargoCommand { label: "run", subcommand: "run", extra_args: "" },
    CargoCommand { label: "run --release", subcommand: "run", extra_args: "--release" },
    CargoCommand { label: "test", subcommand: "test", extra_args: "" },
    CargoCommand { label: "test --all-features", subcommand: "test", extra_args: "--all-features" },
    CargoCommand { label: "clippy", subcommand: "clippy", extra_args: "" },
    CargoCommand {
        label: "clippy --all-features -D warnings",
        subcommand: "clippy",
        extra_args: "--all-features -- -D warnings",
    },
    CargoCommand { label: "fmt --all", subcommand: "fmt", extra_args: "--all" },
    CargoCommand { label: "doc --open", subcommand: "doc", extra_args: "--open" },
    CargoCommand { label: "clean", subcommand: "clean", extra_args: "" },
    CargoCommand { label: "update", subcommand: "update", extra_args: "" },
];

pub struct CargoView {
    pub cargo_root: PathBuf,
    pub cursor: usize,
}

impl CargoView {
    pub fn new(cargo_root: PathBuf) -> Self {
        Self { cargo_root, cursor: 0 }
    }

    pub fn nav_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn nav_down(&mut self) {
        self.cursor = (self.cursor + 1).min(COMMANDS.len().saturating_sub(1));
    }

    /// Returns `(subcommand, extra_args)` for the currently selected command.
    pub fn selected_args(&self) -> (&'static str, &'static str) {
        let cmd = &COMMANDS[self.cursor];
        (cmd.subcommand, cmd.extra_args)
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, palette: &Palette) {
        let popup = centered_rect(50, 80, area);
        frame.render_widget(Clear, popup);

        let title = format!(
            " Cargo: {} ",
            self.cargo_root.display()
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(palette.cargo_border)
            .title(title);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let list_h = inner.height.saturating_sub(1) as usize;
        let offset = if self.cursor < list_h { 0 } else { self.cursor + 1 - list_h };

        let items: Vec<ListItem> = COMMANDS
            .iter()
            .enumerate()
            .skip(offset)
            .take(list_h)
            .map(|(i, cmd)| {
                let label = format!("  cargo {} ", cmd.label);
                let style = if i == self.cursor {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                ListItem::new(label).style(style)
            })
            .collect();

        let hint_area = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("  Enter: run  ↑↓/jk: navigate  ^B/Esc: exit")
                .style(Style::default().add_modifier(Modifier::DIM)),
            hint_area,
        );

        let list_area = Rect { height: list_h as u16, ..inner };
        frame.render_widget(List::new(items), list_area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = (area.width * percent_x / 100).max(40).min(area.width);
    let h = (area.height * percent_y / 100).max(10).min(area.height);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}
