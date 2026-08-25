// C1 wave-2: window-entity types moved verbatim out of main.rs, grouped by
// window. Each submodule is glob-re-exported here so every existing
// unqualified call site in main.rs (and in these modules themselves, via
// `use crate::app::window::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead.
pub(crate) mod extension_titlebar_panel;
pub(crate) mod modal_host;
pub(crate) mod titlebar_panels;
pub(crate) mod toast;

pub(crate) use extension_titlebar_panel::*;
pub(crate) use modal_host::*;
pub(crate) use titlebar_panels::*;
pub(crate) use toast::*;
