//! Chief: the Ghostex-wide coordinator, one per computer, pinned at the top of Main Hub.
//!
//! - [`record`]: which session is this computer's Chief (`metadata.chiefSession`), written only by
//!   gxserver itself when it creates that session.
//! - [`head`]: the head role, given per request at the auth gate to that one session, and never
//!   stored with a token.
//! - [`home`]: the home project at the home folder, whose header is Main Hub.
//! - [`model`]: the agent, model and effort it starts on (the default agent's newest Opus, Sol or
//!   Gemini Neon at High), and the user's own pick, kept for its next chats.
//! - [`session`]: creating the session (first open, New Chat, Change Agent) in the home project,
//!   as a coordinator whose memory and threads follow it to a new chat; earlier chats sleep.
//! - [`presentation`]: `chiefEarlier` on the earlier chats, and the folder-trust answer for
//!   Chief's own sessions at the home folder.
//! - [`sweep`]: one-time startup migration for Chief's older default titles.
//! - [`endpoint`]: `/api/chief` (`status`, `open`, `newChat`, `setAgent`, `guide`, `prompt`).
//!
//! The user's decisions: CDXC:Chief in packages/settings-catalog/src/data/official_extensions.rs
//! and server/src/security/capability.rs.

mod endpoint;
mod head;
mod home;
mod model;
mod presentation;
mod record;
mod session;
mod sweep;
mod welcome;

pub(crate) use endpoint::{handle_chief_http, CHIEF_ENDPOINT};
pub(crate) use head::settle_principal;
pub(crate) use home::{
    is_help_home_project, is_home_folder, is_home_project, lists_under_main_hub, presents_as_home,
};
pub(crate) use model::{follow_agent_switch, remember_hand_pick};
pub(crate) use presentation::{
    chief_home_session_trusted, insert_chief_presentation_payload, insert_chief_session_projection,
};
pub(crate) use record::{chief_key, read_record};
pub(crate) use session::{chief_agent_rows, start_provider};
pub(crate) use sweep::start_chief_sweep;
pub(crate) use welcome::{remember_asked, welcome_state};

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
/// CDXC:Chief 2026-11-21 DECISION:
/// User, relayed by Chief of Staff: "Rename Chief of Staff to 'Ask Ghostex AI' everywhere the user sees the name", keeping `ghostex chief`, every id, setting key and API. Its row, session title, menu entries and the Ask Ghostex AI menu row it answers all read "Ask Ghostex AI"; in a sentence it is "Ghostex AI" ("Ghostex AI's chat", "I'm Ghostex AI"). Supersedes the 2026-11-08 name above. A session still titled "Chief of Staff" (or one of its dated earlier chats) takes the new name once at gxserver start (sweep.rs).
/// CDXC:Chief 2026-11-28 DECISION:
/// User: "Rename Ask Ghostex AI threads default name to Ghostex AI". Its chat's own name (session title, row, header, tabs, Earlier Chats, notifications, Needs you, the phone, CLI listings) is "Ghostex AI"; an earlier chat is "Ghostex AI · 8 Nov, 13:30". Action labels that invite the user to ask (the sidebar menu's and rail's "Ask Ghostex AI", "Design a theme with Ask Ghostex AI", the help popup's title) keep "Ask Ghostex AI". A chat still titled "Ask Ghostex AI" (or a dated earlier one) takes the new name once at gxserver start (sweep.rs); one the user renamed keeps its name. Supersedes the 2026-11-21 chat name above.
pub(crate) const CHIEF_TITLE: &str = "Ghostex AI";
/// The session titles Chief was created with under earlier names; sweep.rs renames such a session
/// once (a title the user gave it stays).
pub(crate) const OLD_CHIEF_TITLES: &[&str] =
    &["Chief", "Main Hub", "Chief of Staff", "Ask Ghostex AI"];
