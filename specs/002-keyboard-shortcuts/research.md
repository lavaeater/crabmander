# Research: Keyboard Shortcut Overhaul

Findings from reading the current code (`src/app.rs`, `src/config.rs`, `src/tui.rs`,
`src/components/func_bar.rs`, `.config/config.json5`) and the decisions they lead to.

## Current state (baseline)

- Only `Normal` has bindings in `.config/config.json5`. `App::handle_key_event` (`src/app.rs`)
  hardcodes keys for `Dialog`, `GitCommit`, `GitBranch`, `GitTag`, `Git`, `GlobalGit` and `Cargo`,
  plus `Ctrl+B` (cargo mode) and `Ctrl+G` (git mode) for Normal.
- Git mode already uses F1–F7 (stage, unstage, commit, push, pull, branches, add-all+commit), so the
  Alt+digit scheme applies there too.
- Git mode PageUp/PageDown call `GitView` methods directly instead of sending an `Action`.
- `KeyBindings` deserialization calls `parse_key_sequence(..).unwrap()`: one bad key string panics
  at startup. An unknown action name fails the whole config load (`Action` is deserialized by serde).
- User bindings are merged per key with `entry().or_insert` — overrides work, removal does not.
- The function bar labels are hardcoded `&[(&str, &str)]` tables per `FuncBarMode`.
- `Action::Help` exists but does nothing.
- `Action::ToggleMarkAll` is handled but bound to no key, although the README documents `*`.
- The kitty keyboard protocol is pushed with `DISAMBIGUATE_ESCAPE_CODES` only when
  `supports_keyboard_enhancement()` is true (`src/tui.rs`).

## R1: Where context-specific bindings live

- **Decision**: Introduce a `KeyContext` enum used only for keybinding sections:
  `Normal`, `Git`, `GitCommit`, `GitBranch`, `GitTag`, `GlobalGit`, `Cargo`, `Dialog`,
  `DialogConfirm`, `QuickCd`, `Help`. Config section names equal the variant names, so existing
  `"Normal": {…}` configs load unchanged. The app's `Mode` stays the runtime state; a small function
  maps `(Mode, dialog kind)` to an ordered list of contexts to search (e.g. QuickCd →
  `[QuickCd, Dialog]`, Confirm → `[DialogConfirm, Dialog]`).
- **Rationale**: Dialog kinds need different keys (QuickCd Tab completion, Confirm y/n) without
  inventing fake app modes. A fallback chain keeps shared dialog keys in one place.
- **Alternatives**: Adding variants to `Mode` (mixes UI state with config scopes); one flat `Dialog`
  section with guards (keeps hidden hardcoded conditions, which the spec wants gone).

## R2: Removing a default binding

- **Decision**: A `null` value removes the binding for that key in that context:
  `"<shift-y>": null`. Parsed as `HashMap<String, Option<String>>`.
- **Rationale**: json5, JSON, YAML and TOML-via-`config` all have a null/none notion that maps to
  `Option`; it reads naturally and needs no magic action name.
- **Alternatives**: A reserved action `"Unbind"` (works in every format, but reads like an action and
  needs special-casing everywhere). Kept as a fallback if a format cannot express null (INI); planning
  accepts `"Unbind"` as an alias for `null` for that reason.

## R3: Robust config parsing and problem reporting

- **Decision**: Parse each `(key string, action string)` entry independently into a
  `Result<Binding, BindingProblem>`. Actions are parsed by name against a catalog of bindable actions
  (unit variants only), not via full serde deserialization of `Action`. Invalid entries are dropped
  and recorded; valid ones apply. After merging, a validation pass adds conflict problems.
  Problems are logged with `tracing::warn!` and shown once at startup in the existing
  `DialogState::ErrorList` dialog.
- **Rationale**: FR-008/FR-009 require startup to survive any mistake. The ErrorList dialog already
  exists with scrolling and tests.
- **Alternatives**: Failing startup (violates FR-009); status-bar only (too small for several
  problems).

## R4: What counts as a conflict

- **Decision**: Report:
  1. *Prefix ambiguity*: a sequence that is both a complete binding and a strict prefix of another in
     the same context (today the single key silently wins).
  2. *Unreachable action*: an action bound by default in a context that has no key left after merge.
  3. *Duplicate after normalization*: two different key strings in the same source that normalize to
     the same key (e.g. `<minus>` and `<hyphen>`, `<shift-y>` and `<Y>`); the later one wins.
  4. *Text-input steal*: in a text-input context (`GitCommit`, `Dialog`, `QuickCd`), a printable key
     with no modifier or only Shift.
  5. *Unknown action* and *unparseable key* (from R3).
  6. *Quit unreachable*: no binding left for `Quit` in `Normal` (warning; Ctrl+C still quits).
  A user override that replaces a default for the same key is intentional and **not** reported.
- **Rationale**: Covers FR-008 (a)–(e). True duplicate keys inside one JSON object are dropped by the
  parser before we see them; that limit is documented (spec edge case already allows for it).

## R5: Alt+digit and Alt+Shift+digit on non-US layouts (FR-004a)

- **Decision**: Bind defaults as `<alt-5>` and `<alt-shift-6>`. Normalize incoming key events before
  lookup: drop `state`, keep only Press events, and for `Char` codes compare case-sensitively with the
  SHIFT bit as reported. With the kitty protocol, terminals report the **unshifted** key plus
  modifiers, so Alt+Shift+6 arrives as `Char('6')` + ALT|SHIFT on any layout and matches. On terminals
  without the protocol, Alt+Shift+6 arrives as Alt + the layout's symbol (e.g. `&`) and cannot be
  mapped back to a digit reliably.
- **Consequence**: On legacy terminals, Alt+Shift+digit fallbacks are documented as unavailable; the
  help screen shows a one-line note when `supports_keyboard_enhancement()` is false, and the README
  shows how to bind the layout's symbol instead (e.g. `"<alt-&>": "Rename"`).
- **To verify during implementation** (first task): log raw `KeyEvent`s with `RUST_LOG=trace` in
  kitty, foot, Alacritty, COSMIC Terminal and one legacy terminal, US and Swedish layouts, for
  Alt+5, Alt+Shift+6, Alt+Minus, Alt+Equals and Shift+Y. If crossterm reports something different from
  the above, adjust the normalization; consider adding `REPORT_ALTERNATE_KEYS`.
- **Alternatives**: Matching on produced symbols per layout (needs layout tables, fragile).

## R6: F11 and F12 fallbacks

- **Decision**: `<alt-minus>` (F11, Select theme) and `<alt-=>` (F12, Global Git); add `equals` as a
  named key in the parser for readability.
- **Rationale**: Matches the spec. On layouts where those positions produce other characters (e.g.
  `+` and a dead key on Nordic layouts), the kitty-protocol behaviour from R5 decides what arrives;
  this is part of the R5 verification. Both are rebindable.

## R7: Printable keys while the Normal-mode filter is active (FR-013)

- **Decision**: When the active panel's filter is non-empty, unmodified printable characters
  (and Shift+letter) go to the filter even if bound (e.g. `j`, `q`, `?`). When the filter is empty,
  bound printable keys run their action and unbound ones start the filter (today's behaviour).
- **Rationale**: Extends the existing special case for Backspace. Lets a user type any filename once
  filtering has started; Esc clears the filter and restores the bindings.
- **Alternatives**: Bound keys always win (today: typing `j` in a filter moves the cursor instead).

## R8: Function bar and help screen generation

- **Decision**: A static action catalog gives, per context, each bindable action's short label, help
  description, help group and function-bar priority. The function bar takes the context's actions in
  priority order and renders each with up to two keys in effect (F-key first, then fallback), dropping
  lowest-priority entries when the width runs out. The help screen lists all actions of the active
  context grouped like the README, with every key in effect, scrollable.
- **Rationale**: FR-011/FR-012 and SC-003: labels can only be correct if derived from the resolved
  bindings.
- **Key display**: `F5`, `⇧F6`, `Alt+5`, `Alt+⇧6`, `^B` (compact) in the function bar; full names in
  help.

## R9: Ctrl+C

- **Decision**: Checked before any context lookup in every mode, including GitCommit, and sends
  `Quit`. Not configurable.
- **Rationale**: FR-010; a safety exit if a config removes everything.

## R10: Testing approach

- Unit tests (no terminal): key parsing (all named keys, Alt/Shift combos, `equals`, `null`), merge
  and removal, every problem kind, context fallback chains, filter rule (R7), function-bar fitting and
  key labels, and **a test that the shipped defaults produce zero problems in every context**
  (FR-006).
- Manual checks in `quickstart.md` for real terminals and layouts (R5).
