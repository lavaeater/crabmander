//! The `?` help overlay: every action of the active context with all its keys in effect
//! (contracts/ui-hints.md "Help screen").

use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{
    action::Action,
    keymap::{self, KeyContext, KeyStyle, Keymap},
    palette::Palette,
};

/// Footer shown when the terminal cannot tell Alt+Shift+digit from Alt+symbol.
pub const LEGACY_NOTE: &str =
    "This terminal cannot report Alt+Shift+digit; see README to bind symbols instead.";

/// One row of the help list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Group(&'static str),
    Action {
        keys: String,
        description: &'static str,
    },
}

/// The rows for a context chain: actions grouped by help group (groups in order of first
/// appearance), each with all its keys in full notation, or "(unbound)".
pub fn rows(keymap: &Keymap, chain: &[KeyContext]) -> Vec<Row> {
    let mut groups: Vec<(&'static str, Vec<Row>)> = Vec::new();
    let mut seen: Vec<&Action> = Vec::new();
    for spec in chain.iter().flat_map(|ctx| keymap::catalog_for(*ctx)) {
        if seen.contains(&&spec.action) {
            continue;
        }
        seen.push(&spec.action);
        let keys = keymap.keys_in_chain(chain, &spec.action);
        let keys = if keys.is_empty() {
            "(unbound)".to_owned()
        } else {
            keys.iter()
                .map(|k| keymap::seq_name(k, KeyStyle::Full))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let row = Row::Action {
            keys,
            description: spec.description,
        };
        match groups.iter_mut().find(|(g, _)| *g == spec.group) {
            Some((_, rows)) => rows.push(row),
            None => groups.push((spec.group, vec![row])),
        }
    }
    groups
        .into_iter()
        .flat_map(|(group, rows)| std::iter::once(Row::Group(group)).chain(rows))
        .collect()
}

/// Title naming the context, e.g. "Keys — Git mode".
pub fn title(chain: &[KeyContext]) -> String {
    format!(" Keys — {} mode ", chain[0])
}

/// Draws the overlay. `scroll` is clamped so the last row stays at the bottom.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    keymap: &Keymap,
    chain: &[KeyContext],
    scroll: usize,
    keyboard_enhanced: bool,
    palette: &Palette,
) {
    let rows = rows(keymap, chain);
    let key_width = rows
        .iter()
        .filter_map(|r| match r {
            Row::Action { keys, .. } => Some(Span::raw(keys.as_str()).width()),
            Row::Group(_) => None,
        })
        .max()
        .unwrap_or(0)
        .min(40);

    let width = area.width.saturating_sub(4).min(100);
    let height = area.height.saturating_sub(2);
    let popup = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title(chain))
        .border_style(palette.dlg_menu);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let footer = if keyboard_enhanced {
        vec![]
    } else {
        vec![Line::from(Span::styled(
            LEGACY_NOTE,
            Style::default().add_modifier(Modifier::DIM),
        ))]
    };
    let [list_area, footer_area] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(u16::try_from(footer.len()).unwrap_or(0)),
    ])
    .areas(inner);

    let visible = usize::from(list_area.height);
    let scroll = scroll.min(rows.len().saturating_sub(visible));
    let lines: Vec<Line> = rows
        .iter()
        .skip(scroll)
        .take(visible)
        .map(|row| match row {
            Row::Group(g) => Line::from(Span::styled(
                *g,
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Row::Action { keys, description } => Line::from(vec![
                Span::styled(format!("  {keys:<key_width$}  "), palette.funcbar_normal),
                Span::raw(*description),
            ]),
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);
    frame.render_widget(Paragraph::new(footer), footer_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::RawBindings;
    use pretty_assertions::assert_eq;

    fn shipped() -> Keymap {
        Keymap::build(&crate::config::default_keybindings(), &RawBindings::new())
    }

    fn keys_of(rows: &[Row], description: &str) -> Option<String> {
        rows.iter().find_map(|r| match r {
            Row::Action {
                keys,
                description: d,
            } if *d == description => Some(keys.clone()),
            _ => None,
        })
    }

    #[test]
    fn lists_all_keys_in_full_notation_grouped() {
        let rows = rows(&shipped(), &[KeyContext::Normal]);
        assert_eq!(rows[0], Row::Group("Navigation"));
        assert_eq!(
            keys_of(&rows, "Copy to other panel").as_deref(),
            Some("F5, Alt+5, Ctrl+F 5")
        );
        assert_eq!(
            keys_of(&rows, "Rename in place").as_deref(),
            Some("Shift+F6, Alt+Shift+6, Ctrl+F s 6")
        );
        // Each group appears once.
        let groups: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Group(g) => Some(*g),
                Row::Action { .. } => None,
            })
            .collect();
        let mut deduped = groups.clone();
        deduped.dedup();
        assert_eq!(groups.len(), deduped.len());
    }

    #[test]
    fn unbound_actions_say_so() {
        let km = Keymap::build(&RawBindings::new(), &RawBindings::new());
        let rows = rows(&km, &[KeyContext::Cargo]);
        assert_eq!(
            keys_of(&rows, "Run selected command").as_deref(),
            Some("(unbound)")
        );
    }

    #[test]
    fn fallback_context_actions_are_included_once() {
        let rows = rows(&shipped(), &[KeyContext::DialogConfirm, KeyContext::Dialog]);
        // "Yes" from DialogConfirm wins over Dialog's "Confirm" for the same action, with the
        // Enter key from the fallback context listed after y/Y.
        assert_eq!(keys_of(&rows, "Yes").as_deref(), Some("Y, y, Enter"));
        assert_eq!(keys_of(&rows, "Confirm"), None);
        assert!(keys_of(&rows, "Previous item").is_some());
    }

    #[test]
    fn title_names_the_context() {
        assert_eq!(title(&[KeyContext::Git]), " Keys — Git mode ");
    }
}
