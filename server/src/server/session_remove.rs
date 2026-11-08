//! Removing a session row and everything that hangs off it: `/api/removeSession`, and the
//! storage cleanup sweep when it drops closed sessions past their age limit.

use super::*;

pub(crate) fn remove_session_row(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let session = repository.remove_session(params)?;
    crate::agentbox::stop_session_box_in_background(repository, &session);
    /*
    CDXC:Drafts 2026-08-28:
    Removing a DRAFT also kills its background agent CLI. The row
    being deleted is the last thing that pointed at that zmx daemon,
    so deleting a draft would otherwise leave an orphaned CLI running
    with nothing in any sidebar able to stop it. Scoped strictly to
    drafts: removing any other session behaves exactly as it did
    before — those are sessions the user is expected to have closed
    deliberately, and changing that is not this feature's call to
    make.
    */
    crate::agents::kill_draft_session_provider(&session);
    let project_id = value_text(&session, "projectId")?;
    let session_id = value_text(&session, "sessionId")?;
    // The session's own token ends with it (crate::security).
    state
        .security
        .revoke_session_token(&project_id, &session_id);
    crate::session_chat_queue::forget_session_chat_queue(db, &project_id, Some(&session_id))?;
    // CDXC:Collab 2026-11-08 DECISION: User (#495): "A share ends when its chat is deleted."
    super::collab_http::end_share_of_deleted_chat(&project_id, &session_id);
    crate::session_chat_queue_attachments::sweep(&state.paths, db);
    schedule_presentation_session_delta(state, db, repository, &project_id, &session_id)?;
    Ok(session)
}
