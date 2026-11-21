//! Native GPUI Add Project picker: a folder window with a sidebar of places and computers, one
//! path field, the folders in columns with a preview of the picked folder, Suggested
//! repositories, New folder (named in place), New project and Clone a repo (side panels), and
//! the notices (inside a repository, Create & Add, folder access, errors).
//!
//! CDXC:AddProject 2026-11-21 DECISION:
//! User (Q4380 "B", Q4381): "the add a project modal is very annoying to use ... when i pick to add a folder ... i dont see the folder being created". Add Project is a folder picker like the modern Windows dialog, drawn in GPUI, in layout B: columns with a preview and a Suggested grid, with the sidebar sources of server/src/server/project_picker.rs. User, on the mockup: "i think it's weird we show the location below and above": the picked folder is shown once, in the path field at the top, which is also where a path or a repository is typed (no Folder field at the bottom). Every option of the command list stays: Local folder is the picker itself, External drives and other folders the sidebar's disks, the machine step the sidebar's Computers and the footer's computer button, New Folder a row named in its column that becomes the picked folder, New project folder the New project panel, the five clone sources, readiness, CLI install, branch and options the Clone a repo panel (the folder on screen is the destination), Add repository root and Open existing project the inside-a-repository notice and Open project. This supersedes CDXC:AddProject 2026-09-28 (the command-palette dialog in a 640x460 window) and the source-row order of CDXC:Onboarding 2026-11-11 (the folder-access line it asked for stays, render.rs).
//! SEE-ALSO: apps/desktop/src/app/add_project_modal_lifecycle.rs (open and host), apps/desktop/src/app/remote_conn/project_browse_and_add.rs (`run_gpui_add_project_dialog_operation`, the gxserver calls), server/src/server/project_picker.rs (places and folder details), apps/desktop/src/bin/native_modal_demo/add_project.rs (standalone preview).
//!
//! The view depends only on gpui, gpui-component, `native_modal_kit/` and the crate-root
//! `ui_fonts` module, so the preview binary can include it with `#[path]`.
mod copy;
mod derive;
mod input;
mod keyboard;
mod model;
mod new_project;
mod paths;
mod render;
mod sheets;
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
// The background clone (app/background_jobs/) reads gxserver's clone answers the dialog's way.
#[allow(unused_imports)]
pub(crate) use model::{
    AddProjectCloneJob, AddProjectCloneJobState, describe_add_project_error,
    read_add_project_clone_handle, read_add_project_clone_job,
};
#[allow(unused_imports)]
pub(crate) use new_project::{
    AddProjectNewProjectConfig, AddProjectPlaceChoice, AddProjectPlacementOptions, AddProjectStart,
    AddProjectStartAgent,
};
#[allow(unused_imports)]
pub(crate) use window::AddProjectModalHost;
