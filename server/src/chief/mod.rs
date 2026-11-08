//! Chief: the Ghostex-wide coordinator, one per computer, pinned under the Spaces bar.
//!
//! - [`record`]: which session is this computer's Chief (`metadata.chiefSession`), written only by
//!   gxserver itself when it creates that session.
//! - [`feature`]: the built-in extension's switch (`chiefHidden`, on by default).
//! - [`head`]: the head role, given per request at the auth gate to that one session while the
//!   switch is on, and never stored with a token.
//! - [`home`]: the home project at the home folder, whose header is Chief's row.
//! - [`session`]: creating the session (first open, New Chat, Change Agent) in the home project,
//!   as a coordinator whose memory and threads follow it to a new chat; earlier chats sleep.
//! - [`presentation`]: `chiefEarlier` on the earlier chats, and the folder-trust answer for
//!   Chief's own sessions at the home folder.
//! - [`sweep`]: puts its agent to sleep when the switch goes off.
//! - [`endpoint`]: `/api/chief` (`status`, `open`, `newChat`, `setAgent`, `guide`).
//!
//! The user's decisions: CDXC:Chief in packages/settings-catalog/src/data/official_extensions.rs
//! and server/src/security/capability.rs.

mod endpoint;
mod feature;
mod head;
mod home;
mod presentation;
mod record;
mod session;
mod sweep;

pub(crate) use endpoint::{handle_chief_http, CHIEF_ENDPOINT};
pub(crate) use feature::{chief_off_message, chief_on, refuses_message};
pub(crate) use head::settle_principal;
pub(crate) use home::{is_home_folder, is_home_project};
pub(crate) use presentation::{
    chief_home_session_trusted, insert_chief_presentation_payload, insert_chief_session_projection,
};
pub(crate) use record::{chief_key, read_record};
pub(crate) use sweep::start_chief_sweep;

/// The playbook Chief follows, printed by `ghostex chief guide` (and `/api/chief` `guide`).
pub(crate) const CHIEF_PLAYBOOK: &str = include_str!("playbook.md");

/// Chief's name in the sidebar, the breadcrumb and its session title: the one place gxserver
/// spells it (the desktop's is `CHIEF_LABEL` in
/// apps/desktop/src/app/native_sidebar/chief_row_view.rs, the phone's `CHIEF_LABEL` in
/// apps/mobile/app/src/contract/chief.ts).
///
/// CDXC:Chief 2026-11-08 DECISION:
/// User (Q1931): the name users see is "Main Hub" everywhere: the row on the desktop and the phone (the home's header), its session title and breadcrumb, the Extensions card, its menus, Settings, notifications and Help. It was "Chief" (Q1746), then "Chief of Staff" (Q1896). Internal ids, the `chiefHidden` setting and the `ghostex chief` command keep their names so nothing breaks.
pub(crate) const CHIEF_TITLE: &str = "Main Hub";
/// The session titles Chief was created with before the renames; sweep.rs renames such a session
/// once (a title the user gave it stays).
pub(crate) const OLD_CHIEF_TITLES: &[&str] = &["Chief", "Chief of Staff"];
