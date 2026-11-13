//! Autopilot's watcher: while the switch is on, it looks at every session Autopilot covers on each
//! sweep (2 minutes by default) and as soon as an agent hook says a turn ended, and acts on the
//! cases a rule can settle, logging every decision:
//!
//! - A session that ended its turn while something waits for it: "carry on". Queued task results
//!   and a turn that stopped mid-task go 45 seconds after the turn ended; "I wait on a background
//!   job" with no job in sight after the coordinator's idle round (10 minutes); a turn cut off
//!   (asleep, restarted) once its transcript has been quiet a whole stall round (8 minutes).
//! - Codex's own update prompt holding a session: Escape, which picks Skip.
//! - A working session with no output long enough to count as stuck (8 minutes; a coordinator's
//!   thread after its coordinator's nudge, 16): interrupt and "carry on"; a quiet round later,
//!   reload it on its own conversation; a round after that, leave it for the user.
//! - A session at a usage limit its own account policy waits out: move it to the ready account
//!   with the most room.
//! - A session whose terminal ended and that Ghostex's self-heal gave up on: reload it once more.
//!
//! Each kind of action has an hourly cap per session, so nothing loops: past it the session is
//! left for the user with a note in the log.
//!
//! CDXC:Autopilot 2026-11-07 DECISION:
//! User, on the design page: Ghostex watches events and sweeps cheaply, and an agent wakes only for judgment calls; this file is the watch, and it acts on the rule-based cases ("carry on a parked thread whose job finished, interrupt and carry on a hung one, reload a frozen one, switch account on a limit, restart a dead session") and logs every decision. It supersedes, for sessions Autopilot covers while its switch is on, the 2026-11-05 decision that Ghostex itself never interrupts or reloads (coordinators/stall.rs): the user turned Autopilot on to have it heal. Answering questions and approval cards, the judgment agent and its notes come with phase 2.
//! SEE-ALSO: server/src/autopilot/ (choices and log), server/src/coordinators/stall.rs and idle_wait.rs (the clocks it reads), server/src/accounts/autopilot.rs (the switch).

use super::*;

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::autopilot::{self, AutopilotConfig, LogEntry, Outcome};
use crate::coordinators::{self, IdleWaitReason, SessionKey};
use crate::presentation::{effective_lifecycle_state, presentation_activity};
use crate::session_chat_queue_runtime::SessionChatTranscriptGate;

const TICK: Duration = Duration::from_secs(10);
/// A session that ended its turn is left this long before Autopilot carries it on: task
/// notifications and the agent's own next turn often follow within seconds.
const SETTLE_MS: i64 = 45_000;
/// A sleeping session is looked at only when it was active this recently.
const SLEEPING_RECENT_MS: i64 = 6 * 60 * 60 * 1000;
/// Messages read back for a session's newest turn and words.
const TURN_TAIL_MESSAGES: usize = 24;
const HOUR_MS: i64 = 60 * 60 * 1000;
const MAX_CARRY_ONS_PER_HOUR: usize = 6;
const MAX_RELOADS_PER_HOUR: usize = 3;
const MAX_SWITCHES_PER_HOUR: usize = 2;
/// Questions answered and approvals given per session per hour.
const MAX_ANSWERS_PER_HOUR: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    CarryOn,
    Interrupt,
    Reload,
    Switch,
    Answer,
}

impl Kind {
    fn cap(self) -> usize {
        match self {
            Self::CarryOn | Self::Interrupt => MAX_CARRY_ONS_PER_HOUR,
            Self::Reload => MAX_RELOADS_PER_HOUR,
            Self::Switch => MAX_SWITCHES_PER_HOUR,
            Self::Answer => MAX_ANSWERS_PER_HOUR,
        }
    }
}

/// What Autopilot remembers about one session.
#[derive(Default)]
struct Pilot {
    gate: SessionChatTranscriptGate,
    idle_path: Option<PathBuf>,
    /// Every action taken, for the hourly caps.
    acted: Vec<(Kind, i64)>,
    /// The idle stretch (its last turn's end) already carried on.
    carried_on: Option<i64>,
    /// The quiet stretch acted on, what was done and when.
    quiet: Option<(i64, Kind, i64)>,
    /// The quiet stretch already left for the user.
    quiet_parked: Option<i64>,
    /// The self-heal give-up already acted on.
    gave_up_seen: Option<i64>,
    /// When the session was last seen at a usage limit Autopilot could not switch away from.
    limit_parked: bool,
    /// Caps already logged, so a capped session is parked once.
    capped: Option<Kind>,
    /// An action is running for this session.
    busy: bool,
    /// Its turn ended too recently to judge: look again after the settle.
    recheck: bool,
    /// Its questions and approval prompts.
    prompts: super::autopilot_answers::PromptMemory,
    /// The prompt the action running now decides.
    deciding: Option<String>,
}

impl Pilot {
    fn count(&mut self, kind: Kind, now: i64) -> usize {
        self.acted.retain(|(_, at)| now - at < HOUR_MS);
        self.acted.iter().filter(|(done, _)| *done == kind).count()
    }
}

#[derive(Default)]
struct Memory {
    pilots: HashMap<SessionKey, Pilot>,
    last_sweep_ms: i64,
    /// The outage Autopilot is waiting through (since when this computer is offline).
    offline_since: Option<i64>,
}

/// What the network lets Autopilot do on one tick.
#[derive(Debug, PartialEq, Eq)]
enum NetworkGate {
    /// Online as before: act as usual.
    Go,
    /// Offline: send nothing, wake no supervisor. `started` on the tick that noticed it.
    Hold { started: Option<i64> },
    /// The network came back: carry on with a full sweep now, not at the next one.
    Back { offline_since: i64 },
}

/// CDXC:OfflineHold 2026-11-07 WHY:
/// A carry-on, a reload or a supervisor call made while the computer is offline cannot reach the agent's API and only burns the hourly caps, so Autopilot holds everything while `network_status` says offline and sweeps at once when it is back (a sweep is 2 minutes apart by default, too slow for "resume within a few seconds").
fn network_gate(memory: &mut Memory, offline_since: Option<i64>) -> NetworkGate {
    match (memory.offline_since, offline_since) {
        (None, Some(since)) => {
            memory.offline_since = Some(since);
            NetworkGate::Hold {
                started: Some(since),
            }
        }
        (Some(_), Some(_)) => NetworkGate::Hold { started: None },
        (Some(since), None) => {
            memory.offline_since = None;
            memory.last_sweep_ms = 0;
            NetworkGate::Back {
                offline_since: since,
            }
        }
        (None, None) => NetworkGate::Go,
    }
}

/// One line in the log for the computer going offline or coming back.
fn log_network(state: &AppState, gate: &NetworkGate, now: i64) {
    let (saw, did) = match gate {
        NetworkGate::Hold {
            started: Some(since),
        } => (
            format!(
                "This computer went offline at {}.",
                crate::network_status::local_clock(*since)
            ),
            "Waiting for the network: no carry-ons, reloads or supervisor calls until it's back."
                .to_string(),
        ),
        NetworkGate::Back { offline_since } => (
            format!(
                "The network is back after {} (offline since {}).",
                crate::network_status::outage_length(now - offline_since),
                crate::network_status::local_clock(*offline_since)
            ),
            "Carrying on: every covered session is looked at now.".to_string(),
        ),
        _ => return,
    };
    autopilot::append_log(
        &state.paths,
        &LogEntry {
            at: coordinators::iso_from_ms(now).unwrap_or_default(),
            project_id: String::new(),
            session_id: String::new(),
            title: "Network".to_string(),
            project: "This computer".to_string(),
            saw,
            did,
            rule: "offline".to_string(),
            outcome: Outcome::Waited,
        },
    );
}

/// One thing to do.
struct Plan {
    key: SessionKey,
    title: String,
    project: String,
    rule: &'static str,
    saw: String,
    act: Act,
    /// The quiet stretch it acts on, for a stuck session.
    quiet_since: Option<i64>,
}

enum Act {
    Send {
        text: String,
        interrupt: bool,
    },
    /// Keys typed straight into the terminal (Escape on a CLI's own prompt).
    Keys {
        text: String,
        did: String,
    },
    /// Answer its prompt (`/api/answerSessionChatPrompt` params without the session's ids).
    Answer {
        params: Value,
        did: String,
    },
    /// Decline its prompt, then tell the agent to carry on without it.
    Decline {
        params: Value,
        message: String,
        note: String,
    },
    /// Leave its prompt for the user with a note.
    Leave {
        note: String,
    },
    /// Answer its open Ghostex forms with their recommended choices.
    Forms,
    /// Hand it to the supervisor.
    Judge {
        why: String,
        detail: String,
    },
    Reload,
    Switch,
    Park,
}

#[derive(Default, Clone, Copy)]
struct Watching {
    last_sweep_ms: i64,
    next_sweep_ms: i64,
    covered: usize,
}

fn watching() -> &'static std::sync::Mutex<Watching> {
    static WATCHING: OnceLock<std::sync::Mutex<Watching>> = OnceLock::new();
    WATCHING.get_or_init(Default::default)
}

/// When the watcher last swept and will sweep next, and how many sessions it covers, for
/// Settings > Autopilot.
pub(crate) fn autopilot_runtime_watching() -> Value {
    let watching = *watching()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    json!({
        "lastSweepAt": (watching.last_sweep_ms > 0)
            .then(|| coordinators::iso_from_ms(watching.last_sweep_ms))
            .flatten(),
        "nextSweepAt": (watching.next_sweep_ms > 0)
            .then(|| coordinators::iso_from_ms(watching.next_sweep_ms))
            .flatten(),
        "covered": watching.covered,
    })
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn start_autopilot_runtime(state: Arc<AppState>) {
    let _ = autopilot::read_config(&state.paths);
    let memory = Arc::new(Mutex::new(Memory::default()));
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
                _ = autopilot::signalled() => {}
            }
            let (ticking, ticking_memory) = (state.clone(), memory.clone());
            let plans = tokio::task::spawn_blocking(move || tick(&ticking, &ticking_memory))
                .await
                .unwrap_or_default();
            for plan in plans {
                tokio::spawn(execute(state.clone(), memory.clone(), plan));
            }
            tokio::spawn(super::autopilot_judge::maybe_wake(state.clone()));
        }
    });
}

fn text_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// BLOCKING (SQLite, transcript tails of the sessions it looks at).
fn tick(state: &AppState, memory: &Mutex<Memory>) -> Vec<Plan> {
    let config = autopilot::read_config(&state.paths);
    let Ok(mut memory) = memory.lock() else {
        return Vec::new();
    };
    let signalled = autopilot::take_signalled();
    if !config.enabled {
        memory.pilots.clear();
        *watching().lock().unwrap_or_else(|e| e.into_inner()) = Watching::default();
        return Vec::new();
    }
    let now = now_ms();
    let gate = network_gate(&mut memory, crate::network_status::offline_since());
    log_network(state, &gate, now);
    if matches!(gate, NetworkGate::Hold { .. }) {
        return Vec::new();
    }
    let sweep_every = coordinators::test_scaled(config.sweep_seconds as i64 * 1000);
    let sweep = now - memory.last_sweep_ms >= sweep_every;
    // Between sweeps only sessions whose turn just ended are looked at, and only once they settled:
    // the signal comes back with the next tick after the settle.
    if !sweep && signalled.is_empty() {
        return Vec::new();
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects: HashMap<String, Value> = repository
        .list_projects()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|project| {
            autopilot::remember_project(&project);
            Some((project["projectId"].as_str()?.to_string(), project))
        })
        .collect();
    let Ok(sessions) = repository.list_sessions_excluding_stopped(None) else {
        return Vec::new();
    };
    let now_iso = crate::presentation::now_iso();
    let gave_up = crate::session_self_heal::gave_up_sessions();
    let supervisor = super::autopilot_judge::supervisor_key(&state.paths);
    let mut plans = Vec::new();
    let mut covered = HashSet::new();
    for session in &sessions {
        let key = (
            text_field(session, "projectId"),
            text_field(session, "sessionId"),
        );
        autopilot::note_read_only_thread(&key.0, &key.1, session);
        let Some(project) = projects.get(&key.0) else {
            continue;
        };
        // The supervisor gets only the update-prompt rule: a Codex supervisor reloaded or started
        // anew shows it, and nothing else would get it past it.
        if supervisor.as_ref() == Some(&key) {
            let pilot = memory.pilots.entry(key.clone()).or_default();
            covered.insert(key.clone());
            if sweep
                && !pilot.busy
                && effective_lifecycle_state(session) == "running"
                && shows_update_prompt(session)
            {
                pilot.busy = true;
                plans.push(Plan {
                    key: key.clone(),
                    title: text_field(session, "title"),
                    project: text_field(project, "name"),
                    rule: "update-prompt",
                    saw: "Codex showed its \"Update available\" prompt, which holds it before it takes input.".to_string(),
                    act: Act::Keys {
                        text: "\u{1b}".to_string(),
                        did: "Picked Skip; the update waits for you.".to_string(),
                    },
                    quiet_since: None,
                });
            }
            continue;
        }
        if !candidate(session, &config, project, &key, supervisor.as_ref(), now) {
            continue;
        }
        covered.insert(key.clone());
        if !sweep && !signalled.contains(&key) {
            continue;
        }
        let pilot = memory.pilots.entry(key.clone()).or_default();
        if pilot.busy || autopilot::user_has_it(&key.0, &key.1, now) {
            continue;
        }
        // Its agent's API stopped answering although the computer is online: nothing sent to it
        // could get through.
        if crate::network_status::session_offline(session) {
            continue;
        }
        let plan = decide(
            state,
            session,
            &key,
            pilot,
            gave_up.get(&key).copied(),
            &config,
            project.get("path").and_then(Value::as_str),
            now,
            &now_iso,
        );
        let Some(mut plan) = plan else {
            continue;
        };
        plan.title = text_field(session, "title");
        plan.project = text_field(project, "name");
        if let Some(kind) = plan_kind(&plan.act) {
            if pilot.count(kind, now) >= kind.cap() {
                if pilot.capped == Some(kind) {
                    continue;
                }
                pilot.capped = Some(kind);
                plan.saw = format!(
                    "{} Autopilot already did this {} times in the last hour.",
                    plan.saw,
                    kind.cap()
                );
                plan.act = Act::Park;
            }
        }
        pilot.busy = true;
        plans.push(plan);
    }
    memory.pilots.retain(|key, _| covered.contains(key));
    if sweep {
        memory.last_sweep_ms = now;
    }
    // A session whose turn ended too recently to judge is looked at again after its settle.
    for (key, pilot) in memory.pilots.iter_mut() {
        if std::mem::take(&mut pilot.recheck) {
            requeue_after_settle(key.clone());
        }
    }
    *watching().lock().unwrap_or_else(|e| e.into_inner()) = Watching {
        last_sweep_ms: memory.last_sweep_ms,
        next_sweep_ms: memory.last_sweep_ms + sweep_every,
        covered: covered.len(),
    };
    plans
}

/// Signals a session again once its turn's settle has passed, so a turn end is acted on within
/// a minute instead of at the next sweep.
fn requeue_after_settle(key: SessionKey) {
    static WAITING: OnceLock<std::sync::Mutex<HashSet<SessionKey>>> = OnceLock::new();
    let waiting = WAITING.get_or_init(Default::default);
    if !waiting
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.clone())
    {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(
            coordinators::test_scaled(SETTLE_MS) as u64 + 2_000,
        ))
        .await;
        waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
        autopilot::signal_turn_ended(&key.0, &key.1);
    });
}

/// Whether the session's screen shows Codex's update prompt ("Update available", with "Skip until
/// next version"), whose footer says Escape skips it. Read only for Codex sessions.
///
/// CDXC:Autopilot 2026-11-07 WHY:
/// The brief's real stops include Codex's update prompt, which a bot had to get past by hand; a reloaded Codex shows it again on start (seen live on the test server), so the carry-on Autopilot sent sat behind it.
fn shows_update_prompt(session: &Value) -> bool {
    if crate::session_chat_follower::session_chat_agent_for_session(session).as_deref()
        != Some("codex")
    {
        return false;
    }
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(session) else {
        return false;
    };
    crate::zmx::read_zmx_session_grid_capture(&zmx_name).is_ok_and(|capture| {
        capture.text.contains("Update available")
            && capture.text.contains("Skip until next version")
    })
}

/// "1 minute", "9 minutes".
fn minutes_text(minutes: i64) -> String {
    match minutes.max(1) {
        1 => "1 minute".to_string(),
        minutes => format!("{minutes} minutes"),
    }
}

/// The words of a failed call's answer: its `message` when it is gxserver's JSON error.
fn error_text(error: &str) -> String {
    serde_json::from_str::<Value>(error)
        .ok()
        .and_then(|value| value["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| error.to_string())
        .chars()
        .take(240)
        .collect()
}

fn plan_kind(act: &Act) -> Option<Kind> {
    match act {
        Act::Send {
            interrupt: false, ..
        } => Some(Kind::CarryOn),
        Act::Send {
            interrupt: true, ..
        } => Some(Kind::Interrupt),
        Act::Reload => Some(Kind::Reload),
        Act::Keys { .. } => Some(Kind::CarryOn),
        Act::Answer { .. } | Act::Forms => Some(Kind::Answer),
        Act::Decline { .. } | Act::Leave { .. } | Act::Judge { .. } => None,
        Act::Switch => Some(Kind::Switch),
        Act::Park => None,
    }
}

/// An agent session of this computer that Autopilot covers and can act on: running, or asleep
/// after recent work; never a draft, a box session, or a session on another computer.
fn candidate(
    session: &Value,
    config: &AutopilotConfig,
    project: &Value,
    key: &SessionKey,
    supervisor: Option<&SessionKey>,
    now: i64,
) -> bool {
    if session.get("kind").and_then(Value::as_str) != Some("agent")
        || crate::agents::session_is_draft(session)
        || crate::agentbox::is_agentbox_session(session)
        // A bot's conversation (Hermes) is a chat, not a task: its greeting's "I'll follow
        // up in a moment." is no turn cut off mid-task (CDXC:Bots 2026-11-08, bot_projects.rs).
        || crate::bot_projects::is_bot_project(project)
    {
        return false;
    }
    let lifecycle = effective_lifecycle_state(session);
    let recent = || {
        session
            .get("lastActiveAt")
            .and_then(Value::as_str)
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .is_some_and(|at| now - at.timestamp_millis() < SLEEPING_RECENT_MS)
    };
    if lifecycle != "running" && !(lifecycle == "sleeping" && recent()) {
        return false;
    }
    // The supervisor is Autopilot's own: its wakes, not the rules, keep it going.
    if supervisor == Some(key) {
        return false;
    }
    config.covers(
        &key.0,
        autopilot::main_project_of(project).as_deref(),
        &key.1,
    )
}

/// BLOCKING. What one session needs, if anything.
fn decide(
    state: &AppState,
    session: &Value,
    key: &SessionKey,
    pilot: &mut Pilot,
    gave_up_at: Option<i64>,
    config: &AutopilotConfig,
    project_path: Option<&str>,
    now: i64,
    now_iso: &str,
) -> Option<Plan> {
    let plan = |rule: &'static str, saw: String, act: Act| {
        Some(Plan {
            key: key.clone(),
            title: String::new(),
            project: String::new(),
            rule,
            saw,
            act,
            quiet_since: None,
        })
    };
    // A question, a form or an approval waits on the user: what the user's choices allow, else
    // nothing (it is theirs).
    if let Some(prompt) = super::autopilot_answers::decide_prompt(
        state,
        session,
        key,
        &mut pilot.prompts,
        config,
        project_path,
        now,
    ) {
        use super::autopilot_answers::PromptAct;
        let act = match prompt.act {
            PromptAct::Wait => return None,
            PromptAct::Answer { params, did } => Act::Answer { params, did },
            PromptAct::Decline {
                params,
                message,
                note,
            } => Act::Decline {
                params,
                message,
                note,
            },
            PromptAct::Leave { note } => Act::Leave { note },
            PromptAct::Forms => Act::Forms,
            PromptAct::Judge { why, detail } => Act::Judge { why, detail },
        };
        pilot.deciding = super::autopilot_answers::current_fingerprint(&pilot.prompts);
        return plan(prompt.rule, prompt.saw, act);
    }
    let lifecycle = effective_lifecycle_state(session);
    // The terminal ended under the agent and Ghostex's own restarts gave up.
    if let Some(gave_up_at) = gave_up_at {
        if pilot.gave_up_seen == Some(gave_up_at) {
            return None;
        }
        pilot.gave_up_seen = Some(gave_up_at);
        return plan(
            "ended",
            "Its terminal ended and Ghostex's restarts didn't keep it running.".to_string(),
            Act::Reload,
        );
    }
    let running = lifecycle == "running";
    // A usage limit the session's own policy waits out.
    if running {
        let at_limit =
            crate::session_chat_options::cached_session_chat_terminal_notice(state, &key.0, &key.1)
                .is_some_and(|notice| notice.kind == "usageLimit");
        if !at_limit {
            pilot.limit_parked = false;
        } else if !pilot.limit_parked {
            return plan(
                "limit",
                "It reached its account's usage limit.".to_string(),
                Act::Switch,
            );
        }
    }
    // A CLI's own update prompt holds the agent before it can take anything: pick Skip.
    if running && shows_update_prompt(session) {
        return plan(
            "update-prompt",
            "Codex showed its \"Update available\" prompt, which holds it before it takes input."
                .to_string(),
            Act::Keys {
                text: "\u{1b}".to_string(),
                did: "Picked Skip; the update waits for you.".to_string(),
            },
        );
    }
    let round = coordinators::quiet_round_ms();
    // A stuck stretch Autopilot already acted on: a round later, whatever the hooks say now (an
    // interrupt the agent never took leaves it looking idle), it is judged by whether the agent
    // wrote anything since. Nothing: reload it; nothing after the reload either: leave it.
    if let Some((since, kind, at)) = pilot.quiet {
        if now - at < round {
            return None;
        }
        let wrote_since = pilot
            .gate
            .tail(session, TURN_TAIL_MESSAGES)
            .is_some_and(|tail| {
                tail.messages.iter().any(|message| {
                    message.role == crate::session_chat::SessionChatRole::Assistant
                        && message.timestamp.is_some_and(|written| written > at)
                })
            });
        pilot.quiet = None;
        if !wrote_since {
            let minutes = minutes_text(coordinators::quiet_minutes(since, now));
            return match kind {
                Kind::Interrupt => plan(
                    "frozen",
                    format!("Nothing from it for {minutes}, even after Autopilot interrupted it."),
                    Act::Reload,
                )
                .map(|mut frozen| {
                    frozen.quiet_since = Some(since);
                    frozen
                }),
                _ => {
                    pilot.quiet_parked = Some(since);
                    plan(
                        "frozen",
                        format!(
                            "Nothing from it for {minutes}, even after an interrupt and a reload."
                        ),
                        Act::Park,
                    )
                }
            };
        }
    }
    let working = running && presentation_activity(session, now_iso) == "working";
    if working {
        let quiet = coordinators::session_quiet(&key.0, &key.1)?;
        if !quiet.stuck || pilot.quiet_parked == Some(quiet.quiet_since) {
            return None;
        }
        let minutes = minutes_text(coordinators::quiet_minutes(quiet.quiet_since, now));
        return plan(
            "hung",
            format!("No output for {minutes}: a command looks hung."),
            Act::Send {
                text: format!(
                    "Ghostex: [Autopilot] you showed no output for {minutes}, so I stopped what looked like a hung command. Don't run it the same way again (no watch modes, nothing that waits for input or never exits); carry on with your task."
                ),
                interrupt: true,
            },
        )
        .map(|mut hung| {
            hung.quiet_since = Some(quiet.quiet_since);
            hung
        });
    }
    // Out of work: does anything wait for it?
    let tail = pilot.gate.tail(session, TURN_TAIL_MESSAGES)?.clone();
    let turn_open = tail.lifecycle.as_ref().is_some_and(|lifecycle| {
        lifecycle.state == crate::session_chat::SessionChatTurnLifecycleState::Working
    });
    let last = tail.messages.iter().rev().find(|message| {
        message.role == crate::session_chat::SessionChatRole::Assistant && !message.queued
    });
    let ended_at = tail
        .lifecycle
        .as_ref()
        .and_then(|lifecycle| lifecycle.timestamp)
        .or_else(|| last.and_then(|message| message.timestamp))?;
    if pilot.carried_on == Some(ended_at) {
        return None;
    }
    if now - ended_at < coordinators::test_scaled(SETTLE_MS) {
        pilot.recheck = true;
        return None;
    }
    let last_words = last.map(|message| {
        message
            .blocks
            .iter()
            .filter_map(|block| match block {
                crate::session_chat::SessionChatBlock::Text { text } => Some(text.trim()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    });
    // A sleeping session is carried on only when its turn was cut off.
    let reason = coordinators::session_idle_reason(
        session,
        last_words.as_deref(),
        turn_open,
        &mut pilot.idle_path,
    )?;
    if !running && reason != IdleWaitReason::TurnCutOff {
        return None;
    }
    // "I wait on a background job" with no running job in sight: the job may be one Ghostex can't
    // see (started detached), so it gets the coordinator's idle round before it counts as done.
    // Queued results prove it ended and go at once.
    if reason == IdleWaitReason::SaidWaiting && now - ended_at < coordinators::idle_round_ms() {
        return None;
    }
    // A turn still open in the transcript is cut off only once nothing was written for a whole
    // quiet round: an agent whose hooks are late or missing (hookless agents, a server it can't
    // reach) looks idle while it still works.
    if reason == IdleWaitReason::TurnCutOff {
        let newest = tail
            .messages
            .iter()
            .filter_map(|message| message.timestamp)
            .max()
            .unwrap_or(ended_at)
            .max(ended_at);
        if now - newest < round {
            return None;
        }
    }
    pilot.carried_on = Some(ended_at);
    let (rule, saw, text) = match reason {
        IdleWaitReason::Queued(count) => (
            "parked-job-finished",
            format!("It stopped with {count} finished background job result{} waiting.", if count == 1 { "" } else { "s" }),
            "Ghostex: [Autopilot] your background job finished: read its result and carry on with your task.",
        ),
        IdleWaitReason::SaidWaiting => (
            "parked-job-finished",
            "It stopped saying it waits on a background job that is no longer running.".to_string(),
            "Ghostex: [Autopilot] your background job looks finished: read its result and carry on with your task.",
        ),
        IdleWaitReason::SaidCarryingOn => (
            "stopped-mid-task",
            "It stopped mid-task, right after saying what it does next.".to_string(),
            "Ghostex: [Autopilot] your last turn stopped before your task was done: carry on where you left off, or say you're finished.",
        ),
        IdleWaitReason::TurnCutOff => (
            "cut-off",
            "Its turn was cut off before it ended (asleep, restarted or stopped mid-turn).".to_string(),
            "Ghostex: [Autopilot] your last turn was cut off before it ended: carry on where you left off, or say you're finished.",
        ),
        IdleWaitReason::JobRunning => return None,
    };
    plan(
        rule,
        saw,
        Act::Send {
            text: text.to_string(),
            interrupt: false,
        },
    )
}

/// Runs one plan, logs it, and frees the session for the next look.
async fn execute(state: Arc<AppState>, memory: Arc<Mutex<Memory>>, plan: Plan) {
    let key = plan.key.clone();
    let now = now_ms();
    let kind = plan_kind(&plan.act);
    let result: std::result::Result<String, String> = match &plan.act {
        Act::Send { text, interrupt } => send(&state, &key, text, *interrupt).await.map(|()| {
            if *interrupt {
                "Interrupted it and sent \"carry on\".".to_string()
            } else {
                "Sent \"carry on\".".to_string()
            }
        }),
        Act::Reload => reload(&state, &key).await,
        Act::Keys { text, did } => type_keys(&state, &key, text).await.map(|()| did.clone()),
        Act::Switch => {
            let switching = state.clone();
            let (project_id, session_id) = key.clone();
            match tokio::task::spawn_blocking(move || {
                crate::accounts::autopilot::switch_at_limit(&switching, &project_id, &session_id)
            })
            .await
            {
                Ok(Ok(crate::accounts::autopilot::AutopilotSwitch::Moved(account))) => Ok(format!(
                    "Moved it to {account} and sent \".\" so it carries on."
                )),
                Ok(Ok(crate::accounts::autopilot::AutopilotSwitch::PolicySwitches)) => {
                    // Its own account policy switches it; nothing for Autopilot to log.
                    finish(&memory, &key, None, now, |_| {});
                    return;
                }
                Ok(Ok(crate::accounts::autopilot::AutopilotSwitch::NoRoom)) => {
                    log(&state, &plan, "Left it for you: no other account is signed in with room. It continues when its limit resets.", Outcome::Parked);
                    finish(&memory, &key, None, now, |pilot| pilot.limit_parked = true);
                    return;
                }
                Ok(Ok(crate::accounts::autopilot::AutopilotSwitch::NotSwitchable)) => {
                    log(&state, &plan, "Left it for you: it runs on its own login, so it waits for its limit to reset.", Outcome::Parked);
                    finish(&memory, &key, None, now, |pilot| pilot.limit_parked = true);
                    return;
                }
                Ok(Err(error)) => Err(error.message),
                Err(error) => Err(error.to_string()),
            }
        }
        Act::Park => {
            log(&state, &plan, "Left it for you.", Outcome::Parked);
            // CDXC:Notifications 2026-11-08: its red dot is the user's now, so it alerts
            // (server/src/server/session_stall_runtime.rs).
            let (alerting, (project_id, session_id)) = (state.clone(), key.clone());
            let _ = tokio::task::spawn_blocking(move || {
                super::session_stall_runtime::announce_left_for_user(
                    &alerting,
                    &project_id,
                    &session_id,
                );
            })
            .await;
            // What no rule could move gets a look from the supervisor: it may know what to tell it.
            super::autopilot_judge::queue_judgment(super::autopilot_judge::JudgmentItem {
                key: key.clone(),
                title: plan.title.clone(),
                project: plan.project.clone(),
                why: "Ghostex's rules couldn't keep it moving and left it for the user."
                    .to_string(),
                detail: format!("What Ghostex saw: {}", plan.saw),
            });
            finish(&memory, &key, None, now, |_| {});
            return;
        }
        Act::Answer { params, did } => answer(&state, &key, params)
            .await
            .map(|()| did.clone())
            .inspect(|_| mark_prompt_done(&memory, &key)),
        Act::Decline {
            params,
            message,
            note,
        } => {
            let declined = match answer(&state, &key, params).await {
                Ok(()) => send(&state, &key, message, false)
                    .await
                    .map_err(|error| error_text(&error)),
                Err(error) => Err(error),
            };
            match declined {
                Ok(()) => {
                    log(
                        &state,
                        &plan,
                        &format!("Declined it and told the agent to carry on without it. Left for you: {note}"),
                        Outcome::Parked,
                    );
                    mark_prompt_done(&memory, &key);
                }
                Err(error) => log(
                    &state,
                    &plan,
                    &format!("Tried to decline it, but it failed: {error}"),
                    Outcome::Failed,
                ),
            }
            finish(&memory, &key, None, now, |_| {});
            return;
        }
        Act::Leave { note } => {
            log(
                &state,
                &plan,
                &format!("Left it for you: {note}"),
                Outcome::Parked,
            );
            mark_prompt_done(&memory, &key);
            finish(&memory, &key, None, now, |_| {});
            return;
        }
        Act::Judge { why, detail } => {
            super::autopilot_judge::queue_judgment(super::autopilot_judge::JudgmentItem {
                key: key.clone(),
                title: plan.title.clone(),
                project: plan.project.clone(),
                why: why.clone(),
                detail: detail.clone(),
            });
            mark_prompt_done(&memory, &key);
            finish(&memory, &key, None, now, |_| {});
            return;
        }
        Act::Forms => {
            use crate::voice_agent::question_forms::{
                autopilot_answer_recommended, AutopilotForms,
            };
            match autopilot_answer_recommended(&state, &key.0, &key.1).await {
                Ok(AutopilotForms::Answered(picked)) => {
                    mark_prompt_done(&memory, &key);
                    Ok(format!(
                        "Answered with the recommended choices: {}.",
                        picked.join("; ")
                    ))
                }
                Ok(AutopilotForms::NeedsJudgment(questions)) => {
                    super::autopilot_judge::queue_judgment(super::autopilot_judge::JudgmentItem {
                        key: key.clone(),
                        title: plan.title.clone(),
                        project: plan.project.clone(),
                        why: "Its chat asks questions without a recommended choice, and the user didn't answer in time.".to_string(),
                        detail: format!("Its open questions (answer them in this order with --answer):\n- {}", questions.join("\n- ")),
                    });
                    mark_prompt_done(&memory, &key);
                    finish(&memory, &key, None, now, |_| {});
                    return;
                }
                Ok(AutopilotForms::None) => {
                    mark_prompt_done(&memory, &key);
                    finish(&memory, &key, None, now, |_| {});
                    return;
                }
                Err(error) => Err(error),
            }
        }
    };
    match result {
        Ok(did) => {
            log(&state, &plan, &did, Outcome::Done);
            let quiet = plan.quiet_since;
            finish(&memory, &key, kind, now, |pilot| {
                if let (Some(since), Some(kind @ (Kind::Interrupt | Kind::Reload))) = (quiet, kind)
                {
                    pilot.quiet = Some((since, kind, now));
                }
            });
        }
        Err(error) => {
            let text = error_text(&error);
            // An agent that refuses a message while its own approval or question card is open is
            // not a failure: the card waits for the user, so the log says that.
            let waiting = text.starts_with("Answer the pending ");
            let (did, outcome) = if waiting {
                (
                    "Waiting for you: an approval or question is open, and the agent takes no new message until it is answered.".to_string(),
                    Outcome::Parked,
                )
            } else {
                (format!("Tried, but it failed: {text}"), Outcome::Failed)
            };
            log(&state, &plan, &did, outcome);
            // An interrupt or a reload that failed still counts for the stretch: a round later the
            // next step is judged by whether the agent wrote anything.
            let quiet = plan.quiet_since;
            finish(&memory, &key, kind, now, |pilot| {
                if let (Some(since), Some(kind @ (Kind::Interrupt | Kind::Reload))) = (quiet, kind)
                {
                    pilot.quiet = Some((since, kind, now));
                }
            });
        }
    }
}

/// Marks the prompt the running action decided as done, so it gets one decision.
fn mark_prompt_done(memory: &Mutex<Memory>, key: &SessionKey) {
    if let Ok(mut memory) = memory.lock() {
        if let Some(pilot) = memory.pilots.get_mut(key) {
            if let Some(fingerprint) = pilot.deciding.take() {
                pilot.prompts.mark_done(&fingerprint);
            }
        }
    }
}

/// The supervisor's picks for the session's open question: the stored prompt's questions (an
/// option's label, its number, or text where the question takes text), else its open Ghostex
/// forms. Returns what was answered.
pub(super) async fn answer_with_picks(
    state: &Arc<AppState>,
    key: &SessionKey,
    picks: &[String],
) -> std::result::Result<String, String> {
    let stored = {
        let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        repository
            .get_session(&key.0, &key.1)
            .ok()
            .flatten()
            .and_then(|session| crate::agents::session_chat_prompt_setting(&session))
            .and_then(|stored| {
                crate::session_chat_interactive::parse_stored_session_chat_prompt(&stored)
            })
    };
    if let Some(crate::session_chat_interactive::SessionChatInteractivePrompt::Question {
        questions,
        tool_use_id,
    }) = stored
    {
        let normalized = |text: &str| {
            text.to_ascii_lowercase()
                .replace("(recommended)", "")
                .trim()
                .to_string()
        };
        let mut selections = Vec::new();
        for (question, pick) in questions
            .iter()
            .zip(picks.iter().chain(std::iter::repeat(&String::new())))
        {
            let wanted = normalized(pick);
            let index = question
                .options
                .iter()
                .position(|option| normalized(&option.label) == wanted)
                .or_else(|| {
                    wanted
                        .parse::<usize>()
                        .ok()
                        .and_then(|number| number.checked_sub(1))
                        .filter(|index| *index < question.options.len())
                });
            selections.push(match index {
                Some(index) => json!({ "indices": [index], "other": null }),
                None if !pick.trim().is_empty() => json!({ "indices": [], "other": pick }),
                None => {
                    return Err(format!(
                        "No pick for the question \"{}\".",
                        question.question
                    ))
                }
            });
        }
        answer(
            state,
            key,
            &json!({ "kind": "question", "toolUseId": tool_use_id, "selections": selections }),
        )
        .await?;
        return Ok(format!("Answered: {}.", picks.join("; ")));
    }
    if crate::voice_agent::question_forms::autopilot_answer_with(state, &key.0, &key.1, picks)
        .await?
    {
        return Ok(format!("Answered its questions: {}.", picks.join("; ")));
    }
    Err("Nothing in that session waits for an answer any more.".to_string())
}

/// Answers the session's prompt through the chat's own answer path.
async fn answer(
    state: &AppState,
    key: &SessionKey,
    params: &Value,
) -> std::result::Result<(), String> {
    let mut params = params.clone();
    params["projectId"] = json!(key.0);
    params["sessionId"] = json!(key.1);
    let routed = crate::session_chat_send::handle_answer_session_chat_prompt_http(
        state,
        "/api/answerSessionChatPrompt".to_string(),
        Uuid::new_v4().to_string(),
        &json!({ "params": params }),
    )
    .await;
    let result = if routed.response.status().is_success() {
        Ok(())
    } else {
        let bytes = to_bytes(routed.response.into_body(), 64 * 1024)
            .await
            .unwrap_or_default();
        Err(error_text(&String::from_utf8_lossy(&bytes)))
    };
    // A question's answer is the agent's own choice; anything else (an approval, a permission or
    // trust prompt in the terminal) is Autopilot letting the agent do something.
    if params.get("kind").and_then(Value::as_str) != Some("question") {
        crate::audit::record(
            crate::audit::Action::new(
                "autopilot.answer",
                "Autopilot answered an approval for an agent",
            )
            .target(format!("{}:{}", key.0, key.1)),
            crate::audit::Actor::Autopilot,
            crate::audit::Outcome::of(&result),
            None,
        );
    }
    result
}

fn finish(
    memory: &Mutex<Memory>,
    key: &SessionKey,
    kind: Option<Kind>,
    now: i64,
    change: impl FnOnce(&mut Pilot),
) {
    let Ok(mut memory) = memory.lock() else {
        return;
    };
    if let Some(pilot) = memory.pilots.get_mut(key) {
        pilot.busy = false;
        if let Some(kind) = kind {
            pilot.acted.push((kind, now));
        }
        change(pilot);
    }
}

fn log(state: &AppState, plan: &Plan, did: &str, outcome: Outcome) {
    autopilot::append_log(
        &state.paths,
        &LogEntry {
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            project_id: plan.key.0.clone(),
            session_id: plan.key.1.clone(),
            title: plan.title.clone(),
            project: plan.project.clone(),
            saw: plan.saw.clone(),
            did: did.to_string(),
            rule: plan.rule.to_string(),
            outcome,
        },
    );
    let _ = state.logger.log_routine(
        crate::logging::DiagnosticLogScenario::ServerLifecycle,
        GxserverLogInput {
            level: LogLevel::Info,
            event: "autopilotDecision".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({
                "projectId": plan.key.0,
                "sessionId": plan.key.1,
                "rule": plan.rule,
                "outcome": outcome,
            })),
        },
    );
}

/// Sends a session a message the way `ghostex agents send` does.
pub(super) async fn send_text(
    state: &AppState,
    key: &SessionKey,
    text: &str,
) -> std::result::Result<(), String> {
    send(state, key, text, false)
        .await
        .map_err(|error| error_text(&error))
}

/// The send `ghostex agents send` makes (typed now, taken by a busy agent at its next input
/// boundary, a sleeping one woken for it); `interrupt` stops the running turn first.
async fn send(
    state: &AppState,
    key: &SessionKey,
    text: &str,
    interrupt: bool,
) -> std::result::Result<(), String> {
    let body = json!({
        "params": {
            "projectId": key.0,
            "sessionId": key.1,
            "text": text,
            "sendRequestId": Uuid::new_v4().to_string(),
            "interrupt": interrupt,
        }
    });
    let routed = crate::session_chat_send::handle_send_session_chat_message_http(
        state,
        "/api/sendSessionChatMessage".to_string(),
        Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    if routed.response.status().is_success() {
        return Ok(());
    }
    let bytes = to_bytes(routed.response.into_body(), 64 * 1024)
        .await
        .unwrap_or_default();
    Err(String::from_utf8_lossy(&bytes).to_string())
}

/// Types `text` into the session's terminal as it is (`/api/sendSessionText`).
async fn type_keys(
    state: &AppState,
    key: &SessionKey,
    text: &str,
) -> std::result::Result<(), String> {
    let queued = {
        let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(key.0));
        params.insert("sessionId".to_string(), json!(key.1));
        params.insert("text".to_string(), json!(text));
        crate::zmx::read_zmx_queued_session_write(&repository, "/api/sendSessionText", &params)
            .map_err(|error| format!("{error:?}"))?
    };
    queued
        .execute()
        .await
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
}

/// `ghostex agents reload`, then "carry on" once the agent is back.
async fn reload(state: &AppState, key: &SessionKey) -> std::result::Result<String, String> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(key.0));
    params.insert("sessionId".to_string(), json!(key.1));
    crate::session_chat_agent_restart::reload(state, &params)
        .await
        .map_err(|error| error.message)?;
    send(
        state,
        key,
        "Ghostex: [Autopilot] your session was reloaded because it stopped responding. Carry on with your task where you left off.",
        false,
    )
    .await?;
    Ok("Reloaded it on its own conversation and sent \"carry on\".".to_string())
}

#[cfg(test)]
mod network_gate_tests {
    use super::*;

    #[test]
    fn autopilot_holds_while_offline_and_sweeps_at_once_when_back() {
        let mut memory = Memory {
            last_sweep_ms: 1_000,
            ..Memory::default()
        };
        assert_eq!(network_gate(&mut memory, None), NetworkGate::Go);
        // The first offline tick says so once; the next ones only hold.
        assert_eq!(
            network_gate(&mut memory, Some(5_000)),
            NetworkGate::Hold {
                started: Some(5_000)
            }
        );
        assert_eq!(
            network_gate(&mut memory, Some(5_000)),
            NetworkGate::Hold { started: None }
        );
        // Back: one line, and the next sweep is due now.
        assert_eq!(
            network_gate(&mut memory, None),
            NetworkGate::Back {
                offline_since: 5_000
            }
        );
        assert_eq!(memory.last_sweep_ms, 0);
        assert_eq!(network_gate(&mut memory, None), NetworkGate::Go);
    }
}
