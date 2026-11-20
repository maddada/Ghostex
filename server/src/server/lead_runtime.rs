//! The lead link watcher: every two seconds it looks at each session linked to a lead and tells
//! the lead when a turn the lead started finished, when the session waits on an answer or goes
//! quiet during that turn, and when it was closed before reporting.
//!
//! CDXC:LeadLinks 2026-11-20 WHY:
//! GitHub #678's reporter ran a watcher every 5 minutes that read every session a lead started and
//! told the lead when one stopped without reporting; it saw "stopped" only, about 10 minutes late.
//! gxserver already knows each session's state, so it watches and the lead only wakes for a report,
//! as the coordinator supervisor does for threads (coordinator_runtime.rs). It is kept apart from
//! that supervisor on purpose: a linked session is an ordinary session (no Mark Done, no board, no
//! relayed cards, no nudges from here; the sidebar-wide stuck watch keeps its red dot), and only a
//! turn the lead started is reported (`owedSince`, set when the lead's own send reached it), so the
//! user's own conversation with a linked session never reaches the lead. A report is settled only
//! after the send succeeded, so a restart or a busy lead delays one but never loses or repeats it.
//! SEE-ALSO: server/src/agents/lead_links/ (records, endpoint, presentation),
//! server/src/session_chat_send/send_reopen.rs (the lead's send marks the turn owed),
//! server/src/server/session_stall_runtime.rs (the quiet clock read here).

use super::*;

use crate::agents::lead_links::{
    lead_turn, list_lead_links, read_lead_link, remove_lead_link, set_link_observed_working,
    settle_finished, settle_quiet, settle_waiting, LeadLink, TurnRow, LEAD_TURN_SLACK_MS,
};
use crate::coordinators::{
    agent_message, classify_thread_session, report_body, SessionKey, ThreadProgress, ThreadReport,
    ThreadState,
};
use crate::presentation::effective_lifecycle_state;

use super::coordinator_runtime::{
    agent_names, send_to_coordinator, session_sender, waiting_report_prompt,
};

const TICK: Duration = Duration::from_secs(2);
/// A linked session must stay out of work this long before its turn counts as finished: hooks can
/// dip between tool calls.
const FINISH_STABILITY_MS: i64 = 4_000;
/// How long a linked session stays closed before an owed report says so.
const CLOSED_GRACE_MS: i64 = 60_000;
/// After a failed send to a lead, its reports wait this long before the next try.
const RETRY_AFTER_MS: i64 = 30_000;
/// A session's activity stamp may lead the lead's `owedSince` by the send's own time.
const ACTIVITY_SLACK_MS: i64 = 30_000;
/// Transcript rows read per page, and the most pages read back for the lead's message.
const TURN_PAGE_ROWS: usize = 200;
const TURN_MAX_PAGES: usize = 25;

/// A linked session's transcript rows from the lead's turn on, kept while the file is unchanged.
#[derive(Default)]
struct TurnCache {
    path: Option<std::path::PathBuf>,
    /// The file's length and modification time, and the `owedSince` the rows were read for.
    read_for: Option<((u64, Option<std::time::SystemTime>), i64)>,
    rows: Vec<TurnRow>,
}

/// BLOCKING (transcript reads, only when the file changed). The transcript's rows back to the
/// lead's message: page by page from the end until a row older than `owed_ms` minus the slack, or
/// `TURN_MAX_PAGES` pages.
///
/// CDXC:LeadLinks 2026-11-20 FIX:
/// Opus re-review of #678 (the blocker): the lead's message was looked for in the newest 60 rows
/// only, and every tool call and result is a row, so a real turn (hundreds of rows) never found it
/// and was never reported. The rows are now read back in pages (`before_offset`, as
/// session_chat_history.rs reads them) until they reach the time of the lead's message.
fn lead_turn_rows(cache: &mut TurnCache, session: &Value, owed_ms: i64) -> Vec<TurnRow> {
    use crate::session_chat_tail::{read_session_chat_tail_page, SessionChatTailPage};
    let Some(agent) = crate::coordinators::resolve_thread_transcript(session, &mut cache.path)
    else {
        return Vec::new();
    };
    let Some(path) = cache.path.clone() else {
        return Vec::new();
    };
    let Some(stamp) = std::fs::metadata(&path)
        .ok()
        .map(|metadata| (metadata.len(), metadata.modified().ok()))
    else {
        return Vec::new();
    };
    if cache.read_for == Some((stamp, owed_ms)) {
        return cache.rows.clone();
    }
    let floor = owed_ms - LEAD_TURN_SLACK_MS;
    let mut messages = Vec::new();
    let mut before = None;
    for _ in 0..TURN_MAX_PAGES {
        let Ok(SessionChatTailPage::Page {
            messages: page,
            has_more,
            before_offset,
            ..
        }) = read_session_chat_tail_page(agent, &path, TURN_PAGE_ROWS, before)
        else {
            break;
        };
        let reached = page
            .iter()
            .find_map(|message| message.timestamp)
            .is_some_and(|at| at < floor);
        let mut older = page;
        older.append(&mut messages);
        messages = older;
        if reached || !has_more {
            break;
        }
        before = Some(before_offset);
    }
    cache.rows = turn_rows(&messages);
    cache.read_for = Some((stamp, owed_ms));
    cache.rows.clone()
}

/// The openings of user rows nobody typed: Ghostex's notes, a slash command's echo and output, a
/// background job's notice. Anything else is a message, even one that starts with a tag
/// (`<task>…`).
///
/// CDXC:LeadLinks 2026-11-20 FIX: Opus re-review of #678: every user row starting with `<` was
/// skipped, so a lead message that opened with a tag was never matched and its turn never
/// reported.
const WRAPPER_OPENINGS: &[&str] = &[
    "<ghostex-system>",
    "<command-name>",
    "<command-message>",
    "<command-args>",
    "<local-command",
    "<task-notification>",
    "<system-reminder>",
];

fn is_wrapper(text: &str) -> bool {
    WRAPPER_OPENINGS
        .iter()
        .any(|opening| text.starts_with(opening))
}

/// A transcript's rows as `lead_turn_reply` reads them: what someone typed (a tool result, a
/// background job's notice or a Ghostex note is nobody's message) and the agent's words.
fn turn_rows(messages: &[crate::session_chat::SessionChatMessage]) -> Vec<TurnRow> {
    use crate::session_chat::{SessionChatBlock, SessionChatRole};
    messages
        .iter()
        .filter(|message| !message.queued)
        .filter_map(|message| {
            let text = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    SessionChatBlock::Text { text } => Some(text.trim()),
                    _ => None,
                })
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            match message.role {
                SessionChatRole::User if !text.is_empty() && !is_wrapper(&text) => Some(TurnRow {
                    user: true,
                    text,
                    at: message.timestamp,
                }),
                SessionChatRole::Assistant if !text.is_empty() => Some(TurnRow {
                    user: false,
                    text,
                    at: message.timestamp,
                }),
                _ => None,
            }
        })
        .collect()
}

#[derive(Default)]
struct WatchMemory {
    /// When each owed link was first seen out of work since it last worked.
    idle_since: HashMap<SessionKey, i64>,
    /// When each owed link's session was first seen closed.
    closed_since: HashMap<SessionKey, i64>,
    /// Links with a report being sent right now.
    in_flight: HashSet<SessionKey>,
    /// Leads whose last send failed, and when they may be tried again.
    retry_at: HashMap<SessionKey, i64>,
    /// The send id of each report still on its way, so a retried report is the same send.
    send_ids: HashMap<(SessionKey, String), String>,
    /// Each owed link's transcript tail, read again only when the file changed.
    transcripts: HashMap<SessionKey, TurnCache>,
}

enum LeadReport {
    Finished {
        message: String,
    },
    Waiting {
        key: String,
        summary: String,
    },
    Quiet {
        since: i64,
        minutes: i64,
    },
    Closed,
    /// It was made a coordinator or a coordinator's thread, which ended the link.
    Ended,
}

struct Delivery {
    link: LeadLink,
    report: LeadReport,
    sender: crate::coordinators::MessageSender,
    /// The linked session's row is gone (deleted, or restored under a new id before the restore
    /// carried links): the link ends once this report is delivered.
    ends_link: bool,
}

pub(crate) fn start_lead_runtime(state: Arc<AppState>) {
    let memory = Arc::new(Mutex::new(WatchMemory::default()));
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let (ticking, ticking_memory) = (state.clone(), memory.clone());
            let deliveries = tokio::task::spawn_blocking(move || tick(&ticking, &ticking_memory))
                .await
                .unwrap_or_default();
            for delivery in deliveries {
                tokio::spawn(deliver(state.clone(), memory.clone(), delivery));
            }
        }
    });
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn iso_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.timestamp_millis())
}

fn running_or_sleeping(session: &Value) -> bool {
    matches!(
        effective_lifecycle_state(session).as_str(),
        "running" | "sleeping"
    )
}

/// A link whose lead or linked session became a coordinator or a coordinator's thread since is
/// dropped: those have threads, and one session can't report two ways.
fn became_coordinator_side(db: &rusqlite::Connection, key: &SessionKey) -> bool {
    crate::coordinators::read_coordinator(db, &key.0, &key.1)
        .ok()
        .flatten()
        .is_some()
        || crate::coordinators::read_thread(db, &key.0, &key.1)
            .ok()
            .flatten()
            .is_some()
}

/// BLOCKING (database, and a transcript tail read for an owed link that went idle).
fn tick(state: &AppState, memory: &Mutex<WatchMemory>) -> Vec<Delivery> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let Ok(links) = list_lead_links(&db) else {
        return Vec::new();
    };
    let Ok(mut memory) = memory.lock() else {
        return Vec::new();
    };
    let keys = links.iter().map(LeadLink::key).collect::<HashSet<_>>();
    memory.idle_since.retain(|key, _| keys.contains(key));
    memory.closed_since.retain(|key, _| keys.contains(key));
    memory.transcripts.retain(|key, _| keys.contains(key));
    // CDXC:LeadLinks 2026-11-20 FIX: Opus review of #678, finding 9: the send ids of reports and
    // the retry clocks of leads that went away were kept for good.
    let leads = links.iter().map(LeadLink::lead_key).collect::<HashSet<_>>();
    memory.send_ids.retain(|(key, _), _| keys.contains(key));
    memory.retry_at.retain(|lead, _| leads.contains(lead));
    if links.is_empty() {
        return Vec::new();
    }
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let now = now_ms();
    let now_iso = crate::presentation::now_iso();
    let mut names: Option<HashMap<String, String>> = None;
    let mut projects: HashMap<String, Option<Value>> = HashMap::new();
    let mut deliveries = Vec::new();
    for link in links {
        let key = link.key();
        let lead_key = link.lead_key();
        let lead = repository
            .get_session(&lead_key.0, &lead_key.1)
            .ok()
            .flatten();
        // The linked session was made a coordinator or a coordinator's thread: it reports another
        // way now, and its lead is told the link ended (Opus review of #678, finding 9).
        let linked_turned = became_coordinator_side(&db, &key);
        if linked_turned
            && lead.as_ref().is_some_and(running_or_sleeping)
            && !became_coordinator_side(&db, &lead_key)
        {
            if !memory.in_flight.contains(&key)
                && !memory.retry_at.get(&lead_key).is_some_and(|at| now < *at)
            {
                if let Some(session) = repository.get_session(&key.0, &key.1).ok().flatten() {
                    let sender = sender_of(
                        state,
                        &repository,
                        &mut projects,
                        &mut names,
                        &session,
                        &key,
                    );
                    memory.in_flight.insert(key.clone());
                    deliveries.push(Delivery {
                        link,
                        report: LeadReport::Ended,
                        sender,
                        ends_link: true,
                    });
                    continue;
                }
            } else {
                continue;
            }
        }
        // A lead that is gone for good, or either side turned coordinator: the link ends.
        if lead.is_none() || became_coordinator_side(&db, &lead_key) || linked_turned {
            let _ = remove_lead_link(&db, &key.0, &key.1);
            for (project_id, session_id) in [&key, &lead_key] {
                let _ = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                );
            }
            continue;
        }
        // A closed lead has nobody to report to; its reports wait until it is resumed.
        if !lead.as_ref().is_some_and(running_or_sleeping) {
            continue;
        }
        if memory.in_flight.contains(&key)
            || memory.retry_at.get(&lead_key).is_some_and(|at| now < *at)
        {
            continue;
        }
        let session = repository.get_session(&key.0, &key.1).ok().flatten();
        // CDXC:LeadLinks 2026-11-20 FIX: Opus review of #678, finding 7: a link whose linked
        // session's row is gone stayed for good (the lead's count, its Stop leading label, and an
        // unlink by a ref that no longer resolves). It ends now: at once when nothing is owed,
        // after its "closed" report when a turn is.
        let gone = session.is_none();
        // Only a turn the lead started is reported; the user's own turns stay with them.
        let Some(owed_ms) = link.owed_since.as_deref().and_then(iso_ms) else {
            memory.idle_since.remove(&key);
            memory.closed_since.remove(&key);
            if gone {
                let _ = remove_lead_link(&db, &key.0, &key.1);
                let _ = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    &lead_key.0,
                    &lead_key.1,
                );
            }
            continue;
        };
        let lifecycle = session
            .as_ref()
            .map(effective_lifecycle_state)
            .unwrap_or_else(|| "stopped".to_string());
        if lifecycle == "stopped" {
            memory.idle_since.remove(&key);
            let since = *memory.closed_since.entry(key.clone()).or_insert(now);
            if now - since >= CLOSED_GRACE_MS {
                let sender = match &session {
                    Some(session) => {
                        sender_of(state, &repository, &mut projects, &mut names, session, &key)
                    }
                    None => crate::coordinators::MessageSender {
                        session_id: key.1.clone(),
                        global_ref: crate::ids::create_global_session_ref(
                            state.metadata.server_id.as_str(),
                            &key.0,
                            &key.1,
                        ),
                        ..Default::default()
                    },
                };
                memory.in_flight.insert(key.clone());
                deliveries.push(Delivery {
                    link,
                    report: LeadReport::Closed,
                    sender,
                    ends_link: gone,
                });
            }
            continue;
        }
        memory.closed_since.remove(&key);
        let Some(session) = session else {
            continue;
        };
        if !running_or_sleeping(&session) {
            continue;
        }
        let as_run = ThreadProgress {
            resolved: false,
            has_run: true,
        };
        let hook_state = classify_thread_session(Some(&session), as_run, &now_iso, false);
        let lead_ref = crate::ids::create_global_session_ref(
            state.metadata.server_id.as_str(),
            &lead_key.0,
            &lead_key.1,
        );
        // The lead's turn as the transcript shows it, read only where a report could go out.
        let lead_turn_now = |memory: &mut WatchMemory| {
            let rows = lead_turn_rows(
                memory.transcripts.entry(key.clone()).or_default(),
                &session,
                owed_ms,
            );
            lead_turn(&rows, &lead_ref, owed_ms)
        };
        let report = match hook_state {
            ThreadState::Working => {
                memory.idle_since.remove(&key);
                if !link.observed_working {
                    let _ = set_link_observed_working(&db, &key, true);
                }
                crate::coordinators::session_quiet(&key.0, &key.1)
                    .filter(|quiet| quiet.stalled)
                    .filter(|quiet| link.reported_quiet_since != Some(quiet.quiet_since))
                    // Only while it works on the lead's turn, not on the user's own.
                    .filter(|_| lead_turn_now(&mut memory).is_some_and(|turn| !turn.user_after))
                    .map(|quiet| LeadReport::Quiet {
                        since: quiet.quiet_since,
                        minutes: crate::coordinators::minutes_of(now - quiet.quiet_since).max(1),
                    })
            }
            ThreadState::Waiting => {
                memory.idle_since.remove(&key);
                let (prompt_key, summary) = waiting_report_prompt(&session);
                // Only a question of the lead's turn, never one of the user's own.
                (link.reported_prompt_key.as_deref() != Some(prompt_key.as_str())
                    && lead_turn_now(&mut memory).is_some_and(|turn| !turn.user_after))
                .then_some(LeadReport::Waiting {
                    key: prompt_key,
                    summary,
                })
            }
            ThreadState::Finished | ThreadState::Sleeping => {
                let since = *memory.idle_since.entry(key.clone()).or_insert(now);
                if now - since < FINISH_STABILITY_MS {
                    None
                } else {
                    // The reply to the lead's own message (`lead_turn_reply`): a turn too short for
                    // a tick to see it working still counts, and a turn the user started after it
                    // is never the one sent.
                    // Nothing moved since the lead's message: no reply to read yet (Opus review of
                    // #678, finding 9). A row with no activity stamp is read anyway.
                    let moved = super::coordinator_remote_runtime::activity_marker_ms(&session)
                        .is_none_or(|at| at + ACTIVITY_SLACK_MS >= owed_ms);
                    moved
                        .then(|| lead_turn_now(&mut memory))
                        .flatten()
                        .and_then(|turn| turn.reply)
                        .map(|message| LeadReport::Finished { message })
                }
            }
            _ => None,
        };
        if let Some(report) = report {
            let sender = sender_of(
                state,
                &repository,
                &mut projects,
                &mut names,
                &session,
                &key,
            );
            memory.in_flight.insert(key.clone());
            deliveries.push(Delivery {
                link,
                report,
                sender,
                ends_link: false,
            });
        }
    }
    deliveries
}

fn sender_of(
    state: &AppState,
    repository: &DomainRepository<'_>,
    projects: &mut HashMap<String, Option<Value>>,
    names: &mut Option<HashMap<String, String>>,
    session: &Value,
    key: &SessionKey,
) -> crate::coordinators::MessageSender {
    let project = projects
        .entry(key.0.clone())
        .or_insert_with(|| repository.get_project(&key.0).ok().flatten())
        .clone();
    session_sender(
        state,
        project.as_ref(),
        session,
        key,
        names.get_or_insert_with(|| agent_names(repository)),
    )
}

fn report_text(report: &LeadReport, linked_ref: &str) -> String {
    match report {
        LeadReport::Finished { message } => report_body(
            &ThreadReport::Finished {
                message: Some(message.as_str()),
            },
            linked_ref,
        ),
        LeadReport::Waiting { summary, .. } => {
            report_body(&ThreadReport::Waiting { prompt: summary }, linked_ref)
        }
        LeadReport::Quiet { since, minutes } => format!(
            "Ghostex thread report: no new output for {minutes} minutes (quiet since {}). Ghostex did not nudge it. Read where it is with: ghostex read-session-chat {linked_ref} --last 2 --format text",
            crate::network_status::local_clock(*since),
        ),
        LeadReport::Ended => "Ghostex thread report: it was made a coordinator (or a coordinator's thread), so it no longer reports to you and its link to you ended.".to_string(),
        LeadReport::Closed => format!(
            "Ghostex thread report: its session was closed before it finished the turn you started. A message to it (ghostex agents send {linked_ref} \"...\") resumes the same conversation."
        ),
    }
}

async fn deliver(state: Arc<AppState>, memory: Arc<Mutex<WatchMemory>>, delivery: Delivery) {
    let key = delivery.link.key();
    let lead_key = delivery.link.lead_key();
    let body = report_text(&delivery.report, &delivery.sender.global_ref);
    let message = agent_message(&delivery.sender, &body);
    let ledger_key = (key.clone(), message.clone());
    let send_id = memory
        .lock()
        .map(|mut memory| {
            memory
                .send_ids
                .entry(ledger_key.clone())
                .or_insert_with(|| Uuid::new_v4().to_string())
                .clone()
        })
        .unwrap_or_else(|_| Uuid::new_v4().to_string());
    let sent = send_to_coordinator(&state, &lead_key, &message, &send_id).await;
    if sent.is_ok() {
        let settle_state = state.clone();
        let link = delivery.link.clone();
        let report = delivery.report;
        let ends_link = delivery.ends_link;
        let _ = tokio::task::spawn_blocking(move || {
            let Ok(db) = open_gxserver_database(&settle_state.paths) else {
                return;
            };
            // The link may have ended while the report was on its way.
            if !read_lead_link(&db, &link.project_id, &link.session_id)
                .ok()
                .flatten()
                .is_some_and(|now| now.lead_key() == link.lead_key())
            {
                return;
            }
            let key = link.key();
            if ends_link {
                let _ = remove_lead_link(&db, &key.0, &key.1);
                let repository =
                    DomainRepository::new(&db, settle_state.metadata.server_id.as_str());
                let lead = link.lead_key();
                let _ = schedule_presentation_session_delta(
                    &settle_state,
                    &db,
                    &repository,
                    &lead.0,
                    &lead.1,
                );
                return;
            }
            let _ = match &report {
                LeadReport::Finished { .. } | LeadReport::Closed | LeadReport::Ended => {
                    settle_finished(&db, &key)
                }
                LeadReport::Waiting { key: prompt, .. } => settle_waiting(&db, &key, prompt),
                LeadReport::Quiet { since, .. } => settle_quiet(&db, &key, *since),
            };
        })
        .await;
    }
    if let Ok(mut memory) = memory.lock() {
        memory.in_flight.remove(&key);
        match &sent {
            Ok(()) => {
                memory.send_ids.remove(&ledger_key);
                memory.retry_at.remove(&lead_key);
                memory.idle_since.remove(&key);
            }
            Err(_) => {
                memory
                    .retry_at
                    .insert(lead_key.clone(), now_ms() + RETRY_AFTER_MS);
            }
        }
    }
    if let Err(error) = sent {
        let _ = state.logger.log_routine(
            crate::logging::DiagnosticLogScenario::ServerLifecycle,
            GxserverLogInput {
                level: LogLevel::Warn,
                event: "leadReportDeferred".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error.chars().take(300).collect()),
                details: Some(json!({
                    "leadProjectId": lead_key.0,
                    "leadSessionId": lead_key.1,
                    "linkedSessionId": key.1,
                })),
            },
        );
    }
}
