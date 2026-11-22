//! The bottom key-hint bar, generated from the keys in effect for the active context
//! (contracts/ui-hints.md "Function bar").

use ratatui::{prelude::*, widgets::Paragraph};

use crate::{
    action::Action,
    keymap::{self, KeyContext, KeyStyle, Keymap},
};

/// One hint: keys (styled) followed by a label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub keys: String,
    pub label: &'static str,
}

impl Entry {
    /// Columns taken, including the space after the label.
    fn width(&self) -> usize {
        Span::raw(self.keys.as_str()).width() + Span::raw(self.label).width() + 1
    }
}

/// Keys shown for one action: the first `max` keys in display order (F-key first), as `F5/Alt+5`.
fn keys_text(keymap: &Keymap, chain: &[KeyContext], action: &Action, max: usize) -> Option<String> {
    let keys = keymap.keys_in_chain(chain, action);
    if keys.is_empty() {
        return None;
    }
    Some(
        keys.iter()
            .take(max)
            .map(|k| keymap::seq_name(k, KeyStyle::Compact))
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Bar entries of the chain's actions in priority order (actions without keys left out), and the
/// `Help` entry if Help has a key.
pub fn entries(
    keymap: &Keymap,
    chain: &[KeyContext],
    keys_per_entry: usize,
) -> (Vec<Entry>, Option<Entry>) {
    let mut specs: Vec<_> = chain
        .iter()
        .enumerate()
        .flat_map(|(i, ctx)| {
            keymap::catalog_for(*ctx).filter_map(move |s| s.bar_priority.map(|p| ((i, p), s)))
        })
        .collect();
    specs.sort_by_key(|(order, _)| *order);

    let mut seen: Vec<&Action> = Vec::new();
    let mut out = Vec::new();
    for (_, spec) in specs {
        if seen.contains(&&spec.action) {
            continue;
        }
        seen.push(&spec.action);
        if let Some(keys) = keys_text(keymap, chain, &spec.action, keys_per_entry) {
            out.push(Entry {
                keys,
                label: spec.label,
            });
        }
    }
    let help = keys_text(keymap, chain, &Action::Help, 1).map(|keys| Entry {
        keys,
        label: "Help",
    });
    (out, help)
}

/// Keeps the highest-priority entries that fit in `width`, whole, with `help` last if it fits.
pub fn fit(entries: Vec<Entry>, help: Option<Entry>, width: usize) -> Vec<Entry> {
    let help_width = help.as_ref().map_or(0, Entry::width);
    let (budget, help) = if help_width <= width {
        (width - help_width, help)
    } else {
        (width, None)
    };
    let mut used = 0;
    let mut out: Vec<Entry> = entries
        .into_iter()
        .take_while(|e| {
            used += e.width();
            used <= budget
        })
        .collect();
    out.extend(help);
    out
}

/// The entries to draw: two keys per entry if everything fits that way, otherwise one key per
/// entry, dropping the lowest-priority entries if even that is too wide.
pub fn layout(keymap: &Keymap, chain: &[KeyContext], width: usize) -> Vec<Entry> {
    let (two, help) = entries(keymap, chain, 2);
    let total: usize = two.iter().chain(&help).map(Entry::width).sum();
    if total <= width {
        return two.into_iter().chain(help).collect();
    }
    let (one, help) = entries(keymap, chain, 1);
    fit(one, help, width)
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    keymap: &Keymap,
    chain: &[KeyContext],
    key_style: Style,
) {
    let mut spans = Vec::new();
    for entry in layout(keymap, chain, usize::from(area.width)) {
        spans.push(Span::styled(entry.keys, key_style));
        spans.push(Span::raw(format!("{} ", entry.label)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::RawBindings;
    use pretty_assertions::assert_eq;

    fn keymap(entries: &[(&str, &str)]) -> Keymap {
        let defaults: RawBindings = [(
            "Normal".to_owned(),
            entries
                .iter()
                .map(|(k, a)| ((*k).to_owned(), Some((*a).to_owned())))
                .collect(),
        )]
        .into();
        Keymap::build(&defaults, &RawBindings::new())
    }

    fn texts(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| format!("{} {}", e.keys, e.label))
            .collect()
    }

    const NORMAL: [KeyContext; 1] = [KeyContext::Normal];

    #[test]
    fn entries_in_priority_order_f_key_first_unbound_hidden() {
        let km = keymap(&[
            ("<ctrl-f><5>", "Copy"),
            ("<alt-5>", "Copy"),
            ("<f5>", "Copy"),
            ("<f1>", "QuickCd"),
            ("<q>", "Quit"),
            ("<f10>", "Quit"),
            ("<?>", "Help"),
        ]);
        let (entries, help) = entries(&km, &NORMAL, 2);
        // Catalog priorities: QuickCD 1, Copy 7, Quit 13. Move etc. are unbound and hidden.
        assert_eq!(
            texts(&entries),
            vec!["F1 QuickCD", "F5/Alt+5 Copy", "F10/q Quit"]
        );
        assert_eq!(help.map(|h| h.keys), Some("?".to_owned()));
    }

    #[test]
    fn layout_shows_two_keys_when_everything_fits() {
        let km = keymap(&[("<f5>", "Copy"), ("<alt-5>", "Copy"), ("<?>", "Help")]);
        assert_eq!(
            texts(&layout(&km, &NORMAL, 80)),
            vec!["F5/Alt+5 Copy", "? Help"]
        );
    }

    #[test]
    fn layout_falls_back_to_one_key_then_drops_lowest_priority_whole() {
        let km = keymap(&[
            ("<f5>", "Copy"),
            ("<alt-5>", "Copy"),
            ("<f6>", "Move"),
            ("<alt-6>", "Move"),
            ("<f10>", "Quit"),
            ("<?>", "Help"),
        ]);
        // Drawn as "F5Copy " 7 + "F6Move " 7 + "F10Quit " 8 + "?Help " 6 = 28 columns.
        assert_eq!(
            texts(&layout(&km, &NORMAL, 28)),
            vec!["F5 Copy", "F6 Move", "F10 Quit", "? Help"]
        );
        // One column short: Quit (lowest priority) goes, Help stays last.
        assert_eq!(
            texts(&layout(&km, &NORMAL, 27)),
            vec!["F5 Copy", "F6 Move", "? Help"]
        );
    }

    #[test]
    fn help_is_dropped_only_when_it_does_not_fit_alone() {
        let entries = vec![Entry {
            keys: "F5".into(),
            label: "Cp",
        }];
        let help = Entry {
            keys: "?".into(),
            label: "Help",
        };
        // Drawn as "F5Cp " (5) and "?Help " (6).
        let fitted = |width| texts(&fit(entries.clone(), Some(help.clone()), width));
        assert_eq!(fitted(11), vec!["F5 Cp", "? Help"]);
        assert_eq!(fitted(10), vec!["? Help"]);
        assert_eq!(fitted(5), vec!["F5 Cp"]);
    }

    #[test]
    fn shipped_normal_bar_starts_with_f_keys() {
        let km = Keymap::build(&crate::config::default_keybindings(), &RawBindings::new());
        let (entries, help) = entries(&km, &NORMAL, 2);
        assert_eq!(
            texts(&entries[..2]),
            vec!["F1/Alt+1 QuickCD", "⇧F1/Alt+⇧1 Recent"]
        );
        assert_eq!(help.map(|h| h.keys), Some("?".to_owned()));
    }
}
