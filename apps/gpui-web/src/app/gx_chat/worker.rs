//! The desktop's `gx_chat/worker.rs` for a browser page: the same handle, and the same host
//! (`world.rs`), run on the page's one thread.
//!
//! CDXC:WebGpui 2026-09-25 WHY:
//! No thread and no channel to one: every call is a turn of the host, made at once, and the core's
//! timers are one `setTimeout` re-armed at the next deadline after each turn. A call made while a
//! turn is running waits in a queue that the running turn drains before it returns.
//!
//! What a turn drains reaches its view in a LATER macrotask, never from inside the call that
//! produced it. The view's wake feeds a task that pumps the view, which calls the host again, which
//! drains again: on the desktop those hops cross threads, but here a synchronous wake let one task
//! go round that circle without ever yielding, and a chat switched back to with a draft in its
//! composer froze the page. The TypeScript chat this replaced drained in a `setTimeout(0)` for the
//! same reason.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use ghostex_gx_chat_client::{ChatStreams, Endpoint};
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_time::Instant;

use super::identity::ChatIdentity;
use super::transport;
use super::world::{self, ChatHostOutput, HostCommand, Sink, World};

struct Host {
    world: World,
    /// The deadline the page's timer is armed for, and its `setTimeout` handle.
    armed: Option<(Instant, i32)>,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
    static QUEUED: RefCell<VecDeque<HostCommand>> = const { RefCell::new(VecDeque::new()) };
    static NEXT_SINK: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
    /// Each view's own wake, by sink id, and the ids owed a wake at the next macrotask.
    static WAKES: RefCell<std::collections::BTreeMap<u64, Box<dyn Fn()>>> = RefCell::new(Default::default());
    static WAKES_DUE: RefCell<std::collections::BTreeSet<u64>> = RefCell::new(Default::default());
    static WAKE_TIMER: Closure<dyn FnMut()> = Closure::new(run_wakes);
    /// The one function the timer calls, kept for the page's lifetime so re-arming allocates
    /// nothing and a firing timer is never a dropped closure.
    static TIMER: Closure<dyn FnMut()> = Closure::new(|| submit(None));
}

/// Runs one command (or the timer pass) now, or queues it behind the turn already running.
fn submit(command: Option<HostCommand>) {
    let mut command = command;
    let ran = HOST.with(|host| {
        let Ok(mut host) = host.try_borrow_mut() else {
            return false;
        };
        let host = host.get_or_insert_with(new_host);
        // The timer that fired is spent.
        if command.is_none() {
            host.armed = None;
        }
        world::turn(&mut host.world, command.take());
        // Whatever arrived while the turn ran, in order.
        while let Some(queued) = QUEUED.with(|queued| queued.borrow_mut().pop_front()) {
            world::turn(&mut host.world, Some(queued));
        }
        arm_timer(host);
        true
    });
    if !ran {
        if let Some(command) = command {
            QUEUED.with(|queued| queued.borrow_mut().push_back(command));
        }
    }
}

/// Queues a view's wake for the next macrotask.
fn defer_wake(id: u64) {
    let first = WAKES_DUE.with(|due| {
        let mut due = due.borrow_mut();
        let first = due.is_empty();
        due.insert(id);
        first
    });
    if !first {
        return;
    }
    if let Some(window) = web_sys::window() {
        WAKE_TIMER.with(|timer| {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                timer.as_ref().unchecked_ref(),
                0,
            );
        });
    }
}

fn run_wakes() {
    let due = WAKES_DUE.with(|due| std::mem::take(&mut *due.borrow_mut()));
    for id in due {
        WAKES.with(|wakes| {
            if let Some(wake) = wakes.borrow().get(&id) {
                wake();
            }
        });
    }
}

fn new_host() -> Host {
    let mut world = World::default();
    // A socket event arrives on its own turn of the page, never inside one of the host's.
    world
        .transport
        .install(ChatStreams::new(Rc::new(|inbound| {
            submit(Some(HostCommand::Inbound(inbound)));
        })));
    Host { world, armed: None }
}

fn arm_timer(host: &mut Host) {
    let next = world::next_wake(&host.world);
    if host.armed.map(|(at, _)| at) == next {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Some((_, handle)) = host.armed.take() {
        window.clear_timeout_with_handle(handle);
    }
    let Some(at) = next else {
        return;
    };
    let delay = at.saturating_duration_since(Instant::now()).as_millis() as i32;
    let handle = TIMER.with(|timer| {
        window.set_timeout_with_callback_and_timeout_and_arguments_0(
            timer.as_ref().unchecked_ref(),
            delay.max(1),
        )
    });
    host.armed = handle.ok().map(|handle| (at, handle));
}

/// Where a machine's gxserver is, for every chat on it.
pub(crate) fn set_endpoint(machine_id: &str, base_url: &str, auth_token: &str) {
    submit(Some(HostCommand::Endpoint {
        machine_id: machine_id.to_string(),
        endpoint: Endpoint::new(base_url, auth_token),
    }));
}

/// A view's door onto the page's chat host.
pub(crate) struct ChatHostHandle {
    key: String,
    id: u64,
    outputs: mpsc::Receiver<ChatHostOutput>,
}

impl ChatHostHandle {
    /// Attaches a view to the chat its config names.
    pub(crate) fn start(mut config: Value, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let identity = ChatIdentity::from_config(&config);
        let key = identity.retention_key();
        if let Some(config) = config.as_object_mut() {
            config.insert("retainedKey".into(), Value::String(key.clone()));
        }
        let id = NEXT_SINK.with(|next| next.replace(next.get() + 1));
        WAKES.with(|wakes| wakes.borrow_mut().insert(id, Box::new(wake)));
        let (outputs, receiver) = mpsc::channel();
        submit(Some(HostCommand::Attach {
            endpoint: transport::config_endpoint(&config),
            identity,
            sink: Sink {
                id,
                outputs,
                wake: Arc::new(move || defer_wake(id)),
                last_revision: 0,
                paused: false,
            },
        }));
        submit(Some(HostCommand::Call {
            key: key.clone(),
            method: "start",
            arguments: vec![config],
        }));
        Self {
            key,
            id,
            outputs: receiver,
        }
    }

    pub(crate) fn call(&self, method: &'static str, arguments: Vec<Value>) {
        submit(Some(HostCommand::Call {
            key: self.key.clone(),
            method,
            arguments,
        }));
    }

    #[allow(dead_code)]
    pub(crate) fn set_paused(&self, paused: bool) {
        submit(Some(HostCommand::Pause {
            key: self.key.clone(),
            sink: self.id,
            paused,
        }));
    }

    /// The host shares the page's thread, so an answer is never pending.
    pub(crate) fn query(
        &self,
        method: &'static str,
        arguments: Vec<Value>,
        timeout: Duration,
    ) -> Option<Value> {
        self.query_for_gesture(method, arguments, timeout)
    }

    pub(crate) fn query_for_gesture(
        &self,
        method: &'static str,
        arguments: Vec<Value>,
        _timeout: Duration,
    ) -> Option<Value> {
        let (reply, answer) = mpsc::channel();
        submit(Some(HostCommand::Query {
            key: self.key.clone(),
            method,
            arguments,
            reply,
        }));
        answer.try_recv().ok().flatten()
    }

    pub(crate) fn take_outputs(&self) -> Vec<ChatHostOutput> {
        self.outputs.try_iter().collect()
    }
}

impl Drop for ChatHostHandle {
    fn drop(&mut self) {
        WAKES.with(|wakes| wakes.borrow_mut().remove(&self.id));
        submit(Some(HostCommand::Detach {
            key: self.key.clone(),
            sink: self.id,
        }));
    }
}
