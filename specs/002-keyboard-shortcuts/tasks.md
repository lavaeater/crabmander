---
description: "Task list for the keyboard shortcut overhaul"
---

# Tasks: Keyboard Shortcut Overhaul

**Input**: Design documents from `specs/002-keyboard-shortcuts/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md),
[data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included. FR-006 requires an automated "defaults are conflict-free" test, and
Constitution V requires unit tests for new logic. Tests are in-module `#[cfg(test)]` blocks, use
`pretty_assertions`, and never touch the real config dir.

**Organization**: Grouped by user story (spec.md priorities). The keymap core and the dispatch
rewrite are foundational because every story depends on them. After Phase 2 the app behaves exactly
as today, but all keys come from the keymap.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on unfinished tasks)
- **[Story]**: US1–US4 from spec.md
- Gate after every task group: `cargo fmt --all --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked --all-features`

---

## Phase 1: Setup (verification spike, R5)

**Purpose**: Confirm how terminals report Alt+digit, Alt+Shift+digit, Alt+Minus and Alt+= before the
normalization is written. Needs the user at a real keyboard.

- [X] T001 Add a `trace!` log of every raw crossterm `KeyEvent` (code, modifiers, kind, state) in the event loop of src/tui.rs, plus a `debug!` at startup saying whether the kitty keyboard enhancement was pushed (near `supports_keyboard_enhancement()`)
- [X] T002 Run `RUST_LOG=trace cargo run` in a kitty-protocol terminal and a legacy terminal, US and Swedish layouts; press Alt+5, Alt+Shift+6, Alt+Minus, Alt+=, Shift+Y; record the events received in a "Spike results" section at the end of specs/002-keyboard-shortcuts/research.md (manual, with the user)
- [X] T003 If T002 contradicts the R5/R6 assumptions (e.g. Alt+Shift+6 arrives as `Char('&')` or `Char('^')` under the kitty protocol), update the KeySeq rule in specs/002-keyboard-shortcuts/data-model.md and the decision in research.md (R5, R6); consider adding `REPORT_ALTERNATE_KEYS` in src/tui.rs

**Checkpoint**: Normalization rule for `KeySeq` is settled.

---

## Phase 2: Foundational (keymap core and dispatch, no behaviour change)

**Purpose**: Every key in every mode resolves through one `Keymap`. Defaults reproduce today's
hardcoded behaviour exactly, so the app works the same at the end of this phase.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Keymap types

- [X] T004 Add new `Action` variants `GitPageUp`, `GitPageDown`, `HelpClose`, `HelpScrollUp`, `HelpScrollDown` in src/action.rs (data-model.md: ActionSpec, "New Action variants needed"); handle `GitPageUp`/`GitPageDown` in `App::handle_actions` in src/app.rs by calling the existing GitView page methods; leave the Help ones as no-ops for now
- [X] T005 Create src/keymap.rs and register it in src/main.rs: `KeyContext` enum with the 11 variants, serde/strum names equal to the variant names, an `is_text_input()` method, and a `chain(mode, dialog)` function returning the ordered fallback contexts (data-model.md: KeyContext table — `Active when`, `Text input?`, `Falls back to`)
- [X] T006 In src/keymap.rs add `KeySeq` (newtype over `Vec<KeyEvent>`) with `normalize(KeyEvent)` per data-model.md KeySeq (Press only, empty `state`, case and SHIFT bit as decided in T003), and `Hash`/`Eq` on the normalized form
- [X] T007 In src/keymap.rs add the static action catalog `ActionSpec { action, name, context, label, description, group, bar_priority }` covering every bindable action of every context (data-model.md: ActionSpec fields; label ≤ 8 chars; labels/groups taken from the README tables and today's func_bar tables in src/components/func_bar.rs); add `lookup_action(context, name) -> Option<Action>`
- [X] T008 In src/keymap.rs add `Binding` and `BindingSource` (data-model.md: Binding fields) and `BindingProblem` with `ProblemKind` (data-model.md: BindingProblem.kind values, BindingProblem.resolution) and a `Display` impl producing the line format in contracts/ui-hints.md "Startup problem report"
- [X] T009 In src/keymap.rs add `Keymap { bindings, keys_for, problems }` (data-model.md: Keymap fields; `keys_for` ordered F-key first, then source order) with `Keymap::build(defaults, user) -> Keymap`: parse each entry independently, unknown action or bad key → problem + entry skipped, user entry replaces the default for the same `(context, keys)`; plus `resolve(chain, &[KeyEvent]) -> Lookup { Action | Prefix | None }`

### Config wiring

- [X] T010 In src/config.rs replace the panicking `KeyBindings` deserializer with a raw form `HashMap<String, HashMap<String, Option<String>>>` (context name → key string → action name or null) kept separately for defaults and user config, then build `Keymap` from it; remove `parse_key_sequence(..).unwrap()`; unknown context names become a problem instead of an error
- [X] T011 [P] In src/config.rs add `equals` as a named key in the key parser (`<alt-equals>` and `<alt-=>` both work) and keep `hyphen`/`minus` (contracts/keybindings-config.md "Key sequence notation")

### Defaults reproduce today

- [X] T012 Move every hardcoded key from `App::handle_key_event` in src/app.rs (Dialog, GitCommit, GitBranch, GitTag, Git incl. F1–F7, PgUp/PgDn → GitPageUp/GitPageDown, GlobalGit, Cargo, Normal Ctrl+B/Ctrl+G) into per-context sections of .config/config.json5 with the same keys; split dialog keys into `Dialog`, `DialogConfirm`, `QuickCd` per contracts/keybindings-config.md "Default bindings"
- [X] T013 Rewrite `App::handle_key_event` in src/app.rs: Ctrl+C → `Quit` before anything else (R9, FR-010); compute the context chain; keep the `last_tick_key_events` buffer for multi-key sequences for every context; on `Lookup::Action` send it; in text-input contexts send unmatched printable keys (no Ctrl/Alt) as `GitCommit` editor input / `DialogInputChar` / `QuickCdChar`; in Normal keep today's unbound-printable → `FilterChar`, Backspace and Esc filter handling; remove the per-mode `match` blocks
- [X] T014 Replace `Config.keybindings` lookups in src/app.rs with the `Keymap` built at startup and stored on `App`

### Foundational tests

- [X] T015 [P] Unit tests in src/keymap.rs: `KeySeq` normalization (repeat/release ignored, state dropped, `<Y>` == `<shift-y>`), `KeyContext::chain` for every mode and dialog kind, `Keymap::build` override-same-key, unknown action and bad key skipped with a problem, `resolve` for single key, prefix and multi-key sequence
- [X] T016 [P] Unit tests in src/config.rs: the embedded default config parses into a keymap; an old-style user config with only a `Normal` section loads with the same Normal bindings as before (FR-007, SC-006); `equals` key name parses

**Checkpoint**: `cargo test` passes; the app behaves the same as before in every mode, but all keys
come from config defaults.

---

## Phase 3: User Story 1 - Every F-key action works without F-keys (Priority: P1) 🎯 MVP

**Goal**: The Alt+digit scheme reaches every F-key and Shift+F-key action in every mode; the
Shift-letter fallbacks are gone.

**Independent Test**: quickstart.md section 2: with no F-keys, Alt+1 … Alt+0, Alt+Minus, Alt+=,
Alt+Shift+1/2/6/9 in Normal and Alt+1 … Alt+7 in Git each do what the F-key does; Shift+Y starts the
filter.

### Tests for User Story 1

- [X] T017 [P] [US1] Test in src/keymap.rs: for every context, every action with an F-key or Shift+F-key default also has an Alt binding and a `<ctrl-f>` leader sequence (US1 scenario 2, SC-001, FR-004c)
- [X] T018 [P] [US1] Test in src/keymap.rs: Normal defaults keep F5 Copy, F6 Move, F7 Mkdir, F8 Delete, F10 Quit, Tab switch panel (FR-005), and contain none of `<shift-x/e/z/y/r/k/d/s/t/g>` (FR-004b)
- [X] T019 [P] [US1] Test in src/keymap.rs: a normalized event for Alt+Shift+6 as reported under the kitty protocol (per T002 results) resolves to `Rename` in Normal (FR-004a)

### Implementation for User Story 1

- [X] T020 [US1] In .config/config.json5 Normal section: add the Alt+digit defaults from the contracts/keybindings-config.md Normal table (Alt+1 … Alt+9, Alt+0, Alt+Minus, Alt+=, Alt+Shift+1/2/6/9), the Ctrl+F leader sequences (`<ctrl-f><1>` … `<ctrl-f><=>`, `<ctrl-f><s><1/2/6/9>`, FR-004c) and remove the Shift-letter fallbacks
- [X] T021 [US1] In .config/config.json5 Git section: add `<alt-1>` … `<alt-7>` and `<ctrl-f><1>` … `<ctrl-f><7>` mirroring F1–F7
- [X] T022 [US1] Add Alt+digit defaults in any other context of .config/config.json5 that binds F-keys (check GlobalGit, Cargo, GitBranch, GitTag after T012) so that T017 passes

**Checkpoint**: MVP. A user on COSMIC can reach every action, including Rename, Recent dirs, User
commands and Invert sort.

---

## Phase 4: User Story 2 - Rebind keys in every mode (Priority: P1)

**Goal**: Any context can be overridden or have bindings removed from the user config; text input is
protected.

**Independent Test**: quickstart.md section 3: the sample config makes Ctrl+Y copy, `q` start a
filter, `x` stage in Git and `r` run in Cargo; everything else unchanged.

### Tests for User Story 2

- [X] T023 [P] [US2] Tests in src/keymap.rs: user `null` removes a default; `"Unbind"` behaves like `null`; user override in a non-Normal context (Git, Cargo, DialogConfirm) applies while other defaults stay; removing a key falls back along the context chain (e.g. QuickCd → Dialog)
- [X] T024 [P] [US2] Tests in src/app.rs for the filter rule (R7, tested through the pure `filter_claims` helper): with an empty filter a bound printable key runs its action and an unbound one sends `FilterChar`; with a non-empty filter bound printable keys (`j`, `q`, `?`, `Y`) send `FilterChar`; Ctrl/Alt keys still resolve as bindings

### Implementation for User Story 2

- [X] T025 [US2] Support `null` and the `"Unbind"` alias in `Keymap::build` in src/keymap.rs (R2): a user entry with no action removes the binding for that `(context, keys)`; removal of a non-existent binding is silently accepted
- [X] T026 [US2] Implement the filter rule (R7) in `App::handle_key_event` in src/app.rs: when the active panel filter is non-empty, unmodified printable chars and Shift+letter go to `FilterChar` before the keymap lookup
- [X] T027 [US2] In src/app.rs make sure GitCommit editor and dialog text fields receive every printable key without Ctrl/Alt even if a binding exists for it in that context (FR-013); such bindings are reported in US3 (T030)
- [X] T028 [US2] Verify the INI and YAML/TOML config formats still load through the new raw form (null in YAML/TOML, `Unbind` in INI) with a test in src/config.rs that loads a small user config string per format (done for YAML `~`, TOML `"Unbind"`, JSON `null`; INI not covered: it has no nested sections for `keybindings.<Context>`)

**Checkpoint**: Every context is rebindable; existing configs still load.

---

## Phase 5: User Story 3 - Conflicts and mistakes are reported (Priority: P2)

**Goal**: Every problem kind from FR-008 is detected, shown once at startup and logged.

**Independent Test**: quickstart.md section 4: a config with one entry per problem kind starts and
shows "Keybinding problems (5)" with one line each; the log has the same lines.

### Tests for User Story 3

- [X] T029 [P] [US3] Tests in src/keymap.rs, one per `ProblemKind` (data-model.md: BindingProblem.kind): PrefixAmbiguity (`<g>` + `<g><g>`), Unreachable (user removes all keys for Rename), DuplicateKey (`<minus>` and `<hyphen>` in one source), TextInputSteal (`<a>` in GitCommit), QuitUnreachable, UnknownAction, UnparseableKey; each with the expected `resolution` text; plus a user override of the same key is NOT a problem
- [X] T030 [P] [US3] Test in src/keymap.rs: the shipped defaults produce zero problems in every context (FR-006)

### Implementation for User Story 3

- [X] T031 [US3] Add the validation pass to `Keymap::build` in src/keymap.rs implementing R4: prefix ambiguity (single key wins), unreachable action (default-bound action with no key left), duplicate after normalization (later wins), text-input steal (entry skipped), Quit unreachable in Normal (warning only)
- [X] T032 [US3] Turn config-level failures (unreadable/malformed user config file) into a `ConfigUnreadable` problem with resolution "defaults used" in src/config.rs instead of failing startup (FR-009)
- [X] T033 [US3] In src/app.rs at startup: log every problem with `warn!` and, if any exist, open `DialogState::ErrorList` titled "Keybinding problems (N)" with one `Display` line per problem before any other dialog (contracts/ui-hints.md "Startup problem report"); dismiss with Enter/Esc

**Checkpoint**: Config mistakes are visible and never block startup.

---

## Phase 6: User Story 4 - On-screen hints show the keys in effect (Priority: P2)

**Goal**: The function bar and a `?` help screen are generated from the keymap.

**Independent Test**: quickstart.md section 5: with overrides the bar shows `^Y` for Copy and `x` for
Stage; `?` lists every action with its keys; `?` in the commit editor types `?`; at 60 columns entries
drop whole.

### Tests for User Story 4

- [ ] T034 [P] [US4] Tests in src/keymap.rs for key display names: compact (`F5`, `⇧F6`, `Alt+5`, `Alt+⇧6`, `^B`, `Esc`, `Spc`, `Tab`, `PgUp`) and full notation for help
- [ ] T035 [P] [US4] Tests in src/components/func_bar.rs: entries in catalog priority order, up to two keys F-key first, unbound actions hidden, lowest-priority entries dropped whole when narrow, `? Help` kept last when it fits

### Implementation for User Story 4

- [ ] T036 [US4] Add compact and full key display functions to src/keymap.rs (contracts/ui-hints.md "Function bar" key names)
- [ ] T037 [US4] Rewrite src/components/func_bar.rs to build entries from the `Keymap` and catalog for the active context (data-model.md: ActionSpec.label, ActionSpec.bar_priority) with width fitting; delete `NORMAL_KEYS`, `GIT_KEYS`, `CARGO_KEYS`, `GLOBAL_GIT_KEYS`; pass the keymap/context in from src/app.rs
- [ ] T038 [US4] Expose from src/tui.rs whether the kitty keyboard enhancement is active, and store it on `App` in src/app.rs
- [ ] T039 [P] [US4] Create src/components/help.rs and re-export from src/components.rs: overlay titled "Keys — <context> mode", actions of the context chain grouped by `ActionSpec.group` (data-model.md: ActionSpec.group, ActionSpec.description), all keys in full notation or "(unbound)", scroll state, and the legacy-terminal footer note when the enhancement is off (contracts/ui-hints.md "Help screen")
- [ ] T040 [US4] Add `Mode::Help` in src/app.rs: `Action::Help` from a non-text context remembers the prior mode and opens help; `HelpClose` returns to it unchanged; `HelpScrollUp`/`HelpScrollDown` and page keys scroll; render the overlay last (data-model.md "State transitions")
- [ ] T041 [US4] Add the `Help` context defaults and `<?>` → Help in every non-text context, and `<*>` → ToggleMarkAll in Normal, in .config/config.json5 (contracts/keybindings-config.md)

**Checkpoint**: All four stories work; hints always match real bindings.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T042 [P] Update README.md keyboard reference (FR-014): Alt+digit scheme, per-context config sections, `null`/`Unbind` removal, `?` help, removal of the Shift-letter fallbacks with the restore snippet from contracts/keybindings-config.md, the legacy-terminal Alt+Shift+digit note and how to bind the layout symbol instead
- [ ] T043 [P] Update CLAUDE.md architecture section: `keymap.rs`, `KeyContext` and its chain, where defaults live, and correct the stale "Mode: currently only Home" line
- [ ] T044 Remove the temporary raw-key `trace!` from T001 in src/tui.rs if it is too noisy, or keep it at trace level with a comment
- [ ] T045 Run the quality gates (fmt, clippy `-D warnings`, `cargo test --locked --all-features`) and the full manual run-through in specs/002-keyboard-shortcuts/quickstart.md, recording the terminal/layout matrix results
- [ ] T046 Update the header of specs/002-keyboard-shortcuts/spec.md (branch `002-keyboard-shortcuts`, status Implemented)

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (spike)**: none; T002 needs the user at a keyboard.
- **Phase 2 (foundational)**: needs T003's normalization decision for T006; blocks all stories.
- **US1 (Phase 3)**: after Phase 2. Independent of US2–US4.
- **US2 (Phase 4)**: after Phase 2. Independent of US1 (works with or without the Alt defaults).
- **US3 (Phase 5)**: after Phase 2. T030 (defaults clean) should be re-run after US1 and US4 change
  the defaults.
- **US4 (Phase 6)**: after Phase 2. Shows whatever bindings exist, so it is best done after US1 so the
  bar shows the fallback keys.
- **Polish (Phase 7)**: after the stories that are shipped.

### Within Phase 2

T004 → T005 → T006 → T007 → T008 → T009 → T010 → T012 → T013 → T014; T011 can run alongside T009;
T015/T016 after T009/T010.

### Within stories

Tests first (they should fail), then implementation. Tasks touching .config/config.json5 (T020–T022,
T041) and src/app.rs (T024, T026, T027, T033, T038, T040) must run one at a time.

---

## Parallel Examples

```text
# Phase 2, once T009 is done:
T011 equals key name in src/config.rs
T015 keymap unit tests in src/keymap.rs

# US1 tests together:
T017, T018, T019 (all src/keymap.rs test module, independent test functions)

# US4:
T035 func_bar tests in src/components/func_bar.rs
T039 help component in src/components/help.rs
```

---

## Implementation Strategy

### MVP first

1. Phase 1 spike with the user (T001–T003).
2. Phase 2 foundation; check the app behaves as before.
3. Phase 3 (US1): Alt+digit defaults. **Stop and validate** with quickstart.md section 2 on COSMIC.

### Incremental delivery

1. US1 → validate → commit.
2. US2 → quickstart.md section 3 → commit.
3. US3 → quickstart.md section 4 → commit.
4. US4 → quickstart.md section 5 → commit.
5. Polish, docs, full quickstart run.

## Notes

- Constrained fields cited from data-model.md: KeyContext (text-input flag, fallback chain) in T005;
  KeySeq normalization in T006; ActionSpec.label/.group/.bar_priority/.description in T007, T037,
  T039; Binding fields in T008; BindingProblem.kind/.resolution in T008, T029; Keymap fields in T009.
- Commit after each task group; the user commits.
