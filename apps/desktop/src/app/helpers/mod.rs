// C1 wave-1: stateless helper fns moved verbatim out of main.rs, grouped by
// rough topic. Each submodule is glob-re-exported here so every existing
// unqualified call site in main.rs (and in the helpers themselves, via
// `use crate::app::helpers::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead.
pub(crate) mod agents_hub;
pub(crate) mod board_gxserver;
pub(crate) mod browser;
pub(crate) mod manage_docs;
pub(crate) mod os_cli;
pub(crate) mod project;
pub(crate) mod remote;
pub(crate) mod sidebar;
pub(crate) mod source_server;
pub(crate) mod titlebar;

pub(crate) use agents_hub::*;
pub(crate) use board_gxserver::*;
pub(crate) use browser::*;
pub(crate) use manage_docs::*;
pub(crate) use os_cli::*;
pub(crate) use project::*;
pub(crate) use remote::*;
pub(crate) use sidebar::*;
pub(crate) use source_server::*;
pub(crate) use titlebar::*;
