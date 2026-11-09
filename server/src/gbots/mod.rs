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
//! - [`day_watch`]: "keep an eye on my day", the Day Watcher GBot and its daily brief to Chief
//!   of Staff; [`apps_access`]: GBots that may only read the user's connected apps.
//! - [`problems`]: problems and escalations of GBot chats, reported to Chief of Staff
//!   (`ghostex gbots problem …`, `ghostex gbots flag`).
//! - [`team_file`]: GBots exported as one JSON team file and imported with a preview first
//!   (`ghostex gbots team …`), and the presets New GBot starts from (`ghostex gbots preset …`).
//! - [`api`]: the GBot API, an app or script messaging a GBot with its key (`/hooks/gbots/…`,
//!   `ghostex gbots api …`).
//! - [`computer`]: where a GBot's chats run (this computer, a connected computer or a Cloud Box)
//!   and computer use there.
//! - [`retire`]: the one-time removal of the previous GBots; [`trash`]: folders to the Trash.
//!
//! CDXC:GBots 2026-11-08 DECISION:
//! User: remove the previous GBots fully, "we are doing our own bots thing." The previous GBots, a downloaded runtime gxserver ran beside itself, is gone with its runtime download, transcript mirror, chat agent, Bots space source and settings; its bots and chats are not imported. Native GBots keep the name, the extension switch (`gbotsEnabled`), the `ghostex gbots` command and the `/api/gbots` route. Existing computers lose the old bots once, in retire.rs.

mod api;
mod apps_access;
mod assign;
mod assignments;
mod brief;
mod card;
mod changes;
mod computer;
mod day_watch;
mod department_ops;
mod departments;
mod endpoint;
mod feature;
mod folder;
mod markers;
mod memory;
mod presentation;
mod problems;
mod project;
mod record;
mod retire;
mod routine_runs;
mod routines;
mod session;
mod skill;
mod sweep;
mod team_file;
mod trash;

pub(crate) use api::{
    approval_subject as api_approval_subject, is_api_chat_session, opens_api, serve_bot_api,
    BOT_API_ROUTE_PREFIX,
};
pub(crate) use apps_access::{project_reads_apps_only, read_only_refusal};
pub(crate) use assign::{
    report_heading as assignment_report_heading, settle_report as settle_assignment_report,
};
pub(crate) use computer::{
    cloud_box_chat_approval as gbots_cloud_box_chat_approval, handle_gbots_remote_http,
    widening_approval as gbots_computer_widening_approval, GBOTS_REMOTE_ENDPOINT,
};
pub(crate) use day_watch::{
    adds_bot as day_watch_adds_bot, brief_turn_alerts as day_brief_turn_alerts,
};
pub(crate) use endpoint::{handle_gbots_http, GBOTS_ENDPOINT};
pub(crate) use feature::{chief_adds_on_its_own, gbots_off_message, gbots_on, gbots_shown};
pub(crate) use folder::gbots_dir as gbots_folder;
pub(crate) use markers::keep_gbot_marks;
pub(crate) use memory::{
    coordinator_remember as gbot_coordinator_remember, observe_activity as observe_gbot_activity,
};
pub(crate) use presentation::{
    insert_gbots_project_presentation, insert_gbots_session_presentation,
};
pub(crate) use problems::start_gbots_problem_watch;
pub(crate) use project::{
    add_create_project_path, gbots_bot_id as gbots_project_bot_id, refuse_bot_project_create,
};
pub(crate) use record::name_problem;
pub(crate) use retire::retire_previous_gbots;
pub(crate) use routine_runs::{
    routine_folder, routine_gate, routine_run_settled, start_routine_run, RoutineGate,
    SettledRoutineRun,
};
pub(crate) use skill::install_for_visiting_chat as install_gbots_skill_for_visiting_chat;
pub(crate) use sweep::start_gbots_sweep;
pub(crate) use team_file::import_approval_summary as team_import_approval_summary;
