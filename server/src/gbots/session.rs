//! A bot's chats: ordinary agent sessions in the bot's project, each registered as a coordinator
//! (so reports, threads, stall checks and the Board work for it from the start), created the way
//! Chief of Staff's are (server/src/chief/session.rs). Several chats per bot are several sessions;
//! the persona and memory they share are the record and the bot folder, not the chat.
//!
//! CDXC:GBots 2026-11-08 DECISION:
//! User (Q-C A): memory reaches a bot through generated CLAUDE.md / AGENTS.md plus `ghostex gbots brief` at start and after compaction. So every chat starts with the locked first message (the `ghostex-gbots` skill line, who it is, and the order to run `ghostex gbots brief` before its first reply and after any compaction), and the instruction files are rewritten from the record and the memory files just before each chat starts.
//! CDXC:GBots 2026-11-08 WHY:
//! `runtimeSettings.gbot = {botId, openedBy, purpose}` records who opened a chat (`user`, `chief`, later `bot:<id>`, `routine:<id>`, `room:<id>`, `api:<id>`): Chief of Staff closes only chats it opened, never the user's, and later steps send problems to whoever opened it.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::record::Bot;
use crate::agents::{apply_created_session_identity, create_agent_session_params_for_project};
use crate::coordinators::{
    coordinator_create_params, coordinator_launcher_family, register_created_coordinator,
    CoordinatorLaunch,
};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{
    dispatch_zmx_lifecycle_http_blocking, prepare_coordinator_create_params,
    queue_session_chat_prompt, restore_parked_project_for_new_session,
    schedule_presentation_session_delta, AppState,
};

/// The `runtimeSettings` key that marks a GBot chat.
pub(crate) const CHAT_KEY: &str = "gbot";
/// The skill every GBot chat is told to follow (skills/ghostex-gbots/).
pub(crate) const GBOTS_SKILL: &str = "ghostex-gbots";

/// The bot and opener a session's chat belongs to, from `runtimeSettings.gbot`.
pub(crate) fn chat_of(session: &Value) -> Option<(&str, &str)> {
    let record = session.pointer(&format!("/runtimeSettings/{CHAT_KEY}"))?;
    Some((
        record.get("botId")?.as_str()?,
        record
            .get("openedBy")
            .and_then(Value::as_str)
            .unwrap_or("user"),
    ))
}

/// The launcher rows a bot can run on (the same list as Chief of Staff's agent picker).
pub(crate) fn bot_agent_rows(
    repository: &DomainRepository<'_>,
) -> Result<Vec<Value>, DomainStateError> {
    Ok(crate::chief::chief_agent_rows(&repository.list_projects()?))
}

/// Refuses an agent id no bot can run on.
pub(crate) fn check_agent(rows: &[Value], agent_id: &str) -> Result<(), DomainStateError> {
    if rows
        .iter()
        .any(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id))
    {
        return Ok(());
    }
    let ids: Vec<&str> = rows
        .iter()
        .filter_map(|row| row.get("agentId").and_then(Value::as_str))
        .collect();
    Err(DomainStateError::bad_request(format!(
        "A GBot can't run on {agent_id}. Pick one of: {}.",
        if ids.is_empty() {
            crate::coordinators::coordinator_agent_families_text().to_string()
        } else {
            ids.join(", ")
        }
    )))
}

/// The first message of every chat of `bot`, in the agent's own skill spelling, then the task.
pub(super) fn first_message(bot: &Bot, family: &str, task: Option<&str>) -> String {
    let skill = ghostex_gx_chat_core::composer::skill_invocation::skill_invocation(
        GBOTS_SKILL,
        Some(family),
    );
    let who = if bot.title.is_empty() {
        bot.name.clone()
    } else {
        format!("{}, {}", bot.name, bot.title)
    };
    let mut text = format!(
        "Read and adhere to this skill: {skill}\n\nYou are {who}. Before your first reply, and again after any context compaction, run `ghostex gbots brief`."
    );
    match task.map(str::trim).filter(|task| !task.is_empty()) {
        Some(task) => {
            text.push_str("\n\n");
            text.push_str(task);
        }
        None => text.push_str(" Then say in one short line that you're ready and wait."),
    }
    text
}

/// A chat's title: the first line of its task (cut to 60 characters), else the bot's name. Kept
/// like a coordinator's given title (coordinators/title.rs), so the rows under a bot say what each
/// chat is for rather than the agent's default "Claude Session".
pub(super) fn chat_title(bot: &Bot, task: Option<&str>) -> String {
    let Some(line) =
        task.and_then(|task| task.lines().map(str::trim).find(|line| !line.is_empty()))
    else {
        return bot.name.clone();
    };
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title = format!("{}…", title.trim_end());
    }
    title
}

/*
CDXC:GBots 2026-11-08 WHY:
The approval level is applied through the launch the way every session's is: `ask` and `edits` turn off a "Run without asking" the launcher or the user's global setting would add (`acceptAllMode: disabled`), `edits` adds the agent's own accept-edits mode (Claude `--permission-mode acceptEdits`, Codex `--sandbox workspace-write --ask-for-approval on-request`), and `full` runs without asking (`acceptAllMode: enabled`). `auto` launches like `edits` for now: Autopilot never acts in bot chats (CDXC:Bots 2026-11-08 in bot_projects.rs), and its Decide tier would also answer the bot's questions, which Chief of Staff does for the bots it leads; whether `auto` should mean Autopilot is decided with delegation.
*/
/*
CDXC:GBots 2026-11-09 DECISION:
User: "Codex GBots start at Edits or Full", no sandbox carve-out. Codex's own sandbox below Full (`workspace-write`, which Edits maps to, and the `ghostex-home` profile's workspace shape) has the network off, and that blocks the loopback connection, so every `ghostex` call fails with "Could not connect to local gxserver"; only Full reaches gxserver. A new Codex GBot therefore starts at Full unless its approval is given; Claude's start at Ask as before.
*/
/// The approval level a new bot on `agent_id` starts at when none is given.
pub(crate) fn default_approval(rows: &[Value], agent_id: &str) -> &'static str {
    let family = rows
        .iter()
        .find(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id))
        .and_then(coordinator_launcher_family);
    if family == Some("codex") {
        "full"
    } else {
        "ask"
    }
}

fn apply_approval(params: &mut Map<String, Value>, approval: &str, family: Option<&str>) {
    let launch = params
        .entry("launchSettings".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(launch) = launch.as_object_mut() else {
        return;
    };
    let mode = if approval == "full" {
        "enabled"
    } else {
        "disabled"
    };
    launch.insert("acceptAllMode".to_string(), json!(mode));
    if !matches!(approval, "edits" | "auto") {
        return;
    }
    let extra = match family {
        Some("claude") => "--permission-mode acceptEdits",
        Some("codex") => "--sandbox workspace-write --ask-for-approval on-request",
        _ => return,
    };
    let Some(command) = launch
        .get("agentCommand")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .map(str::to_string)
    else {
        return;
    };
    let already = [
        "--permission-mode",
        "--sandbox",
        "--ask-for-approval",
        " -s ",
        " -a ",
    ]
    .iter()
    .any(|flag| command.contains(flag));
    if !already {
        launch.insert(
            "agentCommand".to_string(),
            json!(format!("{command} {extra}")),
        );
    }
}

/// Opens a new chat with `bot`: creates the session in its project as a coordinator, queues the
/// locked first message (and `task`), starts its agent. Answers the session and whether its agent
/// started.
pub(crate) fn create_bot_chat(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: &Bot,
    opened_by: &str,
    task: Option<&str>,
) -> Result<(Value, bool), DomainStateError> {
    create_bot_chat_with(
        state,
        db,
        repository,
        bot,
        &ChatOpening {
            opened_by,
            task,
            ..ChatOpening::default()
        },
    )
}

/// How a chat is opened: by whom, with what first task, and (for work handed over with
/// `ghostex gbots ask`, assign.rs) who it reports to.
#[derive(Default)]
pub(crate) struct ChatOpening<'a> {
    pub opened_by: &'a str,
    pub task: Option<&'a str>,
    /// `runtimeSettings.gbot.purpose`; `task` or `chat` from `task` when unset.
    pub purpose: Option<&'a str>,
    /// The sender's chat (`projectId:sessionId`, or `user`) for chats `ask` opens.
    pub from: Option<&'a str>,
    /// The sender's standing chat with this bot (`ask --chat standing`).
    pub standing: bool,
    /// The chat's title instead of the first line of its task (a routine's "<name> · <date>").
    pub title: Option<&'a str>,
    /// Linked as this coordinator's thread (its project and session ids) with this task before its
    /// agent starts, so the supervisor sees its first turn from the beginning.
    pub link: Option<(&'a str, &'a str, &'a str)>,
    /// The approval level this chat runs at instead of the GBot's own (the GBot API's chats run at
    /// `ask`, api/chat.rs).
    pub approval: Option<&'a str>,
    /// The folder the chat starts in instead of the GBot's own (the GBot API's chats start in
    /// their own folder, api/chat.rs).
    pub cwd: Option<&'a str>,
}

/// [`create_bot_chat`] with every option.
pub(crate) fn create_bot_chat_with(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: &Bot,
    opening: &ChatOpening<'_>,
) -> Result<(Value, bool), DomainStateError> {
    let (opened_by, task) = (opening.opened_by, opening.task);
    // A GBot given a connected computer opens its chats there (computer/).
    if let Some(opened) = super::computer::start_elsewhere(state, db, repository, bot, opening)? {
        return Ok(opened);
    }
    let agent_rows = bot_agent_rows(repository)?;
    check_agent(&agent_rows, &bot.agent_id)?;
    let project = super::project::bot_project(repository, bot)?.ok_or_else(|| {
        DomainStateError::not_found(format!(
            "{}'s project is gone. Remove the GBot and add it again.",
            bot.name
        ))
    })?;
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // A chat starts with the memory as it is now, edits made by hand included.
    super::brief::refresh_instruction_files(&state.paths, db, bot);
    let family = agent_rows
        .iter()
        .find(|row| row.get("agentId").and_then(Value::as_str) == Some(bot.agent_id.as_str()))
        .and_then(coordinator_launcher_family);
    let task = task.map(str::trim).filter(|task| !task.is_empty());
    let goal = if bot.title.is_empty() {
        format!("Be {}, one of the user's GBots.", bot.name)
    } else {
        bot.title.clone()
    };
    let mut params = coordinator_create_params(CoordinatorLaunch {
        agent_rows: &agent_rows,
        project_id: &project_id,
        agent_id: &bot.agent_id,
        title: Some(
            opening
                .title
                .map(str::to_string)
                .unwrap_or_else(|| chat_title(bot, task)),
        ),
        goal,
        task: task.unwrap_or_default().to_string(),
        model: bot.model.clone(),
        effort: bot.effort.clone(),
    })
    .as_object()
    .cloned()
    .unwrap_or_default();
    if let Some(coordinator) = params.get_mut("coordinator").and_then(Value::as_object_mut) {
        coordinator.insert(
            "instructions".to_string(),
            json!("Your standing instructions are in SOUL.md and your memory in MEMORY.md, in your folder. Run `ghostex gbots brief` to read them."),
        );
    }
    params.insert("surface".into(), json!("workspace"));
    if let Some(cwd) = opening.cwd {
        params.insert("cwd".into(), json!(cwd));
    }
    params.insert("requireLaunchCommand".into(), json!(true));
    if let Some(runtime) = params
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
    {
        let mut record = json!({
            "botId": bot.bot_id,
            "openedBy": opened_by,
            "purpose": opening
                .purpose
                .unwrap_or(if task.is_some() { "task" } else { "chat" }),
        });
        if let Some(from) = opening.from {
            record["from"] = json!(from);
        }
        if opening.standing {
            record["standing"] = json!(true);
        }
        // Whether its launch keeps the screen tools (agents/gbot_computer_use.rs).
        record["computerUse"] = json!(super::computer::computer_use(db, &bot.bot_id));
        runtime.insert(CHAT_KEY.to_string(), record);
    }
    apply_approval(
        &mut params,
        opening.approval.unwrap_or(&bot.approval),
        family,
    );
    let params = prepare_coordinator_create_params(&params)?;
    let create_params = create_agent_session_params_for_project(db, &project, &params)?;
    let created = repository.create_session(&create_params, false)?;
    let session = apply_created_session_identity(repository, &created, &create_params)?;
    register_created_coordinator(db, &params, &session)?;
    let session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Some((coordinator_project, coordinator_session, thread_task)) = opening.link {
        crate::coordinators::link_thread(
            db,
            &project_id,
            &session_id,
            coordinator_project,
            coordinator_session,
            thread_task,
        )?;
    }
    // A GBot given a Cloud Box: the chat made here is handed to the box and never starts here.
    if super::computer::box_for_chat(db, bot, opening) {
        restore_parked_project_for_new_session(state, db, repository, &project_id)?;
        schedule_presentation_session_delta(state, db, repository, &project_id, &session_id)?;
        return super::computer::hand_to_box(
            state,
            db,
            repository,
            bot,
            &project_id,
            &session_id,
            task,
        );
    }
    if let Some(family) = family {
        // The session exists: a skill that would not install or a message that would not queue is
        // reported by the chat, not a reason to leave a session nobody started.
        let _ = crate::coordinators::ensure_orchestrator_skill_installed(&state.paths, family);
        let _ = queue_session_chat_prompt(
            state,
            &project_id,
            &session_id,
            &first_message(bot, family, task),
            true,
        );
    }
    restore_parked_project_for_new_session(state, db, repository, &project_id)?;
    schedule_presentation_session_delta(state, db, repository, &project_id, &session_id)?;
    // A plain start failure is tried once more; a chat nobody supervises that still did not start
    // is a problem for Chief of Staff (problems/), while a linked chat's sender hears it from `ask`.
    let started = crate::chief::start_provider(state, db, repository, &project_id, &session_id)
        || crate::chief::start_provider(state, db, repository, &project_id, &session_id);
    if !started && opening.link.is_none() {
        super::problems::chat_did_not_start(db, &bot.bot_id, &project_id, &session_id);
    }
    let session = repository
        .get_session(&project_id, &session_id)?
        .unwrap_or(session);
    Ok((session, started))
}

/// A chat as `/api/gbots` answers it.
pub(crate) fn chat_summary(server_id: &str, session: &Value) -> Value {
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    // A chat on another computer (computer/remote_chat.rs) names that computer.
    let chat_server = session
        .get("serverId")
        .and_then(Value::as_str)
        .unwrap_or(server_id);
    json!({
        "projectId": project_id,
        "sessionId": session_id,
        "globalRef": crate::ids::create_global_session_ref(chat_server, &project_id, &session_id),
        "title": text("title"),
        "openedBy": chat_of(session).map(|(_, by)| by).unwrap_or("user"),
        "state": crate::presentation::session_attributes::effective_lifecycle_state(session),
        "createdAt": text("createdAt"),
        "updatedAt": text("updatedAt"),
    })
}

/// The bot's open chats (running or asleep), newest first.
pub(crate) fn bot_chats(
    repository: &DomainRepository<'_>,
    bot: &Bot,
) -> Result<Vec<Value>, DomainStateError> {
    let Some(project) = super::project::bot_project(repository, bot)? else {
        return Ok(Vec::new());
    };
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(repository
        .list_sessions(Some(project_id))?
        .into_iter()
        .filter(|session| {
            session.get("kind").and_then(Value::as_str) == Some("agent")
                && crate::presentation::session_attributes::is_active(session)
        })
        .collect())
}

/// Closes a chat (its transcript stays in Previous Sessions).
pub(crate) fn close_chat(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    let closed = dispatch_zmx_lifecycle_http_blocking(
        state,
        "/api/killSession".to_string(),
        "gbots-close-chat".to_string(),
        params,
    );
    if closed.response.status().is_success() {
        Ok(())
    } else {
        Err(DomainStateError {
            code: "internalError",
            message: format!(
                "The chat did not close (status {}). Close it from the sidebar.",
                closed.response.status()
            ),
        })
    }
}

/// Puts a chat to sleep (GBots turned off).
pub(crate) fn sleep_chat(state: &AppState, project_id: &str, session_id: &str) {
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    params.insert("reason".into(), json!("gbotsTurnedOff"));
    let _ = dispatch_zmx_lifecycle_http_blocking(
        state,
        "/api/sleepSession".to_string(),
        "gbots-turned-off".to_string(),
        params,
    );
}
