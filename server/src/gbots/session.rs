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
use serde_json::{Map, Value, json};

use super::record::Bot;
use crate::agents::{apply_created_session_identity, create_agent_session_params_for_project};
use crate::coordinators::{
    CoordinatorLaunch, coordinator_create_params, coordinator_launcher_family,
    register_created_coordinator,
};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{
    AppState, dispatch_zmx_lifecycle_http_blocking, prepare_coordinator_create_params,
    queue_session_chat_prompt, restore_parked_project_for_new_session,
    schedule_presentation_session_delta,
};

/// The `runtimeSettings` key that marks a GBot chat.
pub(crate) const CHAT_KEY: &str = "gbot";
/// The retired GBot skill, named only for an older computer (the playbook is gbots/playbook.md).
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

/// The first message of every chat of `bot`: the playbook from `ghostex gbots guide` (the old skill
/// in the agent's own spelling for an older computer), who it is, then the task.
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
        "{}\n\nYou are {who}. Before your first reply, and again after any context compaction, run `ghostex gbots guide` and `ghostex gbots brief`.",
        gbots_guide_line(&skill)
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

/// The first line of every GBot chat: the playbook from `ghostex gbots guide`, naming the retired
/// skill (`skill`, in the agent's spelling) only for an older computer that predates the guide.
///
/// CDXC:AgentSkills 2026-11-20 DECISION: User (Q4262 "B"): Ghostex's own skills are retired for CLI help, so the playbook comes from `ghostex gbots guide` (gbots/playbook.md) instead of the `ghostex-gbots` skill. A chat this computer opens on another computer runs that computer's `ghostex`, which is older when it predates the guide; such a computer still installs the skill for a visiting chat (gbots/skill.rs), so the line names it as the way back there.
fn gbots_guide_line(skill: &str) -> String {
    format!(
        "{}run `ghostex gbots guide` and follow what it prints for the whole session. If this Ghostex says `guide` is not a gbots command, read the {skill} skill instead.",
        ghostex_gx_chat_core::composer::skill_invocation::SKILL_LINE_PREFIX
    )
}

/// `text` with the old GBot first line ("Read and adhere to this skill: $ghostex-gbots", in any
/// agent's spelling) replaced by [`gbots_guide_line`]; `None` when it has none.
pub(crate) fn with_current_gbots_line(text: &str) -> Option<String> {
    use ghostex_gx_chat_core::composer::skill_invocation::{SKILL_LINE_PREFIX, skill_invocation};
    for agent in ["claude", "codex", "pi", "opencode"] {
        let skill = skill_invocation(GBOTS_SKILL, Some(agent));
        let Some(rest) = text.strip_prefix(&format!("{SKILL_LINE_PREFIX}{skill}")) else {
            continue;
        };
        if rest.is_empty() || rest.starts_with(['\n', '\r']) {
            return Some(format!("{}{rest}", gbots_guide_line(&skill)));
        }
    }
    None
}

/// Rewrites, at startup before any queue sends, the GBot first messages still waiting in a chat
/// queue that start with the old skill line; returns how many changed.
///
/// CDXC:AgentSkills 2026-11-20 WHY:
/// Opus review, finding 4: a GBot chat opened just before the update keeps its first message in
/// the queue until its agent takes it, and that message named `$ghostex-gbots`, which the first
/// start after the update removes. The brief (brief.rs) tells chats already running.
pub(crate) fn rewrite_queued_gbots_first_lines(
    db: &rusqlite::Connection,
) -> rusqlite::Result<usize> {
    let prefix = format!(
        "{}{}",
        ghostex_gx_chat_core::composer::skill_invocation::SKILL_LINE_PREFIX,
        "%"
    );
    let rows: Vec<(String, String, String)> = {
        let mut statement = db.prepare(
            "SELECT DISTINCT projectId, sessionId, text FROM session_chat_queued_prompts WHERE text LIKE ?1",
        )?;
        let rows = statement
            .query_map(rusqlite::params![prefix], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut changed = 0;
    for (project_id, session_id, text) in rows {
        let Some(current) = with_current_gbots_line(&text) else {
            continue;
        };
        let version = crate::session_chat_queue::bump_queue_version(db, &project_id, &session_id)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        changed += db.execute(
            "UPDATE session_chat_queued_prompts SET text = ?4, updatedAt = ?5, version = ?6 WHERE projectId = ?1 AND sessionId = ?2 AND text = ?3",
            rusqlite::params![project_id, session_id, text, current, now, version as i64],
        )?;
    }
    Ok(changed)
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
        // The session exists: a message that would not queue is reported by the chat, not a
        // reason to leave a session nobody started. Its playbook comes from `ghostex gbots guide`.
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
