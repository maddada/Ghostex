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
