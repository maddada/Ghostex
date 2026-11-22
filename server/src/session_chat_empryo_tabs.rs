//! CDXC:SessionChat 2026-10-07 WHY:
//! Right after a window joins, Ghostex makes Empryo show this session's own tab, and a send never lands in another session's tab: it is held until the right tab shows (the Empryo build coordinator's call for Sven).
//!
//! CDXC:SessionChat 2026-10-07 WHY:
//! Empryo 3.9.1-beta runs one engine per repository for every window in it. A window that starts while the engine is up joins it, holds the engine's tabs plus its own, and opens on `tabs.find(own) ?? tabs[0]` (its engine boot), so the coordinator's `/agent` line and chat messages typed there went into another session's conversation (seen live 2026-10-07). A window never learns of tabs added after it joined, so every bar is a prefix of the engine's tab order, and `.empryo/tabs.json` (the last window to change tabs writes its own bar there once it holds two or more) or the engine's own session `meta.json` gives a session's own tab (the first tab of its own folder) its place in any bar that holds it; Empryo's Ctrl+] / Ctrl+\ step to it. The tab's turns are then logged in the engine's session (session_chat_empryo_mirror.rs follows them there).
//! SEE-ALSO: server/src/session_chat_composer_input.rs `empryo_tab_bar`, server/src/session_chat_send/steps.rs (`SelectEmpryoTab`), server/src/zmx/provider.rs (the start that selects it), proxysoul/Empryo#236.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::domain::DomainRepository;
use crate::session_chat_send::{
    capture_session_terminal_text_vt, execute_session_chat_send, write_session_chat_payload,
    SessionChatSendStep,
};

/// Empryo's next-tab and previous-tab keys (Ctrl+] and Ctrl+\).
const EMPRYO_NEXT_TAB: &str = "\u{1d}";
const EMPRYO_PREVIOUS_TAB: &str = "\u{1c}";
/// How long one press may take to repaint the bar.
const EMPRYO_TAB_POLL: Duration = Duration::from_millis(150);
/// How long one send attempt waits for its window to show its own tab; a queued message is then
/// held and retried like one whose input box is not up yet.
pub(crate) const EMPRYO_SEND_TAB_WAIT_MS: u64 = 5_000;
/// A started window draws its input box before it has joined the engine and named its tab in
/// `tabs.json`, so the selection after a start waits longer.
const EMPRYO_START_TAB_WAIT_MS: u64 = 30_000;
/// How long a started Empryo may take to draw its input box, polled every half second.
const EMPRYO_START_WAIT: Duration = Duration::from_secs(60);
const EMPRYO_START_POLL: Duration = Duration::from_millis(500);
/// How long a window that joined another engine may show its input box without a tab bar: it
/// draws the bar a moment after the box, so one still missing then never opened its own tab.
const EMPRYO_JOIN_BAR_GRACE: Duration = Duration::from_secs(5);

/// The window shows only another session's tab and does not hold its own, so no key reaches it.
const EMPRYO_TAB_MISSING_MESSAGE: &str = "Empryo opened this window on another session's tab without this session's own, so nothing was sent. Sleep and wake the session to reopen its tab.";
/// The window holds the session's tab, but stepping the bar to it did not take.
const EMPRYO_TAB_NOT_SELECTED_MESSAGE: &str =
    "Empryo did not switch to this session's tab, so nothing was sent.";

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn tab_id(tab: &Value) -> Option<&str> {
    tab.get("id").and_then(Value::as_str)
}

/// The tabs a session folder's `meta.json` lists, in order.
fn meta_tabs(folder: &Path) -> Vec<Value> {
    match read_json(&folder.join("meta.json"))
        .and_then(|mut meta| meta.get_mut("tabs").map(Value::take))
    {
        Some(Value::Array(tabs)) => tabs,
        _ => Vec::new(),
    }
}

/// The tab ids a session folder's `meta.json` lists, in order.
fn tab_ids(folder: &Path) -> Vec<String> {
    meta_tabs(folder)
        .iter()
        .filter_map(|tab| tab_id(tab).map(str::to_string))
        .collect()
}

/// A session's own tab: the first tab of its own folder, the one its seed or Empryo wrote.
fn own_tab(session_log: &Path) -> Option<String> {
    tab_ids(session_log.parent()?).into_iter().next()
}

/// The place of `tab` in the bar of any window that holds it: its place among the tabs of the
/// engine's own session, or else in `.empryo/tabs.json`. `None` while neither names it.
fn tab_position(session_log: &Path, tab: &str) -> Option<usize> {
    let position = |ids: Vec<String>| ids.iter().position(|id| id == tab);
    engine_folder(session_log)
        .and_then(|folder| position(tab_ids(&folder)))
        .or_else(|| position(bar_ids(session_log)?))
}

/// `.empryo/tabs.json` beside a session folder: the tab ids of the last window that changed tabs,
/// in its bar order.
fn tabs_json(session_log: &Path) -> Option<PathBuf> {
    Some(session_log.parent()?.parent()?.parent()?.join("tabs.json"))
}

fn bar_ids(session_log: &Path) -> Option<Vec<String>> {
    Some(
        read_json(&tabs_json(session_log)?)?
            .as_array()?
            .iter()
            .filter_map(|tab| tab_id(tab).map(str::to_string))
            .collect(),
    )
}

/// CDXC:SessionChat 2026-10-07 WHY:
/// A running engine holds `writer.lock` (its pid) in the folder of the session it was started for, and logs every tab there. When that is the session's own folder, a window without a tab bar shows the session's own tab: it started the engine, or rejoined the engine an earlier window of the session started (an engine outlives its window by ten idle minutes, so a session woken soon after it slept joins it, says "joined a running engine" and draws no bar). Reading the engine's process off the window's process tree missed that rejoin and held every send to it (seen live 2026-10-07).
fn hosts_engine(folder: &Path) -> bool {
    read_json(&folder.join("writer.lock"))
        .and_then(|lock| lock.get("pid").and_then(Value::as_u64))
        .and_then(|pid| u32::try_from(pid).ok())
        .is_some_and(crate::platform::process::process_is_alive)
}

/// [`hosts_engine`] for the folder of the session's own log.
fn hosts_own_engine(session_log: &Path) -> bool {
    session_log.parent().is_some_and(hosts_engine)
}

/// The session folder the running engine of `session_log`'s checkout logs to.
fn engine_folder(session_log: &Path) -> Option<PathBuf> {
    let own = session_log.parent()?;
    if hosts_engine(own) {
        return Some(own.to_path_buf());
    }
    std::fs::read_dir(own.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|folder| hosts_engine(folder))
}

/// The `/agent` profile the engine runs the session's own tab under (`Some(None)` without one),
/// from the `meta.json` of the engine's session, or of the session's own folder while no engine
/// runs. `None` while neither lists the tab.
pub(crate) fn empryo_tab_agent(session_log: &Path) -> Option<Option<String>> {
    let own = session_log.parent()?;
    let tab = own_tab(session_log)?;
    engine_folder(session_log)
        .filter(|folder| folder != own)
        .into_iter()
        .chain([own.to_path_buf()])
        .find_map(|folder| {
            let tabs = meta_tabs(&folder);
            let entry = tabs
                .iter()
                .find(|entry| tab_id(entry) == Some(tab.as_str()))?;
            Some(
                entry
                    .get("agent")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            )
        })
}

/// Whether the window's header (its top rows) says it joined an engine it did not start.
fn shows_joined_engine(screen: &str) -> bool {
    crate::session_chat_agent_fleet::normalized_screen_lines(screen)
        .iter()
        .take(3)
        .any(|line| line.contains("joined a running engine") || line.contains("joined \u{b7} "))
}

/// The `session.jsonl` of a Ghostex session's Empryo, read from the state database.
fn empryo_session_log_for(project_id: &str, session_id: &str) -> Option<PathBuf> {
    let paths = crate::paths::get_gxserver_paths(None);
    let db = crate::session_chat_hermes::open_read_only_state_db(&paths.state_db_file)?;
    let repository = DomainRepository::new(&db, "");
    let session = repository.get_session(project_id, session_id).ok()??;
    crate::session_chat_pi_models::empryo_session_log(&repository, &session)
}

/// Makes the window show the session's own tab, stepping through the bar one tab at a time and
/// reading it back after each press. Errors when the bar never confirms the tab within `wait_ms`,
/// so the send that asked for it is held instead of typed into another session's conversation.
pub(crate) async fn select_empryo_own_tab(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    wait_ms: u64,
) -> Result<(), String> {
    let Some((log, tab)) = empryo_session_log_for(project_id, session_id)
        .and_then(|log| own_tab(&log).map(|tab| (log, tab)))
    else {
        return Ok(());
    };
    let deadline = Instant::now() + Duration::from_millis(wait_ms);
    let mut held = EMPRYO_TAB_MISSING_MESSAGE;
    let mut nudged = false;
    loop {
        let bar = capture_session_terminal_text_vt(zmx_name)
            .await
            .map(|screen| crate::session_chat_composer::empryo_tab_bar(&screen));
        match bar {
            // No tab bar: the window holds one tab, the session's own when its folder hosts the
            // engine. Otherwise it shows the one tab of another session's engine, or has not
            // joined its own yet, and the send waits.
            Some(None) => {
                if hosts_own_engine(&log) {
                    return Ok(());
                }
            }
            Some(Some((shown, Some(active)))) => {
                match tab_position(&log, &tab).filter(|target| *target < shown) {
                    Some(target) => {
                        held = EMPRYO_TAB_NOT_SELECTED_MESSAGE;
                        let key = match target.cmp(&active) {
                            Ordering::Equal => return Ok(()),
                            Ordering::Greater => EMPRYO_NEXT_TAB,
                            Ordering::Less => EMPRYO_PREVIOUS_TAB,
                        };
                        write_session_chat_payload(project_id, session_id, zmx_name, source, key)
                            .await?;
                    }
                    // CDXC:SessionChat 2026-10-07 WHY: a window that joined the engine holds its own tab in its bar, but neither the engine's `meta.json` nor `.empryo/tabs.json` lists that tab until it runs a turn or a window changes tabs (seen live 2026-10-07: a fork beside its open source session held every send, so its first turn could never start). One step through the bar makes the window write its bar to `tabs.json`, which places the tab; only the shown tab changes, nothing is typed into a conversation.
                    None if shown >= 2 && !nudged => {
                        nudged = true;
                        write_session_chat_payload(
                            project_id,
                            session_id,
                            zmx_name,
                            source,
                            EMPRYO_NEXT_TAB,
                        )
                        .await?;
                    }
                    None => {}
                }
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            return Err(held.to_string());
        }
        tokio::time::sleep(EMPRYO_TAB_POLL).await;
    }
}

/// After an Empryo session is created, or a resume, fork or wake starts its terminal (the desktop
/// app starts a new session's terminal itself, so creation counts as a start): waits for Empryo's
/// input box, then runs the tab selection in the session's send queue, ahead of whatever the chat
/// sends next, so the window shows this session's own conversation as soon as it is up.
///
/// CDXC:SessionChat 2026-10-07 WHY:
/// Empryo 3.9.1-beta sometimes joins a window to the running engine without opening the window's own tab (two threads started a second apart; seen live 2026-10-07): the window shows only the engine's first tab, so its brief went into the coordinator's conversation. Such a window says it joined a running engine, has no tab bar, and its folder does not host the engine; with `state` (a create) it is restarted once through sleep and wake, the cycle the sidebar uses, and the wake selects the tab again. Without one its sends stay held. A window that has not said it joined is still starting its own engine and is waited for, never restarted.
pub(crate) fn select_empryo_own_tab_after_start(
    session: &Value,
    state: Option<crate::server::AppState>,
) {
    if crate::session_chat_follower::session_chat_agent_for_session(session).as_deref()
        != Some("empryo")
    {
        return;
    }
    let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    let (Some(project_id), Some(session_id), Ok(runtime)) = (
        text("projectId"),
        text("sessionId"),
        tokio::runtime::Handle::try_current(),
    ) else {
        return;
    };
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(session) else {
        return;
    };
    runtime.spawn(async move {
        let deadline = Instant::now() + EMPRYO_START_WAIT;
        let mut box_since: Option<Instant> = None;
        let mut log: Option<PathBuf> = None;
        while Instant::now() < deadline {
            let screen = capture_session_terminal_text_vt(&zmx_name).await;
            let input_box = screen.as_deref().is_some_and(|screen| {
                crate::session_chat_composer::empryo_composer_busy(screen).is_some()
            });
            if !input_box {
                box_since = None;
                tokio::time::sleep(EMPRYO_START_POLL).await;
                continue;
            }
            let since = *box_since.get_or_insert_with(Instant::now);
            let bar = screen.as_deref().is_some_and(|screen| {
                crate::session_chat_composer::empryo_tab_bar(screen).is_some()
            });
            if !bar && log.is_none() {
                log = empryo_session_log_for(&project_id, &session_id);
            }
            if bar || log.as_deref().is_none_or(hosts_own_engine) {
                if let Err(error) = execute_session_chat_send(
                    &project_id,
                    &session_id,
                    &zmx_name,
                    "session-chat-empryo-tab",
                    vec![SessionChatSendStep::SelectEmpryoTab {
                        wait_ms: EMPRYO_START_TAB_WAIT_MS,
                    }],
                )
                .await
                {
                    record(
                        &project_id,
                        &session_id,
                        "sessionChatEmpryoTabNotSelected",
                        &error.message,
                    );
                }
                return;
            }
            if since.elapsed() >= EMPRYO_JOIN_BAR_GRACE
                && screen.as_deref().is_some_and(shows_joined_engine)
            {
                let Some(state) = state else {
                    record(
                        &project_id,
                        &session_id,
                        "sessionChatEmpryoTabMissing",
                        EMPRYO_TAB_MISSING_MESSAGE,
                    );
                    return;
                };
                record(
                    &project_id,
                    &session_id,
                    "sessionChatEmpryoTabRejoin",
                    "Empryo joined the running engine without this session's tab; restarting it.",
                );
                let restarted = restart_session(state, &project_id, &session_id).await;
                if let Err(reason) = restarted {
                    record(
                        &project_id,
                        &session_id,
                        "sessionChatEmpryoTabMissing",
                        &reason,
                    );
                }
                return;
            }
            tokio::time::sleep(EMPRYO_START_POLL).await;
        }
    });
}

/// Puts the session to sleep and wakes it, the cycle the sidebar uses, so its Empryo starts again.
async fn restart_session(
    state: crate::server::AppState,
    project_id: &str,
    session_id: &str,
) -> Result<(), String> {
    let (project_id, session_id) = (project_id.to_string(), session_id.to_string());
    tokio::task::spawn_blocking(move || {
        let db = crate::storage::open_gxserver_database(&state.paths)
            .map_err(|error| error.to_string())?;
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        let row = repository
            .get_session(&project_id, &session_id)
            .map_err(|error| error.message)?
            .ok_or_else(|| "The session no longer exists.".to_string())?;
        for path in ["/api/sleepSession", "/api/wakeSession"] {
            crate::accounts::endpoint::cycle(&state, &repository, &row, path)
                .map_err(|error| error.message)?;
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn record(project_id: &str, session_id: &str, event: &str, reason: &str) {
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        event,
        project_id,
        session_id,
        reason,
        &[],
    );
}

/// What a tab scan depends on: the sessions folder's and `tabs.json`'s modification times. A new
/// session folder, or a window joining or switching tabs, changes one of them.
pub(crate) fn empryo_tab_scan_key(
    session_log: &Path,
) -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>) {
    let modified = |path: Option<&Path>| path?.metadata().ok()?.modified().ok();
    (
        modified(session_log.parent().and_then(Path::parent)),
        modified(tabs_json(session_log).as_deref()),
    )
}

/// One pass over the session folders beside `session_log`'s: the `session.jsonl` that logs this
/// session's own tab now, and the tabs other Ghostex sessions own (every other folder's first tab),
/// which that log can hold too. While `.empryo/tabs.json` lists the tab, the log is the one of the
/// engine's own window, whose tab the list starts with (a joined window's tab reaches that
/// session's `meta.json` only minutes later); otherwise it is the most recently written log among
/// the folders that list the tab.
pub(crate) fn empryo_tab_scan(session_log: &Path) -> Option<(PathBuf, HashSet<String>)> {
    let own = session_log.parent()?;
    let tab = own_tab(session_log)?;
    let engine_tab = bar_ids(session_log)
        .filter(|ids| ids.contains(&tab))
        .and_then(|ids| ids.into_iter().next());
    let mut engine_log = None;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    let mut foreign = HashSet::new();
    for folder in std::fs::read_dir(own.parent()?)
        .ok()?
        .filter_map(Result::ok)
    {
        let folder = folder.path();
        let tabs = tab_ids(&folder);
        let log = folder.join("session.jsonl");
        if engine_tab.is_some() && tabs.first() == engine_tab.as_ref() {
            engine_log = Some(log.clone());
        }
        if folder != own {
            foreign.extend(tabs.first().cloned());
        }
        let modified = log.metadata().and_then(|meta| meta.modified());
        if let (true, Ok(modified)) = (tabs.contains(&tab), modified) {
            if newest.as_ref().is_none_or(|(time, _)| modified > *time) {
                newest = Some((modified, log));
            }
        }
    }
    let host = engine_log
        .or(newest.map(|(_, log)| log))
        .unwrap_or_else(|| session_log.to_path_buf());
    Some((host, foreign))
}
