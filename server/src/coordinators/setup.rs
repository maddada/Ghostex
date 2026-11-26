//! The coordinator setup: which agent, model and effort a coordinator gives each kind of work, what
//! it does when usage runs low, which accounts threads never use and which computers they may run
//! on. The user answers it once on a Live page (`ghostex coordinator setup page`, written by
//! ghostex_cli/coordinator/setup.rs); gxserver keeps the answers as the user's default for every
//! new coordinator here, synced to the user's other computers as `coordinator/setup/defaults`
//! (settings_sync/categories.rs), and each coordinator keeps its own copy as "Setup:" memory notes.
//! Also the coordinator flow page Ghostex ships (`coordinator_flow.html`).
//!
//! CDXC:Coordinators 2026-11-26 DECISION:
//! The user, for a new coordinator's first turn: an interview as a Live page "never plain chat text", asked "on a new coordinator's first turn, before any work; and again whenever the user asks ('redo the interview'). Not on new sign-ins, not once per project", with three kinds of work (large and complex, hard and focused, simple), each "a choice of agent + model + effort, only from agents and models signed in and working", what to do when usage runs low, which accounts never to use (asked only with more than one account of an agent) and which computers threads may run on. Stored "as the user's default that prefills every new coordinator's interview, AND in that coordinator's own memory. Not per project", changed "by asking the coordinator to redo or change them. No Settings page."
//! SEE-ALSO: server/src/ghostex_cli/coordinator/setup.rs (the CLI and the page), server/src/coordinators/playbook.md (First turn: setup), server/src/settings_sync/categories.rs (the synced key), server/src/visual_pages/kit.rs (`picked` and menus questions).

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::records::{now_iso, sql_error};
use crate::domain::DomainStateError;

/// Where gxserver keeps the user's default setup.
const DEFAULT_KEY: &str = "coordinatorSetupDefault";

/// Every memory note of a coordinator's setup starts with this; a save replaces them as a set.
pub const SETUP_NOTE_PREFIX: &str = "Setup:";

/// The setup's JSON shape; a newer shape gets a new number.
pub const SETUP_VERSION: u64 = 1;

/// A setup is a few lines of picks and one rule; anything bigger is a mistake.
const SETUP_MAX_BYTES: usize = 16 * 1024;

pub const COORDINATOR_SETUP_ENDPOINTS: &[&str] =
    &["/api/readCoordinatorSetup", "/api/setCoordinatorSetup"];

/// The coordinator flow page, built into Ghostex: what working with a coordinator looks like and
/// why. Its screenshots load over https from the repository's README images
/// (media/readme/coordinator/), so they show once that folder is on the main branch; offline each
/// keeps its frame and its words.
///
/// CDXC:Coordinators 2026-11-26 DECISION:
/// The user asked for a page "about the philosophy of coordinators and why it beats managing threads by hand. Focus on the benefits", recommended shipped "as a ready-made page bundled with Ghostex ... so every coordinator shows the same polished page instantly and spends no tokens building it", with its screenshots loading "over https from the repo's README images on GitHub ... NOT bundled", alt text on every image and a quiet frame instead of a broken picture offline, all three screenshot sets kept. `ghostex coordinator setup explain` publishes it in a coordinator's chat; the How to use pill opens it (gx-chat-core extras/coordinator_intro.rs).
pub const COORDINATOR_FLOW_PAGE_HTML: &str = include_str!("coordinator_flow.html");

/// The flow page's key among the pages Ghostex ships (visual_pages/app_page.rs).
const FLOW_PAGE_KEY: &str = "coordinatorFlow";

/// The user's default setup, `None` before they first answer the interview.
pub fn setup_default(db: &Connection) -> Result<Option<Value>, DomainStateError> {
    let text = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [DEFAULT_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(text
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object))
}

/// Saves (or, with `None`, removes) the user's default setup.
pub fn set_setup_default(db: &Connection, value: Option<&Value>) -> Result<(), DomainStateError> {
    match value {
        Some(value) => {
            if !value.is_object() {
                return Err(DomainStateError::bad_request("A setup is a JSON object."));
            }
            let text = value.to_string();
            if text.len() > SETUP_MAX_BYTES {
                return Err(DomainStateError::bad_request(
                    "This setup is too long to save.",
                ));
            }
            db.execute(
                r#"
                INSERT INTO metadata (key, value, updatedAt)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
                "#,
                rusqlite::params![DEFAULT_KEY, text, now_iso()],
            )
            .map_err(sql_error)?;
        }
        None => {
            db.execute("DELETE FROM metadata WHERE key = ?1", [DEFAULT_KEY])
                .map_err(sql_error)?;
        }
    }
    Ok(())
}

/// The default as settings sync carries it.
pub fn synced_setup_default(db: &Connection) -> Result<Option<Value>, DomainStateError> {
    setup_default(db)
}

/// The default settings sync brought from another computer (`None` removes it).
pub fn write_synced_setup_default(
    db: &Connection,
    value: Option<&Value>,
) -> Result<(), DomainStateError> {
    if setup_default(db)?.as_ref() == value {
        return Ok(());
    }
    set_setup_default(db, value.filter(|value| value.is_object()))
}

/// `/api/readCoordinatorSetup` `{}` answers `{ default }` (`null` before the first interview);
/// `/api/setCoordinatorSetup` `{ default }` saves it (`null` removes it) and answers the same.
pub fn handle_coordinator_setup_endpoint(
    endpoint_path: &str,
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if endpoint_path == "/api/setCoordinatorSetup" {
        let value = params.get("default").filter(|value| !value.is_null());
        set_setup_default(db, value)?;
    }
    Ok(json!({ "default": setup_default(db)? }))
}

/// The coordinator flow page's id and title, publishing it the first time (and again after an
/// update changes it).
pub fn coordinator_flow_page(gxserver_state_dir: &Path) -> Result<(String, String), String> {
    let id = crate::visual_pages::app_page(
        gxserver_state_dir,
        FLOW_PAGE_KEY,
        COORDINATOR_FLOW_PAGE_HTML,
    )?;
    Ok((id, flow_page_title()))
}

/// The page's own `<title>`.
pub fn flow_page_title() -> String {
    COORDINATOR_FLOW_PAGE_HTML
        .split_once("<title>")
        .and_then(|(_, rest)| rest.split_once("</title>"))
        .map(|(title, _)| title.trim().to_string())
        .unwrap_or_else(|| "How working with a coordinator goes".to_string())
}

/// Whether a coordinator's memory holds its setup.
pub fn has_setup_notes(memory: &Value) -> bool {
    memory.as_array().is_some_and(|notes| {
        notes.iter().any(|note| {
            note.get("text")
                .and_then(Value::as_str)
                .or_else(|| note.as_str())
                .is_some_and(|text| text.trim_start().starts_with(SETUP_NOTE_PREFIX))
        })
    })
}
