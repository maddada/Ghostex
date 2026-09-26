//! The CLI's renderer commands (`ghostex focus`, `ghostex settings set`, `ghostex rename-command`,
//! `ghostex browser open`, and the rest gxserver forwards through `/api/dispatchRendererCommand`):
//! which command a frame is, whether its payload is valid, and which session or project it names.
//! The desktop host performs the answer (`apps/desktop/src/app/gx_store/renderer_commands/`).

mod errors;
mod project_step;
mod targets;
mod verbs;

pub use errors::RendererCommandError;
pub use project_step::{plan_project_step, ProjectStep, ProjectStepDirection};
pub use targets::{resolve_renderer_group_project, resolve_renderer_session, RendererSession};
pub use verbs::{
    plan_renderer_command, OpenPathTarget, OpenPathsMode, RendererVerb, SETTINGS_MODAL_TABS,
};
