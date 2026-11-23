<!--
Sync Impact Report
==================
Version change: (unversioned template) → 1.0.0
Modified principles: n/a (initial ratification)
  - [PRINCIPLE_1_NAME] → I. Keyboard-First Commander Experience
  - [PRINCIPLE_2_NAME] → II. Responsive, Non-Blocking UI
  - [PRINCIPLE_3_NAME] → III. Safe File Operations
  - [PRINCIPLE_4_NAME] → IV. Action-Driven Component Architecture
  - [PRINCIPLE_5_NAME] → V. Quality Gates (NON-NEGOTIABLE)
Added sections:
  - Technical Constraints (was [SECTION_2_NAME])
  - Development Workflow (was [SECTION_3_NAME])
  - Governance
Removed sections: none
Follow-up TODOs: none. Ratification date set to the date of first adoption (2026-11-20).
-->

# Crabmander Constitution

## Core Principles

### I. Keyboard-First Commander Experience

Crabmander is a twin-pane file manager in the Norton/Midnight Commander tradition.

- Every user-facing feature MUST be fully operable from the keyboard; mouse support is optional
  and additive.
- Keybindings MUST be declared per `Mode` in configuration (`.config/config.json5` defaults,
  overridable from the user config dir), not hardcoded in component logic, unless the key is
  inherently contextual (e.g. typed filter text, text-area input).
- New features MUST surface their keys in the function bar and/or context menu (F2) and be
  documented in `README.md` keyboard reference tables.
- Established Commander conventions (F5 Copy, F6 Move, F7 MkDir, F8 Delete, F10 Quit, Tab to
  switch panels) MUST NOT be repurposed.

Rationale: muscle memory is the product. Discoverability and configurability keep it usable.

### II. Responsive, Non-Blocking UI

- The render loop MUST never block on filesystem, git, network, or subprocess I/O whose duration
  is unbounded (recursive copy/delete, size calculation, archive work, git push/fetch, etc.).
- Long-running work MUST run on Tokio tasks or blocking pools and report back via `Action`s,
  showing progress or a busy indicator when it can exceed ~200 ms.
- Running external programs that need the terminal (editors, pagers, user commands) MUST
  suspend and restore the TUI cleanly via `Suspend`/`Resume`.

Rationale: a file manager that freezes on a large directory is not trustworthy.

### III. Safe File Operations

- Destructive or overwriting operations (delete, move/rename over existing targets, overwrite on
  copy, git operations that discard work) MUST require explicit confirmation, implemented via the
  `DeferredOp` dialog flow.
- Operations MUST act on marked entries when any exist, otherwise on the cursor entry, and the
  confirmation MUST state what will be affected.
- Failures MUST be surfaced to the user (dialog or status bar) and logged via `tracing`;
  errors MUST NOT be silently swallowed and the app MUST NOT panic on I/O errors.
- Partial failures in bulk operations MUST report which entries failed.

Rationale: user data loss is the single unrecoverable failure mode for this application.

### IV. Action-Driven Component Architecture

- All state changes MUST flow through the `Action` enum and the `App` event loop; components
  communicate by emitting `Action`s, not by holding references to each other.
- New UI surfaces MUST be implemented as `Component`s (registered in `App::new()`) or as
  sub-views owned by an existing component, and scoped by `Mode` where they change key handling.
- New confirmable/queued operations MUST implement `DeferredOp` rather than adding ad-hoc
  dialog plumbing.
- Logic that does not need a terminal (path handling, parsing, sorting, persistence, git queries)
  SHOULD live in plain functions or modules separate from `draw()` so it can be unit tested.

Rationale: a single message vocabulary keeps the growing set of modes (Normal, Git, Cargo,
Repos, …) composable and debuggable.

### V. Quality Gates (NON-NEGOTIABLE)

Every change merged to `main` MUST pass, on the nightly toolchain:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings` (clippy `pedantic` enabled)
- `cargo test --locked --all-features`

Additionally:

- New non-UI logic MUST ship with unit tests; bug fixes MUST include a regression test when the
  bug is reproducible without a real terminal.
- Tests touching the filesystem MUST use temporary directories and MUST NOT depend on or modify
  the developer's home directory or real config.
- `#[allow(clippy::…)]` suppressions MUST be narrowly scoped and justified by a comment.

Rationale: these mirror CI; local and CI results must agree.

## Technical Constraints

- Language/toolchain: Rust, edition 2024, **nightly** toolchain.
- Core stack: Ratatui (rendering), Crossterm (terminal events), Tokio (async runtime),
  `git2` (git integration — prefer it over shelling out to `git`), `opaline` (theming),
  `notify` (file watching), `config` + json5 (configuration), `tracing` (logging),
  `color-eyre` (error reporting).
- Primary target platform is Linux; macOS SHOULD keep compiling. Platform-specific code MUST be
  gated with `cfg` attributes.
- Configuration and persisted state MUST use XDG locations (or `$CRABMANDER_CONFIG` /
  `$CRABMANDER_DATA`), and MUST tolerate missing or malformed files by falling back to defaults.
- New dependencies MUST be justified by a capability the stdlib or existing dependencies cannot
  reasonably provide; `Cargo.lock` MUST be committed.

## Development Workflow

- Feature work happens on branches; `main` is kept releasable and protected by CI
  (test, rustfmt, clippy, docs jobs).
- Non-trivial features SHOULD follow the Spec Kit flow (`/speckit-specify` → `/speckit-plan` →
  `/speckit-tasks` → `/speckit-implement`), and plans MUST include a Constitution Check against
  the principles above.
- User-visible changes (keys, modes, config keys) MUST update `README.md`; architectural changes
  MUST update `DESIGN.md` and/or `CLAUDE.md`.
- Commits SHOULD be small and descriptive; squash merges are acceptable.

## Governance

- This constitution supersedes ad-hoc practices for Crabmander. Where `CLAUDE.md` or other
  guidance conflicts with it, this document wins until amended.
- Amendments are made by editing `.specify/memory/constitution.md` (via `/speckit-constitution`),
  recording a Sync Impact Report, and bumping the version:
  - MAJOR: removal or backward-incompatible redefinition of a principle.
  - MINOR: new principle or section, or materially expanded guidance.
  - PATCH: clarifications and wording fixes with no semantic change.
- Every spec, plan, and review MUST verify compliance with the Core Principles. Deviations MUST be
  documented in the plan's Complexity Tracking with justification and a simpler alternative that
  was rejected.
- Runtime development guidance lives in `CLAUDE.md`.

**Version**: 1.0.0 | **Ratified**: 2026-11-20 | **Last Amended**: 2026-11-20
