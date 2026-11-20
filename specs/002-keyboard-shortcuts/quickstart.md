# Quickstart: Validating the Keyboard Shortcut Overhaul

## Prerequisites

- Built from this feature branch on the pinned nightly: `cargo build`.
- A throwaway config dir so your real config is untouched:
  `export CRABMANDER_CONFIG=$(mktemp -d)`.
- For R5 checks: at least one kitty-protocol terminal (kitty, foot, WezTerm, Alacritty ≥ 0.13) and
  one without it (e.g. xterm), and both a US and a non-US (e.g. Swedish) keyboard layout.

## 1. Automated checks

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked --all-features keymap      # parser, merge, problems, defaults conflict-free
cargo test --locked --all-features func_bar    # bar fitting and key labels
```

Expected: all pass; the "defaults have no problems" test covers FR-006.

## 2. Fallbacks without F-keys (Story 1)

1. Start `cargo run` with an empty config dir.
2. Without touching any F-key, use `Alt+1` … `Alt+0`, `Alt+Minus`, `Alt+=`, and
   `Alt+Shift+1/2/6/9`. Each must do what the matching F-key does (contract defaults table).
3. In Git mode (`Ctrl+G` inside a repo) use `Alt+1` … `Alt+7`.
4. Type `Y` in Normal mode: the filter starts with `Y` (no copy dialog).

## 3. Rebinding and removal (Story 2)

Put this in `$CRABMANDER_CONFIG/config.json5`:

```json5
{ "keybindings": {
  "Normal": { "<ctrl-y>": "Copy", "<q>": null },
  "Git":    { "<x>": "GitStage" },
  "Cargo":  { "<r>": "CargoRunSelected" },
} }
```

Expected: `Ctrl+Y` copies, `q` starts a filter instead of quitting, `x` stages in Git mode, `r` runs
in Cargo mode, everything else unchanged.

## 4. Problem report (Story 3)

Replace the config with one entry per problem kind from
[contracts/ui-hints.md](contracts/ui-hints.md#startup-problem-report) (bad key, unknown action,
`<g>` plus `<g><g>`, removing every key for Rename, a plain letter in `GitCommit`).
Expected: the app starts, shows "Keybinding problems (5)" with one line each, and the log contains the
same lines.

## 5. Hints match reality (Story 4)

With the config from step 3: the function bar shows `^Y` for Copy in Normal mode and `x` for Stage
in Git mode; `?` opens help listing every action of the current mode with its keys; `Esc` closes it.
Typing `?` in the commit editor inserts `?`. Shrink the terminal to 60 columns: entries drop from the
end, none are cut.

## 6. Terminal and layout matrix (R5)

Run with `RUST_LOG=trace` and check the log for the received key events:

| Terminal | Layout | Alt+5 | Alt+Shift+6 | Alt+Minus | Alt+= |
|----------|--------|-------|-------------|-----------|-------|
| kitty-protocol terminal | US | Copy | Rename | Theme | Git repos |
| kitty-protocol terminal | Swedish | Copy | Rename | record result | record result |
| legacy terminal | US | Copy | not available, help shows note | Theme | Git repos |

Record actual results in the PR description.
