use std::path::PathBuf;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::palette::Palette;

// --- Static cargo commands ---

struct CargoCommand {
    label: &'static str,
    subcommand: &'static str,
    extra_args: &'static str,
}

const CARGO_COMMANDS: &[CargoCommand] = &[
    CargoCommand {
        label: "build",
        subcommand: "build",
        extra_args: "",
    },
    CargoCommand {
        label: "build --release",
        subcommand: "build",
        extra_args: "--release",
    },
    CargoCommand {
        label: "check",
        subcommand: "check",
        extra_args: "",
    },
    CargoCommand {
        label: "run",
        subcommand: "run",
        extra_args: "",
    },
    CargoCommand {
        label: "run --release",
        subcommand: "run",
        extra_args: "--release",
    },
    CargoCommand {
        label: "test",
        subcommand: "test",
        extra_args: "",
    },
    CargoCommand {
        label: "test --all-features",
        subcommand: "test",
        extra_args: "--all-features",
    },
    CargoCommand {
        label: "clippy",
        subcommand: "clippy",
        extra_args: "",
    },
    CargoCommand {
        label: "clippy --all-features -D warnings",
        subcommand: "clippy",
        extra_args: "--all-features -- -D warnings",
    },
    CargoCommand {
        label: "fmt --all",
        subcommand: "fmt",
        extra_args: "--all",
    },
    CargoCommand {
        label: "doc --open",
        subcommand: "doc",
        extra_args: "--open",
    },
    CargoCommand {
        label: "clean",
        subcommand: "clean",
        extra_args: "",
    },
    CargoCommand {
        label: "update",
        subcommand: "update",
        extra_args: "",
    },
];

// --- Dynamic command list entry ---

/// A selectable entry in the project command picker.
pub enum Entry {
    /// A `cargo <subcommand> [extra_args]` invocation.
    Cargo { idx: usize },
}

impl Entry {
    const fn is_selectable(&self) -> bool {
        match self {
            Entry::Cargo { .. } => true,
        }
    }
}

// --- CargoView ---

pub struct CargoView {
    pub cargo_root: PathBuf,
    entries: Vec<Entry>,
    /// Index into `entries` pointing at the highlighted item (always selectable).
    pub cursor: usize,
}

impl CargoView {
    pub fn new(cargo_root: PathBuf) -> Self {
        let entries: Vec<Entry> = (0..CARGO_COMMANDS.len())
            .map(|idx| Entry::Cargo { idx })
            .collect();
        Self {
            cargo_root,
            entries,
            cursor: 0,
        }
    }

    fn selectable_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_selectable()).count()
    }

    /// Map a logical cursor position (0-based among selectable entries) to the
    /// real index in `self.entries`.
    fn real_index(&self) -> usize {
        let mut sel = 0usize;
        for (i, e) in self.entries.iter().enumerate() {
            if e.is_selectable() {
                if sel == self.cursor {
                    return i;
                }
                sel += 1;
            }
        }
        0
    }

    pub fn nav_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn nav_down(&mut self) {
        self.cursor = (self.cursor + 1).min(self.selectable_count().saturating_sub(1));
    }

    /// Returns the shell command string for the currently selected entry.
    /// Caller is responsible for `cd`-ing to `cargo_root` first.
    pub fn selected_shell_cmd(&self) -> String {
        match &self.entries[self.real_index()] {
            Entry::Cargo { idx } => {
                let c = &CARGO_COMMANDS[*idx];
                if c.extra_args.is_empty() {
                    format!("cargo {}", c.subcommand)
                } else {
                    format!("cargo {} {}", c.subcommand, c.extra_args)
                }
            }
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, palette: &Palette) {
        let popup = centered_rect(50, 80, area);
        frame.render_widget(Clear, popup);

        let title = format!(" Project: {} ", self.cargo_root.display());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(palette.cargo_border)
            .title(title);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let list_h = inner.height.saturating_sub(1) as usize;

        // Compute display offset so the highlighted row stays visible.
        let real_cur = self.real_index();
        let offset = if real_cur < list_h {
            0
        } else {
            real_cur + 1 - list_h
        };

        let mut sel_idx = 0usize; // running selectable counter for cursor matching
        // pre-count selectable entries before offset so we know sel_idx at offset
        for e in self.entries.iter().take(offset) {
            if e.is_selectable() {
                sel_idx += 1;
            }
        }

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .skip(offset)
            .take(list_h)
            .map(|(_, entry)| match entry {
                Entry::Cargo { idx } => {
                    let cmd = &CARGO_COMMANDS[*idx];
                    let label = format!("  cargo {}", cmd.label);
                    let is_cur = sel_idx == self.cursor;
                    sel_idx += 1;
                    let style = if is_cur {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    ListItem::new(label).style(style)
                }
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

        let list_area = Rect {
            height: list_h as u16,
            ..inner
        };
        frame.render_widget(List::new(items), list_area);
    }
}

// --- Layout ---

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = (area.width * percent_x / 100).max(40).min(area.width);
    let h = (area.height * percent_y / 100).max(10).min(area.height);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}
