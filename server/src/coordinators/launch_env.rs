//! The variables that tell a session's agent, at launch, that it is a coordinator or one of its
//! threads (`GHOSTEX_COORDINATOR_ROLE`, `GHOSTEX_COORDINATOR_SESSION`).
//!
//! CDXC:Coordinators 2026-11-07 WHY:
//! Issue #463: a reporter runs every agent through a launch wrapper that starts coordinator threads
//! with fewer add-ons (MCP servers, plugins) and leaves the sessions they start themselves alone. At
//! launch nothing told the wrapper which one it was: the thread link reaches the session list about
//! 250 ms after the agent starts, and a thread's environment matched a hand-started session's. The
//! user's condition was "only implement this if it's safe and is unlikely to cause regressions for
//! other users", so this only adds two variables to the agent's environment and changes nothing about
//! how threads are linked, reported or listed.
//! They are worked out from the coordinator records each time the session's terminal starts (a fresh
//! launch, a wake, a restart), never stored on the session, so an unlinked thread loses them at its
//! next start and a resolved thread that is reopened gets them again. gxserver strips both from what
//! its own sessions inherit (zmx/env.rs), so a gxserver started from a thread passes neither on.
//! Local only: a thread on another computer (`start-thread --computer`) gets neither, since that
//! computer has no record of the link to read at each start, and carrying it there would need new
//! remote API (the user ruled that out).
//! SEE-ALSO: server/src/zmx/provider.rs (`start_session_provider`, the one place an agent's terminal
//! starts), server/src/zmx/scripts.rs and scripts_windows.rs (the exports),
//! server/src/ghostex_cli/coordinator/threads.rs (`start_thread`, which names the coordinator for a
//! worktree thread whose terminal starts before its link is written).

use rusqlite::Connection;

use super::records::{read_coordinator, read_thread};

/// `coordinator` for a coordinator's own agent, `thread` for a thread's.
pub const COORDINATOR_ROLE_ENV: &str = "GHOSTEX_COORDINATOR_ROLE";
/// A thread's coordinator, as its global session ref (`S1:P1abc:G1abc`).
pub const COORDINATOR_SESSION_ENV: &str = "GHOSTEX_COORDINATOR_SESSION";

/// The request param naming the coordinator (its global ref) of a thread whose terminal starts in
/// the same request that creates it, before `start-thread` links it (`createWorktreeSession`). It
/// counts for that one start only and is never stored.
pub const COORDINATOR_THREAD_OF_PARAM: &str = "coordinatorThreadOf";

/// The coordinator variables for the agent of session `project_id`/`session_id`, from the
/// coordinator records as they are now. `resolve_global_ref` answers a coordinator session's global
/// ref; `thread_of` is a just-created thread's coordinator ref (`COORDINATOR_THREAD_OF_PARAM`).
/// Empty for every other session, and whenever the records cannot be read: a launch never fails
/// over these.
pub fn coordinator_launch_environment(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    thread_of: Option<&str>,
    resolve_global_ref: impl Fn(&str, &str) -> Option<String>,
) -> Vec<(&'static str, String)> {
    if matches!(read_coordinator(db, project_id, session_id), Ok(Some(_))) {
        return vec![(COORDINATOR_ROLE_ENV, "coordinator".to_string())];
    }
    let coordinator_ref = match read_thread(db, project_id, session_id) {
        Ok(Some(thread)) => Some(resolve_global_ref(
            &thread.coordinator_project_id,
            &thread.coordinator_session_id,
        )),
        _ => thread_of
            .map(str::trim)
            .filter(|reference| !reference.is_empty())
            .map(|reference| Some(reference.to_string())),
    };
    let Some(coordinator_ref) = coordinator_ref else {
        return Vec::new();
    };
    let mut environment = vec![(COORDINATOR_ROLE_ENV, "thread".to_string())];
    if let Some(reference) = coordinator_ref.filter(|reference| !reference.is_empty()) {
        environment.push((COORDINATOR_SESSION_ENV, reference));
    }
    environment
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinators::{
        insert_coordinator, link_thread, set_thread_resolved, unlink_thread,
    };

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        for migration in [
            include_str!("../storage/migrations/0041_coordinators.sql"),
            include_str!("../storage/migrations/0042_coordinator_pending_messages.sql"),
            include_str!("../storage/migrations/0050_coordinator_close_threads.sql"),
        ] {
            db.execute_batch(migration).unwrap();
        }
        insert_coordinator(&db, "P1", "C1", "", "").unwrap();
        db
    }

    fn env(
        db: &Connection,
        session_id: &str,
        thread_of: Option<&str>,
    ) -> Vec<(&'static str, String)> {
        coordinator_launch_environment(db, "P1", session_id, thread_of, |project, session| {
            Some(format!("S1:{project}:{session}"))
        })
    }

    #[test]
    fn a_coordinator_is_told_it_is_one() {
        assert_eq!(
            env(&db(), "C1", None),
            vec![(COORDINATOR_ROLE_ENV, "coordinator".to_string())]
        );
    }

    #[test]
    fn a_thread_names_its_coordinator_until_it_is_unlinked() {
        let db = db();
        link_thread(&db, "P1", "T1", "P1", "C1", "task").unwrap();
        let thread = vec![
            (COORDINATOR_ROLE_ENV, "thread".to_string()),
            (COORDINATOR_SESSION_ENV, "S1:P1:C1".to_string()),
        ];
        assert_eq!(env(&db, "T1", None), thread);
        // Resolved and reopened: still its coordinator's thread.
        set_thread_resolved(&db, "P1", "T1", true).unwrap();
        assert_eq!(env(&db, "T1", None), thread);
        // Unlinked: its next start is an ordinary session's.
        assert!(unlink_thread(&db, "P1", "T1").unwrap());
        assert!(env(&db, "T1", None).is_empty());
    }

    #[test]
    fn a_session_a_person_starts_gets_neither() {
        assert!(env(&db(), "S9", None).is_empty());
    }

    #[test]
    fn a_worktree_thread_started_before_its_link_takes_the_named_coordinator() {
        assert_eq!(
            env(&db(), "T2", Some("S1:P1:C1")),
            vec![
                (COORDINATOR_ROLE_ENV, "thread".to_string()),
                (COORDINATOR_SESSION_ENV, "S1:P1:C1".to_string()),
            ]
        );
        assert!(env(&db(), "T2", Some(" ")).is_empty());
    }

    #[test]
    fn a_thread_whose_coordinator_ref_is_unknown_still_has_its_role() {
        let db = db();
        link_thread(&db, "P1", "T1", "P1", "C1", "task").unwrap();
        assert_eq!(
            coordinator_launch_environment(&db, "P1", "T1", None, |_, _| None),
            vec![(COORDINATOR_ROLE_ENV, "thread".to_string())]
        );
    }
}
