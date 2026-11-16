//! Sidebar payload reconciliation, project switching, browser tabs and panes, and the active work area mode, one sibling module per concern (each with its own `impl GhostexGpuiApp` block).

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: sidebar project-context reconciliation, project switching, browser tabs/panes

mod active_mode;
mod browser_page_context_menu;
mod browser_tabs;
mod project_switch;
mod sidebar_payloads;

pub(crate) use browser_page_context_menu::browser_page_gpui_context_menu;
