//! `ghostex help ask "<question>"`: an agent asks a Ghostex Help session and gets its answer back.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::ghostex_cli::agents::delivery::{
    chat_shows, transcript_rows_before_send, TranscriptRows,
};
use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::sessions;

const USAGE: &str = "Usage: ghostex help ask \"<question>\" [--new] [--timeout-seconds N] [--json]

Ask a Ghostex Help session (your default prompt agent with the ghostex-help skill, in the Ghostex
project, like Ask Ghostex in the app's More menu) and print its answer once it finishes.
Later questions go to the same session while it is idle, so it keeps the conversation; --new starts
a fresh one. Waits up to 10 minutes unless --timeout-seconds says otherwise.

  ghostex help             Show the full CLI help";

const DEFAULT_TIMEOUT_SECONDS: f64 = 600.0;
/// How long one read waits for the chat to change before the next.
const LONG_POLL_WAIT_MS: u64 = 20_000;
const LONG_POLL_TIMEOUT_MS: u64 = 40_000;
const READ_LIMIT: usize = 40;
const SESSION_TITLE: &str = "Ghostex Help";

/// `ghostex help` prints the CLI help; `ghostex help ask …` asks Ghostex Help.
pub fn help_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("ask") => ask(&args[1..]),
        _ => {
            println!("{}", crate::ghostex_cli::usage::usage());
            Ok(())
        }
    }
}

/// CDXC:AgentGuide 2026-11-02 DECISION:
/// User (Q1070): agents reach Ghostex Help through a new `ghostex help ask "<question>"` that "starts (or reuses) a Ghostex Help session with /ghostex-help and returns its answer to the caller", following how Ask Ghostex in the ⋯ menu starts one: the default prompt agent, the Ghostex config folder's project, titled Ghostex Help (apps/desktop/src/app/gx_store/create/os_integration.rs `gx_store_create_ghostex_help_chat`). Unlike the menu, which stages an editable draft, the question is sent, and the CLI waits for the turn to end. The session is reused while it is idle and on the same agent, so follow-up questions keep their context; a busy one (another agent asking) gets a fresh session instead of a second message mid-turn.
/// SEE-ALSO: server/src/ghostex_cli/agent_guide.rs (which points agents here).
fn ask(args: &[String]) -> CliResult<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let question = parsed.rest.join(" ").trim().to_string();
    if question.is_empty() {
        return Err(CliError::Other(format!("Ask a question.\n\n{USAGE}")));
    }
    let timeout = Duration::from_secs_f64(
        flags
            .number("timeoutSeconds")
            .filter(|seconds| *seconds > 0.0)
            .unwrap_or(DEFAULT_TIMEOUT_SECONDS),
    );
    let agent = default_prompt_agent(&flags)?;
    let message = format!(
        "{} {question}",
        crate::session_chat_skill_invocation::skill_invocation(Some(&agent.family), "ghostex-help")
    );
    let reused = (!flags.truthy("new"))
        .then(|| idle_saved_session(&agent.id, &flags))
        .flatten();
    let (session, before, is_new) = match reused {
        Some(session) => {
            let before = transcript_rows_before_send(&session, &flags);
            send(
                &session,
                &message,
                "/api/sendSessionChatMessage",
                false,
                &flags,
            )?;
            (session, before, false)
        }
        None => {
            let session = create(&agent.id, &flags)?;
            // A new session has no transcript to read yet, so every row it gets is new.
            let before = TranscriptRows::Known(Default::default());
            send(
                &session,
                &message,
                "/api/queueSessionChatPrompt",
                true,
                &flags,
            )?;
            (session, before, true)
        }
    };
    let reference = text(&session, "globalRef").to_string();
    let answer = wait_for_answer(&session, &question, &before, timeout, &flags).ok_or_else(|| {
        CliError::Other(format!(
            "Ghostex Help ({reference}) hasn't answered within {} seconds. Read its answer later with `ghostex read-session-chat {reference} --limit 5`.",
            timeout.as_secs()
        ))
    })?;
    if flags.truthy("json") {
        print_json(
            &json!({"ok": true, "globalRef": reference, "newSession": is_new, "answer": answer}),
        );
    } else {
        println!("{answer}");
    }
    Ok(())
}

struct PromptAgent {
    id: String,
    /// The agent family, which decides how the skill is spelled (`/ghostex-help` for Claude).
    family: String,
}

/// The default prompt agent (Settings > Agents), which Ask Ghostex starts its chats on.
fn default_prompt_agent(flags: &Flags) -> CliResult<PromptAgent> {
    let hud = call_gxserver_rpc("/api/readSidebarHud", &json!({}), flags)?;
    let id = hud
        .pointer("/settings/defaultPromptAgentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(crate::agents::DEFAULT_PROMPT_AGENT_ID)
        .to_string();
    let agent = hud
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| agents.iter().find(|agent| text(agent, "agentId") == id))
        .ok_or_else(|| {
            CliError::Other(format!(
                "The default prompt agent \"{id}\" isn't set up. Choose one in Settings > Agents."
            ))
        })?;
    let family = crate::agents::infer_agent_id_from_command_executable(text(agent, "command"))
        .unwrap_or_else(|| id.clone());
    Ok(PromptAgent { id, family })
}

fn state_file() -> PathBuf {
    crate::paths::get_gxserver_paths(None)
        .root_dir
        .join("help-ask")
        .join("session.json")
}

/// The Help session the last ask used, when it is still listed, on the same agent, and idle.
fn idle_saved_session(agent_id: &str, flags: &Flags) -> Option<Value> {
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(state_file()).ok()?).ok()?;
    if text(&saved, "agentId") != agent_id {
        return None;
    }
    let reference = text(&saved, "globalRef");
    let session = sessions::fetch_session_list(flags, false)
        .ok()?
        .into_iter()
        .find(|session| text(session, "globalRef") == reference)?;
    (text(&session, "activity") != "working").then_some(session)
}

fn save_session(session: &Value, agent_id: &str) {
    let path = state_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let record = json!({
        "globalRef": session["globalRef"], "projectId": session["projectId"],
        "sessionId": session["sessionId"], "agentId": agent_id,
    });
    let _ = std::fs::write(path, record.to_string());
}

/// A new Ghostex Help session in the Ghostex config folder's project, started like `ghostex
/// agents create` starts one.
fn create(agent_id: &str, flags: &Flags) -> CliResult<Value> {
    let config_dir = crate::paths::get_gxserver_paths(None).app_config_dir;
    let project = call_gxserver_rpc(
        "/api/addProjectPath",
        &json!({"name": "Ghostex", "path": config_dir.to_string_lossy()}),
        flags,
    )?;
    let project_id = project
        .pointer("/project/projectId")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::Other("Ghostex couldn't open its Ghostex project.".into()))?;
    let created = call_gxserver_rpc(
        "/api/createAgentSession",
        &json!({"projectId": project_id, "agentId": agent_id, "title": SESSION_TITLE}),
        flags,
    )?;
    let session = created
        .get("session")
        .filter(|session| !text(session, "globalRef").is_empty())
        .cloned()
        .ok_or_else(|| CliError::Other("Ghostex couldn't start a Ghostex Help session.".into()))?;
    let reference = text(&session, "globalRef").to_string();
    call_gxserver_rpc(
        "/api/startSessionProvider",
        &json!({"globalRef": reference, "projectId": session["projectId"], "sessionId": session["sessionId"]}),
        flags,
    )
    .map_err(|error| {
        CliError::Other(format!(
            "Started Ghostex Help ({reference}), but its agent didn't start: {error}"
        ))
    })?;
    save_session(&session, agent_id);
    Ok(session)
}

fn send(
    session: &Value,
    message: &str,
    endpoint: &str,
    startup: bool,
    flags: &Flags,
) -> CliResult<()> {
    let mut payload = json!({
        "globalRef": session["globalRef"], "projectId": session["projectId"], "sessionId": session["sessionId"],
        "text": message, "sendRequestId": uuid::Uuid::new_v4().to_string(),
    });
    if startup {
        payload["startupSend"] = json!(true);
    }
    call_gxserver_rpc(endpoint, &payload, flags)
        .map(|_| ())
        .map_err(|error| {
            CliError::Other(format!(
                "Couldn't ask Ghostex Help ({}): {error}",
                text(session, "globalRef")
            ))
        })
}

/// Long-polls the Help session's chat until the question shows in its transcript and the turn
/// after it has ended, then returns that turn's text. The match is on the question alone: Claude's
/// transcript records a skill command apart from its arguments.
fn wait_for_answer(
    session: &Value,
    question: &str,
    before: &TranscriptRows,
    timeout: Duration,
    flags: &Flags,
) -> Option<String> {
    let started = Instant::now();
    let mut poll_flags = flags.clone();
    poll_flags.insert_text("timeout", &LONG_POLL_TIMEOUT_MS.to_string());
    let mut fingerprint: Option<String> = None;
    while started.elapsed() < timeout {
        let mut params = json!({
            "globalRef": session["globalRef"], "projectId": session["projectId"], "sessionId": session["sessionId"],
            "limit": READ_LIMIT,
        });
        if let Some(fingerprint) = &fingerprint {
            params["fingerprint"] = json!(fingerprint);
            params["waitMs"] = json!(LONG_POLL_WAIT_MS);
        }
        let Ok(chat) = call_gxserver_rpc("/api/readSessionChat", &params, &poll_flags) else {
            // A restart or a dropped connection: try again until the deadline.
            std::thread::sleep(Duration::from_secs(2));
            fingerprint = None;
            continue;
        };
        if chat["working"] != json!(true) && chat_shows(&chat, question, before) {
            if let Some(answer) = answer_after_question(&chat, question) {
                return Some(answer);
            }
        }
        let next = chat
            .get("fingerprint")
            .and_then(Value::as_str)
            .map(str::to_string);
        if next.is_none() || next == fingerprint {
            std::thread::sleep(Duration::from_secs(1));
        }
        fingerprint = next;
    }
    None
}

/// The text of the assistant rows after the newest user row holding `message`, once the last
/// row is the assistant's.
pub(crate) fn answer_after_question(chat: &Value, message: &str) -> Option<String> {
    let rows = chat.get("messages")?.as_array()?;
    let needles = crate::coordinators::delivery_needles(message);
    let asked = rows.iter().rposition(|row| {
        text(row, "role") == "user"
            && crate::coordinators::holds_message(
                &row_text(row)
                    .lines()
                    .map(crate::coordinators::normalize_delivery_text)
                    .collect::<String>(),
                &needles,
            )
    })?;
    let after = &rows[asked + 1..];
    if after.last().map(|row| text(row, "role")) != Some("assistant") {
        return None;
    }
    let answer = after
        .iter()
        .filter(|row| text(row, "role") == "assistant")
        .map(row_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!answer.is_empty()).then_some(answer)
}

/// A row's text blocks, without its tool calls.
fn row_text(row: &Value) -> String {
    row.get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(role: &str, text: &str) -> Value {
        json!({"id": text, "role": role, "source": "transcript", "blocks": [{"type": "text", "text": text}]})
    }

    #[test]
    fn the_answer_is_the_turn_after_the_question() {
        let question = "/ghostex-help How do I turn off the sound?";
        let chat = json!({"messages": [
            row("user", "/ghostex-help An older question"),
            row("assistant", "An older answer"),
            row("user", question),
            row("assistant", "Checking the settings."),
            json!({"id": "t", "role": "assistant", "blocks": [{"type": "tool-call", "name": "Bash"}]}),
            row("assistant", "Set completionSound to off."),
        ]});
        assert_eq!(
            answer_after_question(&chat, question).as_deref(),
            Some("Checking the settings.\n\nSet completionSound to off.")
        );
    }

    #[test]
    fn no_answer_until_the_assistant_replies() {
        let question = "$ghostex-help Where is the board?";
        let chat = json!({"messages": [row("user", question)]});
        assert_eq!(answer_after_question(&chat, question), None);
    }
}
