//! What GBots add to the sidebar presentation every client draws: `gbotsProject` on a bot's
//! project row (always), `gbotsBot` on it (its look and place) and `gbotsChat` on each of its
//! chats (its bot and who opened it), those two only while GBots are on. Building a presentation
//! never reads the database: the project rows read a copy of the bots kept here, refreshed
//! whenever a bot is added, edited or removed and at gxserver start (sweep.rs).
//!
//! CDXC:GBots 2026-11-08 WHY:
//! A new field rather than the previous GBots' `botKind: "gbots"` / `botProfile`, so a client from before the previous GBots was removed, on another computer, never draws a native GBot as one of those (plan risk 9). Clients that don't know `gbotsBot` yet draw the bot's project as an ordinary project.
//! SEE-ALSO: packages/gx-protocol/src/presentation.rs (`PresentationGbotsBot`, `PresentationGbotsChat`), packages/shared/gxserver-protocol-presentation.ts.

use std::collections::HashMap;
use std::sync::RwLock;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::project::gbots_bot_id;
use super::record::{color_hex, list_bots};

/// `gbotsBot` objects by bot id; `None` until first read.
static BOTS: RwLock<Option<HashMap<String, Value>>> = RwLock::new(None);

/// Reads every bot into the copy presentations use.
pub(crate) fn refresh_presented_bots(db: &Connection) {
    let Ok(bots) = list_bots(db) else {
        return;
    };
    let departments = super::departments::list_departments(db).unwrap_or_default();
    let mut map: HashMap<String, Value> = bots
        .into_iter()
        .map(|bot| {
            let mut look = json!({
                "id": bot.bot_id,
                "name": bot.name,
                "title": bot.title,
                "icon": bot.icon,
                "color": bot.color,
                "colorHex": color_hex(&bot.color),
                "agentId": bot.agent_id,
            });
            if let Some(object) = look.as_object_mut() {
                // Its department's name, icon and place, so every client draws Main Hub's
                // department sub-headings from the bots alone (gx-core sidebar_view/gbots.rs).
                if let Some(department) = bot.department_id.as_deref().and_then(|id| {
                    departments
                        .iter()
                        .find(|department| department.department_id == id)
                }) {
                    object.insert("departmentId".into(), json!(department.department_id));
                    object.insert("departmentName".into(), json!(department.name));
                    if !department.icon.is_empty() {
                        object.insert("departmentIcon".into(), json!(department.icon));
                    }
                    object.insert("departmentOrder".into(), json!(department.sort_order));
                }
                if bot.is_lead {
                    object.insert("isLead".into(), json!(true));
                }
                if bot.paused_at.is_some() {
                    object.insert("paused".into(), json!(true));
                }
            }
            (bot.bot_id, look)
        })
        .collect();
    // Where each one's chats run, and its chats on other computers (computer/).
    super::computer::decorate_presented_bots(db, &mut map);
    *BOTS
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(map);
}

fn presented_bot(bot_id: &str) -> Option<Value> {
    if let Some(map) = BOTS
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
    {
        return map.get(bot_id).cloned();
    }
    // Not read yet (a `ghostex` CLI process building its own presentation).
    let paths = crate::paths::get_gxserver_paths(None);
    if let Ok(db) = crate::storage::open_gxserver_database(&paths) {
        refresh_presented_bots(&db);
    }
    BOTS.read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .and_then(|map| map.get(bot_id).cloned())
}

/// Adds `gbotsProject` to a bot's project row, and `gbotsBot` while GBots are on.
///
/// CDXC:GBots 2026-11-09 WHY:
/// `gbotsProject` is published whether GBots are on or off, so a client keeps a GBot's folder out of the projects list in both cases: while GBots are off their chats sleep and nothing of them shows, rather than each bot turning into a plain project.
pub(crate) fn insert_gbots_project_presentation(project: &Value, output: &mut Map<String, Value>) {
    let Some(bot_id) = gbots_bot_id(project) else {
        return;
    };
    output.insert("gbotsProject".to_string(), Value::Bool(true));
    if !super::gbots_shown() {
        return;
    }
    if let Some(look) = presented_bot(bot_id) {
        output.insert("gbotsBot".to_string(), look);
    }
}

/// Adds `gbotsChat` to a session row of a bot's chat.
pub(crate) fn insert_gbots_session_presentation(session: &Value, output: &mut Map<String, Value>) {
    let Some((bot_id, opened_by)) = super::session::chat_of(session) else {
        return;
    };
    if !super::gbots_shown() {
        return;
    }
    output.insert(
        "gbotsChat".to_string(),
        json!({ "botId": bot_id, "openedBy": opened_by }),
    );
}
