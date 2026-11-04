// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: project-workarea runtime CEF surfaces, source code server, tab scroll handles
//
// 2026-09-20: the flat `workarea.rs` had grown to 1,745 lines, so it became this directory
// during the UI revamp's quiet window. Pure motion: every item below sits byte-identically in
// the sibling file its concern names, and nothing outside this directory changed. Rust allows
// inherent impl blocks in any module of the crate that owns the type, so each file is a plain
// `impl GhostexGpuiApp { .. }` slice and needs no re-export. Add a new workarea concern to the
// file that owns it, or to a new sibling; never to this barrel.
pub(crate) mod code_windows;
pub(crate) mod extension_views;
pub(crate) mod runtime_surfaces;
pub(crate) mod shell_state;
pub(crate) mod source_code_server;
pub(crate) mod tab_scroll;
pub(crate) mod view_visibility;
