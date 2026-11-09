//! Chief: the Ghostex-wide coordinator, one per computer, pinned under the Spaces bar.
//!
//! - [`record`]: which session is this computer's Chief (`metadata.chiefSession`), written only by
//!   gxserver itself when it creates that session.
//! - [`feature`]: the built-in extension's switch (`mainHubHidden`, the Main Hub extension, off by default).
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
pub(crate) use feature::{
    chief_off_message, chief_on, chief_shown, refuses_message, remember_chief_shown,
};
pub(crate) use head::settle_principal;
pub(crate) use home::{is_home_folder, is_home_project};
pub(crate) use presentation::{
    chief_home_session_trusted, insert_chief_presentation_payload, insert_chief_session_projection,
};
pub(crate) use record::{chief_key, read_record};
pub(crate) use session::{chief_agent_rows, start_provider};
pub(crate) use sweep::start_chief_sweep;

/// The playbook Chief follows, printed by `ghostex chief guide` (and `/api/chief` `guide`) through
/// [`chief_playbook`].
const CHIEF_PLAYBOOK: &str = include_str!("playbook.md");

/// The playbook as Chief reads it: the parts between `<!-- gbots -->` and `<!-- /gbots -->` only
/// while GBots are on, and never the marker lines themselves.
///
/// CDXC:GBots 2026-11-09 DECISION:
/// User: "pls fully hide gbots if disabled in experimental under extensions". While GBots are off Chief of Staff's playbook says nothing about leading GBots, their problems, Day Watcher or their settings; it keeps only the line that turning GBots on is the user's, in Settings > Extensions.
pub(crate) fn chief_playbook(gbots_on: bool) -> String {
    let mut out = String::with_capacity(CHIEF_PLAYBOOK.len());
    let mut in_gbots = false;
    for line in CHIEF_PLAYBOOK.split_inclusive('\n') {
        match line.trim_end() {
            "<!-- gbots -->" => in_gbots = true,
            "<!-- /gbots -->" => in_gbots = false,
            _ if in_gbots && !gbots_on => {}
            _ => out.push_str(line),
        }
    }
    out
}

/// Chief's name in the sidebar, the breadcrumb and its session title: the one place gxserver
/// spells it (the desktop's is `CHIEF_LABEL` in
/// apps/desktop/src/app/native_sidebar/chief_row_view.rs, the phone's `CHIEF_LABEL` in
/// apps/mobile/app/src/contract/chief.ts).
///
/// CDXC:Chief 2026-11-08 DECISION:
/// User: "Please don't make Main Hub a thread AND a project header ... Let's just always pin 1 thread to the top of Main Hub called Chief of Staff". The agent is "Chief of Staff" again everywhere users see it (its row and session title, its menus, the Extensions card, notifications, Help); "Main Hub" names only the home project (its header and card). It was "Chief" (Q1746), "Chief of Staff" (Q1896), then "Main Hub" (Q1931), which this supersedes for the agent. Internal ids, the `chiefHidden` setting and the `ghostex chief` command keep their names so nothing breaks.
pub(crate) const CHIEF_TITLE: &str = "Chief of Staff";
/// The session titles Chief was created with under earlier names; sweep.rs renames such a session
/// once (a title the user gave it stays).
pub(crate) const OLD_CHIEF_TITLES: &[&str] = &["Chief", "Main Hub"];
