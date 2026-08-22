pub mod admin;
pub mod launchd;
pub mod listener_discovery;
pub mod repository;
pub mod slug;
pub mod status;
pub mod sync;
#[cfg(test)]
mod tests;
pub mod types;

pub use admin::*;
pub use repository::*;
pub use status::*;
pub use sync::*;
pub use types::*;

// launchd/listener_discovery currently have no crate-external-facing (fully
// `pub`) items of their own, only `pub(crate)` ones. Sibling files reach
// those directly via their own `use super::X::*;`, so the only consumer of
// a re-export here is the `tests` module's `use super::*;`, hence these
// stay cfg(test)-only to avoid an "unused import" warning in non-test
// builds. `slug` has no such consumer (tests never reference its items by
// name), so it is not re-exported here at all.
#[cfg(test)]
pub(crate) use launchd::*;
#[cfg(test)]
pub(crate) use listener_discovery::*;
