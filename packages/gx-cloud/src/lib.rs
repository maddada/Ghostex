//! Cloud Boxes provider rules, transport-free (see Cargo.toml for why).
//!
//! The wire types are in `ghostex_gx_protocol::cloud_box`; this crate holds what turns them into
//! provider calls and back. gxserver (`server/src/cloud_boxes/`) executes what it builds.

pub mod ids;
pub mod local;
pub mod metadata;
pub mod prices;
pub mod project_key;
pub mod state;
pub mod ticket;
