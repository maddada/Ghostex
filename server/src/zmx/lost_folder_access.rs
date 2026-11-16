//! Sessions whose zmx daemon outlived the app it was started from, so macOS no longer lets them into
//! Documents, Desktop or Downloads, and the card that offers to restart them.
//!
//! CDXC:Zmx 2026-11-14 DECISION:
//! User (Q3208): after an update, a session whose daemon's app file is gone says "This terminal lost access to your folders after the update" with a Restart button that restarts the session safely, on the desktop and the web build; no phone part. Daemons started since the fix run from a copy that outlives updates (session_host.rs), so this covers sessions started by older builds and any daemon whose program a rebuild replaced. A daemon whose program file is gone is what macOS judges with no app, which is when it turns its folder access to denied (GitHub #611). Every minute gxserver looks for those daemons, puts this card on the session's chat and prints one line into its terminal (the terminal view has no card), and takes the card away once the session runs a live program again. Restart is the sidebar's Sleep then Wake, so an agent comes back on its own conversation.
//! SEE-ALSO: session_host.rs, `restart` in server/src/session_chat_agent_restart.rs, `max_age` in server/src/session_chat_notice/watchdog_store.rs.

use std::{
    collections::HashSet,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use serde_json::{Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeActionKind, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource,
};
use crate::storage::open_gxserver_database;

pub(crate) const NOTICE_KIND: &str = "folderAccessLost";
pub(crate) const ACTION_RESTART: &str = "folderAccessRestart";
const TITLE: &str = "Lost access to your folders";
const DETAIL: &str =
    "This terminal lost access to your folders after the update. Restart it to get access back.";
const TERMINAL_LINE: &str = "Ghostex: This terminal lost access to your folders after the update. To get access back, restart it: right-click it in the sidebar, choose Sleep, then Wake.";
const CHECK_EVERY: Duration = Duration::from_secs(60);

/// The sessions this gxserver has put the card on, as (project, session).
static ANNOUNCED: LazyLock<Mutex<HashSet<(String, String)>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn notice() -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        NOTICE_KIND,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Watchdog,
        TITLE,
    )
    .with_detail(DETAIL)
    .with_actions(vec![SessionChatTerminalNoticeAction {
        id: ACTION_RESTART.to_string(),
        label: "Restart".to_string(),
        kind: SessionChatTerminalNoticeActionKind::RestartAgent,
        send: None,
    }])
    .with_input_blocking(false)
}

/// Looks for such sessions now and then every minute until gxserver stops.
pub(crate) fn start_lost_folder_access_watch(state: Arc<AppState>) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(CHECK_EVERY);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let checking = state.clone();
            let changed = tokio::task::spawn_blocking(move || check(&checking))
                .await
                .unwrap_or_default();
            for (project_id, session_id) in changed {
                crate::session_chat_options::session_chat_terminal_notice_publisher(
                    &state,
                    &project_id,
                    &session_id,
                )();
            }
        }
    });
}

/// Puts the card on newly found sessions and takes it off recovered ones; returns the sessions whose card changed. BLOCKING.
fn check(state: &AppState) -> Vec<(String, String)> {
    let Some(lost) = lost_daemons(&format!("{}-", state.metadata.server_id)) else {
        return Vec::new();
    };
    let announced = ANNOUNCED
        .lock()
        .map(|announced| announced.clone())
        .unwrap_or_default();
    if lost.is_empty() && announced.is_empty() {
        return Vec::new();
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(sessions) = repository.list_sessions(None) else {
        return Vec::new();
    };
    let text =
        |session: &Value, key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    let mut still_lost = HashSet::new();
    let mut changed = Vec::new();
    for session in &sessions {
        let (Some(zmx_name), Some(project_id), Some(session_id)) = (
            text(session, "zmxName"),
            text(session, "projectId"),
            text(session, "sessionId"),
        ) else {
            continue;
        };
        if !lost.contains(&zmx_name) {
            continue;
        }
        let key = (project_id, session_id);
        if !announced.contains(&key) {
            crate::session_chat_notice::set_session_chat_watchdog_notice(&key.0, &key.1, notice());
            print_into_terminal(&zmx_name);
            changed.push(key.clone());
        }
        still_lost.insert(key);
    }
    for key in announced.difference(&still_lost) {
        clear_card(&key.0, &key.1);
        changed.push(key.clone());
    }
    if let Ok(mut announced) = ANNOUNCED.lock() {
        *announced = still_lost;
    }
    changed
}

/// The zmx names of this server's daemons whose program file is gone, or None when the process list can't be read.
#[cfg(unix)]
fn lost_daemons(prefix: &str) -> Option<HashSet<String>> {
    let ps_output = super::wire_cycle::read_process_snapshot().ok()?;
    Some(
        super::process_identity::zmx_daemon_processes(&ps_output)
            .into_iter()
            .filter(|(pid, name)| name.starts_with(prefix) && program_is_gone(*pid))
            .map(|(_, name)| name)
            .collect(),
    )
}

/// Windows has no such daemons: the watch never starts there, and this keeps it compiling.
#[cfg(not(unix))]
fn lost_daemons(_prefix: &str) -> Option<HashSet<String>> {
    None
}

/// Whether the program a process was started from has been deleted (macOS answers ENOENT for it, ESRCH for a process that ended).
#[cfg(target_os = "macos")]
fn program_is_gone(pid: i64) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length =
        unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    length <= 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn program_is_gone(_pid: i64) -> bool {
    false
}

/// One line on the terminal's screen, where the session shows no card.
fn print_into_terminal(zmx_name: &str) {
    let Ok(zmx) = crate::toolchain::require_bundled_zmx() else {
        return;
    };
    let script = format!(
        "unset ZMX_SESSION ZMX_SESSION_PREFIX\nexec {} print {} {}",
        crate::platform::shell::shell_quote(&zmx.executable_path),
        crate::platform::shell::shell_quote(zmx_name),
        crate::platform::shell::shell_quote(&format!("\r\n\x1b[33m{TERMINAL_LINE}\x1b[0m\r\n")),
    );
    let _ = super::launch::run_zmx_interaction_command(script, Default::default());
}

/// Takes this card off a session, leaving any other card it shows.
fn clear_card(project_id: &str, session_id: &str) {
    if crate::session_chat_notice::session_chat_watchdog_notice(project_id, session_id)
        .is_some_and(|notice| notice.kind == NOTICE_KIND)
    {
        crate::session_chat_notice::clear_session_chat_watchdog_notice(project_id, session_id);
    }
}

/// The card's Restart: the sidebar's Sleep then Wake, whose new daemon runs from a program that is there.
pub(crate) async fn restart(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let state = state.clone();
    let params = params.clone();
    tokio::task::spawn_blocking(move || {
        let target = crate::session_chat_send::resolve_session_chat_send_target(
            &state,
            &params,
            "restartAgent",
        )?;
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "invalidState",
            message: error.to_string(),
        })?;
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        let session = repository
            .get_session(&target.project_id, &target.session_id)?
            .ok_or_else(|| DomainStateError {
                code: "invalidState",
                message: "This session no longer exists.".to_string(),
            })?;
        crate::accounts::endpoint::cycle(&state, &repository, &session, "/api/sleepSession")?;
        crate::accounts::endpoint::cycle(&state, &repository, &session, "/api/wakeSession")?;
        clear_card(&target.project_id, &target.session_id);
        if let Ok(mut announced) = ANNOUNCED.lock() {
            announced.remove(&(target.project_id.clone(), target.session_id.clone()));
        }
        crate::session_chat_options::session_chat_terminal_notice_publisher(
            &state,
            &target.project_id,
            &target.session_id,
        )();
        Ok(())
    })
    .await
    .map_err(|error| DomainStateError {
        code: "invalidState",
        message: error.to_string(),
    })?
}
