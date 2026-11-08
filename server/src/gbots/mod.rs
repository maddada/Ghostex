//! GBots: Ghostex's bots, run by a pinned, renamed build of OpenMausBot's headless server (the
//! "sidecar") that gxserver starts, supervises and stops on the one computer the user picked.
//! Ghostex draws the bots itself: each bot is a bot project in the Bots space, each of its
//! conversations a session whose chat is the native chat, fed by a mirrored transcript.
//!
//! CDXC:GBots 2026-11-06 DECISION:
//! User (Q1500 to Q1504): build GBots on OpenMausBot, carried as a pinned sidecar "like how we ship cef and vscode updates so it's only downloaded if extension is enabled by the user", without its source-available `enterprise/` folder and renamed (its name is a trademark). "all of the bots always run on 1 machine but they can launch sub agents on different machines as they want (like we allow coordinator sessions to launch worker threads on any connected machine)", and they run on the user's own Claude Code and Codex.
//! WHY: Ghostex talks to the sidecar only through its REST API and event stream (`/api/events`), never its code, so upstream's pace (about 160 commits a day) stays outside Ghostex; the version is pinned in tooling/gbots/upstream.json.
//! SEE-ALSO: tooling/gbots/ (the runtime build), server/src/session_chat_decode_gbots.rs (the transcript), packages/gx-core/src/sidebar_view/inputs.rs (`BotsSource`), apps/desktop/src/app/native_sidebar/bots.rs (the Bots button).

mod accounts;
mod client;
mod create;
pub(crate) mod download;
mod endpoint;
mod events;
mod mirror;
mod projects;
pub(crate) mod runtime;
mod send;
pub(crate) mod settings;
pub(crate) mod sidecar;

pub(crate) use endpoint::{gbots_action, gbots_home_action};
pub(crate) use mirror::{
    resolve_gbots_chat_transcript_path, sync_gbots_transcript_mirror_for_path,
};
pub(crate) use projects::{gbots_agent_config, gbots_bot_id, published_bot_activity};
pub(crate) use send::{interrupt_gbots_session, send_chat_text};
#[allow(unused_imports)]
pub(crate) use settings::gbots_enabled;
pub(crate) use sidecar::{start_gbots_supervisor, stop_sidecar_now};

/// The agent id of a GBots conversation session.
pub(crate) const GBOTS_AGENT_ID: &str = "gbots";

/// The command a GBots session's terminal runs: Ghostex's GBots client
/// (`ghostex gbots attach`), by the absolute path every zmx session exports.
pub(crate) const GBOTS_ATTACH_COMMAND: &str = "\"${GHOSTEX_CLI_EXECUTABLE:-ghostex}\" gbots attach";

/// The `launchSettings` key that marks a GBots bot project and names its bot.
pub(crate) const GBOT_ID_KEY: &str = "gbotId";

/// The session's `runtimeSettings` key holding `{botId, threadId}`.
pub(crate) const GBOTS_SESSION_KEY: &str = "gbots";
