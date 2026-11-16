//! Terminal clipboard and paste, text-input handoff, IME preedit and text sends, and close-confirm dialogs, one sibling module per concern (each with its own `impl GhostexGpuiApp` block).

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: clipboard/paste, IME preedit, text-input handoff, close-confirm dialogs

mod close_confirm;
mod paste_and_shortcuts;
mod preedit_and_text_send;
mod text_input_handoff;

pub(crate) use paste_and_shortcuts::TerminalPasteFailure;
