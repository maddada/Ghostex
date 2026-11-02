//! Cloud Boxes: sessions that run on an Instabox or Boat machine (or the `local` Docker provider
//! for development) with gxserver on the box. Home gxserver owns the registry and every provider
//! call here; `box_mode/` is what the same binary does on a box. The wire contract is
//! `ghostex_gx_protocol::cloud_box`, the provider rules are `ghostex_gx_cloud`. Split by concern;
//! see each file.

pub(crate) mod box_mode;
mod create;
mod docker;
pub(crate) mod endpoint;
mod events;
mod local_provider;
mod params;
mod project;
mod providers;
mod registry;
mod spend;
