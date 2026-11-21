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

/// An approval answered in a terminal nothing else reads: the stall tick's read takes the red dot
/// away, also when the detector had already seen the screen clear while the stored mark lost a race.
#[test]
fn the_tick_clears_an_approval_the_screen_no_longer_shows() {
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
