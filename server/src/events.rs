use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use serde_json::{json, Map, Value};
use tokio::sync::{broadcast, mpsc, oneshot, watch, Mutex};
use uuid::Uuid;

use crate::constants::GXSERVER_PROTOCOL_VERSION;

const EVENT_STREAM_QUEUE_CAPACITY: usize = 256;

#[derive(Clone)]
pub struct EventClientSender {
    sender: mpsc::Sender<Value>,
    overflow_tx: watch::Sender<bool>,
}

pub type EventClientReceiver = mpsc::Receiver<Value>;

impl EventClientSender {
    pub fn try_send(&self, event: Value) -> Result<(), mpsc::error::TrySendError<Value>> {
        match self.sender.try_send(event) {
            Ok(()) => Ok(()),
            Err(error @ mpsc::error::TrySendError::Full(_)) => {
                self.signal_overflow();
                Err(error)
            }
            Err(error @ mpsc::error::TrySendError::Closed(_)) => Err(error),
        }
    }

    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    pub fn signal_overflow(&self) {
        self.overflow_tx.send_replace(true);
    }

    pub async fn wait_for_overflow(&self) {
        let mut overflow_rx = self.overflow_tx.subscribe();
        while !*overflow_rx.borrow_and_update() {
            if overflow_rx.changed().await.is_err() {
                return;
            }
        }
    }
}

#[derive(Clone)]
pub struct GxserverEventHub {
    inner: Arc<EventHubInner>,
}

struct EventHubInner {
    broadcast_tx: broadcast::Sender<Value>,
    pending_renderer_commands: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    renderer_clients: Mutex<VecDeque<RendererClient>>,
    server_id: String,
}

#[derive(Clone)]
struct RendererClient {
    client_id: String,
    sender: EventClientSender,
}

#[derive(Debug)]
pub struct RendererCommandError {
    pub code: &'static str,
    pub message: String,
}

/*
CDXC:GxserverPresentationEvents 2026-06-15-09:55:
Phase 4 Rust WebSockets must move beyond eventStreamReady and own the same server event hub contract as TypeScript: broadcast lifecycle/API/presentation events, let clients subscribe for fresh snapshots, and route renderer-only commands through the authenticated event stream without changing the default product port.

CDXC:GxserverPresentationEvents 2026-06-22-04:30:
TypeScript keeps renderer-capable WebSocket clients in insertion order and dispatches renderer commands to the first open subscriber. Rust must retain multiple subscribers instead of replacing the earlier renderer so native command ownership stays stable across secondary clients and reconnect races.
*/
impl GxserverEventHub {
    pub fn new(server_id: impl Into<String>) -> Self {
        let (broadcast_tx, _) = broadcast::channel(EVENT_STREAM_QUEUE_CAPACITY);
        Self {
            inner: Arc::new(EventHubInner {
                broadcast_tx,
                pending_renderer_commands: Mutex::new(HashMap::new()),
                renderer_clients: Mutex::new(VecDeque::new()),
                server_id: server_id.into(),
            }),
        }
    }

    pub fn broadcast(&self, event: Value) {
        let _ = self.inner.broadcast_tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.inner.broadcast_tx.subscribe()
    }

    pub fn client_channel(&self) -> (EventClientSender, EventClientReceiver) {
        /*
        Match the hub's 256-event broadcast retention with one bounded
        per-client delivery queue. Producers use only try_send: a slow socket
        is disconnected on overflow and must resubscribe for a fresh snapshot
        rather than accumulating memory or dropping/reordering deltas.
        */
        let (sender, receiver) = mpsc::channel(EVENT_STREAM_QUEUE_CAPACITY);
        let (overflow_tx, _) = watch::channel(false);
        (
            EventClientSender {
                sender,
                overflow_tx,
            },
            receiver,
        )
    }

    pub async fn register_renderer_client(
        &self,
        client_id: impl Into<String>,
        sender: EventClientSender,
    ) -> String {
        let client_id = client_id.into();
        self.inner
            .renderer_clients
            .lock()
            .await
            .push_back(RendererClient {
                client_id: client_id.clone(),
                sender,
            });
        client_id
    }

    pub async fn unregister_renderer_client(&self, client_id: &str) {
        self.inner
            .renderer_clients
            .lock()
            .await
            .retain(|client| client.client_id != client_id);
    }

    async fn open_renderer_client(&self) -> Option<RendererClient> {
        let mut clients = self.inner.renderer_clients.lock().await;
        while clients
            .front()
            .map(|client| client.sender.is_closed())
            .unwrap_or(false)
        {
            clients.pop_front();
        }
        clients.front().cloned()
    }

    pub async fn handle_renderer_command_result(&self, message: &Map<String, Value>) {
        let command_id = message
            .get("commandId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(sender) = self
            .inner
            .pending_renderer_commands
            .lock()
            .await
            .remove(command_id)
        else {
            return;
        };
        let result = if message.get("ok").and_then(Value::as_bool) == Some(false) {
            json!({
                "error": message
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("renderer command failed."),
                "ok": false,
            })
        } else {
            message
                .get("result")
                .and_then(Value::as_object)
                .cloned()
                .map(Value::Object)
                .unwrap_or_else(|| json!({ "ok": true }))
        };
        let _ = sender.send(result);
    }

    pub async fn dispatch_renderer_command(
        &self,
        action: String,
        payload: Map<String, Value>,
        timeout_ms: u64,
    ) -> Result<Value, RendererCommandError> {
        let Some(renderer) = self.open_renderer_client().await else {
            return Err(RendererCommandError::dependency_unavailable());
        };
        let command_id = format!("renderer-{}", Uuid::new_v4());
        let command = json!({
            "action": action,
            "commandId": command_id,
            "createdAt": now_iso(),
            "payload": Value::Object(payload),
            "timeoutMs": timeout_ms,
        });
        let event = json!({
            "command": command,
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "serverId": self.inner.server_id,
            "type": "rendererCommand",
        });
        let (result_tx, result_rx) = oneshot::channel();
        self.inner
            .pending_renderer_commands
            .lock()
            .await
            .insert(command_id.clone(), result_tx);
        if renderer.sender.try_send(event).is_err() {
            self.inner
                .pending_renderer_commands
                .lock()
                .await
                .remove(&command_id);
            self.unregister_renderer_client(&renderer.client_id).await;
            return Err(RendererCommandError::dependency_unavailable());
        }
        match tokio::time::timeout(Duration::from_millis(timeout_ms), result_rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => {
                self.inner
                    .pending_renderer_commands
                    .lock()
                    .await
                    .remove(&command_id);
                Err(RendererCommandError::dependency_unavailable())
            }
            Err(_) => {
                self.inner
                    .pending_renderer_commands
                    .lock()
                    .await
                    .remove(&command_id);
                Err(RendererCommandError::timeout(&action, timeout_ms))
            }
        }
    }
}

impl RendererCommandError {
    /*
    CDXC:GxserverUbuntu 2026-06-23-07:52:
    Renderer-only RPC behavior must stay protocol-identical on macOS and Ubuntu. Report the same no-renderer dependency failure without naming macOS so Linux clients do not receive platform-specific error copy.
    */
    fn dependency_unavailable() -> Self {
        Self {
            code: "dependencyUnavailable",
            message: "No renderer is connected to gxserver for renderer-only commands.".to_string(),
        }
    }

    fn timeout(action: &str, timeout_ms: u64) -> Self {
        Self {
            code: "dependencyUnavailable",
            message: format!("Timed out waiting {timeout_ms}ms for renderer command {action}."),
        }
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn renderer_commands_use_first_open_renderer_client() {
        let hub = GxserverEventHub::new("S1a");
        let (first_tx, mut first_rx) = hub.client_channel();
        let (second_tx, mut second_rx) = hub.client_channel();
        hub.register_renderer_client("renderer-first", first_tx)
            .await;
        hub.register_renderer_client("renderer-second", second_tx)
            .await;

        let dispatch = tokio::spawn({
            let hub = hub.clone();
            async move {
                hub.dispatch_renderer_command(
                    "toggleSidebarCollapsed".to_string(),
                    Map::new(),
                    1_000,
                )
                .await
            }
        });

        let event = timeout(Duration::from_millis(250), first_rx.recv())
            .await
            .expect("first renderer received command")
            .expect("first renderer event");
        assert_eq!(event["type"], json!("rendererCommand"));
        assert_eq!(event["command"]["action"], json!("toggleSidebarCollapsed"));
        assert!(matches!(
            second_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));

        let command_id = event["command"]["commandId"]
            .as_str()
            .expect("command id")
            .to_string();
        hub.handle_renderer_command_result(&Map::from_iter([
            ("commandId".to_string(), json!(command_id)),
            ("ok".to_string(), json!(true)),
            (
                "result".to_string(),
                json!({ "handledBy": "first", "ok": true }),
            ),
            ("type".to_string(), json!("rendererCommandResult")),
        ]))
        .await;

        let result = timeout(Duration::from_millis(250), dispatch)
            .await
            .expect("renderer command completed")
            .expect("renderer command task")
            .expect("renderer command result");
        assert_eq!(result, json!({ "handledBy": "first", "ok": true }));
    }

    #[tokio::test]
    async fn renderer_commands_skip_closed_first_renderer_client() {
        let hub = GxserverEventHub::new("S1a");
        let (closed_tx, closed_rx) = hub.client_channel();
        hub.register_renderer_client("renderer-closed", closed_tx)
            .await;
        drop(closed_rx);

        let (open_tx, mut open_rx) = hub.client_channel();
        hub.register_renderer_client("renderer-open", open_tx).await;

        let dispatch = tokio::spawn({
            let hub = hub.clone();
            async move {
                hub.dispatch_renderer_command(
                    "toggleSidebarCollapsed".to_string(),
                    Map::new(),
                    1_000,
                )
                .await
            }
        });

        let event = timeout(Duration::from_millis(250), open_rx.recv())
            .await
            .expect("open renderer received command")
            .expect("open renderer event");
        assert_eq!(event["type"], json!("rendererCommand"));
        let command_id = event["command"]["commandId"]
            .as_str()
            .expect("command id")
            .to_string();
        hub.handle_renderer_command_result(&Map::from_iter([
            ("commandId".to_string(), json!(command_id)),
            ("ok".to_string(), json!(true)),
            ("result".to_string(), json!({ "ok": true })),
            ("type".to_string(), json!("rendererCommandResult")),
        ]))
        .await;

        let result = timeout(Duration::from_millis(250), dispatch)
            .await
            .expect("renderer command completed")
            .expect("renderer command task")
            .expect("renderer command result");
        assert_eq!(result, json!({ "ok": true }));
    }
}
