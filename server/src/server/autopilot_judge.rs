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

use crate::autopilot::{self, AutopilotConfig, SupervisorPick};
use crate::coordinators::SessionKey;
use crate::presentation::presentation_activity;

use crate::autopilot::{SupervisorRecord, SUPERVISOR_TITLE};
/// Least time between two wakes.
const WAKE_GAP_MS: i64 = 60_000;
/// A wake that the supervisor has not finished after this long lets the next one through.
const WAKE_STALE_MS: i64 = 20 * 60_000;
/// `wake`'s answer while the supervisor is still on an earlier wake: no failure.
const BUSY: &str = "busy";
/// `wake`'s answer while every eligible supervisor account pool is at 100%: park quietly without
/// spamming failure logs or notifications until an account resets (Q4471).
const PARKED_NO_USAGE: &str = "parkedNoUsage";
/// How long to wait between quiet checks when every supervisor pool is empty (a reset resets this
/// immediately via `setup_key`).
const NO_USAGE_RETRY_MS: i64 = 2 * 60_000;
/// The supervisor's context at and above which Ghostex compacts it between wakes, in tokens.
const COMPACT_AT_TOKENS: u64 = 120_000;
/// Least time between two looks at its context, and after a compaction before the next.
const COMPACT_CHECK_MS: i64 = 60_000;
const COMPACT_SETTLE_MS: i64 = 10 * 60_000;
/// A wake this recent may still be starting its turn: no compaction yet.
const COMPACT_AFTER_WAKE_MS: i64 = 2 * 60_000;
/// How long Autopilot waits before trying again after the 1st, 2nd, … failed start or wake; the
/// last one repeats.
const RETRY_AFTER_MS: &[i64] = &[10_000, 30_000, 60_000, 5 * 60_000, 15 * 60_000];

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
    /// The last failure to start or wake it, logged once until it changes or a wake gets through.
    failure: Option<String>,
    /// Failed starts or wakes in a row, and when the next try may go.
    failures: usize,
    retry_at_ms: i64,
    /// The settings and accounts the failures happened under: a change tries again at once.
    setup: String,
    /// The last look at the supervisor's context, and the last compaction sent.
    compact_checked_ms: i64,
    compacted_ms: i64,
    /// Its context is too big and its agent can't compact: the next wake starts a new session.
    start_fresh: bool,
    /// Already logged once that every supervisor account pool is at its usage limit.
    parked_no_usage: bool,
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

fn read_record(paths: &crate::paths::GxserverPaths) -> Option<SupervisorRecord> {
    autopilot::read_supervisor_record(paths)
}

fn write_record(paths: &crate::paths::GxserverPaths, record: &SupervisorRecord) {
    autopilot::write_supervisor_record(paths, record);
}

/// The supervisor's session: Autopilot leaves it out of everything else.
pub(crate) fn supervisor_key(paths: &crate::paths::GxserverPaths) -> Option<SessionKey> {
    read_record(paths).map(|record| (record.project_id, record.session_id))
}

/// A gxserver call made from inside gxserver, through the same route and checks a client's goes.
pub(crate) async fn internal_rpc(
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

/// Whether an agent can start as the supervisor: it needs a "run without asking" Ghostex can turn
/// on, since nobody answers the supervisor's own cards. A custom agent (`custom-…`) is let through:
/// its launch takes the one of the agent its icon names.
pub(crate) fn supervisor_agent_allowed(agent: &str) -> bool {
    let agent = agent.trim().to_ascii_lowercase();
    if agent.starts_with("custom-") {
        return true;
    }
    let family = crate::agents::identity::normalize_agent_id(Some(&agent)).unwrap_or(agent);
    crate::agents::accept_all_spec(&family).is_some()
}

/// The picks the supervisor may start on, the chosen one first, without agents that can't run
/// without asking.
fn supervisor_candidates(config: &AutopilotConfig) -> Vec<SupervisorPick> {
    config
        .supervisor_candidates()
        .into_iter()
        .filter(|pick| supervisor_agent_allowed(&pick.agent))
        .collect()
}

fn pick_has_room(paths: &crate::paths::GxserverPaths, pick: &SupervisorPick) -> bool {
    crate::accounts::autopilot::agent_has_room(paths, &pick.agent, pick.model())
}

/// Starts a supervisor session on `pick`, with its agent running without asking.
async fn start_supervisor_for(
    state: &Arc<AppState>,
    pick: &SupervisorPick,
) -> std::result::Result<SupervisorRecord, String> {
    let dir = autopilot::ensure_notes(&state.paths).map_err(|error| error.to_string())?;
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
    let mut params = json!({
        "projectId": project_id,
        "agentId": pick.agent,
        "title": SUPERVISOR_TITLE,
        "launchSettings": { "acceptAllMode": "enabled" },
        "runtimeSettings": { autopilot::SUPERVISOR_LAUNCH_KEY: true },
    });
    if let Some(model) = pick.model() {
        params["agentModel"] = json!(model);
    }
    if let Some(effort) = pick.effort() {
        params["agentEffort"] = json!(effort);
    }
    let created = internal_rpc(state, "/api/createAgentSession", params).await?;
    let session_id = created
        .pointer("/session/sessionId")
        .and_then(Value::as_str)
        .ok_or("Ghostex couldn't start the Autopilot supervisor.")?
        .to_string();
    let record = SupervisorRecord {
        project_id,
        session_id,
        agent: pick.agent.clone(),
        model: pick.model.clone(),
    };
    write_record(&state.paths, &record);
    Ok(record)
}

/// Wakes the supervisor when something waits for it and it is free. Called after every tick.
pub(super) async fn maybe_wake(state: Arc<AppState>) {
    let config = autopilot::read_config(&state.paths);
    let now = chrono::Utc::now().timestamp_millis();
    let should_check_setup = {
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
        !judge.queue.is_empty() && !judge.waking
    };
    if !should_check_setup {
        return;
    }
    let setup = {
        let paths = state.paths.clone();
        let config_for_key = config.clone();
        tokio::task::spawn_blocking(move || setup_key(&paths, &config_for_key))
            .await
            .unwrap_or_default()
    };
    let items = {
        let Ok(mut judge) = judge().lock() else {
            return;
        };
        if !config.enabled {
            judge.queue.clear();
            return;
        }
        if judge.setup != setup {
            judge.setup = setup;
            judge.failures = 0;
            judge.retry_at_ms = 0;
            judge.parked_no_usage = false;
        }
        if judge.queue.is_empty()
            || judge.waking
            || now - judge.last_wake_ms < WAKE_GAP_MS
            || now < judge.retry_at_ms
        {
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
            judge.failure = None;
            judge.failures = 0;
            judge.retry_at_ms = 0;
            judge.parked_no_usage = false;
            for item in &items {
                judge.handed.insert((item.key.clone(), item.why.clone()));
            }
        }
        // Every supervisor agent's eligible account pool is full: park quietly until an account
        // resets (`setup_key` flips as soon as room returns).
        Err(error) if error == PARKED_NO_USAGE => {
            judge.retry_at_ms = now + NO_USAGE_RETRY_MS;
            if !judge.parked_no_usage {
                judge.parked_no_usage = true;
                autopilot::append_log(
                    &state.paths,
                    &autopilot::LogEntry {
                        at: crate::coordinators::iso_from_ms(now).unwrap_or_default(),
                        project_id: String::new(),
                        session_id: String::new(),
                        title: SUPERVISOR_TITLE.to_string(),
                        project: "Autopilot".to_string(),
                        saw: "Every auto-switchable supervisor account is at its usage limit."
                            .to_string(),
                        did: "Paused supervisor wakes until an eligible account resets."
                            .to_string(),
                        rule: "supervisor-limit".to_string(),
                        outcome: autopilot::Outcome::Waited,
                    },
                );
            }
            let mut items = items;
            items.append(&mut judge.queue);
            judge.queue = items;
        }
        // Busy, or it could not start: the items wait for the next try.
        Err(error) => {
            // A start or a wake that failed (no account with room, a model it can't launch, a send
            // that didn't land) shows in both logs, once per failure, so it isn't silent.
            if error != BUSY {
                let delay = RETRY_AFTER_MS[judge.failures.min(RETRY_AFTER_MS.len() - 1)];
                judge.failures += 1;
                judge.retry_at_ms = now + delay;
                if judge.failure.as_deref() != Some(error.as_str()) {
                    judge.failure = Some(error.clone());
                    log_failure(&state, &items, &error, delay, now);
                }
            }
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
    let fresh = judge()
        .lock()
        .map(|mut judge| std::mem::take(&mut judge.start_fresh))
        .unwrap_or(false);
    let candidates = supervisor_candidates(config);
    let record = match live_supervisor(state).filter(|_| !fresh) {
        Some((record, session)) => {
            let session_at_limit =
                crate::accounts::reset_used::waits_at_usage_limit(state, &session);
            // An agent Ghostex reads no accounts for (Antigravity and the rest) is out of room on
            // this model until the reset its own notice names.
            let noted_limit = session_at_limit
                && crate::accounts::autopilot::note_untracked_limit(
                    &record.agent,
                    record.model(),
                    crate::session_chat_options::cached_session_chat_terminal_notice(
                        state,
                        &record.project_id,
                        &record.session_id,
                    )
                    .as_ref()
                    .and_then(crate::session_chat_notice::parse_notice_reset_duration),
                    now,
                );
            let provider_has_room = !noted_limit
                && crate::accounts::autopilot::agent_has_room(
                    &state.paths,
                    &record.agent,
                    record.model(),
                );
            if session_at_limit && provider_has_room {
                // Another eligible account of this provider still has room: wait for account
                // recovery to switch the live supervisor rather than abandoning its agent or
                // parking as if the whole pool were empty.
                return Err(BUSY.to_string());
            }
            if provider_has_room {
                restore_title(state, &record, &session);
                let busy =
                    presentation_activity(&session, &crate::presentation::now_iso()) == "working";
                let last = judge().lock().map(|judge| judge.last_wake_ms).unwrap_or(0);
                if busy && now - last < WAKE_STALE_MS {
                    return Err(BUSY.to_string());
                }
                record
            } else {
                // CDXC:Autopilot 2026-11-22 DECISION (Q4471):
                // When the live supervisor's eligible account pool is empty (or it sits at a
                // usage limit), switch the Autopilot supervisor to another configured agent with
                // room (e.g. Codex) instead of waking a 100%-limited supervisor repeatedly. If no
                // supervisor agent has room, park quietly without spamming notifications.
                // CDXC:Autopilot 2026-11-26: the candidates are the picked agent, model and
                // effort and then its fallbacks (config.rs `supervisor_candidates`); another
                // model of the same agent counts, since an Antigravity or a Claude model-only
                // window can be at its limit while another model still has room.
                let fallback = candidates.iter().find(|candidate| {
                    !(candidate.agent == record.agent && candidate.model == record.model)
                        && pick_has_room(&state.paths, candidate)
                });
                let Some(fallback) = fallback else {
                    return Err(PARKED_NO_USAGE.to_string());
                };
                let old_agent = supervisor_name(&record.agent, record.model());
                if presentation_activity(&session, &crate::presentation::now_iso()) != "working" {
                    let _ = internal_rpc(
                        state,
                        "/api/sleepSession",
                        json!({
                            "projectId": record.project_id,
                            "sessionId": record.session_id,
                        }),
                    )
                    .await;
                }
                let switched = start_supervisor_for(state, fallback)
                    .await
                    .map_err(|error| format!("Couldn't switch the supervisor: {error}"))?;
                autopilot::append_log(
                    &state.paths,
                    &autopilot::LogEntry {
                        at: crate::coordinators::iso_from_ms(now).unwrap_or_default(),
                        project_id: switched.project_id.clone(),
                        session_id: switched.session_id.clone(),
                        title: SUPERVISOR_TITLE.to_string(),
                        project: "Autopilot".to_string(),
                        saw: format!("The {old_agent} supervisor is at its usage limit."),
                        did: format!(
                            "Switched the Autopilot supervisor to {} while {old_agent} waits for reset.",
                            supervisor_name(&switched.agent, switched.model())
                        ),
                        rule: "supervisor-switch".to_string(),
                        outcome: autopilot::Outcome::Done,
                    },
                );
                switched
            }
        }
        None => {
            if candidates.is_empty() {
                return Err(
                    "The supervisor's agent can't run commands without asking. Pick another one in Settings > Autopilot."
                        .to_string(),
                );
            }
            let Some(pick) = candidates
                .iter()
                .find(|candidate| pick_has_room(&state.paths, candidate))
            else {
                return Err(PARKED_NO_USAGE.to_string());
            };
            start_supervisor_for(state, pick)
                .await
                .map_err(|error| format!("Couldn't start the supervisor: {error}"))?
        }
    };
    let dir = autopilot::ensure_notes(&state.paths).map_err(|error| error.to_string())?;
    let n = autopilot::next_wake_number(&dir);
    let body = wake_body(config, items, n);
    let path = autopilot::write_wake(&dir, n, &body).map_err(|error| error.to_string())?;
    let key = (record.project_id.clone(), record.session_id.clone());
    let cli = crate::ghostex_cli::launchers::sibling_cli_executable();
    let message = format!(
        "Ghostex Autopilot, wake {n}. First read {readme} and {notes}, then the session notes it names, then handle {wake}. Run Ghostex's CLI as `{cli}` (where the notes say `ghostex`): it is the one this Ghostex ships.",
        readme = dir.join("README.md").display(),
        notes = dir.join("notes.md").display(),
        wake = path.display(),
    );
    if let Err(error) = send_to_supervisor(state, &key, &message).await {
        // A wake that didn't get through leaves no numbered file: the next try reuses the number.
        let _ = std::fs::remove_file(&path);
        return Err(format!("Couldn't wake the supervisor: {error}"));
    }
    Ok(n)
}

/// Sends the supervisor its wake. A new supervisor first shows its agent's "Trust this folder?"
/// for its own notes folder, which Ghostex made: Ghostex trusts it for the agent and sends.
///
/// CDXC:Autopilot 2026-11-18 WHY:
/// The first supervisor on the user's computer sat 17 minutes on Claude's folder-trust prompt for ~/.local/state/ghostex/autopilot: every wake was refused ("Trust this folder?. Answer it in chat before sending.") and every retry wrote another numbered wake file, so one judgment call made six. The folder is Ghostex's own, so answering the prompt here is no decision taken for the user; Codex's "Trust and continue" is answered the same way. A failed wake removes its file (`wake`).
async fn send_to_supervisor(
    state: &AppState,
    key: &SessionKey,
    message: &str,
) -> std::result::Result<(), String> {
    if let Some(params) = super::autopilot_answers::trust_folder_answer(state, key) {
        super::autopilot_runtime::answer(state, key, &params).await?;
        return super::autopilot_runtime::send_after_prompt_closes(state, key, message).await;
    }
    match super::autopilot_runtime::send_text(state, key, message).await {
        Err(error) if error.ends_with("Answer it in chat before sending.") => {
            let Some(params) = super::autopilot_answers::trust_folder_answer(state, key) else {
                return Err(error);
            };
            super::autopilot_runtime::answer(state, key, &params).await?;
            super::autopilot_runtime::send_after_prompt_closes(state, key, message).await
        }
        result => result,
    }
}

/// Compacts the supervisor between wakes once its context reaches 120K tokens. Called after every
/// tick; looks at most once a minute.
///
/// CDXC:Autopilot 2026-11-18 DECISION:
/// User: "also i would like that we compact that thread often like before it gets to 200k to keep cost on it low". While the supervisor is idle between wakes (never mid-wake: not while it works, nor in the two minutes after a wake was sent) and its last known context is 120K tokens or more, Ghostex types its agent's compact command, and logs it. Its notes files hold what it knows, so nothing is lost. An agent with no compact command gets a new supervisor session at the next wake instead (`start_fresh`).
pub(super) async fn maybe_compact(state: Arc<AppState>) {
    let config = autopilot::read_config(&state.paths);
    let now = chrono::Utc::now().timestamp_millis();
    {
        let Ok(mut judge) = judge().lock() else {
            return;
        };
        if !config.enabled
            || judge.waking
            || judge.start_fresh
            || now - judge.compact_checked_ms < COMPACT_CHECK_MS
            || now - judge.compacted_ms < COMPACT_SETTLE_MS
            || now - judge.last_wake_ms < COMPACT_AFTER_WAKE_MS
        {
            return;
        }
        judge.compact_checked_ms = now;
    }
    if crate::network_status::is_offline() {
        return;
    }
    let Some((record, session)) = live_supervisor(&state) else {
        return;
    };
    if presentation_activity(&session, &crate::presentation::now_iso()) == "working"
        || !crate::accounts::autopilot::agent_has_room(&state.paths, &record.agent, record.model())
        || crate::accounts::reset_used::waits_at_usage_limit(&state, &session)
    {
        return;
    }
    let (tokens, command) = {
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(tokens) = crate::carry_on_compaction::context_tokens(&repository, &session)
            .filter(|tokens| *tokens >= COMPACT_AT_TOKENS)
        else {
            return;
        };
        (
            tokens,
            crate::carry_on_compaction::compact_command(&session),
        )
    };
    let size = crate::carry_on_compaction::short_tokens(tokens);
    let (did, outcome) = match command {
        Some(command) => match crate::carry_on_compaction::send_like_coordinator(
            &state,
            &record.project_id,
            &record.session_id,
            command,
        )
        .await
        {
            Ok(()) => {
                if let Ok(mut judge) = judge().lock() {
                    judge.compacted_ms = now;
                }
                (
                    format!(
                        "Compacted it ({command}) between wakes; its notes keep what it knows."
                    ),
                    autopilot::Outcome::Done,
                )
            }
            Err(error) => (
                format!("Couldn't compact it: {error}."),
                autopilot::Outcome::Failed,
            ),
        },
        None => {
            if let Ok(mut judge) = judge().lock() {
                judge.start_fresh = true;
            }
            (
                "Its agent can't compact, so the next wake starts a new supervisor session; its notes keep what it knows.".to_string(),
                autopilot::Outcome::Done,
            )
        }
    };
    autopilot::append_log(
        &state.paths,
        &autopilot::LogEntry {
            at: crate::coordinators::iso_from_ms(now).unwrap_or_default(),
            project_id: record.project_id.clone(),
            session_id: record.session_id.clone(),
            title: SUPERVISOR_TITLE.to_string(),
            project: "Autopilot".to_string(),
            saw: format!("Its conversation reached {size} tokens."),
            did,
            rule: "supervisor-compact".to_string(),
            outcome,
        },
    );
}

/// Puts "Autopilot supervisor" back on a supervisor the agent renamed (Claude named the first ones
/// after their first message), unless the user renamed it. Runs when gxserver starts and on each
/// wake.
fn restore_title(state: &AppState, record: &SupervisorRecord, session: &Value) {
    if session["title"].as_str() == Some(SUPERVISOR_TITLE)
        || session
            .pointer("/runtimeSettings/pendingAgentTitleRequestTitleSource")
            .and_then(Value::as_str)
            == Some("user")
    {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.insert("titleSource".into(), json!("ghostex"));
    let updated = repository.update_session(
        json!({
            "projectId": record.project_id,
            "sessionId": record.session_id,
            "title": SUPERVISOR_TITLE,
            "runtimeSettings": runtime,
        })
        .as_object()
        .expect("object literal"),
    );
    if updated.is_ok() {
        let _ = schedule_presentation_session_delta(
            state,
            &db,
            &repository,
            &record.project_id,
            &record.session_id,
        );
    }
}

/// When gxserver starts: the supervisor gets its own title back (`restore_title`).
pub(super) fn restore_title_at_start(state: &AppState) {
    if let Some((record, session)) = live_supervisor(state) {
        restore_title(state, &record, &session);
    }
}

/// What a failed start or wake depends on: the Autopilot settings and whether each agent has an
/// account with room (a sign-in, a limit reset or a switch of model).
fn setup_key(paths: &crate::paths::GxserverPaths, config: &AutopilotConfig) -> String {
    let room: Vec<bool> = supervisor_candidates(config)
        .iter()
        .map(|pick| pick_has_room(paths, pick))
        .collect();
    format!(
        "{}|{room:?}",
        serde_json::to_string(config).unwrap_or_default(),
    )
}

/// The supervisor's agent and model as the log names them ("antigravity gemini-4-neon").
fn supervisor_name(agent: &str, model: Option<&str>) -> String {
    match model {
        Some(model) => format!("{agent} {model}"),
        None => agent.to_string(),
    }
}

fn retry_words(ms: i64) -> String {
    match ms / 1000 {
        seconds if seconds < 60 => ghostex_gx_protocol::plural(seconds, "second", "seconds"),
        60..=119 => "a minute".to_string(),
        seconds => format!("{} minutes", seconds / 60),
    }
}

/// Logs a supervisor that couldn't start or take a wake, to gxserver's log and Autopilot's.
fn log_failure(state: &AppState, items: &[JudgmentItem], error: &str, delay: i64, now: i64) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "autopilotSupervisorFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(error.to_string()),
        details: Some(json!({ "waiting": items.len(), "retryInMs": delay })),
    });
    let titles = items
        .iter()
        .map(|item| item.title.as_str())
        .filter(|title| !title.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    autopilot::append_log(
        &state.paths,
        &autopilot::LogEntry {
            at: crate::coordinators::iso_from_ms(now).unwrap_or_default(),
            project_id: String::new(),
            session_id: String::new(),
            title: SUPERVISOR_TITLE.to_string(),
            project: "Autopilot".to_string(),
            saw: format!(
                "{} waiting for its judgment{}.",
                if items.len() == 1 {
                    "1 session".to_string()
                } else {
                    format!("{} sessions", items.len())
                },
                if titles.is_empty() {
                    String::new()
                } else {
                    format!(": {titles}")
                }
            ),
            did: format!(
                "{}. It tries again in {}, then less often while it keeps failing.",
                error.trim_end_matches('.'),
                retry_words(delay)
            ),
            rule: "supervisor".to_string(),
            outcome: autopilot::Outcome::Failed,
        },
    );
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
    if items.iter().any(|item| item.why.starts_with("Check-up:")) {
        body.push_str("\nCheck-up: these sessions look like they need a nudge that Autopilot's rules didn't give. For each, decide whether to tell it how to carry on (`--message`), reload it (`--reload`), or leave it for the user (`--leave`).\n");
    }
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
