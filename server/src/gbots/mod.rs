//! GBots: Ghostex's own bots, led by Chief of Staff in Main Hub. A GBot is a lasting persona (a
//! `gbots_bots` row) with its own folder and project; its chats are ordinary coordinator sessions
//! in that project.
//!
//! - [`record`]: the `gbots_bots` table, its limits and colours, and the `gbots_changes` journal.
//! - [`folder`]: the bot folder (`<app data>/gbots/bots/<botId>/`): `SOUL.md`, `MEMORY.md`,
//!   `memory/`, `files/`, the generated `CLAUDE.md` / `AGENTS.md`.
//! - [`brief`]: what loads into a bot (the instruction files and `ghostex gbots brief`).
//! - [`memory`]: `MEMORY.md` within its budget, the archive, topic notes, the daily log, the
//!   journal with undo, the shared preferences, and the `/api/gbots` memory ops.
//! - [`project`]: the bot's project (`launchSettings.gbotsBotId`).
//! - [`session`]: its chats (create, list, close), the Chief of Staff way.
//! - [`presentation`]: `gbotsBot` on the project row, `gbotsChat` on each chat.
//! - [`feature`]: on only while Main Hub and the GBots switch are both on; [`sweep`] keeps that
//!   current and puts bot chats to sleep when GBots go off.
//! - [`assign`]: `ghostex gbots ask`, work handed to a bot or a question asked of it, reported back
//!   the coordinator-thread way; [`assignments`] is its table.
//! - [`departments`]: GBots grouped under a name with shared instructions and a lead;
//!   [`department_ops`] is their `/api/gbots` ops and the lead's right to change its members'
//!   instructions.
//! - [`changes`]: the change journal read back, Undo, Chief of Staff's caps; [`card`]: the "GBot
//!   added" card.
//! - [`endpoint`]: `/api/gbots`.
//! - [`routines`]: a GBot's routines (`/api/gbots` `routine…` ops, the "Routine set" card, Undo);
//!   [`routine_runs`]: whether one may run, the chat it runs in and where its result goes.
//! - [`retire`]: the one-time removal of the previous GBots; [`trash`]: folders to the Trash.
//!
//! CDXC:GBots 2026-11-08 DECISION:
//! User: remove the previous GBots fully, "we are doing our own bots thing." The previous GBots, a downloaded runtime gxserver ran beside itself, is gone with its runtime download, transcript mirror, chat agent, Bots space source and settings; its bots and chats are not imported. Native GBots keep the name, the extension switch (`gbotsEnabled`), the `ghostex gbots` command and the `/api/gbots` route. Existing computers lose the old bots once, in retire.rs.

mod assign;
mod assignments;
mod brief;
mod card;
mod changes;
mod department_ops;
mod departments;
mod endpoint;
mod feature;
mod folder;
mod memory;
mod presentation;
mod project;
mod record;
mod retire;
mod routine_runs;
mod routines;
mod session;
mod sweep;
mod trash;

pub(crate) use assign::{
    report_heading as assignment_report_heading, settle_report as settle_assignment_report,
};
pub(crate) use endpoint::{handle_gbots_http, GBOTS_ENDPOINT};
pub(crate) use feature::{chief_adds_on_its_own, gbots_off_message, gbots_on, gbots_shown};
pub(crate) use memory::{
    coordinator_remember as gbot_coordinator_remember, observe_activity as observe_gbot_activity,
};
pub(crate) use presentation::{
    insert_gbots_project_presentation, insert_gbots_session_presentation,
};
pub(crate) use record::name_problem;
pub(crate) use retire::retire_previous_gbots;
pub(crate) use routine_runs::{
    routine_folder, routine_gate, routine_run_settled, start_routine_run, RoutineGate,
    SettledRoutineRun,
};
pub(crate) use sweep::start_gbots_sweep;
