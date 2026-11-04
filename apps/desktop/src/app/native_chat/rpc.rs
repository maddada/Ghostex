use crate::app::{helpers::*, model::*};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::Duration;

/// Async so the browser build (apps/gpui-web), which has no blocking sockets, can share both call sites; here the body still blocks, on the background thread every caller already runs it on.
pub(super) async fn request(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    endpoint: &str,
    params: &Value,
) -> Result<Value, Value> {
    let response = match shared_read_key(remote.as_ref(), endpoint, params) {
        Some(key) => shared_read(key, || transport(remote, endpoint, params)),
        None => transport(remote, endpoint, params),
    }
    .map_err(|message| json!({"message":message,"endpoint":endpoint}))?;
    let (status, body) = response;
    let mut envelope: Value = serde_json::from_str(&body)
        .map_err(|_| json!({"message":"gxserver returned invalid JSON.","endpoint":endpoint}))?;
    if envelope["ok"] == false {
        return Err(
            json!({"code":envelope["error"],"message":envelope["message"],"endpoint":endpoint}),
        );
    }
    if !(200..300).contains(&status) || envelope["ok"] != true {
        return Err(
            json!({"message":format!("gxserver request failed with HTTP {status}."),"endpoint":endpoint}),
        );
    }
    Ok(envelope["result"].take())
}

/// CDXC:SessionChat 2026-10-31 WHY:
/// gxserver answers a send once it has typed it, and a send to a mid-turn agent waits up to five minutes for each of its three attempts to show on screen (session_chat_send/busy_input_wait.rs). This call used to give up after 60 s of silence: the message left the thread and went back to the chat box with a socket error while gxserver went on typing it, so the agent could still take a message the user then sent again, and the real answer minutes later had nobody listening (GitHub #374). The wait is the send's; only a daemon that has stopped answering should end it.
const SEND_RPC_TIMEOUT: Duration = Duration::from_secs(16 * 60);

fn request_timeout(endpoint: &str) -> Duration {
    if endpoint == "/api/sendSessionChatMessage" {
        SEND_RPC_TIMEOUT
    } else {
        Duration::from_secs(60)
    }
}

/// CDXC:SessionChat 2026-11-05 WHY: how long a chat call rides out a gxserver restart (a closed
/// port, or a connection it dropped while stopping) before its error shows. 5 s covered an
/// in-place `start-server`, but a rebuild's restart and gxserver's own startup gate take longer,
/// and every miss put a socket error on the chat; past this the chat says it couldn't reach
/// Ghostex and offers Retry.
const RESTART_WAIT: Duration = Duration::from_secs(20);

fn transport(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    endpoint: &str,
    params: &Value,
) -> Result<(u16, String), String> {
    let timeout = request_timeout(endpoint);
    match remote {
        Some(target) => {
            gpui_remote_gxserver_post_typed_operation(&target, endpoint, params, timeout)
        }
        None => {
            gxserver_post_typed_operation_across_restart(endpoint, params, timeout, RESTART_WAIT)
        }
    }
}

/// CDXC:SessionChat 2026-10-31 WHY:
/// The chat core gives a read 30 s before it settles it as unanswered and asks again, but the
/// request underneath keeps going until its socket has been silent for 60 s. Over a slow link to
/// a remote machine (GitHub #361) a transcript window can take longer than 30 s to arrive, so the
/// retry started a second download of the same window beside the first, then a third, each one
/// slowing the others: measured through a 1 Mbps, ~1 s, 8% loss link, three chats opening at once
/// never loaded that way. A remote read asked for again with the same parameters while the first
/// is still on the wire now waits for that one and gets its answer. Only remote `readSessionChat`
/// is shared, and only while it is in flight: a read asked for after one finished always goes out
/// fresh, so a resync never gets an answer older than its request needed.
fn shared_read_key(
    remote: Option<&GpuiRemoteGxserverRequestTarget>,
    endpoint: &str,
    params: &Value,
) -> Option<String> {
    let remote = remote?;
    (endpoint == "/api/readSessionChat")
        .then(|| format!("{}|{endpoint}|{params}", remote.local_port))
}

type TransportResult = Result<(u16, String), String>;

#[derive(Default)]
struct SharedRead {
    result: Mutex<Option<TransportResult>>,
    done: Condvar,
}

static SHARED_READS: LazyLock<Mutex<HashMap<String, Arc<SharedRead>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Runs `run` for `key` unless the same request is already in flight, in which case it waits for
/// that one's answer.
fn shared_read(key: String, run: impl FnOnce() -> TransportResult) -> TransportResult {
    let (shared, leader) = {
        let mut reads = SHARED_READS
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match reads.get(&key) {
            Some(shared) => (shared.clone(), false),
            None => {
                let shared = Arc::new(SharedRead::default());
                reads.insert(key.clone(), shared.clone());
                (shared, true)
            }
        }
    };
    if leader {
        let result = run();
        SHARED_READS
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&key);
        *shared
            .result
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(result.clone());
        shared.done.notify_all();
        return result;
    }
    let mut result = shared
        .result
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    while result.is_none() {
        result = shared
            .done
            .wait(result)
            .unwrap_or_else(|poison| poison.into_inner());
    }
    result
        .clone()
        .unwrap_or_else(|| Err("The gxserver request was dropped.".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn a_read_asked_for_again_while_on_the_wire_shares_the_first_answer() {
        let runs = Arc::new(AtomicUsize::new(0));
        let key = "test-share|/api/readSessionChat|{}".to_string();
        let first = {
            let (runs, key) = (runs.clone(), key.clone());
            thread::spawn(move || {
                shared_read(key, || {
                    runs.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(300));
                    Ok((200, "first".to_string()))
                })
            })
        };
        thread::sleep(Duration::from_millis(100));
        let again = shared_read(key.clone(), || {
            runs.fetch_add(1, Ordering::SeqCst);
            Ok((200, "second".to_string()))
        });
        assert_eq!(again, Ok((200, "first".to_string())));
        assert_eq!(first.join().unwrap(), Ok((200, "first".to_string())));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        // Once it finished, the next read goes out fresh.
        let later = shared_read(key, || {
            runs.fetch_add(1, Ordering::SeqCst);
            Ok((200, "fresh".to_string()))
        });
        assert_eq!(later, Ok((200, "fresh".to_string())));
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn only_remote_transcript_reads_are_shared() {
        let target = GpuiRemoteGxserverRequestTarget {
            capabilities: GpuiRemoteGxserverCapabilities::default(),
            code_server_component_platform: None,
            execution_target: GpuiRemoteExecutionTarget::PosixHost,
            local_port: 43_210,
            token: "t".into(),
        };
        let params = json!({"projectId":"p","sessionId":"s","limit":300});
        assert!(shared_read_key(Some(&target), "/api/readSessionChat", &params).is_some());
        assert!(shared_read_key(None, "/api/readSessionChat", &params).is_none());
        assert!(shared_read_key(Some(&target), "/api/sendSessionChatMessage", &params).is_none());
        // A page of older history asks with other parameters, so it never waits on the window.
        let page = json!({"projectId":"p","sessionId":"s","limit":200,"beforeOffset":9});
        assert_ne!(
            shared_read_key(Some(&target), "/api/readSessionChat", &params),
            shared_read_key(Some(&target), "/api/readSessionChat", &page)
        );
    }
}
