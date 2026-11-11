//! Files and links: every attachment, file and link a session mentioned, and for a coordinator
//! every one its threads mentioned too, for the chat's Files and links list and
//! `ghostex read-session-files-links`.
//!
//! CDXC:FilesAndLinks 2026-10-12 WHY:
//! gxserver gathers the list because only it can read every session's transcript, an orchestrator's threads included, which a client showing one chat cannot; the rules for what counts are the chat core's own (`packages/gx-chat-core/src/transcript/files_links.rs`), run over each transcript's every row (the detail pager, newest page first, at most `MAX_PAGES` pages). A relative path is made absolute against the folder its own session runs in when that file exists there, so a thread's `src/lib.rs` opens the thread's file from any chat. Each transcript's list is kept until its file changes, so opening the list again, or a coordinator's list after one thread moved, reads only what changed.
//! SEE-ALSO: packages/gx-chat-core/src/composer/files_links.rs (the card), server/src/coordinators/records.rs (`list_threads_for`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use axum::http::StatusCode;
use ghostex_gx_chat_core::transcript::files_links::{self as rules, FilesLinksItem};
use serde_json::{json, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::storage::open_gxserver_database;

/// Transcript rows per page, the detail pager's own cap (`session_chat_history.rs`).
const PAGE_ROWS: usize = 200;
/// Pages read at most per transcript (20,000 rows).
const MAX_PAGES: usize = 100;
/// Transcripts whose lists are kept.
const CACHE_ENTRIES: usize = 512;

/// One session the list covers.
struct Target {
    project_id: String,
    session_id: String,
    title: String,
    own: bool,
    cwd: Option<String>,
    transcript: Option<(crate::session_chat::SessionChatTranscriptAgent, PathBuf)>,
}

/// A transcript's list, valid while the file keeps its length and modification time.
struct CachedList {
    len: u64,
    modified: Option<SystemTime>,
    items: Vec<FilesLinksItem>,
    /// The pictures the agent's tools opened (a Read of a picture, Codex's view image), which the
    /// chat draws as tiles but the list leaves out.
    read_images: Vec<String>,
}

fn cache() -> &'static Mutex<HashMap<(PathBuf, Option<String>), CachedList>> {
    static CACHE: OnceLock<Mutex<HashMap<(PathBuf, Option<String>), CachedList>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn failure(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

/// `/api/readSessionFilesLinks`: `{sessions: [{projectId, sessionId, title, own, items}], generatedAt}`,
/// the session itself first, then its threads with the most recently mentioned first.
pub(crate) async fn handle_read_session_files_links_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    match read_files_links(state, body).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/// Whether `path` (a file path, or a picture's address on this computer's loopback) is one the
/// session's own chat shows: an attachment, a picture or a file it mentions. Ghostex Collab
/// answers a guest's picture reads only for these (server/src/collab_shim/http.rs).
pub(crate) async fn session_shows_path(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    path: &str,
) -> bool {
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    let body = json!({ "params": { "projectId": project_id, "sessionId": session_id } });
    let Ok(result) = read_files_links(state, &body).await else {
        return false;
    };
    let mentions = |value: &Value| -> bool {
        match value {
            Value::String(text) => text == path,
            Value::Object(map) => ["path", "url", "src", "source"]
                .iter()
                .any(|key| map.get(*key).and_then(Value::as_str) == Some(path)),
            _ => false,
        }
    };
    result["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|session| session["own"] == true)
        .flat_map(|session| session["items"].as_array().into_iter().flatten())
        .any(|item| {
            ["path", "url", "key", "detail"]
                .iter()
                .any(|key| item.get(*key).and_then(Value::as_str) == Some(path))
                || item.get("image").is_some_and(mentions)
        })
}

/// Files and links rows compared by their real path at most, so a chat that lists thousands of
/// files cannot make one check walk them all on disk.
const NAMED_FILE_CANONICAL_CHECKS: usize = 2_000;
/// Files a chat was found to name, kept so the Files view's change watch (a `stat` every 400 ms)
/// does not read the transcript again each time: what a chat once linked stays in that chat.
const NAMED_FILE_CACHE_ENTRIES: usize = 4_096;

fn named_files() -> &'static Mutex<std::collections::HashSet<(String, String, String)>> {
    static NAMED: OnceLock<Mutex<std::collections::HashSet<(String, String, String)>>> =
        OnceLock::new();
    NAMED.get_or_init(Default::default)
}

/// Whether the session's own chat names the file at `path` (absolute, or from the home folder):
/// a file or attachment row of its Files and links list, written as `path` (for a Windows path,
/// up to slashes and case: `names_by_text`) or resolving to the same file. The Files view of a
/// remote project opens only these outside the project (CDXC:Docs 2026-11-07 in
/// server/src/project_docs/chat_file.rs).
///
/// `remembered` lets a file the chat once named count without reading the chat again: the Files
/// view's change watch asks `stat` every 400 ms. A read passes `false`, so it asks the chat as it
/// is now, and a file a rewound or edited chat no longer names is not read.
pub(crate) async fn session_names_file(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    path: &str,
    remembered: bool,
) -> bool {
    let wanted = path.trim().to_string();
    if wanted.is_empty() {
        return false;
    }
    let key = (
        project_id.to_string(),
        session_id.to_string(),
        wanted.clone(),
    );
    if remembered && named_files().lock().is_ok_and(|named| named.contains(&key)) {
        return true;
    }
    let found = chat_names_file(state, project_id, session_id, wanted).await;
    if found {
        if let Ok(mut named) = named_files().lock() {
            if named.len() >= NAMED_FILE_CACHE_ENTRIES {
                named.clear();
            }
            named.insert(key);
        }
    }
    found
}

async fn chat_names_file(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    wanted: String,
) -> bool {
    let body = json!({ "params": { "projectId": project_id, "sessionId": session_id } });
    let Ok(result) = read_files_links(state, &body).await else {
        return false;
    };
    let named: Vec<String> = result["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|session| session["own"] == true)
        .flat_map(|session| session["items"].as_array().into_iter().flatten())
        .filter(|item| item["section"] != "links")
        .filter_map(|item| item.get("path").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    if names_by_text(&named, &wanted) {
        return true;
    }
    tokio::task::spawn_blocking(move || {
        let real = |text: &str| {
            std::fs::canonicalize(crate::project_docs::expanded_chat_file_path(text)).ok()
        };
        let Some(target) = real(&wanted) else {
            return false;
        };
        named
            .iter()
            .filter(|candidate| {
                let candidate = candidate.trim();
                // A network share the chat mentions is never asked about: on Windows that signs
                // the computer in to its host (CDXC:Docs 2026-11-11 WHY in chat_file.rs).
                !crate::project_docs::is_network_or_device_path(candidate)
                    && (Path::new(candidate).is_absolute() || candidate.starts_with('~'))
            })
            // A named path that is itself a link names the link, not the file it points at: that
            // file counts only when the chat names it too (`linked_target` in chat_file.rs).
            .filter(|candidate| {
                !std::fs::symlink_metadata(crate::project_docs::expanded_chat_file_path(
                    candidate.trim(),
                ))
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
            })
            .take(NAMED_FILE_CANONICAL_CHECKS)
            .any(|candidate| real(candidate.trim()).as_ref() == Some(&target))
    })
    .await
    .unwrap_or(false)
}

/// Whether one of the list's paths is `wanted` as written, or, for Windows paths, under the
/// list's own key (`path_key`), which spells `C:\a` and `c:/A` alike: a client that sends a
/// Windows path with forward slashes (the desktop does) matches without a look at the disk, and a
/// named file that is gone still counts.
///
/// CDXC:Docs 2026-11-11 WHY: the key turns every backslash into a slash, and on macOS and Linux a
/// backslash is an ordinary character in a file name, so a chat naming `<dir>/secret\id_rsa` (no
/// such file) matched a request for `<dir>/secret/id_rsa` (security review 2026-11-11). The key is
/// compared only on a Windows computer, or when both paths are drive paths, which only a Windows
/// computer has.
fn names_by_text(named: &[String], wanted: &str) -> bool {
    let windows_key =
        |path: &str| (cfg!(windows) || is_drive_path(path)).then(|| rules::path_key(path));
    let wanted_key = windows_key(wanted);
    named.iter().any(|candidate| {
        let candidate = candidate.trim();
        candidate == wanted || (wanted_key.is_some() && windows_key(candidate) == wanted_key)
    })
}

/// `C:\…` or `C:/…`.
fn is_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

async fn read_files_links(state: &AppState, body: &Value) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let text_param = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text_param("projectId"), text_param("sessionId"));
    if project_id.is_empty() || session_id.is_empty() {
        return Err(failure(
            "invalidParams",
            "readSessionFilesLinks requires projectId and sessionId.",
        ));
    }
    let mut targets = {
        let db = open_gxserver_database(&state.paths).map_err(|error| {
            failure(
                "internalError",
                format!("SQLite gxserver state error: {error}"),
            )
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let target = |project_id: &str, session_id: &str, own: bool| {
            let session = repository.get_session(project_id, session_id).ok()??;
            let project = repository.get_project(project_id).ok().flatten();
            Some(Target {
                project_id: project_id.to_string(),
                session_id: session_id.to_string(),
                title: crate::coordinators::session_title_of(&session),
                own,
                cwd: crate::session_git_status::effective_session_git_cwd(
                    &session,
                    project.as_ref(),
                ),
                transcript: None,
            })
        };
        let own = target(&project_id, &session_id, true)
            .ok_or_else(|| failure("notFound", "The session no longer exists."))?;
        let mut targets = vec![own];
        // Done threads included: the list covers the whole stream of work.
        for thread in crate::coordinators::list_threads_for(&db, &project_id, &session_id)? {
            if thread.project_id == project_id && thread.session_id == session_id {
                continue;
            }
            if let Some(target) = target(&thread.project_id, &thread.session_id, false) {
                targets.push(target);
            }
        }
        targets
    };
    for target in &mut targets {
        let Ok(resolution) = crate::session_chat_read::resolve_session_chat_read_state(
            state,
            &target.project_id,
            &target.session_id,
            None,
        ) else {
            continue;
        };
        let (family, path) = resolution.transcript();
        if let (Some(agent), Some(path)) = (
            crate::session_chat::resolve_session_chat_transcript_agent(family),
            path,
        ) {
            target.transcript = Some((agent, path.to_path_buf()));
        }
    }
    let sessions = tokio::task::spawn_blocking(move || {
        // A coordinator's threads are read side by side: a long transcript takes about a second.
        let workers = std::thread::available_parallelism()
            .map(|count| count.get().clamp(1, 8))
            .unwrap_or(4);
        let next = std::sync::atomic::AtomicUsize::new(0);
        let lists: Vec<Mutex<Vec<FilesLinksItem>>> =
            targets.iter().map(|_| Mutex::new(Vec::new())).collect();
        std::thread::scope(|scope| {
            for _ in 0..workers.min(targets.len()) {
                scope.spawn(|| loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(target) = targets.get(index) else {
                        break;
                    };
                    if let Some((agent, path)) = target.transcript.as_ref() {
                        let items = session_items(
                            *agent,
                            path,
                            target.cwd.as_deref(),
                            &target.project_id,
                            &target.session_id,
                        );
                        if let Ok(mut list) = lists[index].lock() {
                            *list = items;
                        }
                    }
                });
            }
        });
        let mut sessions: Vec<(i64, Value)> = Vec::new();
        for (target, list) in targets.into_iter().zip(lists) {
            let items = list.into_inner().unwrap_or_default();
            if items.is_empty() && !target.own {
                continue;
            }
            let latest = items
                .iter()
                .filter_map(|item| item.last.timestamp)
                .max()
                .unwrap_or(i64::MIN);
            sessions.push((
                if target.own { i64::MAX } else { latest },
                json!({
                    "projectId": target.project_id,
                    "sessionId": target.session_id,
                    "title": target.title,
                    "own": target.own,
                    "items": items,
                }),
            ));
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.0));
        sessions
            .into_iter()
            .map(|(_, session)| session)
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|error| failure("internalError", error.to_string()))?;
    Ok(json!({
        "sessions": sessions,
        "generatedAt": chrono::Utc::now().timestamp_millis(),
    }))
}

/// One transcript's list, from the cache while its file is unchanged.
fn session_items(
    agent: crate::session_chat::SessionChatTranscriptAgent,
    path: &Path,
    cwd: Option<&str>,
    project_id: &str,
    session_id: &str,
) -> Vec<FilesLinksItem> {
    session_lists(agent, path, cwd, project_id, session_id).0
}

/// One transcript's list and the pictures its tool calls read, from the cache while its file is
/// unchanged.
fn session_lists(
    agent: crate::session_chat::SessionChatTranscriptAgent,
    path: &Path,
    cwd: Option<&str>,
    project_id: &str,
    session_id: &str,
) -> (Vec<FilesLinksItem>, Vec<String>) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return (Vec::new(), Vec::new());
    };
    let (len, modified) = (metadata.len(), metadata.modified().ok());
    let key = (path.to_path_buf(), cwd.map(str::to_string));
    if let Some(cached) = cache().lock().ok().and_then(|cache| {
        cache
            .get(&key)
            .filter(|cached| cached.len == len && cached.modified == modified)
            .map(|cached| (cached.items.clone(), cached.read_images.clone()))
    }) {
        return cached;
    }
    let messages = read_transcript(agent, path, len, project_id, session_id);
    let mut items = rules::collect_values(&messages);
    if let Some(cwd) = cwd {
        for item in &mut items {
            if let Some(absolute) = item.path.as_deref().and_then(|path| absolute_in(cwd, path)) {
                item.rebase(&absolute);
            }
        }
        items = rules::merge(items);
    }
    let read_images = tool_read_images(&messages, cwd);
    if let Ok(mut cache) = cache().lock() {
        if cache.len() >= CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(
            key,
            CachedList {
                len,
                modified,
                items: items.clone(),
                read_images: read_images.clone(),
            },
        );
    }
    (items, read_images)
}

/// The picture files the agent's tool calls opened, as the chat's read tiles name them
/// (`read_images::image_paths_in_call`).
fn tool_read_images(messages: &[Value], cwd: Option<&str>) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for message in messages {
        let blocks = message.get("blocks").and_then(Value::as_array);
        for block in blocks.into_iter().flatten() {
            if block.get("type").and_then(Value::as_str) != Some("tool-call") {
                continue;
            }
            let name = block
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let input = block.get("input").unwrap_or(&Value::Null);
            for path in
                ghostex_gx_chat_core::transcript::read_images::image_paths_in_call(name, input, cwd)
            {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

/// Whether the session's agent opened the picture at `path` with a tool (a Read of a picture,
/// Codex's view image): the chat shows it as a tile, so a Ghostex Collab guest may read it too
/// (server/src/collab_shim/http.rs).
pub(crate) async fn session_read_image(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    path: &str,
) -> bool {
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    let cwd = {
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return false;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(session) = repository
            .get_session(project_id, session_id)
            .ok()
            .flatten()
        else {
            return false;
        };
        let project = repository.get_project(project_id).ok().flatten();
        crate::session_git_status::effective_session_git_cwd(&session, project.as_ref())
    };
    let Ok(resolution) = crate::session_chat_read::resolve_session_chat_read_state(
        state, project_id, session_id, None,
    ) else {
        return false;
    };
    let (family, transcript) = resolution.transcript();
    let (Some(agent), Some(transcript)) = (
        crate::session_chat::resolve_session_chat_transcript_agent(family),
        transcript.map(Path::to_path_buf),
    ) else {
        return false;
    };
    let (project_id, session_id, path) = (
        project_id.to_string(),
        session_id.to_string(),
        path.to_string(),
    );
    tokio::task::spawn_blocking(move || {
        session_lists(agent, &transcript, cwd.as_deref(), &project_id, &session_id)
            .1
            .contains(&path)
    })
    .await
    .unwrap_or(false)
}

/// Every row of the transcript the user or the agent wrote, oldest first; the harness's injected
/// turns are left out.
fn read_transcript(
    agent: crate::session_chat::SessionChatTranscriptAgent,
    path: &Path,
    len: u64,
    project_id: &str,
    session_id: &str,
) -> Vec<Value> {
    let mut before = len;
    let mut messages: Vec<Value> = Vec::new();
    for _ in 0..MAX_PAGES {
        let Ok(page) = crate::session_chat_history::read_history(
            agent, path, before, PAGE_ROWS, true, false, project_id, session_id,
        ) else {
            break;
        };
        let mut older = page
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        older.extend(messages);
        messages = older;
        let next = page
            .get("beforeOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if page.get("hasMore").and_then(Value::as_bool) != Some(true) || next >= before {
            break;
        }
        before = next;
    }
    messages.retain(|message| {
        matches!(
            crate::ghostex_cli::session_chat_transcript::effective_role(message),
            "user" | "assistant"
        )
    });
    crate::session_chat_mention_paths::annotate_messages(&mut messages, project_id, session_id);
    messages
}

/// A relative path made absolute against the session's folder, when the file is there.
fn absolute_in(cwd: &str, path: &str) -> Option<String> {
    let relative = Path::new(path);
    if relative.is_absolute() || path.starts_with('/') || path.starts_with('~') {
        return None;
    }
    let joined = Path::new(cwd).join(relative);
    if !joined.exists() {
        return None;
    }
    let joined = joined.to_string_lossy().to_string();
    Some(if cfg!(windows) {
        joined.replace('/', "\\")
    } else {
        joined
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows cannot be run here: the user's chat named `C:\dev\.desk-runner\…` and the desktop
    /// asks for `C:/dev/.desk-runner/…`.
    #[test]
    fn a_windows_path_is_named_whatever_its_slashes_and_drive_case() {
        let named = vec![r"C:\dev\.desk-runner\d1111\university-reply\reply-email.txt".to_string()];
        assert!(names_by_text(
            &named,
            "C:/dev/.desk-runner/d1111/university-reply/reply-email.txt"
        ));
        assert!(names_by_text(
            &named,
            r"c:\dev\.desk-runner\d1111\university-reply\reply-email.txt"
        ));
        assert!(!names_by_text(
            &named,
            "C:/dev/.desk-runner/d1111/university-reply/other.txt"
        ));
        assert!(!names_by_text(
            &named,
            "C:/dev.desk-runner/d1111/university-reply/reply-email.txt"
        ));
        // On macOS and Linux a backslash is a name character: a chat naming `secret\id_rsa` names
        // no file in a `secret` folder.
        #[cfg(not(windows))]
        {
            let named = vec![r"/home/me/secret\id_rsa".to_string()];
            assert!(!names_by_text(&named, "/home/me/secret/id_rsa"));
            let named = vec![r"/home/me/notes\..\secret\id_rsa".to_string()];
            assert!(!names_by_text(&named, "/home/me/notes/../secret/id_rsa"));
            assert!(names_by_text(&named, r"/home/me/notes\..\secret\id_rsa"));
        }
    }

    #[test]
    fn pictures_the_agents_tools_opened() {
        let messages = vec![json!({"role": "assistant", "blocks": [
            {"type": "tool-call", "name": "exec", "input": "const r=await tools.view_image({path:\"/work/receipt.png\"});image(r.image_url);"},
            {"type": "tool-call", "name": "Read", "input": {"file_path": "/work/shot.jpg"}},
            {"type": "tool-call", "name": "Read", "input": {"file_path": "/work/notes.md"}},
            {"type": "text", "text": "/work/secret.png"}
        ]})];
        assert_eq!(
            tool_read_images(&messages, None),
            vec![
                "/work/receipt.png".to_string(),
                "/work/shot.jpg".to_string()
            ]
        );
    }
}
