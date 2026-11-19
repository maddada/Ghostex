use crate::bot_projects;
use crate::ghostex_cli::{
    args::Flags,
    rpc::{self, CliError, CliResult},
    sessions,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub(crate) fn text<'a>(session: &'a Value, key: &str) -> &'a str {
    session
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
}

pub(crate) fn is_agent(session: &Value) -> bool {
    !text(session, "agentId").is_empty()
}

pub(crate) fn inventory_flags(flags: &Flags, reference: &str) -> CliResult<Flags> {
    let mut flags = flags.clone();
    if rpc::is_gxserver_global_session_ref(reference) {
        let target = rpc::resolve_gxserver_server_target(&flags, &json!({"globalRef": reference}))?;
        if target.kind == "local" {
            flags.insert_text("server", "local");
        } else if target.kind == "computer" {
            if let Some(server_id) = &target.server_id {
                flags.0.remove("server");
                flags.insert_text("computer", server_id);
            }
        } else if let Some(profile) = target.profile_id {
            flags.insert_text("server", &profile);
        } else {
            return Err(CliError::Other(format!(
                "No connection profile for {reference}."
            )));
        }
    }
    Ok(flags)
}

/// CDXC:SessionIdentity 2026-09-17 DECISION:
/// User: agents must obtain their own session and agent identifiers from the CLI for message headers. Resolve exact environment identifiers, never the focused pane or a matching title.
///
/// CDXC:Coordinators 2026-10-06 WHY:
/// Empryo 3.9.0-beta's shell tool runs commands without any GHOSTEX_* or ZMX_* variable (probed 2026-10-06: `env` printed none of them), so an Empryo coordinator or thread running `ghostex coordinator status` or `ghostex agents send` had no identity. With none of the variables set, the caller is read from the process tree the way Empryo's hooks find their session: the `zmx run` daemon above the `empryo` process names it. Anything else still fails rather than guessing.
pub(crate) fn caller() -> CliResult<Value> {
    match lookup_caller()? {
        CallerLookup::Agent(caller) => Ok(caller),
        CallerLookup::NoSession => Err(CliError::Other("Cannot identify the caller. Run inside a Ghostex agent session with GHOSTEX_GLOBAL_SESSION_REF or GHOSTEX_SESSION_ID set.".into())),
        CallerLookup::NotOneAgent { key, reference } | CallerLookup::PlainSession { key, reference } => Err(CliError::Other(format!("Cannot identify one agent session from {key}={reference}. Run ghostex sessions --json to inspect the session; identity was not guessed."))),
    }
}

/// CDXC:GrokBot 2026-11-19 DECISION:
/// User's report: Grok Bot runs `ghostex` on the Mac through its desktop app, in a plain shell outside any Ghostex session, and `ghostex agents send <ref> "<text>"` failed with "Cannot identify the caller", so the bot could not pass the user's answers on. The coordinator: a send from outside any session, with the token file (the user's own terminal), goes as the user, the same as the user typing into that chat; who may send otherwise is unchanged (gxserver checks the caller as before). Only "no session at all" counts as the user: a session variable that names no single agent session still fails rather than guessing.
/// CDXC:GrokBot 2026-11-19 DECISION (security review of 38bfa3647): "can't tell who I am" is never "I am the user": an agent that lost its session variables would otherwise speak as the user, its "W7 Q12: …" lines counting as the user's answers and a `/` or `!` body running as a command. Sending as the user takes `--as-user`, refused when a session variable, a session token or a Ghostex session pane above the command is present; without it the old "Cannot identify the caller" error stands.
/// Who a send is from.
pub(crate) enum Sender {
    /// The calling agent session (its sender block goes with the message).
    Agent(Value),
    /// The user, from their own terminal (`terminal`) or Grok Bot on this computer (`grokBot`).
    User { via: &'static str },
}

/// The sender of an `agents send`: the calling agent session, or the user when `as_user`.
pub(crate) fn sender(as_user: bool) -> CliResult<Sender> {
    if !as_user {
        return caller().map(Sender::Agent);
    }
    let refuse = |why: String| {
        CliError::Other(format!(
            "--as-user is for your own terminal: {why}, so this command can't send as the user. An agent sends as itself: drop --as-user."
        ))
    };
    if let Some(key) = SESSION_VARIABLES
        .iter()
        .chain([crate::security::SESSION_TOKEN_ENV].iter())
        .find(|key| std::env::var(key).is_ok_and(|value| !value.trim().is_empty()))
    {
        return Err(refuse(format!("{key} is set")));
    }
    let ancestry = crate::agent_hooks::caller_ancestry();
    if ancestry.in_session_pane {
        return Err(refuse("it runs inside a Ghostex session".to_string()));
    }
    Ok(Sender::User {
        via: if ancestry.grok_bot {
            "grokBot"
        } else {
            "terminal"
        },
    })
}

/// The session variables [`lookup_caller`] reads, in its order.
const SESSION_VARIABLES: [&str; 4] = [
    "GHOSTEX_GLOBAL_SESSION_REF",
    "GHOSTEX_NATIVE_SESSION_ID",
    "GHOSTEX_SESSION_ID",
    "ZMX_SESSION",
];

/// Who runs a raw verb (the raw verbs' sender block).
pub(crate) enum RawCaller {
    /// An agent session: its block goes with the message.
    Agent(Value),
    /// A plain (non-agent) Ghostex terminal pane, named by its variable: the user, as before.
    PlainPane,
    /// No session variable at all: the user's own terminal, or an agent that dropped its
    /// variables, which outbound.rs tells apart by the pane above.
    NoSession,
}

/// The raw verbs' caller. A variable that names no session, or several, is an error, never "the
/// user"; one that names a single plain terminal pane is the person at it.
pub(crate) fn raw_caller() -> CliResult<RawCaller> {
    match lookup_caller()? {
        CallerLookup::Agent(caller) => Ok(RawCaller::Agent(caller)),
        CallerLookup::PlainSession { .. } => Ok(RawCaller::PlainPane),
        CallerLookup::NoSession => Ok(RawCaller::NoSession),
        CallerLookup::NotOneAgent { .. } => caller().map(RawCaller::Agent),
    }
}

/// The calling agent session, or `None` when the command does not run inside one (a person's own
/// terminal, a plain shell pane, the phone over SSH).
pub(crate) fn agent_caller() -> CliResult<Option<Value>> {
    Ok(match lookup_caller()? {
        CallerLookup::Agent(caller) => Some(caller),
        CallerLookup::NoSession
        | CallerLookup::NotOneAgent { .. }
        | CallerLookup::PlainSession { .. } => None,
    })
}

enum CallerLookup {
    NoSession,
    /// The variable names exactly one live session, a plain terminal pane rather than an agent:
    /// the person at that pane, who sends as the user (CDXC:Coordinators 2026-10-24 in
    /// outbound.rs).
    PlainSession {
        key: &'static str,
        reference: String,
    },
    NotOneAgent {
        key: &'static str,
        reference: String,
    },
    Agent(Value),
}

fn lookup_caller() -> CliResult<CallerLookup> {
    let from_environment = || {
        [
            "GHOSTEX_GLOBAL_SESSION_REF",
            "GHOSTEX_NATIVE_SESSION_ID",
            "GHOSTEX_SESSION_ID",
            "ZMX_SESSION",
        ]
        .into_iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(|value| (key, value.trim().to_owned()))
        })
    };
    // A session the environment lacks was already read from the process tree as the CLI started
    // (ghostex_cli/mod.rs `adopt_caller_session_from_process_tree`).
    let Some((key, reference)) = from_environment() else {
        return Ok(CallerLookup::NoSession);
    };
    let flags = inventory_flags(&Flags::default(), &reference)?;
    // A reference that names the session's project and id reads that one row
    // (`fetch_live_gxserver_session_list_for`); the others match on fields only the full list has.
    let ids = reference.split(':').collect::<Vec<_>>();
    let only = match (key, ids.as_slice()) {
        ("GHOSTEX_GLOBAL_SESSION_REF", [_, project_id, session_id])
        | ("GHOSTEX_NATIVE_SESSION_ID", [project_id, session_id]) => {
            Some(vec![(project_id.to_string(), session_id.to_string())])
        }
        _ => None,
    };
    let rows = live_session_rows(&flags, only.as_deref())?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|row| {
            if key == "GHOSTEX_GLOBAL_SESSION_REF" {
                text(row, "globalRef") == reference
            } else if key == "GHOSTEX_NATIVE_SESSION_ID" {
                format!("{}:{}", text(row, "projectId"), text(row, "sessionId")) == reference
            } else {
                ["sessionId", "globalRef", "providerSessionName"]
                    .iter()
                    .any(|field| text(row, field) == reference)
            }
        })
        .collect();
    if matches.len() == 1 && !is_agent(matches[0]) {
        return Ok(CallerLookup::PlainSession { key, reference });
    }
    if matches.len() != 1 || text(matches[0], "globalRef").is_empty() {
        return Ok(CallerLookup::NotOneAgent { key, reference });
    }
    let mut caller = matches[0].clone();
    resolve_names(std::slice::from_mut(&mut caller), &flags);
    Ok(CallerLookup::Agent(caller))
}

/// How long the caller lookup waits for gxserver to answer, which covers a gxserver restart.
const CALLER_LIVE_WAIT: Duration = Duration::from_secs(20);
const CALLER_LIVE_POLL: Duration = Duration::from_millis(500);

/// CDXC:Cli 2026-10-05 WHY:
/// The session list falls back to gxserver's persisted state when gxserver does not answer, and those rows carry no agent session id and no launcher name. A coordinator that messaged its threads right after a gxserver restart sent `Agent: claude` and `Agent Session ID: unavailable` (observed 2026-10-05, coordinator G4snt), while its own record still had the id. The caller's identity is therefore read only from the running gxserver, waiting out a restart; a message cannot be sent without gxserver anyway, so a gxserver that stays down fails the command instead of sending a header with missing identity.
/// SEE-ALSO: server/src/ghostex_cli/sessions/persisted.rs (the fallback rows), `caller` above.
fn live_session_rows(flags: &Flags, only: Option<&[(String, String)]>) -> CliResult<Vec<Value>> {
    let deadline = Instant::now() + CALLER_LIVE_WAIT;
    loop {
        match sessions::fetch_live_gxserver_session_list_for(flags, only) {
            Ok(result) => {
                return Ok(result
                    .get("sessions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default())
            }
            Err(error) if Instant::now() >= deadline => {
                return Err(CliError::Other(format!(
                    "gxserver did not answer, so your own identity for the message header could not be read: {error} It may be restarting; try again in a few seconds."
                )))
            }
            Err(_) => std::thread::sleep(CALLER_LIVE_POLL),
        }
    }
}

pub(crate) fn resolve_names(rows: &mut [Value], flags: &Flags) {
    if let Ok(hud) = rpc::call_gxserver_rpc("/api/readSidebarHud", &json!({}), flags) {
        if let Some(agents) = hud["agents"].as_array() {
            for row in rows.iter_mut() {
                if let Some(agent) = agents
                    .iter()
                    .find(|agent| text(agent, "agentId") == text(row, "agentId"))
                {
                    if !text(agent, "name").is_empty() {
                        row["agentName"] = agent["name"].clone();
                    }
                }
            }
        }
    }
    name_bot_sessions(rows, flags);
}

/// CDXC:Bots 2026-09-27 WHY:
/// Every bot session runs the built-in `hermes-agent`, so the roster lookup names it after whichever roster agent owns that id ("Harry" for a Dobby session). A session in a bot project is that bot, named as `bot_agent_config` names its launch. gxserver keeps publishing the agent id as the row's `agentName` because clients look transcripts up by it.
fn name_bot_sessions(rows: &mut [Value], flags: &Flags) {
    if !rows
        .iter()
        .any(|row| text(row, "agentId") == "hermes-agent")
    {
        return;
    }
    let Ok(response) = rpc::call_gxserver_rpc("/api/listProjects", &json!({}), flags) else {
        return;
    };
    let Some(projects) = response["projects"].as_array() else {
        return;
    };
    for row in rows {
        if let Some(name) = projects
            .iter()
            .find(|project| text(project, "projectId") == text(row, "projectId"))
            .and_then(|project| bot_projects::bot_agent_config(project, text(row, "agentId")))
            .and_then(|config| config.get("name").cloned())
        {
            row["agentName"] = name;
        }
    }
}

pub(crate) fn summary(row: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in [
        "globalRef",
        "sessionId",
        "title",
        "projectId",
        "projectName",
        "projectPath",
        "agentName",
        "agentId",
        "agentSessionId",
        "activity",
        "lifecycleState",
    ] {
        result.insert(key.to_owned(), row.get(key).cloned().unwrap_or(Value::Null));
    }
    if text(row, "agentName").is_empty() {
        result.insert("agentName".into(), json!(text(row, "agentId")));
    }
    Value::Object(result)
}

fn header_value(row: &Value, key: &str) -> String {
    let value = text(row, key);
    if value.is_empty() {
        return "unavailable".into();
    }
    value
        .chars()
        .map(|c| {
            if c.is_control() || c == '\u{2028}' || c == '\u{2029}' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// CDXC:Cli 2026-09-17 DECISION:
/// User: attach the sender's CLI-resolved identity to every agent message. Assemble the block before enqueueing so delayed delivery retains the original sender.
/// CDXC:Cli 2026-09-18 DECISION:
/// User: the block must not render as a heading. The old `MESSAGE FROM` header ended in a dashed line, which Markdown reads as a setext underline, so the chat turned the whole header into an h2. A blank line now separates the block from the body.
/// CDXC:Cli 2026-10-05 DECISION:
/// User: move the block BELOW the body (identity stays in every message), so Claude and Codex title the session from the task text instead of the sender's `Session:` title. Supersedes the 2026-09-17 placement at the top.
/// CDXC:Cli 2026-10-05 DECISION:
/// User: a body that starts with `/` or `!` (after leading whitespace) keeps the block FIRST, as before, so the receiving agent never runs the body's first line as a slash or shell command.
/// SEE-ALSO: packages/gx-chat-core/src/transcript/agent_message.rs parses the block in both positions (and the old dashed one) into the chat's message card; server/src/coordinators/brief.rs `agent_message` writes the same block.
pub(crate) fn message(sender: &Value, body: &str) -> String {
    let sender = summary(sender);
    let block = format!("Message from another agent\nAgent: {}\nSession: {}\nSession ID: {}\nAgent ID: {}\nAgent Session ID: {}\nReply to: {}",
        header_value(&sender, "agentName"), header_value(&sender, "title"), header_value(&sender, "sessionId"),
        header_value(&sender, "agentId"), header_value(&sender, "agentSessionId"), header_value(&sender, "globalRef"));
    crate::coordinators::place_agent_message_block(&block, body)
}
