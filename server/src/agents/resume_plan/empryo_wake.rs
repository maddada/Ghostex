use std::path::Path;

use serde_json::{json, Map, Value};

use super::{get_empryo_session_reference, object_field, read_text_value, to_agent_resume_input};
use crate::domain::{DomainRepository, DomainResult};

/// CDXC:SessionIdentity 2026-10-07 WHY:
/// A woken Empryo session with no written session folder ran bare `empryo`, which reopens the folder's latest session, so it could open a neighbour session's conversation and its hooks then bound that conversation's id. It gets a seeded folder of its own instead, as a new launch does (launch_plan.rs), and the resume plan then wakes it with `--session <id>`.
pub(crate) fn seed_empryo_wake_session(
    repository: &DomainRepository<'_>,
    project: &Value,
    session: Value,
    settings: &Map<String, Value>,
) -> DomainResult<Value> {
    if read_text_value(&session, "kind").as_deref() != Some("agent")
        || crate::agentbox::is_agentbox_session(&session)
        || crate::agents::session_is_draft(&session)
    {
        return Ok(session);
    }
    let input = to_agent_resume_input(project, &session, settings);
    if input.agent_id.as_deref() != Some("empryo") || get_empryo_session_reference(&input).is_some()
    {
        return Ok(session);
    }
    let Some(cwd) = read_text_value(&session, "cwd")
        .or_else(|| read_text_value(project, "path"))
        .filter(|cwd| Path::new(cwd).is_dir())
    else {
        return Ok(session);
    };
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    crate::session_chat_empryo_launch_selection::seed_empryo_launch_session(
        Path::new(&cwd),
        None,
        &mut runtime_settings,
    )?;
    let mut update = Map::new();
    for key in ["projectId", "sessionId"] {
        update.insert(key.to_string(), json!(read_text_value(&session, key)));
    }
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update)
}
