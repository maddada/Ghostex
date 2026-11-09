//! gxserver's own marks on GBot chats, which a session update may not change.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! Security review of the GBot API (finding 9): `/api/updateSession` replaces `runtimeSettings` whole for any agent with a session token, so an agent could erase an API chat's `gbot.openedBy: api:…` mark (which the input, memory and sandbox rules read) or a chat's `gbotHome` mark, or move an API chat out of its own folder into the GBot's (whose instruction files carry its memory). Those marks belong to gxserver: an update keeps them, and an API chat's folder, as they are.

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::domain::DomainRepository;

/// The `runtimeSettings` keys gxserver alone sets on GBot chats.
const MARKS: [&str; 2] = ["gbot", "gbotHome"];

/// `params` for `/api/updateSession` with the session's GBot marks (and an API chat's `cwd`) put
/// back as they are stored.
pub(crate) fn keep_gbot_marks(
    _db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Map<String, Value> {
    let mut params = params.clone();
    let text = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_string);
    let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
        return params;
    };
    let Some(current) = repository
        .get_session(&project_id, &session_id)
        .ok()
        .flatten()
    else {
        return params;
    };
    let stored = current.get("runtimeSettings").and_then(Value::as_object);
    if let Some(runtime) = params
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
    {
        for mark in MARKS {
            match stored.and_then(|stored| stored.get(mark)) {
                Some(value) => {
                    runtime.insert(mark.to_string(), value.clone());
                }
                None => {
                    runtime.remove(mark);
                }
            }
        }
    }
    if super::api::is_api_chat(&current) && params.contains_key("cwd") {
        match current.get("cwd") {
            Some(cwd) => {
                params.insert("cwd".to_string(), cwd.clone());
            }
            None => {
                params.remove("cwd");
            }
        }
    }
    params
}
