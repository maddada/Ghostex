//! The one background thread the Rust chat host runs on, and the per-view handle onto it.
//!
//! CDXC:SessionChat 2026-09-22 WHY:
//! ONE thread for every chat, not one per view. The deleted QuickJS brain needed a thread each
//! because each view owned a 96 MiB runtime; a `ChatCore` is a plain value, so twelve of them share
//! a thread and the store's retention is a map on it. The chat socket's threads
//! (`packages/gx-chat-client`) hand what they read to this one over the same channel.
//!
//! The GPUI web build has its own file of this name with the same handle and no thread; the host
//! itself (`world.rs`) is shared.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread;
use std::time::Duration;

use ghostex_gx_chat_client::{ChatStreams, Endpoint, InboundBacklog};
use serde_json::Value;
use web_time::Instant;

use super::identity::ChatIdentity;
use super::read_aloud::ReadAloudInput;
use super::transport;
use super::world::{self, ChatHostOutput, HostCommand, Sink, World};

/// A view's door onto the shared host.
pub(crate) struct ChatHostHandle {
    key: String,
    id: u64,
    outputs: mpsc::Receiver<ChatHostOutput>,
    idle: Arc<AtomicBool>,
}

static COMMANDS: OnceLock<mpsc::Sender<HostCommand>> = OnceLock::new();
static IDLE: OnceLock<Arc<AtomicBool>> = OnceLock::new();
static NEXT_SINK: AtomicU64 = AtomicU64::new(1);

/// How much wire text the socket threads may have handed the host that it has not handled yet,
/// beyond the one frame always let in (`packages/gx-chat-client/src/backlog.rs`).
///
/// CDXC:SessionChat 2026-11-10 WHY: four of a coordinator chat's 3.4 MB state frames, or thousands
/// of the few-kilobyte frames a streaming reply sends. The queue had no limit (one suspect for the
/// app's 2.8 GB peaks); it now holds at most this much wire text, a few times that once parsed (an
/// estimate, not measured), and a socket thread past it waits while the kernel buffers the rest.
const INBOUND_BACKLOG_BYTES: usize = 16 * 1024 * 1024;

/// Where a machine's gxserver is, for every chat on it. The app calls this whenever the local
/// daemon's bootstrap or a remote machine's forward changes; the same value again does nothing.
pub(crate) fn set_endpoint(machine_id: &str, base_url: &str, auth_token: &str) {
    let _ = COMMANDS.get_or_init(spawn).send(HostCommand::Endpoint {
        machine_id: machine_id.to_string(),
        endpoint: Endpoint::new(base_url, auth_token),
    });
}

/// The published computers changed (`computers.rs`): run a turn so every chat hears it now.
pub(super) fn nudge_computers() {
    let _ = COMMANDS.get_or_init(spawn).send(HostCommand::Computers);
}

/// Read aloud's app-wide facts and the speaker's answers, for every chat (`read_aloud.rs`).
pub(super) fn send_read_aloud(input: ReadAloudInput) {
    let _ = COMMANDS
        .get_or_init(spawn)
        .send(HostCommand::ReadAloud(input));
}

impl ChatHostHandle {
    /// Attaches a view to the chat its config names, starting the host thread on first use.
    ///
    /// `config` is the object `NativeChatView::start_runtime` builds.
    pub(crate) fn start(mut config: Value, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let commands = COMMANDS.get_or_init(spawn).clone();
        let identity = ChatIdentity::from_config(&config);
        let key = identity.retention_key();
        // `StartConfig::retained_key`. The core writes it back inside the retained record's own
        // `key` field and cannot build it, because the machine id is the host's half of the
        // identity; the view's config does not carry it either, so it is added on the way past.
        if let Some(config) = config.as_object_mut() {
            config.insert("retainedKey".into(), Value::String(key.clone()));
        }
        let id = NEXT_SINK.fetch_add(1, Ordering::Relaxed);
        let (outputs, receiver) = mpsc::channel();
        let _ = commands.send(HostCommand::Attach {
            endpoint: transport::config_endpoint(&config),
            identity,
            sink: Sink {
                id,
                outputs,
                wake: Arc::new(wake),
                last_revision: 0,
                paused: false,
            },
        });
        // `start` is an ordinary call once the chat exists, so the core hears it the same way the
        // QuickJS brain heard `nativeChat.start(config)`.
        let _ = commands.send(HostCommand::Call {
            key: key.clone(),
            sink: id,
            method: "start",
            arguments: vec![config],
        });
        Self {
            key,
            id,
            outputs: receiver,
            idle: IDLE
                .get_or_init(|| Arc::new(AtomicBool::new(false)))
                .clone(),
        }
    }

    /// The chat's retention key, which names it as the chat on screen (`read_aloud.rs`).
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn call(&self, method: &'static str, arguments: Vec<Value>) {
        if let Some(commands) = COMMANDS.get() {
            let _ = commands.send(HostCommand::Call {
                key: self.key.clone(),
                sink: self.id,
                method,
                arguments,
            });
        }
    }

    /// Stops or restarts this view's document while it is off screen.
    pub(crate) fn set_paused(&self, paused: bool) {
        if let Some(commands) = COMMANDS.get() {
            let _ = commands.send(HostCommand::Pause {
                key: self.key.clone(),
                sink: self.id,
                paused,
            });
        }
    }

    /// A pure helper for a per-paint caller: a busy thread answers nothing rather than stalling the
    /// frame, which is the rule the QuickJS worker's `query` wrote down.
    pub(crate) fn query(
        &self,
        method: &'static str,
        arguments: Vec<Value>,
        timeout: Duration,
    ) -> Option<Value> {
        if !self.idle.load(Ordering::Acquire) {
            return None;
        }
        self.query_for_gesture(method, arguments, timeout)
    }

    /// A pure helper for one deliberate press, which queues behind running work.
    pub(crate) fn query_for_gesture(
        &self,
        method: &'static str,
        arguments: Vec<Value>,
        timeout: Duration,
    ) -> Option<Value> {
        let commands = COMMANDS.get()?;
        let (reply, answer) = mpsc::channel();
        commands
            .send(HostCommand::Query {
                key: self.key.clone(),
                method,
                arguments,
                reply,
            })
            .ok()?;
        answer.recv_timeout(timeout).ok()?
    }

    pub(crate) fn take_outputs(&self) -> Vec<ChatHostOutput> {
        self.outputs.try_iter().collect()
    }
}

impl Drop for ChatHostHandle {
    fn drop(&mut self) {
        if let Some(commands) = COMMANDS.get() {
            let _ = commands.send(HostCommand::Detach {
                key: self.key.clone(),
                sink: self.id,
            });
        }
    }
}

/// Starts the host thread and hands back its command channel.
fn spawn() -> mpsc::Sender<HostCommand> {
    let (commands, receiver) = mpsc::channel::<HostCommand>();
    let idle = IDLE
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .clone();
    let inbound = commands.clone();
    let backlog = InboundBacklog::new(INBOUND_BACKLOG_BYTES);
    thread::Builder::new()
        .name("ghostex-gx-chat-host".into())
        .spawn(move || {
            // However this thread ends, even by a panic outside `turn`'s guard, no socket thread
            // is left waiting for room it would never get.
            let _open_on_exit = OpenOnExit(backlog.clone());
            let mut world = World::default();
            // The socket threads queue what they read behind the views' own commands, on the same
            // channel, once the backlog has room; the views' commands never wait for it.
            world.transport.install(ChatStreams::new(
                Arc::new(move |value| {
                    let _ = inbound.send(HostCommand::Inbound(value));
                }),
                backlog.clone(),
            ));
            run(&mut world, receiver, idle, &backlog);
        })
        .expect("spawn the chat host thread");
    commands
}

/// Opens the backlog for good when the host thread ends.
struct OpenOnExit(Arc<InboundBacklog>);

impl Drop for OpenOnExit {
    fn drop(&mut self) {
        self.0.close();
    }
}

fn run(
    world: &mut World,
    commands: mpsc::Receiver<HostCommand>,
    idle: Arc<AtomicBool>,
    backlog: &InboundBacklog,
) {
    loop {
        idle.store(true, Ordering::Release);
        let wait = world::next_wake(world).map(|at| at.saturating_duration_since(Instant::now()));
        let command = match wait {
            Some(wait) => match commands.recv_timeout(wait) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            },
            None => match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => return,
            },
        };
        idle.store(false, Ordering::Release);
        // Released after the turn, so the frame being folded still counts against the backlog.
        let inbound = matches!(command, Some(HostCommand::Inbound(_)));
        world::turn(world, command);
        if inbound {
            backlog.handled();
        }
    }
}
