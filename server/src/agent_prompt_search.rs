/*
CDXC:PromptSearch 2026-08-20:
The Find surface (a GUI for `gx f`) is served from here. It keeps one warm
`ghostex_find::SearchIndex` — the same scanner, matcher, favorites file, and Codex cache
the terminal picker uses — so both surfaces rank identically and a prompt starred
in one is starred in the other.

Rows carry a stable per-prompt key (the favorites hash of agent + text), not an
index position. A rebuild reorders records, and the user may sit on a result for
minutes before acting on it; keying by identity means the follow-up call still
lands on the prompt they were looking at instead of being rejected or, worse,
acting on whatever moved into that slot.

Opening a result is deliberately NOT a session factory here. This module resolves
the decision — focus a live Ghostex session that already owns the conversation, or
hand the host a cwd plus the exact command to run — and the host performs it with
the session-creation path it already has.
*/

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use ghostex_find::agent::{Agent, ALL_AGENTS};
use ghostex_find::favorites::{self, FavoriteSet};
use ghostex_find::index::{self as zehn_index, day_key, QueryOptions, SearchIndex};
use ghostex_find::scan::RecordKind;
use ghostex_find::store::SearchStore;

/// Rows carry a bounded slice of the prompt so a keystroke never ships megabytes
/// of pasted transcripts — a full page of 60 pasted AGENTS.md prompts is a
/// quarter-megabyte at 4k each, which is a poor deal over an SSH-tunnelled
/// mobile link. The row slice only has to fill one line; the preview pane pulls
/// the selected prompt in full through `readAgentPromptText`.
const DEFAULT_TEXT_LIMIT: usize = 1_200;
const MAX_TEXT_LIMIT: usize = 200_000;
const DEFAULT_ROW_LIMIT: usize = 60;
const MAX_ROW_LIMIT: usize = 500;
const MAX_CANDIDATES_PER_QUERY: usize = 4_000;
const MAX_DEEP_CANDIDATES: usize = 20_000;

/// How long a synced prompt store is served before the next query runs a quick
/// incremental catch-up pass.
const INDEX_MAX_AGE: Duration = Duration::from_secs(60);
const FOREGROUND_ANSWER_BUDGET_FILES: usize = 40;
const FOREGROUND_ANSWER_BUDGET_MS: u64 = 250;

/// Debounce and queue bounds for the single background transcript nudge worker.
const NUDGE_DEBOUNCE: Duration = Duration::from_secs(3);
const MAX_PENDING_NUDGES: usize = 64;
const NUDGE_WORKER_IDLE_CLOSE: Duration = Duration::from_secs(30);

struct CachedStore {
    store: SearchStore,
    favorites: FavoriteSet,
    favorites_path: PathBuf,
    opencode_error: Option<String>,
    empryo_error: Option<String>,
    epoch: u64,
    synced_at: Instant,
    synced_at_unix: i64,
}

static CACHE: OnceLock<Mutex<Option<CachedStore>>> = OnceLock::new();
static SYNC_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static BACKFILL_RUNNING: AtomicBool = AtomicBool::new(false);

fn cache() -> &'static Mutex<Option<CachedStore>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

fn sync_lock() -> &'static Mutex<()> {
    SYNC_LOCK.get_or_init(|| Mutex::new(()))
}

struct NudgeJob {
    cache_root: PathBuf,
    agent: Agent,
    transcript_path: PathBuf,
    session_id: Option<String>,
    project: Option<String>,
    title: Option<String>,
    ready_at: Instant,
}

#[derive(Default)]
struct NudgeQueue {
    pending: HashMap<(PathBuf, PathBuf), NudgeJob>,
    worker_started: bool,
}

static NUDGE_QUEUE: OnceLock<(Mutex<NudgeQueue>, Condvar)> = OnceLock::new();

fn nudge_queue() -> &'static (Mutex<NudgeQueue>, Condvar) {
    NUDGE_QUEUE.get_or_init(|| (Mutex::new(NudgeQueue::default()), Condvar::new()))
}

#[derive(Debug)]
pub struct PromptSearchError {
    pub code: &'static str,
    pub message: String,
}

impl PromptSearchError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalidParams",
            message: message.into(),
        }
    }

    fn unknown_prompt() -> Self {
        Self {
            code: "notFound",
            message: "That prompt is no longer in this machine's agent history.".to_string(),
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/*
CDXC:AgentHooks 2026-08-06-21:35 (same rule, applied to history):
GHOSTEX_HOME and explicit daemon homes are isolated profiles. Their history
scan, derived cache, and favorites file must stay inside that profile instead
of following the process environment back into the real user's agent data.
Production daemons keep zehn's own environment contract so `gx f` and this
surface read and write exactly the same files.
*/
fn resolve_search_paths(paths: &crate::paths::GxserverPaths) -> (String, PathBuf, PathBuf) {
    if let Some(isolated) = paths.isolated_agent_home_dir.as_ref() {
        let home = isolated.to_string_lossy().to_string();
        return (
            home,
            isolated.join(".cache").join("ghostex").join("zehn"),
            isolated.join(".config").join("zehn").join("favorites"),
        );
    }
    let home = paths.home_dir.to_string_lossy().to_string();
    let cache_root = zehn_index::cache_root_from_env(&home);
    let xdg_config = std::env::var("XDG_CONFIG_HOME").ok();
    let favorites_path = favorites::path_for(&home, xdg_config.as_deref());
    (home, cache_root, favorites_path)
}

fn spawn_answer_backfill(home: PathBuf, cache_root: PathBuf) {
    if BACKFILL_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    std::thread::Builder::new()
        .name("ghostex-find-backfill".to_string())
        .spawn(move || {
            let _guard = BackfillGuard;
            let Some(mut store) = SearchStore::open(&cache_root) else {
                return;
            };
            loop {
                let stats =
                    store.sync_answer_sources_budgeted(&home, 50, Duration::from_millis(400));
                if stats.remaining_files == 0 || stats.files_indexed == 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        })
        .ok();
}

struct BackfillGuard;
impl Drop for BackfillGuard {
    fn drop(&mut self) {
        BACKFILL_RUNNING.store(false, Ordering::Release);
    }
}

fn sync_store(
    paths: &crate::paths::GxserverPaths,
    prev_epoch: u64,
) -> Result<CachedStore, PromptSearchError> {
    let (home, cache_root, favorites_path) = resolve_search_paths(paths);
    let mut store = SearchStore::open(&cache_root).ok_or_else(|| PromptSearchError {
        code: "internalError",
        message: "Could not open the rebuildable search index database.".to_string(),
    })?;
    let built = SearchIndex::build(&home, &cache_root, favorites_path.clone());
    store.sync_user_records(&built.records);
    let home_path = PathBuf::from(&home);
    let stats = store.sync_answer_sources_budgeted(
        &home_path,
        FOREGROUND_ANSWER_BUDGET_FILES,
        Duration::from_millis(FOREGROUND_ANSWER_BUDGET_MS),
    );
    if stats.remaining_files > 0 {
        spawn_answer_backfill(home_path, cache_root);
    }
    // Reload favorites right before installing so any star toggled while syncing survives.
    let mut favorites = FavoriteSet::new();
    favorites.load(&favorites_path);
    Ok(CachedStore {
        store,
        favorites,
        favorites_path,
        opencode_error: built.opencode_error,
        empryo_error: built.empryo_error,
        epoch: prev_epoch + 1,
        synced_at: Instant::now(),
        synced_at_unix: now_unix(),
    })
}

/// Run `action` against the SQLite search store, catching up incrementally when
/// missing, stale, or refreshed.
fn with_store<T>(
    paths: &crate::paths::GxserverPaths,
    force_refresh: bool,
    action: impl FnOnce(&mut CachedStore) -> Result<T, PromptSearchError>,
) -> Result<T, PromptSearchError> {
    with_store_policy(paths, force_refresh, true, action)
}

/// Run `action` against whatever store is open without triggering a sync pass.
///
/// CDXC:PromptSearch 2026-09-21 WHY:
/// Opening a result acts on a row the warm index already served, addressed by a key that survives rebuilds. Letting that call rebuild an index older than `INDEX_MAX_AGE` put a full history scan between Enter and the modal closing whenever the user had browsed for more than 90 seconds.
fn with_warm_store<T>(
    paths: &crate::paths::GxserverPaths,
    action: impl FnOnce(&mut CachedStore) -> Result<T, PromptSearchError>,
) -> Result<T, PromptSearchError> {
    with_store_policy(paths, false, false, action)
}

fn with_store_policy<T>(
    paths: &crate::paths::GxserverPaths,
    force_refresh: bool,
    sync_when_stale: bool,
    action: impl FnOnce(&mut CachedStore) -> Result<T, PromptSearchError>,
) -> Result<T, PromptSearchError> {
    {
        let mut guard = cache().lock().map_err(|_| PromptSearchError {
            code: "internalError",
            message: "The prompt index lock was poisoned.".to_string(),
        })?;
        let fresh = match guard.as_ref() {
            None => false,
            Some(cached) => {
                !force_refresh && (!sync_when_stale || cached.synced_at.elapsed() <= INDEX_MAX_AGE)
            }
        };
        if fresh {
            let cached = guard.as_mut().expect("checked Some above");
            return action(cached);
        }
    }

    // Serialize syncs without holding `CACHE` across scanning or SQLite writes, so
    // warm reads (`readAgentPromptText`, favorites, concurrent queries) never stall.
    let _sync_guard = sync_lock().lock().map_err(|_| PromptSearchError {
        code: "internalError",
        message: "The prompt sync lock was poisoned.".to_string(),
    })?;
    let prev_epoch = {
        let mut guard = cache().lock().map_err(|_| PromptSearchError {
            code: "internalError",
            message: "The prompt index lock was poisoned.".to_string(),
        })?;
        if !force_refresh {
            let fresh = match guard.as_ref() {
                None => false,
                Some(cached) => !sync_when_stale || cached.synced_at.elapsed() <= INDEX_MAX_AGE,
            };
            if fresh {
                let cached = guard.as_mut().expect("checked Some above");
                return action(cached);
            }
        }
        guard.as_ref().map(|prev| prev.epoch).unwrap_or(0)
    };

    let refreshed = sync_store(paths, prev_epoch)?;
    let mut guard = cache().lock().map_err(|_| PromptSearchError {
        code: "internalError",
        message: "The prompt index lock was poisoned.".to_string(),
    })?;
    *guard = Some(refreshed);
    let cached = guard.as_mut().expect("store opened above");
    action(cached)
}

/// Drop the warm store handle so the next call re-syncs.
pub fn invalidate_index() {
    if let Ok(mut guard) = cache().lock() {
        *guard = None;
    }
}

fn run_nudge_worker() {
    let (lock, cv) = nudge_queue();
    let mut cached_store: Option<(PathBuf, SearchStore)> = None;
    loop {
        let next_job = {
            let mut state = match lock.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            loop {
                if state.pending.is_empty() {
                    if cached_store.is_some() {
                        let (next_state, wait_res) =
                            match cv.wait_timeout(state, NUDGE_WORKER_IDLE_CLOSE) {
                                Ok(pair) => pair,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                        state = next_state;
                        if state.pending.is_empty() && wait_res.timed_out() {
                            cached_store = None;
                        }
                    } else {
                        state = match cv.wait(state) {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                    }
                    continue;
                }

                let earliest_key = state
                    .pending
                    .iter()
                    .min_by_key(|(_, job)| job.ready_at)
                    .map(|(k, job)| (k.clone(), job.ready_at));
                let Some((key, ready_at)) = earliest_key else {
                    continue;
                };
                let now = Instant::now();
                if ready_at > now {
                    let (next_state, _) = match cv.wait_timeout(state, ready_at - now) {
                        Ok(pair) => pair,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    state = next_state;
                    continue;
                }
                break state.pending.remove(&key);
            }
        };

        let Some(job) = next_job else {
            continue;
        };
        if !SearchStore::exists(&job.cache_root) {
            if cached_store
                .as_ref()
                .is_some_and(|(root, _)| *root == job.cache_root)
            {
                cached_store = None;
            }
            continue;
        }
        let needs_open = cached_store
            .as_ref()
            .is_none_or(|(root, _)| *root != job.cache_root);
        if needs_open {
            cached_store = SearchStore::open(&job.cache_root).map(|s| (job.cache_root.clone(), s));
        }
        if let Some((_, store)) = cached_store.as_mut() {
            let _ = store.index_single_transcript(
                job.agent,
                &job.transcript_path,
                job.session_id.as_deref(),
                job.project.as_deref(),
                job.title.as_deref(),
            );
        }
    }
}

/// Queue a debounced, deduplicated index catch-up for one changed transcript
/// file. Stays completely lazy (no worker thread, no SQLite open, no file parse)
/// until Find has actually created `search_index.sqlite3`.
pub fn nudge_transcript_file(
    paths: &crate::paths::GxserverPaths,
    agent_name: &str,
    transcript_path: &Path,
    session_id: Option<&str>,
    project: Option<&str>,
    title: Option<&str>,
) {
    let Some(agent) = Agent::parse(agent_name) else {
        return;
    };
    let (_, cache_root, _) = resolve_search_paths(paths);
    if !SearchStore::exists(&cache_root) {
        return;
    }
    let key = (cache_root.clone(), transcript_path.to_path_buf());
    let job = NudgeJob {
        cache_root,
        agent,
        transcript_path: transcript_path.to_path_buf(),
        session_id: session_id.map(str::to_string),
        project: project.map(str::to_string),
        title: title.map(str::to_string),
        ready_at: Instant::now() + NUDGE_DEBOUNCE,
    };

    let (lock, cv) = nudge_queue();
    let mut state = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if !state.pending.contains_key(&key) && state.pending.len() >= MAX_PENDING_NUDGES {
        if let Some(oldest_key) = state
            .pending
            .iter()
            .min_by_key(|(_, item)| item.ready_at)
            .map(|(k, _)| k.clone())
        {
            state.pending.remove(&oldest_key);
        }
    }
    state.pending.insert(key, job);
    if !state.worker_started {
        if std::thread::Builder::new()
            .name("ghostex-find-nudge".to_string())
            .spawn(run_nudge_worker)
            .is_ok()
        {
            state.worker_started = true;
        }
    }
    cv.notify_one();
}

/// Observe a committed `/api/ingestAgentHookEvent` result and enqueue a lazy
/// debounced Find catch-up when a turn ends, using the daemon's `GxserverPaths`.
pub(crate) fn observe_hook_turn_end(
    paths: &crate::paths::GxserverPaths,
    params: &Map<String, Value>,
    result: &Value,
) {
    let ended_turn = crate::session_status::result_ended_turn(result)
        || (result.pointer("/activity/activity").and_then(Value::as_str) == Some("idle")
            && result.get("previousActivity").and_then(Value::as_str) == Some("working"));
    if !ended_turn {
        return;
    }
    let Some(session) = result.get("session") else {
        return;
    };
    let Some(path) = params
        .get("agentSessionPath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            session
                .pointer("/runtimeSettings/agentSessionPath")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
    else {
        return;
    };
    let agent_name = crate::session_chat_follower::session_chat_agent_for_session(session)
        .or_else(|| {
            session
                .get("agentId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    if agent_name.is_empty() {
        return;
    }
    let sid = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str);
    let cwd = session.get("cwd").and_then(Value::as_str);
    let title = session.get("title").and_then(Value::as_str);
    nudge_transcript_file(paths, &agent_name, Path::new(path), sid, cwd, title);
}

fn read_kinds(params: &Map<String, Value>) -> Result<Vec<RecordKind>, PromptSearchError> {
    if let Some(value) = params.get("kinds") {
        let Some(items) = value.as_array() else {
            return Err(PromptSearchError::invalid(
                "kinds must be an array of record kinds (user, agent_answer).",
            ));
        };
        let mut kinds = Vec::new();
        for item in items {
            let Some(name) = item.as_str() else {
                return Err(PromptSearchError::invalid(
                    "kinds must be an array of record kinds (user, agent_answer).",
                ));
            };
            let Some(kind) = RecordKind::parse(name) else {
                return Err(PromptSearchError::invalid(format!(
                    "Unknown search kind \"{name}\"."
                )));
            };
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        return Ok(kinds);
    }
    if let Some(scope) = params.get("scope").and_then(Value::as_str) {
        return match scope.trim().to_ascii_lowercase().as_str() {
            "all" | "both" => Ok(vec![RecordKind::User, RecordKind::AgentAnswer]),
            "user" | "prompt" | "prompts" => Ok(vec![RecordKind::User]),
            "agent_answer" | "agent-answer" | "answer" | "answers" | "agent" => {
                Ok(vec![RecordKind::AgentAnswer])
            }
            other => Err(PromptSearchError::invalid(format!(
                "Unknown search scope \"{other}\"."
            ))),
        };
    }
    // Older clients (including phones on 11.2.1+) omit `kinds` and expect user prompts only.
    Ok(vec![RecordKind::User])
}

fn read_agents(params: &Map<String, Value>) -> Result<Vec<Agent>, PromptSearchError> {
    let Some(value) = params.get("agents") else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(PromptSearchError::invalid(
            "agents must be an array of agent names.",
        ));
    };
    let mut agents = Vec::new();
    for item in items {
        let Some(name) = item.as_str() else {
            return Err(PromptSearchError::invalid(
                "agents must be an array of agent names.",
            ));
        };
        let Some(agent) = Agent::parse(name.trim()) else {
            return Err(PromptSearchError::invalid(format!(
                "Unknown agent \"{name}\"."
            )));
        };
        agents.push(agent);
    }
    Ok(agents)
}

fn read_usize(params: &Map<String, Value>, key: &str, default: usize, max: usize) -> usize {
    params
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
        .min(max)
}

fn truncate_on_char_boundary(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn prompt_row_excerpt(text: &str, highlights: &[u16], limit: usize) -> (String, Vec<u16>, bool) {
    if text.len() <= limit {
        return (text.to_string(), highlights.to_vec(), false);
    }
    if limit == 0 {
        return (String::new(), Vec::new(), true);
    }

    let first_highlight = highlights.first().copied().map(usize::from).unwrap_or(0);
    let leading_context = (limit / 3).min(80);
    let mut start = first_highlight.saturating_sub(leading_context);
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    let end = start + truncate_on_char_boundary(&text[start..], limit).len();

    let visible_highlights = highlights
        .iter()
        .copied()
        .map(usize::from)
        .filter(|position| *position >= start && *position < end)
        .filter_map(|position| u16::try_from(position - start).ok())
        .collect();
    (text[start..end].to_string(), visible_highlights, true)
}

fn build_prompt_row(
    open_sessions: &[PromptLaunchSession],
    rec: &ghostex_find::scan::Record,
    index: usize,
    favorite: bool,
    score: i32,
    positions: &[u16],
    text_limit: usize,
) -> Value {
    let (text, highlights, truncated) = prompt_row_excerpt(&rec.text, positions, text_limit);
    json!({
        "key": format!("{:016x}", zehn_index::record_key(rec)),
        "index": index,
        "agent": rec.agent.label(),
        "agentColor": rec.agent.hex_color(),
        "kind": rec.kind.as_str(),
        "interrupted": rec.interrupted,
        "text": text,
        "textLength": rec.text.len(),
        "truncated": truncated,
        "title": open_session_title(open_sessions, rec)
            .unwrap_or_else(|| rec.display_title()),
        "project": rec.project,
        "projectName": rec.project_display_name(),
        "sessionId": rec.session,
        "ts": rec.ts,
        "dayKey": day_key(rec.ts),
        "favorite": favorite,
        "score": score,
        "highlights": highlights,
        "meta": {
            "provider": rec.meta.provider,
            "model": rec.meta.model,
            "thinking": rec.meta.thinking,
            "plan": rec.meta.plan,
            "usage": {
                "input": rec.meta.usage.input,
                "output": rec.meta.usage.output,
                "cacheRead": rec.meta.usage.cache_read,
                "cacheWrite": rec.meta.usage.cache_write,
                "total": rec.meta.usage.total,
                "contextWindow": rec.meta.usage.context_window,
                "ratePercent": rec.meta.usage.rate_percent,
                "cost": rec.meta.usage.cost,
            },
        },
    })
}

/// `POST /api/searchAgentPrompts`
pub fn search_agent_prompts(
    paths: &crate::paths::GxserverPaths,
    params: &Map<String, Value>,
    open_sessions: &[PromptLaunchSession],
) -> Result<Value, PromptSearchError> {
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let agents = read_agents(params)?;
    let kinds = read_kinds(params)?;
    let project = params
        .get("project")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let group_by_day = params
        .get("groupByDay")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let offset = read_usize(params, "offset", 0, usize::MAX);
    let limit = read_usize(params, "limit", DEFAULT_ROW_LIMIT, MAX_ROW_LIMIT);
    let text_limit = read_usize(params, "textLimit", DEFAULT_TEXT_LIMIT, MAX_TEXT_LIMIT);
    let include_facets = params
        .get("includeFacets")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let refresh = params
        .get("refresh")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    with_store(paths, refresh, |cached| {
        let (rows, matched, total) = if query.is_empty() {
            let page =
                cached
                    .store
                    .query_empty_page(&kinds, &agents, project.as_deref(), offset, limit);
            let total = if agents.is_empty() && project.is_none() {
                page.matched
            } else {
                cached.store.total_count(&kinds)
            };
            let rows: Vec<Value> = page
                .records
                .iter()
                .enumerate()
                .map(|(idx, rec)| {
                    let favorite = cached.favorites.contains(zehn_index::record_key(rec));
                    build_prompt_row(
                        open_sessions,
                        rec,
                        offset.saturating_add(idx),
                        favorite,
                        0,
                        &[],
                        text_limit,
                    )
                })
                .collect();
            (rows, page.matched, total)
        } else {
            let candidate_limit =
                MAX_CANDIDATES_PER_QUERY.max(offset.saturating_add(limit).min(MAX_DEEP_CANDIDATES));
            let candidates = cached.store.query_candidates(
                &query,
                &kinds,
                &agents,
                project.as_deref(),
                candidate_limit,
            );
            let options = QueryOptions {
                query,
                agents,
                kinds: kinds.clone(),
                project,
                group_by_day,
                offset,
                limit,
            };
            let result = zehn_index::rank_records(&candidates, &cached.favorites, &options);
            let total = cached.store.total_count(&kinds);
            let rows: Vec<Value> = result
                .hits
                .iter()
                .map(|hit| {
                    let rec = &candidates[hit.index];
                    build_prompt_row(
                        open_sessions,
                        rec,
                        hit.index,
                        hit.favorite,
                        hit.score,
                        &hit.positions,
                        text_limit,
                    )
                })
                .collect();
            (rows, result.matched, total)
        };

        let mut payload = json!({
            "indexEpoch": cached.epoch,
            "indexedAt": cached.synced_at_unix,
            "total": total,
            "matched": matched,
            "offset": offset,
            "rows": rows,
        });
        if let Some(error) = &cached.opencode_error {
            payload["opencodeError"] = json!(error);
        }
        if let Some(error) = &cached.empryo_error {
            payload["empryoError"] = json!(error);
        }
        if include_facets {
            let (projects, present_agents) = cached.store.facets(&kinds);
            payload["projects"] = json!(projects
                .into_iter()
                .map(|path| json!({
                    "name": ghostex_find::scan::project_display_name(&path),
                    "path": path,
                }))
                .collect::<Vec<_>>());
            payload["agents"] = json!(ALL_AGENTS
                .into_iter()
                .map(|agent| json!({
                    "agent": agent.label(),
                    "color": agent.hex_color(),
                    "present": present_agents.contains(&agent),
                }))
                .collect::<Vec<_>>());
        }
        Ok(payload)
    })
}

fn parse_prompt_key(params: &Map<String, Value>) -> Result<u64, PromptSearchError> {
    let key = params
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| PromptSearchError::invalid("key is required."))?;
    u64::from_str_radix(key.trim(), 16)
        .map_err(|_| PromptSearchError::invalid("key must be a 16-digit hex prompt key."))
}

fn resolve_record(
    cached: &CachedStore,
    params: &Map<String, Value>,
) -> Result<ghostex_find::scan::Record, PromptSearchError> {
    let key = parse_prompt_key(params)?;
    let key_hex = format!("{key:016x}");
    cached
        .store
        .find_by_key(&key_hex)
        .ok_or_else(PromptSearchError::unknown_prompt)
}

/// `POST /api/readAgentPromptText` — the untruncated text for one row.
pub fn read_agent_prompt_text(
    paths: &crate::paths::GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, PromptSearchError> {
    with_warm_store(paths, |cached| {
        let rec = resolve_record(cached, params)?;
        Ok(json!({
            "key": format!("{:016x}", zehn_index::record_key(&rec)),
            "kind": rec.kind.as_str(),
            "text": rec.text,
        }))
    })
}

/// `POST /api/toggleAgentPromptFavorite`
pub fn toggle_agent_prompt_favorite(
    paths: &crate::paths::GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, PromptSearchError> {
    with_warm_store(paths, |cached| {
        let rec = resolve_record(cached, params)?;
        let fav_key = favorites::key_for_kind(rec.agent.label(), rec.kind, &rec.text);
        let favorite = match params.get("favorite").and_then(Value::as_bool) {
            Some(next) => {
                cached.favorites.set(fav_key, next);
                cached.favorites.save(&cached.favorites_path);
                next
            }
            None => {
                let next = cached.favorites.toggle(fav_key);
                cached.favorites.save(&cached.favorites_path);
                next
            }
        };
        Ok(json!({
            "key": format!("{:016x}", zehn_index::record_key(&rec)),
            "favorite": favorite,
        }))
    })
}

/// The decision for opening a result, resolved server-side so every host applies
/// the same rule and the same command text.
#[derive(Debug, Clone, PartialEq)]
pub enum PromptLaunchPlan {
    /// A live Ghostex session already owns this agent conversation.
    Focus {
        project_id: String,
        session_id: String,
    },
    /// Nothing owns it; run `command` in `cwd` as a new agent session.
    Launch {
        agent: Agent,
        command: Vec<String>,
        cwd: String,
        cwd_exists: bool,
        title: String,
    },
}

/// A stored session row paired with the CLI family its launch agent resumes
/// with (`claude` for a `custom-…` Claude configuration). The caller resolves
/// the family because only it holds the project row that declares it.
/// `named_title` is the session's Ghostex title once it has a real name, and
/// `None` while it still shows the agent's placeholder title.
#[derive(Debug, Clone)]
pub struct PromptLaunchSession {
    pub session: Value,
    pub agent_family_id: Option<String>,
    pub named_title: Option<String>,
}

/// CDXC:PromptSearch 2026-09-21 WHY:
/// A result whose conversation is open in the sidebar shows that session's Ghostex name, so the row reads the same as the sidebar row Enter focuses. Every other result keeps the title recorded in the agent's own transcript.
fn open_session_title<'a>(
    open_sessions: &'a [PromptLaunchSession],
    rec: &ghostex_find::scan::Record,
) -> Option<&'a str> {
    if rec.session.is_empty() {
        return None;
    }
    open_session_owner(open_sessions, &rec.session, rec.agent)?
        .named_title
        .as_deref()
}

/// The open session that owns a conversation, a running one ahead of a sleeping one.
fn open_session_owner<'a>(
    sessions: &'a [PromptLaunchSession],
    agent_session_id: &str,
    agent: Agent,
) -> Option<&'a PromptLaunchSession> {
    let mut owners = sessions
        .iter()
        .filter(|candidate| session_owns_agent_conversation(candidate, agent_session_id, agent));
    let first = owners.next()?;
    if session_lifecycle(first) == "running" {
        return Some(first);
    }
    Some(
        owners
            .find(|candidate| session_lifecycle(candidate) == "running")
            .unwrap_or(first),
    )
}

fn session_lifecycle(candidate: &PromptLaunchSession) -> &str {
    candidate
        .session
        .get("lifecycleState")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// True when a session row is an open owner of `agent_session_id`: a running
/// or sleeping lifecycle, or a provider that still exists.
///
/// CDXC:PromptSearch 2026-09-21 WHY:
/// A sleeping session is still a row in the sidebar, and focusing it wakes it. Counting only running sessions made Find start a second `--resume` session next to the sleeping one that already owned the conversation.
///
/// CDXC:PromptSearch 2026-09-19 WHY:
/// A session launched from a custom agent configuration stores that configuration's `custom-…` id as its agent, never the family the transcript belongs to, so the family resolved the way resume planning resolves it must count as a match too. Comparing the raw id alone made Find open a second `claude --resume` writer onto a conversation a running custom Claude session already owned.
/// SEE-ALSO: `resolve_live_agent_session_owner` in server/src/ghostex_cli/wait/lifecycle.rs, the same rule for the `gx f` picker.
fn session_owns_agent_conversation(
    candidate: &PromptLaunchSession,
    agent_session_id: &str,
    agent: Agent,
) -> bool {
    let session = &candidate.session;
    let runtime_settings = session.get("runtimeSettings");
    let stored = runtime_settings
        .and_then(|settings| settings.get("agentSessionId"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if stored != agent_session_id {
        return false;
    }
    let lifecycle = session
        .get("lifecycleState")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let provider_state = session
        .get("providerState")
        .and_then(|state| state.get("lifecycleState"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(lifecycle, "running" | "sleeping") && provider_state != "exists" {
        return false;
    }
    let session_agent = session
        .get("agentId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    session_agent.is_empty()
        || session_agent.eq_ignore_ascii_case(agent.ghostex_agent_id())
        || candidate
            .agent_family_id
            .as_deref()
            .is_some_and(|family| family.trim().eq_ignore_ascii_case(agent.ghostex_agent_id()))
}

/// `POST /api/resolveAgentPromptLaunch`
///
/// `action` is `"resume"` (default) or `"fork"`. Resume prefers focusing a live
/// owner, exactly like pressing Enter in the terminal picker, so Ghostex never
/// opens a second writer onto one agent conversation.
pub fn resolve_agent_prompt_launch(
    paths: &crate::paths::GxserverPaths,
    params: &Map<String, Value>,
    read_conversation_sessions: impl FnOnce(&str) -> Result<Vec<PromptLaunchSession>, PromptSearchError>,
    accept_all_default: bool,
) -> Result<Value, PromptSearchError> {
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("resume");
    /*
    CDXC:PromptSearch 2026-08-20:
    Accept All is a daemon-owned setting, exactly as it is for `gx f` (which
    reads it and prepends `--accept-all`). Clients therefore do not have to know
    it, and a client that does pass it explicitly still wins — that is the
    override, not a guess.
    */
    let accept_all = params
        .get("acceptAll")
        .and_then(Value::as_bool)
        .unwrap_or(accept_all_default);
    let fork_agent = match params.get("forkAgent").and_then(Value::as_str) {
        Some(name) => Some(
            Agent::parse(name.trim())
                .ok_or_else(|| PromptSearchError::invalid(format!("Unknown agent \"{name}\".")))?,
        ),
        None => None,
    };

    with_warm_store(paths, |cached| {
        let rec = resolve_record(cached, params)?;
        let plan = match action {
            "resume" => {
                if rec.session.is_empty() {
                    return Err(PromptSearchError::invalid(format!(
                        "No session id was recorded for this {} prompt, so it cannot be resumed. Fork it instead.",
                        rec.agent.label()
                    )));
                }
                let conversation_sessions = read_conversation_sessions(&rec.session)?;
                let owner = open_session_owner(&conversation_sessions, &rec.session, rec.agent)
                    .map(|candidate| &candidate.session);
                match owner {
                    Some(session) => PromptLaunchPlan::Focus {
                        project_id: session
                            .get("projectId")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        session_id: session
                            .get("sessionId")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    },
                    None => PromptLaunchPlan::Launch {
                        agent: rec.agent,
                        command: rec.resume_argv(accept_all),
                        cwd: rec.project.clone(),
                        cwd_exists: !rec.project.is_empty()
                            && std::path::Path::new(&rec.project).is_dir(),
                        title: rec.display_title().to_string(),
                    },
                }
            }
            "fork" => {
                let agent = fork_agent.unwrap_or(rec.agent);
                let Some(command) = agent.fresh_session_argv(&rec.text, accept_all) else {
                    return Err(PromptSearchError::invalid(format!(
                        "A prompt cannot be forked into {}: it takes no starting prompt. Fork it into another agent.",
                        agent.label()
                    )));
                };
                PromptLaunchPlan::Launch {
                    agent,
                    command,
                    cwd: rec.project.clone(),
                    cwd_exists: !rec.project.is_empty()
                        && std::path::Path::new(&rec.project).is_dir(),
                    title: rec.display_title().to_string(),
                }
            }
            other => {
                return Err(PromptSearchError::invalid(format!(
                    "Unknown action \"{other}\"; expected \"resume\" or \"fork\"."
                )));
            }
        };
        Ok(launch_plan_payload(
            &format!("{:016x}", zehn_index::record_key(&rec)),
            &plan,
        ))
    })
}

fn launch_plan_payload(key: &str, plan: &PromptLaunchPlan) -> Value {
    match plan {
        PromptLaunchPlan::Focus {
            project_id,
            session_id,
        } => json!({
            "key": key,
            "mode": "focus",
            "projectId": project_id,
            "sessionId": session_id,
        }),
        PromptLaunchPlan::Launch {
            agent,
            command,
            cwd,
            cwd_exists,
            title,
        } => json!({
            "key": key,
            "mode": "launch",
            "agent": agent.label(),
            "command": command,
            "commandLine": shell_command_line(command),
            "cwd": cwd,
            "cwdExists": cwd_exists,
            "title": title,
        }),
    }
}

/// Quote an argv into a single POSIX shell command line, so a host that can only
/// type text into a terminal still runs exactly the argv resolved here.
pub fn shell_command_line(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'_' | b'-' | b'.' | b'/' | b':' | b'=' | b'@' | b'+' | b','
                )
        })
    {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quoting_keeps_plain_words_bare_and_escapes_the_rest() {
        assert_eq!(shell_quote("claude"), "claude");
        assert_eq!(shell_quote("--resume"), "--resume");
        assert_eq!(shell_quote("/a/b-c.d"), "/a/b-c.d");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("two words"), "'two words'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(
            shell_command_line(&["codex".into(), "resume".into(), "a b".into()]),
            "codex resume 'a b'"
        );
    }

    #[test]
    fn text_truncation_never_splits_a_utf8_character() {
        let text = "aé日本語";
        for limit in 0..text.len() + 2 {
            let cut = truncate_on_char_boundary(text, limit);
            assert!(text.starts_with(cut));
            assert!(cut.len() <= limit.max(0) || cut.len() == text.len());
        }
        assert_eq!(truncate_on_char_boundary(text, 2), "a");
        assert_eq!(truncate_on_char_boundary(text, 100), text);
    }

    fn session_row(agent_session_id: &str, lifecycle: &str, agent: &str) -> Value {
        json!({
            "projectId": "p1",
            "sessionId": "s1",
            "agentId": agent,
            "lifecycleState": lifecycle,
            "runtimeSettings": { "agentSessionId": agent_session_id },
        })
    }

    fn session(agent_session_id: &str, lifecycle: &str, agent: &str) -> PromptLaunchSession {
        PromptLaunchSession {
            session: session_row(agent_session_id, lifecycle, agent),
            agent_family_id: None,
            named_title: None,
        }
    }

    #[test]
    fn a_live_running_session_owns_the_conversation() {
        assert!(session_owns_agent_conversation(
            &session("abc", "running", "claude"),
            "abc",
            Agent::Claude
        ));
    }

    #[test]
    fn a_stopped_session_does_not_own_the_conversation() {
        assert!(!session_owns_agent_conversation(
            &session("abc", "stopped", "claude"),
            "abc",
            Agent::Claude
        ));
    }

    #[test]
    fn an_existing_provider_counts_as_live_even_when_the_lifecycle_lags() {
        let mut value = session("abc", "stopped", "claude");
        value.session["providerState"] = json!({ "lifecycleState": "exists" });
        assert!(session_owns_agent_conversation(
            &value,
            "abc",
            Agent::Claude
        ));
    }

    #[test]
    fn a_different_agent_or_conversation_never_matches() {
        assert!(!session_owns_agent_conversation(
            &session("abc", "running", "codex"),
            "abc",
            Agent::Claude
        ));
        assert!(!session_owns_agent_conversation(
            &session("other", "running", "claude"),
            "abc",
            Agent::Claude
        ));
    }

    #[test]
    fn prompt_keys_must_be_hex_and_present() {
        let mut params = Map::new();
        assert_eq!(parse_prompt_key(&params).unwrap_err().code, "invalidParams");
        params.insert("key".to_string(), json!("not-hex"));
        assert_eq!(parse_prompt_key(&params).unwrap_err().code, "invalidParams");
        params.insert("key".to_string(), json!("00000000000000ff"));
        assert_eq!(parse_prompt_key(&params).unwrap(), 0xff);
        params.insert("key".to_string(), json!(" 08775375308e09c0 "));
        assert_eq!(parse_prompt_key(&params).unwrap(), 0x0877_5375_308e_09c0);
    }

    #[test]
    fn isolated_profiles_keep_history_state_inside_the_profile() {
        let mut paths = crate::paths::get_gxserver_paths(None);
        paths.isolated_agent_home_dir = Some(PathBuf::from("/tmp/gx-isolated"));
        let (home, cache_root, favorites_path) = resolve_search_paths(&paths);
        assert_eq!(home, "/tmp/gx-isolated");
        assert_eq!(
            cache_root,
            PathBuf::from("/tmp/gx-isolated/.cache/ghostex/zehn")
        );
        assert_eq!(
            favorites_path,
            PathBuf::from("/tmp/gx-isolated/.config/zehn/favorites")
        );
    }

    #[test]
    fn production_profiles_follow_zehn_own_environment_contract() {
        let mut paths = crate::paths::get_gxserver_paths(None);
        paths.isolated_agent_home_dir = None;
        paths.home_dir = PathBuf::from("/Users/example");
        let (home, _cache_root, favorites_path) = resolve_search_paths(&paths);
        assert_eq!(home, "/Users/example");
        // Matches ghostex_find::favorites::path_for, so `gx f` and the GUI share stars.
        let expected = match std::env::var("XDG_CONFIG_HOME") {
            Ok(dir) if !dir.is_empty() => PathBuf::from(dir).join("zehn").join("favorites"),
            _ => PathBuf::from("/Users/example/.config/zehn/favorites"),
        };
        assert_eq!(favorites_path, expected);
    }

    #[test]
    fn launch_plans_serialize_the_fields_hosts_need() {
        let focus = launch_plan_payload(
            "00000000000000ab",
            &PromptLaunchPlan::Focus {
                project_id: "p".into(),
                session_id: "s".into(),
            },
        );
        assert_eq!(focus["mode"], "focus");
        assert_eq!(focus["projectId"], "p");
        assert_eq!(focus["key"], "00000000000000ab");

        let launch = launch_plan_payload(
            "00000000000000cd",
            &PromptLaunchPlan::Launch {
                agent: Agent::Codex,
                command: vec!["codex".into(), "resume".into(), "id".into()],
                cwd: "/tmp/x".into(),
                cwd_exists: false,
                title: "T".into(),
            },
        );
        assert_eq!(launch["mode"], "launch");
        assert_eq!(launch["agent"], "codex");
        assert_eq!(launch["commandLine"], "codex resume id");
        assert_eq!(launch["cwdExists"], false);
    }
}
