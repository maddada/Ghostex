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
    let Ok(metadata) = std::fs::metadata(path) else {
        return Vec::new();
    };
    let (len, modified) = (metadata.len(), metadata.modified().ok());
    let key = (path.to_path_buf(), cwd.map(str::to_string));
    if let Some(cached) = cache().lock().ok().and_then(|cache| {
        cache
            .get(&key)
            .filter(|cached| cached.len == len && cached.modified == modified)
            .map(|cached| cached.items.clone())
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
            },
        );
    }
    items
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
