//! Chief: the Ghostex-wide coordinator, one per computer, pinned under the Spaces bar.
//!
//! - [`record`]: which session is this computer's Chief (`metadata.chiefSession`), written only by
//!   gxserver itself when it creates that session.
//! - [`feature`]: the built-in extension's switch (`chiefHidden`, on by default).
//! - [`head`]: the head role, given per request at the auth gate to that one session while the
//!   switch is on, and never stored with a token.
//! - [`session`]: creating the session (first open, New Chat, Change Agent) in the Ghostex config
//!   folder's project, as a coordinator whose memory and threads follow it to a new chat.
//! - [`sweep`]: puts its agent to sleep when the switch goes off.
//! - [`endpoint`]: `/api/chief` (`status`, `open`, `newChat`, `setAgent`, `guide`).
//!
//! The user's decisions: CDXC:Chief in packages/settings-catalog/src/data/official_extensions.rs
//! and server/src/security/capability.rs.

mod endpoint;
mod feature;
mod head;
mod record;
mod session;
mod sweep;

pub(crate) use endpoint::{handle_chief_http, CHIEF_ENDPOINT};
pub(crate) use feature::{chief_off_message, chief_on, refuses_message};
pub(crate) use head::settle_principal;
pub(crate) use record::{chief_key, read_record};
#[cfg(test)]
pub(crate) use record::record_chief_for_tests;
pub(crate) use sweep::start_chief_sweep;

/// The playbook Chief follows, printed by `ghostex chief guide` (and `/api/chief` `guide`).
pub(crate) const CHIEF_PLAYBOOK: &str = include_str!("playbook.md");

/// Chief's name in the sidebar, the breadcrumb and its session title (Q1746).
pub(crate) const CHIEF_TITLE: &str = "Chief";
