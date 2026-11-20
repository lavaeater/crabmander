# Implementation Plan: Keyboard Shortcut Overhaul

**Branch**: `002-keyboard-shortcuts` | **Date**: 2026-11-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/002-keyboard-shortcuts/spec.md`

## Summary

Move every mode's key handling out of the hardcoded `match` blocks in `App::handle_key_event` into
the keybinding config, using per-context sections with built-in defaults. Add a resolved `Keymap`
that merges defaults and user entries (with `null` to remove), validates them, and reports problems
at startup in the existing error-list dialog instead of panicking. Replace the arbitrary Shift-letter
fallbacks with an Alt+digit scheme mirroring the F-keys, generate the function bar from the bindings
in effect, and add a `?` help screen. Details and trade-offs are in [research.md](research.md).

## Technical Context

**Language/Version**: Rust, edition 2024, nightly (`nightly-2026-11-07` via `tui/rust-toolchain.toml`)

**Primary Dependencies**: ratatui 0.30, crossterm 0.29 (kitty keyboard protocol via
`PushKeyboardEnhancementFlags`), config 0.15 + json5, serde, strum. No new dependencies.

**Storage**: User config file in the XDG config dir / `$CRABMANDER_CONFIG` (existing).

**Testing**: `cargo test` unit tests in-module (`#[cfg(test)]`), `pretty_assertions`.

**Target Platform**: Linux terminals (primary), macOS should keep compiling.

**Project Type**: Single-binary TUI application.

**Performance Goals**: Key lookup stays O(1) per key press; keymap built once at startup (<10 ms for
a few hundred bindings).

**Constraints**: Startup must never fail because of keybinding config (FR-009). Existing configs keep
working (FR-007). No key handling may block the render loop.

**Scale/Scope**: 11 key contexts, ~85 bindable actions, ~150 default bindings. Touches `config.rs`,
`app.rs` (key dispatch ~200 lines replaced), `func_bar.rs`, `action.rs`, one new module and one new
component.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Keyboard-First Commander Experience | ✅ Pass | The feature removes today's violations ("keybindings MUST be declared per Mode in configuration"). F5/F6/F7/F8/F10/Tab kept (FR-005). New keys surface in the function bar and README (FR-009, FR-014). |
| II. Responsive, Non-Blocking UI | ✅ Pass | Config parsing stays synchronous at startup as today; no I/O in key handling. |
| III. Safe File Operations | ✅ Pass | No change to operations or confirmations. Delete/overwrite still confirm. Removing Shift+D as a fallback for Delete makes accidental deletes less likely, not more. |
| IV. Action-Driven Component Architecture | ✅ Pass | All keys now resolve to `Action`s; Git PageUp/PageDown get actions instead of direct calls. Help screen is a component/mode emitting actions. Keymap logic is a terminal-free module, unit-testable. |
| V. Quality Gates | ✅ Pass | New logic ships with unit tests, including a test that the defaults are conflict-free; tests use no real config dir. |
| Technical Constraints | ✅ Pass | No new dependencies; XDG config with fallback to defaults on errors. |
| Development Workflow | ✅ Pass | README keyboard reference and config docs updated (FR-014); CLAUDE.md architecture note updated for the keymap module. |

**Post-design re-check (after Phase 1)**: still passing. The design adds one module and one
component, no new crates, no unjustified complexity. Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/002-keyboard-shortcuts/
├── plan.md              # This file
├── research.md          # Phase 0: baseline findings and decisions R1–R10
├── data-model.md        # Phase 1: KeyContext, ActionSpec, Binding, BindingProblem, Keymap
├── quickstart.md        # Phase 1: validation guide
├── contracts/
│   ├── keybindings-config.md   # config format + shipped defaults
│   └── ui-hints.md             # function bar, help screen, startup report
└── tasks.md             # Phase 2 (/speckit-tasks, not created here)
```

### Source Code (repository root)

```text
.config/
└── config.json5             # defaults for all 11 contexts; Shift-letter fallbacks removed

src/
├── keymap.rs                # NEW: KeyContext, KeySeq normalization, action catalog (ActionSpec),
│                            #      parse/merge/validate → Keymap + BindingProblems, display names
├── config.rs                # keybindings parsed per entry (no unwrap/panic), null removal,
│                            #      `equals` key name; styles untouched
├── action.rs                # + GitPageUp, GitPageDown, HelpClose, HelpScrollUp, HelpScrollDown
├── app.rs                   # handle_key_event → Ctrl+C guard, context chain lookup, text-input
│                            #      fallthrough, filter rule (R7); Mode::Help; startup report dialog
├── tui.rs                   # expose whether the kitty protocol is active (for the help note)
└── components/
    ├── func_bar.rs          # bar generated from Keymap + catalog priorities, width fitting
    └── help.rs              # NEW: help overlay (grouped list, scrolling, protocol note)

README.md                    # keyboard reference: Alt+digit scheme, per-mode config, null removal,
                             #      `?` help, how to restore old fallbacks, legacy-terminal note
CLAUDE.md                    # architecture: keymap module and contexts
```

**Structure Decision**: Single-project layout as today. The new `keymap` module holds all
terminal-free binding logic so it can be unit tested in isolation (Constitution IV/V); `app.rs` only
asks it "which action does this key mean in this context".

## Implementation outline (for /speckit-tasks)

1. **Spike (R5)**: trace-log raw key events; check Alt+digit, Alt+Shift+digit, Alt+Minus, Alt+= on
   kitty-protocol and legacy terminals, US and Swedish layouts. Adjust normalization if needed.
2. **Keymap core**: `keymap.rs` with contexts, normalization, catalog, per-entry parsing, merge with
   `null`, problem detection (R4), defaults-are-clean test.
3. **Config wiring**: `config.rs` produces raw entries; no panics; `equals` key; INI `"Unbind"` alias.
4. **Dispatch**: replace hardcoded mode blocks in `app.rs` with context-chain lookup; Ctrl+C guard;
   text-input fallthrough; filter rule; new Git page actions; move Ctrl+B/Ctrl+G to config.
5. **Defaults**: write all contexts in `.config/config.json5`; Alt+digit fallbacks; remove
   Shift-letters; bind `*` and `?`.
6. **Startup report**: show problems in the ErrorList dialog and log them.
7. **Function bar**: generate from keymap; fitting by priority.
8. **Help screen**: `Mode::Help`, `help.rs`, protocol note.
9. **Docs**: README and CLAUDE.md.

Stories map to steps: Story 1 → 1, 2, 4, 5; Story 2 → 2–5; Story 3 → 2, 6; Story 4 → 7, 8.

## Complexity Tracking

No constitution violations; nothing to justify.
