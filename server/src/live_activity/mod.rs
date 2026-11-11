//! The iPhone's Live Activity (the Lock Screen banner) kept current while the phone is locked: each phone's
//! Live Activity tokens (`store.rs`), the banner's content built as the phone builds it (`content.rs`), and
//! the worker that sends it through Ghostex's Live Activity push relay (`runtime.rs`, `relay.rs`;
//! the relay is `server/push-relay`).

pub(crate) mod content;
pub(crate) mod relay;
pub(crate) mod runtime;
pub(crate) mod store;

pub(crate) use runtime::LiveActivityRuntime;
