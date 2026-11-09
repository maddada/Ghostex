//! `/api/gbots` `dayWatch` (where it stands), `dayWatchOn` (set it up, or turn it back on) and
//! `dayWatchOff` (pause it): you and Chief of Staff.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::super::endpoint::Asker;
use super::super::record::{bot_summary, find_bot, read_bot, update_bot, Bot};
use super::store::{self, DayWatchRecord};
use super::text::*;
use crate::automations::{routine_view, save_routine, set_routine_enabled_by_id, RoutineSave};
use crate::coordinators::now_iso;
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

pub(crate) fn handles(op: &str) -> bool {
    matches!(op, "dayWatch" | "dayWatchOn" | "dayWatchOff")
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

pub(crate) fn run(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    asker: Asker,
    op: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    match op {
        "dayWatch" => status(state, db, repository),
        _ if !asker.may_manage() => Err(forbidden(
            "Only you and Chief of Staff turn the day watch on or off.",
        )),
        "dayWatchOn" => turn_on(state, db, repository, asker, params),
        _ => turn_off(db),
    }
}

/// Whether turning the day watch on would add its GBot (none yet, or it was removed), which asks
/// for approval like adding any GBot (server/src/server/security_gates.rs). A request `run` would
/// refuse anyway (a bad brief time, Connectors off, neither Gmail nor Google Calendar connected)
/// asks nobody: the user would approve and then hear what to connect.
pub(crate) fn adds_bot(state: &AppState, params: &Map<String, Value>) -> bool {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return false;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(record) = store::read(&db) else {
        return false;
    };
    if !matches!(watch_bot(&db, &record), Ok(None))
        || brief_time(params).is_err()
        || !crate::connectors::connectors_enabled(&state.paths)
    {
        return false;
    }
    let workspace = watch_workspace(state, &db, &repository, None);
    super::offer::connected_apps(&state.paths, &workspace)
        .iter()
        .any(|app| app == "gmail" || app == "googlecalendar")
}

/// The day watch's GBot, while it is still there.
fn watch_bot(db: &Connection, record: &DayWatchRecord) -> Result<Option<Bot>, DomainStateError> {
    match record.bot_id.as_deref() {
        Some(bot_id) => read_bot(db, bot_id),
        None => Ok(None),
    }
}

fn routines(db: &Connection, record: &DayWatchRecord) -> Result<Vec<Value>, DomainStateError> {
    let mut routines = Vec::new();
    for id in [&record.brief_routine_id, &record.watch_routine_id]
        .into_iter()
        .flatten()
    {
        if let Some(view) = routine_view(db, id)? {
            routines.push(view);
        }
    }
    Ok(routines)
}

/// The workspace whose connectors the day watch reads: Chief of Staff's project's (where Main
/// Hub's home is), else the default one.
pub(super) fn watch_workspace(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: Option<&Bot>,
) -> String {
    let project_id = bot
        .map(|bot| bot.project_id.clone())
        .filter(|id| !id.is_empty())
        .or_else(|| crate::chief::chief_key(&state.paths).map(|(project, _)| project));
    project_id
        .and_then(|id| repository.get_project(&id).ok().flatten())
        .and_then(|project| crate::connectors::project_workspace(db, repository, &project).ok())
        .unwrap_or_else(|| crate::workspaces::DEFAULT_WORKSPACE_ID.to_string())
}

/// Whether one read of each app works now: `(mail, calendar)`, or why Connectors can't be used.
fn probe(state: &AppState, workspace: &str) -> Result<(bool, bool), String> {
    if !crate::connectors::connectors_enabled(&state.paths) {
        return Err(crate::connectors::connectors_off_message(&state.paths));
    }
    let call = |tool: &str, arguments: Value| {
        let mut params = Map::new();
        params.insert("workspaceId".into(), json!(workspace));
        params.insert("tool".into(), json!(tool));
        params.insert("arguments".into(), arguments);
        crate::connectors::call_composio_tool(state, &params)
            .map(|answer| {
                let output = answer["output"].as_str().unwrap_or_default();
                !output.starts_with("Error: ")
            })
            .unwrap_or(false)
    };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mail = call(
        crate::automations::MAIL_TOOL,
        json!({ "query": IMPORTANT_MAIL_QUERY, "max_results": 1, "ids_only": true }),
    );
    let calendar = call(
        crate::automations::CALENDAR_TOOL,
        json!({ "calendarId": "primary", "timeMin": now, "maxResults": 1, "singleEvents": true }),
    );
    Ok((mail, calendar))
}

fn status(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> Result<Value, DomainStateError> {
    let record = store::read(db)?;
    let bot = watch_bot(db, &record)?;
    let routines = routines(db, &record)?;
    let on = bot.as_ref().is_some_and(|bot| bot.paused_at.is_none())
        && !routines.is_empty()
        && routines
            .iter()
            .any(|routine| routine["enabled"] == json!(true));
    let workspace = watch_workspace(state, db, repository, bot.as_ref());
    let apps = super::offer::connected_apps(&state.paths, &workspace);
    Ok(json!({
        "state": if bot.is_none() { "off" } else if on { "on" } else { "paused" },
        "bot": bot.as_ref().map(bot_summary),
        "routines": routines,
        "workspace": workspace,
        "connectorsOn": crate::connectors::connectors_enabled(&state.paths),
        "gmailConnected": apps.contains(&"gmail".to_string()),
        "calendarConnected": apps.contains(&"googlecalendar".to_string()),
        "offeredAt": record.offered_at,
        "setUpAt": record.set_up_at,
    }))
}

/// `HH:MM`, or why not.
fn brief_time(params: &Map<String, Value>) -> Result<String, DomainStateError> {
    let time = text(params, "briefTime").unwrap_or(DEFAULT_BRIEF_TIME);
    let valid = time.split_once(':').is_some_and(|(hours, minutes)| {
        hours.len() <= 2
            && minutes.len() == 2
            && hours.parse::<u32>().is_ok_and(|hours| hours < 24)
            && minutes.parse::<u32>().is_ok_and(|minutes| minutes < 60)
    });
    if !valid {
        return Err(DomainStateError::bad_request(
            "Give the brief's time as HH:MM, like 08:00.",
        ));
    }
    Ok(time.to_string())
}

fn with_zone(mut schedule: Value, params: &Map<String, Value>) -> Value {
    if let Some(zone) = text(params, "timezone") {
        schedule["timezone"] = json!(zone);
    }
    schedule
}

/// The watch's check: important mail and meetings about to start, the ones that can be read.
fn watch_check(workspace: &str, mail: bool, calendar: bool) -> Value {
    let mut checks = Vec::new();
    if mail {
        checks
            .push(json!({ "kind": "mail", "query": IMPORTANT_MAIL_QUERY, "workspace": workspace }));
    }
    if calendar {
        checks.push(json!({
            "kind": "calendar",
            "withinMinutes": STARTING_SOON_MINUTES,
            "workspace": workspace,
        }));
    }
    match checks.len() {
        1 => checks.remove(0),
        _ => json!({ "kind": "any", "checks": checks }),
    }
}

/// A name for the GBot no other GBot has.
fn free_name(db: &Connection) -> Result<String, DomainStateError> {
    for number in 1..=20 {
        let name = if number == 1 {
            BOT_NAME.to_string()
        } else {
            format!("{BOT_NAME} {number}")
        };
        if find_bot(db, &name)?.is_none() {
            return Ok(name);
        }
    }
    Err(DomainStateError::bad_request(format!(
        "Rename one of your GBots called {BOT_NAME} first."
    )))
}

fn turn_on(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    asker: Asker,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let time = brief_time(params)?;
    let mut record = store::read(db)?;
    let existing = watch_bot(db, &record)?;
    let workspace = watch_workspace(state, db, repository, existing.as_ref());
    let (mail, calendar) = probe(state, &workspace).map_err(DomainStateError::bad_request)?;
    if !mail && !calendar {
        return Err(DomainStateError::bad_request(
            "Connect Gmail and Google Calendar in Settings > Connectors first.",
        ));
    }
    let mut answer = json!({});
    let bot = match existing {
        Some(mut bot) => {
            if bot.paused_at.take().is_some() {
                bot.updated_at = now_iso();
                update_bot(db, &bot)?;
            }
            bot
        }
        None => {
            let mut create = Map::new();
            create.insert("name".into(), json!(free_name(db)?));
            create.insert("title".into(), json!(BOT_TITLE));
            create.insert("description".into(), json!(BOT_DESCRIPTION));
            create.insert("soul".into(), json!(SOUL));
            create.insert("icon".into(), json!(BOT_ICON));
            create.insert("color".into(), json!(BOT_COLOR));
            let created = super::super::endpoint::create(
                state,
                db,
                repository,
                asker,
                &create,
                asker.label(),
            )?;
            let bot_id = created["bot"]["botId"].as_str().unwrap_or_default();
            let bot = read_bot(db, bot_id)?.ok_or_else(|| {
                DomainStateError::not_found("The day watch's GBot was removed while it was set up.")
            })?;
            answer["changeId"] = created["changeId"].clone();
            if created.get("chatCard").is_some() {
                answer["chatCard"] = created["chatCard"].clone();
            }
            record = DayWatchRecord {
                bot_id: Some(bot.bot_id.clone()),
                offered_at: record.offered_at,
                set_up_at: Some(now_iso()),
                set_up_by: Some(asker.label().to_string()),
                ..DayWatchRecord::default()
            };
            bot
        }
    };
    super::super::apps_access::set_apps_read_only(db, &bot.bot_id, true)?;
    // The bot's own project decides its workspace from now on.
    let workspace = watch_workspace(state, db, repository, Some(&bot));
    let by = asker.label();
    let brief = save_watch_routine(
        db,
        &bot,
        record.brief_routine_id.as_deref(),
        BRIEF_ROUTINE,
        BRIEF_PROMPT,
        &with_zone(json!({ "kind": "daily", "time": time }), params),
        None,
        "new",
        by,
    )?;
    let watch = save_watch_routine(
        db,
        &bot,
        record.watch_routine_id.as_deref(),
        WATCH_ROUTINE,
        WATCH_PROMPT,
        &with_zone(json!({ "kind": "cron", "expression": WATCH_CRON }), params),
        Some(&watch_check(&workspace, mail, calendar)),
        "standing",
        by,
    )?;
    record.brief_routine_id = brief["id"].as_str().map(str::to_string);
    record.watch_routine_id = watch["id"].as_str().map(str::to_string);
    store::save(db, &record)?;
    super::super::presentation::refresh_presented_bots(db);
    let mut missing = Vec::new();
    if !mail {
        missing.push("Gmail");
    }
    if !calendar {
        missing.push("Google Calendar");
    }
    answer["bot"] = bot_summary(&bot);
    answer["routines"] = json!([brief, watch]);
    answer["missing"] = json!(missing);
    answer["state"] = json!("on");
    Ok(answer)
}

/// Adds the routine, or puts an existing one back to the day watch's shape and turns it on.
#[allow(clippy::too_many_arguments)]
fn save_watch_routine(
    db: &Connection,
    bot: &Bot,
    id: Option<&str>,
    name: &str,
    prompt: &str,
    schedule: &Value,
    precheck: Option<&Value>,
    continuity: &str,
    by: &str,
) -> Result<Value, DomainStateError> {
    let current = match id {
        Some(id) => routine_view(db, id)?.filter(|view| view["botId"] == json!(bot.bot_id)),
        None => None,
    };
    let before = match &current {
        Some(_) => crate::automations::routine_definition(db, id.unwrap_or_default())?,
        None => None,
    };
    let routine = save_routine(
        db,
        RoutineSave {
            id: current.as_ref().and(id),
            project_id: &bot.project_id,
            agent_id: &bot.agent_id,
            bot_id: &bot.bot_id,
            name,
            prompt,
            schedule,
            precheck,
            deliver: "brief",
            continuity,
            enabled: true,
            created_by: by,
        },
    )?;
    let routine_id = routine["id"].as_str().unwrap_or_default().to_string();
    let after = crate::automations::routine_definition(db, &routine_id)?;
    if before != after {
        let kind = if before.is_some() {
            "routineEdited"
        } else {
            "routineAdded"
        };
        super::super::routines::record_change(
            db,
            kind,
            &routine_id,
            before.as_ref(),
            after.as_ref(),
            by,
        )?;
    }
    Ok(routine_view(db, &routine_id)?.unwrap_or(routine))
}

fn turn_off(db: &Connection) -> Result<Value, DomainStateError> {
    let record = store::read(db)?;
    let mut paused = Vec::new();
    for id in [&record.brief_routine_id, &record.watch_routine_id]
        .into_iter()
        .flatten()
    {
        if routine_view(db, id)?.is_some() {
            set_routine_enabled_by_id(db, id, false)?;
            paused.push(id.clone());
        }
    }
    let bot = watch_bot(db, &record)?;
    Ok(json!({
        "state": if bot.is_some() { "paused" } else { "off" },
        "bot": bot.as_ref().map(bot_summary),
        "pausedRoutines": paused,
        "routines": routines(db, &record)?,
    }))
}
