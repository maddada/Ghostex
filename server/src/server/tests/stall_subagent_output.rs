//! A working OMP lead that waits on its subagents is "not moving" only while they are quiet too:
//! a subagent's transcript write counts as its lead's output in the stall watch, a subagent that
//! stopped writing does not hold its lead off, a watch rebuilt after a gxserver restart starts at
//! the newest subagent write, a lead quiet with its subagents still gets its bell row and push,
//! and a stall report a subagent's later write outdated is dropped, right before it is sent and
//! when it waited in its coordinator's queue.
//! Synthetic OMP transcripts (`<lead>.jsonl`, children in `<lead>/<name>.jsonl`) with set file
//! times; the clock the watch is given is the same one.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::*;
use crate::coordinators::{replace_session_quiet, QuietOwner, StallWatch, SESSION_STUCK_ROUNDS};
use crate::domain::DomainRepository;
use crate::notification_feed::store::read_notification_feed_state;
use crate::push::{expo::ExpoClient, fake_expo::FakeExpo, runtime::SystemClock, PushRuntime};
use crate::storage::open_gxserver_database;

const MIN: i64 = 60_000;
/// The lead's last write of its own; every time below counts from it.
const T0: i64 = 1_795_000_000_000;
const TOKEN: &str = "ExponentPushToken[stallSubagent01]";

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Appends one transcript entry to `path` and sets the file's time to `at` (ms).
fn write_at(path: &Path, entry: Value, at: i64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(file, "{entry}").unwrap();
    file.set_modified(std::time::UNIX_EPOCH + Duration::from_millis(at as u64))
        .unwrap();
}

/// An OMP lead transcript, last written at `T0`, and the folder OMP writes its subagents into.
struct Lead {
    _temp: tempfile::TempDir,
    transcript: PathBuf,
    folder: PathBuf,
}

fn omp_lead() -> Lead {
    let temp = tempfile::tempdir().unwrap();
    let transcript = temp
        .path()
        .join("sessions/2026-11-24T00-00-00-000Z_lead.jsonl");
    write_at(
        &transcript,
        json!({"type":"session","version":3,"id":"lead","timestamp":iso(T0),"cwd":"/work"}),
        T0,
    );
    Lead {
        folder: transcript.with_extension(""),
        transcript,
        _temp: temp,
    }
}

/// The subagent `name` writes a row to its own transcript at `at`.
fn subagent_writes(lead: &Lead, name: &str, at: i64) {
    write_at(
        &lead.folder.join(format!("{name}.jsonl")),
        json!({"type":"message","id":format!("m{at}"),"timestamp":iso(at),
            "message":{"role":"assistant","content":[{"type":"text","text":"Reading the files."}]}}),
        at,
    );
}

/// A working OMP session on `lead`, as the watch reads it.
fn omp_session(lead: &Lead) -> Value {
    json!({
        "projectId": "p-stall-subagent",
        "sessionId": "s-stall-subagent",
        "launchSettings": { "icon": "omp" },
        "runtimeSettings": {
            "activity": "working",
            "agentSessionId": "lead",
            "agentSessionPath": lead.transcript,
            "agentActivity": { "activity": "working", "lastChangedAt": iso(T0) },
        },
    })
}

/// A gxserver with a phone registered for every alert kind, and a working OMP session on `lead`
/// in its database.
struct Server {
    state: Arc<AppState>,
    expo: FakeExpo,
    key: (String, String),
    _temp: tempfile::TempDir,
}

impl Server {
    async fn new(lead: &Lead) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let expo = FakeExpo::start(Arc::new(SystemClock)).await;
        let push = PushRuntime::start_with(
            paths.clone(),
            Arc::new(GxserverLogger::new(paths.clone())),
            Arc::new(SystemClock),
            ExpoClient::for_test_origin(expo.origin.clone()),
        );
        let state = test_app_state_with_push(paths, Arc::new(push));
        let routed = route_http(
            state.clone(),
            rpc_request(
                "/api/registerPushToken",
                &state.auth_token,
                json!({ "protocolVersion": 1, "params": {
                    "action": "register", "token": TOKEN, "machineId": "computer-S",
                    "kinds": { "finished": true, "waiting": true, "error": true },
                } }),
            ),
            "request-register".to_string(),
        )
        .await;
        assert_eq!(routed.response.status(), StatusCode::OK);
        let db = open_gxserver_database(&state.paths).unwrap();
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project_id = repository
            .create_project(
                json!({ "name": "Subagents", "path": temp.path() })
                    .as_object()
                    .unwrap(),
            )
            .unwrap()["projectId"]
            .as_str()
            .unwrap()
            .to_string();
        let session_id = repository
            .create_session(
                json!({
                    "projectId": project_id, "kind": "agent", "agentId": "omp",
                    "title": "Lead", "lifecycleState": "running",
                    "runtimeSettings": {
                        "agentSessionId": "lead", "agentSessionPath": lead.transcript,
                        "agentActivity": { "activity": "working" },
                    },
                })
                .as_object()
                .unwrap(),
                false,
            )
            .unwrap()["sessionId"]
            .as_str()
            .unwrap()
            .to_string();
        Self {
            state,
            expo,
            key: (project_id, session_id),
            _temp: temp,
        }
    }

    fn session(&self) -> Value {
        let db = open_gxserver_database(&self.state.paths).unwrap();
        DomainRepository::new(&db, self.state.metadata.server_id.as_str())
            .get_session(&self.key.0, &self.key.1)
            .unwrap()
            .unwrap()
    }

    /// Publishes the watch's reading as the sidebar-wide stall watch does each tick: a session
    /// that became stuck gets its bell row and its push.
    fn publish(&self, watch: &StallWatch, now: i64) {
        let changes = replace_session_quiet(
            QuietOwner::Sessions,
            HashMap::from([(self.key.clone(), watch.view(now, SESSION_STUCK_ROUNDS))]),
        );
        let db = open_gxserver_database(&self.state.paths).unwrap();
        let repository = DomainRepository::new(&db, self.state.metadata.server_id.as_str());
        crate::server::session_stall_runtime::publish_stuck_changes(
            &self.state,
            &db,
            &repository,
            QuietOwner::Sessions,
            changes,
        );
        let push = &self.state.push;
        assert!(
            tokio::task::block_in_place(|| push.wait_until_idle(Duration::from_secs(10))),
            "the push worker did not settle"
        );
    }

    fn rows(&self) -> Vec<Value> {
        let db = open_gxserver_database(&self.state.paths).unwrap();
        read_notification_feed_state(&db).unwrap()["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row["sessionId"] == self.key.1.as_str())
            .collect()
    }

    fn alert_bodies(&self) -> Vec<Value> {
        self.expo
            .sends()
            .iter()
            .map(|request| request.body.as_array().unwrap()[0].clone())
            .filter(|message| message.get("title").is_some())
            .map(|message| message["body"].clone())
            .collect()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lead_whose_subagent_keeps_writing_is_not_stuck_and_sends_no_push() {
    let lead = omp_lead();
    subagent_writes(&lead, "Worker", T0);
    let server = Server::new(&lead).await;
    let mut watch = StallWatch::new(T0);
    // The lead writes nothing of its own for 20 minutes, 2.5 rounds; its subagent writes every
    // minute.
    for minute in 0..=20 {
        let now = T0 + minute * MIN;
        if minute > 0 {
            subagent_writes(&lead, "Worker", now);
        }
        assert!(
            watch.observe(&server.session(), now).is_none(),
            "nothing is due at minute {minute}"
        );
        let view = watch.view(now, SESSION_STUCK_ROUNDS);
        assert!(
            !view.stalled && !view.stuck,
            "no red dot at minute {minute}: {view:?}"
        );
    }
    server.publish(&watch, T0 + 20 * MIN);
    assert!(server.rows().is_empty(), "{:?}", server.rows());
    assert!(
        server.alert_bodies().is_empty(),
        "{:?}",
        server.alert_bodies()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lead_quiet_with_its_subagents_turns_stuck_with_its_row_and_push() {
    let lead = omp_lead();
    subagent_writes(&lead, "Worker", T0);
    let server = Server::new(&lead).await;
    let mut watch = StallWatch::new(T0);
    for minute in 0..8 {
        let now = T0 + minute * MIN;
        assert!(watch.observe(&server.session(), now).is_none());
        assert!(
            !watch.view(now, SESSION_STUCK_ROUNDS).stuck,
            "minute {minute}"
        );
    }
    let now = T0 + 8 * MIN;
    let due = watch
        .observe(&server.session(), now)
        .expect("due after a round of quiet");
    assert_eq!((due.quiet_since, due.round), (T0, 1));
    assert!(watch.view(now, SESSION_STUCK_ROUNDS).stuck);
    server.publish(&watch, now);
    assert_eq!(server.rows().len(), 1, "{:?}", server.rows());
    assert_eq!(
        server.alert_bodies(),
        vec![json!("Not moving: it may be stuck")]
    );
}

#[test]
fn a_subagent_that_stopped_writing_does_not_hold_its_lead_off() {
    let lead = omp_lead();
    let session = omp_session(&lead);
    let mut watch = StallWatch::new(T0);
    for minute in 0..=10 {
        let now = T0 + minute * MIN;
        // The subagent writes for three minutes and then hangs.
        if (1..=3).contains(&minute) {
            subagent_writes(&lead, "Worker", now);
        }
        assert!(watch.observe(&session, now).is_none(), "minute {minute}");
        assert!(
            !watch.view(now, SESSION_STUCK_ROUNDS).stuck,
            "not stuck at minute {minute}"
        );
    }
    // A round after the subagent's last write the lead is stuck, its subagent still listed.
    let now = T0 + 11 * MIN;
    let due = watch
        .observe(&session, now)
        .expect("due a round after the last write");
    assert_eq!((due.quiet_since, due.round), (T0 + 3 * MIN, 1));
    let view = watch.view(now, SESSION_STUCK_ROUNDS);
    assert!(view.stuck);
    assert_eq!(view.quiet_since, T0 + 3 * MIN);
}

#[test]
fn a_watch_rebuilt_after_a_restart_starts_at_the_newest_subagent_write() {
    let lead = omp_lead();
    subagent_writes(&lead, "Worker", T0 + MIN);
    subagent_writes(&lead, "Checker", T0 + 5 * MIN);
    let session = omp_session(&lead);
    // gxserver restarts at minute 10.
    let mut watch = StallWatch::resumed(&session, T0 + 10 * MIN);
    assert_eq!(watch.quiet_since(), T0 + 5 * MIN);
    for minute in 10..13 {
        assert!(
            watch.observe(&session, T0 + minute * MIN).is_none(),
            "minute {minute}"
        );
    }
    let due = watch
        .observe(&session, T0 + 13 * MIN)
        .expect("due a round after the newest subagent write");
    assert_eq!((due.quiet_since, due.round), (T0 + 5 * MIN, 1));
}

/// A working OMP lead that was due a stall report at minute 8 (quiet since `T0`), as the
/// coordinator supervisor and the queue check read it.
fn due_thread(lead: &Lead) -> (Value, crate::coordinators::StallDue) {
    let mut session = omp_session(lead);
    session["lifecycleState"] = json!("running");
    let mut watch = StallWatch::new(T0);
    assert!(watch.observe(&session, T0).is_none());
    let due = watch
        .observe(&session, T0 + 8 * MIN)
        .expect("due after a round of quiet");
    assert_eq!(due.quiet_since, T0);
    (session, due)
}

#[test]
fn a_stall_report_is_dropped_before_it_is_sent_when_a_subagent_wrote_since() {
    let lead = omp_lead();
    let (session, due) = due_thread(&lead);
    let check =
        || crate::coordinators::stall_last_row(&session, due.transcript_seen, due.quiet_since);
    assert!(check().is_some(), "the report holds while all are quiet");
    // The subagent writes a second after the report was worded, before it goes out.
    subagent_writes(&lead, "Worker", T0 + 8 * MIN + 1_000);
    assert_eq!(check(), None, "a subagent's write drops the report");
}

#[test]
fn a_queued_stall_report_is_dropped_when_a_subagent_wrote_since() {
    use crate::coordinators::{
        quiet_report_stale_reason, QuietReport, QuietReportKind, ThreadRecord,
    };

    let lead = omp_lead();
    let (session, due) = due_thread(&lead);
    let thread = ThreadRecord {
        project_id: "p-stall-subagent".into(),
        session_id: "s-stall-subagent".into(),
        coordinator_project_id: "p-stall-subagent".into(),
        coordinator_session_id: "coordinator".into(),
        task: "task".into(),
        resolved_at: None,
        last_resolved_at: None,
        observed_working: true,
        reported_at: None,
        reported_prompt_key: None,
        last_report: None,
        last_note: None,
        last_note_at: None,
        pending_message: None,
        pending_message_at: None,
        pending_message_send_id: None,
        closed_with_coordinator_at: None,
        created_at: iso(T0),
        updated_at: iso(T0),
    };
    let report = QuietReport {
        kind: QuietReportKind::Stalled,
        thread_ref: "S:p-stall-subagent:s-stall-subagent".into(),
        since: due.quiet_since,
    };
    // The report waits in the coordinator's queue and is checked 20 minutes on.
    let check =
        || quiet_report_stale_reason(&report, Some(&thread), Some(&session), &iso(T0 + 20 * MIN));
    assert_eq!(check(), None, "the report holds while all are quiet");
    subagent_writes(&lead, "Worker", T0 + 9 * MIN);
    assert_eq!(
        check(),
        Some("the thread moved on after the time the report names")
    );
}
