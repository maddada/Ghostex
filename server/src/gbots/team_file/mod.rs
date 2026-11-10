//! Team files and presets: GBots written out as one JSON file and read back with a preview first.
//!
//! - [`format`]: the file's shape, its limits and reading it (unknown fields listed, not refused).
//! - [`export`]: `teamExport`, secrets redacted; [`import`]: `teamPreview`, `teamImport` and the
//!   Undo of an import (one `teamFileAdded` change).
//! - [`presets`]: the GBots New GBot starts from, built in or kept from a preset file.
//!
//! | op | params | who |
//! | --- | --- | --- |
//! | `teamExport` | `preset` (a GBot) or `department` or `bots`; `notes?`, `name?`, `summary?`, `release?` | you, Chief of Staff |
//! | `teamPreview` | `file` (its text), `agentId?` | anyone here |
//! | `teamImport` | `file`, `agentId?` | you (your computer, phone or terminal, after your approval) |
//! | `presets`, `preset` | `preset` | anyone here |
//! | `presetSave` | `file` (a preset file) or `bot` | you, Chief of Staff |
//! | `presetRemove` | `preset` | you, Chief of Staff |
//! | `presetExport` | `preset` | anyone here |
//!
//! `create` takes `preset` too (endpoint.rs): the fields it leaves out come from the preset.

mod export;
mod format;
mod import;
mod presets;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::gbots::changes::Change;
use crate::server::AppState;

pub(crate) use import::TEAM_CHANGE;
pub(crate) use presets::with_preset;

/// Whether `op` is one of these.
pub(crate) fn handles(op: &str) -> bool {
    op.starts_with("team") || op == "presets" || op.starts_with("preset")
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

fn required<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    text(params, key).ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
}

/// Who is asking, as these ops see it.
pub(crate) struct TeamCaller {
    /// Your computer, phone or terminal.
    pub is_user: bool,
    /// You or Chief of Staff.
    pub may_manage: bool,
    pub label: &'static str,
}

/// The ops. `create` and `remove` are the endpoint's own add and removal of one GBot.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    caller: TeamCaller,
    op: &str,
    params: &Map<String, Value>,
    create: &mut dyn FnMut(&Map<String, Value>, &str) -> Result<Value, DomainStateError>,
    remove: &mut dyn FnMut(&str) -> Result<Value, DomainStateError>,
) -> Result<Value, DomainStateError> {
    match op {
        "teamExport" => {
            if !caller.may_manage {
                return Err(forbidden(
                    "Only you and Chief of Staff export GBots to a team file.",
                ));
            }
            export::export(&state.paths, db, params)
        }
        "teamPreview" => {
            let plan = import::plan(db, repository, params)?;
            Ok(json!({ "preview": import::preview_value(&plan) }))
        }
        "teamImport" => {
            if !caller.is_user {
                return Err(forbidden(
                    "Only you import a team file, from Ghostex or your own terminal. Tell the user to import it (GBots heading > Import Team…, or `ghostex gbots team import <file>`); you may preview it with --dry-run.",
                ));
            }
            import::import(state, db, repository, caller.label, params, create, remove)
        }
        "presets" => Ok(presets::list(&state.paths)),
        "preset" => {
            let preset = presets::find(&state.paths, required(params, "preset")?)?;
            Ok(json!({ "preset": preset.summary(true) }))
        }
        "presetExport" => {
            let preset = presets::find(&state.paths, required(params, "preset")?)?;
            let file = presets::as_file(&preset);
            let text = serde_json::to_string_pretty(&file).unwrap_or_default();
            Ok(json!({
                "file": file,
                "text": format!("{text}\n"),
                "fileName": format!("{}{}", format::slug(&file.name), file.kind.extension()),
                "kind": "preset",
            }))
        }
        "presetSave" => {
            if !caller.may_manage {
                return Err(forbidden("Only you and Chief of Staff keep GBot presets."));
            }
            let (file, source) = if let Some(bot) = text(params, "bot") {
                let mut scope = Map::new();
                scope.insert("preset".into(), json!(bot));
                scope.insert(
                    "notes".into(),
                    json!(params.get("notes").and_then(Value::as_bool) == Some(true)),
                );
                let (file, _) = export::build(&state.paths, db, &scope)?;
                (file, "Saved from a GBot".to_string())
            } else {
                let read = import::read_params(params)?;
                if read.file.kind != format::Kind::Preset {
                    return Err(DomainStateError::bad_request(
                        "That is a team file, not a preset: import it with `ghostex gbots team import`.",
                    ));
                }
                if !read.problems.is_empty() {
                    return Err(DomainStateError::bad_request(format!(
                        "That preset can't be kept: {}",
                        read.problems.join(" ")
                    )));
                }
                (read.file, "Imported".to_string())
            };
            let preset = presets::save(&state.paths, file, &source)?;
            Ok(json!({ "preset": preset.summary(true) }))
        }
        "presetRemove" => {
            if !caller.may_manage {
                return Err(forbidden("Only you and Chief of Staff remove GBot presets."));
            }
            let preset = presets::remove(&state.paths, required(params, "preset")?)?;
            Ok(json!({ "removed": preset.summary(false) }))
        }
        other => Err(DomainStateError::bad_request(format!(
            "Unknown GBots op {other}: use teamExport, teamPreview, teamImport, presets, preset, presetSave, presetRemove or presetExport."
        ))),
    }
}

/// After a `create` from a preset: the preset's starter notes go into the new GBot's memory.
pub(crate) fn after_preset_create(
    state: &AppState,
    db: &Connection,
    answer: &Value,
    preset: Option<&format::FileBot>,
    by: &str,
) {
    let Some(preset) = preset else { return };
    let Some(bot) = answer["bot"]["botId"]
        .as_str()
        .and_then(|id| crate::gbots::record::read_bot(db, id).ok().flatten())
    else {
        return;
    };
    let _ = import::write_starter_notes(
        state,
        db,
        &bot,
        preset,
        by,
        &format!("the preset {}", preset.name),
    );
}

/// Whether `change` is an import (changes.rs reads those apart).
pub(crate) fn is_team_change(kind: &str) -> bool {
    kind == TEAM_CHANGE
}

pub(crate) fn undo_problem(
    db: &Connection,
    change: &Change,
) -> Result<Option<String>, DomainStateError> {
    import::undo_problem(db, change)
}

pub(crate) fn change_summary(db: &Connection, change: &Change) -> Result<Value, DomainStateError> {
    import::change_summary(db, change)
}

pub(crate) fn undo(
    db: &Connection,
    change: &Change,
    by: &str,
    remove: &mut dyn FnMut(&str) -> Result<Value, DomainStateError>,
) -> Result<Value, DomainStateError> {
    import::undo(db, change, by, remove)
}
