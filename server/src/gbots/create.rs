//! Making a bot: a name, its standing instructions and the agent it runs on (the user's own
//! Claude Code or Codex).
//!
//! CDXC:GBots 2026-11-06 DECISION:
//! User: every bot gets the `ghostex` CLI and its guidance, so it can start, message and read sessions on any connected computer (`--computer`), "like we allow coordinator sessions to launch worker threads on any connected machine". The sidecar puts this gxserver's `ghostex` first on the bots' PATH with the session variables the CLI needs (sidecar.rs).
//! WHY: the guidance is the start of the bot's standing instructions rather than a runtime skill, because the runtime installs skills only from GitHub URLs and Codex bots read no Claude skills; the user's own instructions follow it unchanged.

use serde_json::{json, Value};

use crate::server::AppState;

/// The agents a bot can run on: Ghostex's id, the runtime's engine id, and the name shown.
pub(crate) const BOT_AGENTS: [(&str, &str, &str); 2] = [
    ("claude", "claude", "Claude Code"),
    ("codex", "codex", "Codex"),
];

const GHOSTEX_GUIDANCE: &str = "## Ghostex\n\
You are a GBot: a bot that runs inside Ghostex on the user's computer. The `ghostex` CLI is on your PATH. Use it to start, message and read agent sessions (Claude Code, Codex and the user's other agents) on this computer and on every computer connected to it, the way a Ghostex coordinator starts its threads. Read `ghostex agents --help` before you start; it is the source of truth for every command.\n\
- `ghostex computers --json` lists the connected computers. Add `--computer <name>` to any session command to act on that computer.\n\
- `ghostex sessions --json` (with `--computer <name>` for another computer) lists the sessions with their project's id (`projectId`) and folder. To work in a folder that is not a Ghostex project yet, `ghostex add-project <path> --json` adds it and prints its id.\n\
- `ghostex agents create <agent-id> --project-id <id> --title \"<title>\" --task \"<task>\" --standalone` starts a session; keep the `globalRef` it prints. `ghostex agents types --json` lists the agent ids.\n\
- `ghostex agents send <globalRef> \"<text>\"` messages it, `ghostex read-session-chat <globalRef> --limit 20 --json` reads its replies, and `ghostex agents close <globalRef>` closes it when the work is done.\n\
If `ghostex` is missing or does not know a command, run the same CLI as `\"$GHOSTEX_CLI_EXECUTABLE\"`, the one this Ghostex ships.\n\
Only message the user's own sessions when they ask you to.\n";

fn text_param<'a>(params: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// `create {name, instructions?, agent: claude|codex}` → `{botId, projectId}`.
pub(crate) fn create_bot(
    state: &AppState,
    params: &serde_json::Map<String, Value>,
) -> Result<Value, String> {
    let name = text_param(params, "name").ok_or("Give the bot a name.")?;
    if name.chars().count() > 40 {
        return Err("A bot's name is at most 40 characters.".to_string());
    }
    let agent = text_param(params, "agent").unwrap_or("claude");
    let (_, engine, agent_name) = BOT_AGENTS
        .iter()
        .find(|(id, _, _)| *id == agent)
        .ok_or("The agent must be claude or codex.")?;
    let instructions = text_param(params, "instructions").unwrap_or_default();
    let model = engine_default_model(engine).ok_or_else(|| {
        format!("{agent_name} is not ready for GBots on this computer. Install it and sign in, then try again.")
    })?;
    let created = super::client::post(
        "/api/bots",
        &json!({ "name": name, "modelSelection": { "instanceId": engine, "model": model } }),
    )?;
    let bot_id = created
        .pointer("/bot/id")
        .and_then(Value::as_str)
        .ok_or("GBots did not make the bot.")?
        .to_string();
    let soul = if instructions.is_empty() {
        GHOSTEX_GUIDANCE.to_string()
    } else {
        format!("{GHOSTEX_GUIDANCE}\n{instructions}\n")
    };
    super::client::send(
        "PATCH",
        &format!("/api/bots/{}", super::client::id_segment(&bot_id)?),
        &json!({ "soul": soul }),
    )?;
    super::projects::full_sync(state);
    let project_id = super::projects::project_for_bot(state, &bot_id);
    Ok(json!({ "botId": bot_id, "projectId": project_id, "agent": agent }))
}

/// The engine's default model when it is installed and signed in.
fn engine_default_model(engine: &str) -> Option<String> {
    let answer = super::client::get("/api/instances").ok()?;
    let instance = answer
        .get("instances")?
        .as_array()?
        .iter()
        .find(|instance| instance.get("instanceId").and_then(Value::as_str) == Some(engine))?;
    if instance.pointer("/snapshot/state").and_then(Value::as_str) != Some("available") {
        return None;
    }
    instance
        .pointer("/models/default")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
}

/// The agents a bot can run on, and whether each is ready here.
pub(crate) fn bot_agents() -> Value {
    let instances = super::client::get("/api/instances").ok();
    Value::Array(
        BOT_AGENTS
            .iter()
            .map(|(id, engine, name)| {
                let ready = instances
                    .as_ref()
                    .and_then(|answer| answer.get("instances")?.as_array().cloned())
                    .into_iter()
                    .flatten()
                    .any(|instance| {
                        instance.get("instanceId").and_then(Value::as_str) == Some(engine)
                            && instance.pointer("/snapshot/state").and_then(Value::as_str)
                                == Some("available")
                    });
                json!({ "id": id, "name": name, "ready": ready })
            })
            .collect(),
    )
}
