//! Focused calls end to end, without a paid provider: a real `Conversation` task runs against a
//! fake provider socket on the loopback, driven through its `Input` channel the way the client
//! socket and the announcement watcher drive it. What the fake provider receives is what the model
//! would get; what the client channel receives is what an app would draw.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::conversation::{Announcement, ClientFrame, Conversation, Input};
use super::focus::FocusScope;
use super::provider::Provider;
use super::sessions::{SessionInfo, ThreadParent, VoiceState};
use super::tools::{self, ToolMemory, ToolOutcome};
use super::{memory, prompt, registry};
use crate::server::AppState;

const WAIT: Duration = Duration::from_secs(5);

/// A gxserver state in a temporary home whose loopback port answers nothing, so the tools that
/// read a chat fail fast instead of reaching a real gxserver.
pub(super) fn test_state(temp: &tempfile::TempDir) -> Arc<AppState> {
    state_in(temp, false)
}

/// For the opt-in live checks (`control_tests.rs`): the tools read this computer's real sessions
/// and projects and call the running gxserver's routes; nothing else of the real install is set up
/// or written (storage and logs stay in `temp`).
pub(super) fn live_state(temp: &tempfile::TempDir) -> Arc<AppState> {
    state_in(temp, true)
}

fn state_in(temp: &tempfile::TempDir, live: bool) -> Arc<AppState> {
    let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
    let storage = crate::storage::initialize_gxserver_storage(&paths).expect("storage");
    let mut config = crate::config::create_default_gxserver_config().expect("config");
    config.listeners.local.port = if live {
        crate::ghostex_cli::rpc::local_gxserver_api_port()
    } else {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("free port");
        listener.local_addr().expect("port").port()
    };
    config.listeners.local.host = "127.0.0.1".to_string();
    let metadata = crate::protocol::RuntimeMetadata {
        build_identity: "test-build".to_string(),
        pid: std::process::id(),
        port: config.listeners.local.port,
        protocol_version: crate::constants::GXSERVER_PROTOCOL_VERSION,
        server_id: "S7k".to_string(),
        started_at: "2026-10-24T10:00:00.000Z".to_string(),
        version: "0.0.0-test".to_string(),
    };
    let api = format!("http://127.0.0.1:{}", config.listeners.local.port);
    let (shutdown_tx, _) = broadcast::channel(8);
    let event_hub = crate::events::GxserverEventHub::new(metadata.server_id.clone());
    let presentation_event_sequence = Arc::new(Mutex::new(()));
    let push = crate::push::PushRuntime::start_with(
        paths.clone(),
        Arc::new(crate::logging::GxserverLogger::new(paths.clone())),
        Arc::new(crate::push::runtime::SystemClock),
        crate::push::expo::ExpoClient::for_test_origin("http://127.0.0.1:9".to_string()),
    );
    Arc::new(AppState {
        accounts: Arc::new(crate::accounts::runtime::AccountRuntime::default()),
        auth_token: if live {
            crate::ghostex_cli::rpc::read_local_gxserver_auth_token().expect("gxserver token")
        } else {
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()
        },
        device_api_tokens: crate::auth::DeviceApiTokens::default(),
        cef_devtools: Default::default(),
        automation_runtime: crate::automations::AutomationRuntime::new(
            paths.clone(),
            metadata.server_id.clone(),
            api.clone(),
        ),
        delayed_send_runtime: crate::delayed_sends::DelayedSendRuntime::new(
            paths.clone(),
            metadata.server_id.clone(),
            event_hub.clone(),
            presentation_event_sequence.clone(),
        ),
        board_start_work_gate: Arc::new(Mutex::new(())),
        build_identity: "test-build".to_string(),
        config,
        event_hub,
        extension_registry: crate::extensions::ExtensionRegistry::new_with_api_url(&paths, api),
        logger: Arc::new(crate::logging::GxserverLogger::new(paths.clone())),
        metadata,
        migration: crate::storage::create_gxserver_migration_status(&storage),
        paths: if live {
            crate::paths::get_gxserver_paths(None)
        } else {
            paths
        },
        presentation_event_sequence,
        push: Arc::new(push),
        remote_pairing_runtime: crate::remote_access::RemotePairingRuntime::new(),
        repository_clone_jobs: crate::repository_clone::RepositoryCloneJobManager::default(),
        session_chat_followers: Arc::new(Mutex::new(HashMap::new())),
        session_chat_option_cache: Arc::new(Mutex::new(HashMap::new())),
        shutdown_tx,
        stale_activity_timers: Arc::new(Mutex::new(HashMap::new())),
        tailcat_runtime: crate::tailcat::TailcatRuntime::new(),
        version: "0.0.0-test".to_string(),
        zmx_title_observers: Arc::new(Mutex::new(HashMap::new())),
    })
}

fn session(project: &str, id: &str, title: &str) -> SessionInfo {
    SessionInfo {
        project_id: project.to_string(),
        session_id: id.to_string(),
        title: title.to_string(),
        project: "Ghostex".to_string(),
        agent: "Claude".to_string(),
        state: VoiceState::Finished,
        waiting: None,
        last_active_at: String::new(),
        turn_event: None,
        has_chat: true,
        thread_of: None,
        threads: None,
        computer: None,
    }
}

/// Main Coordinator with its thread Voice mode, and an unrelated Sidebar fix.
fn sessions() -> Vec<SessionInfo> {
    let mut main = session("P1", "Main", "Main Coordinator");
    main.threads = Some(1);
    let mut thread = session("P1", "T1", "Voice mode");
    thread.thread_of = Some(ThreadParent {
        reference: "P1:Main".to_string(),
        title: "Main Coordinator".to_string(),
        resolved: false,
    });
    vec![main, thread, session("P2", "Other", "Sidebar fix")]
}

fn scope(reference: &str) -> FocusScope {
    FocusScope::from_sessions(&sessions(), reference).expect("scope")
}

pub(super) struct Call {
    pub(super) id: String,
    pub(super) state: Arc<AppState>,
    pub(super) tx: mpsc::UnboundedSender<Input>,
    /// What gxserver sent the provider.
    provider: mpsc::UnboundedReceiver<Value>,
    /// Events the fake provider sends gxserver.
    pub(super) provider_events: mpsc::UnboundedSender<Value>,
    client: mpsc::UnboundedReceiver<ClientFrame>,
    _temp: tempfile::TempDir,
}

impl Call {
    /// Starts a conversation about `focus` (or all sessions) and attaches a client.
    pub(super) async fn start(focus: Option<FocusScope>, client_remote: bool) -> Self {
        let temp = tempfile::tempdir().expect("temp");
        let state = test_state(&temp);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let (to_test, provider) = mpsc::unbounded_channel::<Value>();
        let (provider_events, mut events) = mpsc::unbounded_channel::<Value>();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let socket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("handshake");
            let (mut sink, mut stream) = socket.split();
            loop {
                tokio::select! {
                    frame = stream.next() => match frame {
                        Some(Ok(WsMessage::Text(text))) => {
                            let _ = to_test.send(serde_json::from_str(text.as_str()).unwrap_or(Value::Null));
                        }
                        Some(Ok(_)) => {}
                        _ => break,
                    },
                    event = events.recv() => match event {
                        Some(event) => { let _ = sink.send(WsMessage::Text(event.to_string().into())).await; }
                        None => break,
                    },
                }
            }
        });
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .expect("connect");
        let provider_id = Provider::Xai;
        let update = provider_id.session_update(
            "eve",
            &prompt::instructions(focus.as_ref()),
            &tools::definitions(focus.as_ref()),
            &[],
            false,
        );
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel();
        registry::insert(&id, tx.clone(), provider_id);
        let conversation = Conversation::new(
            id.clone(),
            state.clone(),
            provider_id,
            "eve".to_string(),
            update,
            false,
            focus,
            client_remote,
            socket,
            tx.clone(),
            rx,
        );
        tokio::spawn(conversation.run());
        let (out, client) = mpsc::unbounded_channel();
        tx.send(Input::Attach {
            attach_id: 1,
            out,
            resumed: false,
        })
        .expect("attach");
        Self {
            id,
            state,
            tx,
            provider,
            provider_events,
            client,
            _temp: temp,
        }
    }

    /// The next JSON frame for the client of `kind`, skipping others.
    pub(super) async fn client_frame(&mut self, kind: &str) -> Value {
        tokio::time::timeout(WAIT, async {
            loop {
                match self.client.recv().await.expect("client channel") {
                    ClientFrame::Json(frame) if frame["type"] == kind => return frame,
                    _ => {}
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no {kind} frame for the client"))
    }

    /// Every client frame of `kind` that arrives within `window`.
    pub(super) async fn client_frames(&mut self, kind: &str, window: Duration) -> Vec<Value> {
        let mut frames = Vec::new();
        let _ = tokio::time::timeout(window, async {
            while let Some(frame) = self.client.recv().await {
                if let ClientFrame::Json(frame) = frame {
                    if frame["type"] == kind {
                        frames.push(frame);
                    }
                }
            }
        })
        .await;
        frames
    }

    /// The next event gxserver sent the provider that `matches`, skipping others.
    async fn provider_event(&mut self, what: &str, matches: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(WAIT, async {
            loop {
                let event = self.provider.recv().await.expect("provider channel");
                if matches(&event) {
                    return event;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the provider never got {what}"))
    }

    /// The next message item gxserver added to the conversation.
    pub(super) async fn model_message(&mut self, containing: &str) -> String {
        let event = self
            .provider_event(containing, |event| {
                event["type"] == "conversation.item.create"
                    && event["item"]["type"] == "message"
                    && event["item"]["content"][0]["text"]
                        .as_str()
                        .is_some_and(|text| text.contains(containing))
            })
            .await;
        event["item"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    async fn session_update(&mut self) -> Value {
        self.provider_event("a session.update", |event| {
            event["type"] == "session.update"
        })
        .await
    }

    pub(super) fn end(&self) {
        let _ = self.tx.send(Input::End);
    }
}

fn tool_names(update: &Value) -> HashSet<String> {
    update["session"]["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

fn announcement(reference: &str, kind: &'static str, text: &str) -> Announcement {
    let (project_id, session_id) = reference.split_once(':').unwrap();
    Announcement {
        text: text.to_string(),
        kind,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        server_id: None,
        model_text: None,
    }
}

#[tokio::test]
async fn a_focused_call_says_its_focus_offers_no_memory_and_briefs_the_model() {
    let mut call = Call::start(Some(scope("P1:T1")), false).await;
    let ready = call.client_frame("ready").await;
    assert_eq!(ready["focus"]["title"], "Voice mode");
    assert_eq!(ready["focus"]["orchestrator"], "Main Coordinator");
    assert_eq!(ready["focus"]["closed"], false);
    assert_eq!(ready["memory"], Value::Null);
    assert_eq!(ready["memoryContinued"], false);
    // The briefing goes in once the client attached, and the agent is asked to greet.
    let briefing = call.model_message("[Ghostex focus]").await;
    assert!(briefing.contains("This call is about one session: \"Voice mode\"."));
    call.provider_event("response.create", |event| {
        event["type"] == "response.create"
    })
    .await;
    call.end();
}

#[tokio::test]
async fn an_all_sessions_call_has_a_null_focus() {
    let mut call = Call::start(None, false).await;
    let ready = call.client_frame("ready").await;
    assert_eq!(ready["focus"], Value::Null);
    assert!(ready.get("memory").is_some());
    call.end();
}

#[tokio::test]
async fn the_focused_configuration_offers_only_its_tools() {
    let focused = tools::definitions(Some(&scope("P1:T1")));
    let names: HashSet<String> = focused
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect();
    let expected: HashSet<String> = [
        "read_session",
        "send_prompt",
        "draft_prompt",
        "send_draft",
        "get_pending_prompt",
        "answer_prompt",
        "stop_session",
        "show_session",
        "widen_call",
    ]
    .iter()
    .map(|name| name.to_string())
    .collect();
    assert_eq!(names, expected);
    for tool in focused.as_array().unwrap() {
        let required = tool["parameters"]["required"].as_array();
        assert!(
            !required.is_some_and(|keys| keys.iter().any(|key| key == "session")),
            "{} still requires a session",
            tool["name"]
        );
    }
    let all = tools::definitions(None);
    assert!(all
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "list_sessions"));
    assert!(!all
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "widen_call"));
    let instructions = prompt::instructions(Some(&scope("P1:T1")));
    assert!(instructions.contains("This call is about one session only: \"Voice mode\""));
    assert!(!instructions.contains("list_sessions"));
    assert!(prompt::instructions(None).contains("call list_sessions"));
}

#[tokio::test]
async fn other_sessions_are_refused_by_the_tools() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp);
    let memory = ToolMemory::new(false);
    memory.set_scope(Some(scope("P1:T1")));
    let refusal = "This call is about Voice mode only. Say \"all sessions\" or press All sessions to talk about the others.";
    for (name, args) in [
        ("read_session", json!({"session": "Sidebar fix"})),
        ("send_prompt", json!({"session": "P2:Other", "text": "Hi"})),
        ("list_sessions", json!({})),
        ("start_agent", json!({"project": "Ghostex"})),
    ] {
        let outcome = tools::run(&state, &memory, name, &args).await;
        assert!(!outcome.ok, "{name} was allowed");
        assert_eq!(outcome.output["error"], refusal, "{name}");
    }
    // A widen asked for in an all-sessions call changes nothing.
    let all = ToolMemory::new(false);
    let outcome = tools::run(&state, &all, "widen_call", &json!({})).await;
    assert_eq!(outcome.output["widened"], false);
}

#[tokio::test]
async fn only_the_focused_sessions_announcements_reach_the_call_and_a_finished_turn_is_retold() {
    let mut call = Call::start(Some(scope("P1:T1")), false).await;
    call.client_frame("ready").await;
    call.model_message("[Ghostex focus]").await;
    call.tx
        .send(Input::Announce(vec![
            announcement(
                "P2:Other",
                "turnFinished",
                "\"Sidebar fix\" finished its turn.",
            ),
            announcement(
                "P1:Main",
                "waitingOnQuestion",
                "\"Main Coordinator\" is waiting on a question.",
            ),
            announcement("P1:T1", "turnFinished", "\"Voice mode\" finished its turn."),
        ]))
        .unwrap();
    let frames = call
        .client_frames("announcement", Duration::from_millis(600))
        .await;
    let texts: Vec<&str> = frames
        .iter()
        .filter_map(|frame| frame["text"].as_str())
        .collect();
    assert_eq!(texts, vec!["\"Voice mode\" finished its turn."]);
    // The reply is read before the model hears it (here the chat can't be read, so it hears only
    // that the turn finished).
    let told = call.model_message("[Ghostex update]").await;
    assert!(told.contains("\"Voice mode\" finished its turn."));
    assert!(!told.contains("Sidebar fix"));
    call.end();
}

#[tokio::test]
async fn an_orchestrator_call_hears_its_threads_questions_but_not_their_finished_turns() {
    let mut call = Call::start(Some(scope("P1:Main")), false).await;
    call.client_frame("ready").await;
    call.tx
        .send(Input::Announce(vec![
            announcement("P1:T1", "turnFinished", "thread finished"),
            announcement("P1:T1", "waitingOnApproval", "thread waits for approval"),
            announcement("P2:Other", "waitingOnApproval", "other waits"),
        ]))
        .unwrap();
    let frames = call
        .client_frames("announcement", Duration::from_millis(600))
        .await;
    let texts: Vec<&str> = frames
        .iter()
        .filter_map(|frame| frame["text"].as_str())
        .collect();
    assert_eq!(texts, vec!["thread waits for approval"]);
    call.end();
}

#[tokio::test]
async fn a_closed_session_is_said_once_and_the_call_goes_on() {
    let mut call = Call::start(Some(scope("P1:T1")), false).await;
    call.client_frame("ready").await;
    let mut renamed = sessions();
    renamed[1].title = "Voice mode fixes".to_string();
    call.tx.send(Input::Sessions(Arc::new(renamed))).unwrap();
    let focus = call.client_frame("focus").await;
    assert_eq!(focus["focus"]["title"], "Voice mode fixes");
    let without: Vec<SessionInfo> = sessions()
        .into_iter()
        .filter(|session| session.session_id != "T1")
        .collect();
    call.tx
        .send(Input::Sessions(Arc::new(without.clone())))
        .unwrap();
    let focus = call.client_frame("focus").await;
    assert_eq!(focus["focus"]["closed"], true);
    let closed = call.client_frame("announcement").await;
    assert_eq!(closed["kind"], "sessionClosed");
    let told = call.model_message("was closed").await;
    assert!(told.contains("\"Voice mode fixes\" was closed."));
    // Still closed on the next poll: nothing new.
    call.tx.send(Input::Sessions(Arc::new(without))).unwrap();
    assert!(call
        .client_frames("focus", Duration::from_millis(400))
        .await
        .is_empty());
    // The call is still there.
    assert!(registry::sender(&call.id).is_some());
    call.end();
}

#[tokio::test]
async fn the_client_widens_and_moves_the_same_call() {
    let mut call = Call::start(Some(scope("P1:T1")), false).await;
    call.client_frame("ready").await;
    call.model_message("[Ghostex focus]").await;

    // All sessions: the full configuration comes back on the same provider session.
    call.tx.send(Input::Focus(None)).unwrap();
    let update = call.session_update().await;
    assert!(tool_names(&update).contains("list_sessions"));
    assert!(!tool_names(&update).contains("widen_call"));
    assert!(update["session"]["instructions"]
        .as_str()
        .unwrap()
        .contains("call list_sessions"));
    assert_eq!(update["session"]["voice"], "eve");
    assert_eq!(call.client_frame("focus").await["focus"], Value::Null);
    let widened = call.model_message("[Ghostex focus]").await;
    assert!(widened.contains("all of the user's sessions now instead of only \"Voice mode\""));

    // Moving to a session (looked up and briefed in the background).
    let main = scope("P1:Main");
    call.tx
        .send(Input::FocusResolved(Ok((
            main.clone(),
            super::focus::briefing_message(&main, &json!({"state": "idle"}), true),
        ))))
        .unwrap();
    let update = call.session_update().await;
    assert!(tool_names(&update).contains("widen_call"));
    assert!(!tool_names(&update).contains("list_sessions"));
    assert!(update["session"]["instructions"]
        .as_str()
        .unwrap()
        .contains("\"Main Coordinator\""));
    assert_eq!(
        call.client_frame("focus").await["focus"]["title"],
        "Main Coordinator"
    );
    assert!(call
        .model_message("moved this call")
        .await
        .contains("\"Main Coordinator\""));

    // Asking for the scope it already has only answers it.
    call.tx
        .send(Input::Focus(Some("P1:Main".to_string())))
        .unwrap();
    assert_eq!(
        call.client_frame("focus").await["focus"]["title"],
        "Main Coordinator"
    );

    // A session that is not open: refused, the scope stays.
    call.tx
        .send(Input::Focus(Some("P9:Gone".to_string())))
        .unwrap();
    let error = call.client_frame("error").await;
    assert_eq!(error["code"], "focusNotFound");
    assert_eq!(error["fatal"], false);
    assert_eq!(
        registry::summary()
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["conversationId"] == call.id.as_str())
            .unwrap()["focus"]["title"],
        "Main Coordinator"
    );
    call.end();
}

#[tokio::test]
async fn the_agent_widens_the_call_by_voice() {
    let mut call = Call::start(Some(scope("P1:T1")), false).await;
    call.client_frame("ready").await;
    call.model_message("[Ghostex focus]").await;
    call.provider_events
        .send(json!({"type": "response.function_call_arguments.done", "call_id": "c1", "name": "widen_call", "arguments": "{}"}))
        .unwrap();
    call.provider_events
        .send(json!({"type": "response.done", "response": {"status": "completed", "output": []}}))
        .unwrap();
    let update = call.session_update().await;
    assert!(tool_names(&update).contains("list_sessions"));
    assert_eq!(call.client_frame("focus").await["focus"], Value::Null);
    call.provider_event("the tool's output", |event| {
        event["item"]["type"] == "function_call_output" && event["item"]["call_id"] == "c1"
    })
    .await;
    call.end();
}

#[tokio::test]
async fn a_reattaching_client_gets_the_current_focus() {
    let mut call = Call::start(Some(scope("P1:T1")), false).await;
    call.client_frame("ready").await;
    let (out, mut second) = mpsc::unbounded_channel();
    call.tx
        .send(Input::Attach {
            attach_id: 2,
            out,
            resumed: true,
        })
        .unwrap();
    let ready = tokio::time::timeout(WAIT, async {
        loop {
            if let Some(ClientFrame::Json(frame)) = second.recv().await {
                if frame["type"] == "ready" {
                    return frame;
                }
            }
        }
    })
    .await
    .expect("ready on reattach");
    assert_eq!(ready["resumed"], true);
    assert_eq!(ready["focus"]["title"], "Voice mode");
    call.end();
}

#[tokio::test]
async fn nothing_said_about_one_session_is_remembered_until_the_call_widens() {
    let mut call = Call::start(Some(scope("P1:T1")), false).await;
    call.client_frame("ready").await;
    call.tx
        .send(Input::Text("What is it doing?".to_string()))
        .unwrap();
    call.client_frame("caption").await;
    let pending = memory::pending_path(&call.state.paths, &call.id);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!pending.exists(), "a focused call recorded the user");
    call.tx.send(Input::ContinueMemory).unwrap();
    assert_eq!(call.client_frame("memory").await["state"], "none");

    call.tx.send(Input::Focus(None)).unwrap();
    call.client_frame("focus").await;
    call.tx
        .send(Input::Text("And the others?".to_string()))
        .unwrap();
    call.client_frame("caption").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(pending.exists(), "the widened call did not record the user");
    let saved = std::fs::read_to_string(&pending).unwrap();
    assert!(saved.contains("And the others?"));
    assert!(!saved.contains("What is it doing?"));
    call.end();
}

#[tokio::test]
async fn a_client_on_another_computer_is_told_to_show_sessions_itself() {
    let mut call = Call::start(None, true).await;
    call.client_frame("ready").await;
    let shown = json!({"ok": true, "shown": true, "session": {"ref": "P2:Other", "title": "Sidebar fix", "project": "Ghostex"}});
    call.tx
        .send(Input::ToolDone {
            call_id: "c1".to_string(),
            name: "show_session".to_string(),
            label: "Showing Sidebar fix".to_string(),
            outcome: ToolOutcome {
                output: shown.clone(),
                ok: true,
            },
        })
        .unwrap();
    let tool = call.client_frame("tool").await;
    assert_eq!(tool["session"]["sessionId"], "Other");
    assert_eq!(tool["show"], "now");
    call.tx
        .send(Input::ToolDone {
            call_id: "c2".to_string(),
            name: "read_session".to_string(),
            label: "Reading Sidebar fix".to_string(),
            outcome: ToolOutcome {
                output: shown.clone(),
                ok: true,
            },
        })
        .unwrap();
    assert_eq!(call.client_frame("tool").await["show"], "follow");
    call.end();

    // The computer's own app shows sessions itself, so its client gets no `show`.
    let mut local = Call::start(None, false).await;
    local.client_frame("ready").await;
    local
        .tx
        .send(Input::ToolDone {
            call_id: "c3".to_string(),
            name: "show_session".to_string(),
            label: "Showing Sidebar fix".to_string(),
            outcome: ToolOutcome {
                output: shown,
                ok: true,
            },
        })
        .unwrap();
    assert!(local.client_frame("tool").await.get("show").is_none());
    local.end();
}

#[tokio::test]
async fn a_remote_client_shows_the_session_and_no_follow_happens_here() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp);
    let memory = ToolMemory::new(true);
    let screen = tools::run(&state, &memory, "get_active_session", &json!({})).await;
    assert!(screen.ok);
    assert!(screen.output["desktop"]
        .as_str()
        .unwrap()
        .contains("another computer"));
}

#[test]
fn a_missing_key_on_another_computer_is_said_in_general_words() {
    use super::socket::{bad_key_message, no_key_message};
    assert_eq!(
        no_key_message(Provider::Xai, true),
        "The computer this session runs on has no xAI API key for the voice agent. Add it in Ghostex Settings > Voice on that computer."
    );
    assert_eq!(
        bad_key_message(Provider::OpenAi, true),
        "The OpenAI API key on the computer this session runs on was refused by OpenAI. Check it in Ghostex Settings > Voice on that computer."
    );
    assert_eq!(
        no_key_message(Provider::Xai, false),
        "Add your xAI API key in Settings > Voice."
    );
}

#[tokio::test]
async fn a_long_reply_comes_in_paragraphs_and_a_run_of_one_tool_is_one_row() {
    let mut call = Call::start(None, false).await;
    call.client_frame("ready").await;
    let reply = "One is still working: Cap CLI demo video cleanup, which is scanning a video for QR codes right now, so leave that one. Of the twenty sleeping, five have no messages at all, so those are safe to close, and the rest finished cleanly with nothing pending.";
    let (first, rest) = reply.split_at(60);
    for delta in [first, rest] {
        call.provider_events
            .send(json!({"type": "response.output_audio_transcript.delta", "item_id": "i1", "delta": delta}))
            .unwrap();
    }
    call.provider_events
        .send(json!({"type": "response.output_audio_transcript.done", "item_id": "i1", "transcript": reply}))
        .unwrap();
    let captions = call
        .client_frames("caption", Duration::from_millis(300))
        .await;
    let last = captions.last().expect("captions");
    assert_eq!(last["final"], true);
    assert_eq!(last["text"], reply, "the caption text stays one string");
    assert_eq!(last["paragraphs"].as_array().map(Vec::len), Some(2));
    assert!(
        captions[0].get("paragraphs").is_none(),
        "a short caption has none"
    );

    for id in ["c1", "c2"] {
        call.provider_events
            .send(json!({"type": "response.function_call_arguments.done", "call_id": id, "name": "list_projects", "arguments": "{}"}))
            .unwrap();
    }
    let tools = call.client_frames("tool", Duration::from_millis(500)).await;
    let started: Vec<&Value> = tools
        .iter()
        .filter(|frame| frame["phase"] == "started")
        .collect();
    assert!(started[0].get("run").is_none());
    assert_eq!(started[1]["run"]["id"], "c1");
    assert_eq!(
        started[1]["run"]["label"],
        "Checking your projects (2 times)"
    );
    call.end();
}
