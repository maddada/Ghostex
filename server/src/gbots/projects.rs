//! The Bots space's GBots rows: one gxserver project per bot (flagged `isBot` with its `gbotId` in
//! `launchSettings`, in the bot's working folder in the sidecar's data), and one session per
//! conversation, whose chat is the conversation's mirror. Kept in step with the sidecar by a full
//! read on every event-stream connect and by each `bot` event after it.
//!
//! CDXC:GBots 2026-11-06 WHY:
//! A bot is an ordinary project and a conversation an ordinary session, as Hermes bots are
//! (bot_projects.rs), so the sidebar, the chat, the phone and the `ghostex` CLI show and drive them
//! with no new client surface: only the Bots space's filter (gx-core `BotsSource`) and the row's
//! status dot know GBots exist. A session's terminal runs `ghostex gbots attach`, a small viewer;
//! its chat sends go to the sidecar (send.rs), never into that terminal.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use serde_json::{json, Map, Value};

use super::{GBOTS_AGENT_ID, GBOTS_ATTACH_COMMAND, GBOTS_SESSION_KEY, GBOT_ID_KEY};
use crate::agents::helpers::object_from_value;
use crate::domain::{normalize_project_root_path, DomainRepository, DomainStateError};
use crate::server::{
    schedule_presentation_project_delta, schedule_presentation_session_delta, AppState,
};
use crate::storage::open_gxserver_database;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ThreadState {
    title: String,
    busy: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BotState {
    name: String,
    threads: BTreeMap<String, ThreadState>,
}

impl BotState {
    fn from_wire(bot: &Value) -> Option<(String, Self)> {
        let id = bot.get("id").and_then(Value::as_str)?.to_string();
        if bot.get("hidden").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        let mut name = bot
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Bot")
            .trim()
            .to_string();
        // The runtime names its own first bot (its `chiefOfStaff`) after the product, which in the
        // Bots space would read as a heading rather than a bot.
        // CDXC:Chief 2026-11-08 DECISION: User (Q1903): that bot shows as "Primary Bot", not "Chief of Staff", so it no longer clashes with Ghostex's Chief of Staff (server/src/chief/).
        if name == "GBots" && bot.get("chiefOfStaff").and_then(Value::as_bool) == Some(true) {
            name = "Primary Bot".to_string();
        }
        let threads = bot
            .get("tasks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|task| {
                let thread = task.get("threadId").and_then(Value::as_str)?.to_string();
                let busy = task.get("busy").and_then(Value::as_bool) == Some(true)
                    || task.get("activity").and_then(Value::as_str) == Some("working");
                let title = task
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    // The runtime titles a conversation from its first message, which may be one
                    // Ghostex typed (CDXC:SessionTitles 2026-11-08, agents/activity.rs).
                    .filter(|title| {
                        !title.is_empty()
                            && *title != "New thread"
                            && !crate::agents::is_ghostex_system_message(title)
                    })
                    .unwrap_or_default()
                    .to_string();
                Some((thread, ThreadState { title, busy }))
            })
            .collect();
        Some((id, Self { name, threads }))
    }

    fn busy(&self) -> bool {
        self.threads.values().any(|thread| thread.busy)
    }
}

static BOTS: Mutex<BTreeMap<String, BotState>> = Mutex::new(BTreeMap::new());
/// Serializes the passes that add projects and sessions, so two events never add one twice.
static SYNC_LOCK: Mutex<()> = Mutex::new(());

fn bots() -> std::sync::MutexGuard<'static, BTreeMap<String, BotState>> {
    BOTS.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether the bot is working on this conversation, as last heard.
pub(crate) fn thread_busy(thread_id: &str) -> bool {
    bots()
        .values()
        .any(|bot| bot.threads.get(thread_id).is_some_and(|thread| thread.busy))
}

/// The bot's row status: `working`, `idle`, or `offline` while the sidecar is not running here.
pub(crate) fn published_bot_activity(bot_id: &str) -> &'static str {
    if !super::sidecar::is_running() {
        return "offline";
    }
    match bots().get(bot_id) {
        Some(bot) if bot.busy() => "working",
        _ => "idle",
    }
}

/// The GBot a project stands for; `None` for every other project.
pub(crate) fn gbots_bot_id(project: &Value) -> Option<&str> {
    let launch = project.get("launchSettings")?;
    if launch.get("isBot") != Some(&Value::Bool(true)) {
        return None;
    }
    launch
        .get(GBOT_ID_KEY)?
        .as_str()
        .filter(|id| super::client::id_segment(id).is_ok())
}

/// The agent config a GBots launch into a bot project uses: Ghostex's GBots client for that bot.
/// `None` for any other project or agent.
pub(crate) fn gbots_agent_config(project: &Value, agent_id: &str) -> Option<Map<String, Value>> {
    if !agent_id.trim().eq_ignore_ascii_case(GBOTS_AGENT_ID) {
        return None;
    }
    let bot_id = gbots_bot_id(project)?;
    let mut config = object_from_value(json!({
        "agentId": GBOTS_AGENT_ID,
        "command": format!("{GBOTS_ATTACH_COMMAND} --bot {bot_id}"),
        "icon": GBOTS_AGENT_ID,
    }));
    if let Some(name) = project
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        config.insert("name".to_string(), Value::String(name.to_string()));
    }
    Some(config)
}

/// The conversation a session shows: `runtimeSettings.gbots`, else, for a GBots session that lost
/// it (a closed conversation brought back keeps only its transcript link), its `agentSessionId`
/// with the bot of its project.
pub(crate) fn session_thread(session: &Value) -> Option<(String, String)> {
    if let Some(record) = session.pointer(&format!("/runtimeSettings/{GBOTS_SESSION_KEY}")) {
        return Some((
            record.get("botId")?.as_str()?.to_string(),
            record.get("threadId")?.as_str()?.to_string(),
        ));
    }
    if session.get("agentId").and_then(Value::as_str) != Some(GBOTS_AGENT_ID) {
        return None;
    }
    let thread = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .filter(|thread| super::client::id_segment(thread).is_ok())?;
    let bot = session
        .pointer("/launchSettings/agentLaunchPlan/agentCommand")
        .and_then(Value::as_str)
        .and_then(|command| command.split("--bot ").nth(1))
        .map(|rest| {
            rest.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .filter(|bot| super::client::id_segment(bot).is_ok())?;
    Some((bot, thread.to_string()))
}

/// Reads every bot and brings the projects and sessions up to date.
pub(crate) fn full_sync(state: &AppState) {
    let Ok(answer) = super::client::get("/api/bots?messages=0") else {
        return;
    };
    let wire: Vec<Value> = answer
        .get("bots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let fresh: BTreeMap<String, BotState> = wire.iter().filter_map(BotState::from_wire).collect();
    let previous = std::mem::replace(&mut *bots(), fresh.clone());
    let _ = apply(state, &fresh, &previous);
}

/// One `bot` event.
pub(crate) fn note_bot(state: &AppState, bot: &Value) {
    let Some((id, fresh)) = BotState::from_wire(bot) else {
        return;
    };
    let previous = {
        let mut bots = bots();
        let previous = bots.clone();
        if previous.get(&id) == Some(&fresh) {
            return;
        }
        bots.insert(id.clone(), fresh.clone());
        previous
    };
    let mut one = BTreeMap::new();
    one.insert(id, fresh);
    let _ = apply(state, &one, &previous);
}

/// The sidecar started or stopped: every GBots row's dot changes.
pub(crate) fn on_sidecar_status_changed(state: &AppState) {
    let _ = with_repository(state, |db, repository| {
        for project in repository.list_projects()? {
            if gbots_bot_id(&project).is_some() {
                if let Some(project_id) = project.get("projectId").and_then(Value::as_str) {
                    schedule_presentation_project_delta(
                        state,
                        db,
                        repository,
                        project_id,
                        "projectUpdated",
                    )?;
                }
            }
        }
        Ok(())
    });
}

fn with_repository<T>(
    state: &AppState,
    run: impl FnOnce(&rusqlite::Connection, &DomainRepository<'_>) -> Result<T, DomainStateError>,
) -> Result<T, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    run(&db, &repository)
}

/// Adds each bot's project and a session per conversation, renames what was renamed, and
/// publishes rows whose status changed.
fn apply(
    state: &AppState,
    bots: &BTreeMap<String, BotState>,
    previous: &BTreeMap<String, BotState>,
) -> Result<(), DomainStateError> {
    let _guard = SYNC_LOCK.lock();
    let mut activity_updates: Vec<(String, String, bool)> = Vec::new();
    with_repository(state, |db, repository| {
        let projects = repository.list_projects()?;
        let mut by_bot: HashMap<String, Value> = HashMap::new();
        for project in projects {
            if let Some(bot_id) = gbots_bot_id(&project).map(str::to_string) {
                by_bot.insert(bot_id, project);
            }
        }
        for (bot_id, bot) in bots {
            let project = match by_bot.get(bot_id) {
                Some(project) => project.clone(),
                None => {
                    let added = add_bot_project(repository, bot_id, &bot.name)?;
                    let project_id = text(&added, "projectId");
                    schedule_presentation_project_delta(
                        state,
                        db,
                        repository,
                        &project_id,
                        "projectAdded",
                    )?;
                    added
                }
            };
            let project_id = text(&project, "projectId");
            if text(&project, "name") != bot.name {
                let mut update = Map::new();
                update.insert("projectId".to_string(), json!(project_id));
                update.insert("name".to_string(), json!(bot.name));
                repository.update_project(&update)?;
                schedule_presentation_project_delta(
                    state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
            } else if previous.get(bot_id).map(BotState::busy) != Some(bot.busy()) {
                schedule_presentation_project_delta(
                    state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
            }
            let sessions = repository.list_sessions(Some(&project_id))?;
            for (thread_id, thread) in &bot.threads {
                let twins: Vec<&Value> = sessions
                    .iter()
                    .filter(|session| {
                        session_thread(session).is_some_and(|(_, thread)| &thread == thread_id)
                    })
                    .collect();
                let generated = [bot.name.as_str(), thread.title.as_str()];
                let merged = merge_twins(state, db, repository, &project_id, &twins, &generated)?;
                let existing = merged.as_ref();
                let session_id = match existing {
                    Some(session) => {
                        rename_untouched_conversation(
                            state,
                            db,
                            repository,
                            &project_id,
                            session,
                            &thread.title,
                            &bot.name,
                        )?;
                        text(session, "sessionId")
                    }
                    None => {
                        let title = if thread.title.is_empty() {
                            bot.name.as_str()
                        } else {
                            thread.title.as_str()
                        };
                        let session = add_conversation_session(
                            db, repository, &project, bot_id, thread_id, title,
                        )?;
                        let session_id = text(&session, "sessionId");
                        schedule_presentation_session_delta(
                            state,
                            db,
                            repository,
                            &project_id,
                            &session_id,
                        )?;
                        session_id
                    }
                };
                let was_busy = previous
                    .get(bot_id)
                    .and_then(|bot| bot.threads.get(thread_id))
                    .map(|thread| thread.busy);
                if was_busy != Some(thread.busy) {
                    activity_updates.push((project_id.clone(), session_id, thread.busy));
                }
            }
        }
        Ok(())
    })?;
    for (project_id, session_id, busy) in activity_updates {
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert(
            "activity".to_string(),
            json!(if busy { "working" } else { "idle" }),
        );
        let _ = crate::server::dispatch_agent_http_blocking(
            state,
            "/api/updateAgentActivity".to_string(),
            format!("gbots-activity-{session_id}"),
            params,
        );
    }
    Ok(())
}

/// The one session a conversation keeps when it has several, and the others removed.
///
/// CDXC:GBots 2026-11-08 DECISION:
/// User, through the coordinator, after every new GBots conversation showed twice in the sidebar: a conversation is one session. "+" (`new_conversation`) and the sync pass for the runtime's `bot` event each added a session for the new thread, the sync first while the POST that made the thread was answered; "+" now takes the session the sync added. The twins already made merge here: the one the user used stays (a title they gave, a pin, favorite or tag, then the latest activity), takes what only the other had (their title, pin, favorite, tag), and the other goes, with its terminal.
fn merge_twins(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    twins: &[&Value],
    generated: &[&str],
) -> Result<Option<Value>, DomainStateError> {
    let user_titled = |session: &Value| user_titled(session, generated);
    let Some((keep, rest)) = twins
        .iter()
        .copied()
        .max_by_key(|session| used_rank(session, generated))
        .map(|keep| {
            let rest: Vec<&Value> = twins
                .iter()
                .copied()
                .filter(|session| text(session, "sessionId") != text(keep, "sessionId"))
                .collect();
            (keep, rest)
        })
    else {
        return Ok(None);
    };
    if rest.is_empty() {
        return Ok(Some(keep.clone()));
    }
    let mut update = Map::new();
    for other in &rest {
        if !user_titled(keep) && user_titled(other) && !update.contains_key("title") {
            update.insert("title".to_string(), json!(text(other, "title")));
            let mut runtime = keep
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            runtime.remove("titleSource");
            runtime.remove(GBOTS_TITLE_KEY);
            update.insert("runtimeSettings".to_string(), Value::Object(runtime));
        }
        for flag in ["isPinned", "isFavorite"] {
            if other.get(flag).and_then(Value::as_bool) == Some(true)
                && keep.get(flag).and_then(Value::as_bool) != Some(true)
            {
                update.insert(flag.to_string(), json!(true));
            }
        }
        if keep.get("sessionTag").is_none_or(Value::is_null) && !update.contains_key("sessionTag") {
            if let Some(tag) = other.get("sessionTag").filter(|tag| tag.is_string()) {
                update.insert("sessionTag".to_string(), tag.clone());
            }
        }
    }
    for other in &rest {
        if let Some(zmx_name) = other.get("zmxName").and_then(Value::as_str) {
            if let Ok(zmx) = crate::toolchain::require_bundled_zmx() {
                crate::zmx::kill_zmx_session(zmx_name, &zmx.executable_path);
            }
        }
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(text(other, "sessionId")));
        crate::server::remove_session_row(state, db, repository, &params)?;
    }
    let keep_id = text(keep, "sessionId");
    let kept = if update.is_empty() {
        keep.clone()
    } else {
        update.insert("projectId".to_string(), json!(project_id));
        update.insert("sessionId".to_string(), json!(keep_id));
        repository.update_session_for_lifecycle(&update)?
    };
    schedule_presentation_session_delta(state, db, repository, project_id, &keep_id)?;
    Ok(Some(kept))
}

/// A title the user gave a conversation (any rename: desktop, phone, CLI).
fn user_titled(session: &Value, generated: &[&str]) -> bool {
    !gbots_owns_title(session, generated)
}

/// Whether the session's title is still one GBots gave it, which the sync may replace.
///
/// CDXC:GBots 2026-11-08 DECISION:
/// User, through the coordinator: a GBots conversation the user renamed keeps their title. A rename goes through several doors (the desktop's and phone's rename request, `ghostex rename-session`'s plain session update) and none of them touched `titleSource`, so the runtime's title for the conversation replaced the user's. The sync now records the title it writes (`gbotsTitle`) and only replaces a title that still equals it; any other title is the user's, whichever door renamed it. A session from before the record counts as GBots' while its title is the bot's name, the runtime's title or a Ghostex message.
fn gbots_owns_title(session: &Value, generated: &[&str]) -> bool {
    if session
        .pointer("/runtimeSettings/titleSource")
        .and_then(Value::as_str)
        != Some(GBOTS_TITLE_SOURCE)
    {
        return false;
    }
    let title = text(session, "title");
    let title = title.trim();
    match session
        .pointer(&format!("/runtimeSettings/{GBOTS_TITLE_KEY}"))
        .and_then(Value::as_str)
    {
        Some(given) => title == given.trim(),
        None => {
            title.is_empty()
                || generated.contains(&title)
                || crate::agents::is_ghostex_system_message(title)
        }
    }
}

/// How much the user used a session, to pick which of a conversation's twins stays.
fn used_rank(
    session: &Value,
    generated: &[&str],
) -> (bool, bool, String, std::cmp::Reverse<String>) {
    let marked = ["isPinned", "isFavorite"]
        .iter()
        .any(|flag| session.get(*flag).and_then(Value::as_bool) == Some(true))
        || session.get("sessionTag").is_some_and(Value::is_string);
    (
        user_titled(session, generated),
        marked,
        text(session, "lastActiveAt"),
        std::cmp::Reverse(text(session, "createdAt")),
    )
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn add_bot_project(
    repository: &DomainRepository<'_>,
    bot_id: &str,
    name: &str,
) -> Result<Value, DomainStateError> {
    // The folder the bot's agent works in (its memory, skills and files), not its settings folder.
    let folder = super::sidecar::data_dir().join("workspaces").join(bot_id);
    let _ = std::fs::create_dir_all(&folder);
    let path = Value::String(folder.to_string_lossy().to_string());
    let normalized = normalize_project_root_path(Some(&path), "path", false)?;
    let params = object_from_value(json!({
        "path": normalized,
        "name": name,
        "launchSettings": { "isBot": true, GBOT_ID_KEY: bot_id },
        "deletedDefaultCommandIds": crate::sidebar_hud::default_sidebar_command_ids()
            .collect::<Vec<_>>(),
    }));
    repository.add_project_path(&params)
}

/// A session row for one conversation, not started: its terminal starts the GBots client when it
/// is first opened.
fn add_conversation_session(
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project: &Value,
    bot_id: &str,
    thread_id: &str,
    title: &str,
) -> Result<Value, DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(text(project, "projectId")));
    params.insert("agentId".to_string(), json!(GBOTS_AGENT_ID));
    params.insert("title".to_string(), json!(title));
    let project = repository.resolve_create_session_project(&params)?;
    let mut create = crate::agents::create_agent_session_params_for_project(db, &project, &params)?;
    let mut runtime = create
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    stamp_thread(&mut runtime, bot_id, thread_id);
    runtime.insert("titleSource".to_string(), json!(GBOTS_TITLE_SOURCE));
    runtime.insert(GBOTS_TITLE_KEY.to_string(), json!(title));
    create.insert("runtimeSettings".to_string(), Value::Object(runtime));
    repository.create_session(&create, false)
}

/// Records which conversation a session shows, and its transcript.
pub(crate) fn stamp_thread(runtime: &mut Map<String, Value>, bot_id: &str, thread_id: &str) {
    runtime.insert(
        GBOTS_SESSION_KEY.to_string(),
        json!({ "botId": bot_id, "threadId": thread_id }),
    );
    runtime.insert("agentSessionId".to_string(), json!(thread_id));
    runtime.insert(
        "agentSessionPath".to_string(),
        json!(super::mirror::mirror_path(thread_id)),
    );
}

/// The project that stands for a bot, once the sync added it.
pub(crate) fn project_for_bot(state: &AppState, bot_id: &str) -> Option<String> {
    with_repository(state, |_, repository| {
        Ok(repository
            .list_projects()?
            .into_iter()
            .find(|project| gbots_bot_id(project) == Some(bot_id))
            .map(|project| text(&project, "projectId")))
    })
    .ok()
    .flatten()
}

/// Every bot with its project, status and conversations, for `ghostex gbots list`.
pub(crate) fn list_value(state: &AppState) -> Value {
    let bots = bots().clone();
    Value::Array(
        bots.iter()
            .map(|(id, bot)| {
                json!({
                    "botId": id,
                    "name": bot.name,
                    "projectId": project_for_bot(state, id),
                    "status": published_bot_activity(id),
                    "conversations": bot.threads.iter().map(|(thread, state)| json!({
                        "threadId": thread,
                        "title": state.title,
                        "working": state.busy,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// The conversation session of the bot that runs `ghostex` now, as the CLI's caller record (the
/// fields a message header names): the one working conversation, or among several the one whose
/// bot works in `cwd`.
pub(crate) fn calling_conversation(state: &AppState, cwd: &str) -> Option<Value> {
    let working: Vec<(String, String)> = bots()
        .iter()
        .flat_map(|(bot_id, bot)| {
            bot.threads
                .iter()
                .filter(|(_, thread)| thread.busy)
                .map(move |(thread_id, _)| (bot_id.clone(), thread_id.clone()))
        })
        .collect();
    let in_folder = |bot_id: &str| {
        let data = super::sidecar::data_dir();
        [
            data.join("workspaces").join(bot_id),
            data.join("bots").join(bot_id),
        ]
        .iter()
        .any(|folder| std::path::Path::new(cwd).starts_with(folder))
    };
    let (bot_id, thread_id) = match working.as_slice() {
        [only] => only.clone(),
        several => several
            .iter()
            .find(|(bot_id, _)| in_folder(bot_id))?
            .clone(),
    };
    let (project, session) = conversation_session(state, &bot_id, &thread_id)?;
    let project_id = text(&project, "projectId");
    let session_id = text(&session, "sessionId");
    Some(json!({
        "globalRef": format!("{}:{project_id}:{session_id}", state.metadata.server_id),
        "projectId": project_id,
        "projectName": text(&project, "name"),
        "sessionId": session_id,
        // Who a thread's message card says it is from: the bot, not its conversation.
        "title": text(&project, "name"),
        "agentId": super::GBOTS_AGENT_ID,
        "agentName": text(&project, "name"),
        "agentSessionId": thread_id,
    }))
}

/// The bot's project and the session of its conversation `thread_id`.
pub(crate) fn conversation_session(
    state: &AppState,
    bot_id: &str,
    thread_id: &str,
) -> Option<(Value, Value)> {
    with_repository(state, |_, repository| {
        let project = repository
            .list_projects()?
            .into_iter()
            .find(|project| gbots_bot_id(project) == Some(bot_id));
        let Some(project) = project else {
            return Ok(None);
        };
        let project_id = text(&project, "projectId");
        Ok(repository
            .list_sessions(Some(&project_id))?
            .into_iter()
            .find(|session| session_thread(session).is_some_and(|(_, thread)| thread == thread_id))
            .map(|session| (project, session)))
    })
    .ok()
    .flatten()
}

/// A new conversation with the bot a project stands for (its "+"): a conversation in the sidecar
/// and its session here, which the client then opens.
pub(crate) fn new_conversation(
    state: &AppState,
    project_id: &str,
) -> Result<(String, String), String> {
    let project = with_repository(state, |_, repository| repository.get_project(project_id))
        .map_err(|error| error.message)?
        .ok_or("That bot is gone.")?;
    let bot_id = gbots_bot_id(&project)
        .ok_or("That project is not a GBot.")?
        .to_string();
    let created = super::client::post(
        &format!("/api/bots/{bot_id}/tasks"),
        &json!({ "select": false }),
    )?;
    let thread_id = created
        .pointer("/task/threadId")
        .and_then(Value::as_str)
        .ok_or("GBots did not start a conversation.")?
        .to_string();
    let _guard = SYNC_LOCK.lock();
    let session = with_repository(state, |db, repository| {
        // The runtime announces the new conversation in a `bot` event, whose sync pass may have
        // added its session while the POST above was answered: that session is this one.
        if let Some(session) = repository
            .list_sessions(Some(project_id))?
            .into_iter()
            .find(|session| session_thread(session).is_some_and(|(_, thread)| thread == thread_id))
        {
            return Ok(session);
        }
        let session = add_conversation_session(
            db,
            repository,
            &project,
            &bot_id,
            &thread_id,
            &text(&project, "name"),
        )?;
        schedule_presentation_session_delta(
            state,
            db,
            repository,
            project_id,
            &text(&session, "sessionId"),
        )?;
        Ok(session)
    })
    .map_err(|error| error.message)?;
    if let Some(bot) = bots().get_mut(&bot_id) {
        bot.threads.insert(
            thread_id,
            ThreadState {
                title: String::new(),
                busy: false,
            },
        );
    }
    Ok((project_id.to_string(), text(&session, "sessionId")))
}

/// The `titleSource` of a conversation's session while GBots names it: the bot's name, then the
/// conversation's own title once the runtime gives it one.
///
/// CDXC:GBots 2026-11-07 WHY: a thread a bot starts shows "Message from <the bot's conversation>", named by that session's live title like every agent's card, so the conversation must not be called "Conversation". As a Hermes bot's session is titled after the bot until Hermes names it, a new conversation is titled after its bot and takes the runtime's title for it (from its first message) when one arrives; a title the user gave stays.
const GBOTS_TITLE_SOURCE: &str = "gbots";

/// The session's `runtimeSettings` key holding the title GBots last gave it.
const GBOTS_TITLE_KEY: &str = "gbotsTitle";

fn rename_untouched_conversation(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session: &Value,
    title: &str,
    bot_name: &str,
) -> Result<(), DomainStateError> {
    let untouched = gbots_owns_title(session, &[bot_name, title]);
    // A title a Ghostex message gave it before Ghostex filtered those goes back to the bot's name
    // until the runtime has a real one.
    let title =
        if title.is_empty() && crate::agents::is_ghostex_system_message(&text(session, "title")) {
            bot_name
        } else {
            title
        };
    if !untouched || title.is_empty() || text(session, "title") == title {
        return Ok(());
    }
    let mut update = Map::new();
    for key in ["projectId", "sessionId"] {
        update.insert(
            key.to_string(),
            session.get(key).cloned().unwrap_or_default(),
        );
    }
    update.insert("title".to_string(), json!(title));
    let mut runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime.insert(GBOTS_TITLE_KEY.to_string(), json!(title));
    update.insert("runtimeSettings".to_string(), Value::Object(runtime));
    repository.update_session(&update)?;
    schedule_presentation_session_delta(
        state,
        db,
        repository,
        project_id,
        &text(session, "sessionId"),
    )
}
