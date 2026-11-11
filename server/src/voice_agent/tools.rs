//! The tools the voice model can call, and running them. Every tool answers a small JSON object
//! the model turns into speech; failures answer `{"ok": false, "error": …}` so the model can say
//! what went wrong instead of the conversation stopping.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User, supersedes the earlier "sends prompts right away without a spoken confirmation": "Please add this to the voice agent's startup instructions for all future voice sessions: before sending any prompt that could be risky or destructive (deleting files, removing projects, changing system settings, running commands with side effects), the voice agent must confirm with the user first and only send after they agree. Non-destructive prompts can still go out immediately." This is an instruction in `prompt.rs` only; `send_prompt` does not block in code.
//! The voice agent can act on any session, not only the open chat; it says what it sent after a prompt goes out; it answers question cards and approves or denies permission prompts (saying what it approved); and it can start new agents in a project the user names. Each tool reuses the gxserver route the matching `ghostex` verb calls (sessions, read-session-chat, send-session-chat-message, set-session-chat-draft, send-text, send-enter, answer-session-chat-prompt, create-agent, interrupt-session-chat, coordinator create); stopping and starting a coordinator are in `control.rs`.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Map, Value};

use super::computers;
use super::control;
use super::focus::{self, FocusScope, Purpose};
use super::loopback;
use super::question_forms;
use super::sessions::{self, Match, SessionInfo};
use super::show;
use super::{screen, when};
use crate::server::AppState;

/// Messages `read_session` returns by default and at most.
const READ_DEFAULT: usize = 6;
const READ_MAX: usize = 20;
/// Characters kept of one message, from its end (where the conclusion is).
const MESSAGE_CHARS: usize = 1_500;
/// Sessions `list_sessions` returns at most.
const LIST_MAX: usize = 40;

fn all_definitions() -> Value {
    let session = json!({
        "type": "string",
        "description": "Which session: its ref from list_sessions, or the user's words for it (\"the sidebar one\", \"Linear fix\")."
    });
    json!([
        {
            "type": "function",
            "name": "list_sessions",
            "description": "List the user's open agent sessions with title, project, agent and state (working, waiting on a question, waiting on an approval, idle, sleeping). Waiting sessions come first. A coordinator (orchestrator true) is followed by its sub-threads, the sessions it runs for the user: each has subThreadOf (its coordinator's title) and is not a top-level session. A session on another computer the user connected has computer (that computer's name); unavailableComputers names connected computers whose sessions could not be read, and why.",
            "parameters": {
                "type": "object",
                "properties": {
                    "project": {"type": "string", "description": "Optional: only sessions of this project (its name as the user says it). Words that name no project are looked up as a session or coordinator title instead, and the answer says which it was."}
                }
            }
        },
        {
            "type": "function",
            "name": "read_session",
            "description": "Read the last messages between the user and the agent in a session (no tool calls, no thinking), plus whether it is working and what it waits on. Each message has when, the time it was sent in words you can say as they are (\"5 minutes ago, at 3:40 PM\").",
            "parameters": {
                "type": "object",
                "properties": {
                    "session": session,
                    "count": {"type": "integer", "description": "How many messages, newest last. Default 6, at most 20."}
                },
                "required": ["session"]
            }
        },
        {
            "type": "function",
            "name": "send_prompt",
            "description": "Send a prompt to a session's agent, as if the user typed it in that chat. Call it right away for ordinary prompts; for a risky or destructive one, call it only after the user agreed out loud. A busy agent gets it at its next turn.",
            "parameters": {
                "type": "object",
                "properties": {
                    "session": session,
                    "text": {"type": "string", "description": "The prompt, written as the user's instruction to the agent."}
                },
                "required": ["session", "text"]
            }
        },
        {
            "type": "function",
            "name": "draft_prompt",
            "description": "Put a message in a session's chat box WITHOUT sending it, so the user can read and edit it and press Enter themselves, or ask you to send it. Use it when the user says draft, write it but don't send, or wants to check it first. A session that has only a terminal gets the text typed into its agent's input box, without Enter. If the chat already holds an unsent draft, it is kept and the tool asks you to call again with mode add or replace after asking the user.",
            "parameters": {
                "type": "object",
                "properties": {
                    "session": session,
                    "text": {"type": "string", "description": "The message, written as the user's instruction to the agent."},
                    "mode": {"type": "string", "enum": ["add", "replace"], "description": "Only after the tool said a draft is already there and the user chose: add puts the new text after it, replace swaps it (the old text stays in Recovered drafts)."}
                },
                "required": ["session", "text"]
            }
        },
        {
            "type": "function",
            "name": "send_draft",
            "description": "Send the unsent draft in a session's chat box exactly as it is now, including the user's own edits. Call it when the user says send it or send the draft. Never retype a draft with send_prompt.",
            "parameters": {
                "type": "object",
                "properties": {"session": session},
                "required": ["session"]
            }
        },
        {
            "type": "function",
            "name": "get_pending_prompt",
            "description": "Get what a session is waiting on: a question or permission prompt with numbered options, and the open questions in its chat (Ghostex forms and Ghostex pages), each question with a ref (its tag, like Q335), its numbered options and the recommended one.",
            "parameters": {
                "type": "object",
                "properties": {"session": session},
                "required": ["session"]
            }
        },
        {
            "type": "function",
            "name": "answer_prompt",
            "description": "Answer the question or permission prompt a session is waiting on, using option numbers from get_pending_prompt. For a permission prompt, pick the approve or deny option the user chose. For the open questions in its chat (questionForms from get_pending_prompt), give one answers entry per question with its ref and the pick in options as option numbers (\"answer Q335 with the first option\" is {\"question\": \"Q335\", \"options\": [1]}) or in choice as its label, letter or \"recommended\" ({\"question\": \"Q335\", \"choice\": \"recommended\"}); to skip one on purpose give {\"question\": \"Q335\", \"skip\": true}. Every form or page you answer is sent at once, its questions you leave out as skipped, and the forms you do not touch stay open. An entry with no usable pick is refused with its valid options and nothing is sent. The result's recorded says exactly what was sent (\"Q335: Add both as proposed\"): tell the user that.",
            "parameters": {
                "type": "object",
                "properties": {
                    "session": session,
                    "option": {"type": "integer", "description": "The chosen option number (for a single question or a permission prompt)."},
                    "answers": {
                        "type": "array",
                        "description": "For a card with several questions: one entry per question, in order. For questions in the chat: one entry per question you answer, each with its ref in question.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "options": {"type": "array", "items": {"type": "integer"}, "description": "Chosen option numbers (several only when the question allows it)."},
                                "text": {"type": "string", "description": "A free answer instead of the options, when the question allows one."},
                                "question": {"type": "string", "description": "For a question in the chat: its ref from get_pending_prompt (like Q335)."},
                                "choice": {"type": "string", "description": "For a question in the chat: the chosen option by its label, number or letter, or \"recommended\" for the recommended one (instead of options)."},
                                "note": {"type": "string", "description": "For a question in the chat: a note sent with the chosen options."},
                                "skip": {"type": "boolean", "description": "For a question in the chat: send it as skipped."}
                            }
                        }
                    },
                    "text": {"type": "string", "description": "A free answer for a single question that allows one."},
                    "notes": {"type": "string", "description": "For questions in the chat: notes for anything else, sent with a form of two or more questions."}
                },
                "required": ["session"]
            }
        },
        {
            "type": "function",
            "name": "list_projects",
            "description": "List the user's projects (name and folder, and computer for one on another computer the user connected), to start a new agent in one.",
            "parameters": {"type": "object", "properties": {}}
        },
        {
            "type": "function",
            "name": "list_agent_types",
            "description": "List the agent types the user can start (Claude, Codex and others), with the default first.",
            "parameters": {"type": "object", "properties": {}}
        },
        {
            "type": "function",
            "name": "start_agent",
            "description": "Start a new agent session in a project, optionally with a first prompt it starts working on. For a coordinator, call start_coordinator instead.",
            "parameters": {
                "type": "object",
                "properties": {
                    "project": {"type": "string", "description": "The project, as the user names it."},
                    "agent": {"type": "string", "description": "The agent the user asked for, as they said it (\"Claude\", \"Codex\"). Always pass it when the user named one; leave it out only when they did not, to use their default."},
                    "prompt": {"type": "string", "description": "Optional first prompt for the new agent."}
                },
                "required": ["project"]
            }
        },
        {
            "type": "function",
            "name": "stop_session",
            "description": "Stop a session's running turn right away, like pressing Stop in its chat: the agent stops what it is doing and the session stays open, waiting for the user's next prompt. Call it when the user says stop, halt or cancel about a session; never send \"stop\" as a prompt, which only queues behind the running turn. When the user's words fit the session only loosely or fit several, nothing is stopped: the result says needsConfirmation, a card on screen asks the user, and you ask too; call it again with the ref they confirm. It cannot close a session.",
            "parameters": {
                "type": "object",
                "properties": {"session": session},
                "required": ["session"]
            }
        },
        {
            "type": "function",
            "name": "start_coordinator",
            "description": "Start a new coordinator in a project: an agent session that hands the work to thread sessions it starts and reports back. Call it whenever the user asks for a coordinator or an orchestrator (never start_agent for that). What the user said it should work on becomes its first message.",
            "parameters": {
                "type": "object",
                "properties": {
                    "project": {"type": "string", "description": "The project, as the user names it."},
                    "message": {"type": "string", "description": "What the user wants it to work on, in their words, as its first message."},
                    "agent": {"type": "string", "description": "The agent it runs on, only when the user named one (\"Claude\", \"Codex\"); leave it out to use their default."},
                    "title": {"type": "string", "description": "Its name, only when the user gave one."}
                },
                "required": ["project"]
            }
        },
        {
            "type": "function",
            "name": "get_active_session",
            "description": "Which session the user is looking at: desktop is the session focused in the Ghostex app on the computer (desktopAlsoOnScreen lists other panes showing), phoneChat the chat open on their phone right now, when one is. Call it when the user says this session, this one, the one I'm looking at or the one on screen, then use its ref.",
            "parameters": {"type": "object", "properties": {}}
        },
        {
            "type": "function",
            "name": "show_session",
            "description": "Show a session in the Ghostex app: the sidebar selects it and the work area opens it, switching project when it is in another one. Call it when the user asks to see, show, open or go to a session; it switches even while they type. The other session tools already show the session they work with, unless the user is typing (their result then says shownInApp false).",
            "parameters": {
                "type": "object",
                "properties": {"session": session},
                "required": ["session"]
            }
        },
        {
            "type": "function",
            "name": "recall_last_conversation",
            "description": "Get your notes from your previous voice conversation with the user. Call it when the user asks to pick up or continue where you left off, or asks what you talked about last time.",
            "parameters": {"type": "object", "properties": {}}
        }
    ])
}

/// The tools the model is given: every tool for a call about all sessions; for a call about one
/// session only `FOCUSED_TOOLS`, whose session argument may be left out and means that session.
pub(super) fn definitions(focus: Option<&FocusScope>) -> Value {
    let all = all_definitions();
    let Some(scope) = focus else {
        return all;
    };
    if let Some(bot) = &scope.gbot {
        return super::gbot_call::definitions(bot, &all);
    }
    let title = &scope.title;
    let session_help = if scope.is_orchestrator {
        format!("Leave it out for \"{title}\". Name one of its threads (its title or ref from the briefing) only to read it or answer its questions.")
    } else {
        format!("Leave it out: this call is about \"{title}\" only.")
    };
    let mut tools: Vec<Value> = all
        .as_array()
        .into_iter()
        .flatten()
        .filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| FOCUSED_TOOLS.contains(&name))
        })
        .cloned()
        .map(|mut tool| {
            if let Some(parameters) = tool.get_mut("parameters") {
                if let Some(session) = parameters.pointer_mut("/properties/session") {
                    session["description"] = json!(session_help);
                }
                if let Some(required) = parameters.get_mut("required").and_then(Value::as_array_mut)
                {
                    required.retain(|key| key != "session");
                }
            }
            tool
        })
        .collect();
    tools.push(json!({
        "type": "function",
        "name": "widen_call",
        "description": "Make this call about all of the user's sessions again. Call it when the user asks to talk about all their sessions or their other agents. It is the only way to leave this session; you can never move the call to another single session.",
        "parameters": {"type": "object", "properties": {}}
    }));
    Value::Array(tools)
}

/// What a tool call did: the output for the model, and a short status line for the client.
pub(super) struct ToolOutcome {
    pub(super) output: Value,
    pub(super) ok: bool,
}

/// A short human line for the client's status while the tool runs.
/// `focus_title` names the session a focused call's tools work on when they name none.
pub(super) fn label(name: &str, args: &Value, focus_title: Option<&str>) -> String {
    // A ref ("P3lv0:G8lhp") means nothing to the user; name it only when it is their words.
    let target = args
        .get("session")
        .or_else(|| args.get("project"))
        .and_then(Value::as_str)
        .map(|text| {
            let is_ref = text.contains(':') && !text.contains(' ');
            if is_ref {
                "the session".to_string()
            } else {
                text.chars().take(40).collect::<String>()
            }
        })
        .or_else(|| focus_title.map(|title| title.chars().take(40).collect::<String>()));
    match (name, target) {
        ("list_sessions", _) => "Checking your sessions".to_string(),
        ("read_session", Some(target)) => format!("Reading {target}"),
        ("send_prompt", Some(target)) => format!("Sending to {target}"),
        ("draft_prompt", Some(target)) => format!("Drafting in {target}"),
        ("send_draft", Some(target)) => format!("Sending the draft in {target}"),
        ("get_pending_prompt", Some(target)) => format!("Checking what {target} asks"),
        ("answer_prompt", Some(target)) => format!("Answering {target}"),
        ("list_projects", _) => "Checking your projects".to_string(),
        ("list_agent_types", _) => "Checking your agents".to_string(),
        ("start_agent", Some(target)) => format!("Starting an agent in {target}"),
        ("stop_session", Some(target)) => format!("Stopping {target}"),
        ("start_coordinator", Some(target)) => format!("Starting a coordinator in {target}"),
        ("show_session", Some(target)) => format!("Showing {target}"),
        ("get_active_session", _) => "Checking what's on screen".to_string(),
        ("recall_last_conversation", _) => "Remembering your last conversation".to_string(),
        ("widen_call", _) => "Opening the call to all sessions".to_string(),
        ("talk_to_bot", _) => "Passing it to the chat".to_string(),
        (name, _) => name.replace('_', " "),
    }
}

pub(super) fn failure(message: impl Into<String>) -> ToolOutcome {
    ToolOutcome {
        output: json!({"ok": false, "error": message.into()}),
        ok: false,
    }
}

fn success(output: Value) -> ToolOutcome {
    ToolOutcome { output, ok: true }
}

pub(super) fn arg_text(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

/// What a conversation remembers between tool calls: which session each name the user said
/// resolved to.
///
/// CDXC:VoiceMode 2026-10-20 WHY:
/// Ghostex renames a session from its first prompt, so "the voice test one" stopped matching right after the agent sent it a prompt (live test: the session became "Pineapple"). A name that resolved once keeps naming the same session for the rest of the conversation.
#[derive(Default)]
pub(super) struct ToolMemory {
    names: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// Terminal-only sessions this conversation typed a draft into, which `send_draft` may press
    /// Enter in.
    terminal_drafts: std::sync::Mutex<std::collections::HashSet<String>>,
    /// The unsent draft `draft_prompt` last told the model about, per session: `replace` swaps
    /// only that exact text, so the user heard what they are replacing.
    shown_drafts: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// The session the call is about, when it is a focused call (`focus.rs`); the conversation
    /// keeps it current.
    scope: std::sync::Mutex<Option<FocusScope>>,
    /// The call's client is not on this computer (`clientRemote=1`): sessions are shown there.
    client_remote: bool,
    /// The token the attached client opened the call with; the tools' requests send it
    /// (`loopback::ToolCaller`). The last client to attach is the one the call acts for.
    caller_token: std::sync::Mutex<Option<String>>,
}

impl ToolMemory {
    pub(super) fn new(client_remote: bool) -> Self {
        Self {
            client_remote,
            ..Self::default()
        }
    }

    pub(super) fn set_scope(&self, scope: Option<FocusScope>) {
        if let Ok(mut current) = self.scope.lock() {
            *current = scope;
        }
    }

    pub(super) fn scope(&self) -> Option<FocusScope> {
        self.scope.lock().ok().and_then(|scope| scope.clone())
    }

    pub(super) fn client_remote(&self) -> bool {
        self.client_remote
    }

    pub(super) fn set_caller_token(&self, token: String) {
        if let Ok(mut current) = self.caller_token.lock() {
            *current = Some(token);
        }
    }

    /// A new tool call's requests, as the attached client.
    pub(super) fn tool_caller(&self) -> super::loopback::ToolCaller {
        super::loopback::ToolCaller::new(
            self.caller_token
                .lock()
                .ok()
                .and_then(|token| token.clone()),
        )
    }

    fn key(name: &str) -> String {
        name.trim().to_lowercase()
    }

    pub(super) fn recall(&self, name: &str) -> Option<String> {
        self.names.lock().ok()?.get(&Self::key(name)).cloned()
    }

    pub(super) fn remember(&self, name: &str, session: &SessionInfo) {
        if let Ok(mut names) = self.names.lock() {
            names.insert(Self::key(name), session.reference());
            names.insert(Self::key(&session.title), session.reference());
        }
    }

    fn note_terminal_draft(&self, session: &SessionInfo) {
        if let Ok(mut drafts) = self.terminal_drafts.lock() {
            drafts.insert(session.reference());
        }
    }

    fn take_terminal_draft(&self, session: &SessionInfo) -> bool {
        self.terminal_drafts
            .lock()
            .is_ok_and(|mut drafts| drafts.remove(&session.reference()))
    }

    fn note_shown_draft(&self, session: &SessionInfo, content: &str) {
        if let Ok(mut drafts) = self.shown_drafts.lock() {
            drafts.insert(session.reference(), content.to_string());
        }
    }

    fn was_shown(&self, session: &SessionInfo, content: &str) -> bool {
        self.shown_drafts.lock().is_ok_and(|drafts| {
            drafts.get(&session.reference()).map(String::as_str) == Some(content)
        })
    }
}

pub(super) async fn run(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    name: &str,
    args: &Value,
) -> ToolOutcome {
    run_as(state, memory, &memory.tool_caller(), name, args).await
}

/// [`run`] with the requests of `caller`, which keeps the approvals and the calls already made
/// when the tool runs again after an approval (`approvals.rs`).
pub(super) async fn run_as(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    caller: &super::loopback::ToolCaller,
    name: &str,
    args: &Value,
) -> ToolOutcome {
    super::loopback::with_caller(caller.clone(), run_followed(state, memory, name, args)).await
}

async fn run_followed(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    name: &str,
    args: &Value,
) -> ToolOutcome {
    let mut outcome = run_tool(state, memory, name, args).await;
    // CDXC:VoiceMode 2026-10-23 WHY: in a call about one session, reading, answering or sending no longer switches the app to it, while show_session (the user asked to see it) still shows it. The user picked the session to start the call and may be reading another one meanwhile. The user was asked about this and did not object. A client on another computer (`clientRemote`) shows sessions itself, from the tool frame's `show`.
    if outcome.ok
        && show::FOLLOWING_TOOLS.contains(&name)
        && memory.scope().is_none()
        && !memory.client_remote()
    {
        show::follow(state, &mut outcome.output).await;
    }
    outcome
}

async fn run_tool(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    name: &str,
    args: &Value,
) -> ToolOutcome {
    let scope = memory.scope();
    // A focused call is offered only its own tools; one the model still names is refused.
    if let Some(scope) = &scope {
        // A call with a GBot has its own tools (gbot_call/tools.rs).
        if let Some(bot) = &scope.gbot {
            if !super::gbot_call::TOOLS.contains(&name) {
                return failure(super::gbot_call::refusal(bot));
            }
            if name == "talk_to_bot" {
                return match super::gbot_call::talk(state, memory, bot, args).await {
                    Ok(output) => success(output),
                    Err(message) => failure(message),
                };
            }
        } else if !FOCUSED_TOOLS.contains(&name) {
            return failure(focus::refusal(&scope.title));
        }
    } else if name == "widen_call" {
        return success(
            json!({"ok": true, "widened": false, "note": "This call is already about all sessions."}),
        );
    }
    let result = match name {
        "list_sessions" => list_sessions(state, args).await,
        "read_session" => read_session(state, memory, args).await,
        "send_prompt" => send_prompt(state, memory, args).await,
        "draft_prompt" => draft_prompt(state, memory, args).await,
        "send_draft" => send_draft(state, memory, args).await,
        "get_pending_prompt" => get_pending_prompt(state, memory, args).await,
        "answer_prompt" => answer_prompt(state, memory, args).await,
        "list_projects" => list_projects(state).await,
        "list_agent_types" => list_agent_types(state).await,
        "start_agent" => return start_agent(state, args).await,
        "stop_session" => control::stop_session(state, memory, args).await,
        "start_coordinator" => return control::start_coordinator(state, args).await,
        "get_active_session" => screen::get_active_session(state, memory.client_remote()).await,
        "show_session" => match resolve_session(state, memory, args, Purpose::Read).await {
            Ok(session) if memory.client_remote() => Ok(json!({
                "ok": true,
                "shown": true,
                "note": "Shown in the user's app.",
                "session": session.brief(),
            })),
            // This computer's app is asked to show it through the tool frame (`computerSession`).
            Ok(session) if session.computer.is_some() => Ok(json!({
                "ok": true,
                "shown": true,
                "note": "Shown in the user's Ghostex desktop app when it is connected to that computer.",
                "session": session.brief(),
            })),
            Ok(session) => show::show_session(state, &session).await,
            Err(error) => Err(error),
        },
        // The conversation applies the change when it sees this result (`Conversation::tool_done`).
        "widen_call" => Ok(json!({"ok": true, "widened": true})),
        "recall_last_conversation" => super::memory::recall(&state.paths),
        other => Err(format!("There is no tool called {other}.")),
    };
    match result {
        Ok(output) => success(output),
        Err(message) => failure(message),
    }
}

/// The tools a call about one session has (`definitions` offers only these).
const FOCUSED_TOOLS: &[&str] = &[
    "read_session",
    "send_prompt",
    "draft_prompt",
    "send_draft",
    "get_pending_prompt",
    "answer_prompt",
    "stop_session",
    "show_session",
    "widen_call",
];

/// The session the user named, or an error the model can speak (with candidates when ambiguous).
/// In a focused call, no name means the focused session, only it (and an orchestrator's threads)
/// can be named, and any other session is refused with the words the user agreed on.
pub(super) async fn resolve_session(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    args: &Value,
    purpose: Purpose,
) -> Result<SessionInfo, String> {
    let query = arg_text(args, "session");
    let scope = memory.scope();
    if query.is_empty() && scope.is_none() {
        return Err("Say which session.".to_string());
    }
    let (all, unavailable) = sessions::list_everywhere_async(state).await?;
    let session = match &scope {
        None => named_session(memory, &all, &query, &all)
            .map_err(|error| computers::with_unreachable(error, &unavailable))?,
        Some(scope) => {
            let reference = scope.reference();
            let focused = all.iter().find(|session| session.reference() == reference);
            if query.is_empty() || query == reference || query.eq_ignore_ascii_case(&scope.title) {
                focused.cloned().ok_or_else(|| {
                    format!(
                        "\"{}\" was closed, so it can't be reached anymore. Offer to talk about all sessions.",
                        scope.title
                    )
                })?
            } else {
                let reachable: Vec<SessionInfo> =
                    scope.reachable(&all).into_iter().cloned().collect();
                named_session(memory, &reachable, &query, &all)
                    .map_err(|_| focus::refusal(&scope.title))?
            }
        }
    };
    if let Some(scope) = &scope {
        scope.allows(&session.reference(), purpose)?;
    }
    Ok(session)
}

/// The session among `candidates` that `query` names. A name remembered earlier in the call is
/// looked up in `all`, so a renamed session keeps its name.
fn named_session(
    memory: &ToolMemory,
    candidates: &[SessionInfo],
    query: &str,
    all: &[SessionInfo],
) -> Result<SessionInfo, String> {
    if let Some(reference) = memory.recall(query) {
        if let Some(session) = all.iter().find(|session| session.reference() == reference) {
            return Ok(session.clone());
        }
    }
    match sessions::match_session(candidates, query) {
        Match::One(session) => {
            memory.remember(query, &session);
            Ok(session)
        }
        Match::Ambiguous(candidates) => Err(format!(
            "Ambiguous: \"{query}\" could be {}. Ask the user which one.",
            candidates
                .iter()
                .map(|session| format!(
                    "\"{}\" ({}{}, ref {})",
                    session.title,
                    session.project,
                    session
                        .thread_of
                        .as_ref()
                        .map(|parent| format!(", sub-thread of {}", parent.title))
                        .unwrap_or_default(),
                    session.reference()
                ))
                .collect::<Vec<_>>()
                .join(" or ")
        )),
        Match::None => Err(format!(
            "{NO_MATCH} \"{query}\". Call list_sessions to see their titles."
        )),
    }
}

/// How the answer starts when no session fits the user's words.
pub(super) const NO_MATCH: &str = "No open session matches";

/// CDXC:VoiceMode 2026-11-10 WHY: User: "When the user says something like "tell Big Chungus" about an issue, the assistant searched for a project named Big Chungus first, found nothing, and told the user "no Big Chungus sessions came up under that name", even though Big Chungus is a session title". The model guessed the kind and passed the title as `project`, and this tool answered "No project matches" after one kind of search. A `project` that names no project is now matched against session and coordinator titles too before anything is reported missing (the voice agent's own tool; the app's session and project search is unchanged), and the prompt tells the model to look across all kinds (prompt.rs).
async fn list_sessions(state: &Arc<AppState>, args: &Value) -> Result<Value, String> {
    let (mut all, unavailable) = sessions::list_everywhere_async(state).await?;
    let project = arg_text(args, "project");
    let mut matched_as: Option<Value> = None;
    if !project.is_empty() {
        let projects = sessions::list_projects_async(state).await?;
        match sessions::match_project(&projects, &project) {
            Match::One(found) => all.retain(|session| found.holds(session)),
            Match::Ambiguous(candidates) => {
                return Err(format!(
                    "Ambiguous project \"{project}\": {}. Ask the user which one.",
                    candidates
                        .iter()
                        .map(project_choice)
                        .collect::<Vec<_>>()
                        .join(" or ")
                ))
            }
            // Not a project: the user's words may name a session or a coordinator ("tell Big
            // Chungus"), so they are looked up as a title before anything is reported missing.
            Match::None => match sessions::match_session(&all, &project) {
                Match::One(found) => {
                    all.retain(|session| {
                        session.reference() == found.reference()
                            || session.thread_of.as_ref().is_some_and(|parent| parent.reference == found.reference())
                    });
                    matched_as = Some(json!({"kind": "session", "note": format!("No project is called \"{project}\"; it is the session \"{}\".", found.title)}));
                }
                Match::Ambiguous(candidates) => {
                    return Err(format!(
                        "No project is called \"{project}\", but several sessions are: {}. Ask the user which one.",
                        candidates
                            .iter()
                            .map(|session| format!("\"{}\" ({})", session.title, session.reference()))
                            .collect::<Vec<_>>()
                            .join(" or ")
                    ))
                }
                Match::None => {
                    return Err(format!(
                        "Nothing open is called \"{project}\": no project and no session or coordinator title matches."
                    ))
                }
            },
        }
    }
    let total = all.len();
    let count = |state: sessions::VoiceState| all.iter().filter(|s| s.state == state).count();
    let mut result = json!({
        "ok": true,
        "total": total,
        "working": count(sessions::VoiceState::Working),
        "waitingOnQuestion": count(sessions::VoiceState::WaitingOnQuestion),
        "waitingOnApproval": count(sessions::VoiceState::WaitingOnApproval),
        "idle": count(sessions::VoiceState::Finished),
        "sleeping": count(sessions::VoiceState::Sleeping),
        "sessions": all.iter().take(LIST_MAX).map(SessionInfo::to_tool_value).collect::<Vec<_>>(),
        "truncated": total > LIST_MAX,
    });
    if let Some(matched_as) = matched_as {
        result["matchedAs"] = matched_as;
    }
    if !unavailable.is_empty() {
        result["unavailableComputers"] = json!(unavailable
            .iter()
            .map(computers::Unavailable::to_tool_value)
            .collect::<Vec<_>>());
    }
    Ok(result)
}

/// A message's text blocks only, keeping its end when it is long.
fn spoken_text(message: &Value) -> String {
    let text = message
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let count = text.chars().count();
    if count <= MESSAGE_CHARS {
        text
    } else {
        let tail: String = text.chars().skip(count - MESSAGE_CHARS).collect();
        format!("…{tail}")
    }
}

pub(super) async fn read_chat(
    state: &Arc<AppState>,
    session: &SessionInfo,
    limit: usize,
) -> Result<Value, String> {
    computers::call(
        state,
        session.computer.as_ref(),
        "/api/readSessionChat",
        json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "limit": limit,
            "historyMode": "detail",
        }),
        Duration::from_secs(20),
    )
    .await
}

/// The user's and the agent's messages of a chat, newest last, at most `count`, each with its
/// text and when it was sent.
fn spoken_messages(chat: &Value, count: usize, now: chrono::DateTime<chrono::Local>) -> Vec<Value> {
    let mut messages: Vec<Value> = chat
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|message| {
            let role = match crate::ghostex_cli::session_chat_transcript::effective_role(message) {
                "user" => "user",
                "assistant" => "agent",
                _ => return None,
            };
            let text = spoken_text(message);
            if text.is_empty() {
                return None;
            }
            let mut entry = json!({"role": role, "text": text});
            if let Some(spoken) = message
                .get("timestamp")
                .and_then(Value::as_i64)
                .and_then(|timestamp| when::spoken(timestamp, now))
            {
                entry["when"] = json!(spoken);
            }
            Some(entry)
        })
        .collect();
    if messages.len() > count {
        messages.drain(..messages.len() - count);
    }
    messages
}

async fn read_session(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    args: &Value,
) -> Result<Value, String> {
    let session = resolve_session(state, memory, args, Purpose::Read).await?;
    let count = args
        .get("count")
        .and_then(Value::as_u64)
        .map(|count| (count as usize).clamp(1, READ_MAX))
        .unwrap_or(READ_DEFAULT);
    // Tool rows dominate a transcript, so read well past `count` rows to find enough messages.
    let chat = read_chat(state, &session, (count * 12).clamp(60, 240)).await?;
    let messages = spoken_messages(&chat, count, chrono::Local::now());
    Ok(json!({
        "ok": true,
        "session": session.to_tool_value(),
        "working": chat.get("working").and_then(Value::as_bool).unwrap_or(false),
        "messages": messages,
    }))
}

pub(super) async fn send_prompt(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    args: &Value,
) -> Result<Value, String> {
    let text = arg_text(args, "text");
    if text.is_empty() {
        return Err("The prompt is empty.".to_string());
    }
    let session = resolve_session(state, memory, args, Purpose::Send).await?;
    computers::call(
        state,
        session.computer.as_ref(),
        "/api/sendSessionChatMessage",
        json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "text": text,
            "sendRequestId": uuid::Uuid::new_v4().to_string(),
        }),
        Duration::from_secs(45),
    )
    .await?;
    Ok(json!({"ok": true, "sent": true, "session": session.brief(), "text": text}))
}

/// The `originClientId` voice drafts are saved under. The chat core writes a draft from this
/// client straight into the composer when that loses nothing typed there
/// (`VOICE_DRAFT_CLIENT_ID` in packages/gx-chat-core/src/composer/draft_sync.rs).
const VOICE_DRAFT_CLIENT_ID: &str = "gxserver-voice-agent";

/// The session's chat draft as gxserver holds it (every client's composer syncs to it).
async fn read_draft(
    state: &Arc<AppState>,
    session: &SessionInfo,
) -> Result<Option<crate::session_chat_queue::SessionChatDraft>, String> {
    let state = state.clone();
    let (project, session) = (session.project_id.clone(), session.session_id.clone());
    tokio::task::spawn_blocking(move || {
        let db = crate::storage::open_gxserver_database(&state.paths)
            .map_err(|error| error.to_string())?;
        crate::session_chat_draft_versions::read(&db, &project, &session)
            .map_err(|error| error.message)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Why `draft_prompt` and `send_draft` stop for a chat on another computer: its synced draft lives
/// in that computer's gxserver, which opens neither reading nor writing it to other computers.
fn no_chat_draft_elsewhere(computer: &str) -> String {
    format!("Drafting into a chat on {computer} can't be done from here, so nothing was written. Offer to send it as a prompt right away instead, or the user can type it in Ghostex.")
}

/// The unsent text of a draft, unless the terminal owns it (moved there when the user switched).
fn unsent(
    draft: Option<&crate::session_chat_queue::SessionChatDraft>,
) -> Option<&crate::session_chat_queue::SessionChatDraft> {
    draft.filter(|draft| !draft.parked && !draft.content.trim().is_empty())
}

/// CDXC:VoiceMode 2026-10-20 DECISION:
/// User: "give the voice agent the ability to draft a message into a session's chat without sending it. The draft should sit there unsent so the user can review it, then either press enter themselves or tell the voice agent to send it." The draft is gxserver's synced chat draft for that session, so it shows in the composer of every client that opens the chat (desktop, web, phone), and `send_draft` sends gxserver's copy, which carries the user's edits once their typing pauses. An unsent draft already there is never overwritten silently: the tool refuses until the user chose add or replace, and replace leaves the old text in Recovered drafts. A session without a chat gets the text typed into its agent's input box without Enter, and `send_draft` presses Enter there.
async fn draft_prompt(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    args: &Value,
) -> Result<Value, String> {
    let text = arg_text(args, "text");
    if text.is_empty() {
        return Err("The draft is empty.".to_string());
    }
    let session = resolve_session(state, memory, args, Purpose::Send).await?;
    if !session.has_chat {
        computers::call(
            state,
            session.computer.as_ref(),
            "/api/sendSessionText",
            json!({"projectId": session.project_id, "sessionId": session.session_id, "text": text}),
            Duration::from_secs(20),
        )
        .await?;
        memory.note_terminal_draft(&session);
        return Ok(json!({
            "ok": true,
            "drafted": true,
            "where": "typed into the agent's input box in its terminal, not sent",
            "session": session.brief(),
            "text": text,
        }));
    }
    if let Some(computer) = &session.computer {
        return Err(no_chat_draft_elsewhere(&computer.name));
    }
    let existing = read_draft(state, &session).await?;
    let mode = arg_text(args, "mode").to_lowercase();
    let fresh = || json!({"draftId": format!("voice-{}", uuid::Uuid::new_v4()), "revision": 1});
    let (content, version) = match unsent(existing.as_ref()) {
        None => (text.clone(), fresh()),
        Some(draft) if mode == "add" => {
            let separator = if draft.content.ends_with("\n\n") {
                ""
            } else if draft.content.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            // The same identity one revision on, so sending it retires the user's draft too.
            let version = draft.version.as_ref().map_or_else(
                fresh,
                |version| json!({"draftId": version.draft_id, "revision": version.revision + 1}),
            );
            (format!("{}{separator}{text}", draft.content), version)
        }
        Some(draft) if mode == "replace" && memory.was_shown(&session, &draft.content) => {
            (text.clone(), fresh())
        }
        Some(draft) => {
            memory.note_shown_draft(&session, &draft.content);
            return Err(format!(
                "That chat already has an unsent draft, which was kept: \"{}\". Ask the user whether to add the new text after it or replace it, then call draft_prompt again with mode add or replace.",
                short(draft.content.trim(), 400)
            ));
        }
    };
    let saved = loopback::call(
        state,
        "/api/setSessionChatDraft",
        json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "clientId": VOICE_DRAFT_CLIENT_ID,
            "content": content,
            "draftVersion": version,
        }),
        Duration::from_secs(20),
    )
    .await?;
    if saved.pointer("/draft/content").and_then(Value::as_str) != Some(content.as_str()) {
        return Err("The chat's draft changed while I was writing it. Try again.".to_string());
    }
    Ok(json!({
        "ok": true,
        "drafted": true,
        "where": "in the session's chat box, not sent",
        "session": session.brief(),
        "draft": content,
    }))
}

async fn send_draft(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    args: &Value,
) -> Result<Value, String> {
    let session = resolve_session(state, memory, args, Purpose::Send).await?;
    if !session.has_chat {
        if !memory.take_terminal_draft(&session) {
            return Err("I have not drafted anything into that session's terminal in this conversation, so there is nothing of mine to send.".to_string());
        }
        computers::call(
            state,
            session.computer.as_ref(),
            "/api/sendSessionEnter",
            json!({"projectId": session.project_id, "sessionId": session.session_id}),
            Duration::from_secs(20),
        )
        .await?;
        return Ok(json!({
            "ok": true,
            "sent": true,
            "session": session.brief(),
            "note": "Pressed Enter in the agent's input box, which sent what it held, the user's edits included.",
        }));
    }
    if let Some(computer) = &session.computer {
        return Err(no_chat_draft_elsewhere(&computer.name));
    }
    let existing = read_draft(state, &session).await?;
    let Some(draft) = unsent(existing.as_ref()) else {
        return Err("That chat has no unsent draft; it may have been sent already.".to_string());
    };
    let mut params = json!({
        "projectId": session.project_id,
        "sessionId": session.session_id,
        "text": draft.content,
        "sendRequestId": uuid::Uuid::new_v4().to_string(),
    });
    // Sending under the draft's own revision retires it, which clears it from every composer.
    if let Some(version) = &draft.version {
        params["draftVersion"] = json!(version);
    }
    loopback::call(
        state,
        "/api/sendSessionChatMessage",
        params,
        Duration::from_secs(45),
    )
    .await?;
    Ok(json!({"ok": true, "sent": true, "session": session.brief(), "text": draft.content}))
}

/// One option the user can pick, and the exact answer the chat would send for it.
struct PromptOption {
    label: String,
    answer: Value,
}

/// The prompt a session waits on, read the way the chat reads it: a terminal picker or dialog's
/// rows, its key actions, else the stored question or approval card.
enum PendingPrompt {
    None,
    /// A question card: each question with its options; answered with `kind: question`.
    Questions(Vec<Value>),
    /// A permission prompt or on-screen picker: a title and its options.
    Choice {
        title: String,
        detail: Option<String>,
        options: Vec<PromptOption>,
    },
}

fn pending_prompt(chat: &Value) -> PendingPrompt {
    let prompt = chat.get("prompt").filter(|prompt| prompt.is_object());
    if let Some(prompt) = prompt {
        if prompt.get("kind").and_then(Value::as_str) == Some("question") {
            if let Some(questions) = prompt.get("questions").and_then(Value::as_array) {
                if !questions.is_empty() {
                    return PendingPrompt::Questions(questions.clone());
                }
            }
        }
    }
    if let Some(notice) = chat
        .get("terminalNotice")
        .filter(|notice| notice.is_object())
    {
        let title = notice
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("The agent is asking")
            .to_string();
        let detail = notice
            .get("approvalCommand")
            .or_else(|| notice.get("detail"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let dialog_id = notice.pointer("/dialog/id").and_then(Value::as_str);
        let choice_answer = |index: usize| match dialog_id {
            Some(id) => json!({"kind": "terminalDialog", "choiceIndex": index, "dialogId": id}),
            None => json!({"kind": "terminalChoice", "choiceIndex": index}),
        };
        let mut options: Vec<PromptOption> = notice
            .get("choices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|choice| {
                Some(PromptOption {
                    label: choice.get("label")?.as_str()?.to_string(),
                    answer: choice_answer(choice.get("index")?.as_u64()? as usize),
                })
            })
            .collect();
        if options.is_empty() {
            options = notice
                .pointer("/dialog/rows")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(|(index, row)| {
                    Some(PromptOption {
                        label: row.get("label")?.as_str()?.to_string(),
                        answer: choice_answer(index),
                    })
                })
                .collect();
        }
        if options.is_empty() {
            options = notice
                .get("actions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|action| action.get("kind").and_then(Value::as_str) == Some("sendKeys"))
                .filter_map(|action| {
                    Some(PromptOption {
                        label: action.get("label")?.as_str()?.to_string(),
                        answer: json!({"kind": "approval", "approvalSend": action.get("send")?.as_str()?}),
                    })
                })
                .collect();
        }
        if !options.is_empty() {
            return PendingPrompt::Choice {
                title,
                detail,
                options,
            };
        }
    }
    if let Some(prompt) = prompt {
        if prompt.get("kind").and_then(Value::as_str) == Some("approval") {
            let tool = prompt
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("a tool");
            return PendingPrompt::Choice {
                title: format!("Permission to use {tool}"),
                detail: prompt
                    .get("summary")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                options: vec![
                    PromptOption {
                        label: "Allow".to_string(),
                        answer: json!({"kind": "approval", "approvalSend": "1"}),
                    },
                    PromptOption {
                        label: "Deny".to_string(),
                        answer: json!({"kind": "approval", "approvalSend": ""}),
                    },
                ],
            };
        }
    }
    PendingPrompt::None
}

fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

async fn get_pending_prompt(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    args: &Value,
) -> Result<Value, String> {
    let session = resolve_session(state, memory, args, Purpose::Read).await?;
    let chat = read_chat(state, &session, 20).await?;
    Ok(pending_view(state, &session, &chat).await)
}

/// What `session` waits on, read from its `chat`: `get_pending_prompt`'s answer.
async fn pending_view(state: &Arc<AppState>, session: &SessionInfo, chat: &Value) -> Value {
    // The open questions in its chat (question forms and pages) ride every answer.
    let forms = question_forms::listing(state, session).await;
    let mut result = match pending_prompt(chat) {
        PendingPrompt::None if !forms.is_empty() => json!({
            "ok": true,
            "session": session.to_tool_value(),
            "kind": "questionForms",
        }),
        PendingPrompt::None if session.computer.is_some() => json!({
            "ok": true,
            "session": session.to_tool_value(),
            "kind": "none",
            "note": "No question card or permission prompt is waiting. Ghostex forms and Ghostex pages in a chat on another computer can't be read from here; the user can answer those in Ghostex.",
        }),
        PendingPrompt::None => json!({
            "ok": true,
            "session": session.to_tool_value(),
            "kind": "none",
            "note": "Nothing is waiting for an answer in this session right now.",
        }),
        PendingPrompt::Questions(questions) => json!({
            "ok": true,
            "session": session.to_tool_value(),
            "kind": "question",
            "questions": questions.iter().enumerate().map(|(index, question)| json!({
                "number": index + 1,
                "question": question.get("question").and_then(Value::as_str).map(|text| short(text, 600)),
                "multiSelect": question.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
                "allowsFreeAnswer": question.get("allowCustom").and_then(Value::as_bool).unwrap_or(true),
                "options": question.get("options").and_then(Value::as_array).into_iter().flatten().enumerate().map(|(n, option)| json!({
                    "number": n + 1,
                    "label": option.get("label").and_then(Value::as_str).map(|text| short(text, 200)),
                    "description": option.get("description").and_then(Value::as_str).map(|text| short(text, 300)),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
        PendingPrompt::Choice {
            title,
            detail,
            options,
        } => json!({
            "ok": true,
            "session": session.to_tool_value(),
            "kind": "permissionOrChoice",
            "title": short(&title, 300),
            "detail": detail.map(|detail| short(&detail, 400)),
            "options": options.iter().enumerate().map(|(n, option)| json!({"number": n + 1, "label": short(&option.label, 200)})).collect::<Vec<_>>(),
        }),
    };
    if !forms.is_empty() {
        result["questionForms"] = json!(forms);
    }
    result
}

/// What a focused call's briefing says about its session (`focus::briefing_message`): its state
/// and what it waits on, its last messages, its open questions and approvals, an unsent draft, and
/// an orchestrator's threads. A part that can't be read is left out.
pub(super) async fn briefing(state: &Arc<AppState>, scope: &FocusScope) -> Value {
    let all = sessions::list_sessions_async(state)
        .await
        .unwrap_or_default();
    let reference = scope.reference();
    let Some(session) = all.iter().find(|session| session.reference() == reference) else {
        return json!({"closed": true});
    };
    let mut facts = json!({"session": session.to_tool_value()});
    if let Some(orchestrator) = &scope.orchestrator {
        facts["threadOf"] = json!(orchestrator);
    }
    if let Ok(chat) = read_chat(state, session, (READ_DEFAULT * 12).clamp(60, 240)).await {
        facts["working"] = json!(chat
            .get("working")
            .and_then(Value::as_bool)
            .unwrap_or(false));
        facts["messages"] = json!(spoken_messages(&chat, READ_DEFAULT, chrono::Local::now()));
        let pending = pending_view(state, session, &chat).await;
        if pending.get("kind").and_then(Value::as_str) != Some("none") {
            let mut pending = pending;
            if let Some(fields) = pending.as_object_mut() {
                fields.remove("session");
                fields.remove("ok");
            }
            facts["waitingOn"] = pending;
        }
    }
    if let Ok(draft) = read_draft(state, session).await {
        if let Some(draft) = unsent(draft.as_ref()) {
            facts["unsentDraft"] = json!(short(draft.content.trim(), 600));
        }
    }
    if scope.is_orchestrator {
        facts["threads"] = json!(focus::thread_rows(scope, &all));
    }
    facts
}

/// The reply the focused session's turn just finished with, made speakable, when its chat ends
/// on one.
pub(super) async fn finished_reply(state: &Arc<AppState>, scope: &FocusScope) -> Option<String> {
    let all = sessions::list_sessions_async(state).await.ok()?;
    let reference = scope.reference();
    let session = all
        .iter()
        .find(|session| session.reference() == reference)?;
    let chat = read_chat(state, session, 60).await.ok()?;
    focus::last_reply_markdown(&chat).map(|markdown| focus::speakable_reply(&markdown))
}

fn option_numbers(value: Option<&Value>) -> Vec<usize> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .filter(|number| *number >= 1)
        .map(|number| number as usize - 1)
        .collect()
}

async fn answer_prompt(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    args: &Value,
) -> Result<Value, String> {
    let session = resolve_session(state, memory, args, Purpose::Answer).await?;
    // Answers that name questions by ref are for the question forms and pages in its chat.
    if question_forms::names_questions(args) {
        return question_forms::answer(state, &session, args).await;
    }
    let chat = read_chat(state, &session, 20).await?;
    let single_option = args
        .get("option")
        .and_then(Value::as_u64)
        .filter(|n| *n >= 1)
        .map(|n| n as usize - 1);
    let free_text = arg_text(args, "text");
    let (answer, said) = match pending_prompt(&chat) {
        PendingPrompt::None => return question_forms::answer(state, &session, args).await,
        PendingPrompt::Questions(questions) => {
            let mut selections: Vec<Value> = Vec::new();
            let mut said: Vec<String> = Vec::new();
            let answers = args
                .get("answers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for (index, question) in questions.iter().enumerate() {
                let labels: Vec<String> = question
                    .get("options")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|option| {
                        option
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    })
                    .collect();
                let (indices, other) = match answers.get(index) {
                    Some(entry) => (
                        option_numbers(entry.get("options")),
                        entry
                            .get("text")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|t| !t.is_empty())
                            .map(str::to_string),
                    ),
                    None if index == 0 => (
                        single_option.into_iter().collect(),
                        Some(free_text.clone()).filter(|t| !t.is_empty()),
                    ),
                    None => (Vec::new(), None),
                };
                if indices.iter().any(|i| *i >= labels.len()) {
                    return Err(format!(
                        "Question {} has only {} options.",
                        index + 1,
                        labels.len()
                    ));
                }
                if indices.is_empty() && other.is_none() {
                    return Err(format!(
                        "Question {} needs an answer: an option number or a free answer.",
                        index + 1
                    ));
                }
                said.extend(indices.iter().map(|i| labels[*i].clone()));
                if let Some(other) = &other {
                    said.push(other.clone());
                }
                let mut selection = Map::new();
                selection.insert("indices".to_string(), json!(indices));
                if let Some(other) = other {
                    selection.insert("other".to_string(), json!(other));
                }
                selections.push(Value::Object(selection));
            }
            (
                json!({"kind": "question", "selections": selections}),
                said.join(", "),
            )
        }
        PendingPrompt::Choice { options, title, .. } => {
            let Some(index) = single_option else {
                return Err(format!(
                    "Pick one of the {} options of \"{}\" by number.",
                    options.len(),
                    short(&title, 120)
                ));
            };
            let Some(option) = options.get(index) else {
                return Err(format!("There are only {} options.", options.len()));
            };
            (option.answer.clone(), option.label.clone())
        }
    };
    let mut params = answer.as_object().cloned().unwrap_or_default();
    params.insert("projectId".to_string(), json!(session.project_id));
    params.insert("sessionId".to_string(), json!(session.session_id));
    params.insert(
        "sendRequestId".to_string(),
        json!(uuid::Uuid::new_v4().to_string()),
    );
    computers::call(
        state,
        session.computer.as_ref(),
        "/api/answerSessionChatPrompt",
        Value::Object(params),
        Duration::from_secs(30),
    )
    .await?;
    Ok(json!({"ok": true, "answered": true, "session": session.brief(), "chose": said}))
}

/// A project in an ambiguity question: its name and, to tell same-named ones apart, its folder.
fn project_choice(project: &sessions::ProjectInfo) -> String {
    if let Some(computer) = &project.computer {
        return format!(
            "\"{}\" on {} (to pick it, pass project \"{}\")",
            project.name,
            computer.name,
            project.reference()
        );
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let folder = match project.path.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => project.path.clone(),
    };
    let parent = std::path::Path::new(&project.path)
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    format!(
        "\"{}\" in the \"{parent}\" folder (to pick it, pass project \"{folder}\")",
        project.name
    )
}

async fn list_projects(state: &Arc<AppState>) -> Result<Value, String> {
    let projects = sessions::list_projects_async(state).await?;
    Ok(json!({
        "ok": true,
        "projects": projects.iter().map(|project| {
            let mut entry = json!({"name": project.name, "folder": project.path});
            if let Some(computer) = &project.computer {
                entry["computer"] = json!(computer.name);
            }
            entry
        }).collect::<Vec<_>>(),
    }))
}

/// The agent types the user can start on `computer` (this one when `None`): its visible agents
/// (`ghostex agents types` reads the same list), with the user's Default Prompt Agent as the default
/// when it is one of them.
async fn agent_types(
    state: &Arc<AppState>,
    computer: Option<&computers::Computer>,
) -> Result<(Vec<(String, String)>, Option<String>), String> {
    let hud = computers::call(
        state,
        computer,
        "/api/readSidebarHud",
        json!({}),
        Duration::from_secs(10),
    )
    .await?;
    let agents: Vec<(String, String)> = hud
        .get("agents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|agent| {
            let id = agent.get("agentId")?.as_str()?.to_string();
            let name = agent
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&id)
                .to_string();
            Some((id, name))
        })
        .collect();
    let preferred = super::keys::read_settings(&state.paths)
        .get("defaultPromptAgentId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|id| agents.iter().any(|(agent, _)| agent == id));
    let default = preferred.or_else(|| agents.first().map(|(id, _)| id.clone()));
    Ok((agents, default))
}

async fn list_agent_types(state: &Arc<AppState>) -> Result<Value, String> {
    let (mut agents, default) = agent_types(state, None).await?;
    if let Some(default) = &default {
        agents.sort_by_key(|(id, _)| id != default);
    }
    Ok(json!({
        "ok": true,
        "default": default,
        "agents": agents.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>(),
    }))
}

/// The project the user named, or an error the model can speak (with choices when ambiguous).
pub(super) async fn project_named(
    state: &Arc<AppState>,
    query: &str,
) -> Result<sessions::ProjectInfo, String> {
    if query.is_empty() {
        return Err("Say which project.".to_string());
    }
    let projects = sessions::list_projects_async(state).await?;
    match sessions::match_project(&projects, query) {
        Match::One(project) => Ok(project),
        Match::Ambiguous(candidates) => Err(format!(
            "Ambiguous project \"{query}\": {}. Ask the user which one.",
            candidates
                .iter()
                .map(project_choice)
                .collect::<Vec<_>>()
                .join(" or ")
        )),
        Match::None => Err(format!(
            "No project matches \"{query}\". Call list_projects."
        )),
    }
}

async fn start_agent(state: &Arc<AppState>, args: &Value) -> ToolOutcome {
    let project = match project_named(state, &arg_text(args, "project")).await {
        Ok(project) => project,
        Err(error) => return failure(error),
    };
    let computer = project.computer.as_ref();
    let (agents, default) = match agent_types(state, computer).await {
        Ok(found) => found,
        Err(error) => return failure(error),
    };
    let requested = arg_text(args, "agent");
    let agent_id = if requested.is_empty() {
        default.or_else(|| agents.first().map(|(id, _)| id.clone()))
    } else {
        let lower = requested.to_lowercase();
        agents
            .iter()
            .find(|(id, name)| id.to_lowercase() == lower || name.to_lowercase() == lower)
            .or_else(|| {
                agents.iter().find(|(id, name)| {
                    id.to_lowercase().contains(&lower) || name.to_lowercase().contains(&lower)
                })
            })
            .map(|(id, _)| id.clone())
    };
    let Some(agent_id) = agent_id else {
        return failure(format!(
            "No agent type matches \"{requested}\". Call list_agent_types."
        ));
    };
    let created = match computers::call(
        state,
        computer,
        "/api/createAgentSession",
        json!({"agentId": agent_id, "projectId": project.project_id}),
        Duration::from_secs(30),
    )
    .await
    {
        Ok(created) => created,
        Err(error) => return failure(format!("Couldn't create the session: {error}")),
    };
    let session = created.get("session").cloned().unwrap_or(Value::Null);
    let (Some(project_id), Some(session_id)) = (
        session
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string),
        session
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string),
    ) else {
        return failure("gxserver created no session.");
    };
    if let Some(computer) = computer {
        computers::forget(state, &computer.server_id);
    }
    if let Err(error) = computers::call(
        state,
        computer,
        "/api/startSessionProvider",
        json!({"projectId": project_id, "sessionId": session_id}),
        Duration::from_secs(60),
    )
    .await
    {
        return failure(format!(
            "The session was created but its agent did not start: {error}"
        ));
    }
    let prompt = arg_text(args, "prompt");
    let mut sent = false;
    if !prompt.is_empty() {
        // The send path waits for the agent's input box, so a fresh agent gets the prompt once it is up.
        match computers::call(
            state,
            computer,
            "/api/sendSessionChatMessage",
            json!({
                "projectId": project_id,
                "sessionId": session_id,
                "text": prompt,
                "sendRequestId": uuid::Uuid::new_v4().to_string(),
            }),
            Duration::from_secs(90),
        )
        .await
        {
            Ok(_) => sent = true,
            Err(error) => {
                return ToolOutcome {
                    output: json!({"ok": false, "started": true, "project": project.name, "error": format!("The agent started but the first prompt did not go through: {error}")}),
                    ok: false,
                }
            }
        }
    }
    let mut output = json!({
        "ok": true,
        "started": true,
        "project": project.name,
        "agent": agents.iter().find(|(id, _)| *id == agent_id).map_or(agent_id.as_str(), |(_, name)| name.as_str()),
        "ref": new_session_ref(computer, &project_id, &session_id),
        "promptSent": sent,
    });
    if let Some(computer) = computer {
        output["computer"] = json!(computer.name);
    }
    ToolOutcome { output, ok: true }
}

/// A session just started: its ref as `list_sessions` gives it (global on another computer).
pub(super) fn new_session_ref(
    computer: Option<&computers::Computer>,
    project_id: &str,
    session_id: &str,
) -> String {
    match computer {
        None => format!("{project_id}:{session_id}"),
        Some(computer) => {
            crate::ids::create_global_session_ref(&computer.server_id, project_id, session_id)
        }
    }
}
