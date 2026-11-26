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
/// The user asked for a page "about the philosophy of coordinators and why it beats managing threads by hand. Focus on the benefits", recommended shipped "as a ready-made page bundled with Ghostex ... so every coordinator shows the same polished page instantly and spends no tokens building it", with its screenshots loading "over https from the repo's README images on GitHub ... NOT bundled", alt text on every image and a quiet frame instead of a broken picture offline, all three screenshot sets kept. `ghostex coordinator setup explain` publishes it in a coordinator's chat, sent once after the setup is saved until the user marks it seen (`flow_page_seen`). The user later removed the How to use pill that also opened it: "no need for this pill now that we have this ux, just remove it".
pub const COORDINATOR_FLOW_PAGE_HTML: &str = include_str!("coordinator_flow.html");

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

/// Where gxserver keeps when the user marked the coordinator flow page seen.
const FLOW_SEEN_KEY: &str = "coordinatorFlowPageSeenAt";

/// When the user marked the coordinator flow page seen (an RFC 3339 time), `None` before.
///
/// CDXC:Coordinators 2026-11-26 DECISION:
/// The user, of the flow page a coordinator sends after its setup is saved: "Once the user marks it seen, no future coordinator sends it again (remember that globally, synced, like the setup default)". Set when a page published from the flow page's file comes back seen (visual_pages/answers.rs), synced as `coordinator/setup/flowSeen`.
pub fn flow_page_seen(db: &Connection) -> Result<Option<String>, DomainStateError> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [FLOW_SEEN_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(sql_error)
}

/// Records that the user marked the flow page seen (kept at its first time), or forgets it.
pub fn set_flow_page_seen(db: &Connection, at: Option<&str>) -> Result<(), DomainStateError> {
    match at {
        Some(at) => {
            db.execute(
                r#"
                INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3)
                ON CONFLICT(key) DO NOTHING
                "#,
                rusqlite::params![FLOW_SEEN_KEY, at, now_iso()],
            )
            .map_err(sql_error)?;
        }
        None => {
            db.execute("DELETE FROM metadata WHERE key = ?1", [FLOW_SEEN_KEY])
                .map_err(sql_error)?;
        }
    }
    Ok(())
}

/// The flag as settings sync carries it: `{ "seenAt": "<time>" }`.
pub fn synced_flow_page_seen(db: &Connection) -> Result<Option<Value>, DomainStateError> {
    Ok(flow_page_seen(db)?.map(|at| json!({ "seenAt": at })))
}

/// The flag settings sync brought from another computer. Seen stays seen: a removal is ignored.
pub fn write_synced_flow_page_seen(
    db: &Connection,
    value: Option<&Value>,
) -> Result<(), DomainStateError> {
    match value.and_then(|value| value["seenAt"].as_str()) {
        Some(at) => set_flow_page_seen(db, Some(at)),
        None => Ok(()),
    }
}

/// `/api/readCoordinatorSetup` `{}` answers `{ default, flowPageSeen }` (`default` is `null`
/// before the first interview); `/api/setCoordinatorSetup` `{ default }` saves it (`null` removes
/// it) and answers the same.
pub fn handle_coordinator_setup_endpoint(
    endpoint_path: &str,
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if endpoint_path == "/api/setCoordinatorSetup" {
        let value = params.get("default").filter(|value| !value.is_null());
        set_setup_default(db, value)?;
    }
    Ok(json!({
        "default": setup_default(db)?,
        "flowPageSeen": flow_page_seen(db)?.is_some(),
    }))
}

/// The file `ghostex coordinator setup explain` writes the flow page to and shows from, under
/// gxserver's state folder; a page published from it coming back seen sets `flow_page_seen`.
pub fn coordinator_flow_page_file(gxserver_state_dir: &Path) -> std::path::PathBuf {
    gxserver_state_dir
        .join("coordinator-setup")
        .join("coordinator-flow.html")
}

/// The page's own `<title>`.
pub fn flow_page_title() -> String {
    COORDINATOR_FLOW_PAGE_HTML
        .split_once("<title>")
        .and_then(|(_, rest)| rest.split_once("</title>"))
        .map(|(title, _)| title.trim().to_string())
        .unwrap_or_else(|| "How coordinators work".to_string())
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
