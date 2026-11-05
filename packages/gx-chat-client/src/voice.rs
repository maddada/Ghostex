//! The voice socket on a native target: `/api/voice`, one tungstenite thread per conversation or
//! dictation. Binary frames are 24 kHz mono PCM16 audio both ways and text frames are JSON
//! (docs/2026-10-20/voice-mode/voice-agent-client-contract.md).
//!
//! CDXC:VoiceMode 2026-10-20 WHY: the desktop's voice conversation and Grok dictation talk to the same gxserver, with the same endpoint, token header and protocol version as the chat socket, so the socket lives beside it in this crate instead of in a second WebSocket stack. Unlike the chat socket it never reconnects by itself: a conversation reattaches with its `conversationId`, which only the caller knows.

use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ghostex_gx_protocol::{GXSERVER_PROTOCOL_VERSION, GXSERVER_PROTOCOL_VERSION_HEADER};
use serde_json::Value;
use tungstenite::client::client_with_config;
use tungstenite::http::Uri;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{ClientRequestBuilder, Error as SocketError, Message, WebSocket};

use crate::wire::Endpoint;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long one read may block before the thread sends what the caller queued. Microphone audio
/// arrives every 10 to 40 ms, so this bounds how late a chunk leaves.
const READ_TIMEOUT: Duration = Duration::from_millis(5);

/// What the voice socket hands its caller, on the socket thread.
#[derive(Debug)]
pub enum VoiceInbound {
    /// A JSON text frame, parsed. Frames that are not JSON objects are dropped.
    Event(Value),
    /// A binary frame: agent audio, PCM16 24 kHz mono.
    Audio(Vec<u8>),
    /// The socket is gone. `None` after a normal close; otherwise why it could not open or broke,
    /// in a sentence for the user.
    Closed(Option<String>),
}

/// Called on the socket thread for everything it reads. It must only queue the value (or hand
/// audio to the speaker) and return.
pub type VoiceSink = Arc<dyn Fn(VoiceInbound) + Send + Sync>;

enum Command {
    Audio(Vec<u8>),
    Text(String),
    Close,
}

/// One open `/api/voice` socket. Dropping it closes the socket without an `end` frame, which
/// leaves a conversation reattachable for two minutes.
pub struct VoiceSocket {
    commands: Sender<Command>,
}

impl VoiceSocket {
    /// Opens `/api/voice?<query>` on `endpoint` on a thread of its own. Everything the socket
    /// reads, and how it ended, goes to `sink`.
    pub fn open(endpoint: &Endpoint, query: &[(&str, &str)], sink: VoiceSink) -> Self {
        let (commands, receiver) = mpsc::channel();
        let url = voice_url(&endpoint.base_url, query);
        let endpoint = endpoint.clone();
        let spawned = thread::Builder::new()
            .name("ghostex-voice-socket".into())
            .spawn({
                let sink = sink.clone();
                move || run(endpoint, url, receiver, sink)
            });
        if spawned.is_err() {
            sink(VoiceInbound::Closed(Some(
                "Could not start the voice connection.".to_string(),
            )));
        }
        Self { commands }
    }

    /// Sends microphone audio (PCM16 24 kHz mono, little-endian bytes).
    pub fn send_audio(&self, pcm: Vec<u8>) {
        let _ = self.commands.send(Command::Audio(pcm));
    }

    /// Sends one JSON control frame (`end`, `interrupt`, `playback`, `text`, `cancel`).
    pub fn send_json(&self, frame: &Value) {
        let _ = self.commands.send(Command::Text(frame.to_string()));
    }

    /// Closes the socket after what is already queued has been sent.
    pub fn close(&self) {
        let _ = self.commands.send(Command::Close);
    }
}

impl Drop for VoiceSocket {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Close);
    }
}

/// `ws://host:port/api/voice?protocolVersion=1&...`, each query value percent-encoded.
fn voice_url(base_url: &str, query: &[(&str, &str)]) -> Option<String> {
    let base = base_url.trim().trim_end_matches('/');
    let rest = base.strip_prefix("http://")?;
    let mut url = format!("ws://{rest}/api/voice?protocolVersion={GXSERVER_PROTOCOL_VERSION}");
    for (key, value) in query {
        url.push('&');
        url.push_str(key);
        url.push('=');
        url.push_str(&percent_encode(value));
    }
    Some(url)
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn run(endpoint: Endpoint, url: Option<String>, commands: Receiver<Command>, sink: VoiceSink) {
    let mut socket = match url
        .ok_or_else(unreachable)
        .and_then(|url| connect(&endpoint, &url))
    {
        Ok(socket) => socket,
        Err(message) => {
            sink(VoiceInbound::Closed(Some(message)));
            return;
        }
    };
    let ended = serve(&mut socket, &commands, &sink);
    let _ = socket.close(None);
    let _ = socket.flush();
    sink(VoiceInbound::Closed(ended));
}

/// Pumps the socket until either side closes it. Answers why it broke, or `None`.
fn serve(
    socket: &mut WebSocket<TcpStream>,
    commands: &Receiver<Command>,
    sink: &VoiceSink,
) -> Option<String> {
    let lost = || Some("The connection to Ghostex's service was lost.".to_string());
    loop {
        loop {
            let message = match commands.try_recv() {
                Ok(Command::Audio(pcm)) => Message::Binary(pcm.into()),
                Ok(Command::Text(text)) => Message::Text(text.into()),
                Ok(Command::Close) | Err(TryRecvError::Disconnected) => return None,
                Err(TryRecvError::Empty) => break,
            };
            if socket.send(message).is_err() {
                return lost();
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Ok(value @ Value::Object(_)) = serde_json::from_str::<Value>(text.as_str()) {
                    sink(VoiceInbound::Event(value));
                }
            }
            Ok(Message::Binary(bytes)) => sink(VoiceInbound::Audio(bytes.to_vec())),
            Ok(Message::Ping(_)) => {
                let _ = socket.flush();
            }
            Ok(Message::Close(_)) => return None,
            Ok(_) => {}
            Err(SocketError::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(SocketError::ConnectionClosed | SocketError::AlreadyClosed) => return None,
            Err(_) => return lost(),
        }
    }
}

fn unreachable() -> String {
    "Could not reach Ghostex's service.".to_string()
}

/// Opens the socket with the token in the `Authorization` header, never in the URL. Only
/// `http://` endpoints are served: every native endpoint is a loopback daemon or the loopback end
/// of an SSH forward.
fn connect(endpoint: &Endpoint, url: &str) -> Result<WebSocket<TcpStream>, String> {
    let authority = endpoint
        .base_url
        .trim()
        .strip_prefix("http://")
        .ok_or_else(unreachable)?
        .trim_end_matches('/');
    let address = authority
        .to_socket_addrs()
        .map_err(|_| unreachable())?
        .next()
        .ok_or_else(unreachable)?;
    ghostex_paths::loopback_owner::check_url(&url).map_err(|_| unreachable())?;
    let stream =
        TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|_| unreachable())?;
    stream.set_nodelay(true).map_err(|_| unreachable())?;
    stream
        .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
        .map_err(|_| unreachable())?;
    stream
        .set_write_timeout(Some(WRITE_TIMEOUT))
        .map_err(|_| unreachable())?;
    let uri: Uri = url.parse().map_err(|_| unreachable())?;
    let request = ClientRequestBuilder::new(uri)
        .with_header("Authorization", format!("Bearer {}", endpoint.auth_token))
        .with_header(
            GXSERVER_PROTOCOL_VERSION_HEADER,
            GXSERVER_PROTOCOL_VERSION.to_string(),
        );
    let (socket, _response) = client_with_config(request, stream, Some(WebSocketConfig::default()))
        .map_err(|error| match error {
            tungstenite::HandshakeError::Failure(SocketError::Http(response)) => {
                match response.status().as_u16() {
                    401 => "Ghostex's service refused this app's token.".to_string(),
                    404 => {
                        "This Ghostex service has no voice agent yet. Update Ghostex.".to_string()
                    }
                    426 => {
                        "Ghostex's service runs a different version. Restart Ghostex.".to_string()
                    }
                    status => format!("Ghostex's service answered {status}."),
                }
            }
            _ => unreachable(),
        })?;
    socket
        .get_ref()
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|_| unreachable())?;
    Ok(socket)
}
