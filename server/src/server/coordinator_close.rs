//! Closing a coordinator closes the threads that were open under it.

use super::*;

/// Closes every thread open under the session that was just closed, when that session is a
/// coordinator, the same close a session's own Close is (its conversation stays resumable), and
/// marks each one so restoring the coordinator restores it. Answers how many it closed.
///
/// CDXC:Coordinators 2026-11-02 DECISION: User: closing a coordinator closes every thread under it that is open in the sidebar (not the Done ones) without losing their work, and restoring the coordinator restores the threads that were open when it was closed, on the same conversations, under it again. Done here, after any close of the coordinator's session, so the desktop, the phone and `ghostex kill` all close the threads with it; an agent that exits on its own closes nothing else.
/// SEE-ALSO: server/src/coordinators/close.rs (which threads, and the marks), server/src/previous_session_restore.rs (the restore that brings them back), server/src/server/coordinator_runtime.rs (the supervisor leaves marked threads alone).
pub(crate) fn close_threads_with_coordinator(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    coordinator: &crate::coordinators::SessionKey,
) -> usize {
    let is_coordinator = crate::coordinators::read_coordinator(db, &coordinator.0, &coordinator.1)
        .ok()
        .flatten()
        .is_some();
    if !is_coordinator {
        return 0;
    }
    // A closed main coordinator hands its project's label to the oldest coordinator left there
    // (coordinators/main_coordinator.rs), whose row shows it now.
    if let Some(successor) = crate::coordinators::successor_after_close(db, repository, coordinator)
    {
        let _ =
            schedule_presentation_session_delta(state, db, repository, &successor.0, &successor.1);
    }
    let Ok(threads) = crate::coordinators::threads_open_under(db, repository, coordinator) else {
        return 0;
    };
    let mut closed = 0;
    for thread in threads {
        let mut params = Map::new();
        params.insert("action".to_string(), json!("close"));
        params.insert("projectId".to_string(), json!(thread.project_id));
        params.insert("reason".to_string(), json!("coordinatorClosed"));
        params.insert("closeSource".to_string(), json!("server.coordinatorClosed"));
        params.insert(
            "closedByRef".to_string(),
            json!(format!(
                "{}:{}:{}",
                state.metadata.server_id, coordinator.0, coordinator.1
            )),
        );
        params.insert("sessionId".to_string(), json!(thread.session_id));
        let routed = dispatch_zmx_lifecycle_http_blocking(
            state,
            "/api/transitionSession".to_string(),
            uuid::Uuid::new_v4().to_string(),
            params,
        );
        if routed.response.status().is_success()
            && crate::coordinators::mark_thread_closed_with_coordinator(db, &thread.key()).is_ok()
        {
            closed += 1;
        }
    }
    closed
}
