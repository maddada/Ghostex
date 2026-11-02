//! Cloud Boxes: sessions that run on an Instabox or Boat machine (or the `local` Docker provider
//! for development) with gxserver on the box. Home gxserver owns the registry and every provider
//! call here; `box_mode/` is what the same binary does on a box. The wire contract is
//! `ghostex_gx_protocol::cloud_box`, the provider rules are `ghostex_gx_cloud`. Split by concern;
//! see each file.

pub(crate) mod agent_logins;
mod boot_steps;
pub(crate) mod box_mode;
mod connect;
mod create;
mod docker;
pub(crate) mod endpoint;
mod events;
mod http;
mod lifecycle;
mod local_provider;
mod ops;
mod params;
pub(crate) mod presentation;
mod project;
mod providers;
mod reconcile;
mod registry;
mod scheduler;
mod secret_files;
mod snapshots;
mod spend;
mod store;
mod time;

pub(crate) use scheduler::start_cloud_boxes_scheduler;
