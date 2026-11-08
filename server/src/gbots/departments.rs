//! Departments: GBots grouped under a name, with shared instructions every member reads and at
//! most one lead. The `gbots_departments` row is the truth for the lead (`leadBotId`); each bot's
//! `departmentId` says where it sits, and its `isLead` mirrors the row (kept in step by
//! [`sync_leads`] after every change), so the direction rules (assign.rs) and the sidebar read one
//! flag. `/api/gbots` ops: `departments`, `department`, `departmentCreate`, `departmentEdit`,
//! `departmentRemove`; a bot moves with `edit` (`department`, `lead`).
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q-D A, plan section 1.5): only you and Chief of Staff create, rename and remove departments, set their lead and shared instructions, and move GBots between them; a department's lead edits only its own members' instructions (endpoint.rs `edit`). A department only groups GBots inside GBots (Main Hub's sub-headings, the briefs); it is never a project, a Space or a folder.
//! CDXC:GBots 2026-11-09 WHY:
//! Removing a department keeps its GBots: they stay as GBots of no department (and its lead stops leading), so nothing of a bot's persona, chats or memory goes with a department.

use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::record::{find_bot, list_bots, new_id, Bot};
use crate::coordinators::now_iso;
use crate::domain::DomainStateError;

pub(crate) const NAME_MAX_CHARS: usize = 60;
/// Shared instructions every member reads (plan 1.5).
pub(crate) const INSTRUCTIONS_MAX_CHARS: usize = 4_000;
/// Departments one computer may have.
pub(crate) const MAX_DEPARTMENTS: usize = 30;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Department {
    pub department_id: String,
    pub name: String,
    pub icon: String,
    pub lead_bot_id: Option<String>,
    pub instructions: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

const COLUMNS: &str =
    "departmentId, name, icon, leadBotId, instructions, sortOrder, createdAt, updatedAt";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Department> {
    Ok(Department {
        department_id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        lead_bot_id: row.get(3)?,
        instructions: row.get(4)?,
        sort_order: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

fn forbidden(message: &str) -> DomainStateError {
    DomainStateError {
        code: "forbidden",
        message: message.to_string(),
    }
}

/// Every department, in the order Main Hub draws them (their order, then by name).
pub(crate) fn list_departments(db: &Connection) -> Result<Vec<Department>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {COLUMNS} FROM gbots_departments ORDER BY sortOrder, name COLLATE NOCASE"
        ))
        .map_err(sql)?;
    let rows = statement.query_map([], from_row).map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

pub(crate) fn read_department(
    db: &Connection,
    department_id: &str,
) -> Result<Option<Department>, DomainStateError> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM gbots_departments WHERE departmentId = ?1"),
        [department_id],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// The department `reference` names: its id, or its name (case-blind).
pub(crate) fn find_department(
    db: &Connection,
    reference: &str,
) -> Result<Option<Department>, DomainStateError> {
    let reference = reference.trim();
    if let Some(department) = read_department(db, reference)? {
        return Ok(Some(department));
    }
    let wanted = reference.to_lowercase();
    Ok(list_departments(db)?
        .into_iter()
        .find(|department| department.name.to_lowercase() == wanted))
}

pub(crate) fn require_department(
    db: &Connection,
    reference: &str,
) -> Result<Department, DomainStateError> {
    find_department(db, reference)?.ok_or_else(|| {
        DomainStateError::not_found(format!(
            "There is no department called {reference}. Run `ghostex gbots department list` to see them."
        ))
    })
}

fn insert_department(db: &Connection, department: &Department) -> Result<(), DomainStateError> {
    db.execute(
        &format!(
            "INSERT INTO gbots_departments ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        ),
        rusqlite::params![
            department.department_id,
            department.name,
            department.icon,
            department.lead_bot_id,
            department.instructions,
            department.sort_order,
            department.created_at,
            department.updated_at,
        ],
    )
    .map_err(sql)?;
    Ok(())
}

fn update_department(db: &Connection, department: &Department) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_departments SET name = ?2, icon = ?3, leadBotId = ?4, instructions = ?5, \
         sortOrder = ?6, updatedAt = ?7 WHERE departmentId = ?1",
        rusqlite::params![
            department.department_id,
            department.name,
            department.icon,
            department.lead_bot_id,
            department.instructions,
            department.sort_order,
            department.updated_at,
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// Puts `bot_id` in `department_id` (`None`: no department). `lead`: `Some(true)` makes it the
/// department's lead, `Some(false)` stops it leading, `None` keeps whether it leads (a bot that
/// moves to another department stops leading the one it left).
pub(crate) fn place_bot(
    db: &Connection,
    bot_id: &str,
    department_id: Option<&str>,
    lead: Option<bool>,
) -> Result<(), DomainStateError> {
    let now = now_iso();
    db.execute(
        "UPDATE gbots_bots SET departmentId = ?2, updatedAt = ?3 WHERE botId = ?1",
        rusqlite::params![bot_id, department_id, now],
    )
    .map_err(sql)?;
    match (department_id, lead) {
        (Some(department_id), Some(true)) => {
            db.execute(
                "UPDATE gbots_departments SET leadBotId = ?2, updatedAt = ?3 WHERE departmentId = ?1",
                rusqlite::params![department_id, bot_id, now],
            )
            .map_err(sql)?;
        }
        (_, Some(false)) => {
            db.execute(
                "UPDATE gbots_departments SET leadBotId = NULL, updatedAt = ?2 WHERE leadBotId = ?1",
                rusqlite::params![bot_id, now],
            )
            .map_err(sql)?;
        }
        _ => {}
    }
    sync_leads(db)
}

/// Makes every bot's `isLead` match its department's `leadBotId`, and drops a lead that is no
/// longer in its department (moved out or removed).
pub(crate) fn sync_leads(db: &Connection) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_departments SET leadBotId = NULL WHERE leadBotId IS NOT NULL AND NOT EXISTS \
         (SELECT 1 FROM gbots_bots b WHERE b.botId = gbots_departments.leadBotId \
          AND b.departmentId = gbots_departments.departmentId)",
        [],
    )
    .map_err(sql)?;
    db.execute(
        "UPDATE gbots_bots SET departmentId = NULL WHERE departmentId IS NOT NULL AND NOT EXISTS \
         (SELECT 1 FROM gbots_departments d WHERE d.departmentId = gbots_bots.departmentId)",
        [],
    )
    .map_err(sql)?;
    db.execute(
        "UPDATE gbots_bots SET isLead = CASE WHEN EXISTS \
         (SELECT 1 FROM gbots_departments d WHERE d.leadBotId = gbots_bots.botId \
          AND d.departmentId = gbots_bots.departmentId) THEN 1 ELSE 0 END",
        [],
    )
    .map_err(sql)?;
    Ok(())
}

fn check_name(
    name: &str,
    others: &[Department],
    own_id: Option<&str>,
) -> Result<String, DomainStateError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(DomainStateError::bad_request("Give the department a name."));
    }
    if name.contains(['\n', '\r']) || name.chars().count() > NAME_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "A department's name is one line of up to {NAME_MAX_CHARS} characters."
        )));
    }
    let lower = name.to_lowercase();
    if others.iter().any(|other| {
        Some(other.department_id.as_str()) != own_id && other.name.to_lowercase() == lower
    }) {
        return Err(DomainStateError {
            code: "conflict",
            message: format!("There is already a department called {name}. Pick another name."),
        });
    }
    if lower == "none" {
        return Err(DomainStateError::bad_request(
            "\"None\" means no department; pick another name.",
        ));
    }
    Ok(name)
}

fn check_instructions(instructions: &str) -> Result<String, DomainStateError> {
    let instructions = instructions.trim().to_string();
    if instructions.chars().count() > INSTRUCTIONS_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "A department's instructions are limited to {INSTRUCTIONS_MAX_CHARS} characters."
        )));
    }
    Ok(instructions)
}

fn check_icon(icon: &str) -> Result<String, DomainStateError> {
    let icon = icon.trim().to_string();
    let plain = icon.len() <= 40
        && icon
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
    if !plain {
        return Err(DomainStateError::bad_request(
            "An icon is the name of a Ghostex icon, like robot, search or pencil.",
        ));
    }
    Ok(icon)
}

/// What a department change's `department` / `lead` params ask for a bot: the department it goes
/// to (`Some(None)` for none) and whether it leads.
pub(crate) struct Placement {
    pub department: Option<Option<String>>,
    pub lead: Option<bool>,
}

impl Placement {
    pub fn from_params(
        db: &Connection,
        params: &Map<String, Value>,
    ) -> Result<Self, DomainStateError> {
        let department = match params.get("department") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let reference = value.as_str().unwrap_or_default().trim();
                if reference.is_empty() || reference.eq_ignore_ascii_case("none") {
                    Some(None)
                } else {
                    Some(Some(require_department(db, reference)?.department_id))
                }
            }
        };
        let lead = params.get("lead").and_then(Value::as_bool);
        Ok(Self { department, lead })
    }

    pub fn is_empty(&self) -> bool {
        self.department.is_none() && self.lead.is_none()
    }

    /// Applies it to `bot` (already saved), answering whether anything moved.
    pub fn apply(&self, db: &Connection, bot: &Bot) -> Result<bool, DomainStateError> {
        if self.is_empty() {
            return Ok(false);
        }
        let target = match &self.department {
            Some(target) => target.clone(),
            None => bot.department_id.clone(),
        };
        if self.lead == Some(true) && target.is_none() {
            return Err(DomainStateError::bad_request(format!(
                "{} is in no department, so it can't lead one. Put it in one with --department.",
                bot.name
            )));
        }
        let moved = target != bot.department_id;
        let lead = match self.lead {
            Some(lead) => Some(lead),
            None if moved => Some(false),
            None => None,
        };
        if !moved && lead.is_none_or(|lead| lead == bot.is_lead) {
            return Ok(false);
        }
        place_bot(db, &bot.bot_id, target.as_deref(), lead)?;
        Ok(true)
    }
}

/// A department as `/api/gbots` answers it, with its lead and members (by name, the lead first).
pub(crate) fn department_summary(department: &Department, bots: &[Bot]) -> Value {
    let mut members: Vec<&Bot> = bots
        .iter()
        .filter(|bot| bot.department_id.as_deref() == Some(department.department_id.as_str()))
        .collect();
    members.sort_by_key(|bot| (!bot.is_lead, bot.name.to_lowercase()));
    let lead = department
        .lead_bot_id
        .as_deref()
        .and_then(|lead| bots.iter().find(|bot| bot.bot_id == lead));
    json!({
        "departmentId": department.department_id,
        "name": department.name,
        "icon": department.icon,
        "instructions": department.instructions,
        "sortOrder": department.sort_order,
        "lead": lead.map(|bot| json!({ "botId": bot.bot_id, "name": bot.name })),
        "members": members.iter().map(|bot| json!({
            "botId": bot.bot_id,
            "name": bot.name,
            "title": bot.title,
            "isLead": bot.is_lead,
        })).collect::<Vec<_>>(),
        "createdAt": department.created_at,
        "updatedAt": department.updated_at,
    })
}

/// `departments`: every department, and the GBots in none.
pub(crate) fn list(db: &Connection) -> Result<Value, DomainStateError> {
    let bots = list_bots(db)?;
    let departments = list_departments(db)?;
    let unassigned: Vec<Value> = bots
        .iter()
        .filter(|bot| bot.department_id.is_none())
        .map(|bot| json!({ "botId": bot.bot_id, "name": bot.name, "title": bot.title }))
        .collect();
    Ok(json!({
        "departments": departments
            .iter()
            .map(|department| department_summary(department, &bots))
            .collect::<Vec<_>>(),
        "unassigned": unassigned,
    }))
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

fn lead_bot(db: &Connection, reference: &str) -> Result<Bot, DomainStateError> {
    find_bot(db, reference)?.ok_or_else(|| {
        DomainStateError::not_found(format!(
            "There is no GBot called {reference} to lead the department. Run `ghostex gbots list` to see them."
        ))
    })
}

/// `departmentCreate`: `name`, `icon?`, `instructions?`, `lead?` (a bot, which joins it to lead
/// it). Answers the department and the change id.
pub(crate) fn create(
    db: &Connection,
    by: &str,
    params: &Map<String, Value>,
) -> Result<(Department, String), DomainStateError> {
    let others = list_departments(db)?;
    if others.len() >= MAX_DEPARTMENTS {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "This computer has {MAX_DEPARTMENTS} departments, the most it can have. Remove one first."
            ),
        });
    }
    let name = check_name(text(params, "name").unwrap_or_default(), &others, None)?;
    let instructions = check_instructions(text(params, "instructions").unwrap_or_default())?;
    let icon = check_icon(text(params, "icon").unwrap_or_default())?;
    let lead = text(params, "lead")
        .map(str::trim)
        .filter(|lead| !lead.is_empty())
        .map(|lead| lead_bot(db, lead))
        .transpose()?;
    let now = now_iso();
    let department = Department {
        department_id: new_id("d_"),
        name,
        icon,
        lead_bot_id: None,
        instructions,
        sort_order: others
            .iter()
            .map(|other| other.sort_order)
            .max()
            .unwrap_or(0)
            + 1,
        created_at: now.clone(),
        updated_at: now,
    };
    insert_department(db, &department)?;
    if let Some(lead) = lead {
        place_bot(
            db,
            &lead.bot_id,
            Some(&department.department_id),
            Some(true),
        )?;
    }
    let department = read_department(db, &department.department_id)?.unwrap_or(department);
    let change_id = record_change(db, "departmentAdded", None, Some(&department), by)?;
    Ok((department, change_id))
}

/// `departmentEdit`: `department` and any of `name`, `icon`, `instructions`, `lead` (a bot, or
/// "" for none), `sortOrder`. Answers the department before and after.
pub(crate) fn edit(
    db: &Connection,
    by: &str,
    params: &Map<String, Value>,
) -> Result<(Department, Department, Option<String>), DomainStateError> {
    let reference = text(params, "department")
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("Name the department."))?;
    let before = require_department(db, reference)?;
    let others = list_departments(db)?;
    let mut after = before.clone();
    if let Some(name) = text(params, "name") {
        after.name = check_name(name, &others, Some(&before.department_id))?;
    }
    if let Some(instructions) = text(params, "instructions") {
        after.instructions = check_instructions(instructions)?;
    }
    if let Some(icon) = text(params, "icon") {
        after.icon = check_icon(icon)?;
    }
    if let Some(order) = params.get("sortOrder").and_then(Value::as_i64) {
        after.sort_order = order;
    }
    let lead_change = match text(params, "lead").map(str::trim) {
        None => None,
        Some(lead) if lead.is_empty() || lead.eq_ignore_ascii_case("none") => Some(None),
        Some(lead) => Some(Some(lead_bot(db, lead)?)),
    };
    let fields_changed = after != before;
    if fields_changed {
        after.updated_at = now_iso();
        update_department(db, &after)?;
    }
    let mut lead_moved = false;
    match lead_change {
        Some(None) if before.lead_bot_id.is_some() => {
            db.execute(
                "UPDATE gbots_departments SET leadBotId = NULL, updatedAt = ?2 WHERE departmentId = ?1",
                rusqlite::params![before.department_id, now_iso()],
            )
            .map_err(sql)?;
            sync_leads(db)?;
            lead_moved = true;
        }
        Some(Some(bot)) if before.lead_bot_id.as_deref() != Some(bot.bot_id.as_str()) => {
            place_bot(db, &bot.bot_id, Some(&before.department_id), Some(true))?;
            lead_moved = true;
        }
        _ => {}
    }
    let after = read_department(db, &before.department_id)?.unwrap_or(after);
    if !fields_changed && !lead_moved {
        return Ok((before, after, None));
    }
    let change_id = record_change(db, "departmentEdited", Some(&before), Some(&after), by)?;
    Ok((before, after, Some(change_id)))
}

/// `departmentRemove`: its GBots stay, in no department. Answers it and its members' ids.
pub(crate) fn remove(
    db: &Connection,
    by: &str,
    reference: &str,
) -> Result<(Department, Vec<String>, String), DomainStateError> {
    let department = require_department(db, reference)?;
    let members: Vec<String> = list_bots(db)?
        .into_iter()
        .filter(|bot| bot.department_id.as_deref() == Some(department.department_id.as_str()))
        .map(|bot| bot.bot_id)
        .collect();
    db.execute(
        "DELETE FROM gbots_departments WHERE departmentId = ?1",
        [&department.department_id],
    )
    .map_err(sql)?;
    sync_leads(db)?;
    let change_id = record_change(db, "departmentRemoved", Some(&department), None, by)?;
    Ok((department, members, change_id))
}

fn record_change(
    db: &Connection,
    kind: &str,
    before: Option<&Department>,
    after: Option<&Department>,
    by: &str,
) -> Result<String, DomainStateError> {
    let change_id = new_id("c_");
    let subject = after
        .or(before)
        .map(|department| department.department_id.clone());
    let text = |department: Option<&Department>| {
        department.and_then(|department| serde_json::to_string(department).ok())
    };
    db.execute(
        "INSERT INTO gbots_changes (changeId, kind, subjectId, beforeJson, afterJson, byWho, at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            change_id,
            kind,
            subject.unwrap_or_default(),
            text(before),
            text(after),
            by,
            now_iso()
        ],
    )
    .map_err(sql)?;
    Ok(change_id)
}

/// Whether `kind` is a department's change (changes.rs reads those apart from bots').
pub(crate) fn is_department_change(kind: &str) -> bool {
    kind.starts_with("department")
}

/// Who may change departments, as `/api/gbots` checks it.
pub(crate) fn require_manager(may_manage: bool) -> Result<(), DomainStateError> {
    if may_manage {
        Ok(())
    } else {
        Err(forbidden(
            "Only you and Chief of Staff set up departments and move GBots between them. Ask Chief of Staff to.",
        ))
    }
}
