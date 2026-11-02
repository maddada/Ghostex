//! Cloud Boxes wire contract: sessions that run on an Instabox or Boat machine (or, for zero-cost
//! development, a `local` Docker container) with gxserver on the box.
//!
//! Home gxserver owns the registry and every provider call (`server/src/cloud_boxes/`); the box's
//! own gxserver runs in box mode (`server/src/cloud_boxes/box_mode/`). Every client (desktop, GPUI
//! web build, `ghostex cloud`, phone) speaks to both through one route, [`CLOUD_BOXES_ROUTE`], with
//! an `action` field ([`CloudBoxesAction`]); the request and result shape of each action is in
//! [`rpc`]. Box-state pushes ride the event socket as `cloudBoxesChanged`
//! ([`CloudBoxesChangedFrame`]).
//!
//! The TypeScript twin is `packages/shared/gxserver-protocol-cloud-boxes.ts`; change both together.

pub mod box_side;
pub mod boxes;
pub mod enums;
pub mod events;
pub mod logins;
pub mod machine;
pub mod provider;
pub mod rpc;
pub mod settings;
pub mod snapshot;
pub mod spend;

pub use box_side::*;
pub use boxes::*;
pub use enums::*;
pub use events::*;
pub use logins::*;
pub use machine::*;
pub use provider::*;
pub use rpc::*;
pub use settings::*;
pub use snapshot::*;
pub use spend::*;

/// The one HTTP route of Cloud Boxes (`POST`, RPC envelope, `action` in the params).
pub const CLOUD_BOXES_ROUTE: &str = "/api/cloudBoxes";
