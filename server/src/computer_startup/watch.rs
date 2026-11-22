//! The running gxserver's part: keep the login entry in step with the switch, and resume the
//! sessions found at startup.

use std::{sync::Arc, time::Duration};

use super::{
    ComputerStartupSettings,
    boot_mark::note_this_boot,
    login_entry,
    resume_after_restart::{SessionToResume, find_sessions_to_resume, resume_sessions},
};
use crate::{paths::GxserverPaths, server::AppState};

/// How often the settings file is read, like the Auto Sleep sweep: every writer (the app, the
/// `ghostex settings` CLI, a hand edit) reaches the login entry within this long.
const SETTINGS_POLL: Duration = Duration::from_secs(5);

/// Runs during startup, before clients can connect (their first listing would mark the dead
/// daemons `missing` and erase what this reads). Always saves this boot's mark.
pub(crate) fn find_sessions_to_resume_after_restart(
    paths: &GxserverPaths,
    server_id: &str,
) -> Vec<SessionToResume> {
    let restarted = note_this_boot(paths);
    let resume = restarted && ComputerStartupSettings::read(paths).resume_sessions;
    let cut_off = find_sessions_to_resume(paths, server_id);
    // CDXC:Coordinators 2026-11-14 DECISION in server/src/coordinators/stopped_by_restart.rs:
    // coordinator threads cut off this way wait for Carry on, unless the switch carries them on.
    if let Ok(db) = crate::storage::open_gxserver_database(paths) {
        let repository = crate::domain::DomainRepository::new(&db, server_id);
        // Q67: only threads that were working; a finished one just stays idle.
        let keys: Vec<_> = cut_off
            .iter()
            .filter(|session| session.was_working())
            .map(SessionToResume::key)
            .collect();
        crate::coordinators::note_restart(&repository, &keys, !resume);
    }
    if !resume {
        return Vec::new();
    }
    cut_off
}

pub(crate) fn start_computer_startup(state: Arc<AppState>, to_resume: Vec<SessionToResume>) {
    resume_sessions(state.clone(), to_resume);
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut applied: Option<bool> = None;
        let mut last_error: Option<String> = None;
        let mut poll = crate::ticks::steady_interval(SETTINGS_POLL);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = poll.tick() => {}
            }
            let paths = state.paths.clone();
            let wanted = ComputerStartupSettings::read(&paths).start_server;
            if applied == Some(wanted) {
                continue;
            }
            let result =
                tokio::task::spawn_blocking(move || login_entry::apply(&paths, wanted)).await;
            match result {
                Ok(Ok(())) => {
                    applied = Some(wanted);
                    last_error = None;
                }
                // Tried again at the next poll; logged once per distinct problem.
                Ok(Err(error)) => {
                    let message = format!("{error:#}");
                    if last_error.as_deref() != Some(message.as_str()) {
                        eprintln!("update the Ghostex login entry failed: {message}");
                        last_error = Some(message);
                    }
                }
                Err(_) => {}
            }
        }
    });
}
