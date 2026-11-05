//! CDXC:Sessions 2026-11-05 WHY:
//! Issue #426: a conversation started in the OpenCode CLI never reached Quick Access > Sessions >
//! External. OpenCode 2 keeps conversations in its service's database (shared with OpenCode 1's
//! tables and migrated by OpenCode itself), not one transcript file each, so they are read
//! through its public API like the chat does: the running background service when it answers,
//! else `opencode api --standalone`, a private server that exits with the command (as the model
//! lineup reads it, agent_model_lineups.rs), so discovery never leaves an OpenCode process
//! running. Only top-level, unarchived conversations that got at least one reply are listed; the
//! empty ones every TUI start creates are not. Continuing one runs `opencode --session <id>`.

use super::Conversation;
use serde_json::Value;
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// `GET /api/session` answers at most 50 per page; 20 pages keep a scan bounded.
const PAGE_LIMIT: usize = 50;
const MAX_PAGES: usize = 20;

pub(super) fn read_opencode_conversations(home: &Path) -> Vec<Conversation> {
    let service = crate::session_chat_opencode::Client::discover()
        .ok()
        .filter(|client| client.request("GET", "/api/info", None).is_ok());
    let program = match service {
        Some(_) => None,
        None => match crate::agent_cli::opencode::opencode2_program(home) {
            Some(program) => Some(program),
            None => return Vec::new(),
        },
    };
    let page = |cursor: Option<&str>| -> Option<Value> {
        match (&service, &program) {
            (Some(client), _) => {
                let mut path = format!("/api/session?limit={PAGE_LIMIT}");
                if let Some(cursor) = cursor {
                    let encoded: String =
                        url::form_urlencoded::byte_serialize(cursor.as_bytes()).collect();
                    path.push_str(&format!("&cursor={encoded}"));
                }
                client.request("GET", &path, None).ok()
            }
            (None, Some(program)) => standalone_page(program, home, cursor),
            (None, None) => None,
        }
    };
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("opencode");
    let mut conversations = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..MAX_PAGES {
        let Some(response) = page(cursor.as_deref()) else {
            break;
        };
        for row in response["data"].as_array().into_iter().flatten() {
            if let Some(conversation) = conversation(row, &data_home) {
                conversations.push(conversation);
            }
        }
        match response["cursor"]["next"].as_str() {
            Some(next) if seen.insert(next.to_string()) => cursor = Some(next.to_string()),
            _ => break,
        }
    }
    conversations
}

fn conversation(row: &Value, data_home: &Path) -> Option<Conversation> {
    let id = row["id"].as_str()?;
    if !crate::session_chat_opencode::safe_id(id)
        || row["parentID"].is_string()
        || row["time"]["archived"].is_number()
    {
        return None;
    }
    let tokens = &row["tokens"];
    if tokens["input"].as_u64().unwrap_or(0) == 0 && tokens["output"].as_u64().unwrap_or(0) == 0 {
        return None;
    }
    let cwd = row["location"]["directory"].as_str()?;
    if !Path::new(cwd).is_absolute() {
        return None;
    }
    let updated = row["time"]["updated"]
        .as_i64()
        .and_then(chrono::DateTime::from_timestamp_millis)?;
    let title = row["title"].as_str().unwrap_or_default().trim();
    let title = if title.is_empty() {
        format!(
            "OpenCode conversation {}",
            id.chars().take(12).collect::<String>()
        )
    } else {
        title.chars().take(180).collect()
    };
    Some(Conversation {
        agent: "opencode".to_string(),
        id: id.to_string(),
        cwd: cwd.to_string(),
        title,
        path: crate::session_chat_opencode::mirror_path(id),
        agent_home: data_home.to_path_buf(),
        updated: updated.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    })
}

fn standalone_page(program: &Path, home: &Path, cursor: Option<&str>) -> Option<Value> {
    let mut command = Command::new(program);
    command
        .args(["api", "--standalone", "GET", "/api/session", "--param"])
        .arg(format!("limit={PAGE_LIMIT}"));
    if let Some(cursor) = cursor {
        command.arg("--param").arg(format!("cursor={cursor}"));
    }
    command
        .current_dir(home)
        .env(
            "PATH",
            crate::agent_hooks::probing::normalize_gxserver_process_path(
                std::env::var("PATH").ok().as_deref(),
                home,
            ),
        )
        // The Ghostex plugin only reports for a terminal Ghostex started.
        .env_remove("GHOSTEX_SESSION_ID")
        .env("NO_COLOR", "1");
    crate::platform::process::NoConsoleWindow::no_console_window(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(15) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let text = reader.join().ok()?;
    status
        .success()
        .then(|| serde_json::from_str(&text).ok())
        .flatten()
}
