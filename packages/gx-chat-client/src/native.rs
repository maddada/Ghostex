//! The chat socket on a native target: one tungstenite thread per machine.
//!
//! The thread owns its machine's followers and endpoint; the host only ever sends it commands, so
//! nothing is shared and nothing is locked. It opens the socket only while it has both a follower
//! and a usable endpoint, subscribes every follower on each open (a reconnect's snapshot replaces
//! whatever a dropped socket lost, because gxserver has no replay contract), and walks the
//! [`RECONNECT_DELAYS_MS`](crate::RECONNECT_DELAYS_MS) ladder while the daemon is away.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use ghostex_gx_protocol::{GXSERVER_PROTOCOL_VERSION, GXSERVER_PROTOCOL_VERSION_HEADER};
use tungstenite::client::client_with_config;
use tungstenite::http::Uri;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{ClientRequestBuilder, Error as SocketError, Message, WebSocket};

use crate::wire::{self, Endpoint, Followers, Inbound, StreamKey};

/// Where the thread hands what it read. Called on the socket thread: it must only queue the value
/// for the host's own thread and return.
pub type InboundSink = Arc<dyn Fn(Inbound) + Send + Sync>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long one read may block, which bounds how late the thread notices a command. A subscribe is
/// sent beside the chat's own seed read, so this is never what a chat waits for.
const READ_TIMEOUT: Duration = Duration::from_millis(50);
/// A snapshot of a long conversation is several megabytes; tungstenite's 16 MiB default would turn
/// a larger one into a reconnect loop.
const MAX_MESSAGE_BYTES: usize = 256 * 1024 * 1024;

enum Command {
    Endpoint(Endpoint),
    Follow(StreamKey, u32),
    Refresh(StreamKey),
    Unfollow(StreamKey),
    Shutdown,
}

/// Every machine's chat socket. Dropping it stops every thread.
pub struct ChatStreams {
    sink: InboundSink,
    machines: BTreeMap<String, Sender<Command>>,
}

impl ChatStreams {
    pub fn new(sink: InboundSink) -> Self {
        Self {
            sink,
            machines: BTreeMap::new(),
        }
    }

    /// Where `machine_id`'s gxserver is. A change reconnects that machine's socket; the same value
    /// again does nothing.
    pub fn set_endpoint(&mut self, machine_id: &str, endpoint: Endpoint) {
        self.send(machine_id, Command::Endpoint(endpoint));
    }

    /// Follows a conversation with at least `limit` messages. The first follow of a conversation
    /// subscribes it on the wire; a repeat only records the window for the next reconnect, which is
    /// `store.ts`'s rule: a conversation already streaming is not asked for its snapshot again.
    pub fn follow(&mut self, machine_id: &str, project_id: &str, session_id: &str, limit: u32) {
        let key = (project_id.to_string(), session_id.to_string());
        self.send(machine_id, Command::Follow(key, limit));
    }

    /// Subscribes a followed conversation again on the live socket, which makes gxserver send a
    /// fresh authoritative snapshot. Nothing happens while the socket is down: its reconnect
    /// subscribes every follower anyway.
    pub fn refresh(&mut self, machine_id: &str, project_id: &str, session_id: &str) {
        let key = (project_id.to_string(), session_id.to_string());
        self.send(machine_id, Command::Refresh(key));
    }

    /// Stops following a conversation. The machine's socket closes with its last follower.
    pub fn unfollow(&mut self, machine_id: &str, project_id: &str, session_id: &str) {
        if let Some(commands) = self.machines.get(machine_id) {
            let key = (project_id.to_string(), session_id.to_string());
            let _ = commands.send(Command::Unfollow(key));
        }
    }

    fn send(&mut self, machine_id: &str, command: Command) {
        let sink = self.sink.clone();
        let commands = self
            .machines
            .entry(machine_id.to_string())
            .or_insert_with(|| spawn(machine_id.to_string(), sink));
        let _ = commands.send(command);
    }
}

impl Drop for ChatStreams {
    fn drop(&mut self) {
        for commands in self.machines.values() {
            let _ = commands.send(Command::Shutdown);
        }
    }
}

fn spawn(machine_id: String, sink: InboundSink) -> Sender<Command> {
    let (commands, receiver) = mpsc::channel();
    let link = Link {
        machine_id,
        commands: receiver,
        sink,
        followers: Followers::new(),
        endpoint: Endpoint::default(),
        attempts: 0,
    };
    // A thread that cannot start leaves the sender with no reader: every command is dropped and the
    // chat's own stall watchdog is what notices, as it would for a daemon that never answered.
    let _ = thread::Builder::new()
        .name("ghostex-gx-chat-socket".into())
        .spawn(move || link.run());
    commands
}

/// How a served socket ended.
enum End {
    /// The host is gone or asked the thread to stop.
    Stop,
    /// The last follower left: close and wait for the next one, no backoff.
    Idle,
    /// The endpoint changed: reconnect at once, no backoff.
    Moved,
    /// The daemon closed or the socket failed: walk the ladder.
    Lost,
}

struct Link {
    machine_id: String,
    commands: Receiver<Command>,
    sink: InboundSink,
    followers: Followers,
    endpoint: Endpoint,
    /// Failed or dropped connections in a row, reset on every successful open.
    attempts: usize,
}

impl Link {
    fn run(mut self) {
        loop {
            // Idle until there is someone to follow and somewhere to connect.
            while self.followers.is_empty() || !self.endpoint.usable() {
                match self.commands.recv() {
                    Ok(Command::Shutdown) | Err(_) => return,
                    Ok(command) => {
                        self.apply_offline(command);
                    }
                }
            }
            let end = match connect(&self.endpoint) {
                Ok(mut socket) => {
                    self.attempts = 0;
                    let end = self.serve(&mut socket);
                    // Best effort: gxserver drops a vanished client's followers either way.
                    let _ = socket.close(None);
                    let _ = socket.flush();
                    end
                }
                Err(()) => End::Lost,
            };
            match end {
                End::Stop => return,
                End::Idle | End::Moved => {}
                End::Lost => {
                    if self.back_off() {
                        return;
                    }
                }
            }
        }
    }

    /// A command while no socket is open. Answers whether it changed the endpoint.
    fn apply_offline(&mut self, command: Command) -> bool {
        match command {
            Command::Endpoint(endpoint) => {
                let moved = endpoint != self.endpoint;
                self.endpoint = endpoint;
                if moved {
                    self.attempts = 0;
                }
                moved
            }
            Command::Follow(key, limit) => {
                self.followers.insert(key, limit);
                false
            }
            Command::Unfollow(key) => {
                self.followers.remove(&key);
                false
            }
            Command::Refresh(_) | Command::Shutdown => false,
        }
    }

    /// Waits out the ladder step while staying responsive. Returns `true` to stop.
    fn back_off(&mut self) -> bool {
        let delay = Duration::from_millis(wire::reconnect_delay_ms(self.attempts));
        self.attempts = self.attempts.saturating_add(1);
        let deadline = Instant::now() + delay;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            match self.commands.recv_timeout(remaining) {
                Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => return true,
                // A new endpoint is worth trying at once rather than at the end of the step.
                Ok(command) => {
                    if self.apply_offline(command) {
                        return false;
                    }
                }
                Err(RecvTimeoutError::Timeout) => return false,
            }
        }
    }

    fn serve(&mut self, socket: &mut WebSocket<TcpStream>) -> End {
        for (key, limit) in &self.followers {
            if send(socket, wire::subscribe_message(key, *limit)).is_err() {
                return End::Lost;
            }
        }
        loop {
            loop {
                let command = match self.commands.try_recv() {
                    Ok(command) => command,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return End::Stop,
                };
                let sent = match command {
                    Command::Shutdown => return End::Stop,
                    Command::Endpoint(endpoint) => {
                        if endpoint == self.endpoint {
                            continue;
                        }
                        self.endpoint = endpoint;
                        return End::Moved;
                    }
                    Command::Follow(key, limit) => {
                        let fresh = self.followers.insert(key.clone(), limit).is_none();
                        if !fresh {
                            continue;
                        }
                        send(socket, wire::subscribe_message(&key, limit))
                    }
                    Command::Refresh(key) => match self.followers.get(&key) {
                        Some(limit) => send(socket, wire::subscribe_message(&key, *limit)),
                        None => continue,
                    },
                    Command::Unfollow(key) => {
                        if self.followers.remove(&key).is_none() {
                            continue;
                        }
                        let sent = send(socket, wire::unsubscribe_message(&key));
                        if self.followers.is_empty() {
                            return End::Idle;
                        }
                        sent
                    }
                };
                if sent.is_err() {
                    return End::Lost;
                }
            }
            let text = match socket.read() {
                Ok(Message::Text(text)) => text,
                Ok(Message::Binary(bytes)) => match String::from_utf8(bytes.to_vec()) {
                    Ok(text) => text.into(),
                    Err(_) => continue,
                },
                Ok(Message::Ping(_)) => {
                    // The pong is queued by the read; push it now, since this socket rarely writes.
                    let _ = socket.flush();
                    continue;
                }
                Ok(Message::Close(_)) => return End::Lost,
                Ok(_) => continue,
                Err(SocketError::Io(error))
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    continue
                }
                Err(_) => return End::Lost,
            };
            if let Some(inbound) = wire::route(&self.machine_id, text.as_str(), &self.followers) {
                (self.sink)(inbound);
            }
        }
    }
}

fn send(socket: &mut WebSocket<TcpStream>, text: String) -> Result<(), ()> {
    socket.send(Message::Text(text.into())).map_err(|_| ())
}

/// Opens the chat-only event stream. The token travels in the `Authorization` header, never in the
/// URL. Only `http://` endpoints are served: every native endpoint is a loopback daemon or the
/// loopback end of a remote machine's SSH forward.
fn connect(endpoint: &Endpoint) -> Result<WebSocket<TcpStream>, ()> {
    let url = wire::events_url(&endpoint.base_url).ok_or(())?;
    let authority = endpoint
        .base_url
        .trim()
        .strip_prefix("http://")
        .ok_or(())?
        .trim_end_matches('/');
    let address = authority
        .to_socket_addrs()
        .map_err(|_| ())?
        .next()
        .ok_or(())?;
    let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|_| ())?;
    stream.set_nodelay(true).map_err(|_| ())?;
    stream
        .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
        .map_err(|_| ())?;
    stream
        .set_write_timeout(Some(WRITE_TIMEOUT))
        .map_err(|_| ())?;
    let uri: Uri = url.parse().map_err(|_| ())?;
    let request = ClientRequestBuilder::new(uri)
        .with_header("Authorization", format!("Bearer {}", endpoint.auth_token))
        .with_header(
            GXSERVER_PROTOCOL_VERSION_HEADER,
            GXSERVER_PROTOCOL_VERSION.to_string(),
        );
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let (socket, _response) = client_with_config(request, stream, Some(config)).map_err(|_| ())?;
    socket
        .get_ref()
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|_| ())?;
    Ok(socket)
}
