//! Making an existing session a coordinator (`/api/promoteCoordinator`, the sidebar's Advanced >
//! Make Coordinator, `ghostex coordinator promote`).

use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde_json::{json, Map, Value};

use super::endpoint::session_title_of;
use super::records::{
    insert_coordinator, read_coordinator, read_thread, sql_error, SessionKey,
    COORDINATOR_GOAL_MAX_CHARS,
};
use super::role::{
    coordinator_agent_families_text, coordinator_agent_family_supported, orchestrator_skill_line,
};
use crate::domain::{DomainRepository, DomainStateError};

/// What a promotion did: the session it turned into a coordinator, its agent family (whose skill
/// folder gets the orchestrator skill), and the message that hands the running agent its role.
pub struct CoordinatorPromotion {
    pub key: SessionKey,
    pub title: String,
    pub family: String,
    pub playbook_message: String,
}

/// CDXC:Coordinators 2026-10-04 DECISION:
/// User: "Make Coordinator" sits in the session context menu's Advanced submenu, "but I wanna be able to switch to coordinator without affecting the running thread at all". So promoting never restarts, reloads or interrupts the session: it stores the coordinator record (the crown, the thread tree, reports and `ghostex coordinator status` follow from it) and queues the message that hands over the role, which the queue delivers only once the agent is idle. Ghostex never resumes the session to apply the role.
/// CDXC:Coordinators 2026-10-17 DECISION:
/// User: "we should install this orchestrator skill when user picks the orchstrator option so no need to inject things the other way". A promoted session gets its role like a created one: gxserver installs the `ghostex-orchestrator` skill for its agent and the queued message starts with the skill line. Its saved commands are no longer rewritten with a role flag (supersedes that part of the 2026-10-04 decision); the session is marked as taking its role from the skill.
/// SEE-ALSO: server/src/server/coordinator_runtime.rs (`promote_coordinator`, which installs the skill and queues the message), server/src/coordinators/role.rs (the skill line), packages/gx-core/src/sidebar_menu/session.rs (the menu row).
pub fn promote_session_to_coordinator(
    db: &Connection,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<CoordinatorPromotion, DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    if project_id.is_empty() || session_id.is_empty() {
        return Err(DomainStateError::bad_request(
            "Making a coordinator needs projectId and sessionId.",
        ));
    }
    let goal = text("goal");
    if goal.chars().count() > COORDINATOR_GOAL_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "Keep the goal under {COORDINATOR_GOAL_MAX_CHARS} characters; set standing instructions later with ghostex coordinator set-instructions."
        )));
    }
    // One writer reservation over the read and both writes, so a hook updating the session in
    // between cannot be overwritten by the row read here.
    let transaction =
        Transaction::new_unchecked(db, TransactionBehavior::Immediate).map_err(sql_error)?;
    let repository = DomainRepository::new(&transaction, server_id);
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError::not_found("That session does not exist."))?;
    let title = session_title_of(&session);
    refuse_ineligible(
        &transaction,
        &repository,
        &session,
        &project_id,
        &session_id,
    )?;
    let project = repository
        .get_project(&project_id)?
        .ok_or_else(|| DomainStateError::not_found("The session's project does not exist."))?;
    let family = crate::agents::session_agent_family_id(&project, &session)
        .filter(|family| coordinator_agent_family_supported(family))
        .ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "A coordinator runs on {}, and this session runs another agent. Start a New Coordinator instead.",
                coordinator_agent_families_text()
            ))
        })?;

    let mut runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime.insert(
        super::role::COORDINATOR_ROLE_SETTING.to_string(),
        json!(super::role::COORDINATOR_ROLE_SKILL),
    );
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert("runtimeSettings".to_string(), Value::Object(runtime));
    repository.update_session(&update)?;
    insert_coordinator(&transaction, &project_id, &session_id, &goal, "")?;
    transaction.commit().map_err(sql_error)?;

    let playbook_message = playbook_message(&family, &goal);
    Ok(CoordinatorPromotion {
        key: (project_id, session_id),
        title,
        family,
        playbook_message,
    })
}

fn refuse_ineligible(
    db: &Connection,
    repository: &DomainRepository<'_>,
    session: &Value,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    if read_coordinator(db, project_id, session_id)?.is_some() {
        return Err(DomainStateError::bad_request(
            "That session is already a coordinator.",
        ));
    }
    if let Some(thread) = read_thread(db, project_id, session_id)? {
        let coordinator = repository
            .get_session(
                &thread.coordinator_project_id,
                &thread.coordinator_session_id,
            )?
            .map(|coordinator| session_title_of(&coordinator))
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "another coordinator".to_string());
        return Err(DomainStateError::bad_request(format!(
            "That session is a thread of \"{coordinator}\". A thread reports to its coordinator, so it cannot also lead threads of its own; make another session the coordinator instead."
        )));
    }
    if session.get("kind").and_then(Value::as_str) != Some("agent") {
        return Err(DomainStateError::bad_request(
            "Only an agent session can become a coordinator.",
        ));
    }
    if crate::agentbox::is_agentbox_session(session) {
        return Err(DomainStateError::bad_request(
            "A coordinator runs on this computer, so a session in a box cannot become one.",
        ));
    }
    // A draft can still switch its agent before its first prompt, and the role would not follow.
    if crate::agents::session_is_draft(session) {
        return Err(DomainStateError::bad_request(
            "This session has not started its conversation yet. Send it a first message first, or start a New Coordinator instead.",
        ));
    }
    Ok(())
}

/// The chat message that hands a promoted session its role: the skill line, then what changed.
pub fn playbook_message(family: &str, goal: &str) -> String {
    let mut message = orchestrator_skill_line(family);
    message.push_str(
        "\n\nGhostex: this session is now a coordinator. You keep this conversation; from now on you work by that skill.",
    );
    if !goal.trim().is_empty() {
        message.push_str(&format!("\n\nYour goal: {}", goal.trim()));
    }
    message.push_str(
        "\n\nSessions you started earlier in this conversation are not your threads yet. If there are any, list them for the user and, once they confirm, adopt each one with `ghostex coordinator link <session ref> --task \"<what it works on>\"`.\
\n\nNow run `ghostex coordinator status`, then tell the user in a line or two that you are their coordinator.",
    );
    message
}
