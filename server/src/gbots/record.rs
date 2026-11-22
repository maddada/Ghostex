//! The `gbots_bots` table: one row per GBot, the truth for its persona (name, title, standing
//! instructions, look, agent, approval level). The bot folder's `SOUL.md` only mirrors `soul`
//! (folder.rs). Every add, edit and removal also writes a `gbots_changes` row.

use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::coordinators::{now_iso, COORDINATOR_INSTRUCTIONS_MAX_CHARS};
use crate::domain::DomainStateError;

pub(crate) const NAME_MAX_CHARS: usize = 60;
pub(crate) const TITLE_MAX_CHARS: usize = 120;
pub(crate) const DESCRIPTION_MAX_CHARS: usize = 1_000;
/// Standing instructions, the same limit as a coordinator's.
pub(crate) const SOUL_MAX_CHARS: usize = COORDINATOR_INSTRUCTIONS_MAX_CHARS;
/// GBots one computer may have.
pub(crate) const MAX_BOTS: usize = 100;
/// The icon a bot gets when none is picked (the robot of the GBots card).
pub(crate) const DEFAULT_ICON: &str = "robot";

/// A bot's colour: one of eight, by id, with the hex clients draw it in.
///
/// CDXC:GBots 2026-11-08 DECISION:
/// User (Q2102 A): a bot is marked by "an icon and a colour", picked from Ghostex's own icon set and eight colours. The first is the GBots robot's periwinkle (the default); the other seven are session-tag hues already tuned to read on the dark sidebar (packages/settings-catalog/src/data/session_tags.rs).
pub(crate) const COLORS: [(&str, &str); 8] = [
    ("periwinkle", "#9fb0ff"),
    ("sky", "#59d9ff"),
    ("mint", "#4ee6b8"),
    ("gold", "#f3cc5f"),
    ("coral", "#ff8b6b"),
    ("lavender", "#d2a7ff"),
    ("pink", "#ff9ee7"),
    ("rose", "#ff5f73"),
];

/// How much a bot's chats may do without asking: `ask` (the agent's own prompts, which Chief of
/// Staff may answer), `edits` (file edits allowed), `auto` (see session.rs), `full` (no prompts;
/// owner-only).
pub(crate) const APPROVALS: [&str; 4] = ["ask", "edits", "auto", "full"];

pub(crate) fn color_hex(color: &str) -> &'static str {
    COLORS
        .iter()
        .find(|(id, _)| *id == color)
        .map(|(_, hex)| *hex)
        .unwrap_or(COLORS[0].1)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Bot {
    pub bot_id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub soul: String,
    pub icon: String,
    pub color: String,
    pub agent_id: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub approval: String,
    pub department_id: Option<String>,
    pub is_lead: bool,
    pub computer: String,
    pub project_id: String,
    pub work_folder: Option<String>,
    pub memory_enabled: bool,
    pub paused_at: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

const COLUMNS: &str =
    "botId, name, title, description, soul, icon, color, agentId, model, effort, \
     approval, departmentId, isLead, computer, projectId, workFolder, memoryEnabled, pausedAt, \
     createdBy, createdAt, updatedAt";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Bot> {
    Ok(Bot {
        bot_id: row.get(0)?,
        name: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        soul: row.get(4)?,
        icon: row.get(5)?,
        color: row.get(6)?,
        agent_id: row.get(7)?,
        model: row.get(8)?,
        effort: row.get(9)?,
        approval: row.get(10)?,
        department_id: row.get(11)?,
        is_lead: row.get::<_, i64>(12)? != 0,
        computer: row.get(13)?,
        project_id: row.get(14)?,
        work_folder: row.get(15)?,
        memory_enabled: row.get::<_, i64>(16)? != 0,
        paused_at: row.get(17)?,
        created_by: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
    })
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

/// Every bot, by name.
pub(crate) fn list_bots(db: &Connection) -> Result<Vec<Bot>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {COLUMNS} FROM gbots_bots ORDER BY name COLLATE NOCASE"
        ))
        .map_err(sql)?;
    let rows = statement.query_map([], from_row).map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

pub(crate) fn read_bot(db: &Connection, bot_id: &str) -> Result<Option<Bot>, DomainStateError> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM gbots_bots WHERE botId = ?1"),
        [bot_id],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// The bot `reference` names: its id, or its name (case-blind).
pub(crate) fn find_bot(db: &Connection, reference: &str) -> Result<Option<Bot>, DomainStateError> {
    let reference = reference.trim();
    if let Some(bot) = read_bot(db, reference)? {
        return Ok(Some(bot));
    }
    let wanted = reference.to_lowercase();
    Ok(list_bots(db)?
        .into_iter()
        .find(|bot| bot.name.to_lowercase() == wanted))
}

/// [`find_bot`], or the sentence saying there is none.
pub(crate) fn require_bot(db: &Connection, reference: &str) -> Result<Bot, DomainStateError> {
    find_bot(db, reference)?.ok_or_else(|| {
        DomainStateError::not_found(format!(
            "There is no GBot called {reference}. Run `ghostex gbots list` to see them."
        ))
    })
}

/// The bot whose project is `project_id`.
pub(crate) fn bot_for_project(
    db: &Connection,
    project_id: &str,
) -> Result<Option<Bot>, DomainStateError> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM gbots_bots WHERE projectId = ?1"),
        [project_id],
        from_row,
    )
    .optional()
    .map_err(sql)
}

pub(crate) fn insert_bot(db: &Connection, bot: &Bot) -> Result<(), DomainStateError> {
    db.execute(
        &format!(
            "INSERT INTO gbots_bots ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, \
              ?19, ?20, ?21)"
        ),
        rusqlite::params![
            bot.bot_id,
            bot.name,
            bot.title,
            bot.description,
            bot.soul,
            bot.icon,
            bot.color,
            bot.agent_id,
            bot.model,
            bot.effort,
            bot.approval,
            bot.department_id,
            bot.is_lead as i64,
            bot.computer,
            bot.project_id,
            bot.work_folder,
            bot.memory_enabled as i64,
            bot.paused_at,
            bot.created_by,
            bot.created_at,
            bot.updated_at,
        ],
    )
    .map_err(sql)?;
    Ok(())
}

pub(crate) fn update_bot(db: &Connection, bot: &Bot) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_bots SET name = ?2, title = ?3, description = ?4, soul = ?5, icon = ?6, \
         color = ?7, agentId = ?8, model = ?9, effort = ?10, approval = ?11, projectId = ?12, \
         pausedAt = ?13, updatedAt = ?14 WHERE botId = ?1",
        rusqlite::params![
            bot.bot_id,
            bot.name,
            bot.title,
            bot.description,
            bot.soul,
            bot.icon,
            bot.color,
            bot.agent_id,
            bot.model,
            bot.effort,
            bot.approval,
            bot.project_id,
            bot.paused_at,
            bot.updated_at,
        ],
    )
    .map_err(sql)?;
    Ok(())
}

pub(crate) fn delete_bot(db: &Connection, bot_id: &str) -> Result<(), DomainStateError> {
    db.execute("DELETE FROM gbots_bots WHERE botId = ?1", [bot_id])
        .map_err(sql)?;
    Ok(())
}

/// Writes one `gbots_changes` row (`botAdded`, `botEdited`, `botRemoved`); answers its id.
pub(crate) fn record_change(
    db: &Connection,
    kind: &str,
    subject_id: &str,
    before: Option<&Bot>,
    after: Option<&Bot>,
    by: &str,
) -> Result<String, DomainStateError> {
    let change_id = new_id("c_");
    let text = |bot: Option<&Bot>| bot.and_then(|bot| serde_json::to_string(bot).ok());
    db.execute(
        "INSERT INTO gbots_changes (changeId, kind, subjectId, beforeJson, afterJson, byWho, at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            change_id,
            kind,
            subject_id,
            text(before),
            text(after),
            by,
            now_iso()
        ],
    )
    .map_err(sql)?;
    Ok(change_id)
}

/// `<prefix>` and ten lowercase letters or digits (`b_k3x9…` for a bot).
pub(crate) fn new_id(prefix: &str) -> String {
    use rand::RngExt;
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    let body: String = (0..10)
        .map(|_| CHARS[rng.random_range(0..CHARS.len())] as char)
        .collect();
    format!("{prefix}{body}")
}

/// Why `name` can't be a GBot's name; `None` when it can (uniqueness aside).
pub(crate) fn name_problem(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        Some("Give the GBot a name.".to_string())
    } else if name.contains(['\n', '\r']) {
        Some("A GBot's name is one line.".to_string())
    } else if name.chars().count() > NAME_MAX_CHARS {
        Some(format!(
            "A GBot's name is limited to {NAME_MAX_CHARS} characters."
        ))
    } else {
        None
    }
}

/// The fields of a create or edit that set a bot's persona; `None` leaves a field as it is.
#[derive(Default)]
pub(crate) struct BotFields {
    pub name: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub soul: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub agent_id: Option<String>,
    /// `Some(None)` clears it (the agent's own default).
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
    pub approval: Option<String>,
}

impl BotFields {
    pub fn from_params(params: &Map<String, Value>) -> Self {
        let text = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_string);
        let optional = |key: &str| {
            params.get(key).map(|value| {
                value
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string)
            })
        };
        Self {
            name: text("name"),
            title: text("title"),
            description: text("description"),
            soul: text("soul"),
            icon: text("icon"),
            color: text("color"),
            agent_id: text("agentId"),
            model: optional("model"),
            effort: optional("effort"),
            approval: text("approval"),
        }
    }

    /// Whether this edit asks for an approval level above `ask`.
    pub fn raises_approval(&self) -> bool {
        self.approval
            .as_deref()
            .is_some_and(|approval| approval.trim() != "ask")
    }

    /// Applies the fields to `bot`, checking every limit; the name's uniqueness is checked against
    /// `others` (every other bot).
    pub fn apply(self, bot: &mut Bot, others: &[Bot]) -> Result<(), DomainStateError> {
        if let Some(name) = self.name {
            let name = name.trim().to_string();
            if let Some(problem) = name_problem(&name) {
                return Err(DomainStateError::bad_request(problem));
            }
            let lower = name.to_lowercase();
            if others
                .iter()
                .any(|other| other.bot_id != bot.bot_id && other.name.to_lowercase() == lower)
            {
                return Err(DomainStateError {
                    code: "conflict",
                    message: format!("There is already a GBot called {name}. Pick another name."),
                });
            }
            bot.name = name;
        }
        if let Some(title) = self.title {
            let title = title.trim().to_string();
            if title.contains(['\n', '\r']) || title.chars().count() > TITLE_MAX_CHARS {
                return Err(DomainStateError::bad_request(format!(
                    "A GBot's title is one line of up to {TITLE_MAX_CHARS} characters."
                )));
            }
            bot.title = title;
        }
        if let Some(description) = self.description {
            let description = description.trim().to_string();
            if description.chars().count() > DESCRIPTION_MAX_CHARS {
                return Err(DomainStateError::bad_request(format!(
                    "A GBot's description is limited to {DESCRIPTION_MAX_CHARS} characters."
                )));
            }
            bot.description = description;
        }
        if let Some(soul) = self.soul {
            let soul = soul.trim().to_string();
            if soul.chars().count() > SOUL_MAX_CHARS {
                return Err(DomainStateError::bad_request(format!(
                    "A GBot's instructions are limited to {SOUL_MAX_CHARS} characters."
                )));
            }
            bot.soul = soul;
        }
        if let Some(icon) = self.icon {
            let icon = icon.trim().to_string();
            let plain = !icon.is_empty()
                && icon.len() <= 40
                && icon
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
            if !plain {
                return Err(DomainStateError::bad_request(
                    "An icon is the name of a Ghostex icon, like robot, search or pencil.",
                ));
            }
            bot.icon = icon;
        }
        if let Some(color) = self.color {
            let color = color.trim().to_lowercase();
            if !COLORS.iter().any(|(id, _)| *id == color) {
                return Err(DomainStateError::bad_request(format!(
                    "Pick one of these colours: {}.",
                    COLORS.map(|(id, _)| id).join(", ")
                )));
            }
            bot.color = color;
        }
        if let Some(agent_id) = self.agent_id {
            let agent_id = agent_id.trim().to_string();
            if agent_id.is_empty() {
                return Err(DomainStateError::bad_request(
                    "Name the agent the GBot runs on.",
                ));
            }
            bot.agent_id = agent_id;
        }
        if let Some(model) = self.model {
            bot.model = model;
        }
        if let Some(effort) = self.effort {
            bot.effort = effort;
        }
        if let Some(approval) = self.approval {
            let approval = approval.trim().to_lowercase();
            if !APPROVALS.contains(&approval.as_str()) {
                return Err(DomainStateError::bad_request(
                    "The approval level is ask, edits, auto or full.",
                ));
            }
            bot.approval = approval;
        }
        Ok(())
    }
}

/// A bot as `/api/gbots` answers it: the record without its full instructions, plus its look's
/// hex colour.
pub(crate) fn bot_summary(bot: &Bot) -> Value {
    json!({
        "botId": bot.bot_id,
        "name": bot.name,
        "title": bot.title,
        "description": bot.description,
        "icon": bot.icon,
        "color": bot.color,
        "colorHex": color_hex(&bot.color),
        "agentId": bot.agent_id,
        "model": bot.model,
        "effort": bot.effort,
        "approval": bot.approval,
        "departmentId": bot.department_id,
        "isLead": bot.is_lead,
        "computer": bot.computer,
        "projectId": bot.project_id,
        "paused": bot.paused_at.is_some(),
        "createdBy": bot.created_by,
        "createdAt": bot.created_at,
        "updatedAt": bot.updated_at,
    })
}
