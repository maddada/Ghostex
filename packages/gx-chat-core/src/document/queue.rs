//! Ghostex's own prompt queue: the rows the agent has not seen yet.

use ghostex_gx_protocol::Tri;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The queue as the renderer draws it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    pub capabilities: QueueCapabilities,
    /// Authoritative order, head first. Empty while supported but nothing waits.
    pub prompts: Vec<QueuedPrompt>,
}

/// What this daemon supports.
///
/// `supported` is false until a read or a frame CARRIES a queue field; that presence, even as an
/// empty list, is the capability probe. Every control hides rather than calling an endpoint that
/// would answer 404.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueCapabilities {
    pub supported: bool,
    pub can_queue: bool,
    pub can_edit: bool,
    pub can_remove: bool,
    pub can_reorder: bool,
    pub can_retry: bool,
    pub can_send_now: bool,
    pub can_sync_draft: bool,
}

/// One queued prompt row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedPrompt {
    pub id: String,
    /// A normal send held during startup, which is drawn in the transcript rather than above the
    /// composer.
    ///
    /// CDXC:SessionChat 2026-09-11 DECISION:
    /// Normal sends held during startup appear only in the transcript, including after reopening
    /// or switching devices. Explicitly queued prompts stay above the composer.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub startup_send: Tri<bool>,
    pub text: String,
    /// `queued`, `sending`, `failed`.
    pub state: String,
    /// Set only when `state` is `failed`: why the delivery attempt failed.
    ///
    /// Spelled `errorMessage`, which is the wire name `session-chat-queue.ts` defines and the name
    /// `apps/desktop/src/app/native_chat/queue.rs` reads.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub error_message: Tri<String>,
    /// Why gxserver still holds a waiting row, in the user's words, once it has waited about a
    /// minute (server/src/session_chat_queue_runtime/wait_reason.rs); both renderers show it under
    /// the row.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub wait_reason: Tri<String>,
    pub created_at: String,
    pub updated_at: String,
    /// The single line the row shows, projected so both renderers clip identically.
    #[serde(default)]
    pub preview: String,
    /// The row is mid-flight, so its controls are held.
    #[serde(default)]
    pub busy: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
