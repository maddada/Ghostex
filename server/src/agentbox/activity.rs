//! Activity, attention and titles for box sessions. A boxed agent fires no Ghostex hooks and its
//! terminal title carries no spinner, so this poller reads the box session's screen (primary) and
//! `agentbox list -g --json` (supplement) and feeds the result through `/api/updateAgentActivity`,
//! the same path that drives a local agent's sidebar dot, notifications and Done state.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};
use tokio::sync::Notify;

use super::session::{patch_agentbox_record, session_agentbox};
use super::status::{list_boxes, ListedBox};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;
use crate::storage::open_gxserver_database;

/// One poll of every running box session.
const TICK: Duration = Duration::from_secs(5);
/// With no running box session the loop only waits for `wake_agentbox_activity_poller`.
const IDLE_WAIT: Duration = Duration::from_secs(60);
/// `agentbox list` runs at most this often, backing off up to `LIST_MAX_INTERVAL` while its answer
/// does not change.
const LIST_MIN_INTERVAL: Duration = Duration::from_secs(5);
const LIST_MAX_INTERVAL: Duration = Duration::from_secs(30);
const LIST_FAILURE_MAX_INTERVAL: Duration = Duration::from_secs(120);
/// How old a box list may be for its "idle" to settle a session whose screen says nothing.
const LIST_FRESH_FOR_IDLE: Duration = Duration::from_secs(30);
/// A box list older than this is not used for attention.
const LIST_STALE_AFTER: Duration = Duration::from_secs(90);
/// A stored "working" the screen has shown idle for this long is settled to idle.
const STALE_WORKING_MS: i64 = 30_000;
/// Rows of the live grid searched for the working status line.
const STATUS_TAIL_ROWS: usize = 14;

fn wake() -> &'static Notify {
    static WAKE: OnceLock<Notify> = OnceLock::new();
    WAKE.get_or_init(Notify::new)
}

/// Starts the next poll now (a box session was just created or started).
pub(crate) fn wake_agentbox_activity_poller() {
    wake().notify_one();
}

/// What the box's screen shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    /// The agent's working status line (`esc to interrupt`).
    Working,
    /// A question, approval or picker owns the agent's input.
    Dialog,
    /// The agent's input box, idle.
    Ready,
    /// Nothing the classifier recognises: the box is starting, an install wizard, a shell.
    Unknown,
}

/// The session state this poller last reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Observed {
    Working,
    NeedsInput,
    Idle,
}

#[derive(Default)]
struct SessionMemory {
    last: Option<Observed>,
    /// Consecutive idle readings after working: one is not enough to call the turn done.
    idle_streak: u8,
}

#[derive(Default)]
struct Poller {
    sessions: HashMap<(String, String), SessionMemory>,
    boxes: Option<(Vec<ListedBox>, Instant)>,
    list_due: Option<Instant>,
    list_interval: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TitleKind {
    Placeholder,
    Other,
}

struct BoxSession {
    project_id: String,
    session_id: String,
    zmx_name: String,
    agent: String,
    box_name: String,
    /// `runtimeSettings.agentbox.created`: the poller has seen the box in `agentbox list`.
    created: bool,
    activity: String,
    working_source: Option<String>,
    attention_source: Option<String>,
    /// When the stored activity last changed, in epoch milliseconds.
    activity_changed_ms: Option<i64>,
    title: String,
    /// The title is the launcher's placeholder or an earlier automatic title, so the agent's own
    /// session title may replace it; a title the user or the title generator chose is kept.
    title_is_automatic: bool,
    title_kind: TitleKind,
    cwd: Option<String>,
}

fn read_box_session(session: &Value) -> Option<BoxSession> {
    let agentbox = session_agentbox(session)?;
    let text = |pointer: &str| {
        session
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let title = text("/title").unwrap_or_default();
    let title_source = crate::agents::normalize_title_source(
        text("/runtimeSettings/titleSource").as_deref(),
        &title,
    );
    let auto_title_running =
        text("/runtimeSettings/gxserverFirstPromptAutoTitleStatus").as_deref() == Some("running");
    Some(BoxSession {
        project_id: text("/projectId")?,
        session_id: text("/sessionId")?,
        zmx_name: text("/zmxName")?,
        agent: agentbox.agent,
        box_name: agentbox.box_name,
        created: agentbox.created,
        activity: text("/runtimeSettings/agentActivity/activity")
            .unwrap_or_else(|| "idle".to_string()),
        working_source: text("/runtimeSettings/agentActivity/workingSource"),
        attention_source: text("/runtimeSettings/agentActivity/attentionSource"),
        activity_changed_ms: text("/runtimeSettings/agentActivity/lastChangedAt")
            .as_deref()
            .and_then(crate::session_status::parse_iso_ms),
        title,
        title_is_automatic: matches!(title_source.as_str(), "placeholder" | "terminal-auto")
            && !auto_title_running,
        title_kind: if title_source == "placeholder" {
            TitleKind::Placeholder
        } else {
            TitleKind::Other
        },
        cwd: text("/cwd"),
    })
}

/// The live grid without agentbox's own footer row (` agentbox ▸ <box> (<state>) …`).
fn agent_screen(grid: &str) -> String {
    grid.lines()
        .filter(|line| !line.contains("agentbox \u{25b8}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn classify_screen(agent: &str, grid: &str) -> Screen {
    let screen = agent_screen(grid);
    let working = screen
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(STATUS_TAIL_ROWS)
        .any(|line| line.contains("esc to interrupt"));
    if working && matches!(agent, "claude" | "codex") {
        return Screen::Working;
    }
    let dialog = match agent {
        "claude" => crate::session_chat_claude_dialog::detect_claude_dialog(&screen).is_some(),
        "codex" => {
            crate::session_chat_codex_dialog::detect_codex_dialog(&screen).is_some()
                || crate::session_chat_codex_blocking::detect_codex_blocking_screen(&screen)
                    .is_some()
        }
        _ => false,
    };
    if dialog {
        return Screen::Dialog;
    }
    match crate::session_chat_composer::detect_session_chat_composer_ready(Some(agent), &screen)
        .state
    {
        crate::session_chat_composer::SessionChatComposerState::Ready => Screen::Ready,
        _ => Screen::Unknown,
    }
}

/// The session state a reading means, given the previous one and agentbox's view of the agent.
///
/// CDXC:AgentBox 2026-10-01 WHY: the screen is the primary source. agentbox's `agentStatus` can report Codex "working" long after the turn ended (observed 2026-10-01: ten minutes on an idle `› Ask Codex` prompt), so for Claude and Codex, whose working status line and input box Ghostex already recognises, it only adds attention states (question, waiting, end-plan, error) when the screen cannot tell, and never overrides an idle screen with "working". OpenCode and Pi have no working-line reader here, so their state comes from agentbox.
fn observe(
    agent: &str,
    screen: Screen,
    previous: Option<Observed>,
    box_activity: Option<&str>,
    fresh_idle: bool,
) -> Option<Observed> {
    let box_attention = matches!(
        box_activity,
        Some("question" | "waiting" | "end-plan" | "error")
    );
    if matches!(agent, "claude" | "codex") {
        return match screen {
            Screen::Working => Some(Observed::Working),
            // A picker the user opened from an idle prompt is not the agent asking anything.
            Screen::Dialog
                if box_attention
                    || matches!(previous, Some(Observed::Working | Observed::NeedsInput)) =>
            {
                Some(Observed::NeedsInput)
            }
            Screen::Dialog | Screen::Ready => Some(Observed::Idle),
            Screen::Unknown if box_attention => Some(Observed::NeedsInput),
            // The inline client dropped (a shell prompt, an error): a fresh "idle" from agentbox
            // settles the session instead of leaving it working with nothing on screen to say so.
            Screen::Unknown if fresh_idle => Some(Observed::Idle),
            Screen::Unknown => None,
        };
    }
    match box_activity {
        Some("working" | "compacting") => Some(Observed::Working),
        _ if box_attention => Some(Observed::NeedsInput),
        Some("idle") => Some(Observed::Idle),
        _ if screen == Screen::Ready => Some(Observed::Idle),
        _ => None,
    }
}

/// The `/api/updateAgentActivity` a change of reading sends, if any: `(activity, turn complete)`.
fn activity_update(
    session: &BoxSession,
    previous: Option<Observed>,
    observed: Observed,
) -> Option<(&'static str, bool)> {
    let stored = session.activity.as_str();
    match observed {
        // An Escape that interrupted nothing leaves the claim title-sourced, which the stale-title
        // rule would close; the screen still shows the turn, so it is claimed again.
        Observed::Working => (stored != "working"
            || session.working_source.as_deref() != Some("explicit"))
        .then_some(("working", false)),
        Observed::NeedsInput => (stored != "attention").then_some(("attention", false)),
        Observed::Idle => match (previous, stored) {
            // A turn that ended: Done, like a local agent's Stop hook. An Escape from the pane
            // already demoted the working claim to the title, which is an interrupt, not a finish.
            (Some(Observed::Working) | None, "working") => {
                Some(if session.working_source.as_deref() == Some("explicit") {
                    ("attention", true)
                } else {
                    ("idle", false)
                })
            }
            (Some(Observed::NeedsInput), "attention")
                if session.attention_source.as_deref() != Some(TURN_COMPLETE_ATTENTION_SOURCE) =>
            {
                Some(("idle", false))
            }
            // A working claim the screen has not backed for a while (set before this poller saw
            // the session, or never followed by a turn) settles to idle rather than staying on.
            (Some(Observed::Idle), "working")
                if session.activity_changed_ms.is_some_and(|changed| {
                    chrono::Utc::now().timestamp_millis() - changed > STALE_WORKING_MS
                }) =>
            {
                Some(("idle", false))
            }
            _ => None,
        },
    }
}

impl Poller {
    async fn refresh_boxes(&mut self, home: PathBuf) {
        let now = Instant::now();
        if self.list_due.is_some_and(|due| now < due) {
            return;
        }
        let listed = tokio::task::spawn_blocking(move || list_boxes(&home))
            .await
            .ok()
            .and_then(Result::ok);
        // An unchanged answer backs off to LIST_MAX_INTERVAL; a failure (agentbox missing, an
        // unreachable remote host timing out) backs off further, so a broken host is not retried
        // every few seconds.
        self.list_interval = match (&listed, &self.boxes) {
            (None, _) => {
                (self.list_interval.max(LIST_MIN_INTERVAL) * 2).min(LIST_FAILURE_MAX_INTERVAL)
            }
            (Some(listed), Some((previous, _)))
                if listed == previous && !self.list_interval.is_zero() =>
            {
                (self.list_interval * 2).min(LIST_MAX_INTERVAL)
            }
            _ => LIST_MIN_INTERVAL,
        };
        self.list_due = Some(Instant::now() + self.list_interval);
        if let Some(listed) = listed {
            self.boxes = Some((listed, Instant::now()));
        }
    }

    fn listed_box(&self, box_name: &str) -> Option<&ListedBox> {
        let (boxes, at) = self.boxes.as_ref()?;
        if at.elapsed() > LIST_STALE_AFTER {
            return None;
        }
        boxes.iter().find(|listed| listed.name == box_name)
    }

    fn list_is_fresh(&self) -> bool {
        self.boxes
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() <= LIST_FRESH_FOR_IDLE)
    }

    async fn tick(&mut self, state: &Arc<AppState>, sessions: Vec<BoxSession>) {
        self.sessions.retain(|key, _| {
            sessions
                .iter()
                .any(|session| session.project_id == key.0 && session.session_id == key.1)
        });
        self.refresh_boxes(state.paths.home_dir.clone()).await;
        for session in sessions {
            let zmx_name = session.zmx_name.clone();
            let Ok(Ok(capture)) = tokio::task::spawn_blocking(move || {
                crate::zmx::read_zmx_session_grid_capture(&zmx_name)
            })
            .await
            else {
                continue;
            };
            let listed = self.listed_box(&session.box_name).cloned();
            if listed.is_some() && !session.created {
                mark_box_created(state, &session).await;
            }
            let fresh_idle = self.list_is_fresh()
                && listed
                    .as_ref()
                    .and_then(|listed| listed.activity.as_deref())
                    == Some("idle");
            let screen = classify_screen(&session.agent, &capture.text);
            let key = (session.project_id.clone(), session.session_id.clone());
            let memory = self.sessions.entry(key).or_default();
            let previous = memory.last;
            let Some(mut observed) = observe(
                &session.agent,
                screen,
                previous,
                listed
                    .as_ref()
                    .and_then(|listed| listed.activity.as_deref()),
                fresh_idle,
            ) else {
                continue;
            };
            if observed == Observed::Idle && previous == Some(Observed::Working) {
                memory.idle_streak = memory.idle_streak.saturating_add(1);
                if memory.idle_streak < 2 {
                    observed = Observed::Working;
                }
            } else {
                memory.idle_streak = 0;
            }
            if let Some((activity, turn_complete)) = activity_update(&session, previous, observed) {
                send_activity(state, &session, activity, turn_complete).await;
            }
            memory.last = Some(observed);
            if !session.title_is_automatic {
                continue;
            }
            let title = listed
                .as_ref()
                .and_then(|listed| listed.session_title.as_deref())
                .and_then(crate::agents::get_visible_terminal_title)
                .filter(|title| !is_non_conversation_title(title, &session, None));
            let adopt = title.as_ref().is_some_and(|title| title != &session.title);
            // A non-conversation title adopted earlier goes back to the placeholder.
            let restore = title.is_none()
                && session.title_kind != TitleKind::Placeholder
                && is_non_conversation_title(&session.title, &session, None);
            if adopt || restore {
                let state = state.clone();
                let (project_id, session_id) =
                    (session.project_id.clone(), session.session_id.clone());
                let _ = tokio::task::spawn_blocking(move || {
                    apply_box_title(&state, &project_id, &session_id, title.as_deref())
                })
                .await;
            }
        }
    }
}

/// Records that the box exists, so restores reattach to it from now on (restore.rs).
async fn mark_box_created(state: &Arc<AppState>, session: &BoxSession) {
    let state = state.clone();
    let (project_id, session_id) = (session.project_id.clone(), session.session_id.clone());
    let _ = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&state.paths).ok()?;
        patch_agentbox_record(&db, &project_id, &session_id, "created", &json!(true), None).ok()
    })
    .await;
}

async fn send_activity(
    state: &Arc<AppState>,
    session: &BoxSession,
    activity: &'static str,
    turn_complete: bool,
) {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(session.project_id));
    params.insert("sessionId".to_string(), json!(session.session_id));
    params.insert("agentName".to_string(), json!(session.agent));
    params.insert("activity".to_string(), json!(activity));
    if turn_complete {
        params.insert(
            "attentionSource".to_string(),
            json!(TURN_COMPLETE_ATTENTION_SOURCE),
        );
    }
    let state = state.clone();
    let _ = tokio::task::spawn_blocking(move || {
        crate::server::dispatch_agent_http_blocking(
            &state,
            "/api/updateAgentActivity".to_string(),
            "agentbox-activity".to_string(),
            params,
        )
    })
    .await;
}

/// Whether a title names no conversation: the box's working folder (`/workspace`), the box, the
/// agent, or the project folder. agentbox reports such a `sessionTitle` (observed 2026-10-01:
/// "workspace" for a Codex box with no thread yet) until the agent names its thread.
fn is_non_conversation_title(
    title: &str,
    session: &BoxSession,
    project_folder: Option<&str>,
) -> bool {
    let normalized = title.trim().trim_matches('/').to_lowercase();
    let folder = |path: Option<&str>| {
        path.and_then(|path| {
            path.trim_end_matches(['/', '\\'])
                .rsplit(['/', '\\'])
                .next()
        })
        .map(str::to_lowercase)
    };
    normalized.is_empty()
        || normalized == "workspace"
        || normalized == session.box_name
        || normalized == session.agent
        || crate::agents::default_agent_session_title_name(&session.agent)
            .is_some_and(|name| normalized == name.to_lowercase())
        || folder(session.cwd.as_deref()).as_deref() == Some(normalized.as_str())
        || folder(project_folder).as_deref() == Some(normalized.as_str())
}

/// Names a box session after the agent's own session title (agentbox's `sessionTitle`, Codex's
/// thread name) while its title is automatic, as a local Codex session follows its thread name.
/// With no usable title, an automatic title that names no conversation goes back to the
/// launcher's placeholder ("Codex Session").
fn apply_box_title(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    title: Option<&str>,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    if !write_box_title(state, &db, project_id, session_id, title)? {
        return Ok(());
    }
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    crate::server::schedule_presentation_session_delta(
        state,
        &db,
        &repository,
        project_id,
        session_id,
    )
}

/// The read-modify-write of `apply_box_title`, in one IMMEDIATE transaction so a hook, title or
/// activity write between the read and the write is not overwritten. `true` when it changed.
fn write_box_title(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    title: Option<&str>,
) -> Result<bool, DomainStateError> {
    let transaction =
        rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("SQLite gxserver state error: {error}"),
            })?;
    let repository = DomainRepository::new(&transaction, state.metadata.server_id.as_str());
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(false);
    };
    let Some(box_session) = read_box_session(&session) else {
        return Ok(false);
    };
    if !box_session.title_is_automatic {
        return Ok(false);
    }
    let project_path = repository.get_project(project_id)?.and_then(|project| {
        project
            .get("path")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let title = title
        .map(|title| title.trim().chars().take(180).collect::<String>())
        .filter(|title| !is_non_conversation_title(title, &box_session, project_path.as_deref()));
    let (title, title_source) = match title {
        Some(title) => (title, "terminal-auto"),
        None if box_session.title_kind != TitleKind::Placeholder
            && is_non_conversation_title(
                &box_session.title,
                &box_session,
                project_path.as_deref(),
            ) =>
        {
            (
                crate::agents::create_agent_session_default_title(None, Some(&box_session.agent)),
                "placeholder",
            )
        }
        None => return Ok(false),
    };
    if title == box_session.title {
        return Ok(false);
    }
    let mut runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.insert("titleSource".to_string(), json!(title_source));
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert("title".to_string(), json!(title));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update)?;
    drop(repository);
    transaction.commit().map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    Ok(true)
}

fn running_box_sessions(state: &AppState) -> Vec<BoxSession> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository
        .list_agentbox_sessions(true)
        .unwrap_or_default()
        .iter()
        .filter_map(read_box_session)
        .collect()
}

/// Starts the box activity poller: one tick every few seconds while a box session runs, a single
/// `agentbox list` in flight at a time, and nothing at all while no box session runs.
pub(crate) fn start_agentbox_activity_poller(state: Arc<AppState>) {
    if cfg!(windows) {
        return;
    }
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut poller = Poller::default();
        loop {
            let reader = state.clone();
            let sessions = tokio::task::spawn_blocking(move || running_box_sessions(&reader))
                .await
                .unwrap_or_default();
            let delay = if sessions.is_empty() {
                poller = Poller::default();
                IDLE_WAIT
            } else {
                poller.tick(&state, sessions).await;
                TICK
            };
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(delay) => {}
                _ = wake().notified() => {}
            }
        }
    });
}
