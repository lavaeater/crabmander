use std::path::PathBuf;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

pub use crate::ops::DeferredOp;

// --- Context menu ---

#[derive(Debug, Clone)]
pub enum MenuAction {
    OpenWithOs(PathBuf),
    RunCodeHere(PathBuf),
    RequestExecute(PathBuf), // opens an args Input dialog, then runs
    ExtractHere { archives: Vec<PathBuf>, dest: PathBuf },
    MountDevice { device: String },
    UnmountDevice { device: String },
    Chown { paths: Vec<PathBuf>, current_owner: String, reload_sides: Vec<crate::action::Side> },
    SetTheme(String),
    CdTo(std::path::PathBuf),
    GitPush { follow_tags: bool },
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

#[derive(Debug)]
pub enum DialogState {
    Confirm {
        title: String,
        message: String,
        op: Box<dyn DeferredOp>,
    },
    Input {
        title: String,
        prompt: String,
        value: String,
        op: Box<dyn DeferredOp>,
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
    ErrorList {
        title: String,
        errors: Vec<String>,
        scroll: usize,
    },
}

impl DialogState {
    pub fn confirm(title: impl Into<String>, message: impl Into<String>, op: Box<dyn DeferredOp>) -> Self {
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
        op: Box<dyn DeferredOp>,
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

    pub fn error_list(title: impl Into<String>, errors: Vec<String>) -> Self {
        Self::ErrorList {
            title: title.into(),
            errors,
            scroll: 0,
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
            Self::ErrorList { scroll, .. } if *scroll > 0 => {
                *scroll -= 1;
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
            Self::ErrorList {
                scroll, errors, ..
            } if *scroll + 1 < errors.len() => {
                *scroll += 1;
            }
            _ => {}
        }
    }

    pub fn is_quick_cd(&self) -> bool {
        matches!(self, Self::QuickCd { .. })
    }
}

// --- Rendering ---

pub fn draw(frame: &mut Frame, dialog: &DialogState, area: Rect, palette: &crate::palette::Palette) {
    match dialog {
        DialogState::Confirm { title, message, .. } => {
            let popup = centered_rect(64, 8, area);
            frame.render_widget(Clear, popup);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(palette.dlg_confirm)
                .title(format!(" {} ", title));
            let text = format!("\n{}\n\n[ Yes / Enter ]   [ No / Esc ]", message);
            let para = Paragraph::new(text)
                .block(block)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false });
            frame.render_widget(para, popup);
        }

        DialogState::Input { title, prompt, value, .. } => {
            let popup = centered_rect(64, 8, area);
            frame.render_widget(Clear, popup);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(palette.dlg_input)
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
            frame.render_widget(
                Paragraph::new(format!("{}_", value)),
                value_area,
            );
            frame.render_widget(
                Paragraph::new("[ Enter ] OK   [ Esc ] Cancel")
                    .alignment(Alignment::Center)
                    .style(Style::default().add_modifier(Modifier::DIM)),
                hint_area,
            );
        }

        DialogState::QuickCd { input, matches, selected, .. } => {
            let height = (matches.len() as u16 + 6)
                .clamp(10, 24)
                .min(area.height.saturating_sub(2));
            let popup = centered_rect(66, height, area);
            frame.render_widget(Clear, popup);

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(palette.dlg_qcd)
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

            frame.render_widget(
                Paragraph::new(format!("{}_", input))
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                input_area,
            );
            frame.render_widget(
                Paragraph::new("─".repeat(inner.width as usize))
                    .style(Style::default().add_modifier(Modifier::DIM)),
                sep_area,
            );

            let list_items: Vec<ListItem> = matches
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let style = if i == *selected {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        palette.entry_dir
                    };
                    ListItem::new(format!(" {}/", name)).style(style)
                })
                .collect();

            let mut list_state = ListState::default();
            if !matches.is_empty() {
                list_state.select(Some(*selected));
            }
            frame.render_stateful_widget(List::new(list_items), list_area, &mut list_state);

            frame.render_widget(
                Paragraph::new("↑↓ select   Tab complete   ↵ cd   Esc cancel")
                    .alignment(Alignment::Center)
                    .style(Style::default().add_modifier(Modifier::DIM)),
                hint_area,
            );
        }

        DialogState::ContextMenu { title, items, selected } => {
            let height = (items.len() as u16 + 4).min(area.height.saturating_sub(2));
            let popup = centered_rect(50, height, area);
            frame.render_widget(Clear, popup);

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(palette.dlg_menu)
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

        DialogState::ErrorList { title, errors, scroll } => {
            let visible_lines = 10u16;
            let height = (visible_lines + 4).min(area.height.saturating_sub(2));
            let popup = centered_rect(72, height, area);
            frame.render_widget(Clear, popup);

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(palette.dlg_error)
                .title(format!(" {} ", title));
            let inner = block.inner(popup);
            frame.render_widget(block, popup);

            let [list_area, hint_area] =
                Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);

            let list_items: Vec<ListItem> = errors
                .iter()
                .skip(*scroll)
                .take(list_area.height as usize)
                .map(|msg| ListItem::new(format!(" {}", msg)).style(palette.status_error))
                .collect();

            frame.render_widget(List::new(list_items), list_area);

            let scroll_hint = if errors.len() > list_area.height as usize {
                format!("↑↓ scroll ({}/{})   ", *scroll + 1, errors.len())
            } else {
                String::new()
            };
            frame.render_widget(
                Paragraph::new(format!("{}[ Enter / Esc ] Close", scroll_hint))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_op() -> Box<dyn DeferredOp> {
        #[derive(Debug)]
        struct Noop;
        impl DeferredOp for Noop {
            fn execute(self: Box<Self>, _ctx: crate::ops::OpCtx) {}
        }
        Box::new(Noop)
    }

    fn menu_items(n: usize) -> Vec<MenuItem> {
        (0..n)
            .map(|i| MenuItem::new(format!("item {i}"), MenuAction::CdTo("/".into())))
            .collect()
    }

    // --- constructors ---

    #[test]
    fn confirm_constructor_stores_fields() {
        let d = DialogState::confirm("Delete", "Are you sure?", dummy_op());
        if let DialogState::Confirm { title, message, .. } = d {
            assert_eq!(title, "Delete");
            assert_eq!(message, "Are you sure?");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn input_constructor_stores_fields() {
        let d = DialogState::input("Copy", "Copy to:", "/dest", dummy_op());
        if let DialogState::Input { title, prompt, value, .. } = d {
            assert_eq!(title, "Copy");
            assert_eq!(prompt, "Copy to:");
            assert_eq!(value, "/dest");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn context_menu_constructor_starts_at_zero() {
        let d = DialogState::context_menu("Menu", menu_items(5));
        if let DialogState::ContextMenu { selected, items, .. } = &d {
            assert_eq!(*selected, 0);
            assert_eq!(items.len(), 5);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn error_list_constructor_scroll_starts_at_zero() {
        let d = DialogState::error_list("Errors", vec!["e1".into(), "e2".into()]);
        if let DialogState::ErrorList { scroll, errors, .. } = &d {
            assert_eq!(*scroll, 0);
            assert_eq!(errors.len(), 2);
        } else {
            panic!("wrong variant");
        }
    }

    // --- push_char / pop_char ---

    #[test]
    fn push_char_appends_to_input_value() {
        let mut d = DialogState::input("T", "P", "he", dummy_op());
        d.push_char('y');
        if let DialogState::Input { value, .. } = &d {
            assert_eq!(value, "hey");
        }
    }

    #[test]
    fn push_char_is_noop_on_confirm() {
        let mut d = DialogState::confirm("T", "m", dummy_op());
        d.push_char('x'); // must not panic
        matches!(d, DialogState::Confirm { .. });
    }

    #[test]
    fn pop_char_removes_last_character() {
        let mut d = DialogState::input("T", "P", "hello", dummy_op());
        d.pop_char();
        if let DialogState::Input { value, .. } = &d {
            assert_eq!(value, "hell");
        }
    }

    #[test]
    fn pop_char_on_empty_input_is_noop() {
        let mut d = DialogState::input("T", "P", "", dummy_op());
        d.pop_char(); // must not panic
        if let DialogState::Input { value, .. } = &d {
            assert_eq!(value, "");
        }
    }

    // --- ContextMenu nav ---

    #[test]
    fn context_menu_nav_down_increments_selected() {
        let mut d = DialogState::context_menu("M", menu_items(3));
        d.nav_down();
        if let DialogState::ContextMenu { selected, .. } = &d {
            assert_eq!(*selected, 1);
        }
    }

    #[test]
    fn context_menu_nav_down_at_last_item_stays() {
        let mut d = DialogState::context_menu("M", menu_items(2));
        d.nav_down();
        d.nav_down(); // now at last (index 1)
        d.nav_down(); // must not go past last
        if let DialogState::ContextMenu { selected, .. } = &d {
            assert_eq!(*selected, 1);
        }
    }

    #[test]
    fn context_menu_nav_up_at_zero_stays() {
        let mut d = DialogState::context_menu("M", menu_items(3));
        d.nav_up();
        if let DialogState::ContextMenu { selected, .. } = &d {
            assert_eq!(*selected, 0);
        }
    }

    #[test]
    fn context_menu_nav_up_decrements_selected() {
        let mut d = DialogState::context_menu("M", menu_items(3));
        d.nav_down();
        d.nav_down(); // selected = 2
        d.nav_up();   // selected = 1
        if let DialogState::ContextMenu { selected, .. } = &d {
            assert_eq!(*selected, 1);
        }
    }

    // --- QuickCd nav ---

    #[test]
    fn quick_cd_nav_down_increments_selected() {
        let mut d = DialogState::QuickCd {
            input: String::new(),
            matches: vec!["a".into(), "b".into(), "c".into()],
            selected: 0,
            base_path: "/tmp".into(),
        };
        d.nav_down();
        if let DialogState::QuickCd { selected, .. } = &d {
            assert_eq!(*selected, 1);
        }
    }

    #[test]
    fn quick_cd_nav_down_at_last_stays() {
        let mut d = DialogState::QuickCd {
            input: String::new(),
            matches: vec!["a".into(), "b".into()],
            selected: 1,
            base_path: "/tmp".into(),
        };
        d.nav_down();
        if let DialogState::QuickCd { selected, .. } = &d {
            assert_eq!(*selected, 1);
        }
    }

    #[test]
    fn quick_cd_nav_up_at_zero_stays() {
        let mut d = DialogState::QuickCd {
            input: String::new(),
            matches: vec!["a".into()],
            selected: 0,
            base_path: "/tmp".into(),
        };
        d.nav_up();
        if let DialogState::QuickCd { selected, .. } = &d {
            assert_eq!(*selected, 0);
        }
    }

    // --- ErrorList nav ---

    #[test]
    fn error_list_nav_down_increments_scroll() {
        let mut d = DialogState::error_list("E", vec!["e1".into(), "e2".into(), "e3".into()]);
        d.nav_down();
        if let DialogState::ErrorList { scroll, .. } = &d {
            assert_eq!(*scroll, 1);
        }
    }

    #[test]
    fn error_list_nav_up_decrements_scroll() {
        let mut d = DialogState::error_list("E", vec!["e1".into(), "e2".into()]);
        d.nav_down();
        d.nav_up();
        if let DialogState::ErrorList { scroll, .. } = &d {
            assert_eq!(*scroll, 0);
        }
    }

    #[test]
    fn error_list_nav_down_at_last_stays() {
        let mut d = DialogState::error_list("E", vec!["only".into()]);
        d.nav_down();
        if let DialogState::ErrorList { scroll, .. } = &d {
            assert_eq!(*scroll, 0);
        }
    }

    // --- is_quick_cd ---

    #[test]
    fn is_quick_cd_true_for_quick_cd_variant() {
        let d = DialogState::QuickCd {
            input: String::new(),
            matches: vec![],
            selected: 0,
            base_path: "/tmp".into(),
        };
        assert!(d.is_quick_cd());
    }

    #[test]
    fn is_quick_cd_false_for_other_variants() {
        assert!(!DialogState::confirm("T", "m", dummy_op()).is_quick_cd());
        assert!(!DialogState::input("T", "p", "v", dummy_op()).is_quick_cd());
        assert!(!DialogState::context_menu("M", vec![]).is_quick_cd());
        assert!(!DialogState::error_list("E", vec![]).is_quick_cd());
    }
}
