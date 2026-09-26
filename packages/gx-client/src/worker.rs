//! The socket thread: connect, subscribe, read, reconnect.

use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ghostex_gx_core::{ConnectionUpdate, Event};
use ghostex_gx_protocol::{
    peek_event_type, ClientMessage, EventParseError, ServerEvent, EVENT_TYPE_API_REQUEST_HANDLED,
    GXSERVER_PROTOCOL_VERSION,
};
use tungstenite::{Error as SocketError, Message};

use crate::config::{
    GxClientConfig, DOMAIN_PROJECTS_READ_TIMEOUT, FORCED_RESUBSCRIBE_INTERVAL,
    HEALTHY_STREAM_DURATION, RECONNECT_LADDER_MS, SUBSCRIBE_ACK_TIMEOUT,
};
use crate::http;
use crate::output::{bump, redact_quoted_values, ClientDiagnostic, ClientOutput, ClientStats};
use crate::socket::{self, Endpoint, EventSocket};

pub(crate) enum Command {
    /// Subscribe again without `lastRevision`, which forces a full snapshot.
    Resubscribe,
    /// A message the host wants written to the daemon: a renderer command's answer.
    Send(String),
    Shutdown,
}

/// How one connection ended.
struct ConnectionEnd {
    /// `None` when the host asked the thread to stop.
    lost: Option<String>,
    /// When the daemon first acknowledged a subscribe on this socket.
    acknowledged_at: Option<Instant>,
}

/// Why the read loop must leave a socket.
enum Leave {
    Stop,
    Lost(String),
}

pub(crate) struct Worker {
    pub(crate) config: GxClientConfig,
    pub(crate) endpoint: Endpoint,
    pub(crate) outputs: Sender<ClientOutput>,
    pub(crate) commands: Receiver<Command>,
    pub(crate) shutdown: Arc<AtomicBool>,
    pub(crate) wake: Box<dyn Fn() + Send>,
    pub(crate) wake_pending: Arc<AtomicBool>,
    pub(crate) stats: Arc<ClientStats>,
}

/// State that outlives one socket.
#[derive(Default)]
struct Resubscribes {
    /// The next subscribe must leave `lastRevision` out. Set by a host request or by a delta
    /// that failed to parse; cleared only when a full snapshot arrives, so a socket that drops
    /// in between cannot turn the request into a `presentationSnapshotCurrent` answer.
    full_snapshot_wanted: bool,
    /// When the client last forced a resubscribe on its own, and when the next one is due.
    last_forced_at: Option<Instant>,
    forced_due_at: Option<Instant>,
    /// Host messages that arrived while no socket was open, written after the next subscribe.
    unsent: Vec<String>,
}

impl Worker {
    /// Runs the thread body and reports any exit the host did not ask for.
    ///
    /// `catch_unwind` rather than a drop guard: the thread must still say something after a
    /// panic, and saying it takes the worker's own sender and wake. A guard would need second
    /// handles to both, and could not read the panic message. `AssertUnwindSafe` is sound here
    /// because after a panic the worker is only used to queue these two outputs. Whatever
    /// happens, the wake closure is dropped when this returns, which is the host's race-free
    /// signal that the thread is gone.
    pub(crate) fn run_reporting_exit(self) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run()));
        let reason = match outcome {
            Ok(()) if self.stopped() => return,
            Ok(()) => "the client thread returned without being asked to stop".to_string(),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|message| (*message).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "no message".to_string());
                // A panic message can quote data; keep its shape only, and keep it short.
                let message: String = redact_quoted_values(&message).chars().take(200).collect();
                format!("the client thread panicked: {message}")
            }
        };
        self.send(ClientOutput::Event(self.connection(
            ConnectionUpdate::Lost {
                error: Some("the client thread stopped".to_string()),
            },
        )));
        self.emit(ClientOutput::Diagnostic(ClientDiagnostic::ThreadStopped {
            reason,
        }));
    }

    fn run(&self) {
        let agent = http::agent(DOMAIN_PROJECTS_READ_TIMEOUT);
        let mut resubscribes = Resubscribes::default();
        let mut attempt: u32 = 0;
        loop {
            if self.stopped() {
                return;
            }
            self.emit(ClientOutput::Event(
                self.connection(ConnectionUpdate::Connecting { attempt }),
            ));
            let end = self.run_connection(&agent, &mut resubscribes);
            let Some(error) = end.lost else {
                return;
            };
            self.emit(ClientOutput::Event(
                self.connection(ConnectionUpdate::Lost { error: Some(error) }),
            ));
            // A close is the only signal that frames were lost, so every reconnect subscribes
            // again with the revision the store holds. The ladder starts over only after the
            // stream had been healthy for a while; a flapping daemon keeps escalating.
            if end
                .acknowledged_at
                .is_some_and(|at| at.elapsed() >= HEALTHY_STREAM_DURATION)
            {
                attempt = 0;
            }
            let step = (attempt as usize).min(RECONNECT_LADDER_MS.len() - 1);
            attempt = attempt.saturating_add(1);
            if self.wait(
                Duration::from_millis(RECONNECT_LADDER_MS[step]),
                &mut resubscribes,
            ) {
                return;
            }
        }
    }

    fn connection(&self, update: ConnectionUpdate) -> Event {
        Event::Connection {
            machine: self.config.machine.clone(),
            update,
        }
    }

    fn stopped(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Sleeps between reconnect attempts while staying responsive. Returns `true` to stop.
    fn wait(&self, delay: Duration, resubscribes: &mut Resubscribes) -> bool {
        let deadline = Instant::now() + delay;
        loop {
            if self.stopped() {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            match self.commands.recv_timeout(remaining) {
                // The reconnect subscribes anyway; remember that it must be a full snapshot.
                Ok(Command::Resubscribe) => resubscribes.full_snapshot_wanted = true,
                Ok(Command::Send(text)) => resubscribes.unsent.push(text),
                Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => return true,
                Err(RecvTimeoutError::Timeout) => return false,
            }
        }
    }

    fn run_connection(
        &self,
        agent: &ureq::Agent,
        resubscribes: &mut Resubscribes,
    ) -> ConnectionEnd {
        let mut acknowledged_at = None;
        let lost = match socket::connect(&self.endpoint, &self.config.auth_token) {
            Err(error) => Some(error),
            Ok(mut socket) => {
                bump(&self.stats.connects);
                let leave =
                    self.read_socket(&mut socket, agent, resubscribes, &mut acknowledged_at);
                // Best effort: the daemon notices a vanished client either way.
                let _ = socket.close(None);
                let _ = socket.flush();
                match leave {
                    Leave::Stop => None,
                    Leave::Lost(error) => Some(error),
                }
            }
        };
        ConnectionEnd {
            lost,
            acknowledged_at,
        }
    }

    fn read_socket(
        &self,
        socket: &mut EventSocket,
        agent: &ureq::Agent,
        resubscribes: &mut Resubscribes,
        acknowledged_at: &mut Option<Instant>,
    ) -> Leave {
        if let Err(leave) = self.subscribe(socket, resubscribes) {
            return leave;
        }
        for text in std::mem::take(&mut resubscribes.unsent) {
            if let Err(leave) = self.send_text(socket, text) {
                return leave;
            }
        }
        let mut ack_deadline = Some(Instant::now() + SUBSCRIBE_ACK_TIMEOUT);
        loop {
            loop {
                match self.commands.try_recv() {
                    Ok(Command::Resubscribe) => {
                        resubscribes.full_snapshot_wanted = true;
                        if let Err(leave) = self.subscribe(socket, resubscribes) {
                            return leave;
                        }
                        ack_deadline = Some(Instant::now() + SUBSCRIBE_ACK_TIMEOUT);
                    }
                    Ok(Command::Send(text)) => {
                        if let Err(leave) = self.send_text(socket, text) {
                            return leave;
                        }
                    }
                    Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => return Leave::Stop,
                    Err(TryRecvError::Empty) => break,
                }
            }
            if self.stopped() {
                return Leave::Stop;
            }
            let now = Instant::now();
            if resubscribes.forced_due_at.is_some_and(|due| now >= due) {
                if let Err(leave) = self.subscribe(socket, resubscribes) {
                    return leave;
                }
                ack_deadline = Some(now + SUBSCRIBE_ACK_TIMEOUT);
            }
            if ack_deadline.is_some_and(|deadline| now >= deadline) {
                self.emit(ClientOutput::Diagnostic(
                    ClientDiagnostic::SubscribeNotAcknowledged,
                ));
                return Leave::Lost("the daemon did not acknowledge the subscribe".to_string());
            }
            let text = match socket.read() {
                Ok(Message::Text(text)) => text,
                // The daemon sends text; a binary frame is accepted when it is UTF-8 JSON.
                Ok(Message::Binary(bytes)) => match String::from_utf8(bytes.to_vec()) {
                    Ok(text) => text.into(),
                    Err(_) => continue,
                },
                Ok(Message::Ping(_)) => {
                    // The pong is queued by the read; push it out now rather than with the next
                    // write, which may never come on a socket that only listens.
                    let _ = socket.flush();
                    continue;
                }
                Ok(Message::Close(_)) => {
                    return Leave::Lost("the daemon closed the event stream".to_string())
                }
                Ok(_) => continue,
                Err(SocketError::Io(error))
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    continue
                }
                Err(SocketError::ConnectionClosed | SocketError::AlreadyClosed) => {
                    return Leave::Lost("the event stream closed".to_string())
                }
                Err(error) => return Leave::Lost(format!("the event stream failed: {error}")),
            };
            if let Err(leave) = self.handle_frame(
                text.as_str(),
                agent,
                resubscribes,
                acknowledged_at,
                &mut ack_deadline,
            ) {
                return leave;
            }
        }
    }

    /// Writes one host message (a renderer command's answer) on the live socket.
    fn send_text(&self, socket: &mut EventSocket, text: String) -> Result<(), Leave> {
        socket
            .send(Message::Text(text.into()))
            .map_err(|error| Leave::Lost(format!("could not send a renderer answer: {error}")))?;
        bump(&self.stats.renderer_answers);
        Ok(())
    }

    /// Sends `subscribePresentation`. `rendererCommands` is set only when the host's config asks
    /// for it, which makes this socket the daemon's renderer-command target.
    fn subscribe(
        &self,
        socket: &mut EventSocket,
        resubscribes: &mut Resubscribes,
    ) -> Result<(), Leave> {
        let held = self.config.held_revision.load(Ordering::Acquire);
        let message = ClientMessage::SubscribePresentation {
            client_id: Some(self.config.client_id.clone()),
            last_revision: (!resubscribes.full_snapshot_wanted && held > 0).then_some(held),
            renderer_commands: self.config.renderer_commands.then_some(true),
        };
        let text = serde_json::to_string(&message)
            .map_err(|error| Leave::Lost(format!("could not encode the subscribe: {error}")))?;
        socket
            .send(Message::Text(text.into()))
            .map_err(|error| Leave::Lost(format!("could not send the subscribe: {error}")))?;
        bump(&self.stats.subscribes);
        if resubscribes.forced_due_at.take().is_some() {
            resubscribes.last_forced_at = Some(Instant::now());
        }
        Ok(())
    }

    fn handle_frame(
        &self,
        text: &str,
        agent: &ureq::Agent,
        resubscribes: &mut Resubscribes,
        acknowledged_at: &mut Option<Instant>,
        ack_deadline: &mut Option<Instant>,
    ) -> Result<(), Leave> {
        bump(&self.stats.frames_received);
        self.stats
            .bytes_received
            .fetch_add(text.len() as u64, Ordering::Relaxed);
        // One `apiRequestHandled` frame arrives per HTTP request any client makes: by far the
        // most frequent frame, and nothing reads it. Drop it before paying for a parse.
        match peek_event_type(text) {
            Some(EVENT_TYPE_API_REQUEST_HANDLED) => {
                bump(&self.stats.api_request_handled_dropped);
                return Ok(());
            }
            Some(event_type)
                if !self.config.forward_chat_frames && event_type.starts_with("sessionChat") =>
            {
                bump(&self.stats.chat_frames_dropped);
                return Ok(());
            }
            _ => {}
        }
        let frame = match ServerEvent::parse(text) {
            Ok(frame) => frame,
            Err(error) => {
                self.report_parse_failure(&error, resubscribes);
                return Ok(());
            }
        };
        if self.config.renderer_commands {
            if let ServerEvent::RendererCommand(command) = frame {
                bump(&self.stats.renderer_commands);
                self.emit(ClientOutput::RendererCommand(command.command));
                return Ok(());
            }
        }
        if let ServerEvent::EventStreamReady(header) = &frame {
            if header.protocol_version != GXSERVER_PROTOCOL_VERSION {
                let received = header.protocol_version;
                self.emit(ClientOutput::Diagnostic(
                    ClientDiagnostic::ProtocolMismatch { received },
                ));
                return Err(Leave::Lost(format!(
                    "the daemon speaks protocol version {received}"
                )));
            }
        }
        let acknowledges = matches!(
            frame,
            ServerEvent::PresentationSnapshot(_) | ServerEvent::PresentationSnapshotCurrent(_)
        );
        if matches!(frame, ServerEvent::PresentationSnapshot(_)) {
            resubscribes.full_snapshot_wanted = false;
        }
        if acknowledges {
            acknowledged_at.get_or_insert_with(Instant::now);
            *ack_deadline = None;
            // The domain project rows are read here, on the socket thread, between receiving the
            // acknowledgement and handing it to the host. That fixes the order of the read
            // relative to the stream: every delta still waiting in the socket was produced at
            // or before the read or after it, and is applied after the list either way, so a
            // `domainProject` a delta carries can never be overwritten by an older list. A read
            // on a second thread would land at an arbitrary point between those deltas and could
            // drop a project added in the gap. The cost is that the socket is not read for the
            // length of one loopback request (milliseconds, capped by a short timeout); the
            // daemon only closes a socket that falls 256 frames behind. Both events then reach
            // the host in one burst, so the store's first loaded state already knows which
            // projects are chat projects.
            match http::list_projects(agent, &self.endpoint, &self.config.auth_token) {
                Ok(projects) => {
                    bump(&self.stats.domain_project_reads);
                    self.send(ClientOutput::Event(Event::DomainProjectsRead {
                        machine: self.config.machine.clone(),
                        projects,
                    }));
                }
                Err(error) => self.send(ClientOutput::Diagnostic(
                    ClientDiagnostic::DomainProjectsReadFailed { error },
                )),
            }
        }
        bump(&self.stats.frames_forwarded);
        self.emit(ClientOutput::Event(Event::Frame {
            machine: self.config.machine.clone(),
            frame: Box::new(frame),
        }));
        Ok(())
    }

    fn report_parse_failure(&self, error: &EventParseError, resubscribes: &mut Resubscribes) {
        bump(&self.stats.parse_failures);
        // A delta that cannot be read is a change the store never saw, and no later frame
        // repeats it. A full snapshot does, and its rows are parsed one by one, so the bad row
        // is skipped and reported there instead of failing the frame. Rate limited: a daemon
        // that keeps producing the bad row must not be asked for a snapshot per delta.
        let resubscribe_scheduled = error.event_type() == Some("presentationDelta");
        if resubscribe_scheduled {
            resubscribes.full_snapshot_wanted = true;
            if resubscribes.forced_due_at.is_none() {
                let now = Instant::now();
                let earliest = resubscribes
                    .last_forced_at
                    .map_or(now, |at| at + FORCED_RESUBSCRIBE_INTERVAL);
                resubscribes.forced_due_at = Some(earliest.max(now));
            }
        }
        self.emit(ClientOutput::Diagnostic(
            ClientDiagnostic::FrameParseFailed {
                event_type: error.event_type().map(str::to_string),
                error: describe_parse_error(error),
                resubscribe_scheduled,
            },
        ));
    }

    /// Queues an output without waking the host; follow it with [`Self::emit`].
    fn send(&self, output: ClientOutput) {
        if self.outputs.send(output).is_err() {
            // The host dropped the client: nobody is listening any more.
            self.shutdown.store(true, Ordering::Release);
        }
    }

    /// Queues an output and wakes the host, at most once until the host drains: a burst of
    /// frames costs one wake, not one per frame.
    fn emit(&self, output: ClientOutput) {
        self.send(output);
        if !self.wake_pending.swap(true, Ordering::AcqRel) {
            (self.wake)();
        }
    }
}

/// The parser's message, without the values it quotes.
fn describe_parse_error(error: &EventParseError) -> String {
    match error {
        EventParseError::MissingType => "the frame has no string `type`".to_string(),
        EventParseError::Shape { error, .. } => redact_quoted_values(&error.to_string()),
    }
}
