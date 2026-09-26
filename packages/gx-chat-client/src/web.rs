//! The chat socket in a browser page: a `WebSocket` per machine on the page's one thread.
//!
//! The same rules as `native.rs`, driven by the socket's own events instead of a read loop.
//! Browsers cannot set headers on a WebSocket, so the token rides the query string, which gxserver
//! accepts on `/api/events` for exactly this reason.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

use crate::wire::{self, Endpoint, Followers, Inbound};

/// Where the socket hands what it read. It is called from the socket's own event, never from
/// inside a [`ChatStreams`] call, so it may call back into [`ChatStreams`].
pub type InboundSink = Rc<dyn Fn(Inbound)>;

/// Every machine's chat socket.
pub struct ChatStreams {
    sink: InboundSink,
    machines: BTreeMap<String, Rc<RefCell<Machine>>>,
}

impl ChatStreams {
    pub fn new(sink: InboundSink) -> Self {
        Self {
            sink,
            machines: BTreeMap::new(),
        }
    }

    /// Where `machine_id`'s gxserver is. A change reconnects that machine's socket.
    pub fn set_endpoint(&mut self, machine_id: &str, endpoint: Endpoint) {
        let machine = self.machine(machine_id);
        let moved = {
            let mut state = machine.borrow_mut();
            let moved = state.endpoint != endpoint;
            state.endpoint = endpoint;
            if moved {
                state.attempts = 0;
                state.disconnect();
            }
            moved
        };
        if moved {
            connect(&machine);
        }
    }

    /// Follows a conversation; see the native twin for the repeat rule.
    pub fn follow(&mut self, machine_id: &str, project_id: &str, session_id: &str, limit: u32) {
        let machine = self.machine(machine_id);
        let key = (project_id.to_string(), session_id.to_string());
        let idle = {
            let mut state = machine.borrow_mut();
            let fresh = state.followers.insert(key.clone(), limit).is_none();
            if fresh && state.open {
                state.send(&wire::subscribe_message(&key, limit));
            }
            state.socket.is_none() && state.retry.is_none()
        };
        if idle {
            connect(&machine);
        }
    }

    /// Subscribes a followed conversation again on the live socket.
    pub fn refresh(&mut self, machine_id: &str, project_id: &str, session_id: &str) {
        let Some(machine) = self.machines.get(machine_id) else {
            return;
        };
        let state = machine.borrow();
        let key = (project_id.to_string(), session_id.to_string());
        if let (true, Some(limit)) = (state.open, state.followers.get(&key).copied()) {
            state.send(&wire::subscribe_message(&key, limit));
        }
    }

    /// Stops following a conversation. The socket closes with its last follower.
    pub fn unfollow(&mut self, machine_id: &str, project_id: &str, session_id: &str) {
        let Some(machine) = self.machines.get(machine_id) else {
            return;
        };
        let mut state = machine.borrow_mut();
        let key = (project_id.to_string(), session_id.to_string());
        if state.followers.remove(&key).is_none() {
            return;
        }
        if state.open {
            state.send(&wire::unsubscribe_message(&key));
        }
        if state.followers.is_empty() {
            state.disconnect();
        }
    }

    fn machine(&mut self, machine_id: &str) -> Rc<RefCell<Machine>> {
        let sink = self.sink.clone();
        self.machines
            .entry(machine_id.to_string())
            .or_insert_with(|| {
                Rc::new(RefCell::new(Machine {
                    machine_id: machine_id.to_string(),
                    sink,
                    endpoint: Endpoint::default(),
                    followers: Followers::new(),
                    socket: None,
                    open: false,
                    retry: None,
                    attempts: 0,
                    generation: 0,
                }))
            })
            .clone()
    }
}

impl Drop for ChatStreams {
    fn drop(&mut self) {
        for machine in self.machines.values() {
            machine.borrow_mut().disconnect();
        }
    }
}

struct Machine {
    machine_id: String,
    sink: InboundSink,
    endpoint: Endpoint,
    followers: Followers,
    socket: Option<WebSocket>,
    open: bool,
    /// The pending reconnect's `setTimeout` handle.
    retry: Option<i32>,
    attempts: usize,
    /// Bumped per socket, so a replaced socket's late events are ignored.
    generation: u64,
}

impl Machine {
    fn send(&self, text: &str) {
        if let Some(socket) = &self.socket {
            let _ = socket.send_with_str(text);
        }
    }

    /// Closes the socket and cancels a pending reconnect. The old socket's handlers are detached
    /// first, so its close event cannot schedule a reconnect of its own.
    fn disconnect(&mut self) {
        self.generation += 1;
        self.open = false;
        if let Some(handle) = self.retry.take() {
            if let Some(window) = web_sys::window() {
                window.clear_timeout_with_handle(handle);
            }
        }
        if let Some(socket) = self.socket.take() {
            socket.set_onopen(None);
            socket.set_onmessage(None);
            socket.set_onclose(None);
            socket.set_onerror(None);
            let _ = socket.close();
        }
    }
}

/// Opens the machine's socket when it has a follower and an endpoint.
fn connect(machine: &Rc<RefCell<Machine>>) {
    let mut state = machine.borrow_mut();
    state.retry = None;
    if state.followers.is_empty() || !state.endpoint.usable() || state.socket.is_some() {
        return;
    }
    let Some(url) = wire::events_url(&state.endpoint.base_url) else {
        return;
    };
    let url = format!(
        "{url}&authToken={}",
        js_sys::encode_uri_component(&state.endpoint.auth_token)
    );
    state.generation += 1;
    let generation = state.generation;
    let Ok(socket) = WebSocket::new(&url) else {
        drop(state);
        schedule_retry(machine);
        return;
    };
    let weak = Rc::downgrade(machine);
    // The handlers are leaked on purpose: a socket's handler may be running when the machine
    // replaces the socket, and dropping a closure while it runs is an error in wasm-bindgen. A
    // replaced socket's handlers are detached in `disconnect` and ignore a stale generation.
    let on_open = Closure::<dyn FnMut()>::new({
        let weak = weak.clone();
        move || {
            let Some(machine) = current(&weak, generation) else {
                return;
            };
            let mut state = machine.borrow_mut();
            state.open = true;
            state.attempts = 0;
            let messages: Vec<String> = state
                .followers
                .iter()
                .map(|(key, limit)| wire::subscribe_message(key, *limit))
                .collect();
            for message in messages {
                state.send(&message);
            }
        }
    });
    socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    on_open.forget();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new({
        let weak = weak.clone();
        move |event: MessageEvent| {
            let Some(machine) = current(&weak, generation) else {
                return;
            };
            let Some(text) = event.data().as_string() else {
                return;
            };
            // The borrow ends before the sink runs: the host may follow or unfollow from it.
            let (inbound, sink) = {
                let state = machine.borrow();
                (
                    wire::route(&state.machine_id, &text, &state.followers),
                    state.sink.clone(),
                )
            };
            if let Some(inbound) = inbound {
                sink(inbound);
            }
        }
    });
    socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    let on_close = Closure::<dyn FnMut()>::new({
        let weak = weak.clone();
        move || {
            let Some(machine) = current(&weak, generation) else {
                return;
            };
            {
                let mut state = machine.borrow_mut();
                state.socket = None;
                state.open = false;
            }
            schedule_retry(&machine);
        }
    });
    socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    on_close.forget();
    let on_error = Closure::<dyn FnMut()>::new(move || {
        if let Some(machine) = current(&weak, generation) {
            if let Some(socket) = &machine.borrow().socket {
                let _ = socket.close();
            }
        }
    });
    socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    on_error.forget();
    state.socket = Some(socket);
}

/// The machine, if `generation` is still its socket's.
fn current(weak: &Weak<RefCell<Machine>>, generation: u64) -> Option<Rc<RefCell<Machine>>> {
    let machine = weak.upgrade()?;
    let live = machine.borrow().generation == generation;
    live.then_some(machine)
}

/// Waits out the ladder step, then connects again if anyone is still followed.
fn schedule_retry(machine: &Rc<RefCell<Machine>>) {
    let mut state = machine.borrow_mut();
    if state.followers.is_empty() || state.retry.is_some() {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    let delay = wire::reconnect_delay_ms(state.attempts);
    state.attempts = state.attempts.saturating_add(1);
    let weak = Rc::downgrade(machine);
    // `once_into_js` frees itself after its one call, so the timer never outlives what it owns.
    let callback = Closure::once_into_js(move || {
        if let Some(machine) = weak.upgrade() {
            connect(&machine);
        }
    });
    state.retry = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.unchecked_ref(),
            delay as i32,
        )
        .ok();
}
