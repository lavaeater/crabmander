# Contract: Function Bar, Help Screen and Startup Report

## Function bar

- Content comes from the resolved keymap of the active context, never from fixed labels.
- Entries appear in catalog priority order. Each shows up to two keys in effect: the F-key binding
  first (if any), then the first other binding, joined with `/`: `F5/Alt+5 Copy`.
- Compact key names: `F5`, `⇧F6`, `Alt+5`, `Alt+⇧6`, `^B`, `Esc`, `Spc`, `Tab`, `PgUp`.
- An action with no key in effect is not shown.
- When the width is too small, lowest-priority entries are dropped whole; an entry is never cut in
  the middle. The last slot shows `? Help` whenever Help has a key and there is room for it.

## Help screen

- Opens with the `Help` action (`?` by default) from any non-text context; overlays the current view.
- Title names the context ("Keys — Git mode").
- Lists every catalog action of the active context (and its fallback contexts), grouped by help
  group, each with its description and **all** keys in effect in full notation
  (`F5, Alt+5`). Actions with no key show "(unbound)".
- If the terminal does not support the kitty keyboard protocol, a footer line says:
  "This terminal cannot report Alt+Shift+digit; see README to bind symbols instead."
- Scrolls with HelpScrollUp/Down and page keys; closes with HelpClose (`Esc`, `?`), returning to the
  previous mode unchanged.

## Startup problem report

- Shown once after startup, before any other dialog, only if there are problems.
- Uses the existing error-list dialog. Title: "Keybinding problems (N)".
- One line per problem: `[Context] <keys> → Action: what is wrong — what Crabmander did`.
  Examples:
  - `[Normal] <alt-x5> → Copy: unparseable key — entry skipped`
  - `[Git] <ctrl-q> → GitPushh: unknown action — entry skipped`
  - `[Normal] <g> → NavTop vs <g><g> → NavBottom: <g> is also a prefix — single key wins`
  - `[Normal] Rename: no key left after overrides — action unreachable`
  - `[GitCommit] <a> → GitCommitSubmit: would steal typed text — entry skipped`
- Every line is also written to the log at warn level.
- Dismissed with Enter or Esc; the app then works normally.
