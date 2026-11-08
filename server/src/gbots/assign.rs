//! `ghostex gbots ask`: hand a GBot work, or ask it a question, and get its answer back the way a
//! coordinator gets a thread's report.
//!
//! The bot's chat (a new one, the sender's standing chat with that bot, or one the sender opened
//! before) is linked to the sender as a coordinator thread, so the supervisor
//! (server/coordinator_runtime.rs) forwards its final message, its waiting questions, stalls and
//! closing with no delivery code of its own. A `gbots_assignments` row (assignments.rs) adds what a
//! thread does not know: work or question, the chain it is part of, and its reworks. The sender's
//! call returns at once; the report wakes it later, headed "Researcher reports on A12".
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q-D A, plan section 1.5): you and Chief of Staff hand any GBot work; a department's lead hands work only to its own members; any other GBot (and any other coordinator) may only ask a question (`--consult`), never hand work. Chains are capped (4 deep, 24 hand-offs, no work back to a bot already above in the chain) so bots handing work to each other can never loop, and a result sent back for rework goes back at most twice before the sender tells the user.
//! CDXC:GBots 2026-11-09 WHY:
//! A question is a separate chat by default, resolved once it answers and closed when it was opened for that question, so the bot's standing chat with its sender keeps only work; work goes to that standing chat by default so the bot keeps the context of earlier work from the same sender.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::assignments::{
    ancestors, assignment_summary, count_in_root, insert_assignment, latest_assignment_in_chat,
    list_assignments, mark_rework, next_assignment_id, open_assignment_in_chat, read_assignment,
    session_key, set_status, Assignment, FROM_USER, MAX_DEPTH, MAX_PER_ROOT, MAX_REWORKS,
};
use super::record::{bot_summary, read_bot, require_bot, Bot};
use super::session::{bot_chats, chat_summary, create_bot_chat_with, ChatOpening, CHAT_KEY};
use crate::coordinators::{
    agent_message, link_thread, now_iso, pending_message_excerpt, read_coordinator,
    set_thread_pending_message, set_thread_resolved, thread_brief, BriefContext, MessageSender,
};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{dispatch_zmx_lifecycle_http_blocking, queue_session_chat_prompt, AppState};

/// A brief, `--done-when` and rework text are each kept to this many characters.
const BRIEF_MAX_CHARS: usize = 16_000;
const DONE_WHEN_MAX_CHARS: usize = 1_000;

/// Who hands the work over.
pub(super) enum Sender {
    /// You, from your terminal, computer or phone: no chat gets the report; you read the bot's.
    User,
    /// Chief of Staff's chat.
    Chief(String, String),
    /// A chat of another GBot.
    Bot(String, String, Bot),
    /// Any other coordinator's chat.
    Coordinator(String, String),
}

impl Sender {
    fn key(&self) -> String {
        match self {
            Self::User => FROM_USER.to_string(),
            Self::Chief(project, session)
            | Self::Bot(project, session, _)
            | Self::Coordinator(project, session) => session_key(project, session),
        }
    }

    fn session(&self) -> Option<(&str, &str)> {
        match self {
            Self::User => None,
            Self::Chief(project, session)
            | Self::Bot(project, session, _)
            | Self::Coordinator(project, session) => Some((project, session)),
        }
    }

    /// `runtimeSettings.gbot.openedBy` of a chat this sender opens.
    fn opened_by(&self) -> String {
        match self {
            Self::User => "user".to_string(),
            Self::Chief(..) => "chief".to_string(),
            Self::Bot(_, _, bot) => format!("bot:{}", bot.bot_id),
            Self::Coordinator(..) => "coordinator".to_string(),
        }
    }

    /// Who `caller` is, as `ask` sees it; `None` for a session that can't take a report back.
    pub(super) fn of(
        db: &Connection,
        project_and_session: Option<(&str, &str)>,
        is_user: bool,
        is_chief: bool,
    ) -> Result<Self, DomainStateError> {
        if is_user {
            return Ok(Self::User);
        }
        let Some((project, session)) = project_and_session else {
            return Err(forbidden(
                "Only you, Chief of Staff and GBots hand work to a GBot or ask it something.",
            ));
        };
        let key = (project.to_string(), session.to_string());
        if is_chief {
            return Ok(Self::Chief(key.0, key.1));
        }
        if let Some(bot) = super::record::bot_for_project(db, project)? {
            return Ok(Self::Bot(key.0, key.1, bot));
        }
        if read_coordinator(db, project, session)?.is_some() {
            return Ok(Self::Coordinator(key.0, key.1));
        }
        Err(forbidden(
            "A GBot's answer goes back to a coordinator, and this session isn't one. Ask Chief of Staff to hand the GBot the work.",
        ))
    }
}

fn forbidden(message: &str) -> DomainStateError {
    DomainStateError {
        code: "forbidden",
        message: message.to_string(),
    }
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn session_text(session: &Value, key: &str) -> String {
    session
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The sender block's fields, from the sender's own session.
fn message_sender(
    state: &AppState,
    repository: &DomainRepository<'_>,
    sender: &Sender,
) -> Option<MessageSender> {
    let (project, session_id) = sender.session()?;
    let session = repository.get_session(project, session_id).ok().flatten()?;
    let agent_id = session_text(&session, "agentId");
    let agent_name =
        crate::chief::chief_agent_rows(&repository.list_projects().unwrap_or_default())
            .iter()
            .find(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id.as_str()))
            .and_then(|row| row.get("label").or_else(|| row.get("name")))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| agent_id.clone());
    Some(MessageSender {
        agent_name,
        title: session_text(&session, "title"),
        session_id: session_id.to_string(),
        agent_id,
        agent_session_id: session
            .pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        global_ref: crate::ids::create_global_session_ref(
            state.metadata.server_id.as_str(),
            project,
            session_id,
        ),
    })
}

/// Refuses a sender that may not hand `bot` this kind of request (1.5's table).
fn check_direction(sender: &Sender, bot: &Bot, consult: bool) -> Result<(), DomainStateError> {
    if bot.paused_at.is_some() {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "{} is paused, so it takes no work or questions. Resume it in its GBot Settings first.",
                bot.name
            ),
        });
    }
    match sender {
        Sender::User | Sender::Chief(..) => Ok(()),
        Sender::Bot(_, _, own) if own.bot_id == bot.bot_id => Err(DomainStateError::bad_request(
            format!("You are {}: do it yourself, or ask another GBot.", bot.name),
        )),
        Sender::Bot(_, _, own) => {
            let leads_it = own.is_lead
                && own.department_id.is_some()
                && own.department_id == bot.department_id;
            if consult || leads_it {
                Ok(())
            } else {
                Err(forbidden(&format!(
                    "Only Chief of Staff and the lead of {}'s department hand it work. Ask it a question with --consult, or say in your report that {} should do it.",
                    bot.name, bot.name
                )))
            }
        }
        Sender::Coordinator(..) if consult => Ok(()),
        Sender::Coordinator(..) => Err(forbidden(&format!(
            "Only you, Chief of Staff and a department's lead hand a GBot work. Ask {} a question with --consult, or ask Chief of Staff to hand it the work.",
            bot.name
        ))),
    }
}

/// Where the new assignment sits in its chain: (root, parent, depth), after the chain's limits.
fn place_in_chain(
    db: &Connection,
    sender: &Sender,
    bot: &Bot,
    consult: bool,
    assignment_id: &str,
) -> Result<(String, Option<String>, i64), DomainStateError> {
    let Sender::Bot(project, session, own) = sender else {
        return Ok((assignment_id.to_string(), None, 1));
    };
    let Some(parent) = latest_assignment_in_chat(db, &session_key(project, session))? else {
        return Ok((assignment_id.to_string(), None, 1));
    };
    let depth = parent.depth + 1;
    if depth > MAX_DEPTH {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "GBots hand work on at most {MAX_DEPTH} levels deep, and this would be level {depth}. Do it yourself, or say in your report what is left."
            ),
        });
    }
    if count_in_root(db, &parent.root_id)? >= MAX_PER_ROOT {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "This piece of work has already been handed on {MAX_PER_ROOT} times. Finish it yourself, or say in your report what is left."
            ),
        });
    }
    if !consult {
        let mut above: Vec<String> = ancestors(db, &parent)?
            .into_iter()
            .map(|assignment| assignment.bot_id)
            .collect();
        above.push(parent.bot_id.clone());
        above.push(own.bot_id.clone());
        if above.contains(&bot.bot_id) {
            return Err(forbidden(&format!(
                "{} is above you in this piece of work, so it can't get work back from you. Ask it a question with --consult, or say in your report what you need.",
                bot.name
            )));
        }
    }
    Ok((parent.root_id, Some(parent.assignment_id), depth))
}

/// The chat `chat` names for this sender and bot, or `None` when a new one is to be opened.
fn pick_chat(
    db: &Connection,
    repository: &DomainRepository<'_>,
    sender: &Sender,
    bot: &Bot,
    chat: &str,
) -> Result<Option<Value>, DomainStateError> {
    let from = sender.key();
    let opened_by_sender = |session: &Value| {
        session
            .pointer(&format!("/runtimeSettings/{CHAT_KEY}/from"))
            .and_then(Value::as_str)
            == Some(from.as_str())
    };
    match chat {
        "new" => Ok(None),
        "standing" => Ok(bot_chats(repository, bot)?.into_iter().find(|session| {
            opened_by_sender(session)
                && session
                    .pointer(&format!("/runtimeSettings/{CHAT_KEY}/standing"))
                    .and_then(Value::as_bool)
                    == Some(true)
        })),
        reference => {
            let (project, session_id) = super::endpoint::chat_key(db, repository, reference)?;
            let session = repository
                .get_session(&project, &session_id)?
                .filter(|session| super::session::chat_of(session).map(|(id, _)| id) == Some(bot.bot_id.as_str()))
                .filter(|session| crate::presentation::session_attributes::is_active(session))
                .ok_or_else(|| {
                    DomainStateError::not_found(format!(
                        "{reference} isn't an open chat of {}. Use --chat new or --chat standing, or `ghostex gbots chats {}` to see its chats.",
                        bot.name, bot.name
                    ))
                })?;
            if !opened_by_sender(&session) {
                return Err(forbidden(&format!(
                    "You didn't open {reference}, so you can't hand work into it. Use --chat new or --chat standing."
                )));
            }
            Ok(Some(session))
        }
    }
}

/// What the bot reads: the task, its done-when, which assignment it is, and (for a chat that
/// reports back) the thread rules every coordinator thread gets.
fn assignment_text(assignment: &Assignment, reports_back: bool) -> String {
    let mut text = assignment.brief.trim().to_string();
    if !assignment.done_when.trim().is_empty() {
        text.push_str(&format!("\n\nDone when: {}", assignment.done_when.trim()));
    }
    if assignment.is_consult() {
        text.push_str(&format!(
            "\n\nGBots assignment {}: a question, not a job. Answer it in your final message. Don't change files, start threads or hand work on; if a real answer needs work, say what it would take and stop.",
            assignment.assignment_id
        ));
    } else {
        text.push_str(&format!(
            "\n\nGBots assignment {}: work handed to you. Your final message is your report on it.",
            assignment.assignment_id
        ));
    }
    if !reports_back {
        return text;
    }
    thread_brief(
        &BriefContext {
            goal: "",
            instructions: "",
            worktree: false,
        },
        &text,
    )
}

fn sql_time_now() -> String {
    now_iso()
}

/// Wakes a sleeping chat so it takes what was just queued; a running one takes it when idle.
pub(super) fn wake_if_asleep(state: &AppState, session: &Value) {
    if crate::presentation::session_attributes::effective_lifecycle_state(session) != "sleep" {
        return;
    }
    let mut params = Map::new();
    params.insert(
        "projectId".into(),
        json!(session_text(session, "projectId")),
    );
    params.insert(
        "sessionId".into(),
        json!(session_text(session, "sessionId")),
    );
    let _ = dispatch_zmx_lifecycle_http_blocking(
        state,
        "/api/wakeSession".to_string(),
        "gbots-ask-wake".to_string(),
        params,
    );
}

/// The thread link of a chat that already exists: (re)linked to the sender, reopened and shown
/// again if it was resolved, and its new message watched until the chat's transcript shows it.
fn relink(
    db: &Connection,
    repository: &DomainRepository<'_>,
    sender: &Sender,
    project: &str,
    session_id: &str,
    task: &str,
) -> Result<(), DomainStateError> {
    let Some((coordinator_project, coordinator_session)) = sender.session() else {
        return Ok(());
    };
    link_thread(
        db,
        project,
        session_id,
        coordinator_project,
        coordinator_session,
        task,
    )?;
    set_thread_resolved(db, project, session_id, false)?;
    if let Some(session) = repository.get_session(project, session_id)? {
        if session.get("isParked").and_then(Value::as_bool) == Some(true) {
            let mut update = Map::new();
            update.insert("projectId".into(), json!(project));
            update.insert("sessionId".into(), json!(session_id));
            update.insert("isParked".into(), json!(false));
            repository.update_session(&update)?;
        }
    }
    watch(db, sender, project, session_id, task)
}

/// The supervisor's undelivered-message watch (coordinators/endpoint.rs `watch_pending_message`).
fn watch(
    db: &Connection,
    sender: &Sender,
    project: &str,
    session_id: &str,
    task: &str,
) -> Result<(), DomainStateError> {
    if sender.session().is_none() {
        return Ok(());
    }
    let excerpt = pending_message_excerpt(task);
    if crate::coordinators::delivery_needles(&excerpt).is_empty() {
        return Ok(());
    }
    set_thread_pending_message(db, project, session_id, &excerpt, &sql_time_now())
}

fn limited(value: &str, max: usize, what: &str) -> Result<String, DomainStateError> {
    if value.chars().count() > max {
        return Err(DomainStateError::bad_request(format!(
            "{what} is limited to {max} characters. Put the details in a file in the GBot's folder and point to it."
        )));
    }
    Ok(value.trim().to_string())
}

/// `op: ask`.
pub(super) fn ask(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    sender: Sender,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if let Some(rework_id) = text(params, "rework") {
        return rework(state, db, repository, &sender, rework_id, params);
    }
    let bot = require_bot(
        db,
        text(params, "bot").ok_or_else(|| DomainStateError::bad_request("Name the GBot."))?,
    )?;
    let task = limited(
        text(params, "task").ok_or_else(|| {
            DomainStateError::bad_request(
                "Say what you want: pass --task \"<brief>\" or --body-file <file>.",
            )
        })?,
        BRIEF_MAX_CHARS,
        "A brief",
    )?;
    let done_when = limited(
        text(params, "doneWhen").unwrap_or_default(),
        DONE_WHEN_MAX_CHARS,
        "--done-when",
    )?;
    let consult = params.get("consult").and_then(Value::as_bool) == Some(true);
    check_direction(&sender, &bot, consult)?;
    let assignment_id = next_assignment_id(db)?;
    let (root_id, parent_id, depth) = place_in_chain(db, &sender, &bot, consult, &assignment_id)?;
    let chat = text(params, "chat").unwrap_or(if consult { "new" } else { "standing" });
    let existing = pick_chat(db, repository, &sender, &bot, chat)?;
    let mut assignment = Assignment {
        assignment_id: assignment_id.clone(),
        root_id,
        parent_id,
        depth,
        from_session_key: sender.key(),
        bot_id: bot.bot_id.clone(),
        chat_session_key: String::new(),
        mode: if consult { "consult" } else { "work" }.to_string(),
        brief: task.clone(),
        done_when,
        reworks: 0,
        status: "open".to_string(),
        created_at: now_iso(),
        settled_at: None,
    };
    let sender_block = message_sender(state, repository, &sender);
    let body = assignment_text(&assignment, sender_block.is_some());
    let message = match &sender_block {
        Some(from) => agent_message(from, &body),
        None => body,
    };
    let (session, started) = match existing {
        Some(session) => {
            let (project, session_id) = (
                session_text(&session, "projectId"),
                session_text(&session, "sessionId"),
            );
            assignment.chat_session_key = session_key(&project, &session_id);
            insert_assignment(db, &assignment)?;
            relink(db, repository, &sender, &project, &session_id, &task)?;
            let queued = queue_session_chat_prompt(state, &project, &session_id, &message, false);
            wake_if_asleep(state, &session);
            (session, queued.is_ok())
        }
        None => {
            let from = sender.key();
            let opened_by = sender.opened_by();
            let link = sender
                .session()
                .map(|(project, session)| (project, session, task.as_str()));
            let (session, started) = create_bot_chat_with(
                state,
                db,
                repository,
                &bot,
                &ChatOpening {
                    opened_by: &opened_by,
                    task: Some(&message),
                    purpose: Some(if consult { "consult" } else { "task" }),
                    from: Some(&from),
                    standing: !consult && chat == "standing",
                    link,
                    title: None,
                },
            )?;
            let (project, session_id) = (
                session_text(&session, "projectId"),
                session_text(&session, "sessionId"),
            );
            assignment.chat_session_key = session_key(&project, &session_id);
            insert_assignment(db, &assignment)?;
            watch(db, &sender, &project, &session_id, &task)?;
            (session, started)
        }
    };
    let _ = crate::server::schedule_presentation_session_delta(
        state,
        db,
        repository,
        &session_text(&session, "projectId"),
        &session_text(&session, "sessionId"),
    );
    let status = if started { "started" } else { "pending" };
    Ok(json!({
        "assignment": assignment_summary(&assignment, Some(&bot.name)),
        "bot": bot_summary(&bot),
        "chat": chat_summary(&state.metadata.server_id, &session),
        "status": status,
        "reportsBack": sender.session().is_some(),
    }))
}

/// `op: ask` with `rework`: sends the corrections into the assignment's chat.
fn rework(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    sender: &Sender,
    assignment_id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let assignment = read_assignment(db, assignment_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!(
            "There is no assignment {assignment_id}. `ghostex gbots assignments` lists yours."
        ))
    })?;
    let id = assignment.assignment_id.clone();
    if assignment.from_session_key != sender.key() {
        return Err(forbidden(&format!(
            "You didn't hand over {id}, so you can't send it back."
        )));
    }
    if assignment.is_open() {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "{id} hasn't reported yet. Wait for its report before sending it back."
            ),
        });
    }
    if assignment.reworks >= MAX_REWORKS {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "{id} has gone back {MAX_REWORKS} times already. Tell the user what is still missing instead of sending it back again."
            ),
        });
    }
    let corrections = limited(
        text(params, "task").ok_or_else(|| {
            DomainStateError::bad_request("Say what to fix: pass --task \"<what is missing>\".")
        })?,
        BRIEF_MAX_CHARS,
        "A rework note",
    )?;
    let bot = read_bot(db, &assignment.bot_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("The GBot {id} went to was removed."))
    })?;
    let (project, session_id) = assignment
        .chat_session_key
        .split_once(':')
        .map(|(project, session)| (project.to_string(), session.to_string()))
        .unwrap_or_default();
    let session = repository
        .get_session(&project, &session_id)?
        .filter(|session| crate::presentation::session_attributes::is_active(session))
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "{id}'s chat with {} was closed. Hand the work over again with `ghostex gbots ask`.",
                bot.name
            ))
        })?;
    let round = assignment.reworks + 1;
    let mut body = format!(
        "Rework of {id} ({round} of {MAX_REWORKS}): your report doesn't meet the brief yet.\n\n{corrections}"
    );
    if !assignment.done_when.trim().is_empty() {
        body.push_str(&format!("\n\nDone when: {}", assignment.done_when.trim()));
    }
    body.push_str("\n\nFix it, then end your turn with your report again; your final message goes back automatically.");
    let message = match message_sender(state, repository, sender) {
        Some(from) => agent_message(&from, &body),
        None => body,
    };
    mark_rework(db, &id)?;
    relink(db, repository, sender, &project, &session_id, &corrections)?;
    let queued = queue_session_chat_prompt(state, &project, &session_id, &message, false);
    wake_if_asleep(state, &session);
    let _ = crate::server::schedule_presentation_session_delta(
        state,
        db,
        repository,
        &project,
        &session_id,
    );
    let updated = read_assignment(db, &id)?.unwrap_or(assignment);
    Ok(json!({
        "assignment": assignment_summary(&updated, Some(&bot.name)),
        "bot": bot_summary(&bot),
        "chat": chat_summary(&state.metadata.server_id, &session),
        "status": if queued.is_ok() { "started" } else { "pending" },
        "reportsBack": sender.session().is_some(),
        "rework": round,
    }))
}

/// `op: assignments`: the sender's own (or, for you and Chief of Staff, `all`).
pub(super) fn assignments(
    db: &Connection,
    sender: Option<&Sender>,
    all: bool,
) -> Result<Value, DomainStateError> {
    let from = match sender {
        Some(sender) if !all => Some(sender.key()),
        _ => None,
    };
    let rows = list_assignments(db, from.as_deref(), 50)?;
    let names: std::collections::HashMap<String, String> = super::record::list_bots(db)?
        .into_iter()
        .map(|bot| (bot.bot_id, bot.name))
        .collect();
    Ok(json!({
        "assignments": rows
            .iter()
            .map(|row| assignment_summary(row, names.get(&row.bot_id).map(String::as_str)))
            .collect::<Vec<_>>(),
    }))
}

/// The line a report from a chat starts with when the chat is working on an assignment:
/// "Researcher reports on A12."
pub(crate) fn report_heading(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<String> {
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let assignment = open_assignment_in_chat(&db, &session_key(project_id, session_id)).ok()??;
    let bot = read_bot(&db, &assignment.bot_id).ok()??;
    let what = if assignment.is_consult() {
        "answers your question"
    } else {
        "reports on"
    };
    let rework = if assignment.reworks > 0 {
        format!(" (rework {} of {MAX_REWORKS})", assignment.reworks)
    } else {
        String::new()
    };
    Some(format!(
        "{} {what} {}{rework}.",
        bot.name, assignment.assignment_id
    ))
}

/// After a chat's report reached its sender: the assignment it answered is settled; a question's
/// chat is resolved, and closed when it was opened for that question. `finished` is false when the
/// chat was closed before it reported.
pub(crate) fn settle_report(state: &AppState, project_id: &str, session_id: &str, finished: bool) {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let chat = session_key(project_id, session_id);
    let Ok(Some(assignment)) = open_assignment_in_chat(&db, &chat) else {
        return;
    };
    if !finished {
        let _ = set_status(&db, &assignment.assignment_id, "closed");
        return;
    }
    if !assignment.is_consult() {
        let _ = set_status(&db, &assignment.assignment_id, "reported");
        return;
    }
    let _ = set_status(&db, &assignment.assignment_id, "answered");
    if matches!(open_assignment_in_chat(&db, &chat), Ok(Some(_))) {
        return;
    }
    let _ = set_thread_resolved(&db, project_id, session_id, true);
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let opened_for_it = repository
        .get_session(project_id, session_id)
        .ok()
        .flatten()
        .and_then(|session| {
            session
                .pointer(&format!("/runtimeSettings/{CHAT_KEY}/purpose"))
                .and_then(Value::as_str)
                .map(|purpose| purpose == "consult")
        })
        .unwrap_or(false);
    if opened_for_it {
        let _ = super::session::close_chat(state, project_id, session_id);
    }
}
