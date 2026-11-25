//! Stdio JSON-RPC transport, child process container, reader exit guard, and activity reporter.

use std::collections::HashMap;
use std::io::Write;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use crate::acp::host::{GxserverEndpoint, SpoolWriter};

use super::state::GrokState;

pub(super) struct GrokTransport {
    pub(super) stdin: Mutex<Option<ChildStdin>>,
    pub(super) next_id: AtomicU64,
    pub(super) pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    pub(super) spool: Mutex<SpoolWriter>,
}

impl GrokTransport {
    pub(super) fn new(stdin: ChildStdin, session_id: &str) -> Self {
        Self {
            stdin: Mutex::new(Some(stdin)),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            spool: Mutex::new(SpoolWriter::open(session_id)),
        }
    }

    pub(super) fn write_json(&self, message: &Value) -> Result<(), String> {
        let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        let Some(stdin) = guard.as_mut() else {
            return Err("Grok Native host stdin is closed.".to_string());
        };
        let encoded = serde_json::to_string(message).map_err(|e| e.to_string())?;
        stdin
            .write_all(encoded.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Failed writing to Grok Native host: {e}"))
    }

    pub(super) fn close_stdin(&self) {
        let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
        *guard = None;
    }

    pub(super) fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    pub(super) fn start_call(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(u64, mpsc::Receiver<Value>), String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if let Err(err) = self.write_json(&msg) {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            return Err(err);
        }
        Ok((id, rx))
    }

    pub(super) fn finish_call(
        &self,
        id: u64,
        method: &str,
        rx: mpsc::Receiver<Value>,
        timeout: Duration,
    ) -> Result<Value, String> {
        match rx.recv_timeout(timeout) {
            Ok(resp) => {
                if let Some(error) = resp.get("error") {
                    Err(crate::acp::host::extract_jsonrpc_error_message(
                        error,
                        "Grok Native request failed",
                    ))
                } else {
                    Ok(resp.get("result").cloned().unwrap_or(Value::Null))
                }
            }
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                Err(format!("Grok Native request `{method}` timed out."))
            }
        }
    }

    pub(super) fn call(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let (id, rx) = self.start_call(method, params)?;
        self.finish_call(id, method, rx, timeout)
    }

    pub(super) fn respond_to_agent(&self, rpc_id: Value, result: Value) -> Result<(), String> {
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "result": result,
        }))
    }

    pub(super) fn respond_error_to_agent(&self, rpc_id: Value, code: i64, message: &str) {
        let _ = self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "error": { "code": code, "message": message },
        }));
    }

    pub(super) fn spool_event(&self, event: &Value) {
        if let Ok(mut spool) = self.spool.lock() {
            spool.append(event);
        }
    }
}

pub(super) struct GrokProcess {
    pub(super) child: Child,
    pub(super) child_lifetime: crate::acp::child_lifetime::ChildLifetime,
    pub(super) transport: Arc<GrokTransport>,
}

pub(super) struct GrokReaderExitGuard {
    pub(super) state: Arc<Mutex<GrokState>>,
    pub(super) intentional_stop: Arc<AtomicBool>,
}

impl Drop for GrokReaderExitGuard {
    fn drop(&mut self) {
        if !self.intentional_stop.load(Ordering::Acquire) {
            let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
            s.child_exited = true;
            s.working = false;
            s.abort_all_prompt_waiters();
        }
    }
}

#[derive(Clone)]
pub(super) struct GrokActivityReporter {
    pub(super) tx: Option<mpsc::Sender<Option<String>>>,
}

impl GrokActivityReporter {
    pub(super) fn start(
        endpoint: Option<GxserverEndpoint>,
        state: Arc<Mutex<GrokState>>,
        family: String,
    ) -> Self {
        let Some(ep) = endpoint else {
            return Self { tx: None };
        };
        let (tx, rx) = mpsc::channel::<Option<String>>();
        thread::spawn(move || {
            let mut pending_next: Option<Option<String>> = None;
            loop {
                let mut stop_error = match pending_next.take() {
                    Some(v) => v,
                    None => match rx.recv() {
                        Ok(v) => v,
                        Err(_) => return,
                    },
                };
                while let Ok(next_err) = rx.try_recv() {
                    if next_err.is_some() {
                        stop_error = next_err;
                    }
                }
                let (act, sid, perm, interrupted) = {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let front = s
                        .pending_requests
                        .front()
                        .map(|r| r.to_pending_permission());
                    let act = if front.is_some() {
                        "attention"
                    } else if s.working {
                        "working"
                    } else {
                        "idle"
                    };
                    (
                        act,
                        s.native_session_id.clone(),
                        front,
                        s.last_turn_interrupted,
                    )
                };
                let mut ok = ep.report_activity_with_error(
                    &family,
                    act,
                    sid.as_deref(),
                    None,
                    perm.as_ref(),
                    interrupted,
                    stop_error.as_deref(),
                );
                for delay_ms in [500u64, 1_000, 2_000] {
                    if ok {
                        break;
                    }
                    match rx.recv_timeout(Duration::from_millis(delay_ms)) {
                        Ok(next_err) => {
                            pending_next = Some(next_err.or(stop_error));
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let (act, sid, perm, interrupted) = {
                                let s = state.lock().unwrap_or_else(|p| p.into_inner());
                                let front = s
                                    .pending_requests
                                    .front()
                                    .map(|r| r.to_pending_permission());
                                let act = if front.is_some() {
                                    "attention"
                                } else if s.working {
                                    "working"
                                } else {
                                    "idle"
                                };
                                (
                                    act,
                                    s.native_session_id.clone(),
                                    front,
                                    s.last_turn_interrupted,
                                )
                            };
                            ok = ep.report_activity_with_error(
                                &family,
                                act,
                                sid.as_deref(),
                                None,
                                perm.as_ref(),
                                interrupted,
                                stop_error.as_deref(),
                            );
                        }
                    }
                }
            }
        });
        Self { tx: Some(tx) }
    }

    pub(super) fn sync(&self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(None);
        }
    }

    pub(super) fn sync_with_error(&self, code: String) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Some(code));
        }
    }
}
