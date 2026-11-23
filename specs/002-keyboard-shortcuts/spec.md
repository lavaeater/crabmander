# Feature Specification: Keyboard Shortcut Overhaul

**Feature Branch**: `002-keyboard-shortcuts`

**Created**: 2026-11-20

**Status**: Implemented

**Input**: User description: "Keyboard shortcut overhaul: make every Crabmander shortcut reliable,
configurable and discoverable. Today only Normal mode keybindings live in the config file; Git, Git
Commit, Git Branch, Git Tag, Global Git (repos), Cargo and Dialog modes have their keys hardcoded, so
users cannot rebind them. Some terminals and desktop compositors (e.g. COSMIC) intercept function
keys, and Shift-letter fallbacks exist for only some F-key actions (none for Shift+F1 Recent dirs,
Shift+F2 User commands, Shift+F6 Rename, Shift+F9 Invert sort), and the fallback letters are
arbitrary (Shift+Y copy, Shift+R move, Shift+K mkdir). Goals: all modes configurable from the same
keybinding config with built-in defaults; every F-key action has a working non-F-key alternative;
detect and report conflicting bindings (same key bound twice in a mode, or a user override shadowing
something) at startup instead of silently picking one; the function bar and a help screen show the
keys actually in effect (including user overrides and fallbacks), not hardcoded labels; keep
Commander conventions (F5 copy, F6 move, F7 mkdir, F8 delete, F10 quit, Tab switch panel). The kitty
keyboard protocol (disambiguate escape codes) is already enabled where supported. Out of scope:
embedding a terminal multiplexer."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every F-key action works without F-keys (Priority: P1)

A user whose terminal or desktop swallows function keys (for example the COSMIC desktop) can still
reach every action Crabmander offers on an F-key, including the Shift+F-key ones, using an
alternative key that follows one consistent, easy-to-remember scheme.

Fallback scheme: Alt plus the digit key matching the F-key number replaces F-keys (Alt+1 = F1 …
Alt+9 = F9, Alt+0 = F10), and Alt+Shift plus the same digit replaces Shift+F-keys (Alt+Shift+6 =
Shift+F6). F11 and F12 use the two keys right of 0 on the number row, Alt+Minus and Alt+Equals. Today's
arbitrary Shift-letter fallbacks (Shift+X, E, Z, Y, R, K, D, S, T, G) are removed from the defaults.

**Why this priority**: Users on affected desktops currently cannot reach Recent dirs, User
commands, Rename or Invert sort at all. That is a hard blocker; everything else is improvement.

**Independent Test**: With all F-keys disabled in the terminal, perform every action listed in the
function bar of every mode using only the alternative keys.

**Acceptance Scenarios**:

1. **Given** a terminal that never delivers F-keys, **When** the user presses the alternative key for
   any F-key or Shift+F-key action in any mode, **Then** that action runs exactly as with the F-key.
2. **Given** the default bindings, **When** a list is made of every action bound to an F-key in any
   mode, **Then** each of those actions also has at least one non-F-key binding.
3. **Given** the default bindings, **When** the user presses F5, F6, F7, F8, F10 or Tab in Normal
   mode, **Then** they still copy, move, make a directory, delete, quit and switch panels.
4. **Given** the default bindings in Normal mode, **When** the user types a capital letter that used
   to be a fallback (e.g. Shift+Y), **Then** it starts or extends the filter like any other printable
   character.
5. **Given** a keyboard layout where Shift+digit types a symbol (e.g. Swedish, where Shift+6 is `&`),
   **When** the user presses Alt+Shift+6, **Then** the Shift+F6 action runs.

---

### User Story 2 - Rebind keys in every mode (Priority: P1)

A user can change, add or remove the keys for any action in any mode (Normal, Git, Git Commit, Git
Branch, Git Tag, Global Git, Cargo and dialogs) from the same keybinding config they already use for
Normal mode. Anything they don't override keeps its built-in default.

**Why this priority**: Without this, any key that clashes with the user's terminal, desktop or habits
in a non-Normal mode is unfixable. It is also the foundation for accurate on-screen key hints.

**Independent Test**: In the user config, rebind one action in each mode and remove one default
binding; restart and confirm the new keys work, the removed key no longer does anything, and all
other defaults are unchanged.

**Acceptance Scenarios**:

1. **Given** a user config that binds a new key to a Git mode action, **When** the user presses it in
   Git mode, **Then** the action runs; the action's default key also still works unless removed.
2. **Given** a user config that removes a default binding, **When** the user presses that key in that
   mode, **Then** nothing happens (or, in Normal mode, a printable key starts the filter as it would
   for any unbound printable key).
3. **Given** an existing user config written for the current format, **When** Crabmander starts,
   **Then** it loads and behaves exactly as before.
4. **Given** the Git Commit editor or a text input dialog, **When** the user types printable
   characters, **Then** they are inserted as text; only non-text keys (submit, cancel, navigation
   between fields) are rebindable there.

---

### User Story 3 - Conflicts and mistakes are reported, not silently resolved (Priority: P2)

When the keybinding config contains problems, Crabmander tells the user at startup exactly what is
wrong and what it did about it, instead of silently picking one binding or failing to start.

**Why this priority**: As soon as every mode is configurable (Story 2), mistakes become likely.
Silent resolution makes them very hard to diagnose.

**Independent Test**: Put each kind of problem listed in FR-008 into a user config, start Crabmander,
and confirm each is reported with mode, key and action(s), and that the app still starts.

**Acceptance Scenarios**:

1. **Given** a user config where a key sequence is both a complete binding and the start of a longer
   binding in the same mode, **When** Crabmander starts, **Then** it reports the ambiguity, naming
   the mode, both sequences and both actions.
2. **Given** a user override that leaves a default action with no key at all, **When** Crabmander
   starts, **Then** it reports which action became unreachable and in which mode.
3. **Given** a config entry with an unknown action name or an unparseable key, **When** Crabmander
   starts, **Then** that entry is skipped, the problem is reported, and all valid entries apply.
4. **Given** problems were reported, **When** the user dismisses the report, **Then** Crabmander works
   normally and the same report is also written to the log.

---

### User Story 4 - On-screen hints show the keys actually in effect (Priority: P2)

The function bar and a help screen always show the keys that really trigger each action right now,
including the user's overrides and the fallback keys, for whichever mode is active.

Help screen key: `?` opens the help screen in every mode except text-input contexts. F1 stays Quick
CD. In Normal mode `?` therefore no longer starts a filter.

**Why this priority**: Wrong hints are worse than no hints, and they become wrong as soon as Story 2
lets users rebind. It also makes the new fallback keys discoverable.

**Independent Test**: Rebind three actions in different modes, then open the help screen and look at
the function bar in each mode; confirm every shown key matches the config and pressing it triggers
the action named next to it.

**Acceptance Scenarios**:

1. **Given** the user rebinds Copy in Normal mode, **When** the function bar is shown, **Then** it
   shows the new key for Copy and no longer shows a removed one.
2. **Given** any mode is active, **When** the user opens the help screen, **Then** it lists every
   action available in that mode with all its keys, grouped the same way as the README tables.
3. **Given** the help screen is open, **When** the user presses Esc or `?` again, **Then** it closes
   and the previous mode resumes unchanged.
4. **Given** the Git Commit editor or a dialog text field has focus, **When** the user types `?`,
   **Then** a `?` is inserted as text and the help screen does not open.
5. **Given** the terminal is too narrow for every hint, **When** the function bar is drawn, **Then**
   it shows the most important actions first and never shows a key that does not work.

---

### Edge Cases

- A user binds the same key to two different actions in the same mode in one config file (the
  config format may only keep one of them; the user must still be told).
- A user rebinds a key that the active mode treats as text input (letters in the commit editor).
- A user removes every binding for Quit: Crabmander must warn, and Ctrl+C must still exit.
- Multi-key sequences (e.g. `<g><g>`) where the user pauses part-way, or presses a key that matches
  no sequence.
- A terminal without the kitty keyboard protocol sends the same code for two keys (e.g. Tab and
  Ctrl+I, Esc-prefixed Alt combinations); a binding the terminal cannot tell apart must be reported
  as such in the help screen rather than appearing to be broken.
- Config file missing, empty or malformed: built-in defaults apply and the problem is reported.
- A key shown in the function bar is wider than the space available (long modifier combinations).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every mode (Normal, Git, Git Commit, Git Branch, Git Tag, Global Git, Cargo and every
  dialog type) MUST take its non-text key bindings from the keybinding config, with built-in
  defaults for every action.
- **FR-002**: User bindings MUST be merged with the built-in defaults per mode: a user entry for a
  key replaces the default for that key; defaults for keys the user did not mention stay in effect.
- **FR-003**: Users MUST be able to remove a default binding for a key in a mode.
- **FR-004**: Every action that has an F-key or Shift+F-key binding by default MUST also have a
  default binding following the Alt+digit scheme: Fn → Alt+digit n (F10 → Alt+0, F11 → Alt+Minus,
  F12 → Alt+Equals) and Shift+Fn → Alt+Shift+ the same key. This applies in every mode that uses
  F-keys.
- **FR-004a**: Alt+Shift+digit bindings MUST match the physical digit key, whatever character
  Shift+digit produces on the user's keyboard layout.
- **FR-004c**: Every such action MUST also be reachable by a leader sequence that works in every
  terminal and layout: Ctrl+F then the digit (or `-`, `=`) for Fn, and Ctrl+F, `s`, then the digit
  for Shift+Fn (e.g. Ctrl+F 5 = F5, Ctrl+F s 6 = Shift+F6). Added 2026-11-20 after the Tilix spike
  showed terminals that capture Alt+digit.
- **FR-004b**: The previous Shift-letter fallbacks (Shift+X, E, Z, Y, R, K, D, S, T, G in Normal mode)
  MUST be removed from the built-in defaults. A user config that binds them explicitly MUST keep
  working.
- **FR-005**: The defaults MUST keep F5 Copy, F6 Move, F7 MkDir, F8 Delete, F10 Quit and Tab Switch
  panel in Normal mode.
- **FR-006**: The built-in defaults MUST contain no conflicts as defined in FR-008, verified by an
  automated test across all modes.
- **FR-007**: Configs valid under the current format MUST keep loading with the same behaviour.
- **FR-008**: At startup Crabmander MUST detect and report: (a) a key sequence that is both a full
  binding and a prefix of a longer one in the same mode; (b) an action that has a default binding
  but no key at all after user overrides; (c) unknown action names; (d) unparseable key sequences;
  (e) the same key bound to more than one action in the same mode where the config makes this
  detectable.
- **FR-009**: Config problems MUST NOT prevent startup. Invalid entries are skipped, valid ones
  apply, and the problems are shown in one dismissible report and written to the log.
- **FR-010**: Ctrl+C MUST always quit, regardless of configuration.
- **FR-011**: The function bar MUST be generated from the bindings in effect for the active mode,
  including user overrides and fallback keys.
- **FR-012**: A help screen MUST list every action of the active mode with all keys in effect. It
  MUST open with `?` by default (rebindable like any action) in every mode outside text-input
  contexts, and close with Esc or `?`. F1 MUST remain Quick CD.
- **FR-013**: In text-input contexts (commit message editor, dialog text fields, Normal-mode filter
  typing) printable characters MUST be treated as text and MUST NOT be stolen by bindings unless the
  binding uses a modifier other than Shift.
- **FR-014**: The README keyboard reference MUST document the Alt+digit scheme, the config format for
  every mode, how to remove a binding, the help key, and that the old Shift-letter fallbacks were
  removed (with the config lines to restore them).

### Key Entities

- **Mode**: A context with its own set of active bindings (Normal, Git, Git Commit, Git Branch, Git
  Tag, Global Git, Cargo, each dialog type).
- **Action**: A named command a binding triggers; has a human-readable label used by the function bar
  and help screen, and a priority used when space is short.
- **Binding**: A key sequence (one or more key presses, with modifiers) mapped to an action within a
  mode; comes from either the built-in defaults or the user config.
- **Binding problem**: A detected conflict or config error, with its kind, mode, key sequence(s),
  action(s) and what Crabmander did about it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of actions reachable by an F-key are also reachable without any F-key, in every
  mode, using only default bindings.
- **SC-002**: 100% of actions in every mode can be rebound from the config file without rebuilding
  Crabmander.
- **SC-003**: For every key shown in the function bar or help screen, pressing it triggers the action
  shown next to it (0 stale or wrong hints), checked against a config with at least one override per
  mode.
- **SC-004**: Every config problem of the kinds in FR-008 is reported at startup with its mode, key
  and action; 0 are resolved silently.
- **SC-005**: A user who has never used Crabmander can find the key for any action in under 30
  seconds using only the help screen.
- **SC-006**: All existing user configs that load today still load and produce the same bindings.

## Assumptions

- The existing keybinding config file and its key notation (e.g. `<ctrl-a>`, `<shift-f5>`, `<g><g>`)
  are extended, not replaced; new per-mode sections use the same notation.
- Removing a binding uses a value in the config that means "nothing" (exact syntax is a planning
  decision).
- Removing the Shift-letter fallbacks is an intentional breaking change for muscle memory, accepted
  by the project owner (2026-11-20).
- Telling the physical digit key apart from the character it produces (FR-004a) depends on the
  terminal reporting it; terminals with the kitty keyboard protocol can. Planning must confirm
  behaviour on terminals without it and on Nordic layouts, and document any limits in the help
  screen.
- Alt combinations may be captured by some desktops or terminals (Tilix, GNOME Terminal and Konsole
  use Alt+digit for tabs/panes); the Ctrl+F leader sequences (FR-004c) cover those, and every
  fallback is rebindable.
- Keys that the terminal emulator or desktop never delivers cannot be detected by Crabmander; the
  fallback scheme (FR-004) is the mitigation. A "show me which key you pressed" diagnostic is out of
  scope.
- Theme and style configuration are unchanged by this feature.
- The context menu (F2) items and user commands (Shift+F2) are lists the user picks from; their
  per-item shortcuts, if any, are out of scope.
- Embedding or integrating a terminal multiplexer is out of scope (see the parked
  `specs/001-zellij-workspace` draft).
