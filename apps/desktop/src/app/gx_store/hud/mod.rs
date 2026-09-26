//! The sidebar HUD on the desktop: `reads.rs` asks gxserver for the HUD and the recent projects
//! (web-ready: `gx_rpc` and gx-core only), `host.rs` keeps the sources, composes the document with
//! gx-core `hud::compose_sidebar_hud` and hands it to every reader.

mod host;
mod reads;

pub(crate) use host::HudHost;
