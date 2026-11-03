//! Search by Prompt (the GUI for `gx f`) as a native GPUI window.
//!
//! CDXC:PromptSearch 2026-09-27 DECISION:
//! User: "lets migrate find by prompt modal to gpui also please but make it stays exactly same as react one we have now and make sure it's performant". The window reproduces the React Find page (packages/core-ui/find/, later the phone's views/find/; deleted 2026-10-01 when the phone's Find became native screens, in git history) and asks gxserver's four prompt-search endpoints through `gx_rpc`; search itself stays in gxserver.
//! SEE-ALSO: apps/desktop/src/app/find_prompts_modal_lifecycle.rs (open, focus, launch, close), apps/mobile/app/src/find/ (the phone's native Find screens, which share gxserver's ranking and favorites).
mod menus;
pub(crate) mod model;
pub(crate) mod palette;
mod render;
mod toolbar_steps;
mod window;

pub(crate) use window::{FindPromptsModalCommand, GpuiFindPromptsModalWindow};
