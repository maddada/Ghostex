use std::{
    collections::HashMap,
    env, fs, io,
    io::{BufRead as _, BufReader, Write as _},
    path::{Component, Path, PathBuf},
    process,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose};
use interprocess::{
    TryClone as _,
    local_socket::{GenericFilePath, Listener, ListenerOptions, Name, Stream, prelude::*},
};
use percent_encoding::percent_decode_str;
use serde_json::{Value, json};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Window, WindowBuilder, WindowId},
};
use tempfile::NamedTempFile;
use uuid::Uuid;
use wry::{
    WebView, WebViewBuilder,
    http::{Request, Response, header::CONTENT_TYPE},
};

const PROTOCOL_VERSION: u64 = 1;
const SOCKET_FILE_NAME: &str = "ghostex-editor.sock";
const DEFAULT_TITLE: &str = "Prompt Editor";
// Linux/Windows have no native window tabs, so the app name and the session
// title (macOS: titlebar + tab title) share the one window title here.
const APP_WINDOW_TITLE: &str = "Ghostex Prompt Editor";
const DEFAULT_LANGUAGE: &str = "markdown";
const CUSTOM_PROTOCOL: &str = "ghostex-editor";

#[derive(Clone)]
struct ClientConnection {
    writer: Arc<Mutex<Stream>>,
}

impl ClientConnection {
    fn send(&self, value: Value) {
        let _ = self.send_checked(value);
    }

    fn send_checked(&self, value: Value) -> bool {
        let Ok(mut line) = serde_json::to_vec(&value) else {
            return false;
        };
        line.push(b'\n');
        let Ok(mut writer) = self.writer.lock() else {
            return false;
        };
        writer.write_all(&line).is_ok() && writer.flush().is_ok()
    }

    fn send_error(&self, message: impl Into<String>) {
        self.send(json!({
            "type": "error",
            "v": PROTOCOL_VERSION,
            "message": message.into(),
        }));
    }
}

enum DaemonEvent {
    Request {
        request: Value,
        connection: ClientConnection,
    },
    WebMessage {
        window_id: WindowId,
        body: String,
    },
    SaveAllAndExit,
    EnsureWarm,
}

struct SocketEndpoint {
    display_path: String,
    name: Name<'static>,
    cleanup_path: Option<PathBuf>,
}

struct EditorApp {
    socket_cleanup_path: Option<PathBuf>,
    web_root: Arc<PathBuf>,
    proxy: EventLoopProxy<DaemonEvent>,
    windows: HashMap<WindowId, EditorWindow>,
    sessions: HashMap<String, EditorSession>,
    warm_window: Option<WindowId>,
    warm_waiters: Vec<ClientConnection>,
    open_count_watchers: Vec<ClientConnection>,
    pending_shutdown: bool,
    should_exit: bool,
    cascade_offset: i32,
    last_cursor_snapshot: Option<CursorSnapshot>,
}

struct EditorWindow {
    window: Window,
    webview: WebView,
    is_ready: bool,
    session_request_id: Option<String>,
}

struct EditorSession {
    request_id: String,
    originating_session_id: Option<String>,
    file_path: PathBuf,
    status_file: PathBuf,
    language: Option<String>,
    title: String,
    initial_text: String,
    initial_cursor_offset: Option<usize>,
    latest_draft: String,
    latest_cursor_offset: Option<usize>,
    opener: ClientConnection,
    window_id: WindowId,
    has_opened: bool,
    is_finishing: bool,
}

struct CursorSnapshot {
    text: String,
    cursor_offset: usize,
}

#[derive(Clone, Copy)]
enum CloseAction {
    Save,
    Cancel,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("ghostex-editor: {error}");
        process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let socket_arg = parse_daemon_args(&args)?;
    let endpoint = resolve_socket_endpoint(socket_arg.as_deref())?;

    if ping_existing_daemon(&endpoint.name) {
        return Ok(());
    }
    remove_stale_socket(&endpoint.cleanup_path);

    let listener = ListenerOptions::new()
        .name(endpoint.name.clone())
        .create_sync()
        .map_err(|error| {
            format!(
                "unable to bind editor daemon socket {}: {error}",
                endpoint.display_path
            )
        })?;

    let web_root = Arc::new(resolve_web_root()?);
    if !web_root.join("index.html").is_file() {
        return Err(format!(
            "missing editor web entry at {}",
            web_root.join("index.html").display()
        ));
    }

    let mut event_loop = EventLoopBuilder::<DaemonEvent>::with_user_event().build();
    set_process_window_policy(&mut event_loop);
    let proxy = event_loop.create_proxy();
    install_signal_handler(proxy.clone());
    spawn_accept_thread(listener, proxy.clone());

    let mut app = EditorApp {
        socket_cleanup_path: endpoint.cleanup_path,
        web_root,
        proxy,
        windows: HashMap::new(),
        sessions: HashMap::new(),
        warm_window: None,
        warm_waiters: Vec::new(),
        open_count_watchers: Vec::new(),
        pending_shutdown: false,
        should_exit: false,
        cascade_offset: 0,
        last_cursor_snapshot: None,
    };

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(_) => app.ensure_warm_window(target),
            Event::UserEvent(event) => app.handle_daemon_event(event, target),
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => app.handle_window_close(window_id),
            Event::LoopDestroyed => app.cleanup_socket(),
            _ => {}
        }
        if app.should_exit {
            app.cleanup_socket();
            *control_flow = ControlFlow::Exit;
        }
    });
}

fn parse_daemon_args(args: &[String]) -> Result<Option<String>, String> {
    if args.len() == 2 && args[1] == "--daemon" {
        return Ok(None);
    }
    if args.len() == 4 && args[1] == "--daemon" && args[2] == "--socket" {
        return Ok(Some(args[3].clone()));
    }
    Err("usage: ghostex-editor --daemon [--socket <path>]".to_string())
}

fn resolve_socket_endpoint(socket_arg: Option<&str>) -> Result<SocketEndpoint, String> {
    let display_path = env::var("GHOSTEX_EDITOR_SOCKET")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| socket_arg.map(str::to_string))
        .unwrap_or_else(default_socket_path);

    #[cfg(unix)]
    {
        let path = PathBuf::from(&display_path);
        if !path.is_absolute() {
            return Err(format!("socket path must be absolute: {display_path}"));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "unable to create socket directory {}: {error}",
                    parent.display()
                )
            })?;
            set_directory_private(parent);
        }
    }

    let name = socket_name(&display_path)
        .map_err(|error| format!("invalid editor daemon socket path {display_path}: {error}"))?;
    let cleanup_path = unix_cleanup_path(&display_path);
    Ok(SocketEndpoint {
        display_path,
        name,
        cleanup_path,
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn default_socket_path() -> String {
    if let Some(ghostex_home) = absolute_environment_path("GHOSTEX_HOME") {
        return ghostex_home
            .join("runtime")
            .join(SOCKET_FILE_NAME)
            .to_string_lossy()
            .into_owned();
    }
    if let Some(runtime_dir) = absolute_environment_path("XDG_RUNTIME_DIR") {
        return runtime_dir
            .join("ghostex")
            .join(SOCKET_FILE_NAME)
            .to_string_lossy()
            .into_owned();
    }
    resolved_state_directory()
        .join("runtime")
        .join(SOCKET_FILE_NAME)
        .to_string_lossy()
        .into_owned()
}

#[cfg(target_os = "windows")]
fn default_socket_path() -> String {
    let user = env::var("USERNAME")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "user".to_string());
    let sanitized: String = user
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    format!(r"\\.\pipe\ghostex-editor-{sanitized}")
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn default_socket_path() -> String {
    "/tmp/ghostex-editor.sock".to_string()
}

fn socket_name(path: &str) -> io::Result<Name<'static>> {
    path.to_fs_name::<GenericFilePath>().map(Name::into_owned)
}

#[cfg(unix)]
fn unix_cleanup_path(path: &str) -> Option<PathBuf> {
    Some(PathBuf::from(path))
}

#[cfg(not(unix))]
fn unix_cleanup_path(_path: &str) -> Option<PathBuf> {
    None
}

#[cfg(unix)]
fn set_directory_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn set_directory_private(_path: &Path) {}

fn ping_existing_daemon(name: &Name<'static>) -> bool {
    let Ok(mut stream) = Stream::connect(name.clone()) else {
        return false;
    };
    let _ = stream.set_recv_timeout(Some(Duration::from_millis(750)));
    let _ = stream.set_send_timeout(Some(Duration::from_millis(750)));
    if stream.write_all(b"{\"v\":1,\"type\":\"ping\"}\n").is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return false;
    }
    serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .as_deref()
        == Some("pong")
}

#[cfg(unix)]
fn remove_stale_socket(path: &Option<PathBuf>) {
    use std::os::unix::fs::FileTypeExt as _;
    let Some(path) = path else {
        return;
    };
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_socket() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(not(unix))]
fn remove_stale_socket(_path: &Option<PathBuf>) {}

fn resolve_web_root() -> Result<PathBuf, String> {
    if let Ok(value) = env::var("GHOSTEX_EDITOR_WEB_ROOT") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    let executable = env::current_exe()
        .map_err(|error| format!("unable to resolve current executable: {error}"))?;
    let executable_dir = executable
        .parent()
        .ok_or_else(|| "unable to resolve executable directory".to_string())?;
    Ok(executable_dir.join("web"))
}

fn spawn_accept_thread(listener: Listener, proxy: EventLoopProxy<DaemonEvent>) {
    thread::spawn(move || {
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => spawn_connection_thread(stream, proxy.clone()),
                Err(error) => eprintln!("ghostex-editor: accept failed: {error}"),
            }
        }
    });
}

fn spawn_connection_thread(stream: Stream, proxy: EventLoopProxy<DaemonEvent>) {
    thread::spawn(move || {
        let writer_stream = match stream.try_clone() {
            Ok(writer) => writer,
            Err(error) => {
                eprintln!("ghostex-editor: unable to clone client stream: {error}");
                return;
            }
        };
        let connection = ClientConnection {
            writer: Arc::new(Mutex::new(writer_stream)),
        };
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => return,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if trimmed.is_empty() {
                        connection.send_error("malformed JSON");
                        continue;
                    }
                    match serde_json::from_str::<Value>(trimmed) {
                        Ok(value) if value.is_object() => {
                            let _ = proxy.send_event(DaemonEvent::Request {
                                request: value,
                                connection: connection.clone(),
                            });
                        }
                        Ok(_) => connection.send_error("request must be a JSON object"),
                        Err(_) => connection.send_error("malformed JSON"),
                    }
                }
                Err(_) => return,
            }
        }
    });
}

fn install_signal_handler(proxy: EventLoopProxy<DaemonEvent>) {
    let _ = ctrlc::set_handler(move || {
        let _ = proxy.send_event(DaemonEvent::SaveAllAndExit);
    });
}

#[cfg(target_os = "macos")]
fn set_process_window_policy(event_loop: &mut tao::event_loop::EventLoop<DaemonEvent>) {
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
}

#[cfg(not(target_os = "macos"))]
fn set_process_window_policy(_event_loop: &mut tao::event_loop::EventLoop<DaemonEvent>) {}

impl EditorApp {
    fn handle_daemon_event(
        &mut self,
        event: DaemonEvent,
        target: &EventLoopWindowTarget<DaemonEvent>,
    ) {
        match event {
            DaemonEvent::Request {
                request,
                connection,
            } => self.handle_request(request, connection, target),
            DaemonEvent::WebMessage { window_id, body } => self.handle_web_message(window_id, body),
            DaemonEvent::SaveAllAndExit => self.save_all_sessions_and_exit(),
            DaemonEvent::EnsureWarm => self.ensure_warm_window(target),
        }
    }

    fn handle_request(
        &mut self,
        request: Value,
        connection: ClientConnection,
        target: &EventLoopWindowTarget<DaemonEvent>,
    ) {
        if request.get("v").and_then(Value::as_u64) != Some(PROTOCOL_VERSION) {
            connection.send_error("unsupported protocol version");
            return;
        }
        let Some(request_type) = request.get("type").and_then(Value::as_str) else {
            connection.send_error("missing request type");
            return;
        };
        match request_type {
            "ping" => connection.send(json!({
                "type": "pong",
                "v": PROTOCOL_VERSION,
                "openCount": self.sessions.len(),
                "warm": self.warm_window_is_ready(),
            })),
            "warm" => self.handle_warm(connection, target),
            "open" => self.handle_open(request, connection, target),
            "close" => self.handle_close(request, connection),
            "status" => self.handle_status(connection),
            "front" => self.handle_front(request, connection),
            "retitle" => self.handle_retitle(request),
            "watch" => {
                // Watch subscriptions push openCount changes over a held-open
                // connection so hosts can reflect editor windows the moment
                // they open or close instead of polling with ping.
                connection.send(json!({
                    "type": "watching",
                    "v": PROTOCOL_VERSION,
                    "openCount": self.sessions.len(),
                }));
                self.open_count_watchers.push(connection);
            }
            "shutdown" => self.handle_shutdown(connection),
            _ => connection.send_error("unknown request type"),
        }
    }

    fn handle_warm(
        &mut self,
        connection: ClientConnection,
        target: &EventLoopWindowTarget<DaemonEvent>,
    ) {
        if self.warm_window_is_ready() {
            connection.send(json!({"type": "warmed", "v": PROTOCOL_VERSION}));
            return;
        }
        self.warm_waiters.push(connection);
        self.ensure_warm_window(target);
    }

    fn handle_open(
        &mut self,
        request: Value,
        connection: ClientConnection,
        target: &EventLoopWindowTarget<DaemonEvent>,
    ) {
        let Some(request_id) =
            string_field(&request, "requestId").filter(|value| !value.is_empty())
        else {
            connection.send_error("open request requires requestId");
            return;
        };
        if self.sessions.contains_key(&request_id) {
            connection.send_error("requestId already open");
            return;
        }
        let Some(file_path) = absolute_path_field(&request, "filePath") else {
            connection.send_error("open request requires absolute filePath");
            return;
        };
        let Some(status_file) = absolute_path_field(&request, "statusFile") else {
            connection.send_error("open request requires absolute statusFile");
            return;
        };

        let language = string_field(&request, "language")
            .filter(|value| !value.is_empty())
            .or_else(|| Some(DEFAULT_LANGUAGE.to_string()));
        let originating_session_id = originating_session_id_field(&request);
        let title = string_field(&request, "title")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_TITLE.to_string());
        let initial_text = match fs::read_to_string(&file_path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                connection.send_error(format!("unable to read file: {error}"));
                return;
            }
        };

        let window_id = match self.take_ready_warm_window() {
            Some(window_id) => window_id,
            None => match self.make_editor_window(target) {
                Ok(window_id) => window_id,
                Err(error) => {
                    connection.send_error(format!("unable to open editor window: {error}"));
                    return;
                }
            },
        };

        if let Some(window) = self.windows.get_mut(&window_id) {
            window.window.set_title(&session_window_title(&title));
            window.session_request_id = Some(request_id.clone());
        }

        self.sessions.insert(
            request_id.clone(),
            EditorSession {
                request_id,
                originating_session_id,
                file_path,
                status_file,
                language,
                title,
                initial_text: initial_text.clone(),
                initial_cursor_offset: self.last_cursor_snapshot.as_ref().and_then(|snapshot| {
                    (snapshot.text == initial_text).then_some(snapshot.cursor_offset)
                }),
                latest_draft: initial_text,
                latest_cursor_offset: None,
                opener: connection,
                window_id,
                has_opened: false,
                is_finishing: false,
            },
        );
        self.configure_window_if_ready(window_id);
        // Presentation happens right at open handling, before the configure
        // round-trip through the web layer completes: a warm window already
        // has Monaco loaded, so waiting for the "configured" reply only
        // delays window visibility.
        self.present_window(window_id);
        self.notify_open_count_watchers();
        self.ensure_warm_window(target);
    }

    fn notify_open_count_watchers(&mut self) {
        let message = json!({
            "type": "openCountChanged",
            "v": PROTOCOL_VERSION,
            "openCount": self.sessions.len(),
        });
        self.open_count_watchers
            .retain(|watcher| watcher.send_checked(message.clone()));
    }

    fn handle_close(&mut self, request: Value, connection: ClientConnection) {
        let Some(request_id) =
            string_field(&request, "requestId").filter(|value| !value.is_empty())
        else {
            connection.send_error("close request requires requestId");
            return;
        };
        let Some(action) = string_field(&request, "action") else {
            connection.send_error("close request requires action");
            return;
        };
        if !self.sessions.contains_key(&request_id) {
            connection.send_error("unknown requestId");
            return;
        }
        match action.as_str() {
            "save" => {
                connection.send(json!({"type": "ok", "v": PROTOCOL_VERSION}));
                self.request_session_save_and_close(&request_id);
            }
            "cancel" => {
                connection.send(json!({"type": "ok", "v": PROTOCOL_VERSION}));
                self.finish_session(&request_id, CloseAction::Cancel);
            }
            _ => connection.send_error("close action must be save or cancel"),
        }
    }

    fn handle_status(&self, connection: ClientConnection) {
        let sessions: Vec<Value> = self
            .sessions
            .values()
            .map(|session| {
                json!({
                    "requestId": session.request_id,
                    "title": session.title,
                })
            })
            .collect();
        connection.send(json!({
            "type": "status",
            "v": PROTOCOL_VERSION,
            "sessions": sessions,
            "warm": self.warm_window_is_ready(),
        }));
    }

    fn handle_front(&self, request: Value, connection: ClientConnection) {
        let originating_session_id = originating_session_id_field(&request);
        let mut fronted_count = 0usize;
        for session in self.sessions.values() {
            if originating_session_id
                .as_deref()
                .is_some_and(|originating_session_id| {
                    session.originating_session_id.as_deref() != Some(originating_session_id)
                })
            {
                continue;
            }
            let Some(window) = self.windows.get(&session.window_id) else {
                continue;
            };
            window.window.set_visible(true);
            window.window.set_focus();
            fronted_count += 1;
        }
        connection.send(json!({
            "type": "fronted",
            "v": PROTOCOL_VERSION,
            "frontedCount": fronted_count,
            "openCount": self.sessions.len(),
        }));
    }

    fn handle_retitle(&mut self, request: Value) {
        // No-reply notification: the CLI resolves the originating terminal
        // session's display title from gxserver after `open`, so a reply (or
        // an unknown-requestId error for a session that already closed) would
        // only inject noise into the opener's opened/closed message waiters.
        let Some(request_id) =
            string_field(&request, "requestId").filter(|value| !value.is_empty())
        else {
            return;
        };
        let Some(title) = string_field(&request, "title").filter(|value| !value.is_empty()) else {
            return;
        };
        let Some(session) = self.sessions.get_mut(&request_id) else {
            return;
        };
        session.title = title;
        if let Some(window) = self.windows.get(&session.window_id) {
            window
                .window
                .set_title(&session_window_title(&session.title));
        }
    }

    fn handle_shutdown(&mut self, connection: ClientConnection) {
        self.pending_shutdown = true;
        connection.send(json!({"type": "ok", "v": PROTOCOL_VERSION}));
        if self.sessions.is_empty() {
            self.should_exit = true;
        }
    }

    fn handle_web_message(&mut self, window_id: WindowId, body: String) {
        let Ok(message) = serde_json::from_str::<Value>(&body) else {
            return;
        };
        let Some(message_type) = message.get("type").and_then(Value::as_str) else {
            return;
        };
        match message_type {
            "ready" => {
                if let Some(window) = self.windows.get_mut(&window_id) {
                    window.is_ready = true;
                }
                self.configure_window_if_ready(window_id);
                self.notify_warm_waiters_if_ready(window_id);
            }
            "configured" => self.session_configured(window_id),
            "draftUpdate" => {
                if let Some(text) = message.get("text").and_then(Value::as_str) {
                    self.update_session_draft(window_id, text);
                }
                if let Some(cursor_offset) = cursor_offset_field(&message, "cursorOffset") {
                    self.update_session_cursor_offset(window_id, cursor_offset);
                }
            }
            "cursorUpdate" => {
                if let Some(cursor_offset) = cursor_offset_field(&message, "cursorOffset") {
                    self.update_session_cursor_offset(window_id, cursor_offset);
                }
            }
            "saveAndClose" => {
                if let Some(text) = message.get("text").and_then(Value::as_str) {
                    self.update_session_draft(window_id, text);
                }
                if let Some(cursor_offset) = cursor_offset_field(&message, "cursorOffset") {
                    self.update_session_cursor_offset(window_id, cursor_offset);
                }
                if let Some(request_id) = self.request_id_for_window(window_id) {
                    self.finish_session(&request_id, CloseAction::Save);
                }
            }
            "save" => {
                if let Some(text) = message.get("text").and_then(Value::as_str) {
                    self.update_session_draft(window_id, text);
                }
                if let Some(cursor_offset) = cursor_offset_field(&message, "cursorOffset") {
                    self.update_session_cursor_offset(window_id, cursor_offset);
                }
                if let Some(request_id) = self.request_id_for_window(window_id) {
                    self.save_session_draft_without_closing(&request_id);
                }
            }
            "cancel" => {
                if let Some(text) = message.get("text").and_then(Value::as_str) {
                    self.update_session_draft(window_id, text);
                }
                if let Some(cursor_offset) = cursor_offset_field(&message, "cursorOffset") {
                    self.update_session_cursor_offset(window_id, cursor_offset);
                }
                if let Some(request_id) = self.request_id_for_window(window_id) {
                    self.finish_session(&request_id, CloseAction::Cancel);
                }
            }
            "pasteImage" => self.handle_paste_image(window_id, &message),
            "loadImagePreview" => self.handle_load_image_preview(window_id, &message),
            _ => {}
        }
    }

    fn handle_window_close(&mut self, window_id: WindowId) {
        if let Some(request_id) = self.request_id_for_window(window_id) {
            self.request_session_save_and_close(&request_id);
        } else if Some(window_id) == self.warm_window {
            self.warm_window = None;
            self.windows.remove(&window_id);
        }
    }

    fn ensure_warm_window(&mut self, target: &EventLoopWindowTarget<DaemonEvent>) {
        if self.pending_shutdown || self.warm_window.is_some() {
            return;
        }
        match self.make_editor_window(target) {
            Ok(window_id) => self.warm_window = Some(window_id),
            Err(error) => eprintln!("ghostex-editor: unable to warm editor window: {error}"),
        }
    }

    fn warm_window_is_ready(&self) -> bool {
        let Some(window_id) = self.warm_window else {
            return false;
        };
        self.windows
            .get(&window_id)
            .is_some_and(|window| window.is_ready && window.session_request_id.is_none())
    }

    fn notify_warm_waiters_if_ready(&mut self, window_id: WindowId) {
        if Some(window_id) != self.warm_window || !self.warm_window_is_ready() {
            return;
        }
        let waiters = std::mem::take(&mut self.warm_waiters);
        for waiter in waiters {
            waiter.send(json!({"type": "warmed", "v": PROTOCOL_VERSION}));
        }
    }

    fn take_ready_warm_window(&mut self) -> Option<WindowId> {
        if !self.warm_window_is_ready() {
            return None;
        }
        self.warm_window.take()
    }

    fn make_editor_window(
        &mut self,
        target: &EventLoopWindowTarget<DaemonEvent>,
    ) -> Result<WindowId, String> {
        let builder = apply_window_platform_policy(
            WindowBuilder::new()
                .with_title(APP_WINDOW_TITLE)
                .with_inner_size(LogicalSize::new(900.0, 620.0))
                .with_min_inner_size(LogicalSize::new(480.0, 320.0))
                .with_visible(false),
        );
        let window = builder
            .build(target)
            .map_err(|error| format!("unable to create window: {error}"))?;
        let window_id = window.id();
        let proxy = self.proxy.clone();
        let web_root = self.web_root.clone();
        let protocol_root = self.web_root.clone();
        let webview_builder = WebViewBuilder::new()
            .with_initialization_script(
                r#"
Object.defineProperty(window, "__require", {
  configurable: true,
  get: function() { return window.require; }
});
"#,
            )
            .with_custom_protocol(CUSTOM_PROTOCOL.to_string(), move |_webview_id, request| {
                asset_response(&protocol_root, request)
            })
            .with_ipc_handler(move |request| {
                let _ = proxy.send_event(DaemonEvent::WebMessage {
                    window_id,
                    body: request.body().clone(),
                });
            })
            .with_url(format!("{CUSTOM_PROTOCOL}://localhost/index.html"));
        let webview = build_webview(webview_builder, &window)
            .map_err(|error| format!("unable to create webview: {error}"))?;
        self.windows.insert(
            window_id,
            EditorWindow {
                window,
                webview,
                is_ready: false,
                session_request_id: None,
            },
        );
        if !web_root.join("index.html").is_file() {
            return Err("editor web root disappeared".to_string());
        }
        Ok(window_id)
    }

    fn configure_window_if_ready(&mut self, window_id: WindowId) {
        let Some(request_id) = self.request_id_for_window(window_id) else {
            return;
        };
        let Some(window) = self.windows.get(&window_id) else {
            return;
        };
        if !window.is_ready {
            return;
        }
        let Some(session) = self.sessions.get(&request_id) else {
            return;
        };
        let mut detail = json!({
            "type": "configure",
            "initialText": session.initial_text,
            "language": session.language,
            "filePath": session.file_path,
            "title": session.title,
        });
        if let Some(cursor_offset) = session.initial_cursor_offset {
            detail["cursorOffset"] = json!(cursor_offset);
        }
        dispatch_host_message(window, &detail);
    }

    fn session_configured(&mut self, window_id: WindowId) {
        let Some(request_id) = self.request_id_for_window(window_id) else {
            return;
        };
        let Some((status_file, opener, opened_request_id)) =
            self.sessions.get_mut(&request_id).and_then(|session| {
                if session.has_opened {
                    return None;
                }
                session.has_opened = true;
                Some((
                    session.status_file.clone(),
                    session.opener.clone(),
                    session.request_id.clone(),
                ))
            })
        else {
            return;
        };
        write_status(&status_file, "started");
        opener.send(json!({
            "type": "opened",
            "requestId": opened_request_id,
        }));
    }

    fn present_window(&mut self, window_id: WindowId) {
        let saved_frame = load_saved_window_frame();
        let offset = self.cascade_offset;
        if saved_frame.is_none() {
            self.cascade_offset = (self.cascade_offset + 28) % 224;
        }
        if let Some(editor_window) = self.windows.get(&window_id) {
            if let Some(frame) = saved_frame {
                editor_window
                    .window
                    .set_inner_size(LogicalSize::new(frame.width, frame.height));
                editor_window
                    .window
                    .set_outer_position(LogicalPosition::new(frame.x, frame.y));
            } else {
                editor_window
                    .window
                    .set_outer_position(LogicalPosition::new(
                        80.0 + offset as f64,
                        80.0 + offset as f64,
                    ));
            }
            editor_window.window.set_visible(true);
            editor_window.window.set_focus();
        }
    }

    fn request_session_save_and_close(&mut self, request_id: &str) {
        let Some(session) = self.sessions.get(request_id) else {
            return;
        };
        if let Some(window) = self.windows.get(&session.window_id) {
            let javascript = r#"
document.dispatchEvent(new KeyboardEvent("keydown", {
  key: "s",
  metaKey: true,
  ctrlKey: true,
  bubbles: true,
  cancelable: true
}));
"#;
            let _ = window.webview.evaluate_script(javascript);
        } else {
            self.finish_session(request_id, CloseAction::Save);
        }
    }

    fn update_session_draft(&mut self, window_id: WindowId, text: &str) {
        let Some(request_id) = self.request_id_for_window(window_id) else {
            return;
        };
        if let Some(session) = self.sessions.get_mut(&request_id) {
            session.latest_draft = text.to_string();
        }
    }

    fn update_session_cursor_offset(&mut self, window_id: WindowId, cursor_offset: usize) {
        let Some(request_id) = self.request_id_for_window(window_id) else {
            return;
        };
        if let Some(session) = self.sessions.get_mut(&request_id) {
            session.latest_cursor_offset = Some(cursor_offset);
        }
    }

    fn save_session_draft_without_closing(&self, request_id: &str) {
        if let Some(session) = self.sessions.get(request_id) {
            if let Err(error) = write_draft_atomically(&session.file_path, &session.latest_draft) {
                eprintln!(
                    "ghostex-editor: save failed for {}: {error}",
                    session.file_path.display()
                );
            }
        }
    }

    fn finish_session(&mut self, request_id: &str, action: CloseAction) {
        let Some((window_id, cursor_snapshot)) =
            self.sessions.get_mut(request_id).and_then(|session| {
                if session.is_finishing {
                    return None;
                }
                session.is_finishing = true;

                match action {
                    CloseAction::Save => {
                        if let Err(error) =
                            write_draft_atomically(&session.file_path, &session.latest_draft)
                        {
                            eprintln!(
                                "ghostex-editor: save failed for {}: {error}",
                                session.file_path.display()
                            );
                        }
                        write_status(&session.status_file, "saved");
                        session.opener.send(json!({
                            "type": "closed",
                            "requestId": session.request_id,
                            "status": "saved",
                        }));
                    }
                    CloseAction::Cancel => {
                        write_status(&session.status_file, "cancelled");
                        session.opener.send(json!({
                            "type": "closed",
                            "requestId": session.request_id,
                            "status": "cancelled",
                        }));
                    }
                }

                let cursor_snapshot =
                    session
                        .latest_cursor_offset
                        .map(|cursor_offset| CursorSnapshot {
                            text: session.latest_draft.clone(),
                            cursor_offset,
                        });
                Some((session.window_id, cursor_snapshot))
            })
        else {
            return;
        };
        if let Some(cursor_snapshot) = cursor_snapshot {
            self.last_cursor_snapshot = Some(cursor_snapshot);
        }
        // Every session window is presented at open handling, so its frame is
        // the user's latest; the never-presented warm window is not a session.
        if let Some(window) = self.windows.get(&window_id) {
            save_window_frame(&window.window);
        }
        self.sessions.remove(request_id);
        self.windows.remove(&window_id);
        self.notify_open_count_watchers();
        if self.pending_shutdown && self.sessions.is_empty() {
            self.should_exit = true;
        } else if !self.pending_shutdown && self.warm_window.is_none() {
            let _ = self.proxy.send_event(DaemonEvent::EnsureWarm);
        }
    }

    fn handle_paste_image(&mut self, window_id: WindowId, message: &Value) {
        let Some(request_id) = self.request_id_for_window(window_id) else {
            return;
        };
        let Some(session) = self.sessions.get(&request_id) else {
            return;
        };
        let paste_request_id = message
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if paste_request_id.is_empty() {
            return;
        }

        let result = save_pasted_image(session, message);
        let detail = match result {
            Ok(path) => json!({
                "type": "imagePasteResult",
                "requestId": paste_request_id,
                "path": path,
            }),
            Err(error) => json!({
                "type": "imagePasteResult",
                "requestId": paste_request_id,
                "error": error,
            }),
        };
        if let Some(window) = self.windows.get(&window_id) {
            dispatch_host_message(window, &detail);
        }
    }

    fn handle_load_image_preview(&mut self, window_id: WindowId, message: &Value) {
        let preview_request_id = message
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let Some(path) = message.get("path").and_then(Value::as_str) else {
            return;
        };
        if preview_request_id.is_empty() {
            return;
        }

        // The thumbnail shelf must load every image path already present in
        // the Monaco text. Resolve short ~ paths natively and send data URLs
        // back to the web layer so webview local-file read limits do not
        // block thumbnail or popup rendering.
        let detail = match load_image_preview_data_url(path) {
            Ok(data_url) => json!({
                "type": "imagePreviewResult",
                "requestId": preview_request_id,
                "path": path,
                "dataUrl": data_url,
            }),
            Err(error) => json!({
                "type": "imagePreviewResult",
                "requestId": preview_request_id,
                "path": path,
                "error": error,
            }),
        };
        if let Some(window) = self.windows.get(&window_id) {
            dispatch_host_message(window, &detail);
        }
    }

    fn request_id_for_window(&self, window_id: WindowId) -> Option<String> {
        self.windows
            .get(&window_id)
            .and_then(|window| window.session_request_id.clone())
    }

    fn save_all_sessions_and_exit(&mut self) {
        self.pending_shutdown = true;
        let request_ids: Vec<String> = self.sessions.keys().cloned().collect();
        for request_id in request_ids {
            self.finish_session(&request_id, CloseAction::Save);
        }
        self.should_exit = true;
    }

    fn cleanup_socket(&mut self) {
        if let Some(path) = self.socket_cleanup_path.take() {
            remove_stale_socket(&Some(path));
        }
    }
}

fn load_image_preview_data_url(path: &str) -> Result<String, String> {
    let file_path = resolve_image_preview_path(path)
        .ok_or_else(|| "Image preview path does not point to a local image.".to_string())?;
    let mime_type = image_preview_mime_type(&file_path)
        .ok_or_else(|| "Image preview path does not point to a local image.".to_string())?;
    let data =
        fs::read(&file_path).map_err(|error| format!("Image preview read failed: {error}"))?;
    Ok(format!(
        "data:{mime_type};base64,{}",
        general_purpose::STANDARD.encode(data)
    ))
}

fn resolve_image_preview_path(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if let Some(stripped) = trimmed.strip_prefix("file://") {
        let decoded = percent_decode_str(stripped).decode_utf8().ok()?;
        return Some(PathBuf::from(decoded.as_ref()));
    }
    if let Some(stripped) = trimmed.strip_prefix("~/") {
        let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
        return Some(PathBuf::from(home).join(stripped));
    }
    let candidate = PathBuf::from(trimmed);
    candidate.is_absolute().then_some(candidate)
}

fn image_preview_mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "avif" => Some("image/avif"),
        "gif" => Some("image/gif"),
        "heic" | "heif" => Some("image/heic"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "svg" => Some("image/svg+xml"),
        "tif" | "tiff" => Some("image/tiff"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn string_field(request: &Value, key: &str) -> Option<String> {
    request.get(key).and_then(Value::as_str).map(str::to_string)
}

fn originating_session_id_field(request: &Value) -> Option<String> {
    let value = request.get("originatingSessionId")?.as_str()?;
    let (project_id, session_id) = value.split_once(':')?;
    (valid_originating_session_id_part(project_id, b'P')
        && valid_originating_session_id_part(session_id, b'G'))
    .then(|| value.to_string())
}

fn valid_originating_session_id_part(value: &str, prefix: u8) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 5
        && bytes[0] == prefix
        && bytes[1].is_ascii_digit()
        && bytes[2..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
}

fn session_window_title(session_title: &str) -> String {
    if session_title == DEFAULT_TITLE {
        return APP_WINDOW_TITLE.to_string();
    }
    format!("{session_title} — {APP_WINDOW_TITLE}")
}

fn absolute_path_field(request: &Value, key: &str) -> Option<PathBuf> {
    let value = request.get(key).and_then(Value::as_str)?;
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

fn cursor_offset_field(request: &Value, key: &str) -> Option<usize> {
    request
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn apply_window_platform_policy(builder: WindowBuilder) -> WindowBuilder {
    apply_skip_taskbar(builder)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn apply_window_platform_policy(builder: WindowBuilder) -> WindowBuilder {
    builder
}

#[cfg(target_os = "linux")]
fn apply_skip_taskbar(builder: WindowBuilder) -> WindowBuilder {
    use tao::platform::unix::WindowBuilderExtUnix;
    builder.with_skip_taskbar(true)
}

#[cfg(target_os = "windows")]
fn apply_skip_taskbar(builder: WindowBuilder) -> WindowBuilder {
    use tao::platform::windows::WindowBuilderExtWindows;
    builder.with_skip_taskbar(true)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> wry::Result<WebView> {
    builder.build(window)
}

#[cfg(target_os = "linux")]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> wry::Result<WebView> {
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewBuilderExtUnix;
    let vbox = window
        .default_vbox()
        .ok_or_else(|| wry::Error::Message("missing GTK vbox".into()))?;
    builder.build_gtk(vbox)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> wry::Result<WebView> {
    builder.build(window)
}

fn dispatch_host_message(window: &EditorWindow, detail: &Value) {
    let Ok(json) = serde_json::to_string(detail) else {
        return;
    };
    let script = format!(
        "window.dispatchEvent(new CustomEvent(\"ghostex-editor-host-message\", {{ detail: {json} }}));"
    );
    let _ = window.webview.evaluate_script(&script);
}

fn write_draft_atomically(path: &Path, draft: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(directory)?;
    temp.write_all(draft.as_bytes())?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map(|_| ()).map_err(|error| error.error)
}

#[derive(Clone, Copy)]
struct WindowFrame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn window_frame_store_path() -> Option<PathBuf> {
    Some(resolved_state_directory().join("editor-window-frame.json"))
}

fn absolute_environment_path(variable: &str) -> Option<PathBuf> {
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

#[cfg(not(target_os = "windows"))]
fn resolved_state_directory() -> PathBuf {
    if let Some(ghostex_home) = absolute_environment_path("GHOSTEX_HOME") {
        return ghostex_home.join("state");
    }
    let home = absolute_environment_path("HOME").unwrap_or_else(|| PathBuf::from("."));
    absolute_environment_path("XDG_STATE_HOME")
        .unwrap_or_else(|| home.join(".local/state"))
        .join("ghostex")
}

#[cfg(target_os = "windows")]
fn resolved_state_directory() -> PathBuf {
    if let Some(ghostex_home) = absolute_environment_path("GHOSTEX_HOME") {
        return ghostex_home.join("state");
    }
    let user_home = absolute_environment_path("USERPROFILE").unwrap_or_else(|| PathBuf::from("."));
    absolute_environment_path("LOCALAPPDATA")
        .unwrap_or_else(|| user_home.join("AppData/Local"))
        .join("Ghostex/State")
}

fn load_saved_window_frame() -> Option<WindowFrame> {
    let path = window_frame_store_path()?;
    let text = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let field = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|number| number.is_finite())
    };
    let frame = WindowFrame {
        x: field("x")?,
        y: field("y")?,
        width: field("width")?,
        height: field("height")?,
    };
    (frame.width > 0.0 && frame.height > 0.0).then_some(frame)
}

fn save_window_frame(window: &Window) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    let Some(path) = window_frame_store_path() else {
        return;
    };
    let scale = window.scale_factor();
    let logical_position = position.to_logical::<f64>(scale);
    let logical_size = window.inner_size().to_logical::<f64>(scale);
    let value = json!({
        "x": logical_position.x,
        "y": logical_position.y,
        "width": logical_size.width,
        "height": logical_size.height,
    });
    if let Err(error) = write_draft_atomically(&path, &value.to_string()) {
        eprintln!("ghostex-editor: window frame save failed: {error}");
    }
}

fn write_status(path: &Path, status: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(error) = write_draft_atomically(path, status) {
        eprintln!(
            "ghostex-editor: status write failed for {}: {error}",
            path.display()
        );
    }
}

fn save_pasted_image(session: &EditorSession, message: &Value) -> Result<PathBuf, String> {
    let base64_data = message
        .get("base64Data")
        .and_then(Value::as_str)
        .ok_or_else(|| "Image paste did not include base64Data.".to_string())?;
    let encoded = base64_data
        .split_once(',')
        .map(|(_, tail)| tail)
        .unwrap_or(base64_data);
    let data = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Image paste data was not valid base64.".to_string())?;
    let image_directory = ensure_image_directory(&session.file_path)
        .map_err(|error| format!("Unable to prepare image directory: {error}"))?;
    let suggested_name = message
        .get("suggestedName")
        .and_then(Value::as_str)
        .unwrap_or("image.png");
    let file_name = unique_image_file_name(suggested_name);
    let file_path = image_directory.join(file_name);
    fs::write(&file_path, data).map_err(|error| format!("Unable to write image: {error}"))?;
    Ok(file_path)
}

fn ensure_image_directory(file_path: &Path) -> io::Result<PathBuf> {
    let draft_directory = file_path.parent().unwrap_or_else(|| Path::new("."));
    let base_directory = if draft_directory.exists() {
        draft_directory.to_path_buf()
    } else {
        env::temp_dir()
    };
    let image_directory = base_directory.join("ghostex-editor-images");
    fs::create_dir_all(&image_directory)?;
    Ok(image_directory)
}

fn unique_image_file_name(suggested_name: &str) -> String {
    let base = Path::new(suggested_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image.png");
    let mut filtered: String = base
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    filtered = filtered.trim_matches(['.', '-', ' ']).to_string();
    if filtered.is_empty() {
        filtered = "image.png".to_string();
    }
    if Path::new(&filtered).extension().is_none() {
        filtered.push_str(".png");
    }
    format!("{}-{filtered}", Uuid::new_v4())
}

fn asset_response(
    web_root: &Path,
    request: Request<Vec<u8>>,
) -> Response<std::borrow::Cow<'static, [u8]>> {
    match asset_response_body(web_root, request) {
        Ok((mime, body)) => Response::builder()
            .header(CONTENT_TYPE, mime)
            .body(std::borrow::Cow::Owned(body))
            .unwrap(),
        Err((status, message)) => Response::builder()
            .status(status)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(std::borrow::Cow::Owned(message.into_bytes()))
            .unwrap(),
    }
}

fn asset_response_body(
    web_root: &Path,
    request: Request<Vec<u8>>,
) -> Result<(&'static str, Vec<u8>), (u16, String)> {
    let relative_path = request.uri().path().trim_start_matches('/');
    let relative_path = if relative_path.is_empty() {
        "index.html"
    } else {
        relative_path
    };
    let decoded = percent_decode_str(relative_path)
        .decode_utf8()
        .map_err(|_| (400, "invalid asset path".to_string()))?;
    let path = safe_asset_path(web_root, &decoded)?;
    let body = fs::read(&path).map_err(|_| (404, "asset not found".to_string()))?;
    let mime = mime_guess::from_path(&path)
        .first_raw()
        .unwrap_or("application/octet-stream");
    Ok((mime, body))
}

fn safe_asset_path(web_root: &Path, relative: &str) -> Result<PathBuf, (u16, String)> {
    let mut clean = PathBuf::new();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return Err((403, "invalid asset path".to_string())),
        }
    }
    let path = web_root.join(clean);
    let canonical_root = web_root
        .canonicalize()
        .map_err(|_| (500, "web root unavailable".to_string()))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|_| (404, "asset not found".to_string()))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err((403, "invalid asset path".to_string()));
    }
    Ok(canonical_path)
}
