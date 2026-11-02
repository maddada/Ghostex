//! The `workTicketSources` metadata row: per workspace, the command's path, the id it pinned and
//! its last accepted `info`, plus the ids retired from it. It is local to this computer and never
//! broadcast, unlike the workspaces document, which remote clients read and write.
//!
//! ```json
//! { "workspaces": { "ws-1": { "path": "~/bin/fixture-source", "resolvedPath": "/Users/me/bin/fixture-source",
//!     "sourceId": "fixture", "info": { … }, "checkedAt": "…", "lastError": null,
//!     "lastErrorDetail": null, "offeredId": null } },
//!   "retired": [ { "id": "old", "format": "number", "name": "Old", "icon": null, "retiredAt": "…" } ] }
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::*;

const WORK_SOURCES_METADATA_KEY: &str = "workTicketSources";

/// Ticket command sources this computer knows, the workspaces' active ones and the retired ones
/// together. Only pinning an id that is neither can reach it; retiring never does.
pub(crate) const MAX_SOURCE_IDS: usize = 256;
/// How many retired ids are kept once nothing refers to them; ones a saved link or a tracker still
/// names are kept past it.
pub(crate) const RETIRED_PRUNING_TARGET: usize = 64;

/// The whole record.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WorkSources {
    pub(crate) workspaces: BTreeMap<String, WorkspaceCommand>,
    /// Oldest first.
    pub(crate) retired: Vec<RetiredSource>,
}

/// One workspace's ticket command.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkspaceCommand {
    /// As the user supplied it.
    pub(crate) path: String,
    pub(crate) resolved_path: PathBuf,
    /// The id pinned on the first accepted `info`; links and credentials are kept under it.
    pub(crate) source_id: String,
    /// This workspace's last accepted `info`: its name, keys and credential fields may differ from
    /// another workspace's for the same id, since they can depend on the credentials.
    pub(crate) info: CommandInfo,
    pub(crate) checked_at: String,
    /// Why the last check failed, already masked; `None` after a good one.
    pub(crate) last_error: Option<String>,
    /// The safe excerpt of an answer that couldn't be read (its byte count while a secret is set).
    pub(crate) last_error_detail: Option<String>,
    /// The other id a later `info` answered with, until the user switches to it.
    pub(crate) offered_id: Option<String>,
}

/// An id no workspace pins any more, kept so the links saved under it still read as unavailable.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RetiredSource {
    pub(crate) id: String,
    pub(crate) format: IdFormat,
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
    pub(crate) retired_at: String,
}

impl WorkSources {
    /// Every id the record knows, active or retired.
    pub(crate) fn known_ids(&self) -> BTreeSet<&str> {
        self.workspaces
            .values()
            .map(|command| command.source_id.as_str())
            .chain(self.retired.iter().map(|retired| retired.id.as_str()))
            .collect()
    }

    /// The `ids.format` an id has on this computer, if a workspace pins it or it was retired.
    pub(crate) fn format_of(&self, id: &str, except_workspace: &str) -> Option<IdFormat> {
        self.workspaces
            .iter()
            .filter(|(workspace_id, _)| workspace_id.as_str() != except_workspace)
            .find(|(_, command)| command.source_id == id)
            .map(|(_, command)| command.info.id_format)
            .or_else(|| {
                self.retired
                    .iter()
                    .find(|retired| retired.id == id)
                    .map(|retired| retired.format)
            })
    }

    fn from_value(value: &Value) -> Option<Self> {
        let workspaces = value
            .get("workspaces")?
            .as_object()?
            .iter()
            .map(|(workspace_id, command)| {
                Some((workspace_id.clone(), WorkspaceCommand::from_value(command)?))
            })
            .collect::<Option<BTreeMap<_, _>>>()?;
        let retired = value
            .get("retired")?
            .as_array()?
            .iter()
            .map(RetiredSource::from_value)
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            workspaces,
            retired,
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "workspaces": self
                .workspaces
                .iter()
                .map(|(workspace_id, command)| (workspace_id.clone(), command.to_value()))
                .collect::<Map<String, Value>>(),
            "retired": self.retired.iter().map(RetiredSource::to_value).collect::<Vec<_>>(),
        })
    }
}

impl WorkspaceCommand {
    fn from_value(value: &Value) -> Option<Self> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        // The stored info is read back through the contract parser. A record the contract doesn't
        // accept makes the whole store unreadable, so a later write cannot erase its pin.
        let mut info = parse_info(value.get("info")?, &AnswerGuard::NONE).ok()?;
        if let Some(reason) = value.pointer("/info/iconError").and_then(Value::as_str) {
            info.icon = Some(Err(reason.to_string()));
        }
        let source_id = text("sourceId")?;
        (info.id == source_id).then_some(())?;
        Some(Self {
            path: text("path")?,
            resolved_path: PathBuf::from(text("resolvedPath")?),
            source_id,
            info,
            checked_at: text("checkedAt").unwrap_or_default(),
            last_error: text("lastError"),
            last_error_detail: text("lastErrorDetail"),
            offered_id: text("offeredId"),
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "path": self.path,
            "resolvedPath": self.resolved_path.to_string_lossy(),
            "sourceId": self.source_id,
            "info": info_value(&self.info),
            "checkedAt": self.checked_at,
            "lastError": self.last_error,
            "lastErrorDetail": self.last_error_detail,
            "offeredId": self.offered_id,
        })
    }
}

impl RetiredSource {
    fn from_value(value: &Value) -> Option<Self> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Self {
            id: text("id")?,
            format: match value.get("format")?.as_str()? {
                "key-number" => IdFormat::KeyNumber,
                "number" => IdFormat::Number,
                _ => return None,
            },
            name: text("name")?,
            icon: text("icon"),
            retired_at: text("retiredAt").unwrap_or_default(),
        })
    }

    fn to_value(&self) -> Value {
        json!({
            "id": self.id,
            "format": format_word(self.format),
            "name": self.name,
            "icon": self.icon,
            "retiredAt": self.retired_at,
        })
    }
}

pub(crate) fn format_word(format: IdFormat) -> &'static str {
    match format {
        IdFormat::KeyNumber => "key-number",
        IdFormat::Number => "number",
    }
}

/// An accepted `info` in the contract's own shape, plus `iconError` when its icon was refused.
pub(crate) fn info_value(info: &CommandInfo) -> Value {
    json!({
        "id": info.id,
        "name": info.name,
        "icon": info.icon.as_ref().and_then(|icon| icon.as_ref().ok()),
        "iconError": info.icon.as_ref().and_then(|icon| icon.as_ref().err()),
        "ids": { "format": format_word(info.id_format), "keys": info.id_keys },
        "credentials": info.credentials.iter().map(|field| json!({
            "name": field.name,
            "label": field.label,
            "secret": field.secret,
            "required": field.required,
            "placeholder": field.placeholder,
        })).collect::<Vec<_>>(),
        "canCreate": info.can_create,
    })
}

pub(crate) fn read_work_sources(db: &Connection) -> Result<WorkSources, DomainStateError> {
    let stored = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [WORK_SOURCES_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(stored) = stored else {
        return Ok(WorkSources::default());
    };
    serde_json::from_str::<Value>(&stored)
        .ok()
        .and_then(|value| WorkSources::from_value(&value))
        .ok_or_else(|| DomainStateError {
            code: "internalError",
            // A damaged row can contain a credential. Never quote its text or a parser excerpt.
            message: "Ticket command store can't be read; no changes were saved.".to_string(),
        })
}

pub(crate) fn write_work_sources(
    db: &Connection,
    sources: &WorkSources,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![
            WORK_SOURCES_METADATA_KEY,
            sources.to_value().to_string(),
            now_iso()
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite ticket sources error: {error}"),
    }
}
