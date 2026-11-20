# Feature Specification: Bundled Zellij Workspace

**Feature Branch**: `speccy`

**Created**: 2026-11-20

**Status**: Draft

**Input**: User description: "Add ZelliJ dependency to use as our actual terminal for more control over
keyboard shortcuts etc. ZelliJ is a Rust-made terminal multiplexer / terminal that might be a perfect
fit for us. See for example https://zellij.dev/documentation/distributions.html and
https://github.com/zellij-org/zellij/tree/main/example/distribution"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Launch Crabmander in its own workspace (Priority: P1)

A user starts Crabmander and it opens inside a dedicated Crabmander workspace (a bundled terminal
multiplexer session) instead of running bare in whatever terminal they launched it from. The file
manager looks and behaves exactly as it does today, and every Crabmander shortcut (function keys,
Tab, Ctrl/Shift combinations) reaches Crabmander rather than being swallowed by the multiplexer.
Quitting Crabmander ends the workspace and returns the user to where they started.

The launch behaviour is: [NEEDS CLARIFICATION: Is the workspace always used, the default with an
opt-out (e.g. a `--no-workspace` flag or config setting), or opt-in only (e.g. a `--workspace`
flag)?]

**Why this priority**: Everything else in this feature builds on Crabmander running inside a
workspace it controls. On its own it already delivers a consistent, self-contained environment with
known keyboard behaviour, regardless of the user's own multiplexer setup.

**Independent Test**: Launch Crabmander from a plain terminal, exercise every key in the README
keyboard reference, confirm each performs its documented action, then quit and confirm the user is
back at their original shell with no leftover session.

**Acceptance Scenarios**:

1. **Given** a user in a plain terminal, **When** they launch Crabmander in workspace mode, **Then**
   the twin-pane file manager is shown full-screen with no visible multiplexer chrome other than
   what Crabmander's layout defines.
2. **Given** Crabmander is running in its workspace, **When** the user presses any key listed in the
   README keyboard reference, **Then** Crabmander performs the documented action and the multiplexer
   performs no action of its own.
3. **Given** Crabmander is running in its workspace, **When** the user quits Crabmander (F10),
   **Then** the workspace closes and the user returns to their original shell.
4. **Given** the user already uses the same multiplexer with their own configuration, **When** they
   launch Crabmander, **Then** their own sessions, layouts and keybindings are neither changed nor
   used by the Crabmander workspace.

---

### User Story 2 - Run programs beside the file manager instead of suspending it (Priority: P2)

Today, opening a file in an editor (F3), running a user command (Shift+F2) or running a cargo
command suspends Crabmander, hands the whole screen to the program, and asks the user to press Enter
to come back. Inside the workspace, these programs instead open in their own pane so the file
manager stays visible and usable.

Placement of these programs: [NEEDS CLARIFICATION: Should they open in a floating pane over the file
manager, a tiled split pane beside/below it, or should each action keep today's suspend behaviour
with panes only as an opt-in per command?]

**Why this priority**: This is the main new capability a multiplexer gives a file manager, but it
only makes sense once User Story 1 works.

**Independent Test**: In workspace mode, open a file with F3, run a user command and run a cargo
command; confirm each runs in its own pane, the file manager remains visible and navigable, and
closing the pane returns focus to the file manager.

**Acceptance Scenarios**:

1. **Given** Crabmander in workspace mode, **When** the user opens a file in the editor, **Then** the
   editor opens in its own pane and the file manager is not suspended.
2. **Given** a command is running in a pane, **When** it finishes, **Then** its output stays visible
   until the user dismisses the pane, and the affected file panels are reloaded.
3. **Given** a program is running in a pane, **When** the user switches focus back to the file
   manager, **Then** the file manager responds to keys while the program keeps running.
4. **Given** Crabmander is not running in workspace mode, **When** the user triggers the same
   actions, **Then** today's suspend-and-resume behaviour is unchanged.

---

### User Story 3 - Workspace keyboard shortcuts that never clash (Priority: P3)

The user can manage workspace panes (switch focus between panes, close a pane, toggle a floating
pane) from the keyboard without any of those shortcuts taking a key that Crabmander already uses.

Shortcut policy: [NEEDS CLARIFICATION: Should all multiplexer shortcuts be disabled except a small
fixed set chosen by Crabmander (e.g. one leader key), should the multiplexer's defaults be kept
minus the ones that clash with Crabmander, or should the user be able to define them in
Crabmander's own keybinding config?]

**Why this priority**: Needed to make User Story 2 comfortable, but basic use works with a minimal
fixed set of pane keys.

**Independent Test**: With one program pane open, use only the documented workspace shortcuts to
move focus between panes and close the pane; confirm none of them triggers a Crabmander action and
no Crabmander key triggers a workspace action.

**Acceptance Scenarios**:

1. **Given** the workspace with two panes, **When** the user presses the documented focus-switch
   shortcut, **Then** focus moves to the other pane.
2. **Given** the shipped shortcut set, **When** it is compared with Crabmander's default keybindings
   for every mode, **Then** there is no key sequence assigned to both.

---

### Edge Cases

- Crabmander is launched from inside an existing session of the same multiplexer (nested sessions).
- Crabmander is launched from inside a different multiplexer (tmux, screen) or a terminal that does
  not report some keys at all (some terminals capture F11/F12 or Ctrl combinations themselves).
- The terminal is resized, including to very small sizes, while panes are open.
- Crabmander crashes or panics while running inside the workspace: the terminal must be restored and
  the workspace must not linger.
- Two Crabmander workspaces are started at the same time.
- The user quits Crabmander while a program is still running in a pane.
- `--install-desktop-entry` and other CLI flags are used together with workspace mode.
- The user's config and data directories already contain Crabmander's own settings; workspace
  settings must not overwrite or misread them.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Crabmander MUST be able to start inside its own bundled terminal-multiplexer workspace
  that is shipped as part of the single installed program (no separate install step).
- **FR-002**: The workspace MUST pass every key sequence in Crabmander's default keybindings, for
  every mode, through to Crabmander unchanged.
- **FR-003**: The workspace MUST be isolated from any separately installed copy of the same
  multiplexer: it MUST NOT read, change or attach to the user's own sessions, config or layouts.
- **FR-004**: Workspace settings (layout, shortcuts, theme) MUST be stored with Crabmander's own
  settings and follow the existing configuration rules (XDG locations, `$CRABMANDER_CONFIG`
  override, fall back to built-in defaults when missing or malformed).
- **FR-005**: Quitting Crabmander MUST close the workspace and restore the user's terminal; a crash
  inside the workspace MUST also restore the terminal.
- **FR-006**: In workspace mode, opening a file in the editor, running user commands and running
  cargo commands MUST run in a separate pane without suspending the file manager.
- **FR-007**: When a pane program that can change files finishes, Crabmander MUST reload the file
  panels it would reload today after the same action.
- **FR-008**: Users MUST be able to move focus between the file manager and program panes, and close
  program panes, using the keyboard only.
- **FR-009**: Workspace shortcuts MUST be documented in the README keyboard reference and MUST appear
  in the function bar or help when a program pane is open.
- **FR-010**: Crabmander MUST keep working without the workspace (current behaviour), and every
  existing feature MUST behave identically in that case.
- **FR-011**: When launched from inside a session of the same multiplexer, Crabmander MUST NOT nest a
  second workspace; it MUST fall back to running without the workspace and say so in the status bar.
- **FR-012**: The bundled workspace MUST support the same platforms Crabmander supports today (Linux
  primary; macOS SHOULD keep working).

### Key Entities

- **Workspace**: The bundled multiplexer session that hosts Crabmander. Has a layout, a shortcut set
  and a lifetime tied to the Crabmander process.
- **Program pane**: A pane running an external program launched by a Crabmander action. Knows which
  file panels to reload when it finishes.
- **Workspace shortcut set**: The keys the workspace itself reacts to; must be disjoint from
  Crabmander's keybindings.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of the key sequences in Crabmander's default keybindings perform their documented
  Crabmander action when running in the workspace.
- **SC-002**: Starting Crabmander in the workspace takes no more than 1 second longer than starting
  it without the workspace on the same machine.
- **SC-003**: A user can open a file in the editor, edit it, and get back to browsing with the file
  manager visible throughout, using at most 2 keypresses beyond those for the edit itself.
- **SC-004**: After quitting (normally or by crash), 0 workspace sessions or processes belonging to
  Crabmander remain running.
- **SC-005**: A user with their own configuration of the same multiplexer sees no change to their
  own sessions or settings after using Crabmander.
- **SC-006**: Installing Crabmander remains a single install command, with no extra programs to
  install for the workspace.

## Assumptions

- The workspace is built as a Zellij "distribution": Zellij is linked into the Crabmander program as
  a library, with Crabmander's own config, layout and shortcut presets bundled in. This gives
  Crabmander its own config folders, separate from a stock Zellij install.
- The multiplexer still runs inside the user's terminal emulator. Keys that the terminal emulator
  itself captures (for example some terminals' own use of F11 or Ctrl+Shift combinations) still
  never reach Crabmander; this feature controls the multiplexer layer, not the terminal emulator.
- The multiplexer's distribution-name config folder (`~/.config/crabmander`) is the same folder
  Crabmander already uses. Workspace files must live alongside Crabmander's existing config without
  clashing; the exact file names are a planning concern.
- Session sharing, session resurrection and multiplexer plugins beyond what the layout needs are out
  of scope for this feature.
- Windows support is out of scope (it is not supported today either).
- The added dependency noticeably increases build time and binary size; this is accepted for this
  feature, and the plan should measure it.
