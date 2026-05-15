use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;

/// Context passed to every `DeferredOp::execute` call.
pub struct OpCtx {
    /// Channel back to the main event loop.
    pub tx: UnboundedSender<Action>,
    /// Pre-allocated progress-tracking id for this operation.
    pub op_id: u64,
    /// Text entered by the user in an Input dialog (empty string for Confirm dialogs).
    pub input: String,
}

/// A deferred file/git operation carried inside a dialog until the user confirms.
///
/// Each operation is its own struct. `App` holds a registry of implementations.
/// The dialog carries a `Box<dyn DeferredOp>` and calls `execute` on confirmation.
pub trait DeferredOp: Send + Sync + std::fmt::Debug {
    /// Whether `App` should stay in `Mode::Git` after confirming (default: false → Normal mode).
    fn stays_in_git_mode(&self) -> bool {
        false
    }

    /// Called by `App` when the dialog is confirmed. Spawns async work and returns immediately.
    fn execute(self: Box<Self>, ctx: OpCtx);
}
