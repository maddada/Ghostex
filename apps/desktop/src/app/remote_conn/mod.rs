// C1 wave-4 deferred split: apps/desktop/src/app/remote_conn.rs (~4.5k lines
// of `impl GhostexGpuiApp` methods) further divided into responsibility-scoped
// submodules, pure move, no logic changes. Each submodule holds its own
// `impl GhostexGpuiApp { .. }` block with a subset of the methods; since these
// are inherent impl methods on GhostexGpuiApp (not free functions or types),
// callers resolve them through `self.method_name(..)` regardless of which
// submodule defines them, so no re-export globs are needed here (unlike the
// os_cli/ and helpers/remote/ splits, which re-export free functions and
// types). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
pub(crate) mod app_modal_bridge;
pub(crate) mod attach_terminal;
pub(crate) mod clone_job_and_preview;
pub(crate) mod clone_lifecycle;
pub(crate) mod dispatch_results;
pub(crate) mod native_action;
pub(crate) mod presentation_and_watchdog;
pub(crate) mod project_browse_and_add;
pub(crate) mod reconnect;
pub(crate) mod settings_and_install_probe;
pub(crate) mod sidebar_request_and_recent_projects;
