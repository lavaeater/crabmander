//! Keybinding resolution: which `Action` a key (sequence) means in the active context.
//!
//! Terminal-free so it can be unit tested. Built once at startup from the default bindings baked
//! into the binary and the user's config (see `specs/002-keyboard-shortcuts/data-model.md`).

use std::{
    collections::{BTreeSet, HashMap, HashSet},
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
            Mode::Help => &[Self::Help],
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

// --- Display names ---

/// How a key is written on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStyle {
    /// Function bar: `F5`, `⇧F6`, `Alt+⇧6`, `^B`, `Spc`, `PgUp`, `^F 5`.
    Compact,
    /// Help screen: `Shift+F6`, `Alt+Shift+6`, `Ctrl+B`, `Space`, `Page Up`, `Ctrl+F 5`.
    Full,
}

fn code_name(code: KeyCode, style: KeyStyle) -> String {
    let compact = style == KeyStyle::Compact;
    let name = match code {
        KeyCode::F(n) => return format!("F{n}"),
        KeyCode::Char(' ') => {
            if compact {
                "Spc"
            } else {
                "Space"
            }
        }
        KeyCode::Char(c) => return c.to_string(),
        KeyCode::Esc => "Esc",
        KeyCode::Enter => "Enter",
        KeyCode::Tab | KeyCode::BackTab => "Tab",
        KeyCode::Backspace => {
            if compact {
                "BkSp"
            } else {
                "Backspace"
            }
        }
        KeyCode::Delete => {
            if compact {
                "Del"
            } else {
                "Delete"
            }
        }
        KeyCode::Insert => {
            if compact {
                "Ins"
            } else {
                "Insert"
            }
        }
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::PageUp => {
            if compact {
                "PgUp"
            } else {
                "Page Up"
            }
        }
        KeyCode::PageDown => {
            if compact {
                "PgDn"
            } else {
                "Page Down"
            }
        }
        KeyCode::Up => {
            if compact {
                "↑"
            } else {
                "Up"
            }
        }
        KeyCode::Down => {
            if compact {
                "↓"
            } else {
                "Down"
            }
        }
        KeyCode::Left => {
            if compact {
                "←"
            } else {
                "Left"
            }
        }
        KeyCode::Right => {
            if compact {
                "→"
            } else {
                "Right"
            }
        }
        other => return format!("{other:?}"),
    };
    name.to_owned()
}

/// One key as shown on screen. Letters show Shift through their case (`Y`), except with Ctrl,
/// where the letter is always upper case (`^B`, `Ctrl+Shift+B`).
pub fn key_name(key: &KeyEvent, style: KeyStyle) -> String {
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let letter = matches!(key.code, KeyCode::Char(c) if c.is_alphabetic());
    let shift = mods.contains(KeyModifiers::SHIFT) && (ctrl || !letter);
    let code = match key.code {
        KeyCode::Char(c) if letter && ctrl => {
            code_name(KeyCode::Char(c.to_ascii_uppercase()), style)
        }
        code => code_name(code, style),
    };
    let mut out = String::new();
    match style {
        KeyStyle::Compact => {
            if ctrl {
                out.push('^');
            }
            if mods.contains(KeyModifiers::SUPER) {
                out.push_str("Super+");
            }
            if mods.contains(KeyModifiers::ALT) {
                out.push_str("Alt+");
            }
            if shift {
                out.push('⇧');
            }
        }
        KeyStyle::Full => {
            for (flag, name) in [
                (ctrl, "Ctrl+"),
                (mods.contains(KeyModifiers::SUPER), "Super+"),
                (mods.contains(KeyModifiers::ALT), "Alt+"),
                (shift, "Shift+"),
            ] {
                if flag {
                    out.push_str(name);
                }
            }
        }
    }
    out.push_str(&code);
    out
}

/// A key sequence as shown on screen; the keys of a sequence are separated by spaces.
pub fn seq_name(seq: &[KeyEvent], style: KeyStyle) -> String {
    seq.iter()
        .map(|k| key_name(k, style))
        .collect::<Vec<_>>()
        .join(" ")
}

// --- Action catalog ---

/// One bindable action in one context, with its on-screen texts.
#[derive(Debug)]
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
        spec(C::Normal, A::Help, "Help", APP, "Show the keys of this mode", None),

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
        spec(C::Git, A::Help, "Help", APP, "Show the keys of this mode", None),

        // Git commit editor
        spec(C::GitCommit, A::GitCommitSubmit, "Commit", GIT, "Commit with this message", Some(1)),
        spec(C::GitCommit, A::GitCommitCancel, "Cancel", GIT, "Cancel commit", Some(2)),

        // Git branches
        spec(C::GitBranch, A::GitBranchNavUp, "Up", NAV, "Cursor up", None),
        spec(C::GitBranch, A::GitBranchNavDown, "Down", NAV, "Cursor down", None),
        spec(C::GitBranch, A::GitBranchConfirm, "Checkout", GIT, "Check out branch", Some(1)),
        spec(C::GitBranch, A::GitNewBranch, "New", GIT, "New branch", Some(2)),
        spec(C::GitBranch, A::ExitGitMode, "Back", APP, "Close branch list", Some(3)),
        spec(C::GitBranch, A::Help, "Help", APP, "Show the keys of this mode", None),

        // Git tags
        spec(C::GitTag, A::GitTagNavUp, "Up", NAV, "Cursor up", None),
        spec(C::GitTag, A::GitTagNavDown, "Down", NAV, "Cursor down", None),
        spec(C::GitTag, A::GitTagConfirm, "Checkout", GIT, "Check out tagged commit", Some(1)),
        spec(C::GitTag, A::GitTagDelete, "Delete", GIT, "Delete tag", Some(2)),
        spec(C::GitTag, A::ExitGitMode, "Back", APP, "Close tag list", Some(3)),
        spec(C::GitTag, A::Help, "Help", APP, "Show the keys of this mode", None),

        // Global git (repositories)
        spec(C::GlobalGit, A::GlobalGitNavUp, "Up", NAV, "Cursor up", None),
        spec(C::GlobalGit, A::GlobalGitNavDown, "Down", NAV, "Cursor down", None),
        spec(C::GlobalGit, A::GlobalGitSelect, "Open", GIT, "Open repository in git mode", Some(1)),
        spec(C::GlobalGit, A::GlobalGitRemove, "Remove", GIT, "Remove from list", Some(2)),
        spec(C::GlobalGit, A::ExitGlobalGitView, "Back", APP, "Back to file panels", Some(3)),
        spec(C::GlobalGit, A::Help, "Help", APP, "Show the keys of this mode", None),

        // Cargo
        spec(C::Cargo, A::CargoNavUp, "Up", NAV, "Cursor up", None),
        spec(C::Cargo, A::CargoNavDown, "Down", NAV, "Cursor down", None),
        spec(C::Cargo, A::CargoRunSelected, "Run", OPS, "Run selected command", Some(1)),
        spec(C::Cargo, A::ExitCargoMode, "Exit", APP, "Back to file panels", Some(2)),
        spec(C::Cargo, A::Help, "Help", APP, "Show the keys of this mode", None),

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

        // Help screen
        spec(C::Help, A::HelpClose, "Close", APP, "Close help", Some(1)),
        spec(C::Help, A::HelpScrollUp, "Up", NAV, "Scroll up", Some(2)),
        spec(C::Help, A::HelpScrollDown, "Down", NAV, "Scroll down", Some(3)),
        spec(C::Help, A::HelpPageUp, "PgUp", NAV, "Page up", None),
        spec(C::Help, A::HelpPageDown, "PgDn", NAV, "Page down", None),
    ]
};

/// Catalog entries of one context, in catalog order. The `Help` action is shown at the end of
/// the function bar by `func_bar`, so it has no `bar_priority`.
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
        // Normalized keys already written in this section, for duplicate detection.
        let mut seen: HashMap<KeySeq, &str> = HashMap::new();
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
            if action.is_some() && context.is_text_input() && is_printable(&keys[0]) {
                problems.push(BindingProblem::new(
                    ProblemKind::TextInputSteal,
                    Some(context),
                    subject,
                    "would steal typed text",
                    "entry skipped",
                ));
                continue;
            }
            if let Some(earlier) = seen.insert(keys.clone(), raw_keys.as_str()) {
                problems.push(BindingProblem::new(
                    ProblemKind::DuplicateKey,
                    Some(context),
                    format!("{earlier} and {raw_keys}"),
                    "same key written twice",
                    "later entry wins",
                ));
            }
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

/// A key that types text in a text field: a character without Ctrl/Alt/Super (Shift allowed).
fn is_printable(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(_))
        && !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
}

/// Display order for an action's keys: F-keys, named keys (Esc, arrows, Space), plain characters,
/// Alt, Ctrl, then sequences.
fn key_rank(seq: &KeySeq) -> (u8, String) {
    let first = &seq[0];
    let rank = if seq.len() > 1 {
        5
    } else if matches!(first.code, KeyCode::F(_)) {
        0
    } else if first.modifiers.contains(KeyModifiers::CONTROL) {
        4
    } else if first.modifiers.contains(KeyModifiers::ALT) {
        3
    } else if matches!(first.code, KeyCode::Char(c) if c != ' ') {
        2
    } else {
        1
    };
    (rank, format!("{seq:?}"))
}

/// Bindings while merging: context → keys → (action, key string as written).
type Merged = HashMap<KeyContext, HashMap<KeySeq, (Action, String)>>;

/// Action names that have at least one key, per context.
fn bound_actions(merged: &Merged) -> HashMap<KeyContext, BTreeSet<String>> {
    merged
        .iter()
        .map(|(ctx, map)| (*ctx, map.values().map(|(a, _)| a.to_string()).collect()))
        .collect()
}

/// Checks the merged bindings (research.md R4): prefix ambiguity and actions left without a key.
fn validate(
    merged: &Merged,
    default_actions: &HashMap<KeyContext, BTreeSet<String>>,
    problems: &mut Vec<BindingProblem>,
) {
    let mut contexts: Vec<_> = merged
        .keys()
        .chain(default_actions.keys())
        .copied()
        .collect();
    contexts.sort_unstable();
    contexts.dedup();
    let empty = HashMap::new();
    for context in contexts {
        let map = merged.get(&context).unwrap_or(&empty);

        // A complete binding that is also the start of a longer one: the single key wins.
        let mut entries: Vec<_> = map.iter().collect();
        entries.sort_by(|a, b| a.1.1.cmp(&b.1.1));
        for (keys, (action, raw)) in &entries {
            let longer: Vec<String> = entries
                .iter()
                .filter(|(k, _)| k.len() > keys.len() && k.starts_with(keys))
                .map(|(_, (a, r))| format!("{r} → {a}"))
                .collect();
            if !longer.is_empty() {
                problems.push(BindingProblem::new(
                    ProblemKind::PrefixAmbiguity,
                    Some(context),
                    format!("{raw} → {action} vs {}", longer.join(", ")),
                    format!("{raw} is also a prefix"),
                    "single key wins",
                ));
            }
        }

        // Actions bound by default that the user's config left without any key.
        let now: BTreeSet<String> = map.values().map(|(a, _)| a.to_string()).collect();
        for action in default_actions.get(&context).into_iter().flatten() {
            if now.contains(action) {
                continue;
            }
            let quit = context == KeyContext::Normal && action == "Quit";
            problems.push(BindingProblem::new(
                if quit {
                    ProblemKind::QuitUnreachable
                } else {
                    ProblemKind::Unreachable
                },
                Some(context),
                action.clone(),
                "no key left after overrides",
                if quit {
                    "Ctrl+C still quits"
                } else {
                    "action unreachable"
                },
            ));
        }
    }
}

impl Keymap {
    /// Merges the defaults with the user's bindings. Never fails; problems are collected.
    pub fn build(defaults: &RawBindings, user: &RawBindings) -> Self {
        let mut problems = Vec::new();
        let mut merged: Merged = HashMap::new();

        let apply = |merged: &mut Merged, bindings: Vec<Binding>| {
            for binding in bindings {
                let map = merged.entry(binding.context).or_default();
                match binding.action {
                    Some(action) => {
                        map.insert(binding.keys, (action, binding.raw));
                    }
                    None => {
                        map.remove(&binding.keys);
                    }
                }
            }
        };
        apply(&mut merged, parse_source(defaults, &mut problems));
        let default_actions = bound_actions(&merged);
        apply(&mut merged, parse_source(user, &mut problems));

        validate(&merged, &default_actions, &mut problems);

        let mut keymap = Self {
            problems,
            ..Self::default()
        };
        for (context, map) in merged {
            let prefixes = keymap.prefixes.entry(context).or_default();
            let mut bindings = HashMap::with_capacity(map.len());
            for (keys, (action, _)) in map {
                for len in 1..keys.len() {
                    prefixes.insert(keys[..len].to_vec());
                }
                keymap
                    .keys_for
                    .entry((context, action.to_string()))
                    .or_default()
                    .push(keys.clone());
                bindings.insert(keys, action);
            }
            keymap.bindings.insert(context, bindings);
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
    pub fn keys_for(&self, context: KeyContext, action: &Action) -> &[KeySeq] {
        self.keys_for
            .get(&(context, action.to_string()))
            .map_or(&[], Vec::as_slice)
    }

    /// Keys that trigger `action` with this context chain, in display order: the keys of each
    /// context in turn, skipping keys an earlier context binds to something else.
    pub fn keys_in_chain(&self, chain: &[KeyContext], action: &Action) -> Vec<KeySeq> {
        let mut out: Vec<KeySeq> = Vec::new();
        for (i, context) in chain.iter().enumerate() {
            for keys in self.keys_for(*context, action) {
                let shadowed = chain[..i]
                    .iter()
                    .any(|c| self.bindings.get(c).is_some_and(|m| m.contains_key(keys)));
                if !shadowed && !out.contains(keys) {
                    out.push(keys.clone());
                }
            }
        }
        out
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
        let defaults = raw(&[(
            "Normal",
            &[
                ("<q>", Some("Quit")),
                ("<f10>", Some("Quit")),
                ("<f5>", Some("Copy")),
            ],
        )]);
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

    // --- Problem detection (User Story 3) ---

    fn problems(defaults: &RawBindings, user: &RawBindings) -> Vec<String> {
        Keymap::build(defaults, user)
            .problems
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn kinds(defaults: &RawBindings, user: &RawBindings) -> Vec<ProblemKind> {
        Keymap::build(defaults, user)
            .problems
            .iter()
            .map(|p| p.kind)
            .collect()
    }

    #[test]
    fn user_override_of_the_same_key_is_not_a_problem() {
        let defaults = raw(&[("Normal", &[("<f5>", Some("Copy")), ("<f6>", Some("Move"))])]);
        let user = raw(&[("Normal", &[("<f5>", Some("Move")), ("<c>", Some("Copy"))])]);
        assert_eq!(problems(&defaults, &user), Vec::<String>::new());
    }

    #[test]
    fn prefix_ambiguity_is_reported_and_single_key_wins() {
        let defaults = raw(&[("Normal", &[("<g>", Some("NavTop"))])]);
        let user = raw(&[("Normal", &[("<g><g>", Some("NavBottom"))])]);
        assert_eq!(
            problems(&defaults, &user),
            vec![
                "[Normal] <g> → NavTop vs <g><g> → NavBottom: <g> is also a prefix — single key wins"
            ]
        );
        let km = Keymap::build(&defaults, &user);
        assert_eq!(
            km.resolve(&[KeyContext::Normal], &seq("<g>")),
            Lookup::Action(Action::NavTop)
        );
    }

    #[test]
    fn action_left_without_keys_is_unreachable() {
        let defaults = raw(&[(
            "Normal",
            &[
                ("<shift-f6>", Some("Rename")),
                ("<alt-shift-6>", Some("Rename")),
            ],
        )]);
        let user = raw(&[(
            "Normal",
            &[("<shift-f6>", None), ("<alt-shift-6>", Some("Unbind"))],
        )]);
        assert_eq!(
            problems(&defaults, &user),
            vec!["[Normal] Rename: no key left after overrides — action unreachable"]
        );
        // Rebinding the only key to another action also leaves it unreachable.
        let defaults = raw(&[("Git", &[("<s>", Some("GitStage"))])]);
        let user = raw(&[("Git", &[("<s>", Some("GitPush"))])]);
        assert_eq!(kinds(&defaults, &user), vec![ProblemKind::Unreachable]);
    }

    #[test]
    fn duplicate_key_after_normalization_later_entry_wins() {
        let user = raw(&[(
            "Normal",
            &[("<minus>", Some("Copy")), ("<hyphen>", Some("Move"))],
        )]);
        assert_eq!(
            problems(&RawBindings::new(), &user),
            vec!["[Normal] <hyphen> and <minus>: same key written twice — later entry wins"]
        );
        // Sorted, `<minus>` comes after `<hyphen>`, so it wins.
        let km = Keymap::build(&RawBindings::new(), &user);
        assert_eq!(
            km.resolve(&[KeyContext::Normal], &seq("<minus>")),
            Lookup::Action(Action::Copy)
        );
        let user = raw(&[(
            "Normal",
            &[("<Y>", Some("Copy")), ("<shift-y>", Some("Copy"))],
        )]);
        assert_eq!(
            kinds(&RawBindings::new(), &user),
            vec![ProblemKind::DuplicateKey]
        );
    }

    #[test]
    fn printable_key_in_text_input_context_is_skipped() {
        let user = raw(&[(
            "GitCommit",
            &[
                ("<a>", Some("GitCommitSubmit")),
                ("<A>", Some("GitCommitSubmit")),
                ("<ctrl-s>", Some("GitCommitSubmit")),
                ("<alt-a>", Some("GitCommitSubmit")),
                ("<x>", None),
            ],
        )]);
        assert_eq!(
            problems(&RawBindings::new(), &user),
            vec![
                "[GitCommit] <A> → GitCommitSubmit: would steal typed text — entry skipped",
                "[GitCommit] <a> → GitCommitSubmit: would steal typed text — entry skipped",
            ]
        );
        let km = Keymap::build(&RawBindings::new(), &user);
        let chain = [KeyContext::GitCommit];
        assert_eq!(km.resolve(&chain, &seq("<a>")), Lookup::None);
        assert_eq!(
            km.resolve(&chain, &seq("<ctrl-s>")),
            Lookup::Action(Action::GitCommitSubmit)
        );
        // Not a text-input context: plain letters are fine.
        let user = raw(&[("DialogConfirm", &[("<j>", Some("DialogConfirm"))])]);
        assert_eq!(problems(&RawBindings::new(), &user), Vec::<String>::new());
    }

    #[test]
    fn quit_unreachable_is_a_warning() {
        let defaults = raw(&[("Normal", &[("<f10>", Some("Quit"))])]);
        let user = raw(&[("Normal", &[("<f10>", None)])]);
        assert_eq!(
            problems(&defaults, &user),
            vec!["[Normal] Quit: no key left after overrides — Ctrl+C still quits"]
        );
        assert_eq!(kinds(&defaults, &user), vec![ProblemKind::QuitUnreachable]);
    }

    #[test]
    fn unknown_action_and_unparseable_key_messages() {
        let user = raw(&[(
            "Git",
            &[
                ("<ctrl-q>", Some("GitPushh")),
                ("<alt-x5>", Some("GitPush")),
            ],
        )]);
        assert_eq!(
            problems(&RawBindings::new(), &user),
            vec![
                "[Git] <alt-x5> → GitPush: unparseable key — entry skipped",
                "[Git] <ctrl-q> → GitPushh: unknown action — entry skipped",
            ]
        );
    }

    #[test]
    fn config_unreadable_message() {
        assert_eq!(
            BindingProblem::config_unreadable("bad json").to_string(),
            "config file: could not be read (bad json) — defaults used"
        );
    }

    #[test]
    fn shipped_defaults_have_no_problems_in_any_context() {
        assert_eq!(
            problems(&crate::config::default_keybindings(), &RawBindings::new()),
            Vec::<String>::new()
        );
    }

    // --- Display names (User Story 4) ---

    fn names(raw: &str) -> (String, String) {
        let s = seq(raw);
        (
            seq_name(&s, KeyStyle::Compact),
            seq_name(&s, KeyStyle::Full),
        )
    }

    #[test]
    fn key_names_compact_and_full() {
        let cases = [
            ("<f5>", "F5", "F5"),
            ("<shift-f6>", "⇧F6", "Shift+F6"),
            ("<alt-5>", "Alt+5", "Alt+5"),
            ("<alt-shift-6>", "Alt+⇧6", "Alt+Shift+6"),
            ("<ctrl-b>", "^B", "Ctrl+B"),
            ("<ctrl-shift-b>", "^⇧B", "Ctrl+Shift+B"),
            ("<esc>", "Esc", "Esc"),
            ("<space>", "Spc", "Space"),
            ("<tab>", "Tab", "Tab"),
            ("<backtab>", "⇧Tab", "Shift+Tab"),
            ("<pageup>", "PgUp", "Page Up"),
            ("<up>", "↑", "Up"),
            ("<a>", "a", "a"),
            ("<A>", "A", "A"),
            ("<?>", "?", "?"),
            ("<alt-minus>", "Alt+-", "Alt+-"),
            ("<ctrl-enter>", "^Enter", "Ctrl+Enter"),
            ("<ctrl-f><5>", "^F 5", "Ctrl+F 5"),
            ("<ctrl-f><s><6>", "^F s 6", "Ctrl+F s 6"),
        ];
        for (raw, compact, full) in cases {
            assert_eq!(names(raw), (compact.to_owned(), full.to_owned()), "{raw}");
        }
    }

    #[test]
    fn keys_in_chain_skips_keys_shadowed_by_an_earlier_context() {
        let defaults = raw(&[
            (
                "Dialog",
                &[
                    ("<enter>", Some("DialogConfirm")),
                    ("<y>", Some("DialogConfirm")),
                ],
            ),
            (
                "DialogConfirm",
                &[
                    ("<y>", Some("DialogCancel")),
                    ("<Y>", Some("DialogConfirm")),
                ],
            ),
        ]);
        let km = Keymap::build(&defaults, &RawBindings::new());
        let chain = [KeyContext::DialogConfirm, KeyContext::Dialog];
        assert_eq!(
            km.keys_in_chain(&chain, &Action::DialogConfirm),
            vec![seq("<Y>"), seq("<enter>")]
        );
    }
}
