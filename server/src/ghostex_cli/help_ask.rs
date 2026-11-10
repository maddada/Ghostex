//! `ghostex help ask "<question>"`: an agent asks a Ghostex Help session and gets its answer back.

use std::path::{Path, PathBuf};
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
Later questions go to the same session while it is idle, so it keeps the conversation; asking
again while your earlier question is still being answered waits for that answer instead of starting
another session. --new starts a fresh one. Waits up to 10 minutes unless --timeout-seconds says
otherwise. A Ghostex Help session can't ask Ghostex Help.

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
    refuse_inside_help()?;
    let timeout = Duration::from_secs_f64(
        flags
            .number("timeoutSeconds")
            .filter(|seconds| *seconds > 0.0)
            .unwrap_or(DEFAULT_TIMEOUT_SECONDS),
    );
    let deadline = Instant::now() + timeout;
    let agent = default_prompt_agent(&flags)?;
    let message = format!(
        "{} {question}",
        crate::session_chat_skill_invocation::skill_invocation(Some(&agent.family), "ghostex-help")
    );
    let asker = crate::ghostex_cli::rpc::caller_session_from_env().unwrap_or_default();
    let mut ledger = AskLedger::read();
    let listed = (!flags.truthy("new"))
        .then(|| sessions::fetch_session_list(&flags, false).ok())
        .flatten()
        .unwrap_or_default();
    let session_of = |ask: &Ask| {
        listed
            .iter()
            .find(|session| text(session, "globalRef") == ask.global_ref)
            .cloned()
    };
    let own = ledger
        .newest_for(&asker, &agent.id)
        .and_then(|ask| Some((ask.clone(), session_of(ask)?)));
    let mut wait_for = question.clone();
    let (session, before, is_new) = match own {
        // This asker's earlier ask of the same question (its wait timed out, or the command was
        // stopped): its answer when it has one, else wait for it instead of asking again. Only an
        // ask that went quiet long ago is sent again.
        Some((ask, session)) if same_question(&ask.question, &question) => {
            if let Some(answer) = read_answer(&session, &ask.question, &flags) {
                return print_answer(&session, &answer, false, &flags);
            }
            if is_working(&session) || now_ms() - ask.asked_at_ms < STALE_ASK_MS {
                wait_for = ask.question;
                (session, TranscriptRows::Known(Default::default()), false)
            } else {
                let before = transcript_rows_before_send(&session, &flags);
                send(
                    &session,
                    &message,
                    "/api/sendSessionChatMessage",
                    false,
                    &flags,
                )?;
                ledger.record(&asker, &agent.id, &session, &question, false);
                (session, before, false)
            }
        }
        Some((_, session)) => {
            // Its own Help chat is busy with an earlier question: the next one waits its turn.
            if is_working(&session) && !wait_until_idle(&session, deadline, &flags) {
                return Err(not_answered(&session, timeout));
            }
            let before = transcript_rows_before_send(&session, &flags);
            send(
                &session,
                &message,
                "/api/sendSessionChatMessage",
                false,
                &flags,
            )?;
            ledger.record(&asker, &agent.id, &session, &question, false);
            (session, before, false)
        }
        None => match ledger
            .newest_idle(&agent.id, &session_of)
            .filter(|_| !flags.truthy("new"))
        {
            Some(session) => {
                let before = transcript_rows_before_send(&session, &flags);
                send(
                    &session,
                    &message,
                    "/api/sendSessionChatMessage",
                    false,
                    &flags,
                )?;
                ledger.record(&asker, &agent.id, &session, &question, false);
                (session, before, false)
            }
            None => {
                ledger.refuse_over_cap(&asker)?;
                let session = create(&agent.id, &asker, &question, &mut ledger, &flags)?;
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
        },
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    let answer = wait_for_answer(&session, &wait_for, &before, remaining, &flags)
        .ok_or_else(|| not_answered(&session, timeout))?;
    print_answer(&session, &answer, is_new, &flags)
}

fn print_answer(session: &Value, answer: &str, is_new: bool, flags: &Flags) -> CliResult<()> {
    if flags.truthy("json") {
        print_json(
            &json!({"ok": true, "globalRef": text(session, "globalRef"), "newSession": is_new, "answer": answer}),
        );
    } else {
        println!("{answer}");
    }
    Ok(())
}

fn not_answered(session: &Value, timeout: Duration) -> CliError {
    let reference = text(session, "globalRef");
    CliError::Other(format!(
        "Ghostex Help ({reference}) hasn't answered within {} seconds. Read its answer later with `ghostex read-session-chat {reference} --limit 5`; asking the same question again waits for this answer instead of starting another chat.",
        timeout.as_secs()
    ))
}

/// CDXC:AgentGuide 2026-11-10 WHY:
/// A Ghostex Help chat that couldn't find an answer ran `ghostex help ask` itself (the Ghostex note sends every agent asked about Ghostex to `ghostex agent-guide`, which ends with "Still unsure: `ghostex help ask`"); its own chat was busy, so a new one started and did the same. A Windows user's one question about a GBot's icon filled the "Ghostex" project with about ten Codex chats on 9 Nov, and a test gxserver made 16 from one question. Help never asks Help: it answers from the guide, or says the guide doesn't cover it.
/// SEE-ALSO: server/src/agents/turn_create_cap.rs (the per-turn limit gxserver keeps behind this).
fn refuse_inside_help() -> CliResult<()> {
    let paths = crate::paths::get_gxserver_paths(None);
    let in_help = |folder: &str| {
        !folder.is_empty()
            && crate::agents::turn_create_cap::is_ghostex_help_folder(&paths, Path::new(folder))
    };
    // The calling session's own row (any session: an agent, or a terminal in the Help project).
    let caller_in_help = crate::ghostex_cli::rpc::caller_session_from_env().is_some_and(|caller| {
        sessions::fetch_session_list(&Flags::default(), false)
            .unwrap_or_default()
            .iter()
            .find(|session| {
                ["globalRef", "sessionId", "zmxName"]
                    .iter()
                    .any(|key| text(session, key) == caller)
            })
            .is_some_and(|session| in_help(text(session, "projectPath")))
    });
    let cwd_in_help = std::env::current_dir()
        .ok()
        .is_some_and(|dir| in_help(&dir.to_string_lossy()));
    if caller_in_help || cwd_in_help {
        return Err(CliError::Other("You are Ghostex Help, so `ghostex help ask` would only start another Ghostex Help chat. Answer from the ghostex-help skill (`ghostex guide`, `ghostex guide features`, `ghostex settings get <key>`); if it doesn't cover the question, say so plainly.".into()));
    }
    Ok(())
}

fn is_working(session: &Value) -> bool {
    text(session, "activity") == "working"
}

/// The same question, give or take case, spacing and punctuation.
fn same_question(left: &str, right: &str) -> bool {
    let words = |text: &str| {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    };
    words(left) == words(right)
}

/// The answer the session already gave to `question`, when its turn has ended.
fn read_answer(session: &Value, question: &str, flags: &Flags) -> Option<String> {
    let chat = call_gxserver_rpc(
        "/api/readSessionChat",
        &json!({
            "globalRef": session["globalRef"], "projectId": session["projectId"], "sessionId": session["sessionId"],
            "limit": READ_LIMIT,
        }),
        flags,
    )
    .ok()?;
    (chat["working"] != json!(true))
        .then(|| answer_after_question(&chat, question))
        .flatten()
}

/// Waits until the session's turn ends, or the deadline passes (false).
fn wait_until_idle(session: &Value, deadline: Instant, flags: &Flags) -> bool {
    let mut poll_flags = flags.clone();
    poll_flags.insert_text("timeout", &LONG_POLL_TIMEOUT_MS.to_string());
    let mut fingerprint: Option<String> = None;
    while Instant::now() < deadline {
        let mut params = json!({
            "globalRef": session["globalRef"], "projectId": session["projectId"], "sessionId": session["sessionId"],
            "limit": 1,
        });
        if let Some(fingerprint) = &fingerprint {
            params["fingerprint"] = json!(fingerprint);
            params["waitMs"] = json!(LONG_POLL_WAIT_MS);
        }
        match call_gxserver_rpc("/api/readSessionChat", &params, &poll_flags) {
            Ok(chat) if chat["working"] != json!(true) => return true,
            Ok(chat) => {
                let next = chat
                    .get("fingerprint")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if next.is_none() || next == fingerprint {
                    std::thread::sleep(Duration::from_secs(1));
                }
                fingerprint = next;
            }
            Err(_) => {
                std::thread::sleep(Duration::from_secs(2));
                fingerprint = None;
            }
        }
    }
    false
}

/// How long an asker keeps its own Help chat: later asks within it go to that chat.
const OWN_CHAT_WINDOW_MS: i64 = 30 * 60 * 1000;
/// At most this many new Help chats for one asker within [`CREATE_WINDOW_MS`].
const CREATES_PER_ASKER: usize = 3;
const CREATE_WINDOW_MS: i64 = 10 * 60 * 1000;
/// An unanswered ask this old, on a chat that isn't working, is sent again.
const STALE_ASK_MS: i64 = 10 * 60 * 1000;
/// Asks older than this are forgotten.
const KEEP_MS: i64 = 24 * 60 * 60 * 1000;

/// One question asked of a Help chat.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ask {
    /// The session that asked (`GHOSTEX_GLOBAL_SESSION_REF` and the like), empty from a terminal.
    asker: String,
    agent_id: String,
    global_ref: String,
    question: String,
    asked_at_ms: i64,
    /// This ask started the chat.
    #[serde(default)]
    created: bool,
}

/// CDXC:AgentGuide 2026-11-10 WHY:
/// Every ask is written down (`help-ask/asks.json`), a new chat's before its agent starts, so an ask that timed out or was stopped is found by the next one: the same asker's same question waits for that answer, its other questions go to the same chat once it is free, and one asker may start at most three Help chats in ten minutes. Retries are fine; duplicates are not.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct AskLedger {
    #[serde(default)]
    asks: Vec<Ask>,
}

impl AskLedger {
    fn path() -> PathBuf {
        crate::paths::get_gxserver_paths(None)
            .root_dir
            .join("help-ask")
            .join("asks.json")
    }

    fn read() -> Self {
        let mut ledger: Self = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let now = now_ms();
        ledger.asks.retain(|ask| now - ask.asked_at_ms < KEEP_MS);
        ledger
    }

    fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string(self) {
            let _ = std::fs::write(path, text);
        }
    }

    fn record(
        &mut self,
        asker: &str,
        agent_id: &str,
        session: &Value,
        question: &str,
        created: bool,
    ) {
        self.asks.push(Ask {
            asker: asker.to_string(),
            agent_id: agent_id.to_string(),
            global_ref: text(session, "globalRef").to_string(),
            question: question.to_string(),
            asked_at_ms: now_ms(),
            created,
        });
        self.save();
    }

    /// This asker's newest ask on the agent, while it still owns that chat.
    fn newest_for(&self, asker: &str, agent_id: &str) -> Option<&Ask> {
        let now = now_ms();
        self.asks.iter().rev().find(|ask| {
            ask.asker == asker
                && ask.agent_id == agent_id
                && now - ask.asked_at_ms < OWN_CHAT_WINDOW_MS
        })
    }

    /// The newest idle Help chat on the agent, whoever asked it last.
    fn newest_idle(
        &self,
        agent_id: &str,
        session_of: &dyn Fn(&Ask) -> Option<Value>,
    ) -> Option<Value> {
        let ask = self
            .asks
            .iter()
            .rev()
            .find(|ask| ask.agent_id == agent_id)?;
        session_of(ask).filter(|session| !is_working(session))
    }

    fn refuse_over_cap(&self, asker: &str) -> CliResult<()> {
        let now = now_ms();
        let started = self
            .asks
            .iter()
            .filter(|ask| {
                ask.created && ask.asker == asker && now - ask.asked_at_ms < CREATE_WINDOW_MS
            })
            .collect::<Vec<_>>();
        if started.len() < CREATES_PER_ASKER {
            return Ok(());
        }
        let chats = started
            .iter()
            .map(|ask| ask.global_ref.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Err(CliError::Other(format!(
            "You've started {} Ghostex Help chats in the last 10 minutes ({chats}), the most Ghostex starts for one asker. Read their answers with `ghostex read-session-chat <ref> --limit 5` instead of asking again.",
            started.len()
        )))
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
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

/// A new Ghostex Help session in the Ghostex config folder's project, started like `ghostex
/// agents create` starts one.
fn create(
    agent_id: &str,
    asker: &str,
    question: &str,
    ledger: &mut AskLedger,
    flags: &Flags,
) -> CliResult<Value> {
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
    // Written before its agent starts, so a retry after a start that timed out finds this chat.
    ledger.record(asker, agent_id, &session, question, true);
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
