//! Keybinding resolution: which `Action` a key (sequence) means in the active context.
//!
//! Terminal-free so it can be unit tested. Built once at startup from the default bindings baked
//! into the binary and the user's config (see `specs/002-keyboard-shortcuts/data-model.md`).

use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use strum::{Display, EnumIter, EnumString};

use crate::{
    action::Action, app::Mode, components::dialog::DialogState, config::parse_key_sequence,
};

/// Raw keybinding config: context name → key string → action name (`None`/`"Unbind"` = remove).
pub type RawBindings = HashMap<String, HashMap<String, Option<String>>>;

/// A normalized key sequence (see [`normalize`]).
pub type KeySeq = Vec<KeyEvent>;

/// Scope a set of bindings applies to. Config section names equal the variant names.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Display, EnumString, EnumIter,
)]
pub enum KeyContext {
    Normal,
    Git,
    GitCommit,
    GitBranch,
    GitTag,
    GlobalGit,
    Cargo,
    Dialog,
    DialogConfirm,
    QuickCd,
    Help,
}

impl KeyContext {
    /// Contexts where unmodified printable keys are text, not commands.
    pub const fn is_text_input(self) -> bool {
        matches!(self, Self::GitCommit | Self::Dialog | Self::QuickCd)
    }

    /// Contexts to search, in order, for the given app state. The first one is the primary
    /// context; the rest are fallbacks (shared dialog keys).
    pub const fn chain(mode: Mode, dialog: Option<&DialogState>) -> &'static [Self] {
        match mode {
            Mode::Normal => &[Self::Normal],
            Mode::Git => &[Self::Git],
            Mode::GitCommit => &[Self::GitCommit],
            Mode::GitBranch => &[Self::GitBranch],
            Mode::GitTag => &[Self::GitTag],
            Mode::GlobalGit => &[Self::GlobalGit],
            Mode::Cargo => &[Self::Cargo],
            Mode::Dialog => match dialog {
                Some(DialogState::QuickCd { .. }) => &[Self::QuickCd, Self::Dialog],
                Some(DialogState::Confirm { .. }) => &[Self::DialogConfirm, Self::Dialog],
                _ => &[Self::Dialog],
            },
        }
    }
}

/// Normalizes a key event for lookup: Press kind, no state, and a canonical SHIFT/case for chars.
///
/// Letters: upper case + SHIFT if either was reported, else lower case without SHIFT.
/// Digits: SHIFT kept as reported (kitty reports Alt+Shift+6 as `'6'` + SHIFT, which must differ
/// from Alt+6). Other chars: SHIFT dropped, since legacy terminals send the shifted symbol without
/// it (see research.md "Spike results").
pub fn normalize(key: KeyEvent) -> KeyEvent {
    let mut mods = key.modifiers
        & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::SUPER);
    let code = match key.code {
        KeyCode::Char(c) if c.is_alphabetic() => {
            if mods.contains(KeyModifiers::SHIFT) || c.is_uppercase() {
                mods.insert(KeyModifiers::SHIFT);
                KeyCode::Char(c.to_uppercase().next().unwrap_or(c))
            } else {
                KeyCode::Char(c)
            }
        }
        KeyCode::Char(c) if c.is_ascii_digit() => KeyCode::Char(c),
        KeyCode::Char(c) => {
            mods.remove(KeyModifiers::SHIFT);
            KeyCode::Char(c)
        }
        KeyCode::BackTab => {
            mods.insert(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        other => other,
    };
    KeyEvent::new_with_kind_and_state(code, mods, KeyEventKind::Press, KeyEventState::NONE)
}

/// Parses a config key string (`<ctrl-f><5>`) into a normalized sequence.
pub fn parse_seq(raw: &str) -> Result<KeySeq, String> {
    let seq = parse_key_sequence(raw)?;
    if seq.is_empty() {
        return Err(format!("empty key sequence `{raw}`"));
    }
    Ok(seq.into_iter().map(normalize).collect())
}

// --- Action catalog ---

/// One bindable action in one context, with its on-screen texts.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "used by the problem report and help screen (User Stories 3, 4)"
)]
pub struct ActionSpec {
    pub action: Action,
    pub context: KeyContext,
    /// Short function-bar label (≤ 8 chars).
    pub label: &'static str,
    /// Help-screen group.
    pub group: &'static str,
    /// Help-screen description.
    pub description: &'static str,
    /// Function-bar order; `None` = not shown in the bar. Lower is shown first.
    pub bar_priority: Option<u8>,
}

#[allow(
    dead_code,
    reason = "used by the problem report and help screen (User Stories 3, 4)"
)]
impl ActionSpec {
    pub fn name(&self) -> String {
        self.action.to_string()
    }
}

const fn spec(
    context: KeyContext,
    action: Action,
    label: &'static str,
    group: &'static str,
    description: &'static str,
    bar_priority: Option<u8>,
) -> ActionSpec {
    ActionSpec {
        action,
        context,
        label,
        group,
        description,
        bar_priority,
    }
}

const NAV: &str = "Navigation";
const OPS: &str = "Operations";
const MARK: &str = "Marking";
const VIEW: &str = "Views";
const APP: &str = "App";
const GIT: &str = "Git";
const TABS: &str = "Tabs";
const DLG: &str = "Dialog";

#[rustfmt::skip]
pub static CATALOG: &[ActionSpec] = {
    use KeyContext as C;
    use Action as A;
    &[
        // Normal
        spec(C::Normal, A::QuickCd, "QuickCD", NAV, "Quick change directory", Some(1)),
        spec(C::Normal, A::RecentDirs, "Recent", NAV, "Recent directories", Some(2)),
        spec(C::Normal, A::ContextMenu, "Menu", OPS, "Context menu", Some(3)),
        spec(C::Normal, A::UserCommands, "UserCmds", OPS, "User commands", Some(4)),
        spec(C::Normal, A::View, "View", OPS, "View file", Some(5)),
        spec(C::Normal, A::CalcSizes, "Sizes", OPS, "Calculate directory sizes", Some(6)),
        spec(C::Normal, A::Copy, "Copy", OPS, "Copy to other panel", Some(7)),
        spec(C::Normal, A::Move, "Move", OPS, "Move to other panel", Some(8)),
        spec(C::Normal, A::Rename, "Rename", OPS, "Rename in place", Some(9)),
        spec(C::Normal, A::Mkdir, "MkDir", OPS, "Make directory", Some(10)),
        spec(C::Normal, A::Delete, "Delete", OPS, "Delete", Some(11)),
        spec(C::Normal, A::CycleSortMode, "Sort", VIEW, "Cycle sort mode", Some(12)),
        spec(C::Normal, A::InvertSort, "Invert", VIEW, "Invert sort order", Some(16)),
        spec(C::Normal, A::Quit, "Quit", APP, "Quit", Some(13)),
        spec(C::Normal, A::SelectTheme, "Theme", APP, "Select theme", Some(14)),
        spec(C::Normal, A::GlobalGitView, "GitRepos", VIEW, "Git repositories", Some(15)),
        spec(C::Normal, A::EnterGitMode, "Git", VIEW, "Git mode for this repository", Some(17)),
        spec(C::Normal, A::EnterCargoMode, "Cargo", VIEW, "Cargo mode for this project", Some(18)),
        spec(C::Normal, A::Suspend, "Suspend", APP, "Suspend to shell", None),
        spec(C::Normal, A::NavUp, "Up", NAV, "Cursor up", None),
        spec(C::Normal, A::NavDown, "Down", NAV, "Cursor down", None),
        spec(C::Normal, A::NavPageUp, "PgUp", NAV, "Page up", None),
        spec(C::Normal, A::NavPageDown, "PgDn", NAV, "Page down", None),
        spec(C::Normal, A::NavTop, "Top", NAV, "First entry", None),
        spec(C::Normal, A::NavBottom, "Bottom", NAV, "Last entry", None),
        spec(C::Normal, A::NavEnter, "Enter", NAV, "Open directory / run", None),
        spec(C::Normal, A::NavParent, "Parent", NAV, "Parent directory", None),
        spec(C::Normal, A::SwitchPanel, "Panel", NAV, "Switch panel", None),
        spec(C::Normal, A::SyncPanelDir, "Sync", NAV, "Show this directory in the other panel", None),
        spec(C::Normal, A::ToggleMark, "Mark", MARK, "Mark / unmark entry", None),
        spec(C::Normal, A::ToggleMarkAll, "MarkAll", MARK, "Mark / unmark all", None),

        // Git
        spec(C::Git, A::GitStage, "Stage", GIT, "Stage", Some(1)),
        spec(C::Git, A::GitUnstage, "Unstage", GIT, "Unstage", Some(2)),
        spec(C::Git, A::GitCommit, "Commit", GIT, "Commit", Some(3)),
        spec(C::Git, A::GitPush, "Push", GIT, "Push", Some(5)),
        spec(C::Git, A::GitPull, "Pull", GIT, "Pull", Some(6)),
        spec(C::Git, A::GitListBranches, "Branches", GIT, "List branches", Some(7)),
        spec(C::Git, A::GitAddAllAndCommit, "All+Cmt", GIT, "Stage all and commit", Some(9)),
        spec(C::Git, A::GitSuperCommit, "SuperCmt", GIT, "Commit submodules and parent", Some(4)),
        spec(C::Git, A::GitNewBranch, "Branch", GIT, "New branch", Some(8)),
        spec(C::Git, A::GitCreateTag, "Tag", GIT, "Create tag", Some(10)),
        spec(C::Git, A::GitListTags, "Tags", GIT, "List tags", Some(11)),
        spec(C::Git, A::GitSwitchPane, "Pane", NAV, "Switch pane", Some(12)),
        spec(C::Git, A::GitToggleMark, "Mark", MARK, "Mark / unmark entry", Some(13)),
        spec(C::Git, A::ExitGitMode, "Normal", APP, "Back to file panels", Some(14)),
        spec(C::Git, A::GitReload, "Reload", GIT, "Reload status", None),
        spec(C::Git, A::GitNavUp, "Up", NAV, "Cursor up", None),
        spec(C::Git, A::GitNavDown, "Down", NAV, "Cursor down", None),
        spec(C::Git, A::GitPageUp, "PgUp", NAV, "Page up", None),
        spec(C::Git, A::GitPageDown, "PgDn", NAV, "Page down", None),
        spec(C::Git, A::GitTabStatus, "Status", TABS, "Status tab", None),
        spec(C::Git, A::GitTabLog, "Log", TABS, "Log tab", None),
        spec(C::Git, A::GitTabGraph, "Graph", TABS, "Graph tab", None),
        spec(C::Git, A::GitTabReflog, "Reflog", TABS, "Reflog tab", None),
        spec(C::Git, A::GitTabDiff, "Diff", TABS, "Diff tab", None),
        spec(C::Git, A::GitTabBlame, "Blame", TABS, "Blame tab", None),

        // Git commit editor
        spec(C::GitCommit, A::GitCommitSubmit, "Commit", GIT, "Commit with this message", Some(1)),
        spec(C::GitCommit, A::GitCommitCancel, "Cancel", GIT, "Cancel commit", Some(2)),

        // Git branches
        spec(C::GitBranch, A::GitBranchNavUp, "Up", NAV, "Cursor up", None),
        spec(C::GitBranch, A::GitBranchNavDown, "Down", NAV, "Cursor down", None),
        spec(C::GitBranch, A::GitBranchConfirm, "Checkout", GIT, "Check out branch", Some(1)),
        spec(C::GitBranch, A::GitNewBranch, "New", GIT, "New branch", Some(2)),
        spec(C::GitBranch, A::ExitGitMode, "Back", APP, "Close branch list", Some(3)),

        // Git tags
        spec(C::GitTag, A::GitTagNavUp, "Up", NAV, "Cursor up", None),
        spec(C::GitTag, A::GitTagNavDown, "Down", NAV, "Cursor down", None),
        spec(C::GitTag, A::GitTagConfirm, "Checkout", GIT, "Check out tagged commit", Some(1)),
        spec(C::GitTag, A::GitTagDelete, "Delete", GIT, "Delete tag", Some(2)),
        spec(C::GitTag, A::ExitGitMode, "Back", APP, "Close tag list", Some(3)),

        // Global git (repositories)
        spec(C::GlobalGit, A::GlobalGitNavUp, "Up", NAV, "Cursor up", None),
        spec(C::GlobalGit, A::GlobalGitNavDown, "Down", NAV, "Cursor down", None),
        spec(C::GlobalGit, A::GlobalGitSelect, "Open", GIT, "Open repository in git mode", Some(1)),
        spec(C::GlobalGit, A::GlobalGitRemove, "Remove", GIT, "Remove from list", Some(2)),
        spec(C::GlobalGit, A::ExitGlobalGitView, "Back", APP, "Back to file panels", Some(3)),

        // Cargo
        spec(C::Cargo, A::CargoNavUp, "Up", NAV, "Cursor up", None),
        spec(C::Cargo, A::CargoNavDown, "Down", NAV, "Cursor down", None),
        spec(C::Cargo, A::CargoRunSelected, "Run", OPS, "Run selected command", Some(1)),
        spec(C::Cargo, A::ExitCargoMode, "Exit", APP, "Back to file panels", Some(2)),

        // Dialogs
        spec(C::Dialog, A::DialogConfirm, "OK", DLG, "Confirm", Some(1)),
        spec(C::Dialog, A::DialogCancel, "Cancel", DLG, "Cancel", Some(2)),
        spec(C::Dialog, A::DialogNavUp, "Up", NAV, "Previous item", None),
        spec(C::Dialog, A::DialogNavDown, "Down", NAV, "Next item", None),
        spec(C::Dialog, A::DialogDeleteSelected, "Remove", DLG, "Remove selected item", None),
        spec(C::Dialog, A::DialogInputBackspace, "BkSp", DLG, "Delete last character", None),
        spec(C::DialogConfirm, A::DialogConfirm, "Yes", DLG, "Yes", Some(1)),
        spec(C::DialogConfirm, A::DialogCancel, "No", DLG, "No", Some(2)),
        spec(C::QuickCd, A::QuickCdComplete, "Complete", DLG, "Complete selected directory", Some(1)),
        spec(C::QuickCd, A::QuickCdBackspace, "BkSp", DLG, "Delete last character", None),
    ]
};

/// Catalog entries of one context, in catalog order.
pub fn catalog_for(context: KeyContext) -> impl Iterator<Item = &'static ActionSpec> {
    CATALOG.iter().filter(move |s| s.context == context)
}

fn lookup_action(context: KeyContext, name: &str) -> Option<Action> {
    catalog_for(context)
        .find(|s| s.action.to_string() == name)
        .map(|s| s.action.clone())
}

fn is_known_action(name: &str) -> bool {
    CATALOG.iter().any(|s| s.action.to_string() == name)
}

// --- Problems ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "used by the problem report and help screen (User Stories 3, 4)"
)]
pub enum ProblemKind {
    UnknownContext,
    UnparseableKey,
    UnknownAction,
    NotBindableHere,
    PrefixAmbiguity,
    Unreachable,
    DuplicateKey,
    TextInputSteal,
    QuitUnreachable,
    ConfigUnreadable,
}

/// A detected keybinding problem, rendered as one line in the startup report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingProblem {
    pub kind: ProblemKind,
    pub context: Option<KeyContext>,
    /// What the problem is about, e.g. `<alt-x5> → Copy`.
    pub subject: String,
    /// What is wrong.
    pub what: String,
    /// What Crabmander did about it.
    pub resolution: &'static str,
}

impl fmt::Display for BindingProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ctx) = self.context {
            write!(f, "[{ctx}] ")?;
        }
        write!(f, "{}: {} — {}", self.subject, self.what, self.resolution)
    }
}

impl BindingProblem {
    fn new(
        kind: ProblemKind,
        context: Option<KeyContext>,
        subject: impl Into<String>,
        what: impl Into<String>,
        resolution: &'static str,
    ) -> Self {
        Self {
            kind,
            context,
            subject: subject.into(),
            what: what.into(),
            resolution,
        }
    }

    #[allow(
        dead_code,
        reason = "used by the problem report and help screen (User Stories 3, 4)"
    )]
    pub fn config_unreadable(err: impl fmt::Display) -> Self {
        Self::new(
            ProblemKind::ConfigUnreadable,
            None,
            "config file",
            format!("could not be read ({err})"),
            "defaults used",
        )
    }
}

// --- Keymap ---

/// One binding before merging. `action == None` removes the binding (user config only).
#[derive(Debug, Clone)]
pub struct Binding {
    pub context: KeyContext,
    pub keys: KeySeq,
    pub action: Option<Action>,
    /// The key string as written, for messages.
    #[allow(dead_code, reason = "used by the problem report (User Story 3)")]
    pub raw: String,
}

/// Result of looking up the keys pressed so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// The keys form a complete binding.
    Action(Action),
    /// The keys are the start of a longer binding; wait for more.
    Prefix,
    /// Nothing is bound to these keys.
    None,
}

/// The resolved keybindings for every context.
#[derive(Debug, Default)]
pub struct Keymap {
    bindings: HashMap<KeyContext, HashMap<KeySeq, Action>>,
    /// Strict prefixes of multi-key bindings, per context.
    prefixes: HashMap<KeyContext, HashSet<KeySeq>>,
    /// Keys in effect per action, display order (see [`key_rank`]).
    keys_for: HashMap<(KeyContext, String), Vec<KeySeq>>,
    #[allow(dead_code, reason = "shown at startup by User Story 3")]
    pub problems: Vec<BindingProblem>,
}

/// Parses one source (defaults or user config) into bindings, collecting problems.
fn parse_source(raw: &RawBindings, problems: &mut Vec<BindingProblem>) -> Vec<Binding> {
    let mut out = Vec::new();
    // Sorted for deterministic results and messages (the config maps are unordered).
    let mut contexts: Vec<_> = raw.iter().collect();
    contexts.sort_by(|a, b| a.0.cmp(b.0));
    for (ctx_name, entries) in contexts {
        let Ok(context) = ctx_name.parse::<KeyContext>() else {
            problems.push(BindingProblem::new(
                ProblemKind::UnknownContext,
                None,
                format!("\"{ctx_name}\""),
                "unknown keybinding section",
                "section skipped",
            ));
            continue;
        };
        let mut entries: Vec<_> = entries.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (raw_keys, action_name) in entries {
            let action_name = action_name.as_deref().filter(|n| *n != "Unbind");
            let subject = format!("{raw_keys} → {}", action_name.unwrap_or("null"));
            let Ok(keys) = parse_seq(raw_keys) else {
                problems.push(BindingProblem::new(
                    ProblemKind::UnparseableKey,
                    Some(context),
                    subject,
                    "unparseable key",
                    "entry skipped",
                ));
                continue;
            };
            let action = match action_name {
                None => None,
                Some(name) => {
                    if let Some(action) = lookup_action(context, name) {
                        Some(action)
                    } else {
                        let (kind, what) = if is_known_action(name) {
                            (
                                ProblemKind::NotBindableHere,
                                format!("action not available in {context}"),
                            )
                        } else {
                            (ProblemKind::UnknownAction, "unknown action".to_owned())
                        };
                        problems.push(BindingProblem::new(
                            kind,
                            Some(context),
                            subject,
                            what,
                            "entry skipped",
                        ));
                        continue;
                    }
                }
            };
            out.push(Binding {
                context,
                keys,
                action,
                raw: raw_keys.clone(),
            });
        }
    }
    out
}

/// Display order for an action's keys: F-keys, then plain keys, Alt, Ctrl, then sequences.
fn key_rank(seq: &KeySeq) -> (u8, String) {
    let first = &seq[0];
    let rank = if seq.len() > 1 {
        4
    } else if matches!(first.code, KeyCode::F(_)) {
        0
    } else if first.modifiers.contains(KeyModifiers::CONTROL) {
        3
    } else if first.modifiers.contains(KeyModifiers::ALT) {
        2
    } else {
        1
    };
    (rank, format!("{seq:?}"))
}

impl Keymap {
    /// Merges the defaults with the user's bindings. Never fails; problems are collected.
    pub fn build(defaults: &RawBindings, user: &RawBindings) -> Self {
        let mut problems = Vec::new();
        let mut merged: HashMap<KeyContext, HashMap<KeySeq, Action>> = HashMap::new();

        for binding in parse_source(defaults, &mut problems)
            .into_iter()
            .chain(parse_source(user, &mut problems))
        {
            let map = merged.entry(binding.context).or_default();
            match binding.action {
                Some(action) => {
                    map.insert(binding.keys, action);
                }
                None => {
                    map.remove(&binding.keys);
                }
            }
        }

        let mut keymap = Self {
            problems,
            ..Self::default()
        };
        for (context, map) in merged {
            let prefixes = keymap.prefixes.entry(context).or_default();
            for (keys, action) in &map {
                for len in 1..keys.len() {
                    prefixes.insert(keys[..len].to_vec());
                }
                keymap
                    .keys_for
                    .entry((context, action.to_string()))
                    .or_default()
                    .push(keys.clone());
            }
            keymap.bindings.insert(context, map);
        }
        for keys in keymap.keys_for.values_mut() {
            keys.sort_by_cached_key(key_rank);
        }
        keymap
    }

    /// Looks up the (normalized) keys pressed so far, searching the context chain in order.
    pub fn resolve(&self, chain: &[KeyContext], keys: &[KeyEvent]) -> Lookup {
        for context in chain {
            if let Some(action) = self.bindings.get(context).and_then(|m| m.get(keys)) {
                return Lookup::Action(action.clone());
            }
            if self.prefixes.get(context).is_some_and(|p| p.contains(keys)) {
                return Lookup::Prefix;
            }
        }
        Lookup::None
    }

    /// Keys bound to `action` in `context`, in display order.
    #[allow(
        dead_code,
        reason = "used by the problem report and help screen (User Stories 3, 4)"
    )]
    pub fn keys_for(&self, context: KeyContext, action: &Action) -> &[KeySeq] {
        self.keys_for
            .get(&(context, action.to_string()))
            .map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    type Entries<'a> = &'a [(&'a str, Option<&'a str>)];

    fn raw(entries: &[(&str, Entries)]) -> RawBindings {
        entries
            .iter()
            .map(|(ctx, binds)| {
                (
                    (*ctx).to_owned(),
                    binds
                        .iter()
                        .map(|(k, a)| ((*k).to_owned(), a.map(str::to_owned)))
                        .collect(),
                )
            })
            .collect()
    }

    fn seq(s: &str) -> KeySeq {
        parse_seq(s).unwrap()
    }

    #[test]
    fn normalize_ignores_kind_and_state() {
        let mut ev = key(KeyCode::Char('j'), KeyModifiers::NONE);
        ev.kind = KeyEventKind::Repeat;
        ev.state = KeyEventState::CAPS_LOCK;
        assert_eq!(normalize(ev), key(KeyCode::Char('j'), KeyModifiers::NONE));
    }

    #[test]
    fn normalize_letters_shift_and_case_agree() {
        let expected = key(KeyCode::Char('Y'), KeyModifiers::SHIFT);
        assert_eq!(
            normalize(key(KeyCode::Char('Y'), KeyModifiers::SHIFT)),
            expected
        );
        assert_eq!(
            normalize(key(KeyCode::Char('Y'), KeyModifiers::NONE)),
            expected
        );
        assert_eq!(
            normalize(key(KeyCode::Char('y'), KeyModifiers::SHIFT)),
            expected
        );
        assert_eq!(seq("<Y>"), vec![expected]);
        assert_eq!(seq("<shift-y>"), vec![expected]);
    }

    #[test]
    fn normalize_keeps_shift_on_digits_and_drops_it_on_symbols() {
        let alt_shift = KeyModifiers::ALT | KeyModifiers::SHIFT;
        assert_eq!(
            normalize(key(KeyCode::Char('6'), alt_shift)),
            key(KeyCode::Char('6'), alt_shift)
        );
        assert_eq!(
            normalize(key(KeyCode::Char('&'), alt_shift)),
            key(KeyCode::Char('&'), KeyModifiers::ALT)
        );
        assert_ne!(seq("<alt-shift-6>"), seq("<alt-6>"));
    }

    #[test]
    fn normalize_backtab_always_has_shift() {
        assert_eq!(
            normalize(key(KeyCode::BackTab, KeyModifiers::NONE)),
            key(KeyCode::BackTab, KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn chain_for_dialogs() {
        assert_eq!(KeyContext::chain(Mode::Normal, None), &[KeyContext::Normal]);
        assert_eq!(KeyContext::chain(Mode::Dialog, None), &[KeyContext::Dialog]);
        let qcd = DialogState::QuickCd {
            input: String::new(),
            matches: vec![],
            selected: 0,
            base_path: std::path::PathBuf::new(),
        };
        assert_eq!(
            KeyContext::chain(Mode::Dialog, Some(&qcd)),
            &[KeyContext::QuickCd, KeyContext::Dialog]
        );
        let errs = DialogState::error_list("t", vec![]);
        assert_eq!(
            KeyContext::chain(Mode::Dialog, Some(&errs)),
            &[KeyContext::Dialog]
        );
    }

    #[test]
    fn user_entry_overrides_default_for_same_key() {
        let defaults = raw(&[("Normal", &[("<q>", Some("Quit")), ("<f5>", Some("Copy"))])]);
        let user = raw(&[("Normal", &[("<q>", Some("Copy"))])]);
        let km = Keymap::build(&defaults, &user);
        let chain = [KeyContext::Normal];
        assert_eq!(
            km.resolve(&chain, &seq("<q>")),
            Lookup::Action(Action::Copy)
        );
        assert_eq!(
            km.resolve(&chain, &seq("<f5>")),
            Lookup::Action(Action::Copy)
        );
        assert_eq!(km.problems, vec![]);
    }

    #[test]
    fn bad_entries_are_skipped_and_reported() {
        let defaults = raw(&[("Normal", &[("<f5>", Some("Copy"))])]);
        let user = raw(&[
            (
                "Normal",
                &[
                    ("<alt-x5>", Some("Copy")),
                    ("<x>", Some("Copyy")),
                    ("<y>", Some("GitStage")),
                    ("<z>", Some("Move")),
                ],
            ),
            ("Nromal", &[("<q>", Some("Quit"))]),
        ]);
        let km = Keymap::build(&defaults, &user);
        let kinds: Vec<_> = km.problems.iter().map(|p| p.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ProblemKind::UnparseableKey,
                ProblemKind::UnknownAction,
                ProblemKind::NotBindableHere,
                ProblemKind::UnknownContext,
            ]
        );
        assert_eq!(
            km.problems[0].to_string(),
            "[Normal] <alt-x5> → Copy: unparseable key — entry skipped"
        );
        let chain = [KeyContext::Normal];
        assert_eq!(
            km.resolve(&chain, &seq("<z>")),
            Lookup::Action(Action::Move)
        );
        assert_eq!(
            km.resolve(&chain, &seq("<f5>")),
            Lookup::Action(Action::Copy)
        );
    }

    #[test]
    fn resolve_sequences_and_prefixes() {
        let defaults = raw(&[(
            "Normal",
            &[
                ("<ctrl-f><5>", Some("Copy")),
                ("<ctrl-f><s><6>", Some("Rename")),
            ],
        )]);
        let km = Keymap::build(&defaults, &RawBindings::new());
        let chain = [KeyContext::Normal];
        assert_eq!(km.resolve(&chain, &seq("<ctrl-f>")), Lookup::Prefix);
        assert_eq!(km.resolve(&chain, &seq("<ctrl-f><s>")), Lookup::Prefix);
        assert_eq!(
            km.resolve(&chain, &seq("<ctrl-f><5>")),
            Lookup::Action(Action::Copy)
        );
        assert_eq!(
            km.resolve(&chain, &seq("<ctrl-f><s><6>")),
            Lookup::Action(Action::Rename)
        );
        assert_eq!(km.resolve(&chain, &seq("<ctrl-f><9>")), Lookup::None);
    }

    #[test]
    fn chain_falls_back_to_shared_dialog_keys() {
        let defaults = raw(&[
            (
                "Dialog",
                &[
                    ("<esc>", Some("DialogCancel")),
                    ("<backspace>", Some("DialogInputBackspace")),
                ],
            ),
            ("QuickCd", &[("<backspace>", Some("QuickCdBackspace"))]),
        ]);
        let km = Keymap::build(&defaults, &RawBindings::new());
        let chain = [KeyContext::QuickCd, KeyContext::Dialog];
        assert_eq!(
            km.resolve(&chain, &seq("<backspace>")),
            Lookup::Action(Action::QuickCdBackspace)
        );
        assert_eq!(
            km.resolve(&chain, &seq("<esc>")),
            Lookup::Action(Action::DialogCancel)
        );
    }

    // --- Rebinding and removal (User Story 2) ---

    #[test]
    fn null_and_unbind_remove_default_bindings() {
        let defaults = raw(&[(
            "Normal",
            &[
                ("<q>", Some("Quit")),
                ("<f10>", Some("Quit")),
                ("<j>", Some("NavDown")),
            ],
        )]);
        let user = raw(&[("Normal", &[("<q>", None), ("<j>", Some("Unbind"))])]);
        let km = Keymap::build(&defaults, &user);
        let chain = [KeyContext::Normal];
        assert_eq!(km.resolve(&chain, &seq("<q>")), Lookup::None);
        assert_eq!(km.resolve(&chain, &seq("<j>")), Lookup::None);
        assert_eq!(
            km.resolve(&chain, &seq("<f10>")),
            Lookup::Action(Action::Quit)
        );
        assert_eq!(
            km.keys_for(KeyContext::Normal, &Action::Quit),
            &[seq("<f10>")]
        );
    }

    #[test]
    fn removing_a_key_that_is_not_bound_is_accepted() {
        let user = raw(&[("Git", &[("<ctrl-x>", None)])]);
        let km = Keymap::build(&RawBindings::new(), &user);
        assert_eq!(km.problems, vec![]);
    }

    #[test]
    fn overrides_apply_in_every_context() {
        let defaults = raw(&[
            (
                "Git",
                &[("<a>", Some("GitStage")), ("<q>", Some("ExitGitMode"))],
            ),
            ("Cargo", &[("<enter>", Some("CargoRunSelected"))]),
            ("DialogConfirm", &[("<y>", Some("DialogConfirm"))]),
        ]);
        let user = raw(&[
            ("Git", &[("<x>", Some("GitStage"))]),
            ("Cargo", &[("<r>", Some("CargoRunSelected"))]),
            ("DialogConfirm", &[("<j>", Some("DialogConfirm"))]),
        ]);
        let km = Keymap::build(&defaults, &user);
        let git = [KeyContext::Git];
        assert_eq!(
            km.resolve(&git, &seq("<x>")),
            Lookup::Action(Action::GitStage)
        );
        assert_eq!(
            km.resolve(&git, &seq("<a>")),
            Lookup::Action(Action::GitStage)
        );
        assert_eq!(
            km.resolve(&git, &seq("<q>")),
            Lookup::Action(Action::ExitGitMode)
        );
        let cargo = [KeyContext::Cargo];
        assert_eq!(
            km.resolve(&cargo, &seq("<r>")),
            Lookup::Action(Action::CargoRunSelected)
        );
        let confirm = [KeyContext::DialogConfirm, KeyContext::Dialog];
        assert_eq!(
            km.resolve(&confirm, &seq("<j>")),
            Lookup::Action(Action::DialogConfirm)
        );
    }

    #[test]
    fn removed_key_falls_back_along_the_chain() {
        let defaults = raw(&[
            ("Dialog", &[("<backspace>", Some("DialogInputBackspace"))]),
            ("QuickCd", &[("<backspace>", Some("QuickCdBackspace"))]),
        ]);
        let user = raw(&[("QuickCd", &[("<backspace>", None)])]);
        let km = Keymap::build(&defaults, &user);
        let chain = [KeyContext::QuickCd, KeyContext::Dialog];
        assert_eq!(
            km.resolve(&chain, &seq("<backspace>")),
            Lookup::Action(Action::DialogInputBackspace)
        );
    }

    // --- Shipped defaults (User Story 1) ---

    fn shipped() -> Keymap {
        Keymap::build(&crate::config::default_keybindings(), &RawBindings::new())
    }

    /// The Alt and Ctrl+F leader keys that mirror F`n` (with Shift if `shift`).
    fn mirrors(n: u8, shift: bool) -> [KeySeq; 2] {
        let d = match n {
            1..=9 => char::from(b'0' + n).to_string(),
            10 => "0".into(),
            11 => "-".into(),
            _ => "=".into(),
        };
        if shift {
            [
                seq(&format!("<alt-shift-{d}>")),
                seq(&format!("<ctrl-f><s><{d}>")),
            ]
        } else {
            [seq(&format!("<alt-{d}>")), seq(&format!("<ctrl-f><{d}>"))]
        }
    }

    #[test]
    fn every_f_key_action_has_alt_and_leader_fallbacks() {
        let km = shipped();
        let mut checked = 0;
        for (context, map) in &km.bindings {
            for (keys, action) in map {
                let [only] = keys.as_slice() else { continue };
                let KeyCode::F(n) = only.code else { continue };
                let shift = only.modifiers.contains(KeyModifiers::SHIFT);
                for fallback in mirrors(n, shift) {
                    assert_eq!(
                        map.get(&fallback),
                        Some(action),
                        "[{context}] {action} on F{n} (shift: {shift}) lacks {fallback:?}"
                    );
                }
                checked += 1;
            }
        }
        assert!(checked >= 22, "only {checked} F-key bindings found");
    }

    #[test]
    fn commander_keys_kept_and_shift_letter_fallbacks_removed() {
        let km = shipped();
        let normal = [KeyContext::Normal];
        for (k, action) in [
            ("<f5>", Action::Copy),
            ("<f6>", Action::Move),
            ("<f7>", Action::Mkdir),
            ("<f8>", Action::Delete),
            ("<f10>", Action::Quit),
            ("<tab>", Action::SwitchPanel),
            ("<f1>", Action::QuickCd),
        ] {
            assert_eq!(km.resolve(&normal, &seq(k)), Lookup::Action(action), "{k}");
        }
        for letter in ["x", "e", "z", "y", "r", "k", "d", "s", "t", "g"] {
            let k = format!("<shift-{letter}>");
            assert_eq!(
                km.resolve(&normal, &seq(&k)),
                Lookup::None,
                "{k} still bound"
            );
        }
    }

    #[test]
    fn alt_shift_digit_as_reported_by_kitty_resolves() {
        let km = shipped();
        let ev = normalize(key(
            KeyCode::Char('6'),
            KeyModifiers::ALT | KeyModifiers::SHIFT,
        ));
        assert_eq!(
            km.resolve(&[KeyContext::Normal], &[ev]),
            Lookup::Action(Action::Rename)
        );
        let ev = normalize(key(KeyCode::Char('6'), KeyModifiers::ALT));
        assert_eq!(
            km.resolve(&[KeyContext::Normal], &[ev]),
            Lookup::Action(Action::Move)
        );
    }

    #[test]
    fn keys_for_lists_f_key_first() {
        let defaults = raw(&[(
            "Normal",
            &[
                ("<ctrl-f><5>", Some("Copy")),
                ("<alt-5>", Some("Copy")),
                ("<f5>", Some("Copy")),
            ],
        )]);
        let km = Keymap::build(&defaults, &RawBindings::new());
        assert_eq!(
            km.keys_for(KeyContext::Normal, &Action::Copy),
            &[seq("<f5>"), seq("<alt-5>"), seq("<ctrl-f><5>")]
        );
    }
}
