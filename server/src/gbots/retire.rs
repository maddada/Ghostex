//! The one-time clean-up of the previous GBots on each computer, run once at gxserver start:
//! stops its leftover background server, removes its bot projects and their chats, deletes its
//! downloaded runtime, and moves its old folder (the bots' instructions, notes and chat copies) to
//! the Trash. Done once; `gbotsSidecarRetired` in `metadata` records it.
//!
//! CDXC:GBots 2026-11-08 DECISION:
//! User: remove the previous GBots fully, "we are doing our own bots thing." The previous GBots' bots and chats are not imported into native GBots; they are gone. Its folder goes to the Trash, not deleted, so anyone who wants an old bot's text can still get it by hand. A step that fails (the folder can't reach the Trash) leaves the record unset, so the next start tries again; every step is safe to repeat. The previous GBots' own settings left in settings files are not touched: nothing reads them.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::OptionalExtension;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::server::{schedule_presentation_project_delta, AppState};
use crate::storage::open_gxserver_database;

const DONE_KEY: &str = "gbotsSidecarRetired";
/// The agent id of the previous GBots' chat sessions.
const OLD_AGENT_ID: &str = "gbots";
/// The `launchSettings` key that marked a previous GBots bot project.
const OLD_BOT_PROJECT_KEY: &str = "gbotId";
/// The name the old folder takes in the Trash.
const TRASH_NAME: &str = "GBots (previous)";

/// Starts the clean-up on a blocking thread; a computer that never had the previous GBots only
/// records that it is done.
pub(crate) fn retire_previous_gbots(state: Arc<AppState>) {
    tokio::task::spawn_blocking(move || {
        if let Err(error) = retire(&state) {
            log(&state, LogLevel::Warn, "gbots.retireFailed", error);
        }
    });
}

fn retire(state: &AppState) -> Result<(), String> {
    let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
    let done: Option<String> = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [DONE_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if done.is_some() {
        return Ok(());
    }
    let old_folder = ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("gbots");
    stop_leftover_server(&old_folder);
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let removed = remove_bots_and_chats(state, &db, &repository).map_err(|error| error.message)?;
    let runtime = crate::components::component_root("gbots");
    if runtime.exists() {
        std::fs::remove_dir_all(&runtime)
            .map_err(|error| format!("could not delete {}: {error}", runtime.display()))?;
    }
    if old_folder.exists() {
        // The wrappers that ran the bots' Claude Code and Codex on saved accounts are deleted, not
        // kept in the Trash: they hold nothing of the user's.
        let _ = std::fs::remove_dir_all(old_folder.join("bin"));
        move_to_trash(&state.paths.home_dir, &old_folder)?;
    }
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![
            DONE_KEY,
            json!({ "projects": removed.0, "sessions": removed.1 }).to_string(),
            crate::domain::now_iso()
        ],
    )
    .map_err(|error| error.to_string())?;
    if removed != (0, 0) {
        log(
            state,
            LogLevel::Info,
            "gbots.retired",
            format!(
                "removed {} previous GBots bot projects and {} chats",
                removed.0, removed.1
            ),
        );
    }
    Ok(())
}

/// Ends the previous GBots' background server a gxserver from before this release left running.
/// Its pid record is checked against the process's command line, so a reused pid is never hit.
fn stop_leftover_server(old_folder: &Path) {
    let pid = std::fs::read_to_string(old_folder.join("sidecar.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|record| record.get("pid").and_then(Value::as_u64))
        .and_then(|pid| u32::try_from(pid).ok());
    let Some(pid) = pid else {
        return;
    };
    if crate::runtime::is_process_running(pid) && runs_old_server(pid, old_folder) {
        signal_group(pid, false);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && crate::runtime::is_process_running(pid) {
            std::thread::sleep(Duration::from_millis(100));
        }
        if crate::runtime::is_process_running(pid) {
            signal_group(pid, true);
        }
    }
    let _ = std::fs::remove_file(old_folder.join("sidecar.json"));
}

/// The process was started with the old folder's data directory (`serve … --data-dir <folder>/data`).
#[cfg(unix)]
fn runs_old_server(pid: u32, old_folder: &Path) -> bool {
    let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .output()
    else {
        return false;
    };
    let command = String::from_utf8_lossy(&output.stdout);
    command.contains(" serve ") && command.contains(&*old_folder.join("data").to_string_lossy())
}

#[cfg(not(unix))]
fn runs_old_server(_pid: u32, _old_folder: &Path) -> bool {
    false
}

/// Signals the process group the old server led, so the agent turns it started stop with it.
#[cfg(unix)]
fn signal_group(pid: u32, kill: bool) {
    if let Ok(pid) = i32::try_from(pid) {
        // SAFETY: signalling a process group whose leader's command line was checked above.
        unsafe {
            libc::kill(-pid, if kill { libc::SIGKILL } else { libc::SIGTERM });
        }
    }
}

#[cfg(not(unix))]
fn signal_group(_pid: u32, _kill: bool) {}

/// Removes the previous GBots' chat sessions (with their terminals) and bot projects; answers
/// how many projects and sessions went.
fn remove_bots_and_chats(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
) -> Result<(usize, usize), DomainStateError> {
    let bot_projects: Vec<String> = repository
        .list_projects()?
        .iter()
        .filter(|project| {
            project
                .pointer(&format!("/launchSettings/{OLD_BOT_PROJECT_KEY}"))
                .is_some_and(Value::is_string)
        })
        .map(|project| text(project, "projectId"))
        .collect();
    let sessions: Vec<Value> = repository
        .list_sessions(None)?
        .into_iter()
        .filter(|session| {
            text(session, "agentId") == OLD_AGENT_ID
                || bot_projects.contains(&text(session, "projectId"))
        })
        .collect();
    let zmx = crate::toolchain::require_bundled_zmx().ok();
    for session in &sessions {
        if let (Some(zmx), Some(name)) = (&zmx, session.get("zmxName").and_then(Value::as_str)) {
            crate::zmx::kill_zmx_session(name, &zmx.executable_path);
        }
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(text(session, "projectId")));
        params.insert("sessionId".to_string(), json!(text(session, "sessionId")));
        crate::server::remove_session_row(state, db, repository, &params)?;
    }
    for project_id in &bot_projects {
        repository.remove_project(project_id)?;
        crate::session_chat_queue::forget_session_chat_queue(db, project_id, None)?;
        schedule_presentation_project_delta(state, db, repository, project_id, "projectUpdated")?;
    }
    Ok((bot_projects.len(), sessions.len()))
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A name in `dir` not taken yet: `GBots (previous)`, then `GBots (previous) 2`, …
fn free_trash_name(dir: &Path) -> String {
    (1..)
        .map(|n| {
            if n == 1 {
                TRASH_NAME.to_string()
            } else {
                format!("{TRASH_NAME} {n}")
            }
        })
        .find(|name| !dir.join(name).exists())
        .unwrap_or_else(|| TRASH_NAME.to_string())
}

/// Moves a folder to the user's Trash (macOS `~/.Trash`).
#[cfg(target_os = "macos")]
fn move_to_trash(home: &Path, folder: &Path) -> Result<(), String> {
    let trash = home.join(".Trash");
    std::fs::create_dir_all(&trash).map_err(|error| error.to_string())?;
    let target = trash.join(free_trash_name(&trash));
    std::fs::rename(folder, &target)
        .map_err(|error| format!("could not move {} to the Trash: {error}", folder.display()))
}

/// Moves a folder to the user's Trash (the freedesktop.org Trash, with its `.trashinfo`, so file
/// managers can put it back).
#[cfg(all(unix, not(target_os = "macos")))]
fn move_to_trash(home: &Path, folder: &Path) -> Result<(), String> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local").join("share"));
    let trash = data_home.join("Trash");
    let (files, info) = (trash.join("files"), trash.join("info"));
    std::fs::create_dir_all(&files).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&info).map_err(|error| error.to_string())?;
    let name = free_trash_name(&files);
    let deleted_at = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    std::fs::write(
        info.join(format!("{name}.trashinfo")),
        format!(
            "[Trash Info]\nPath={}\nDeletionDate={deleted_at}\n",
            folder.display()
        ),
    )
    .map_err(|error| error.to_string())?;
    std::fs::rename(folder, files.join(&name)).map_err(|error| {
        let _ = std::fs::remove_file(info.join(format!("{name}.trashinfo")));
        format!("could not move {} to the Trash: {error}", folder.display())
    })
}

/// Moves a folder to the Recycle Bin.
#[cfg(windows)]
fn move_to_trash(_home: &Path, folder: &Path) -> Result<(), String> {
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($env:GHOSTEX_TRASH_PATH, 'OnlyErrorDialogs', 'SendToRecycleBin')",
        ])
        .env("GHOSTEX_TRASH_PATH", folder)
        .stdin(std::process::Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "could not move {} to the Recycle Bin",
            folder.display()
        ))
    }
}

fn log(state: &AppState, level: LogLevel, event: &str, message: String) {
    let _ = GxserverLogger::new(state.paths.clone()).log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message),
        details: None,
    });
}
