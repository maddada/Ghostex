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
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User: "sending messages is the most important part of this app and it's broken now", after
//! messages sent from the chat reached the agent 7 to 18 seconds late (and once two minutes):
//! gxserver took them in under a second, the app held them. Everything this thread does came
//! through one first-come queue, so an Enter waited behind every frame the chat socket had read
//! for every retained chat, each of which costs a document. What the app itself asks (a view's
//! call such as a send, its answer to a request, an attach, a query) now goes ahead of what the
//! socket read ([`Lanes`]): it waits for the turn in progress at most, never for the backlog.
//! Frames keep their own order and a view's calls keep theirs; the two were never ordered against
//! each other (they come from different threads). A turn that takes long, or a command that
//! waited long, leaves an always-kept `gxChat.host.slowTurn` line, so a busy thread is no longer
//! invisible on the user's computer.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::Duration;

use ghostex_gx_chat_client::{ChatStreams, Endpoint};
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

static COMMANDS: OnceLock<Arc<Lanes>> = OnceLock::new();
static IDLE: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// Which queue a command waits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lane {
    /// What the app asks: a view's call (a send among them), its answers, attaches and queries,
    /// and the app's own facts (endpoints, computers, Read aloud). Taken first.
    App,
    /// What the chat socket read, for any retained chat.
    Socket,
}

/// The host thread's two queues, the app's ahead of the socket's, each in its own order.
#[derive(Default)]
struct Lanes {
    queues: Mutex<Queues>,
    ready: Condvar,
}

#[derive(Default)]
struct Queues {
    app: VecDeque<(HostCommand, Instant)>,
    socket: VecDeque<(HostCommand, Instant)>,
}

/// One command off the queues: which lane it came from, when it was queued, and how many frames
/// from the socket still wait behind it.
struct Taken {
    command: HostCommand,
    lane: Lane,
    queued_at: Instant,
    socket_waiting: usize,
}

impl Lanes {
    fn push(&self, lane: Lane, command: HostCommand) {
        let mut queues = self
            .queues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = (command, Instant::now());
        match lane {
            Lane::App => queues.app.push_back(entry),
            Lane::Socket => queues.socket.push_back(entry),
        }
        drop(queues);
        self.ready.notify_one();
    }

    /// The app's oldest command, else the socket's oldest, else whatever arrives within `wait`
    /// (for ever when `None`). `None` when the wait ran out with nothing queued.
    fn take(&self, wait: Option<Duration>) -> Option<Taken> {
        let deadline = wait.map(|wait| Instant::now() + wait);
        let mut queues = self
            .queues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            let taken = match queues.app.pop_front() {
                Some(entry) => Some((entry, Lane::App)),
                None => queues.socket.pop_front().map(|entry| (entry, Lane::Socket)),
            };
            if let Some(((command, queued_at), lane)) = taken {
                return Some(Taken {
                    command,
                    lane,
                    queued_at,
                    socket_waiting: queues.socket.len(),
                });
            }
            queues = match deadline {
                None => self
                    .ready
                    .wait(queues)
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return None;
                    }
                    self.ready
                        .wait_timeout(queues, left)
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .0
                }
            };
        }
    }
}

/// The host's queues, starting its thread on first use.
fn lanes() -> &'static Arc<Lanes> {
    COMMANDS.get_or_init(spawn)
}
static NEXT_SINK: AtomicU64 = AtomicU64::new(1);

/// Where a machine's gxserver is, for every chat on it. The app calls this whenever the local
/// daemon's bootstrap or a remote machine's forward changes; the same value again does nothing.
pub(crate) fn set_endpoint(machine_id: &str, base_url: &str, auth_token: &str) {
    lanes().push(
        Lane::App,
        HostCommand::Endpoint {
            machine_id: machine_id.to_string(),
            endpoint: Endpoint::new(base_url, auth_token),
        },
    );
}

/// The published computers changed (`computers.rs`): run a turn so every chat hears it now.
pub(super) fn nudge_computers() {
    lanes().push(Lane::App, HostCommand::Computers);
}

/// Read aloud's app-wide facts and the speaker's answers, for every chat (`read_aloud.rs`).
pub(super) fn send_read_aloud(input: ReadAloudInput) {
    lanes().push(Lane::App, HostCommand::ReadAloud(input));
}

impl ChatHostHandle {
    /// Attaches a view to the chat its config names, starting the host thread on first use.
    ///
    /// `config` is the object `NativeChatView::start_runtime` builds.
    pub(crate) fn start(mut config: Value, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let commands = lanes();
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
        commands.push(
            Lane::App,
            HostCommand::Attach {
                endpoint: transport::config_endpoint(&config),
                identity,
                sink: Sink {
                    id,
                    outputs,
                    wake: Arc::new(wake),
                    last_revision: 0,
                    paused: false,
                },
            },
        );
        // `start` is an ordinary call once the chat exists, so the core hears it the same way the
        // QuickJS brain heard `nativeChat.start(config)`.
        commands.push(
            Lane::App,
            HostCommand::Call {
                key: key.clone(),
                sink: id,
                method: "start",
                arguments: vec![config],
            },
        );
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
            commands.push(
                Lane::App,
                HostCommand::Call {
                    key: self.key.clone(),
                    sink: self.id,
                    method,
                    arguments,
                },
            );
        }
    }

    /// Stops or restarts this view's document while it is off screen.
    pub(crate) fn set_paused(&self, paused: bool) {
        if let Some(commands) = COMMANDS.get() {
            commands.push(
                Lane::App,
                HostCommand::Pause {
                    key: self.key.clone(),
                    sink: self.id,
                    paused,
                },
            );
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
        commands.push(
            Lane::App,
            HostCommand::Query {
                key: self.key.clone(),
                method,
                arguments,
                reply,
            },
        );
        answer.recv_timeout(timeout).ok()?
    }

    pub(crate) fn take_outputs(&self) -> Vec<ChatHostOutput> {
        self.outputs.try_iter().collect()
    }
}

impl Drop for ChatHostHandle {
    fn drop(&mut self) {
        if let Some(commands) = COMMANDS.get() {
            commands.push(
                Lane::App,
                HostCommand::Detach {
                    key: self.key.clone(),
                    sink: self.id,
                },
            );
        }
    }
}

/// Starts the host thread and hands back its queues.
fn spawn() -> Arc<Lanes> {
    let lanes = Arc::new(Lanes::default());
    let idle = IDLE
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .clone();
    let inbound = lanes.clone();
    let commands = lanes.clone();
    thread::Builder::new()
        .name("ghostex-gx-chat-host".into())
        .spawn(move || {
            let mut world = World::default();
            // The socket threads queue what they read behind the app's own commands.
            world
                .transport
                .install(ChatStreams::new(Arc::new(move |value| {
                    inbound.push(Lane::Socket, HostCommand::Inbound(value));
                })));
            run(&mut world, &commands, idle);
        })
        .expect("spawn the chat host thread");
    lanes
}

fn run(world: &mut World, commands: &Lanes, idle: Arc<AtomicBool>) {
    let mut slow = SlowTurns::default();
    loop {
        idle.store(true, Ordering::Release);
        let wait = world::next_wake(world).map(|at| at.saturating_duration_since(Instant::now()));
        let taken = commands.take(wait);
        idle.store(false, Ordering::Release);
        let started = Instant::now();
        let facts = taken.as_ref().map(|taken| {
            (
                taken.command.kind(),
                taken.lane,
                started.saturating_duration_since(taken.queued_at),
                taken.socket_waiting,
            )
        });
        world.slowest_drive = None;
        world::turn(world, taken.map(|taken| taken.command));
        slow.note(world, facts, started.elapsed());
    }
}

/// A turn at least this long, or a command that waited at least `WAITED_LONG` for its turn, is
/// written down. An ordinary turn is a few milliseconds.
const TURN_LONG: Duration = Duration::from_millis(250);
const WAITED_LONG: Duration = Duration::from_secs(1);
/// At most one line this often; the next one says how many it stands for.
const SLOW_LINE_EVERY: Duration = Duration::from_secs(2);

/// The `gxChat.host.slowTurn` line and its throttle.
#[derive(Default)]
struct SlowTurns {
    written_at: Option<Instant>,
    /// Slow turns since the last line, and the longest of them.
    unwritten: u64,
    longest_unwritten: Duration,
}

impl SlowTurns {
    /// `facts` is the command's kind, its lane, how long it waited and how many socket frames
    /// were still queued when it was taken; `None` for a timer pass.
    fn note(
        &mut self,
        world: &World,
        facts: Option<(&'static str, Lane, Duration, usize)>,
        took: Duration,
    ) {
        let waited = facts.map_or(Duration::ZERO, |(_, _, waited, _)| waited);
        if took < TURN_LONG && waited < WAITED_LONG {
            return;
        }
        if self
            .written_at
            .is_some_and(|at| at.elapsed() < SLOW_LINE_EVERY)
        {
            self.unwritten += 1;
            self.longest_unwritten = self.longest_unwritten.max(took);
            return;
        }
        // No message, title or text: what kind of command it was, which chat's document took the
        // longest (the session the send log names too), and how far behind the thread was.
        let (kind, lane, _, socket_waiting) =
            facts.unwrap_or(("timers", Lane::App, Duration::ZERO, 0));
        crate::support_logs::append_always(
            crate::support_logs::GpuiSupportLog::SessionChat,
            "gxChat.host.slowTurn",
            serde_json::json!({
                "command": kind,
                "lane": match (facts.is_some(), lane) {
                    (false, _) => "timers",
                    (true, Lane::App) => "app",
                    (true, Lane::Socket) => "socket",
                },
                "tookMs": took.as_millis() as u64,
                "waitedMs": waited.as_millis() as u64,
                "socketFramesWaiting": socket_waiting,
                "slowestChat": world.slowest_drive.as_ref().map(|(key, _)| key.clone()),
                "slowestChatMs": world
                    .slowest_drive
                    .as_ref()
                    .map(|(_, took)| took.as_millis() as u64),
                "chatsRetained": world.store.len(),
                "slowTurnsSinceLastLine": self.unwritten,
                "longestSinceLastLineMs": self.longest_unwritten.as_millis() as u64,
            }),
        );
        self.written_at = Some(Instant::now());
        self.unwritten = 0;
        self.longest_unwritten = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(method: &'static str) -> HostCommand {
        HostCommand::Call {
            key: "chat".into(),
            sink: 1,
            method,
            arguments: Vec::new(),
        }
    }

    fn method(taken: Option<Taken>) -> (&'static str, Lane, usize) {
        let taken = taken.expect("a command");
        match taken.command {
            HostCommand::Call { method, .. } => (method, taken.lane, taken.socket_waiting),
            _ => ("other", taken.lane, taken.socket_waiting),
        }
    }

    /// A send from a view is taken ahead of every frame the socket read before it, and each lane
    /// keeps its own order (CDXC:SessionChat 2026-11-11 above).
    #[test]
    fn what_the_app_asks_goes_ahead_of_what_the_socket_read() {
        let lanes = Lanes::default();
        lanes.push(Lane::Socket, call("frame 1"));
        lanes.push(Lane::Socket, call("frame 2"));
        lanes.push(Lane::App, call("send"));
        lanes.push(Lane::Socket, call("frame 3"));
        lanes.push(Lane::App, call("resolve"));
        let zero = Some(Duration::ZERO);
        assert_eq!(method(lanes.take(zero)), ("send", Lane::App, 3));
        assert_eq!(method(lanes.take(zero)), ("resolve", Lane::App, 3));
        assert_eq!(method(lanes.take(zero)), ("frame 1", Lane::Socket, 2));
        assert_eq!(method(lanes.take(zero)), ("frame 2", Lane::Socket, 1));
        assert_eq!(method(lanes.take(zero)), ("frame 3", Lane::Socket, 0));
        assert!(lanes.take(zero).is_none(), "nothing left: the timers' turn");
        assert!(lanes.take(Some(Duration::from_millis(5))).is_none());
    }

    /// A thread waiting on empty queues is woken by a command from either lane.
    #[test]
    fn a_waiting_host_wakes_for_a_command() {
        for lane in [Lane::App, Lane::Socket] {
            let lanes = Arc::new(Lanes::default());
            let pusher = lanes.clone();
            let pushed = thread::spawn(move || {
                thread::sleep(Duration::from_millis(20));
                pusher.push(lane, call("late"));
            });
            let taken = lanes.take(Some(Duration::from_secs(10)));
            assert_eq!(method(taken), ("late", lane, 0));
            pushed.join().unwrap();
        }
    }
}
