//! What a bot's turn gets beside its account, so a bot can do what a Claude or Codex session can:
//! its workspace's connector tools (every Composio and Executor app, with the same Ask/Allow/Never
//! rules and approval cards, shown in the bot's conversation), the workspace's `gh` and `gws`
//! sign-ins, and the Ghostex note, which sends Ghostex questions to `ghostex guide` instead of the
//! runtime's own screens.
//!
//! CDXC:GBots 2026-11-08 WHY:
//! A user asked a bot to read their Gmail; the bot had no connector tools (the runtime starts `claude` and `codex` itself, so the launch flags a session gets never reached it) and sent them to "GBots app/settings (Apps)", a screen of the runtime that Ghostex never shows. The user: "this isn't just about gmail man it's about all sorts of connectors/tools". The runtime spawns every engine through `ghostex gbots agent` (accounts.rs), so that wrapper adds what a session's launch adds. The conversation is named by the thread id the runtime hands its own agents server (`OMB_THREAD_ID`), so a card lands in the conversation that asked even while the bot works in several; the connector server alone gets it (`GHOSTEX_CONNECTORS_SESSION_REF`), leaving the bot's `ghostex` CLI to find its caller the GBots way. The runtime's own system prompt and Codex developer instructions are kept: the note joins them.
//! SEE-ALSO: server/src/ghostex_cli/gbots_cli.rs (`agent`, which applies this), server/src/connectors/agent/launch.rs, server/src/connectors/cli/launch.rs, server/src/agents/ghostex_note.rs.

use serde_json::{json, Value};

use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// Told to every bot after the runtime's own instructions: where the person really is, and that
/// Ghostex's guide, not the runtime's screens, answers how-to questions.
pub(crate) const GBOTS_GHOSTEX_NOTE: &str = "You are a bot in Ghostex's GBots, and the person talks to you in Ghostex's Bots space. The GBots screens other instructions mention (its apps, settings, pairing and routines pages) are not shown to them, so never send them there. For any question about how to set something up in Ghostex or where something is, run `ghostex guide`, `ghostex guide features` or `ghostex guide settings` first and answer with Ghostex's real screens and `ghostex` commands. Their apps (Gmail, Google Calendar, Slack, Notion, GitHub, Linear and many more) reach you through Ghostex Connectors, as the tools of the ghostex-connectors server; reads run at once and anything that changes something shows an approval card in this chat. If you have none of those tools or the app is not connected, tell them: turn on Settings > About > Experimental > Enable Experimental Features, then in Settings > Connectors sign in to Composio and connect the app (or run `ghostex connectors sign-in` and `ghostex connectors connect <app>`); a connected app reaches you within seconds.";

/// `agentArgv` beyond the account: `args` to add after the CLI's own, `env` for the CLI's
/// process, and `note`, the text to add to its system prompt.
pub(crate) fn turn_extras(
    state: &AppState,
    agent: &str,
    bot_id: &str,
    thread_id: &str,
    cwd: &str,
) -> Value {
    let mut args: Vec<String> = Vec::new();
    let mut env: Vec<(String, String)> = Vec::new();
    if let Some((project, session_id)) = conversation(state, bot_id, thread_id, cwd) {
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if let Some(connector_args) = crate::connectors::connector_tool_args(agent) {
            args.extend(connector_args);
            env.push((
                crate::connectors::CONNECTORS_SESSION_REF_ENV.to_string(),
                format!("{}:{project_id}:{session_id}", state.metadata.server_id),
            ));
        }
        if let Ok(db) = open_gxserver_database(&state.paths) {
            let sign_ins = crate::connectors::project_cli_env(&db, &project);
            // Codex hands its commands an environment of its own policy, as for a session.
            if agent == "codex" {
                for (name, value) in &sign_ins {
                    args.push("-c".to_string());
                    args.push(format!(
                        "shell_environment_policy.set.{name}={}",
                        serde_json::to_string(value).unwrap_or_default()
                    ));
                }
            }
            env.extend(sign_ins);
        }
    }
    let note = match crate::agents::ghostex_note_enabled(&state.paths) {
        true => format!(
            "{GBOTS_GHOSTEX_NOTE}\n\n{}",
            crate::agents::ghostex_note_text(crate::agents::agent_preview_browser(&state.paths))
        ),
        false => GBOTS_GHOSTEX_NOTE.to_string(),
    };
    json!({
        "args": args,
        "env": env
            .into_iter()
            .map(|(name, value)| (name, Value::String(value)))
            .collect::<serde_json::Map<String, Value>>(),
        "note": note,
    })
}

/// The bot's project and the session id of the conversation this turn serves: the thread the
/// runtime named, else the working conversation the CLI's caller lookup finds by folder.
fn conversation(
    state: &AppState,
    bot_id: &str,
    thread_id: &str,
    cwd: &str,
) -> Option<(Value, String)> {
    if !bot_id.is_empty() && !thread_id.is_empty() {
        if let Some((project, session)) =
            super::projects::conversation_session(state, bot_id, thread_id)
        {
            let session_id = session.get("sessionId")?.as_str()?.to_string();
            return Some((project, session_id));
        }
    }
    let caller = super::projects::calling_conversation(state, cwd)?;
    let project_id = caller.get("projectId")?.as_str()?;
    let session_id = caller.get("sessionId")?.as_str()?.to_string();
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = repository.get_project(project_id).ok()??;
    Some((project, session_id))
}
