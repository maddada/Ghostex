//! The Changes view: a project's diff in the side panel, with line comments for the agent, revert
//! and the commit bar. The view depends only on gpui, gpui-component, the modal kit and the Git
//! calls so the GPUI web build compiles the same folder.

mod comments;
mod commit_bar;
mod model;
mod render;
mod rows;
mod static_diff;
mod style;
mod tokens;
mod view;

pub(crate) use model::ChangesScope;
#[cfg(test)]
pub(crate) use model::parse_patch_for_tests;
#[cfg_attr(target_arch = "wasm32", allow(unused_imports))]
pub(crate) use static_diff::render_static_diff;
pub(crate) use view::{
    ChangesHostFn, ChangesHostInfo, ChangesPreset, ChangesRequest, ChangesTarget, ChangesView,
    CommitAction,
};
