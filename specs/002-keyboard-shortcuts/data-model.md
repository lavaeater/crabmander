# Data Model: Keyboard Shortcut Overhaul

In-memory types only; the persistent form is the config file described in
[contracts/keybindings-config.md](contracts/keybindings-config.md).

## KeyContext

Scope a set of bindings applies to. Config section names equal variant names.

| Variant | Active when | Text input? | Falls back to |
|---------|-------------|-------------|---------------|
| `Normal` | `Mode::Normal` | no (filter rule R7) | — |
| `Git` | `Mode::Git` | no | — |
| `GitCommit` | `Mode::GitCommit` | yes | — |
| `GitBranch` | `Mode::GitBranch` | no | — |
| `GitTag` | `Mode::GitTag` | no | — |
| `GlobalGit` | `Mode::GlobalGit` | no | — |
| `Cargo` | `Mode::Cargo` | no | — |
| `Dialog` | `Mode::Dialog`, any dialog kind | yes | — |
| `DialogConfirm` | Confirm dialog | no | `Dialog` |
| `QuickCd` | Quick CD dialog | yes | `Dialog` |
| `Help` | help screen open | no | — |

Rules: lookup searches the context chain in order and stops at the first match. In text-input
contexts, printable keys without Ctrl/Alt that match no binding go to the text field.

## KeySeq

Normalized key sequence: `Vec<KeyEvent>` where each event has `kind = Press` and empty `state`.
For `Char` codes (research.md "Spike results"): letters are upper case + SHIFT if either was
reported, else lower case without SHIFT; digits keep SHIFT as reported; other chars drop SHIFT.
Equality and hashing are on the normalized form.

## ActionSpec (static catalog)

One entry per bindable action per context.

| Field | Meaning |
|-------|---------|
| `action` | `Action` unit variant (e.g. `Copy`, `GitStage`, `HelpClose`) |
| `name` | config name, equal to the variant name |
| `context` | `KeyContext` it applies in |
| `label` | short function-bar label (≤ 8 chars, e.g. "Copy") |
| `description` | help-screen text |
| `group` | help-screen group ("Navigation", "Operations", …) |
| `bar_priority` | `Option<u8>`; `None` = not in function bar, lower = shown first |

Validation: every action bound in the shipped defaults has a catalog entry for that context, and
every catalog entry with `bar_priority` has at least one default binding (tested).

New `Action` variants needed: `GitPageUp`, `GitPageDown`, `HelpClose`, `HelpScrollUp`,
`HelpScrollDown`. `Help` now opens the help screen. `QuickCdChar`, `DialogInputChar`, `FilterChar`
stay internal (not bindable).

## Binding

| Field | Meaning |
|-------|---------|
| `context` | `KeyContext` |
| `keys` | `KeySeq` |
| `action` | catalog action, or removed (`null`) |
| `source` | `Default` or `User` |
| `raw` | key string as written, for messages |

Merge: start from defaults; for each user entry, replace or remove the binding for the same
`(context, keys)`. Result per context: `HashMap<KeySeq, Action>` plus a reverse map
`Action → Vec<KeySeq>` (ordered: F-key first, then others in source order) for display.

## BindingProblem

| Field | Meaning |
|-------|---------|
| `kind` | `UnparseableKey`, `UnknownAction`, `NotBindableHere`, `PrefixAmbiguity`, `Unreachable`, `DuplicateKey`, `TextInputSteal`, `QuitUnreachable`, `ConfigUnreadable` |
| `context` | `KeyContext` (if any) |
| `keys` | raw key string(s) involved |
| `actions` | action name(s) involved |
| `resolution` | what was done: "entry skipped", "single key wins", "later entry wins", "defaults used" |

Rendered as one line each in the startup ErrorList dialog and logged with `warn!`.

## Keymap (resolved)

| Field | Meaning |
|-------|---------|
| `bindings` | `HashMap<KeyContext, HashMap<KeySeq, Action>>` |
| `keys_for` | `HashMap<(KeyContext, Action), Vec<KeySeq>>` |
| `problems` | `Vec<BindingProblem>` |

Built once at startup from `Config`; replaces `Config.keybindings` lookups in `App`.

## State transitions

- Help: any non-text context + `Help` → `Mode::Help` (remember prior mode) → `HelpClose` (Esc, `?`)
  → prior mode.
- Pending sequence: key that is a strict prefix of a binding is buffered
  (`last_tick_key_events`); next key completes, or the buffer is cleared and the key is handled
  alone. Unchanged from today, now for every context.
