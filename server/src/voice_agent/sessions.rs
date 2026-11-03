//! What the voice agent knows about sessions and projects: every open session with a spoken-style
//! state, and matching the names the user says ("the sidebar one") to sessions and projects.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: "Add the capability for the voice assistant to see sub-threads under Main Coordinator. They should be marked as sub-threads in the session list so it's clear they're not top-level sessions. The voice assistant should be able to read from them when the user asks, but by default it should only read Main Coordinator unless the user specifically requests a sub-thread." Every orchestrator's open threads (the sessions the sidebar draws under it, resolved ones included) are listed right after it, each marked with its orchestrator's title; a name that matches a top-level session and a thread alike means the top-level one. Reading only the orchestrator by default is an instruction in prompt.rs.

use std::sync::Arc;

use serde_json::{json, Value};

use super::computers::{self, Computer, Unavailable};
use crate::coordinators::{
    classify_thread_session, thread_needs_user_approval, thread_prompt, ThreadProgress, ThreadState,
};
use crate::domain::DomainRepository;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// A session's state in the words the agent speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VoiceState {
    Working,
    WaitingOnQuestion,
    WaitingOnApproval,
    Finished,
    Sleeping,
}

impl VoiceState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::WaitingOnQuestion => "waiting on a question",
            Self::WaitingOnApproval => "waiting on an approval",
            Self::Finished => "idle (turn finished)",
            Self::Sleeping => "sleeping",
        }
    }

    fn order(self) -> u8 {
        match self {
            Self::WaitingOnApproval => 0,
            Self::WaitingOnQuestion => 1,
            Self::Finished => 2,
            Self::Working => 3,
            Self::Sleeping => 4,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SessionInfo {
    pub(super) project_id: String,
    pub(super) session_id: String,
    pub(super) title: String,
    pub(super) project: String,
    pub(super) agent: String,
    pub(super) state: VoiceState,
    /// What it waits on, when it waits ("It asked which branch to use.").
    pub(super) waiting: Option<String>,
    pub(super) last_active_at: String,
    /// The attention event of the turn the agent's Stop hook last finished (a new id per turn);
    /// `None` while the session is not in a finished-turn attention or its agent has no hooks.
    pub(super) turn_event: Option<String>,
    /// Its agent has a chat (a transcript Ghostex reads); without one it is only a terminal.
    pub(super) has_chat: bool,
    /// The orchestrator this session is a thread of, when that orchestrator is open too.
    pub(super) thread_of: Option<ThreadParent>,
    /// How many open threads it has, when it is an orchestrator.
    pub(super) threads: Option<usize>,
    /// The connected computer it runs on, when that is not this one (`computers.rs`).
    pub(super) computer: Option<Computer>,
}

#[derive(Clone, Debug)]
pub(super) struct ThreadParent {
    pub(super) reference: String,
    pub(super) title: String,
    /// Its orchestrator marked it done; it stays listed while its session is open.
    pub(super) resolved: bool,
}

impl SessionInfo {
    /// `projectId:sessionId` on this computer; the global `serverId:projectId:sessionId` on
    /// another, since ids are only unique per computer.
    pub(super) fn reference(&self) -> String {
        match &self.computer {
            None => format!("{}:{}", self.project_id, self.session_id),
            Some(computer) => crate::ids::create_global_session_ref(
                &computer.server_id,
                &self.project_id,
                &self.session_id,
            ),
        }
    }

    /// Its title in quotes, with the computer it runs on when that is not this one, for a
    /// spoken update: "Sidebar fix" on Omarchy.
    pub(super) fn spoken_title(&self) -> String {
        match &self.computer {
            None => format!("\"{}\"", self.title),
            Some(computer) => format!("\"{}\" on {}", self.title, computer.name),
        }
    }

    /// Who it is, without its state: for a result that just changed that state.
    pub(super) fn brief(&self) -> Value {
        let mut value =
            json!({"ref": self.reference(), "title": self.title, "project": self.project});
        if let Some(computer) = &self.computer {
            value["computer"] = json!(computer.name);
        }
        value
    }

    pub(super) fn to_tool_value(&self) -> Value {
        let mut value = json!({
            "ref": self.reference(),
            "title": self.title,
            "project": self.project,
            "agent": self.agent,
            "state": self.state.as_str(),
        });
        if let Some(computer) = &self.computer {
            value["computer"] = json!(computer.name);
        }
        if let Some(waiting) = &self.waiting {
            value["waitingOn"] = json!(waiting);
        }
        if let Some(parent) = &self.thread_of {
            value["subThreadOf"] = json!(parent.title);
            value["orchestratorRef"] = json!(parent.reference);
            if parent.resolved {
                value["threadDone"] = json!(true);
            }
        }
        if let Some(threads) = self.threads {
            value["orchestrator"] = json!(true);
            value["subThreads"] = json!(threads);
        }
        value
    }
}

#[derive(Clone, Debug)]
pub(super) struct ProjectInfo {
    pub(super) project_id: String,
    pub(super) name: String,
    pub(super) path: String,
    /// The connected computer it is on, when that is not this one.
    pub(super) computer: Option<Computer>,
}

impl ProjectInfo {
    /// Its id on this computer; `serverId:projectId` on another.
    pub(super) fn reference(&self) -> String {
        match &self.computer {
            None => self.project_id.clone(),
            Some(computer) => format!("{}:{}", computer.server_id, self.project_id),
        }
    }

    /// Whether `session` is one of its sessions.
    pub(super) fn holds(&self, session: &SessionInfo) -> bool {
        self.project_id == session.project_id && self.computer == session.computer
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn agent_label(agent: &str) -> String {
    let lower = agent.to_ascii_lowercase();
    for (needle, label) in [
        ("claude", "Claude"),
        ("codex", "Codex"),
        ("opencode", "OpenCode"),
        ("cursor", "Cursor"),
        ("gemini", "Gemini"),
        ("grok", "Grok"),
        ("copilot", "Copilot"),
        ("hermes", "Hermes"),
        ("empryo", "Empryo"),
        ("antigravity", "Antigravity"),
        ("pi", "Pi"),
    ] {
        if lower == needle || lower.contains(needle) {
            return label.to_string();
        }
    }
    if agent.is_empty() {
        "terminal".to_string()
    } else {
        agent.to_string()
    }
}

/// This computer's open (running or sleeping) sessions, waiting ones first. Blocking: run it off
/// the runtime.
pub(super) fn list_sessions(state: &AppState) -> Result<Vec<SessionInfo>, String> {
    let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
    list_sessions_in(&db, state.metadata.server_id.as_str())
}

fn list_sessions_in(
    db: &rusqlite::Connection,
    server_id: &str,
) -> Result<Vec<SessionInfo>, String> {
    let repository = DomainRepository::new(db, server_id);
    let projects: Vec<ProjectInfo> = project_rows(&repository)?;
    let sessions = repository
        .list_sessions_excluding_stopped(None)
        .map_err(|error| error.message)?;
    let thread_links = crate::coordinators::list_threads(db).map_err(|error| error.message)?;
    let orchestrators: std::collections::HashSet<String> =
        crate::coordinators::list_coordinators(db)
            .map_err(|error| error.message)?
            .iter()
            .map(|record| format!("{}:{}", record.project_id, record.session_id))
            .collect();
    let now = crate::presentation::now_iso();
    let rows = sorted(
        sessions
            .iter()
            .filter_map(|session| session_info(session, &projects, &now))
            .collect(),
    );
    Ok(nest_threads(rows, &thread_links, &orchestrators))
}

/// Waiting sessions first, then the most recently active.
pub(super) fn sorted(mut rows: Vec<SessionInfo>) -> Vec<SessionInfo> {
    rows.sort_by(|a, b| {
        a.state
            .order()
            .cmp(&b.state.order())
            .then_with(|| b.last_active_at.cmp(&a.last_active_at))
    });
    rows
}

/// One session row as gxserver stores it (and `/api/listSessions` answers it, on any computer),
/// with its spoken state; `None` when it is closed. `projects` are its computer's, for the name.
pub(super) fn session_info(
    session: &Value,
    projects: &[ProjectInfo],
    now: &str,
) -> Option<SessionInfo> {
    let project_id = text(session, "projectId");
    let session_id = text(session, "sessionId");
    if project_id.is_empty() || session_id.is_empty() {
        return None;
    }
    let thread = classify_thread_session(
        Some(session),
        ThreadProgress {
            resolved: false,
            has_run: true,
        },
        now,
        false,
    );
    let (state, waiting) = match thread {
        ThreadState::Working => (VoiceState::Working, None),
        // A finished turn that left questions in its chat waits on the user too
        // (server/src/session_chat_open_questions.rs).
        ThreadState::Finished
            if crate::session_chat_open_questions::open_question_count(session) > 0 =>
        {
            let count = crate::session_chat_open_questions::open_question_count(session);
            (
                VoiceState::WaitingOnQuestion,
                Some(match count {
                    1 => "It has 1 open question in its chat.".to_string(),
                    count => format!("It has {count} open questions in its chat."),
                }),
            )
        }
        ThreadState::Finished => (VoiceState::Finished, None),
        ThreadState::Sleeping => (VoiceState::Sleeping, None),
        ThreadState::Waiting => {
            let summary = thread_prompt(session).map(|prompt| prompt.summary);
            if thread_needs_user_approval(session) {
                (VoiceState::WaitingOnApproval, summary)
            } else {
                (VoiceState::WaitingOnQuestion, summary)
            }
        }
        ThreadState::Closed | ThreadState::Done => return None,
    };
    let project = projects
        .iter()
        .find(|project| project.project_id == project_id)
        .map(|project| project.name.clone())
        .unwrap_or_default();
    let title = Some(text(session, "title"))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Untitled session".to_string());
    Some(SessionInfo {
        project_id,
        session_id,
        title,
        project,
        agent: agent_label(&text(session, "agentId")),
        state,
        waiting,
        last_active_at: text(session, "lastActiveAt"),
        turn_event: turn_event(session),
        has_chat: crate::session_chat_follower::session_chat_agent_for_session(session).is_some(),
        thread_of: None,
        threads: None,
        computer: None,
    })
}

/// This computer's sessions (each orchestrator followed by its threads) and another computer's,
/// in one waiting-first list. An orchestrator keeps its threads right after it.
pub(super) fn merge(local: Vec<SessionInfo>, remote: Vec<SessionInfo>) -> Vec<SessionInfo> {
    let mut groups: Vec<Vec<SessionInfo>> = Vec::new();
    for row in local {
        match groups.last_mut() {
            Some(group) if row.thread_of.is_some() => group.push(row),
            _ => groups.push(vec![row]),
        }
    }
    groups.extend(remote.into_iter().map(|row| vec![row]));
    // Stable: this computer's rows keep their order among equals.
    groups.sort_by(|a, b| {
        a[0].state
            .order()
            .cmp(&b[0].state.order())
            .then_with(|| b[0].last_active_at.cmp(&a[0].last_active_at))
    });
    groups.into_iter().flatten().collect()
}

/// Marks each open thread of an open orchestrator and moves it right after that orchestrator,
/// keeping the waiting-first order among the threads. A thread whose orchestrator is not open
/// stays a top-level session, as in the sidebar.
fn nest_threads(
    rows: Vec<SessionInfo>,
    links: &[crate::coordinators::ThreadRecord],
    orchestrators: &std::collections::HashSet<String>,
) -> Vec<SessionInfo> {
    let titles: std::collections::HashMap<String, String> = rows
        .iter()
        .map(|row| (row.reference(), row.title.clone()))
        .collect();
    let (mut threads, top): (Vec<SessionInfo>, Vec<SessionInfo>) = rows
        .into_iter()
        .map(|mut row| {
            let reference = row.reference();
            row.thread_of = links
                .iter()
                .find(|link| format!("{}:{}", link.project_id, link.session_id) == reference)
                .and_then(|link| {
                    let parent = format!(
                        "{}:{}",
                        link.coordinator_project_id, link.coordinator_session_id
                    );
                    let title = titles.get(&parent)?.clone();
                    (parent != reference).then(|| ThreadParent {
                        reference: parent,
                        title,
                        resolved: link.is_resolved(),
                    })
                });
            row
        })
        .partition(|row| row.thread_of.is_some());
    let mut nested = Vec::with_capacity(top.len() + threads.len());
    for mut row in top {
        let reference = row.reference();
        let mine: Vec<SessionInfo> = {
            let (mine, rest): (Vec<_>, Vec<_>) = threads.into_iter().partition(|thread| {
                thread.thread_of.as_ref().map(|parent| &parent.reference) == Some(&reference)
            });
            threads = rest;
            mine
        };
        if orchestrators.contains(&reference) {
            row.threads = Some(mine.len());
        }
        nested.push(row);
        nested.extend(mine);
    }
    // Threads of an orchestrator that is itself a thread (rare) come last, still marked.
    nested.extend(threads);
    nested
}

fn turn_event(session: &Value) -> Option<String> {
    let activity = session.pointer("/runtimeSettings/agentActivity")?;
    if activity.get("attentionSource").and_then(Value::as_str)
        != Some(crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE)
    {
        return None;
    }
    activity
        .get("attentionEventId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn project_rows(repository: &DomainRepository<'_>) -> Result<Vec<ProjectInfo>, String> {
    Ok(repository
        .list_projects()
        .map_err(|error| error.message)?
        .iter()
        .filter_map(project_info)
        .collect())
}

/// One project as gxserver stores it (and `/api/listProjects` answers it, on any computer).
pub(super) fn project_info(project: &Value) -> Option<ProjectInfo> {
    let project_id = text(project, "projectId");
    if project_id.is_empty() {
        return None;
    }
    let path = text(project, "path");
    let name = Some(text(project, "name"))
        .filter(|name| !name.is_empty())
        .or_else(|| {
            std::path::Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| project_id.clone());
    Some(ProjectInfo {
        project_id,
        name,
        path,
        computer: None,
    })
}

pub(super) fn list_projects(state: &AppState) -> Result<Vec<ProjectInfo>, String> {
    let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    project_rows(&repository)
}

/// Every open session on this computer and on the connected computers that answered, waiting ones
/// first.
pub(super) async fn list_sessions_async(state: &Arc<AppState>) -> Result<Vec<SessionInfo>, String> {
    Ok(list_everywhere_async(state).await?.0)
}

/// Every open session here and on the connected computers, and the computers that could not be
/// read (Agent access off, or not answering), with why.
pub(super) async fn list_everywhere_async(
    state: &Arc<AppState>,
) -> Result<(Vec<SessionInfo>, Vec<Unavailable>), String> {
    let local_state = state.clone();
    let (local, elsewhere) = tokio::join!(
        tokio::task::spawn_blocking(move || list_sessions(&local_state)),
        computers::other_computers(state),
    );
    let local = local.map_err(|error| error.to_string())??;
    Ok((merge(local, elsewhere.sessions), elsewhere.unavailable))
}

/// This computer's projects, then the connected computers'.
pub(super) async fn list_projects_async(state: &Arc<AppState>) -> Result<Vec<ProjectInfo>, String> {
    let local_state = state.clone();
    let (local, elsewhere) = tokio::join!(
        tokio::task::spawn_blocking(move || list_projects(&local_state)),
        computers::other_computers(state),
    );
    let mut projects = local.map_err(|error| error.to_string())??;
    projects.extend(elsewhere.projects);
    Ok(projects)
}

/// Words that say nothing about which session is meant ("the sidebar one").
const FILLER: &[&str] = &[
    "the", "a", "an", "one", "session", "sessions", "agent", "chat", "thread", "that", "this",
    "my", "on", "in", "of", "for", "about", "with", "to", "and", "is", "it", "thing", "task",
];

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !FILLER.contains(word))
        .map(str::to_string)
        .collect()
}

/// How well `query` names a target described by `name` (its title) and `context` (its project and
/// agent): 0 when unrelated.
fn score(query: &str, name: &str, context: &str) -> u32 {
    let query_lower = query.trim().to_lowercase();
    let name_lower = name.to_lowercase();
    if query_lower.is_empty() {
        return 0;
    }
    if name_lower == query_lower {
        return 1000;
    }
    let query_words = words(query);
    if query_words.is_empty() {
        return 0;
    }
    let name_words = words(name);
    let context_words = words(context);
    let mut points = 0u32;
    let mut matched = 0usize;
    for word in &query_words {
        let in_name = name_words.iter().any(|candidate| {
            candidate == word
                || (word.len() >= 3 && candidate.starts_with(word.as_str()))
                || (candidate.len() >= 3 && word.starts_with(candidate.as_str()))
        });
        let in_context = context_words.iter().any(|candidate| candidate == word);
        if in_name {
            points += 100;
            matched += 1;
        } else if in_context {
            points += 40;
            matched += 1;
        }
    }
    if matched == 0 {
        return 0;
    }
    if name_lower.contains(&query_lower) {
        points += 150;
    }
    // Every word the user said matched: much stronger than a partial match.
    if matched == query_words.len() {
        points += 120;
    }
    points
}

/// Whether every word the user said (filler aside) is a word of `title`, or the start of one, as
/// `score` counts a name match: "the Discord one" for "Discord triage".
pub(super) fn title_has_every_word(title: &str, query: &str) -> bool {
    let query_words = words(query);
    let title_words = words(title);
    !query_words.is_empty()
        && query_words.iter().all(|word| {
            title_words.iter().any(|candidate| {
                candidate == word
                    || (word.len() >= 3 && candidate.starts_with(word.as_str()))
                    || (candidate.len() >= 3 && word.starts_with(candidate.as_str()))
            })
        })
}

pub(super) enum Match<T> {
    One(T),
    Ambiguous(Vec<T>),
    None,
}

/// The session `query` names: a `project:session` ref, a session id, or words of its title,
/// project or agent.
pub(super) fn match_session(sessions: &[SessionInfo], query: &str) -> Match<SessionInfo> {
    let trimmed = query.trim();
    // An exact ref first: two computers can hold the same project and session ids.
    if let Some(found) = sessions
        .iter()
        .find(|session| session.reference() == trimmed)
        .or_else(|| {
            sessions.iter().find(|session| {
                session.session_id == trimmed
                    || trimmed.ends_with(&format!(":{}", session.reference()))
            })
        })
    {
        return Match::One(found.clone());
    }
    match pick(
        sessions
            .iter()
            .map(|session| {
                let context = format!(
                    "{} {} {}",
                    session.project,
                    session.agent,
                    computer_name(&session.computer)
                );
                (score(trimmed, &session.title, &context), session.clone())
            })
            .collect(),
    ) {
        // A name that fits top-level sessions and threads alike means a top-level one: the user
        // names a thread clearly when they want it.
        Match::Ambiguous(candidates)
            if candidates.iter().any(|session| session.thread_of.is_none()) =>
        {
            let mut top: Vec<SessionInfo> = candidates
                .into_iter()
                .filter(|session| session.thread_of.is_none())
                .collect();
            if top.len() == 1 {
                Match::One(top.remove(0))
            } else {
                Match::Ambiguous(top)
            }
        }
        other => other,
    }
}

pub(super) fn match_project(projects: &[ProjectInfo], query: &str) -> Match<ProjectInfo> {
    let trimmed = query.trim();
    if let Some(found) = projects
        .iter()
        .find(|project| project.reference() == trimmed)
        .or_else(|| {
            projects
                .iter()
                .find(|project| project.project_id == trimmed)
        })
    {
        return Match::One(found.clone());
    }
    // A folder anywhere in the argument ("~/dev/_active/Ghostex", or the hint an ambiguity answer
    // gave) names the project exactly; this computer's comes first when another has the same
    // folder (an ambiguity answer names another computer's project by its ref).
    let home = std::env::var("HOME").unwrap_or_default();
    let folder = trimmed
        .split(|c: char| c.is_whitespace() || c == '"' || c == '(' || c == ')')
        .find(|part| part.starts_with('/') || part.starts_with("~/"))
        .map(|part| match part.strip_prefix('~') {
            Some(rest) if !home.is_empty() => format!("{home}{rest}"),
            _ => part.to_string(),
        })
        .map(|part| part.trim_end_matches('/').to_string());
    if let Some(folder) = folder {
        if let Some(found) = projects
            .iter()
            .find(|project| project.path.trim_end_matches('/') == folder)
        {
            return Match::One(found.clone());
        }
    }
    pick(
        projects
            .iter()
            .map(|project| {
                let context = format!("{} {}", project.path, computer_name(&project.computer));
                (score(trimmed, &project.name, &context), project.clone())
            })
            .collect(),
    )
}

fn computer_name(computer: &Option<Computer>) -> &str {
    computer
        .as_ref()
        .map_or("", |computer| computer.name.as_str())
}

/// The clear winner, or the close candidates when two score alike.
fn pick<T: Clone>(mut scored: Vec<(u32, T)>) -> Match<T> {
    scored.retain(|(points, _)| *points > 0);
    scored.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    match scored.as_slice() {
        [] => Match::None,
        [(_, only)] => Match::One(only.clone()),
        [(best, first), (second, _), ..] => {
            // Two exact names (two projects called "Ghostex") are a question, not a pick.
            if (*best >= 1000 && *second < 1000) || (*best < 1000 && *best >= second + 60) {
                Match::One(first.clone())
            } else {
                Match::Ambiguous(
                    scored
                        .iter()
                        .take_while(|(points, _)| *points + 60 > *best || *points >= 1000)
                        .take(5)
                        .map(|(_, item)| item.clone())
                        .collect(),
                )
            }
        }
    }
}
