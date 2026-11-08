//! The `/api/gbots` department ops (`departments`, `department`, `departmentCreate`,
//! `departmentEdit`, `departmentRemove`), and the rule that lets a department's lead change its
//! members' instructions. The table and its rules are departments.rs; this file adds what every
//! change must do after it: redraw the bots' rows, rewrite every bot's generated instruction files
//! (each brief lists the team by department) and tell the open chats of the members whose
//! department instructions changed.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::departments::{self, department_summary, require_department, require_manager};
use super::record::{list_bots, require_bot, Bot};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

/// Whether `op` is one of these.
pub(super) fn handles(op: &str) -> bool {
    matches!(
        op,
        "departments" | "department" | "departmentCreate" | "departmentEdit" | "departmentRemove"
    )
}

pub(super) fn run(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    may_manage: bool,
    by: &str,
    op: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let reference = || {
        params
            .get("department")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|reference| !reference.is_empty())
            .ok_or_else(|| DomainStateError::bad_request("Name the department."))
    };
    match op {
        "departments" => departments::list(db),
        "department" => {
            let department = require_department(db, reference()?)?;
            Ok(json!({ "department": department_summary(&department, &list_bots(db)?) }))
        }
        "departmentCreate" => {
            require_manager(may_manage)?;
            let (department, change_id) = departments::create(db, by, params)?;
            after_change(state, db, repository, &department.department_id, true);
            Ok(json!({
                "department": department_summary(&department, &list_bots(db)?),
                "changeId": change_id,
            }))
        }
        "departmentEdit" => {
            require_manager(may_manage)?;
            let (before, after, change_id) = departments::edit(db, by, params)?;
            if change_id.is_some() {
                let told = before.instructions != after.instructions;
                after_change(state, db, repository, &after.department_id, told);
            }
            Ok(json!({
                "department": department_summary(&after, &list_bots(db)?),
                "changed": change_id.is_some(),
                "changeId": change_id,
            }))
        }
        "departmentRemove" => {
            require_manager(may_manage)?;
            let (department, members, change_id) = departments::remove(db, by, reference()?)?;
            redraw(state, db, repository, &members);
            let bots = list_bots(db)?;
            let left: Vec<Bot> = bots
                .into_iter()
                .filter(|bot| members.contains(&bot.bot_id))
                .collect();
            if !department.instructions.is_empty() {
                super::memory::tell_every_bot(state, repository, &left);
            }
            Ok(json!({
                "removed": { "departmentId": department.department_id, "name": department.name },
                "members": left.iter().map(|bot| json!({ "botId": bot.bot_id, "name": bot.name })).collect::<Vec<_>>(),
                "changeId": change_id,
            }))
        }
        other => Err(DomainStateError::bad_request(format!(
            "Unknown department op {other}."
        ))),
    }
}

/// After `department_id` changed: redraw its members' rows and rewrite every brief; `tell` its
/// members' open chats to read their brief again (its shared instructions changed).
fn after_change(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    department_id: &str,
    tell: bool,
) {
    let members: Vec<Bot> = list_bots(db)
        .unwrap_or_default()
        .into_iter()
        .filter(|bot| bot.department_id.as_deref() == Some(department_id))
        .collect();
    redraw(
        state,
        db,
        repository,
        &members
            .iter()
            .map(|bot| bot.bot_id.clone())
            .collect::<Vec<_>>(),
    );
    if tell {
        super::memory::tell_every_bot(state, repository, &members);
    }
}

/// Every bot's row shows its department (presentation.rs), so after a department change every
/// bot's look is read again; the projects of `bot_ids` are redrawn on the clients now.
pub(super) fn redraw(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot_ids: &[String],
) {
    super::presentation::refresh_presented_bots(db);
    super::brief::refresh_all_instruction_files(&state.paths, db);
    for bot in list_bots(db).unwrap_or_default() {
        if bot.project_id.is_empty() || !bot_ids.contains(&bot.bot_id) {
            continue;
        }
        let _ = crate::server::schedule_presentation_project_delta(
            state,
            db,
            repository,
            &bot.project_id,
            "projectUpdated",
        );
    }
}

/// A GBot's chat changing another GBot: allowed only for a department's lead, only for its own
/// members, and only their instructions (`soul`).
pub(super) fn check_lead_edit(
    db: &Connection,
    own: Option<&Bot>,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let refused = |message: String| DomainStateError {
        code: "forbidden",
        message,
    };
    let Some(own) = own else {
        return Err(refused(
            "Only you and Chief of Staff can change a GBot. Ask Chief of Staff to.".to_string(),
        ));
    };
    let target = require_bot(
        db,
        params
            .get("bot")
            .and_then(Value::as_str)
            .ok_or_else(|| DomainStateError::bad_request("Pass bot."))?,
    )?;
    if target.bot_id == own.bot_id {
        return Err(refused(
            "Your own instructions are set by the user and Chief of Staff. Say in your report what should change.".to_string(),
        ));
    }
    let department = own
        .department_id
        .as_deref()
        .filter(|_| own.is_lead)
        .map(|id| departments::read_department(db, id))
        .transpose()?
        .flatten();
    let Some(department) = department else {
        return Err(refused(format!(
            "Only you, Chief of Staff and the lead of {}'s department change it. Say in your report what should change.",
            target.name
        )));
    };
    if target.department_id.as_deref() != Some(department.department_id.as_str()) {
        return Err(refused(format!(
            "You lead {}, and {} isn't in it, so you can't change its instructions. Ask Chief of Staff to.",
            department.name, target.name
        )));
    }
    let other_fields: Vec<&str> = params
        .keys()
        .map(String::as_str)
        .filter(|key| !matches!(*key, "op" | "bot" | "soul"))
        .collect();
    if !other_fields.is_empty() || !params.contains_key("soul") {
        return Err(refused(format!(
            "As {}'s lead you change only your members' instructions (--soul or --soul-file). Ask Chief of Staff for the rest.",
            department.name
        )));
    }
    Ok(())
}
