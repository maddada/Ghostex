//! A GBot's routines through `/api/gbots`: `routines`, `routineAdd`, `routineEdit`,
//! `routinePause`, `routineResume`, `routineRemove`, `routineRunNow`. A routine is an Automation in
//! the bot's project with an owner (server/src/automations/routines_api.rs); its runs are chats of
//! the bot (routine_runs.rs).
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (plan piece 7): a GBot is asked for a routine in plain words, turns them into a schedule and shows a "Routine set" card in its chat with Pause, Edit and Undo, on the desktop and the phone. Adding a routine is a change (`routineAdded`, `routineEdited` in `gbots_changes`), so Undo removes it, or puts the earlier version back, and is refused once it was changed since, as for GBots themselves.
//! CDXC:GBots 2026-11-09 WHY:
//! Who may set a GBot's routines: you, Chief of Staff and the GBot's own chats; no other agent. A command check runs a shell command on its own every few minutes, so one set by an agent (the GBot or Chief of Staff) for a GBot that asks first (approval Ask or Allow edits) starts paused, and only you turn it on, from its card, GBot Settings or your terminal.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::endpoint::Asker;
use super::record::{new_id, require_bot, Bot};
use crate::automations::{
    bot_routines, delete_routine, routine_definition, routine_view, run_routine_now, save_routine,
    set_routine_enabled_by_id, RoutineSave,
};
use crate::coordinators::now_iso;
use crate::domain::{DomainRepository, DomainStateError};
use crate::security::Principal;
use crate::server::AppState;

pub(crate) const NAME_MAX_CHARS: usize = 80;
pub(crate) const PROMPT_MAX_CHARS: usize = 8_000;
/// The most routines one GBot keeps.
pub(crate) const MAX_ROUTINES_PER_BOT: usize = 20;

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn forbidden(message: &str) -> DomainStateError {
    DomainStateError {
        code: "forbidden",
        message: message.to_string(),
    }
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

/// Who is setting a routine, as its rules see it.
struct Setter {
    asker: Asker,
    /// The GBot whose chat is asking, if one is.
    own_bot: Option<Bot>,
}

impl Setter {
    fn may_set(&self, bot: &Bot) -> bool {
        self.asker.may_manage()
            || self
                .own_bot
                .as_ref()
                .is_some_and(|own| own.bot_id == bot.bot_id)
    }

    fn label(&self) -> String {
        match (&self.own_bot, self.asker) {
            (_, asker) if asker.is_user() => "user".to_string(),
            (_, Asker::Chief) => "chief".to_string(),
            (Some(bot), _) => format!("bot:{}", bot.bot_id),
            _ => "agent".to_string(),
        }
    }

    fn require(&self, bot: &Bot) -> Result<(), DomainStateError> {
        if self.may_set(bot) {
            return Ok(());
        }
        Err(forbidden(&format!(
            "Only you, Chief of Staff and {}'s own chats set its routines.",
            bot.name
        )))
    }
}

/// A command check set by an agent for a GBot that asks first starts paused (module comment).
fn needs_your_go_ahead(setter: &Setter, bot: &Bot, precheck: Option<&Value>) -> bool {
    !setter.asker.is_user()
        && !matches!(bot.approval.as_str(), "auto" | "full")
        && precheck
            .and_then(|check| check.get("kind"))
            .and_then(Value::as_str)
            == Some("command")
}

/// A routine by id, or by name (case-blind) among `bot`'s routines, or all GBots' when unique.
fn resolve(db: &Connection, reference: &str, bot: Option<&Bot>) -> Result<Value, DomainStateError> {
    if let Some(view) = routine_view(db, reference)? {
        if view.get("botId").is_some_and(Value::is_string) {
            return Ok(view);
        }
    }
    let candidates: Vec<Value> = match bot {
        Some(bot) => bot_routines(db, &bot.bot_id)?,
        None => {
            let mut all = Vec::new();
            for bot in super::record::list_bots(db)? {
                all.extend(bot_routines(db, &bot.bot_id)?);
            }
            all
        }
    };
    let wanted = reference.trim().to_lowercase();
    let matches: Vec<Value> = candidates
        .into_iter()
        .filter(|routine| {
            routine
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.trim().to_lowercase() == wanted)
        })
        .collect();
    match matches.len() {
        1 => Ok(matches.into_iter().next().unwrap_or(Value::Null)),
        0 => Err(DomainStateError::not_found(format!(
            "There is no routine {reference}. `ghostex gbots routine list --bot <bot>` lists them."
        ))),
        _ => Err(DomainStateError::bad_request(format!(
            "Several routines are called {reference}: name the GBot with --bot, or use the routine's id."
        ))),
    }
}

fn record_change(
    db: &Connection,
    kind: &str,
    routine_id: &str,
    before: Option<&Value>,
    after: Option<&Value>,
    by: &str,
) -> Result<String, DomainStateError> {
    let change_id = new_id("c_");
    db.execute(
        "INSERT INTO gbots_changes (changeId, kind, subjectId, beforeJson, afterJson, byWho, at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            change_id,
            kind,
            routine_id,
            before.map(Value::to_string),
            after.map(Value::to_string),
            by,
            now_iso()
        ],
    )
    .map_err(sql)?;
    Ok(change_id)
}

/// The "Routine set" card a routine add carries into the chat that asked for it.
fn routine_card(change_id: &str, routine: &Value, bot: &Bot) -> Value {
    json!({
        "kind": ghostex_gx_protocol::chat_card::ROUTINE_SET,
        "changeId": change_id,
        "routineId": routine["id"],
        "botId": bot.bot_id,
        "botName": bot.name,
        "icon": bot.icon,
        "colorHex": super::record::color_hex(&bot.color),
        "name": routine["name"],
        "scheduleSentence": routine["scheduleSentence"],
        "precheckSentence": routine["precheckSentence"],
    })
}

/// The routine's bot, from a `bot` param or the asking chat's own bot.
fn target_bot(
    db: &Connection,
    setter: &Setter,
    params: &Map<String, Value>,
) -> Result<Bot, DomainStateError> {
    match text(params, "bot") {
        Some(reference) => require_bot(db, reference),
        None => setter
            .own_bot
            .clone()
            .ok_or_else(|| DomainStateError::bad_request("Name the GBot with --bot.")),
    }
}

fn limited(value: &str, max: usize, what: &str) -> Result<String, DomainStateError> {
    if value.chars().count() > max {
        return Err(DomainStateError::bad_request(format!(
            "A routine's {what} is at most {max} characters."
        )));
    }
    Ok(value.trim().to_string())
}

pub(super) fn run(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    caller: &Principal,
    asker: Asker,
    op: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let setter = Setter {
        asker,
        own_bot: super::endpoint::own_bot(db, caller)?,
    };
    match op {
        "routines" => {
            let bot = target_bot(db, &setter, params)?;
            Ok(json!({
                "bot": super::routine_runs::routine_bot(db, &bot.bot_id),
                "routines": bot_routines(db, &bot.bot_id)?,
            }))
        }
        "routineAdd" => add(db, &setter, params),
        "routineEdit" => edit(db, &setter, params),
        "routinePause" | "routineResume" => {
            let routine = resolve(db, required(params, "routine")?, None)?;
            let bot = routine_owner(db, &routine)?;
            setter.require(&bot)?;
            let resume = op == "routineResume";
            if resume
                && !setter.asker.is_user()
                && needs_your_go_ahead(&setter, &bot, routine.get("precheck"))
            {
                return Err(forbidden(
                    "This routine runs a command on its own, so only you turn it on: press Resume on its card or in the GBot's settings.",
                ));
            }
            set_routine_enabled_by_id(db, id_of(&routine), resume)?;
            answer(db, id_of(&routine), params)
        }
        "routineRemove" => {
            let routine = resolve(db, required(params, "routine")?, None)?;
            let bot = routine_owner(db, &routine)?;
            setter.require(&bot)?;
            let before = routine_definition(db, id_of(&routine))?;
            delete_routine(db, id_of(&routine))?;
            let change_id = record_change(
                db,
                "routineRemoved",
                id_of(&routine),
                before.as_ref(),
                None,
                &setter.label(),
            )?;
            Ok(json!({ "removed": routine, "changeId": change_id }))
        }
        "routineRunNow" => {
            let routine = resolve(db, required(params, "routine")?, None)?;
            let bot = routine_owner(db, &routine)?;
            setter.require(&bot)?;
            let run = run_routine_now(&state.automation_runtime, db, repository, id_of(&routine))?;
            let mut answer = answer(db, id_of(&routine), params)?;
            answer["run"] = run;
            Ok(answer)
        }
        other => Err(DomainStateError::bad_request(format!(
            "Unknown GBots op {other}: use routines, routineAdd, routineEdit, routinePause, routineResume, routineRemove or routineRunNow."
        ))),
    }
}

fn required<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    text(params, key).ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
}

fn id_of(routine: &Value) -> &str {
    routine
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn routine_owner(db: &Connection, routine: &Value) -> Result<Bot, DomainStateError> {
    require_bot(
        db,
        routine
            .get("botId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
}

/// A routine as it is now, and the change a card asked about (`change`), for the card to redraw.
fn answer(
    db: &Connection,
    id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let mut answer = json!({ "routine": routine_view(db, id)? });
    if let Some(change_id) = text(params, "change") {
        if let Some(change) = super::changes::read_change(db, change_id)? {
            answer["change"] = super::changes::change_summary(db, &change)?;
        }
    }
    Ok(answer)
}

fn add(
    db: &Connection,
    setter: &Setter,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let bot = target_bot(db, setter, params)?;
    setter.require(&bot)?;
    if bot.project_id.is_empty() {
        return Err(DomainStateError::not_found(format!(
            "{}'s project is gone. Remove the GBot and add it again.",
            bot.name
        )));
    }
    if bot_routines(db, &bot.bot_id)?.len() >= MAX_ROUTINES_PER_BOT {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "{} has {MAX_ROUTINES_PER_BOT} routines, the most a GBot keeps. Remove one first.",
                bot.name
            ),
        });
    }
    let name = limited(required(params, "name")?, NAME_MAX_CHARS, "name")?;
    let prompt = limited(required(params, "prompt")?, PROMPT_MAX_CHARS, "prompt")?;
    let schedule = params
        .get("schedule")
        .filter(|value| value.is_object())
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Pass the schedule as JSON, e.g. {\"kind\":\"daily\",\"time\":\"09:00\"}.",
            )
        })?;
    let precheck = params.get("precheck").filter(|value| !value.is_null());
    let held = needs_your_go_ahead(setter, &bot, precheck);
    let paused = params.get("paused").and_then(Value::as_bool) == Some(true) || held;
    let created_by = setter.label();
    let routine = save_routine(
        db,
        RoutineSave {
            id: None,
            project_id: &bot.project_id,
            agent_id: &bot.agent_id,
            bot_id: &bot.bot_id,
            name: &name,
            prompt: &prompt,
            schedule,
            precheck,
            deliver: text(params, "deliver").unwrap_or("chat"),
            continuity: text(params, "continuity").unwrap_or("new"),
            enabled: !paused,
            created_by: &created_by,
        },
    )?;
    let definition = routine_definition(db, id_of(&routine))?;
    let change_id = record_change(
        db,
        "routineAdded",
        id_of(&routine),
        None,
        definition.as_ref(),
        &created_by,
    )?;
    let mut answer = json!({
        "routine": routine,
        "changeId": change_id,
        "heldForYou": held,
    });
    // The chat that asked for it shows the card (a GBot's, or Chief of Staff's); your terminal
    // prints it too, where it is one line.
    answer["chatCard"] = routine_card(&change_id, &answer["routine"], &bot);
    Ok(answer)
}

fn edit(
    db: &Connection,
    setter: &Setter,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let current = resolve(db, required(params, "routine")?, None)?;
    let bot = routine_owner(db, &current)?;
    setter.require(&bot)?;
    let before = routine_definition(db, id_of(&current))?
        .ok_or_else(|| DomainStateError::not_found("There is no such routine."))?;
    let field = |key: &str| {
        before
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let name = match text(params, "name") {
        Some(name) => limited(name, NAME_MAX_CHARS, "name")?,
        None => field("name"),
    };
    let prompt = match text(params, "prompt") {
        Some(prompt) => limited(prompt, PROMPT_MAX_CHARS, "prompt")?,
        None => field("prompt"),
    };
    let schedule = params
        .get("schedule")
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| before["schedule"].clone());
    // `precheck: null` removes the check; leaving it out keeps it.
    let precheck = match params.get("precheck") {
        Some(value) => value.clone(),
        None => before["precheck"].clone(),
    };
    let precheck = Some(&precheck).filter(|value| !value.is_null());
    let enabled = current
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let check_changed = precheck != Some(&before["precheck"]).filter(|value| !value.is_null());
    let held = check_changed && needs_your_go_ahead(setter, &bot, precheck);
    let deliver = text(params, "deliver")
        .map(str::to_string)
        .unwrap_or_else(|| field("deliver"));
    let continuity = text(params, "continuity")
        .map(str::to_string)
        .unwrap_or_else(|| field("continuity"));
    let routine = save_routine(
        db,
        RoutineSave {
            id: Some(id_of(&current)),
            project_id: &bot.project_id,
            agent_id: &bot.agent_id,
            bot_id: &bot.bot_id,
            name: &name,
            prompt: &prompt,
            schedule: &schedule,
            precheck,
            deliver: &deliver,
            continuity: &continuity,
            enabled: enabled && !held,
            created_by: &setter.label(),
        },
    )?;
    let after = routine_definition(db, id_of(&routine))?;
    if after.as_ref() == Some(&before) {
        return Ok(json!({ "routine": routine, "changed": false }));
    }
    let change_id = record_change(
        db,
        "routineEdited",
        id_of(&routine),
        Some(&before),
        after.as_ref(),
        &setter.label(),
    )?;
    Ok(json!({ "routine": routine, "changed": true, "changeId": change_id, "heldForYou": held }))
}

/// Why a routine change can't be undone now; `None` when it can (changes.rs asks).
pub(super) fn undo_problem(
    db: &Connection,
    kind: &str,
    routine_id: &str,
    before: Option<&Value>,
    after: Option<&Value>,
) -> Result<Option<String>, DomainStateError> {
    let current = routine_definition(db, routine_id)?;
    let name = after
        .or(before)
        .and_then(|routine| routine.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("That routine")
        .to_string();
    Ok(match (kind, current) {
        ("routineAdded" | "routineEdited", None) => Some(format!("The routine {name} was removed since.")),
        ("routineAdded" | "routineEdited", Some(current)) if Some(&current) != after => Some(format!(
            "The routine {name} was changed since, so Undo would lose that change. Change or remove it in the GBot's settings instead."
        )),
        ("routineAdded" | "routineEdited", Some(_)) => None,
        _ => Some(format!(
            "A removed routine can't come back with Undo. Ask the GBot to set {name} again."
        )),
    })
}

/// Undoes a routine change: removes an added routine or puts back the version before an edit.
pub(super) fn undo(
    db: &Connection,
    kind: &str,
    routine_id: &str,
    before: Option<&Value>,
) -> Result<Value, DomainStateError> {
    match (kind, before) {
        ("routineAdded", _) => {
            delete_routine(db, routine_id)?;
            Ok(json!({ "removedRoutine": routine_id }))
        }
        ("routineEdited", Some(before)) => {
            let text = |key: &str| before.get(key).and_then(Value::as_str).unwrap_or_default();
            let enabled = routine_view(db, routine_id)?
                .and_then(|view| view.get("enabled").and_then(Value::as_bool))
                .unwrap_or(false);
            let precheck = Some(&before["precheck"]).filter(|value| !value.is_null());
            let routine = save_routine(
                db,
                RoutineSave {
                    id: Some(routine_id),
                    project_id: text("projectId"),
                    agent_id: text("agentId"),
                    bot_id: text("botId"),
                    name: text("name"),
                    prompt: text("prompt"),
                    schedule: &before["schedule"],
                    precheck,
                    deliver: text("deliver"),
                    continuity: text("continuity"),
                    enabled,
                    created_by: "user",
                },
            )?;
            Ok(json!({ "routine": routine }))
        }
        _ => Err(DomainStateError::bad_request(
            "Only adding or changing a routine can be undone.",
        )),
    }
}

/// The routine a change is about, as it is now (the card's live part).
pub(super) fn current(
    db: &Connection,
    routine_id: &str,
) -> Result<Option<Value>, DomainStateError> {
    routine_view(db, routine_id)
}
