//! An approval that exists only on a session's screen makes that session read as waiting for the
//! user, whether or not a coordinator watches it. The screen is the one of 2026-11-21: OMP asks to
//! run `commit` from an eval cell, a path that raises no hook, so the session's hooks keep saying
//! working. Each test serves the screen through the session's zmx capture socket and reads the
//! presentation every client draws the red dot from.

use super::*;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicBool, Ordering};

/// The screen as Ghostex read it (24 × 200), from the report copied at 12:30:30 +01:00.
const APPROVAL_SCREEN: &str =
    include_str!("../../session_chat_tests/fixtures/omp-eval-commit-approval.txt");

/// The same screen once the approval is answered: OMP's box is gone and its input box is back.
fn answered_screen() -> String {
    let lines: Vec<&str> = APPROVAL_SCREEN.lines().collect();
    let top = lines
        .iter()
        .position(|line| line.starts_with("╭─ Allow tool: commit"))
        .expect("the approval box");
    let bottom = top
        + lines[top..]
            .iter()
            .position(|line| line.starts_with('╰'))
            .expect("its last line");
    let mut kept = lines[..top].to_vec();
    kept.extend_from_slice(&lines[bottom + 1..]);
    kept.join("\n") + "\n"
}

/// The session's zmx daemon, as far as a screen capture sees it: it answers every capture with
/// whatever `screen` holds.
struct ScreenDaemon {
    screen: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
    socket: std::path::PathBuf,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ScreenDaemon {
    fn serve(zmx_name: &str, screen: &str) -> Self {
        let socket = crate::zmx::zmx_session_socket_path(zmx_name);
        std::fs::create_dir_all(socket.parent().expect("socket folder")).expect("socket folder");
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).expect("capture socket");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let screen = Arc::new(Mutex::new(screen.to_string()));
        let stop = Arc::new(AtomicBool::new(false));
        let (serving, stopping) = (screen.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // macOS hands the accepted stream the listener's nonblocking flag.
                        stream.set_nonblocking(false).expect("blocking stream");
                        stream
                            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                            .expect("read timeout");
                        let mut request = [0; 24];
                        if stream.read_exact(&mut request[..8]).is_err() || request[0] != 22 {
                            continue;
                        }
                        stream
                            .read_exact(&mut request[8..])
                            .expect("capture request");
                        let text = serving.lock().expect("screen").clone();
                        stream
                            .write_all(&crate::zmx::ipc_frame::zmx_ipc_header(
                                22,
                                text.len() as u32,
                            ))
                            .expect("capture header");
                        stream.write_all(text.as_bytes()).expect("capture text");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(error) => panic!("capture listener: {error}"),
                }
            }
        });
        Self {
            screen,
            stop,
            socket,
            thread: Some(thread),
        }
    }

    fn show(&self, screen: &str) {
        *self.screen.lock().expect("screen") = screen.to_string();
    }
}

impl Drop for ScreenDaemon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

struct Fixture {
    state: Arc<AppState>,
    project: Value,
    session_id: String,
    zmx_name: String,
    _temp: tempfile::TempDir,
}

impl Fixture {
    /// A running OMP session nobody coordinates, its hooks at working (the eval call started).
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let db = open_gxserver_database(&state.paths).expect("db");
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project = repository
            .create_project(
                json!({ "name": "Approvals", "path": temp.path() })
                    .as_object()
                    .expect("params"),
            )
            .expect("project");
        let session = repository
            .create_session(
                json!({
                    "projectId": project["projectId"], "kind": "agent", "agentId": "omp",
                    "title": "Commit review evidence", "lifecycleState": "running",
                    "runtimeSettings": {
                        "agentName": "omp",
                        "activity": "working",
                        "agentActivity": { "activity": "working" },
                    },
                })
                .as_object()
                .expect("params"),
                false,
            )
            .expect("session");
        Self {
            session_id: session["sessionId"]
                .as_str()
                .expect("sessionId")
                .to_string(),
            zmx_name: session["zmxName"].as_str().expect("zmxName").to_string(),
            state,
            project,
            _temp: temp,
        }
    }

    fn project_id(&self) -> &str {
        self.project["projectId"].as_str().expect("projectId")
    }

    /// One read of the screen through the detector every chat read, follower and tick uses.
    fn read_screen(&self) {
        crate::session_chat_options::SessionChatOptionDetector::new(&self.state).detect_blocking(
            self.project_id(),
            &self.session_id,
            Some("omp"),
            true,
        );
    }

    fn stored(&self) -> Value {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        DomainRepository::new(&db, self.state.metadata.server_id.as_str())
            .get_session(self.project_id(), &self.session_id)
            .expect("read session")
            .expect("session")
    }

    /// The presentation row's `awaitingApproval`, which the desktop, web and phone sidebars draw
    /// the red dot from.
    fn awaiting_approval(&self) -> Option<Value> {
        let generated_at = crate::presentation::now_iso();
        crate::presentation::project_presentation_session(
            &self.project,
            "group",
            &self.stored(),
            &generated_at,
        )
        .get("awaitingApproval")
        .cloned()
    }
}

#[test]
fn a_standalone_omp_session_showing_an_approval_waits_for_the_user() {
    let fixture = Fixture::new();
    let mut events = fixture.state.event_hub.subscribe();
    let daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    fixture.read_screen();
    // The chat reads the approval off the screen, as the report's "Terminal notice" line says.
    let notice = crate::session_chat_options::cached_session_chat_terminal_notice(
        &fixture.state,
        fixture.project_id(),
        &fixture.session_id,
    )
    .expect("the screen notice");
    assert_eq!(
        (notice.kind.as_str(), notice.title.as_str()),
        ("permissionPrompt", "Allow this?")
    );
    assert_eq!(
        fixture.stored()["runtimeSettings"]["agentActivity"]["activity"],
        json!("working"),
        "no hook told Ghostex about the approval"
    );
    assert_eq!(fixture.awaiting_approval(), Some(json!(true)));
    // Every client hears it at once, not only on the session's next change.
    let mut announced = false;
    while let Ok(event) = events.try_recv() {
        let event = event.to_string();
        if event.contains("\"presentationDelta\"")
            && event.contains(&fixture.session_id)
            && event.contains("\"awaitingApproval\":true")
        {
            announced = true;
        }
    }
    assert!(
        announced,
        "a presentation delta carries the waiting approval"
    );

    // Answered: the box is gone, so the session works again.
    daemon.show(&answered_screen());
    fixture.read_screen();
    assert_eq!(fixture.awaiting_approval(), None);
}

#[test]
fn a_session_without_an_approval_on_screen_does_not_wait() {
    let fixture = Fixture::new();
    let _daemon = ScreenDaemon::serve(&fixture.zmx_name, &answered_screen());
    fixture.read_screen();
    assert_eq!(fixture.awaiting_approval(), None);
}

/// The refresh helper alone: a mark the detector had already seen clear (its cache agrees with the
/// screen) while the stored mark lost a race is still removed.
#[test]
fn the_refresh_clears_a_mark_the_detector_already_saw_clear() {
    let fixture = Fixture::new();
    let _daemon = ScreenDaemon::serve(&fixture.zmx_name, &answered_screen());
    fixture.read_screen();
    let db = open_gxserver_database(&fixture.state.paths).expect("db");
    db.execute(
        r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatScreenApprovalDetectedAt', '2026-11-21T11:30:00.000Z')
        WHERE projectId = ?1 AND sessionId = ?2"#,
        rusqlite::params![fixture.project_id(), fixture.session_id],
    )
    .expect("a stale mark");
    assert_eq!(fixture.awaiting_approval(), Some(json!(true)));
    crate::session_chat_compacting::refresh_screen_approval_marker_from_screen(
        &fixture.state,
        &fixture.stored(),
    );
    assert_eq!(fixture.awaiting_approval(), None);
}

/// Marks `fixture`'s session as showing an approval without any reading in memory, as a gxserver
/// restart leaves it: the mark is stored, the screen cache is empty.
fn stored_mark_only(fixture: &Fixture) {
    let db = open_gxserver_database(&fixture.state.paths).expect("db");
    db.execute(
        r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatScreenApprovalDetectedAt', '2026-11-21T11:30:00.000Z')
        WHERE projectId = ?1 AND sessionId = ?2"#,
        rusqlite::params![fixture.project_id(), fixture.session_id],
    )
    .expect("a stored mark");
}

/// The approval was answered in a terminal nobody follows: no chat, no follower, only the
/// sidebar-wide sweep that runs every few seconds. Its read of the screen takes the red dot away.
#[test]
fn the_periodic_sweep_clears_an_answered_approval_nobody_watches() {
    let fixture = Fixture::new();
    let _daemon = ScreenDaemon::serve(&fixture.zmx_name, &answered_screen());
    stored_mark_only(&fixture);
    assert!(fixture
        .state
        .session_chat_followers
        .lock()
        .expect("followers")
        .is_empty());
    assert_eq!(fixture.awaiting_approval(), Some(json!(true)));
    crate::server::session_stall_runtime::tick(&fixture.state, &Mutex::new(HashMap::new()));
    assert_eq!(fixture.awaiting_approval(), None);
    // A sweep over an approval still on the screen keeps it.
    let waiting = Fixture::new();
    let _shown = ScreenDaemon::serve(&waiting.zmx_name, APPROVAL_SCREEN);
    stored_mark_only(&waiting);
    crate::server::session_stall_runtime::tick(&waiting.state, &Mutex::new(HashMap::new()));
    assert_eq!(waiting.awaiting_approval(), Some(json!(true)));
}

/// Everything that drops a session's screen reading (agent switch, account restart or sign-in,
/// self-heal restart, removal) drops the mark with it.
#[test]
fn dropping_the_screen_reading_clears_the_mark() {
    let fixture = Fixture::new();
    let _daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    fixture.read_screen();
    assert_eq!(fixture.awaiting_approval(), Some(json!(true)));
    crate::session_chat_options::forget_session_chat_options(
        &fixture.state,
        fixture.project_id(),
        &fixture.session_id,
    );
    assert_eq!(fixture.awaiting_approval(), None);
    assert!(fixture.stored()["runtimeSettings"]
        .get("sessionChatScreenApprovalDetectedAt")
        .is_none());
}

fn screen_notice(
    fixture: &Fixture,
) -> Option<crate::session_chat_notice::SessionChatTerminalNotice> {
    crate::session_chat_options::cached_session_chat_terminal_notice(
        &fixture.state,
        fixture.project_id(),
        &fixture.session_id,
    )
}

/// OMP draws two eval `commit` requests alike. Once a whole read saw the first one gone, the
/// second is its own request with its own identity (`detectedAt`, the key a closed card is
/// remembered by), so a card closed for the first does not hide it. A read that failed proves
/// nothing, so the same box after it is still the first request.
#[test]
fn a_same_looking_approval_after_a_clear_screen_is_a_new_request() {
    let fixture = Fixture::new();
    let daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    fixture.read_screen();
    let first = screen_notice(&fixture).expect("the first commit approval");
    drop(daemon);
    fixture.read_screen();
    let daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    fixture.read_screen();
    let again = screen_notice(&fixture).expect("the first approval after a failed read");
    assert_eq!(again.detected_at, first.detected_at);

    daemon.show(&answered_screen());
    fixture.read_screen();
    assert!(
        screen_notice(&fixture).is_none(),
        "a whole read saw it gone"
    );
    // Detection stamps are milliseconds; the second request comes later.
    std::thread::sleep(std::time::Duration::from_millis(5));
    daemon.show(APPROVAL_SCREEN);
    fixture.read_screen();
    let second = screen_notice(&fixture).expect("the second commit approval");
    assert_eq!(
        (second.kind.as_str(), second.title.as_str()),
        ("permissionPrompt", "Allow this?")
    );
    assert_ne!(second.detected_at, first.detected_at);
}

/// A chat as every Ghostex client folds it: the chat core, fed the frames one follower sent.
struct ChatClient {
    core: ghostex_gx_chat_core::ChatCore,
    folded: usize,
}

impl ChatClient {
    fn open(fixture: &Fixture) -> Self {
        let mut client = Self {
            core: ghostex_gx_chat_core::ChatCore::new(),
            folded: 0,
        };
        let start: ghostex_gx_chat_core::StartConfig = serde_json::from_value(json!({
            "clientId": "client-1",
            "projectId": fixture.project_id(),
            "sessionId": fixture.session_id,
        }))
        .expect("the start config parses");
        client.drive(ghostex_gx_chat_core::Event::Start(Box::new(start)));
        let boot: ghostex_gx_chat_core::ComposerBootRead = serde_json::from_value(json!({
            "sessionKey": format!("{}:{}", fixture.project_id(), fixture.session_id),
            "clientId": "client-1",
        }))
        .expect("the boot read parses");
        client.drive(ghostex_gx_chat_core::Event::ComposerBootRead(Box::new(
            boot,
        )));
        client
    }

    fn drive(&mut self, event: ghostex_gx_chat_core::Event) -> Vec<ghostex_gx_chat_core::Effect> {
        let now_ms = chrono::Utc::now().timestamp_millis() as f64;
        self.core
            .handle(event, ghostex_gx_chat_core::ChatContext::at(now_ms))
    }

    /// Folds the frames it has not seen yet, in the order the follower sent them.
    fn fold(&mut self, frames: &Mutex<Vec<Value>>) {
        use ghostex_gx_chat_core::ChatFrame;
        let frames = frames.lock().expect("frames").clone();
        for frame in &frames[self.folded..] {
            let parsed = match frame["type"].as_str() {
                Some("sessionChatSnapshot") => ChatFrame::Snapshot(Box::new(
                    serde_json::from_value(frame.clone()).expect("snapshot frame"),
                )),
                Some("sessionChatReplaced") => ChatFrame::Replaced(Box::new(
                    serde_json::from_value(frame.clone()).expect("replaced frame"),
                )),
                Some("sessionChatAppended") => ChatFrame::Appended(Box::new(
                    serde_json::from_value(frame.clone()).expect("appended frame"),
                )),
                Some("sessionChatState") => ChatFrame::State(Box::new(
                    serde_json::from_value(frame.clone()).expect("state frame"),
                )),
                other => panic!("a follower frame of type {other:?}"),
            };
            self.drive(ghostex_gx_chat_core::Event::Frame(Box::new(parsed)));
        }
        self.folded = frames.len();
    }

    fn act(&mut self, action: Value) -> Vec<ghostex_gx_chat_core::Effect> {
        let action: ghostex_gx_chat_core::UserAction =
            serde_json::from_value(action).expect("the action parses");
        self.drive(ghostex_gx_chat_core::Event::Action(Box::new(action)))
    }

    /// Closes the approval card the way a person can: the pick they made is refused (OMP's box
    /// changed under it), and then the card offers to close.
    fn close_after_a_refused_pick(&mut self) {
        let effects = self.act(json!({
            "type": "answer",
            "answer": { "kind": "terminalChoice", "choiceIndex": 1 },
        }));
        let request_id = effects
            .iter()
            .find_map(|effect| match effect {
                ghostex_gx_chat_core::Effect::SendRpc {
                    request_id, method, ..
                } if *method == ghostex_gx_chat_core::ChatRpcMethod::AnswerSessionChatPrompt => {
                    Some(*request_id)
                }
                _ => None,
            })
            .expect("the pick goes to gxserver");
        self.drive(ghostex_gx_chat_core::Event::RpcSettled {
            request_id,
            outcome: Box::new(ghostex_gx_chat_core::RpcOutcome::Err {
                code: Some("invalidState".into()),
                message: "OMP's approval changed. Review the current choices and try again.".into(),
                endpoint: Some("/api/answerSessionChatPrompt".into()),
            }),
        });
        self.act(json!({ "type": "dismissNotice" }));
    }

    fn card_shown(&self) -> bool {
        serde_json::to_value(self.core.document()).expect("the document is JSON")["noticeVisible"]
            == true
    }
}

/// Waits up to ten seconds for `done`.
async fn until(what: &str, done: impl Fn() -> bool) {
    for _ in 0..400 {
        if done() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

/// Two readers of one screen. The chat's follower publishes the first `commit` approval and the
/// person closes its card. OMP takes the answer from the terminal; another read of the screen (the
/// one after an answer) sees the box gone, and OMP asks for the second `commit` before the
/// follower reads again. The follower reads the two same-looking requests and never the clear
/// screen between them; the chat still shows the second request's card.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_next_approval_shows_its_card_when_another_read_saw_the_first_answered() {
    use std::sync::atomic::AtomicUsize;

    let fixture = Fixture::new();
    let daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    let transcript = fixture._temp.path().join("omp-session.jsonl");
    std::fs::write(&transcript, "").expect("the session's transcript");
    // The follower's reads wait while the test holds the screen, so it never sees the clear one.
    let turn = Arc::new(Mutex::new(()));
    let refreshes = Arc::new(AtomicUsize::new(0));
    // The reader the follower registry gives every follower (session_chat_follower/registry.rs),
    // behind the turn.
    let options_reader: crate::session_chat_options::SessionChatOptionsReader = {
        let detector = crate::session_chat_options::SessionChatOptionDetector::new(&fixture.state);
        let project_id = fixture.project_id().to_owned();
        let session_id = fixture.session_id.clone();
        let (turn, refreshes) = (turn.clone(), refreshes.clone());
        Arc::new(move |mode| {
            let _turn = turn.lock().expect("turn");
            let mut detection = match mode {
                crate::session_chat_options::SessionChatOptionsReadMode::Cached => {
                    detector.cached(&project_id, &session_id)
                }
                crate::session_chat_options::SessionChatOptionsReadMode::Refresh => {
                    refreshes.fetch_add(1, Ordering::SeqCst);
                    detector.detect_blocking(&project_id, &session_id, Some("omp"), true)
                }
            };
            detection.notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
                &project_id,
                &session_id,
                detection.notice,
            );
            detection
        })
    };
    let state_reader: crate::session_chat::SessionChatStateReader = {
        let transcript = transcript.to_string_lossy().into_owned();
        Arc::new(move || crate::session_chat::SessionChatLiveState {
            prompt: None,
            // The eval call's hooks said working, and no hook announced the approval.
            working: true,
            agent_session_id: Some("omp-session".into()),
            agent_session_path: Some(transcript.clone()),
        })
    };
    let frames: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let emit: crate::session_chat::SessionChatFrameEmitter = {
        let frames = frames.clone();
        Arc::new(move |frame| frames.lock().expect("frames").push(frame))
    };
    let config = crate::session_chat::SessionChatFollowerConfig {
        project_id: fixture.project_id().to_owned(),
        session_id: fixture.session_id.clone(),
        agent: Some("omp".into()),
        agent_session_id: Some("omp-session".into()),
        agent_session_path: Some(transcript.to_string_lossy().into_owned()),
        limit: 50,
        protocol_version: 1,
        server_id: fixture.state.metadata.server_id.clone(),
        state_reader: Some(state_reader),
        options_reader: Some(options_reader),
        options_change_watch: None,
        screen_change_watch: None,
        queue_reader: None,
        successor_hooks: None,
        notice_publisher: None,
        tuning: crate::session_chat::SessionChatFollowerTuning {
            reconcile_interval: std::time::Duration::from_millis(20),
            stale_transcript_idle: std::time::Duration::from_secs(600),
            successor_scan_interval: std::time::Duration::from_secs(600),
            successor_stale_substantive_idle_ms: 600_000,
        },
    };
    let follower = tokio::spawn(crate::session_chat::run_session_chat_follower(
        config,
        Arc::new(crate::session_chat::SessionChatStream::new()),
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(crate::session_chat::SessionChatFollowerHeartbeat::new()),
        emit,
    ));

    let published_key = |frames: &Mutex<Vec<Value>>| {
        frames
            .lock()
            .expect("frames")
            .iter()
            .rev()
            .find_map(|frame| {
                frame["terminalNotice"]["detectedAt"]
                    .as_str()
                    .map(str::to_owned)
            })
    };
    until("the follower to publish the first approval", || {
        published_key(&frames).is_some()
    })
    .await;
    let first = published_key(&frames).expect("the first approval's key");
    let mut chat = ChatClient::open(&fixture);
    chat.fold(&frames);
    assert!(chat.card_shown(), "the first approval shows its card");
    chat.close_after_a_refused_pick();
    assert!(!chat.card_shown(), "the person closed it");

    {
        let _turn = turn.lock().expect("turn");
        daemon.show(&answered_screen());
        fixture.read_screen();
        assert!(
            screen_notice(&fixture).is_none(),
            "the other read saw the first approval answered"
        );
        // Detection stamps are milliseconds; the second request comes later.
        std::thread::sleep(std::time::Duration::from_millis(5));
        daemon.show(APPROVAL_SCREEN);
    }
    let before = refreshes.load(Ordering::SeqCst);
    until("the follower to read the screen again", || {
        refreshes.load(Ordering::SeqCst) >= before + 3
    })
    .await;
    let second = screen_notice(&fixture).expect("the second commit approval");
    assert_ne!(
        second.detected_at, first,
        "the second request has its own key"
    );
    chat.fold(&frames);
    follower.abort();
    assert!(chat.card_shown(), "the second request shows its card");
    assert_eq!(
        published_key(&frames).as_deref(),
        Some(second.detected_at.as_str()),
        "under the second request's own key"
    );
}

/// The bell keeps its meaning: a screen-only approval gives the red dot, but does not turn the
/// session's newest row (a finished turn already read) into an unread approval.
#[test]
fn a_screen_only_approval_does_not_relabel_a_read_row() {
    use crate::notification_feed::store::*;

    let fixture = Fixture::new();
    let _daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    let db = open_gxserver_database(&fixture.state.paths).expect("db");
    let finished = insert_notification_feed_row(
        &db,
        NewNotificationFeedRow {
            project_id: fixture.project_id().to_owned(),
            session_id: fixture.session_id.clone(),
            kind: NOTIFICATION_FEED_KIND_FINISHED,
            title: "OMP".into(),
            subtitle: "Approvals".into(),
            body: "Finished the previous turn".into(),
            agent_name: Some("omp".into()),
            attention_event_id: None,
        },
    )
    .expect("prior completion");
    mark_notification_read(&db, finished["id"].as_str().expect("id")).expect("mark it read");
    let before = crate::notification_feed::read_notification_feed_endpoint(&db).expect("before");
    assert_eq!(before["unreadCount"], json!(0));
    fixture.read_screen();
    assert_eq!(fixture.awaiting_approval(), Some(json!(true)));
    let after = crate::notification_feed::read_notification_feed_endpoint(&db).expect("after");
    assert_eq!(after, before, "the bell is as it was");
}

fn stored_mark(fixture: &Fixture) -> Option<String> {
    fixture.stored()["runtimeSettings"]
        .get("sessionChatScreenApprovalDetectedAt")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The detector's latest verdict on whether the screen shows an approval.
fn latest_verdict(fixture: &Fixture) -> Option<bool> {
    fixture
        .state
        .session_chat_option_cache
        .lock()
        .expect("cache")
        .get(&crate::server::session_observer_key(
            fixture.project_id(),
            &fixture.session_id,
        ))
        .and_then(|entry| entry.projected_screen_approval)
}

/// Two readers of one session finish out of order: the one that saw approval A answered decides
/// first and writes last, after another reader saw the next approval B (same words, its own
/// `detectedAt`). The mark follows the latest reading, so the session still waits for the user.
#[test]
fn a_clear_written_after_a_newer_approval_keeps_the_mark() {
    let fixture = Fixture::new();
    let daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    fixture.read_screen();
    let first = screen_notice(&fixture).expect("approval A");
    assert!(stored_mark(&fixture).is_some(), "approval A is marked");

    // The clear reader stops between its verdict and its mark write: it first clears a fleet mark,
    // and that write's presentation delta waits for the event sequence this test holds.
    fixture
        .state
        .session_chat_option_cache
        .lock()
        .expect("cache")
        .get_mut(&crate::server::session_observer_key(
            fixture.project_id(),
            &fixture.session_id,
        ))
        .expect("the reading of approval A")
        .projected_fleet = Some(true);
    open_gxserver_database(&fixture.state.paths)
        .expect("db")
        .execute(
            r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatFleetDetectedAt', '2026-11-23T10:00:00.000Z')
            WHERE projectId = ?1 AND sessionId = ?2"#,
            rusqlite::params![fixture.project_id(), fixture.session_id],
        )
        .expect("a fleet mark");
    daemon.show(&answered_screen());
    let held = fixture
        .state
        .presentation_event_sequence
        .lock()
        .expect("event sequence");
    let clear_reader =
        {
            let state = fixture.state.clone();
            let (project_id, session_id) =
                (fixture.project_id().to_string(), fixture.session_id.clone());
            std::thread::spawn(move || {
                crate::session_chat_options::SessionChatOptionDetector::new(&state)
                    .detect_blocking(&project_id, &session_id, Some("omp"), true);
            })
        };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while latest_verdict(&fixture) != Some(false) {
        assert!(
            std::time::Instant::now() < deadline,
            "the clear reader decides"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Detection stamps are milliseconds; approval B comes later.
    std::thread::sleep(std::time::Duration::from_millis(5));
    daemon.show(APPROVAL_SCREEN);
    fixture.read_screen();
    let second = screen_notice(&fixture).expect("approval B");
    assert_ne!(second.detected_at, first.detected_at);
    assert_eq!(latest_verdict(&fixture), Some(true));

    // Now the clear reader writes.
    drop(held);
    clear_reader.join().expect("the clear reader");
    assert!(stored_mark(&fixture).is_some(), "approval B is marked");
    assert_eq!(fixture.awaiting_approval(), Some(json!(true)));
}

/// A mark write the database refused is made by the next reading, though that reading decides
/// nothing new.
#[test]
fn a_refused_mark_write_is_made_by_the_next_reading() {
    let fixture = Fixture::new();
    let _daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    let db = open_gxserver_database(&fixture.state.paths).expect("db");
    db.execute_batch(
        r#"CREATE TRIGGER refuse_screen_approval_mark BEFORE UPDATE ON sessions
        WHEN json_type(NEW.runtimeSettingsJson, '$.sessionChatScreenApprovalDetectedAt') IS NOT NULL
        BEGIN SELECT RAISE(ABORT, 'the database refused the write'); END;"#,
    )
    .expect("a refusing database");
    fixture.read_screen();
    assert_eq!(latest_verdict(&fixture), Some(true));
    assert_eq!(fixture.awaiting_approval(), None, "the write was refused");

    db.execute_batch("DROP TRIGGER refuse_screen_approval_mark;")
        .expect("the database takes writes again");
    fixture.read_screen();
    assert_eq!(fixture.awaiting_approval(), Some(json!(true)));
}

/// A reading is dropped (an agent restart, an account restart or sign-in) while another reader's
/// capture of the same session fails and commits its empty reading in between. The dropped
/// reading's mark goes with it, so the session does not wait for an approval nobody sees.
#[test]
fn a_failed_capture_while_a_reading_is_dropped_does_not_keep_the_mark() {
    let fixture = Fixture::new();
    let daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    fixture.read_screen();
    assert!(stored_mark(&fixture).is_some(), "the approval is marked");

    // Dropping the reading stops after removing the cache entry: its clears wait for the event
    // sequence this test holds, a fleet mark's clear at the latest.
    open_gxserver_database(&fixture.state.paths)
        .expect("db")
        .execute(
            r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatFleetDetectedAt', '2026-11-23T10:00:00.000Z')
            WHERE projectId = ?1 AND sessionId = ?2"#,
            rusqlite::params![fixture.project_id(), fixture.session_id],
        )
        .expect("a fleet mark");
    let reading_present = || {
        fixture
            .state
            .session_chat_option_cache
            .lock()
            .expect("cache")
            .contains_key(&crate::server::session_observer_key(
                fixture.project_id(),
                &fixture.session_id,
            ))
    };
    let wait_for = |what: &str, done: &dyn Fn() -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    };
    let held = fixture
        .state
        .presentation_event_sequence
        .lock()
        .expect("event sequence");
    let forget = {
        let state = fixture.state.clone();
        let (project_id, session_id) =
            (fixture.project_id().to_string(), fixture.session_id.clone());
        std::thread::spawn(move || {
            crate::session_chat_options::forget_session_chat_options(
                &state,
                &project_id,
                &session_id,
            );
        })
    };
    wait_for("the reading is dropped", &|| !reading_present());

    // The screen daemon is gone, so the other reader's capture fails.
    drop(daemon);
    let failed_reader =
        {
            let state = fixture.state.clone();
            let (project_id, session_id) =
                (fixture.project_id().to_string(), fixture.session_id.clone());
            std::thread::spawn(move || {
                crate::session_chat_options::SessionChatOptionDetector::new(&state)
                    .detect_blocking(&project_id, &session_id, Some("omp"), true);
            })
        };
    wait_for("the failed capture commits its reading", &reading_present);
    assert_eq!(latest_verdict(&fixture), None, "the capture saw nothing");

    drop(held);
    forget.join().expect("dropping the reading");
    failed_reader.join().expect("the failed reader");
    assert_eq!(
        stored_mark(&fixture),
        None,
        "the dropped reading's mark is gone"
    );
    assert_eq!(fixture.awaiting_approval(), None);
}

/// A reading is dropped while another reader's whole capture sees an approval and commits it after
/// the removal. That newer reading wins: the drop holds off mark writes from before it removes the
/// reading until its clear is written, so the capture's mark lands after the clear and the session
/// waits for the user.
#[test]
fn an_approval_read_while_a_reading_is_dropped_keeps_its_mark() {
    let fixture = Fixture::new();
    let daemon = ScreenDaemon::serve(&fixture.zmx_name, &answered_screen());
    fixture.read_screen();
    assert_eq!(latest_verdict(&fixture), Some(false), "a reading to drop");

    // Dropping the reading stops at its fleet mark's clear: the clear is written, and its
    // presentation delta waits for the event sequence this test holds.
    open_gxserver_database(&fixture.state.paths)
        .expect("db")
        .execute(
            r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatFleetDetectedAt', '2026-11-23T10:00:00.000Z')
            WHERE projectId = ?1 AND sessionId = ?2"#,
            rusqlite::params![fixture.project_id(), fixture.session_id],
        )
        .expect("a fleet mark");
    let reading_present = || {
        fixture
            .state
            .session_chat_option_cache
            .lock()
            .expect("cache")
            .contains_key(&crate::server::session_observer_key(
                fixture.project_id(),
                &fixture.session_id,
            ))
    };
    let wait_for = |what: &str, done: &dyn Fn() -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    };
    let held = fixture
        .state
        .presentation_event_sequence
        .lock()
        .expect("event sequence");

    // At the removal: the drop waits for the cache this test holds, and mark writes are already
    // held off.
    let cache = fixture
        .state
        .session_chat_option_cache
        .lock()
        .expect("cache");
    let forget = {
        let state = fixture.state.clone();
        let (project_id, session_id) =
            (fixture.project_id().to_string(), fixture.session_id.clone());
        std::thread::spawn(move || {
            crate::session_chat_options::forget_session_chat_options(
                &state,
                &project_id,
                &session_id,
            );
        })
    };
    wait_for(
        "the drop holds off mark writes before it removes the reading",
        &crate::session_chat_compacting::screen_approval_marker_writes_held,
    );
    drop(cache);
    wait_for("the reading is dropped", &|| !reading_present());

    // After the removal the approval is on screen, so the other reader's whole capture sees it and
    // goes to write its mark while the drop is stopped at its fleet clear.
    daemon.show(APPROVAL_SCREEN);
    let approval_reader =
        {
            let state = fixture.state.clone();
            let (project_id, session_id) =
                (fixture.project_id().to_string(), fixture.session_id.clone());
            std::thread::spawn(move || {
                crate::session_chat_options::SessionChatOptionDetector::new(&state)
                    .detect_blocking(&project_id, &session_id, Some("omp"), true);
            })
        };
    wait_for("the capture commits the approval", &|| {
        latest_verdict(&fixture) == Some(true)
    });
    assert!(
        crate::session_chat_compacting::screen_approval_marker_writes_held(),
        "the drop still holds off mark writes"
    );
    assert_eq!(
        stored_mark(&fixture),
        None,
        "the capture's mark waits for the drop's clear"
    );

    drop(held);
    forget.join().expect("dropping the reading");
    approval_reader.join().expect("the approval reader");
    assert!(
        stored_mark(&fixture).is_some(),
        "the newer reading's approval is marked"
    );
    assert_eq!(fixture.awaiting_approval(), Some(json!(true)));
}

/// Makes the database refuse every clear of a stored screen-approval mark until the returned
/// connection drops the trigger.
fn refuse_screen_approval_clears(fixture: &Fixture) -> crate::storage::GxserverDatabase {
    let db = open_gxserver_database(&fixture.state.paths).expect("db");
    db.execute_batch(
        r#"CREATE TRIGGER refuse_screen_approval_clear BEFORE UPDATE ON sessions
        WHEN json_type(OLD.runtimeSettingsJson, '$.sessionChatScreenApprovalDetectedAt') IS NOT NULL
         AND json_type(NEW.runtimeSettingsJson, '$.sessionChatScreenApprovalDetectedAt') IS NULL
        BEGIN SELECT RAISE(ABORT, 'the database refused the clear'); END;"#,
    )
    .expect("a database refusing the clear");
    db
}

/// Drops the reading of a marked approval while the screen cannot be read (an agent switch or a
/// restart) and the database refuses the drop's clear, then lets the database take writes again.
fn refuse_the_clear_of_a_dropped_reading(fixture: &Fixture) {
    let daemon = ScreenDaemon::serve(&fixture.zmx_name, APPROVAL_SCREEN);
    fixture.read_screen();
    assert!(stored_mark(fixture).is_some(), "the approval is marked");
    drop(daemon);
    let db = refuse_screen_approval_clears(fixture);
    crate::session_chat_options::forget_session_chat_options(
        &fixture.state,
        fixture.project_id(),
        &fixture.session_id,
    );
    assert_eq!(latest_verdict(fixture), None, "the reading was dropped");
    assert_eq!(
        fixture.awaiting_approval(),
        Some(json!(true)),
        "the database refused the clear"
    );
    db.execute_batch("DROP TRIGGER refuse_screen_approval_clear;")
        .expect("the database takes writes again");
}

/// A drop's clear the database refused is made by the next capture of the session, though that
/// capture fails and leaves no verdict.
#[test]
fn a_refused_clear_of_a_dropped_reading_is_made_by_the_next_failed_capture() {
    let fixture = Fixture::new();
    refuse_the_clear_of_a_dropped_reading(&fixture);
    fixture.read_screen();
    assert_eq!(
        latest_verdict(&fixture),
        None,
        "the screen could not be read"
    );
    assert_eq!(stored_mark(&fixture), None, "the owed clear is made");
    assert_eq!(fixture.awaiting_approval(), None);
}

/// A drop's clear the database refused is made by the stall sweep's refresh of the session, with
/// no other reading in between.
#[test]
fn a_refused_clear_of_a_dropped_reading_is_made_by_the_sweep() {
    let fixture = Fixture::new();
    refuse_the_clear_of_a_dropped_reading(&fixture);
    crate::server::session_stall_runtime::tick(&fixture.state, &Mutex::new(HashMap::new()));
    assert_eq!(stored_mark(&fixture), None, "the owed clear is made");
    assert_eq!(fixture.awaiting_approval(), None);
}

/// A drop's clear the database refused is made by the stall sweep even when a failed capture's
/// own retry of it was refused too: the sweep's refresh gets that recent capture back from the
/// detector's cache without reading the screen, so only the sweep's own publication can clear.
#[test]
fn a_refused_clear_of_a_dropped_reading_is_made_by_the_sweep_from_a_cached_failed_capture() {
    let fixture = Fixture::new();
    refuse_the_clear_of_a_dropped_reading(&fixture);
    let db = refuse_screen_approval_clears(&fixture);
    fixture.read_screen();
    assert_eq!(
        latest_verdict(&fixture),
        None,
        "the screen could not be read"
    );
    assert!(
        stored_mark(&fixture).is_some(),
        "the database refused the capture's retry too"
    );
    db.execute_batch("DROP TRIGGER refuse_screen_approval_clear;")
        .expect("the database takes writes again");
    let key = crate::server::session_observer_key(fixture.project_id(), &fixture.session_id);
    let captured_at = || fixture.state.session_chat_option_cache.lock().unwrap()[&key].fetched_at;
    let failed_capture = captured_at();
    crate::server::session_stall_runtime::tick(&fixture.state, &Mutex::new(HashMap::new()));
    assert_eq!(
        captured_at(),
        failed_capture,
        "the sweep got the failed capture back from the cache"
    );
    assert_eq!(stored_mark(&fixture), None, "the owed clear is made");
    assert_eq!(fixture.awaiting_approval(), None);
}
