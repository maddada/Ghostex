//! Native GPUI Add Project dialog, the desktop twin of the React `AddProjectModal` in
//! packages/core-ui/add-project-modal/add-project-modal.tsx (deleted 2026-10-01): the machine step, the sources step
//! with source-control readiness, path browsing with live directory listing and New Folder, the
//! clone wizard (repository, destination, review) with clone job progress and cancel, and the
//! persistent error region, all with the React keyboard model (no auto-highlight in path modes,
//! Enter submits the typed path, Cmd/Ctrl+Enter overrides a highlighted row, Backspace on an
//! empty field steps back, Escape closes).
//!
//! CDXC:AddProject 2026-09-28 DECISION:
//! User: migrate every React modal to GPUI with GPUI-Kit components, looking and working exactly as now, so the app runs without CEF. Add Project keeps the command-palette dialog in its fixed 640x460 child window: the same steps, copy, colours (measured from the Storybook story in both appearances), key caps, scroll edge fades and gxserver round trips, which the app now answers by calling its request runner directly instead of posting `addProjectDialogRequest` bridge JSON.
//! SEE-ALSO: packages/core-ui/add-project-modal/ (the React twin and its stories), packages/core-ui/remote-project-picker/remote-project-paths.ts (deleted 2026-10-01) (paths.rs), apps/desktop/src/app/add_project_modal_lifecycle.rs (open and host), apps/desktop/src/app/remote_conn/project_browse_and_add.rs (`run_gpui_add_project_dialog_operation`, the gxserver calls), apps/desktop/src/bin/native_modal_demo/add_project.rs (standalone preview).
//!
//! The view depends only on gpui, gpui-component, `native_modal_kit/` and the crate-root
//! `hotkey_label` and `ui_fonts` modules, so the preview binary can include it with `#[path]`.
mod copy;
mod derive;
mod input;
mod keyboard;
mod model;
mod new_project;
mod paths;
mod render;
mod review;
mod skin;
mod steps;
mod tool_install;
mod window;

pub(crate) use window::{
    ADD_PROJECT_MODAL_HEIGHT, ADD_PROJECT_MODAL_WIDTH, AddProjectModalCommand,
    AddProjectModalConfig, GpuiAddProjectModalWindow,
};
// The app reads the machine list from JSON; the preview binary builds machines and a host itself.
#[allow(unused_imports)]
pub(crate) use model::{AddProjectMachineOption, parse_add_project_machine_options};
#[allow(unused_imports)]
pub(crate) use new_project::{
    AddProjectNewProjectConfig, AddProjectPlaceChoice, AddProjectPlacementOptions, AddProjectStart,
    AddProjectStartAgent,
};
#[allow(unused_imports)]
pub(crate) use window::AddProjectModalHost;
