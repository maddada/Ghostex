//! Browser host for the shared Rust chat core. Only timers and storage are browser-specific.
use ghostex_gx_chat_core::{ChatContext, ChatCore, Effect, Event, StorageKey};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};
use wasm_bindgen::prelude::*;

#[path = "../../../../desktop/src/app/gx_chat/effects.rs"]
mod effects;
#[path = "../../../../desktop/src/app/gx_chat/events.rs"]
mod events;
#[path = "../../../../desktop/src/app/gx_chat/frame.rs"]
mod frame;
#[path = "../../../../desktop/src/app/gx_chat/identity.rs"]
mod identity;
#[path = "../../../../desktop/src/app/gx_chat/queries.rs"]
mod queries;
#[path = "../../../../desktop/src/app/gx_chat/transfers.rs"]
mod transfers;

#[wasm_bindgen(inline_js = r#"
export function rust_chat_io(operation, input) { return globalThis.ghostexRustChatStorage(operation, input); }
export function rust_chat_enabled() { return new URLSearchParams(location.search).get('chatBrain') !== 'quickjs'; }
export function rust_chat_clock() { return -new Date().getTimezoneOffset(); }
export function rust_chat_random() { return Array.from(crypto.getRandomValues(new Uint32Array(8))); }
export function rust_chat_interval(callback) { return setInterval(callback, 50); }
export function rust_chat_clear(id) { clearInterval(id); }
export function rust_chat_trace(message) { if (new URLSearchParams(location.search).has('chatDebug')) console.debug('rust-chat', message); }
export function rust_chat_draft_request(session, request) { globalThis.ghostexRustChatDraftRequest(session, request); }
export function rust_chat_draft_settled(session, result) { globalThis.ghostexRustChatDraftSettled(session, result); }
"#)]
extern "C" {
    fn rust_chat_io(operation: &str, input: &str) -> js_sys::Promise;
    pub(super) fn rust_chat_enabled() -> bool;
    fn rust_chat_clock() -> i32;
    fn rust_chat_random() -> Vec<u32>;
    fn rust_chat_interval(callback: &Closure<dyn FnMut()>) -> i32;
    fn rust_chat_clear(id: i32);
    fn rust_chat_trace(message: &str);
    fn rust_chat_draft_request(session: &str, request: &str);
    fn rust_chat_draft_settled(session: &str, result: &str);
}

struct Inner {
    core: ChatCore,
    revision: u64,
    outputs: Vec<Value>,
    pending: VecDeque<Effect>,
    pumping: bool,
    session_key: String,
    retained_key: String,
    transfers: BTreeMap<String, transfers::Transfer>,
    due: Option<f64>,
    draft_result: Option<Value>,
    wake: Rc<dyn Fn()>,
}

pub(super) struct Runtime {
    inner: Rc<RefCell<Inner>>,
    interval: i32,
    _timer: Closure<dyn FnMut()>,
}

fn context() -> ChatContext {
    let random = rust_chat_random();
    let id = |start: usize| {
        random[start..start + 4]
            .iter()
            .fold(0u128, |id, part| (id << 32) | u128::from(*part))
    };
    ChatContext {
        now_ms: js_sys::Date::now(),
        utc_offset_minutes: rust_chat_clock(),
        random_units: [js_sys::Math::random(), js_sys::Math::random()],
        random_ids: [id(0), id(4)],
        ..Default::default()
    }
}

impl Runtime {
    pub(super) fn new(mut config: Value, wake: impl Fn() + 'static) -> Self {
        let identity = identity::ChatIdentity::from_config(&config);
        let session_key = identity.storage_session_key();
        let retained_key = identity.retention_key();
        config["retainedKey"] = json!(retained_key);
        let inner = Rc::new(RefCell::new(Inner {
            core: ChatCore::new(),
            revision: 0,
            outputs: Vec::new(),
            pending: VecDeque::new(),
            pumping: false,
            session_key,
            retained_key,
            transfers: BTreeMap::new(),
            due: None,
            draft_result: None,
            wake: Rc::new(wake),
        }));
        let weak = Rc::downgrade(&inner);
        let timer = Closure::<dyn FnMut()>::new(move || {
            if let Some(inner) = weak.upgrade() {
                let expired = !transfers::expire(&mut inner.borrow_mut().transfers).is_empty();
                if expired {
                    dispatch(&inner, vec![events::transfer_failed()]);
                }
                let due = inner
                    .borrow()
                    .due
                    .is_some_and(|due| js_sys::Date::now() >= due);
                if due {
                    dispatch(&inner, vec![Event::Tick]);
                }
            }
        });
        let interval = rust_chat_interval(&timer);
        let runtime = Self {
            inner,
            interval,
            _timer: timer,
        };
        runtime.call("start", vec![config]);
        runtime
    }
    pub(super) fn call(&self, method: &str, mut args: Vec<Value>) {
        if method == "resolve" {
            rust_chat_draft_settled(&self.inner.borrow().session_key, &json!(args).to_string());
        }
        if method == "brokerMessage" && transfers::is_piece(args.first()) {
            let key = self.inner.borrow().session_key.clone();
            let piece =
                transfers::accept(&mut self.inner.borrow_mut().transfers, &key, args.first());
            match piece {
                transfers::Piece::Pending => return,
                transfers::Piece::Failed => {
                    dispatch(&self.inner, vec![events::transfer_failed()]);
                    return;
                }
                transfers::Piece::Complete(value) => args = vec![value],
            }
        }
        let events = if method == "resolve" {
            events::resolved(&args).into_iter().collect()
        } else {
            events::events_for(method, &args)
        };
        let action = args
            .first()
            .and_then(|value| value.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        rust_chat_trace(&format!("{method} {action}: {} events", events.len()));
        dispatch(&self.inner, events);
    }
    pub(super) fn query(&self, method: &str, args: &[Value]) -> Option<Value> {
        queries::answer(self.inner.borrow().core.state(), context(), method, args)
    }
    pub(super) fn take(&self) -> Vec<Value> {
        std::mem::take(&mut self.inner.borrow_mut().outputs)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        rust_chat_clear(self.interval);
    }
}

fn dispatch(inner: &Rc<RefCell<Inner>>, events: Vec<Event>) {
    let mut state = inner.borrow_mut();
    let mut events: VecDeque<_> = events.into();
    let mut requests = Vec::new();
    while let Some(event) = events.pop_front() {
        if let Event::ComposerBootRead(read) = &event {
            requests.push(ghostex_gx_chat_core::HostRequest {
                id: None,
                kind: ghostex_gx_chat_core::RequestKind::ComposerInit,
                method: "restore".into(),
                params: serde_json::to_value(read)
                    .ok()
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_default(),
            });
        }
        for effect in state.core.handle(event, context()) {
            if matches!(effect, Effect::RecordDeliveries { .. }) {
                state.pending.push_back(effect);
                continue;
            }
            match effects::route(effect) {
                effects::Routed::Renderer(mut request) => {
                    if request.kind == ghostex_gx_chat_core::RequestKind::Rpc {
                        rust_chat_draft_request(
                            &state.session_key,
                            &serde_json::to_string(&request).unwrap(),
                        );
                    }
                    if request.kind == ghostex_gx_chat_core::RequestKind::DraftSubmitted {
                        if let Some(Value::Object(result)) = state.draft_result.take() {
                            request.params.extend(result);
                        }
                    }
                    requests.push(*request);
                }
                effects::Routed::SelfAction(action) => events.push_back(Event::Action(action)),
                effects::Routed::Host(Effect::SetTimer { .. }) => {}
                effects::Routed::Host(effect) => state.pending.push_back(effect),
                effects::Routed::Swallowed(_) => {}
            }
        }
    }
    let revision = state.revision;
    let output = frame::envelope(state.core.frame(revision), requests);
    state.revision = state.core.revision();
    state.due = state
        .core
        .next_wake_ms()
        .map(|ms| js_sys::Date::now() + ms as f64);
    let changed = output
        .as_object()
        .is_some_and(frame::envelope_carries_change);
    if changed {
        state.outputs.push(output);
    }
    let wake = state.wake.clone();
    let start = !state.pumping && !state.pending.is_empty();
    if start {
        state.pumping = true;
    }
    drop(state);
    if changed {
        wake();
    }
    if start {
        let weak = Rc::downgrade(inner);
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                let effect = inner.borrow_mut().pending.pop_front();
                let Some(effect) = effect else {
                    inner.borrow_mut().pumping = false;
                    break;
                };
                let (session, retained) = {
                    let s = inner.borrow();
                    (s.session_key.clone(), s.retained_key.clone())
                };
                let event = perform(effect, &session, &retained, &inner).await;
                if let (Some(inner), Some(event)) = (weak.upgrade(), event) {
                    dispatch(&inner, vec![event]);
                }
            }
        });
    }
}

async fn io(operation: &str, input: Value) -> Result<Value, String> {
    rust_chat_trace(&format!("storage {operation} start"));
    let value = wasm_bindgen_futures::JsFuture::from(rust_chat_io(operation, &input.to_string()))
        .await
        .map_err(|error| format!("Chat storage: {error:?}"))?;
    rust_chat_trace(&format!("storage {operation} done"));
    serde_json::from_str(&value.as_string().unwrap_or_default()).map_err(|error| error.to_string())
}

async fn perform(
    effect: Effect,
    session: &str,
    retained: &str,
    inner: &Rc<RefCell<Inner>>,
) -> Option<Event> {
    Some(match effect {
        Effect::ReadComposerBoot { .. } => match io("boot", json!({"sessionKey":session}))
            .await
            .and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
        {
            Ok(read) => Event::ComposerBootRead(Box::new(read)),
            Err(error) => Event::ComposerBootFailed { error },
        },
        Effect::ReadStorage { key } => {
            let value = io("read", json!({"key":key}))
                .await
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned));
            Event::StorageLoaded { key, value }
        }
        Effect::ReadStorageBatch { keys } => {
            let records = io("readBatch", json!({"keys":keys}))
                .await
                .ok()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            Event::StorageBatchLoaded { records }
        }
        Effect::WriteStorage {
            key,
            value,
            durable,
        } => {
            let result = io(
                "write",
                json!({"sessionKey":session,"key":key,"value":value,"durable":durable}),
            )
            .await;
            if matches!(key.store.as_str(), "draftSubmitted" | "draftPark") {
                inner.borrow_mut().draft_result = result.as_ref().ok().cloned();
            }
            Event::StorageWritten {
                key,
                error: result.err(),
            }
        }
        Effect::WriteStorageBatch { writes } => {
            let keys = writes.iter().map(|w| w.key.clone()).collect();
            let error = io("writeBatch", json!({"writes":writes})).await.err();
            Event::StorageBatchWritten { keys, error }
        }
        Effect::FlushStorage { store } => {
            let error = io("flush", json!({"sessionKey":session})).await.err();
            Event::StorageWritten {
                key: StorageKey {
                    store,
                    suffix: String::new(),
                },
                error,
            }
        }
        Effect::ReadRetainedSnapshot => {
            let value = io("readSnapshot", json!({"key":retained}))
                .await
                .ok()
                .filter(|v| !v.is_null())
                .map(|v| v.to_string());
            Event::RetainedSnapshotLoaded { value }
        }
        // The broker's retained store owns persistence while it owns the subscription.
        Effect::WriteRetainedSnapshot { .. } => return None,
        Effect::RecordDeliveries { deliveries } => {
            let _ = io(
                "deliveries",
                json!({"sessionKey":session,"deliveries":deliveries}),
            )
            .await;
            return None;
        }
        _ => return None,
    })
}
