# Contract: Keybinding Configuration

User-facing contract for the `keybindings` section of the Crabmander config file
(`$CRABMANDER_CONFIG` or the XDG config dir; json5, json, yaml, toml or ini).

## Shape

```json5
{
  "keybindings": {
    "<Context>": {
      "<key sequence>": "<ActionName>",   // bind or override
      "<key sequence>": null               // remove a default binding
    }
  }
}
```

- `<Context>`: one of `Normal`, `Git`, `GitCommit`, `GitBranch`, `GitTag`, `GlobalGit`, `Cargo`,
  `Dialog`, `DialogConfirm`, `QuickCd`, `Help`. Unknown contexts are reported and ignored.
- `<ActionName>`: a bindable action name for that context (see defaults below). `"Unbind"` is accepted
  as an alias for `null` in formats without null (INI).
- Sections and entries not mentioned keep their built-in defaults.

## Key sequence notation (unchanged, extended)

- One key: `<name>` with optional modifiers `ctrl-`, `alt-`, `shift-` in any order:
  `<f5>`, `<shift-f6>`, `<alt-5>`, `<alt-shift-6>`, `<ctrl-b>`.
- Several keys: concatenated, `<g><g>`.
- Names: `esc enter left right up down home end pageup pagedown tab backtab backspace delete
  insert space f1`…`f12 hyphen minus equals` (**`equals` is new**), or any single character
  (`<?>`, `<*>`, `<=>`).
- Shift + letter may be written `<shift-y>` or `<Y>`; both mean the same key.
- Matching is case-sensitive for letters and ignores key repeat/release.

## Compatibility guarantees

- Every config that loads today loads with the same result, except that bindings to the removed
  default Shift-letter fallbacks are now only present if the user's config contains them.
- `Ctrl+C` always quits and cannot be rebound.
- Problems never stop startup; see [ui-hints.md](ui-hints.md#startup-problem-report).

## Default bindings (shipped)

Normal (F-key, then fallbacks). Every F-key action also has a leader sequence that works in any
terminal: `Ctrl+F` then the same digit (`-` for F11, `=` for F12), or `Ctrl+F`, `s`, digit for the
Shift+F-key ones. Config notation: `<ctrl-f><5>`, `<ctrl-f><s><6>`. These are not repeated in the
table.

| Action | Keys |
|--------|------|
| QuickCd | `F1`, `Alt+1` |
| RecentDirs | `Shift+F1`, `Alt+Shift+1` |
| ContextMenu | `F2`, `Alt+2` |
| UserCommands | `Shift+F2`, `Alt+Shift+2` |
| View | `F3`, `Alt+3` |
| CalcSizes | `F4`, `Alt+4` |
| Copy | `F5`, `Alt+5` |
| Move | `F6`, `Alt+6` |
| Rename | `Shift+F6`, `Alt+Shift+6` |
| Mkdir | `F7`, `Alt+7` |
| Delete | `F8`, `Alt+8` |
| CycleSortMode | `F9`, `Alt+9` |
| InvertSort | `Shift+F9`, `Alt+Shift+9` |
| Quit | `F10`, `Alt+0`, `q`, `Ctrl+C` |
| SelectTheme | `F11`, `Alt+Minus` |
| GlobalGitView | `F12`, `Alt+=` |
| Help | `?` |
| EnterGitMode | `Ctrl+G` (was hardcoded) |
| EnterCargoMode | `Ctrl+B` (was hardcoded) |
| ToggleMark | `Space`, `Insert` |
| ToggleMarkAll | `*` (documented before, now actually bound) |
| Suspend | `Ctrl+Z` |
| Navigation | unchanged (`↑/k`, `↓/j`, PgUp, PgDn, Home, End, Enter/→, Backspace/←, Tab, Shift+Tab) |

Removed from defaults: `Shift+X`, `Shift+E`, `Shift+Z`, `Shift+Y`, `Shift+R`, `Shift+K`, `Shift+D`,
`Shift+S`, `Shift+T`, `Shift+G`.

Git: today's hardcoded keys move to config unchanged, plus `Alt+1`…`Alt+7` and `Ctrl+F 1`…`7`
mirroring `F1`…`F7`,
`?` Help, and `GitPageUp`/`GitPageDown` on PgUp/PgDn.

GitBranch, GitTag, GlobalGit, Cargo: today's hardcoded keys move to config unchanged, plus `?` Help.

GitCommit: `Esc` GitCommitCancel, `Ctrl+Enter` and `Alt+Enter` GitCommitSubmit. All other keys go to
the editor.

Dialog: `Enter` DialogConfirm, `Esc` DialogCancel, `↑` DialogNavUp, `↓` DialogNavDown, `Delete`
DialogDeleteSelected, `Backspace` DialogInputBackspace. DialogConfirm: `y`/`Y` DialogConfirm,
`n`/`N` DialogCancel. QuickCd: `Tab` QuickCdComplete, `Backspace` QuickCdBackspace.

Help: `Esc` and `?` HelpClose, `↑/k` HelpScrollUp, `↓/j` HelpScrollDown, PgUp/PgDn page.

## Restoring the old Shift-letter fallbacks

```json5
"keybindings": { "Normal": {
  "<shift-x>": "ContextMenu", "<shift-e>": "View", "<shift-z>": "CalcSizes",
  "<shift-y>": "Copy", "<shift-r>": "Move", "<shift-k>": "Mkdir", "<shift-d>": "Delete",
  "<shift-s>": "CycleSortMode", "<shift-t>": "SelectTheme", "<shift-g>": "GlobalGitView",
} }
```
