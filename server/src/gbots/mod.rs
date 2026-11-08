//! GBots: Ghostex's own bots, led by Chief of Staff in Main Hub. A GBot is a lasting persona (a
//! `gbots_bots` row) with its own folder and project; its chats are ordinary coordinator sessions
//! in that project.
//!
//! - [`record`]: the `gbots_bots` table, its limits and colours, and the `gbots_changes` journal.
//! - [`folder`]: the bot folder (`<app data>/gbots/bots/<botId>/`): `SOUL.md`, `MEMORY.md`,
//!   `memory/`, `files/`, the generated `CLAUDE.md` / `AGENTS.md`.
//! - [`brief`]: what loads into a bot (the instruction files and `ghostex gbots brief`).
//! - [`project`]: the bot's project (`launchSettings.gbotsBotId`).
//! - [`session`]: its chats (create, list, close), the Chief of Staff way.
//! - [`presentation`]: `gbotsBot` on the project row, `gbotsChat` on each chat.
//! - [`feature`]: on only while Main Hub and the GBots switch are both on; [`sweep`] keeps that
//!   current and puts bot chats to sleep when GBots go off.
//! - [`endpoint`]: `/api/gbots`.
//! - [`retire`]: the one-time removal of the previous GBots; [`trash`]: folders to the Trash.
//!
//! CDXC:GBots 2026-11-08 DECISION:
//! User: remove the previous GBots fully, "we are doing our own bots thing." The previous GBots, a downloaded runtime gxserver ran beside itself, is gone with its runtime download, transcript mirror, chat agent, Bots space source and settings; its bots and chats are not imported. Native GBots keep the name, the extension switch (`gbotsEnabled`), the `ghostex gbots` command and the `/api/gbots` route. Existing computers lose the old bots once, in retire.rs.

mod brief;
mod endpoint;
mod feature;
mod folder;
mod presentation;
mod project;
mod record;
mod retire;
mod session;
mod sweep;
mod trash;

pub(crate) use endpoint::{handle_gbots_http, GBOTS_ENDPOINT};
pub(crate) use feature::{gbots_off_message, gbots_on, gbots_shown};
pub(crate) use presentation::{
    insert_gbots_project_presentation, insert_gbots_session_presentation,
};
pub(crate) use record::name_problem;
pub(crate) use retire::retire_previous_gbots;
pub(crate) use sweep::start_gbots_sweep;
