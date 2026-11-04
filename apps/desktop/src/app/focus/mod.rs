//! Shell focus, first-responder reconciliation, leaf borders, agents tab selection and closing, and terminal mount slot focus and mouse forwarding, one sibling module per concern (each with its own `impl GhostexGpuiApp` block).

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: shell focus, first-responder reconciliation, directional/spatial focus, leaf borders

mod agents_tab_actions;
mod agents_tab_selection;
mod shell_focus;
mod terminal_focus_hotkeys;
mod terminal_mount_slots;
