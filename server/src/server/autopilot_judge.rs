//! Autopilot's judgment agent (the supervisor): one agent session Ghostex starts the first time
//! something needs judgment, in an "Autopilot" project whose folder holds its notes
//! (server/src/autopilot/notes.rs). The watcher queues what no rule settles (a question with no
//! recommended choice, a session the rules left for the user); when the supervisor is free, Ghostex
//! writes a wake file and sends it one message. It acts only through `ghostex autopilot decide`,
//! which gxserver applies and logs (autopilot_http.rs).
//!
//! CDXC:Autopilot 2026-11-07 DECISION:
//! User: the supervisor "watch and decide" (Ghostex watches events and sweeps; an agent wakes only for judgment calls), on Opus 5.5 at medium or "sol 6.1 high", and "keep in mind we need to use all sessions/models available to launch the agent to check things when required": it starts on the picked model when that agent has an account with room, else on the other one (Q1601: "Any account with room", Ghostex's saved accounts and Claude Swap / Codex Swap), and a new session's account is picked the way every new session's is, the one with the most room.
//! SEE-ALSO: server/src/autopilot/notes.rs (its notes and playbook), server/src/server/autopilot_runtime.rs (what it is woken for).

use super::*;

use std::sync::OnceLock;

use axum::body::Body;
use axum::http::{header, Method, Request};

use crate::autopilot::{self, AutopilotConfig, SupervisorModel};
use crate::coordinators::SessionKey;
use crate::presentation::presentation_activity;

const SUPERVISOR_FILE: &str = "autopilot-supervisor.json";
const SUPERVISOR_TITLE: &str = "Autopilot supervisor";
/// Least time between two wakes.
const WAKE_GAP_MS: i64 = 60_000;
/// A wake that the supervisor has not finished after this long lets the next one through.
const WAKE_STALE_MS: i64 = 20 * 60_000;

/// One thing that needs judgment.
#[derive(Clone, Debug)]
pub(super) struct JudgmentItem {
    pub key: SessionKey,
    pub title: String,
    pub project: String,
    /// Why it needs the supervisor, in a line.
    pub why: String,
    /// What Ghostex saw (the question and its choices, the last words).
    pub detail: String,
}

#[derive(Default)]
struct Judge {
    queue: Vec<JudgmentItem>,
    /// Items already handed to a wake, by session and reason, so each goes once.
    handed: HashSet<(SessionKey, String)>,
    last_wake_ms: i64,
    wakes: u64,
    waking: bool,
}

fn judge() -> &'static Mutex<Judge> {
    static JUDGE: OnceLock<Mutex<Judge>> = OnceLock::new();
    JUDGE.get_or_init(Default::default)
}

/// Queues an item for the next wake, once per session and reason.
pub(super) fn queue_judgment(item: JudgmentItem) {
    let Ok(mut judge) = judge().lock() else {
        return;
    };
    let id = (item.key.clone(), item.why.clone());
    if judge.handed.contains(&id)
        || judge
            .queue
            .iter()
            .any(|queued| (queued.key.clone(), queued.why.clone()) == id)
    {
        return;
    }
    judge.queue.push(item);
}

/// Forgets what was handed for a session (it moved on), so a new stop can be judged again.
pub(super) fn forget_session(key: &SessionKey) {
    if let Ok(mut judge) = judge().lock() {
        judge.handed.retain(|(handed, _)| handed != key);
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SupervisorRecord {
    project_id: String,
    session_id: String,
    agent: String,
}

fn read_record(paths: &crate::paths::GxserverPaths) -> Option<SupervisorRecord> {
    std::fs::read_to_string(paths.app_state_dir.join(SUPERVISOR_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn write_record(paths: &crate::paths::GxserverPaths, record: &SupervisorRecord) {
    let _ = std::fs::write(
        paths.app_state_dir.join(SUPERVISOR_FILE),
        serde_json::to_string_pretty(record).unwrap_or_default(),
    );
}

/// The supervisor's session: Autopilot leaves it out of everything else.
pub(crate) fn supervisor_key(paths: &crate::paths::GxserverPaths) -> Option<SessionKey> {
    read_record(paths).map(|record| (record.project_id, record.session_id))
}

/// A gxserver call made from inside gxserver, through the same route and checks a client's goes.
pub(super) async fn internal_rpc(
    state: &Arc<AppState>,
    path: &str,
    params: Value,
) -> std::result::Result<Value, String> {
    let body = json!({ "protocolVersion": GXSERVER_PROTOCOL_VERSION, "params": params });
    let request = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", state.auth_token),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            "x-gxserver-protocol-version",
            GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .body(Body::from(body.to_string()))
        .map_err(|error| error.to_string())?;
    let routed =
        super::route_http::route_http(state.clone(), request, Uuid::new_v4().to_string()).await;
    let ok = routed.response.status().is_success();
    let bytes = to_bytes(routed.response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap_or_default();
    let answer: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if ok {
        Ok(answer.get("result").cloned().unwrap_or(answer))
    } else {
        Err(answer
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| String::from_utf8_lossy(&bytes).chars().take(240).collect()))
    }
}

/// The supervisor's session when it is there and not stopped.
fn live_supervisor(state: &AppState) -> Option<(SupervisorRecord, Value)> {
    let record = read_record(&state.paths)?;
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository
        .get_session(&record.project_id, &record.session_id)
        .ok()??;
    (session["lifecycleState"].as_str() != Some("stopped")).then_some((record, session))
}

/// Starts the supervisor: the user's model when its agent has an account with room, else the
/// other one.
async fn start_supervisor(
    state: &Arc<AppState>,
    config: &AutopilotConfig,
) -> std::result::Result<SupervisorRecord, String> {
    let dir = autopilot::ensure_notes(&state.paths).map_err(|error| error.to_string())?;
    let mut pick = config.supervisor;
    if !crate::accounts::autopilot::agent_has_room(pick.launch().0)
        && crate::accounts::autopilot::agent_has_room(pick.other().launch().0)
    {
        pick = pick.other();
    }
    let (agent, model, effort) = SupervisorModel::launch(pick);
    let project = internal_rpc(
        state,
        "/api/addProjectPath",
        json!({ "name": "Autopilot", "path": dir.to_string_lossy() }),
    )
    .await?;
    let project_id = project
        .pointer("/project/projectId")
        .and_then(Value::as_str)
        .ok_or("Ghostex couldn't open the Autopilot project.")?
        .to_string();
    let created = internal_rpc(
        state,
        "/api/createAgentSession",
        json!({
            "projectId": project_id,
            "agentId": agent,
            "agentModel": model,
            "agentEffort": effort,
            "title": SUPERVISOR_TITLE,
        }),
    )
    .await?;
    let session_id = created
        .pointer("/session/sessionId")
        .and_then(Value::as_str)
        .ok_or("Ghostex couldn't start the Autopilot supervisor.")?
        .to_string();
    let record = SupervisorRecord {
        project_id,
        session_id,
        agent: agent.to_string(),
    };
    write_record(&state.paths, &record);
    Ok(record)
}

/// Wakes the supervisor when something waits for it and it is free. Called after every tick.
pub(super) async fn maybe_wake(state: Arc<AppState>) {
    let config = autopilot::read_config(&state.paths);
    let now = chrono::Utc::now().timestamp_millis();
    let items = {
        let Ok(mut judge) = judge().lock() else {
            return;
        };
        if !config.enabled {
            judge.queue.clear();
            return;
        }
        // Offline, the supervisor could not reach its model: its items wait for the network.
        if crate::network_status::is_offline() {
            return;
        }
        if judge.queue.is_empty() || judge.waking || now - judge.last_wake_ms < WAKE_GAP_MS {
            return;
        }
        judge.waking = true;
        std::mem::take(&mut judge.queue)
    };
    let woke = wake(&state, &config, &items, now).await;
    let Ok(mut judge) = judge().lock() else {
        return;
    };
    judge.waking = false;
    match woke {
        Ok(n) => {
            judge.last_wake_ms = now;
            judge.wakes = n;
            for item in &items {
                judge.handed.insert((item.key.clone(), item.why.clone()));
            }
        }
        // Busy, or it could not start: the items wait for the next try.
        Err(_) => {
            let mut items = items;
            items.append(&mut judge.queue);
            judge.queue = items;
        }
    }
}

async fn wake(
    state: &Arc<AppState>,
    config: &AutopilotConfig,
    items: &[JudgmentItem],
    now: i64,
) -> std::result::Result<u64, String> {
    let record = match live_supervisor(state) {
        Some((record, session)) => {
            let busy =
                presentation_activity(&session, &crate::presentation::now_iso()) == "working";
            let last = judge().lock().map(|judge| judge.last_wake_ms).unwrap_or(0);
            if busy && now - last < WAKE_STALE_MS {
                return Err("busy".to_string());
            }
            record
        }
        None => start_supervisor(state, config).await?,
    };
    let dir = autopilot::ensure_notes(&state.paths).map_err(|error| error.to_string())?;
    let n = autopilot::next_wake_number(&dir);
    let body = wake_body(config, items, n);
    let path = autopilot::write_wake(&dir, n, &body).map_err(|error| error.to_string())?;
    let cli = crate::ghostex_cli::launchers::sibling_cli_executable();
    let message = format!(
        "Ghostex Autopilot, wake {n}. First read {readme} and {notes}, then the session notes it names, then handle {wake}. Run Ghostex's CLI as `{cli}` (where the notes say `ghostex`): it is the one this Ghostex ships.",
        readme = dir.join("README.md").display(),
        notes = dir.join("notes.md").display(),
        wake = path.display(),
    );
    super::autopilot_runtime::send_text(
        state,
        &(record.project_id.clone(), record.session_id.clone()),
        &message,
    )
    .await?;
    Ok(n)
}

/// What one wake asks: what the supervisor may and may not do, then each session.
fn wake_body(config: &AutopilotConfig, items: &[JudgmentItem], n: u64) -> String {
    let limits = &config.limits;
    let not_allowed: Vec<&str> = [
        (!limits.push_branch, "push a branch"),
        (!limits.finish_release, "finish a release"),
        (!limits.push_main, "push main"),
        (!limits.merge, "merge pull requests"),
        (!limits.delete_outside_project, "delete outside the project"),
        (!limits.post_publicly, "post publicly"),
    ]
    .into_iter()
    .filter_map(|(blocked, what)| blocked.then_some(what))
    .collect();
    let mut body = format!(
        "# Wake {n} ({})\n\nNot allowed without the user: {}; and never sign-ins, 2FA, identity checks or payments.\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
        if not_allowed.is_empty() {
            "nothing beyond the never list".to_string()
        } else {
            not_allowed.join(", ")
        }
    );
    for item in items {
        body.push_str(&format!(
            "\n## {title} ({project})\n\nSession: `{p}` `{s}` (ref `{p}:{s}`). Its notes: `sessions/{p}-{s}.md`.\n\nWhy you: {why}\n\n{detail}\n",
            title = item.title,
            project = item.project,
            p = item.key.0,
            s = item.key.1,
            why = item.why,
            detail = item.detail,
        ));
    }
    body
}

/// The supervisor as Settings > Autopilot shows it: its session, how many wakes, the last one, and
/// what waits for it.
pub(crate) fn supervisor_view(paths: &crate::paths::GxserverPaths) -> Value {
    let record = read_record(paths);
    let (wakes, last_wake_ms, queued) = judge()
        .lock()
        .map(|judge| (judge.wakes, judge.last_wake_ms, judge.queue.len()))
        .unwrap_or_default();
    json!({
        "projectId": record.as_ref().map(|record| record.project_id.clone()),
        "sessionId": record.as_ref().map(|record| record.session_id.clone()),
        "agent": record.as_ref().map(|record| record.agent.clone()),
        "wakes": wakes,
        "lastWakeAt": (last_wake_ms > 0)
            .then(|| crate::coordinators::iso_from_ms(last_wake_ms))
            .flatten(),
        "waiting": queued,
        "notesDir": crate::autopilot::notes_dir(paths).to_string_lossy(),
    })
}
