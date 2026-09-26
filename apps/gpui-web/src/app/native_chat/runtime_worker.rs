//! Browser hosts for gx-chat-core and the optional QuickJS parity comparison bundle.
//!
//! CDXC:WebGpui 2026-09-22 WHY: the bundle replaces `globalThis.setTimeout` and friends with virtual timers it services from `tick`, which is right inside QuickJS and would break the page (GPUI's web dispatcher is built on the real `setTimeout`). An iframe gives every chat its own global object, and calls into it stay synchronous, which the composer's per-paint `query` needs.
#[path = "rust_runtime.rs"]
mod rust_runtime;
use serde_json::Value;
use std::time::Duration;
use wasm_bindgen::prelude::*;

pub(crate) enum ChatRuntimeOutput {
    Drained(Value),
    Error(String),
}

#[wasm_bindgen(inline_js = r#"
const runtimes = new Map();
let nextId = 1;

function pump(runtime) {
  clearTimeout(runtime.timer);
  const chat = runtime.frame.contentWindow.nativeChat;
  try {
    chat.tick();
    const text = chat.take(runtime.revision);
    const output = JSON.parse(text);
    if (typeof output.revision === 'number') runtime.revision = output.revision;
    if (typeof output.nextWakeMs === 'number') {
      runtime.timer = setTimeout(() => pump(runtime), Math.max(1, output.nextWakeMs));
    }
    const carriesChange =
      (output.itemsSplice && typeof output.itemsSplice === 'object') ||
      (output.snapshot && typeof output.snapshot === 'object') ||
      (output.rowDetails && typeof output.rowDetails === 'object') ||
      (Array.isArray(output.requests) && output.requests.length > 0);
    if (carriesChange) {
      if (globalThis.ghostexChatDebug) console.log('chat output', Object.keys(output).filter((k) => output[k] != null).join(','), (output.requests ?? []).map((r) => r.kind + ':' + r.method).join(' '));
      runtime.outputs.push(text);
      runtime.wake();
    }
  } catch (error) {
    runtime.outputs.push(JSON.stringify({ __error: String(error?.stack ?? error) }));
    runtime.wake();
  }
}

// The desktop runs every pending promise job and only then drains (`ChatRuntime::jobs` before `drain`). A page cannot flush its microtasks on demand, so the drain is a macrotask, which the browser only starts once they have all run. Draining straight after the call was tried and handed the view a snapshot from before the controller's own state updates: it saw `loadingEarlier` still false and asked for the same page in an endless loop.
function pumpSoon(runtime) {
  if (runtime.pumpQueued) return;
  runtime.pumpQueued = true;
  setTimeout(() => {
    runtime.pumpQueued = false;
    if (runtimes.has(runtime.id)) pump(runtime);
  }, 0);
}

export function chat_runtime_start(configJson, wake) {
  const frame = document.createElement('iframe');
  frame.style.display = 'none';
  document.body.appendChild(frame);
  const runtime = { id: nextId++, frame, revision: 0, outputs: [], wake, timer: null };
  runtimes.set(runtime.id, runtime);
  try {
    const scope = frame.contentWindow;
    scope.eval(globalThis.ghostexChatBundle);
    scope.nativeChat.start(scope.JSON.parse(configJson));
    pumpSoon(runtime);
  } catch (error) {
    runtime.outputs.push(JSON.stringify({ __error: String(error?.stack ?? error) }));
    wake();
  }
  return runtime.id;
}

function invoke(id, method, argsJson) {
  const runtime = runtimes.get(id);
  const scope = runtime.frame.contentWindow;
  return scope.nativeChat[method](...scope.JSON.parse(argsJson));
}

export function chat_runtime_call(id, method, argsJson) {
  const runtime = runtimes.get(id);
  if (!runtime) return;
  try {
    invoke(id, method, argsJson);
  } catch (error) {
    runtime.outputs.push(JSON.stringify({ __error: String(error?.stack ?? error) }));
  }
  pumpSoon(runtime);
}

export function chat_runtime_query(id, method, argsJson) {
  if (!runtimes.has(id)) return undefined;
  try {
    const text = JSON.stringify(invoke(id, method, argsJson));
    return text === undefined ? 'null' : text;
  } catch (error) {
    return undefined;
  }
}

export function chat_runtime_take(id) {
  const runtime = runtimes.get(id);
  if (!runtime) return [];
  const outputs = runtime.outputs;
  runtime.outputs = [];
  return outputs;
}

export function chat_runtime_stop(id) {
  const runtime = runtimes.get(id);
  if (!runtime) return;
  clearTimeout(runtime.timer);
  runtime.frame.remove();
  runtimes.delete(id);
}
"#)]
extern "C" {
    fn chat_runtime_start(config_json: &str, wake: &Closure<dyn FnMut()>) -> u32;
    fn chat_runtime_call(id: u32, method: &str, args_json: &str);
    fn chat_runtime_query(id: u32, method: &str, args_json: &str) -> Option<String>;
    fn chat_runtime_take(id: u32) -> Vec<String>;
    fn chat_runtime_stop(id: u32);
}

pub(crate) struct ChatRuntimeWorker {
    id: u32,
    rust: Option<rust_runtime::Runtime>,
    /// Held so the JavaScript side can keep calling it for as long as the runtime lives.
    _wake: Closure<dyn FnMut()>,
}

impl ChatRuntimeWorker {
    pub(crate) fn start(
        config: Value,
        _recording: Option<std::path::PathBuf>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let wake = std::rc::Rc::new(wake);
        let rust = if rust_runtime::rust_chat_enabled() {
            let wake = wake.clone();
            Some(rust_runtime::Runtime::new(config.clone(), move || wake()))
        } else {
            None
        };
        let wake = Closure::<dyn FnMut()>::new(move || wake());
        let id = if rust.is_none() {
            chat_runtime_start(&config.to_string(), &wake)
        } else {
            0
        };
        Self {
            id,
            rust,
            _wake: wake,
        }
    }

    pub(crate) fn call(&self, method: &'static str, arguments: Vec<Value>) {
        if let Some(rust) = &self.rust {
            rust.call(method, arguments);
            return;
        }
        chat_runtime_call(self.id, method, &Value::Array(arguments).to_string());
    }

    pub(crate) fn call_raw(&self, method: &'static str, raw: String) {
        if let Some(rust) = &self.rust {
            if let Ok(value) = serde_json::from_str(&raw) {
                rust.call(method, vec![value]);
            }
            return;
        }
        chat_runtime_call(self.id, method, &format!("[{raw}]"));
    }

    pub(crate) fn query(
        &self,
        method: &'static str,
        arguments: Vec<Value>,
        timeout: Duration,
    ) -> Option<Value> {
        self.query_for_gesture(method, arguments, timeout)
    }

    /// The runtime shares the page's thread, so an answer is never pending: the timeout the desktop waits out does not apply.
    pub(crate) fn query_for_gesture(
        &self,
        method: &'static str,
        arguments: Vec<Value>,
        _timeout: Duration,
    ) -> Option<Value> {
        if let Some(rust) = &self.rust {
            return rust.query(method, &arguments);
        }
        let text = chat_runtime_query(self.id, method, &Value::Array(arguments).to_string())?;
        serde_json::from_str(&text).ok()
    }

    pub(crate) fn take_outputs(&self) -> Vec<ChatRuntimeOutput> {
        if let Some(rust) = &self.rust {
            return rust
                .take()
                .into_iter()
                .map(ChatRuntimeOutput::Drained)
                .collect();
        }
        chat_runtime_take(self.id)
            .into_iter()
            .map(|text| match serde_json::from_str::<Value>(&text) {
                Ok(value) => match value["__error"].as_str() {
                    Some(error) => ChatRuntimeOutput::Error(error.to_string()),
                    None => ChatRuntimeOutput::Drained(value),
                },
                Err(error) => ChatRuntimeOutput::Error(error.to_string()),
            })
            .collect()
    }
}

impl Drop for ChatRuntimeWorker {
    fn drop(&mut self) {
        chat_runtime_stop(self.id);
    }
}
