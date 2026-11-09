//! A GBot routine's runs: whether one may start now, the chat it runs in, and where its result
//! goes. The scheduler, pacing and pre-checks are Automations' (server/src/automations/routine_tick.rs).
//!
//! CDXC:GBots 2026-11-09 WHY:
//! A routine run is a chat of its GBot (`openedBy: routine:<id>`, titled "<routine> · <date>"), so it shows under the bot's row like any of its chats and carries the bot's instructions and memory; with `standing` continuity every run goes into the one chat the routine keeps. The run's own chat is where its result lands; a routine set to deliver to Chief of Staff also tells Chief's chat, but only when the run found something, needs you or failed, so quiet runs keep Chief of Staff quiet.

use std::path::PathBuf;

use rusqlite::Connection;
use serde_json::{json, Value};

use super::record::{read_bot, require_bot};
use super::session::{create_bot_chat_with, ChatOpening};
use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::server::{queue_session_chat_prompt, AppState};

/// Whether a due routine of `bot_id` may run now.
pub(crate) enum RoutineGate {
    Run,
    /// Not now, and why (said in the run history).
    Hold(String),
    /// The GBot is gone.
    Gone,
}

pub(crate) fn routine_gate(paths: &GxserverPaths, db: &Connection, bot_id: &str) -> RoutineGate {
    let bot = match read_bot(db, bot_id) {
        Ok(Some(bot)) => bot,
        Ok(None) => return RoutineGate::Gone,
        Err(_) => return RoutineGate::Hold("Ghostex could not read its GBot".to_string()),
    };
    if !super::gbots_on(paths) {
        return RoutineGate::Hold("GBots are turned off".to_string());
    }
    if bot.paused_at.is_some() {
        return RoutineGate::Hold(format!("{} is paused", bot.name));
    }
    RoutineGate::Run
}

/// Where a routine's command check runs: the GBot's folder.
pub(crate) fn routine_folder(paths: &GxserverPaths, bot_id: &str) -> PathBuf {
    super::folder::bot_folder(paths, bot_id)
}

fn text(session: &Value, key: &str) -> String {
    session
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Opens the run's chat (or, for a standing routine, sends the run into its chat) and answers
/// the chat's project and session ids.
pub(crate) fn start_routine_run(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot_id: &str,
    automation_id: &str,
    name: &str,
    prompt: &str,
) -> Result<(String, String), DomainStateError> {
    let bot = require_bot(db, bot_id)?;
    let task = format!("Your routine \"{name}\" is due. {prompt}");
    let (continuity, standing) = crate::automations::routine_continuity(db, automation_id);
    if continuity == "standing" {
        if let Some((project_id, session_id)) =
            standing.as_deref().and_then(|key| key.split_once(':'))
        {
            let session = repository.get_session(project_id, session_id)?;
            if let Some(session) =
                session.filter(crate::presentation::session_attributes::is_active)
            {
                queue_session_chat_prompt(state, project_id, session_id, &task, false)?;
                super::assign::wake_if_asleep(state, &session);
                return Ok((project_id.to_string(), session_id.to_string()));
            }
        }
    }
    let date = chrono::Local::now().format("%a %-d %b").to_string();
    let title = if continuity == "standing" {
        name.to_string()
    } else {
        format!("{name} · {date}")
    };
    let opened_by = format!("routine:{automation_id}");
    let (session, _started) = create_bot_chat_with(
        state,
        db,
        repository,
        &bot,
        &ChatOpening {
            opened_by: &opened_by,
            task: Some(&task),
            purpose: Some("routine"),
            standing: continuity == "standing",
            title: Some(&title),
            ..ChatOpening::default()
        },
    )?;
    let (project_id, session_id) = (text(&session, "projectId"), text(&session, "sessionId"));
    if continuity == "standing" {
        crate::automations::set_routine_standing_chat(
            db,
            automation_id,
            &format!("{project_id}:{session_id}"),
        );
    }
    Ok((project_id, session_id))
}

/// A finished routine run, as the scheduler saw it.
pub(crate) struct SettledRoutineRun<'a> {
    pub bot_id: &'a str,
    pub automation_id: &'a str,
    pub name: &'a str,
    pub deliver: &'a str,
    pub status: &'a str,
    pub summary: &'a str,
    pub session: Option<&'a str>,
}

/// Tells Chief of Staff about a run that found something or needs you, when the routine delivers
/// there. A run that failed is a GBot problem whatever the routine delivers to (problems/), so
/// Chief of Staff hears of it once, as a problem it can retry.
pub(crate) fn routine_run_settled(state: &AppState, db: &Connection, run: SettledRoutineRun<'_>) {
    let chat_key = run.session.and_then(|session_id| {
        let project_id: String = db
            .query_row(
                "SELECT projectId FROM sessions WHERE sessionId = ?1",
                [session_id],
                |row| row.get(0),
            )
            .ok()?;
        Some(format!("{project_id}:{session_id}"))
    });
    if matches!(
        run.status,
        "failed" | "findings" | "no_findings" | "needs_attention"
    ) {
        super::problems::routine_run_settled(
            db,
            run.bot_id,
            run.automation_id,
            run.status == "failed",
            run.summary,
            chat_key.as_deref(),
        );
    }
    if run.deliver == "brief" {
        super::day_watch::deliver(state, db, &run);
        return;
    }
    if run.deliver != "chief" {
        return;
    }
    let what = match run.status {
        "findings" => "found something",
        "needs_attention" => "needs you",
        _ => return,
    };
    let Some((chief_project, chief_session)) = crate::chief::chief_key(&state.paths) else {
        return;
    };
    let bot_name = read_bot(db, run.bot_id)
        .ok()
        .flatten()
        .map(|bot| bot.name)
        .unwrap_or_else(|| "A GBot".to_string());
    let chat = run.session.and_then(|session_id| {
        let project_id: String = db
            .query_row(
                "SELECT projectId FROM sessions WHERE sessionId = ?1",
                [session_id],
                |row| row.get(0),
            )
            .ok()?;
        Some(crate::ids::create_global_session_ref(
            &state.metadata.server_id,
            &project_id,
            session_id,
        ))
    });
    let summary: String = run.summary.trim().chars().take(4_000).collect();
    let mut message = format!(
        "Ghostex GBot routine: {bot_name}'s routine \"{}\" {what}.",
        run.name
    );
    if !summary.is_empty() {
        message.push_str("\n\n");
        message.push_str(&summary);
    }
    if let Some(chat) = chat {
        message.push_str(&format!(
            "\n\nIts chat: {chat} (`ghostex read-session-chat {chat}`)."
        ));
    }
    message.push_str(&format!(
        "\nRoutine {}: tell the user only what needs them.",
        run.automation_id
    ));
    let _ = queue_session_chat_prompt(state, &chief_project, &chief_session, &message, false);
}

/// The JSON a routine's chat card and list carry about its GBot.
pub(crate) fn routine_bot(db: &Connection, bot_id: &str) -> Value {
    match read_bot(db, bot_id) {
        Ok(Some(bot)) => json!({
            "botId": bot.bot_id,
            "name": bot.name,
            "icon": bot.icon,
            "colorHex": super::record::color_hex(&bot.color),
            "paused": bot.paused_at.is_some(),
        }),
        _ => Value::Null,
    }
}
