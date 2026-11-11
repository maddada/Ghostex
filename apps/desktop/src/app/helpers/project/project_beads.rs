// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// The blocking project board Beads bridge and the desktop's own title-generation subprocess that
// lived here were removed on 2026-11-10: the native board calls gxserver through `gx_rpc`
// (`native_kanban/beads.rs`) and titles come from gxserver's `/api/beadsGenerateTitle`.

pub(crate) fn gpui_normalized_project_path_for_comparison(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}
