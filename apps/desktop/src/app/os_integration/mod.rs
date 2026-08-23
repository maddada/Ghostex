// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). Each
// submodule contributes its own `impl GhostexGpuiApp` block, so no glob
// re-export is needed here (inherent methods resolve on the type regardless
// of which module defines them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
pub(crate) mod cua_gte_and_file_open;
pub(crate) mod first_run_onboarding;
pub(crate) mod gxserver_bootstrap;
pub(crate) mod gxserver_stop_and_workspace_sleep;
pub(crate) mod keep_awake_automation;
pub(crate) mod keep_awake_core;
pub(crate) mod keep_awake_lid_sleep;
pub(crate) mod notifications_and_portless;
pub(crate) mod toast_and_status_dispatch;
pub(crate) mod updater;
