//! Whether an orchestrator's threads keep it out of automatic sleep (CDXC:SessionSleep 2026-10-21,
//! whose decision text sits on `coordinator_thread_activity` in server/src/session_auto_sleep.rs).

use super::records::list_threads_for;
use super::state::{ThreadProgress, ThreadState};
use crate::domain::DomainRepository;

/// A working thread, or one waiting on a question or approval, is active; finished, sleeping,
/// closed and done threads are not. Takes `ThreadState::as_str`, the presentation's
/// `coordinatorThreadState`.
pub fn thread_state_keeps_coordinator_awake(state: &str) -> bool {
    state == ThreadState::Working.as_str() || state == ThreadState::Waiting.as_str()
}

/// The daemon's automatic-sleep decline: whether any thread linked to this orchestrator is active
/// now, classified the way the presentation classifies it.
pub fn coordinator_has_active_thread(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> bool {
    let Ok(threads) = list_threads_for(repository.connection(), project_id, session_id) else {
        return false;
    };
    let now = crate::presentation::now_iso();
    threads
        .iter()
        .filter(|thread| !thread.is_resolved())
        .filter(|thread| !(thread.project_id == project_id && thread.session_id == session_id))
        .any(|thread| {
            let session = repository
                .get_session(&thread.project_id, &thread.session_id)
                .ok()
                .flatten();
            thread_state_keeps_coordinator_awake(
                super::live::classify_thread_live(
                    session.as_ref(),
                    ThreadProgress::of(thread),
                    &now,
                )
                .as_str(),
            )
        })
}
