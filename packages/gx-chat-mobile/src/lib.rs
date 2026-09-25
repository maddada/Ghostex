//! UniFFI binding of the Ghostex chat core for the mobile app.
//!
//! The whole surface is [`MobileChatCore`] plus two free functions, with JSON strings at the boundary
//! (shapes: `event_json.rs`, `context_json.rs`, `effect_json.rs`, and the doc comment in
//! `apps/mobile/app/modules/gx-chat-core/src/index.ts`). Nothing here decides anything about chat;
//! it parses, calls [`ChatCore`], and serializes.
//!
//! Every exported call catches a panic and answers `{"error": {"code": "panic", ...}}` rather than
//! unwinding into Swift or Kotlin. A core that panicked stays refused afterwards, because its state
//! may be half-applied; the desktop host disables a panicked chat the same way.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

use ghostex_gx_chat_core::query::{answer_query, Query};
use ghostex_gx_chat_core::session::persistence::storage_key;
use ghostex_gx_chat_core::ChatCore;
use serde_json::{json, Value};

mod context_json;
mod effect_json;
mod event_json;

uniffi::setup_scaffolding!();

/// One chat session's brain.
#[derive(uniffi::Object)]
pub struct MobileChatCore {
    inner: Mutex<Inner>,
}

struct Inner {
    core: ChatCore,
    /// Set when a call panicked; every later call is refused.
    panicked: bool,
}

#[uniffi::export]
impl MobileChatCore {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                core: ChatCore::new(),
                panicked: false,
            }),
        }
    }

    /// Applies one event; returns the effects as a JSON array, or a JSON error.
    pub fn handle(&self, event_json: String, context_json: String) -> String {
        self.run(|core| {
            let event = event_json::parse(&event_json).map_err(|m| error("badEvent", &m))?;
            let context =
                context_json::parse(&context_json).map_err(|m| error("badContext", &m))?;
            Ok(effect_json::encode_all(core.handle(event, context)))
        })
    }

    /// The drained frame envelope as JSON, with `nextWakeMs` measured at `now_ms`.
    ///
    /// A negative `last_revision` means the host holds no document, so the snapshot ships.
    pub fn frame_at(&self, last_revision: i64, now_ms: f64) -> String {
        self.run(|core| {
            let last = u64::try_from(last_revision).unwrap_or(u64::MAX);
            serde_json::to_string(&core.frame_at(last, now_ms))
                .map_err(|e| error("serialize", &e.to_string()))
        })
    }

    /// Milliseconds until the earliest armed deadline, or `None` (also after a panic).
    pub fn next_wake_ms(&self) -> Option<u64> {
        self.read(|core| core.next_wake_ms()).flatten()
    }

    /// The current publish revision (0 after a panic).
    pub fn revision(&self) -> u64 {
        self.read(|core| core.revision()).unwrap_or_default()
    }

    /// Makes the next frame ship every channel whole. Returns `{}` or a JSON error.
    pub fn forget_sent(&self) -> String {
        self.run(|core| {
            core.forget_sent();
            Ok("{}".to_string())
        })
    }

    /// A fresh id from the core's request counter (0 after a panic, which the core never hands out).
    pub fn allocate_request_id(&self) -> u64 {
        self.read(|core| core.allocate_request_id())
            .unwrap_or_default()
    }

    /// One of the five pure helpers (`composerReferences`, `composerKeyIntent`, `referenceMenu`,
    /// `transcriptMenu`, `sendBlockedToast`), answered from the current state.
    pub fn query(&self, name: String, arguments_json: String) -> String {
        self.run(|core| {
            let query = Query::from_wire(&name)
                .ok_or_else(|| error("unknownQuery", &format!("no helper named {name:?}")))?;
            let arguments: Vec<Value> = serde_json::from_str(&arguments_json)
                .map_err(|e| error("badArguments", &e.to_string()))?;
            answer_query(core.state(), core.context(), query, &arguments)
                .ok_or_else(|| error("badArguments", &format!("{name} refused these arguments")))
        })
    }
}

impl Default for MobileChatCore {
    fn default() -> Self {
        Self::new()
    }
}

impl MobileChatCore {
    /// Runs a string-answering call under the panic guard.
    fn run(&self, call: impl FnOnce(&mut ChatCore) -> Result<String, String>) -> String {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if inner.panicked {
            return error("panic", "This chat core panicked earlier and was disabled.");
        }
        let outcome = catch_unwind(AssertUnwindSafe(|| call(&mut inner.core)));
        match outcome {
            Ok(Ok(json)) | Ok(Err(json)) => json,
            Err(payload) => {
                inner.panicked = true;
                error("panic", &panic_message(payload.as_ref()))
            }
        }
    }

    /// Runs a value-answering call under the same guard; `None` when refused or panicked.
    fn read<T>(&self, call: impl FnOnce(&mut ChatCore) -> T) -> Option<T> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if inner.panicked {
            return None;
        }
        match catch_unwind(AssertUnwindSafe(|| call(&mut inner.core))) {
            Ok(value) => Some(value),
            Err(_) => {
                inner.panicked = true;
                None
            }
        }
    }
}

/// `JSON.stringify([machineId, projectId, sessionId])`, the retained-transcript record key the
/// host passes back as `start`'s `config.retainedKey`.
#[uniffi::export]
pub fn retained_snapshot_key(machine_id: String, project_id: String, session_id: String) -> String {
    storage_key(&machine_id, &project_id, &session_id)
}

/// This binding's version, for diagnostics.
#[uniffi::export]
pub fn core_version() -> String {
    concat!("gx-chat-mobile ", env!("CARGO_PKG_VERSION")).to_string()
}

fn error(code: &str, message: &str) -> String {
    json!({ "error": { "code": code, "message": message } }).to_string()
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "The chat core panicked.".to_string())
}
