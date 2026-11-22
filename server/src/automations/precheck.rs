//! The cheap check a routine runs as plain code before any model turn: new mail, an upcoming
//! calendar event, a command's output, a web page, a folder. Each answers a fingerprint of what it
//! saw; a run starts only when the fingerprint differs from the last one.
//!
//! | kind | fields | changed when |
//! | --- | --- | --- |
//! | `command` | `command` | its exit code or output changes |
//! | `url` | `url` (https, or http on this computer) | the page's body changes |
//! | `files` | `path` | a file in the folder is added, removed or saved |
//! | `mail` | `workspace?`, `query?` | Gmail returns a message it did not before |
//! | `calendar` | `workspace?`, `withinHours?` or `withinMinutes?` | an event enters the next `withinHours` (4) hours, or `withinMinutes` |
//! | `connector` | `tool`, `arguments?`, `app?`, `workspace?` | a read-only connector tool returns an id it did not before (or, with no ids, its answer changes) |
//! | `any` | `checks` (2 to 4 of the kinds above) | any of them sees something new |
//!
//! CDXC:GBots 2026-11-09 WHY:
//! A routine that watches something must cost nothing while nothing happens, so the check runs in gxserver with no model: only a changed fingerprint starts a GBot's turn. What a check read never reaches the model, only that it changed (the plan's "pre-check output must never reach a model unredacted"), and connector checks go through gxserver's own read-only connector calls, which refuse anything that writes. The first check only records what it sees, so a watcher starts quiet instead of running on everything that was already there.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::server::AppState;

/// How long a command or a page may take.
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);
/// The most of a command's output or a page's body a check reads.
const READ_LIMIT: usize = 4 * 1024 * 1024;
/// The most files a folder check walks.
const FILES_LIMIT: usize = 20_000;
/// Composio's read tools for Gmail and Google Calendar (names and arguments checked against the
/// Connectors tool search on 2026-11-09).
pub(crate) const MAIL_TOOL: &str = "GMAIL_FETCH_EMAILS";
pub(crate) const CALENDAR_TOOL: &str = "GOOGLECALENDAR_EVENTS_LIST";
/// A fingerprint that lists what a check saw (hashed ids), so only something new counts.
const SEEN_PREFIX: &str = "seen:";
/// The most ids a fingerprint keeps.
const SEEN_LIMIT: usize = 200;
const ANY_MAX_CHECKS: usize = 4;

fn text<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn hash(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    hex::encode(&hasher.finalize()[..16])
}

/// A pre-check as stored, or the sentence saying what is wrong with it.
pub(crate) fn normalize_precheck(value: &Value) -> Result<Value, String> {
    let object = value
        .as_object()
        .ok_or("A pre-check is a JSON object with a kind: command, url, files, mail, calendar, connector or any.")?;
    let kind = text(object, "kind").unwrap_or_default();
    let workspace = text(object, "workspace").map(str::to_string);
    let mut stored = match kind {
        "command" => {
            let command =
                text(object, "command").ok_or("A command pre-check needs its command.")?;
            json!({ "kind": "command", "command": command })
        }
        "url" => {
            let url = text(object, "url").ok_or("A url pre-check needs its url.")?;
            if !(url.starts_with("https://") || is_local_http(url)) {
                return Err(
                    "A url pre-check reads an https:// page (http:// only on this computer)."
                        .into(),
                );
            }
            json!({ "kind": "url", "url": url })
        }
        "files" => {
            let path = text(object, "path").ok_or("A files pre-check needs the folder's path.")?;
            json!({ "kind": "files", "path": path })
        }
        "mail" => {
            json!({ "kind": "mail", "query": text(object, "query").unwrap_or("is:unread newer_than:2d") })
        }
        "calendar" => match object.get("withinMinutes").and_then(Value::as_u64) {
            Some(minutes) => {
                if !(5..=180).contains(&minutes) {
                    return Err("A calendar pre-check looks 5 to 180 minutes ahead.".into());
                }
                json!({ "kind": "calendar", "withinMinutes": minutes })
            }
            None => {
                let hours = object
                    .get("withinHours")
                    .and_then(Value::as_u64)
                    .unwrap_or(4);
                if !(1..=72).contains(&hours) {
                    return Err("A calendar pre-check looks 1 to 72 hours ahead.".into());
                }
                json!({ "kind": "calendar", "withinHours": hours })
            }
        },
        "any" => {
            let checks = object
                .get("checks")
                .and_then(Value::as_array)
                .filter(|checks| (2..=ANY_MAX_CHECKS).contains(&checks.len()))
                .ok_or("An any pre-check lists 2 to 4 checks in checks.")?;
            let mut stored = Vec::new();
            for check in checks {
                if check.get("kind").and_then(Value::as_str) == Some("any") {
                    return Err("An any pre-check can't hold another any.".into());
                }
                stored.push(normalize_precheck(check)?);
            }
            json!({ "kind": "any", "checks": stored })
        }
        "connector" => {
            let tool =
                text(object, "tool").ok_or("A connector pre-check needs the tool to call.")?;
            let mut stored = json!({
                "kind": "connector",
                "tool": tool,
                "arguments": object.get("arguments").cloned().unwrap_or_else(|| json!({})),
            });
            if let Some(app) = text(object, "app") {
                stored["app"] = json!(app);
            }
            stored
        }
        _ => return Err(
            "Unknown pre-check kind: use command, url, files, mail, calendar, connector or any."
                .into(),
        ),
    };
    if let Some(workspace) = workspace {
        stored["workspace"] = json!(workspace);
    }
    Ok(stored)
}

fn is_local_http(url: &str) -> bool {
    ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

/// What a pre-check watches, in words ("runs `ls ~/inbox`", "new mail in Gmail").
pub(crate) fn precheck_sentence(precheck: &Value) -> String {
    let field = |key: &str| {
        precheck
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    match field("kind") {
        "command" => format!("the output of `{}`", field("command")),
        "url" => format!("the page {}", field("url")),
        "files" => format!("the files in {}", field("path")),
        "mail"
            if precheck.get("query").and_then(Value::as_str)
                == Some("is:important is:unread newer_than:1d") =>
        {
            "new important mail in Gmail".to_string()
        }
        "mail" => "new mail in Gmail".to_string(),
        "calendar" => match precheck.get("withinMinutes").and_then(Value::as_u64) {
            Some(minutes) => {
                format!("events starting in the next {minutes} minutes in Google Calendar")
            }
            None => format!(
                "events in the next {} hours in Google Calendar",
                precheck
                    .get("withinHours")
                    .and_then(Value::as_u64)
                    .unwrap_or(4)
            ),
        },
        "any" => precheck
            .get("checks")
            .and_then(Value::as_array)
            .map(|checks| {
                checks
                    .iter()
                    .map(precheck_sentence)
                    .collect::<Vec<_>>()
                    .join(" or ")
            })
            .unwrap_or_else(|| "a check".to_string()),
        "connector" => format!("what {} returns", field("tool")),
        _ => "a check".to_string(),
    }
}

/// Runs `precheck` (in `folder` for a command) and answers its fingerprint, or why it failed.
pub(crate) fn run_precheck(
    state: Option<&AppState>,
    precheck: &Value,
    folder: &Path,
) -> Result<String, String> {
    let field = |key: &str| {
        precheck
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    match field("kind").as_str() {
        "command" => run_command(&field("command"), folder),
        "url" => read_url(&field("url")),
        "files" => folder_fingerprint(&expand_home(&field("path"))),
        "mail" => connector(
            state,
            precheck,
            MAIL_TOOL,
            json!({ "query": field("query"), "max_results": 25, "ids_only": true }),
            None,
            true,
        ),
        "calendar" => {
            let window = match precheck.get("withinMinutes").and_then(Value::as_i64) {
                Some(minutes) => chrono::Duration::minutes(minutes),
                None => chrono::Duration::hours(
                    precheck
                        .get("withinHours")
                        .and_then(Value::as_i64)
                        .unwrap_or(4),
                ),
            };
            let now = chrono::Utc::now();
            let iso = |at: chrono::DateTime<chrono::Utc>| {
                at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            };
            connector(
                state,
                precheck,
                CALENDAR_TOOL,
                json!({
                    "calendarId": "primary",
                    "timeMin": iso(now),
                    "timeMax": iso(now + window),
                    "singleEvents": true,
                    "maxResults": 50,
                    "fields": "items(id),nextPageToken",
                }),
                None,
                true,
            )
        }
        "connector" => connector(
            state,
            precheck,
            &field("tool"),
            precheck
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({})),
            Some(field("app")).filter(|app| !app.is_empty()),
            false,
        ),
        "any" => {
            let mut seen = Vec::new();
            let checks = precheck
                .get("checks")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for (index, check) in checks.iter().enumerate() {
                let fingerprint = run_precheck(state, check, folder)?;
                match fingerprint.strip_prefix(SEEN_PREFIX) {
                    Some(ids) => seen.extend(
                        ids.split(',')
                            .filter(|id| !id.is_empty())
                            .map(|id| format!("{index}.{id}")),
                    ),
                    None => seen.push(format!("{index}#{fingerprint}")),
                }
            }
            Ok(seen_fingerprint(seen))
        }
        other => Err(format!("Unknown pre-check kind {other}.")),
    }
}

fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/").or((path == "~").then_some("")) {
        Some(rest) => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(rest),
        None => PathBuf::from(path),
    }
}

fn run_command(command: &str, folder: &Path) -> Result<String, String> {
    let (shell, flag) = if cfg!(windows) {
        ("cmd", "/C")
    } else if Path::new("/bin/zsh").exists() {
        ("/bin/zsh", "-lc")
    } else {
        ("/bin/sh", "-lc")
    };
    let mut child = crate::platform::process::background_command(shell)
        .arg(flag)
        .arg(command)
        .current_dir(if folder.is_dir() {
            folder
        } else {
            Path::new("/")
        })
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("The check's command did not start: {error}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("The check's command had no output.")?;
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = (&mut stdout)
            .take(READ_LIMIT as u64)
            .read_to_end(&mut output);
        output
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() > CHECK_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The check's command ran longer than a minute and was stopped.".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let output = reader.join().unwrap_or_default();
    let code = status.code().unwrap_or(-1).to_string();
    Ok(hash(&[code.as_bytes(), &output]))
}

fn read_url(url: &str) -> Result<String, String> {
    let response = crate::http_agent::agent()
        .get(url)
        .header("User-Agent", "Ghostex routine check")
        .config()
        .timeout_global(Some(CHECK_TIMEOUT))
        .build()
        .call()
        .map_err(|error| format!("The page could not be read: {error}"))?;
    let mut body = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(READ_LIMIT as u64)
        .read_to_end(&mut body)
        .map_err(|error| format!("The page could not be read: {error}"))?;
    Ok(hash(&[&body]))
}

fn folder_fingerprint(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|_| format!("{} isn't there.", path.display()))?;
    let stamp = |time: std::io::Result<SystemTime>| {
        time.ok()
            .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|since| since.as_nanos())
            .unwrap_or(0)
    };
    if !meta.is_dir() {
        return Ok(hash(&[
            &stamp(meta.modified()).to_le_bytes(),
            &meta.len().to_le_bytes(),
        ]));
    }
    let (mut count, mut newest) = (0usize, stamp(meta.modified()));
    let mut stack = vec![path.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            count += 1;
            if count > FILES_LIMIT {
                break;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            newest = newest.max(stamp(meta.modified()));
            if meta.is_dir() {
                stack.push(entry.path());
            }
        }
    }
    Ok(hash(&[&count.to_le_bytes(), &newest.to_le_bytes()]))
}

/// The ids in a connector's answer, sorted: a message, event or item the check saw.
fn ids_in(value: &Value, into: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, item) in object {
                let is_id = matches!(
                    key.as_str(),
                    "id" | "messageId" | "message_id" | "threadId" | "eventId" | "iCalUID"
                );
                match item {
                    Value::String(id) if is_id => into.push(id.clone()),
                    Value::Number(id) if is_id => into.push(id.to_string()),
                    _ => ids_in(item, into),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| ids_in(item, into)),
        // A tool's output is often JSON inside a string.
        Value::String(text) if text.trim_start().starts_with(['{', '[']) => {
            if let Ok(inner) = serde_json::from_str::<Value>(text) {
                ids_in(&inner, into);
            }
        }
        _ => {}
    }
}

fn connector(
    state: Option<&AppState>,
    precheck: &Value,
    tool: &str,
    arguments: Value,
    app: Option<String>,
    lists_ids: bool,
) -> Result<String, String> {
    let state = state.ok_or("Ghostex is still starting; the check runs again soon.")?;
    if !crate::connectors::connectors_enabled(&state.paths) {
        return Err(crate::connectors::connectors_off_message(&state.paths));
    }
    let mut params = Map::new();
    if let Some(workspace) = precheck.get("workspace").and_then(Value::as_str) {
        params.insert("workspaceId".into(), json!(workspace));
    }
    params.insert("tool".into(), json!(tool));
    let answer = match app {
        Some(app) => {
            params.insert("app".into(), json!(app));
            params.insert("input".into(), arguments);
            crate::connectors::call_executor_tool(state, &params)
        }
        None => {
            params.insert("arguments".into(), arguments);
            crate::connectors::call_composio_tool(state, &params)
        }
    }
    .map_err(|error| error.message)?;
    let output = answer.get("output").cloned().unwrap_or(Value::Null);
    let mut ids = Vec::new();
    ids_in(&output, &mut ids);
    // Mail and events with nothing in them saw nothing; another tool's answer without ids is
    // compared whole.
    if ids.is_empty() && !lists_ids {
        return Ok(hash(&[output.to_string().as_bytes()]));
    }
    Ok(seen_fingerprint(
        ids.iter()
            .map(|id| hash(&[id.as_bytes()])[..12].to_string())
            .collect(),
    ))
}

/// `seen:` and the sorted ids, so [`something_new`] can tell a new one from one that left.
fn seen_fingerprint(mut ids: Vec<String>) -> String {
    ids.sort();
    ids.dedup();
    ids.truncate(SEEN_LIMIT);
    format!("{SEEN_PREFIX}{}", ids.join(","))
}

/// Whether a check that saw `now` after `previous` saw something new. For checks that list what
/// they saw (mail, events, a connector's ids) only an id that wasn't there before counts: a
/// message read or an event that ended starts nothing. Any other check counts any change.
///
/// CDXC:GBots 2026-11-09 WHY:
/// The day watch looks at important unread mail and meetings about to start every 15 minutes; with a plain hash, reading a message or a meeting ending changed the fingerprint and spent a model run on nothing new.
pub(crate) fn something_new(previous: &str, now: &str) -> bool {
    match (
        previous.strip_prefix(SEEN_PREFIX),
        now.strip_prefix(SEEN_PREFIX),
    ) {
        (Some(before), Some(after)) => {
            let before: std::collections::HashSet<&str> = before.split(',').collect();
            after
                .split(',')
                .filter(|id| !id.is_empty())
                .any(|id| !before.contains(id))
        }
        _ => previous != now,
    }
}
