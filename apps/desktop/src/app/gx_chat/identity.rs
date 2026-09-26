//! Who a retained chat is, and the two strings every other file keys off it.
//!
//! Two different keys, for two different tables, and they are not interchangeable:
//!
//! - The RETENTION key is `JSON.stringify([machineId, projectId, sessionId])`, which is what the
//!   deleted QuickJS retained store keyed its sessions by, and what the snapshot cache is keyed by.
//! - The STORAGE session key is `<projectId>:<sessionId>`, with a `remote-<machineId>:` prefix off
//!   the local machine (the deleted `broker.ts`'s rule). It is what every per-session client-storage record's suffix
//!   is built from, so a draft written under one spelling is invisible under the other.

use serde_json::Value;

/// What `session_chat_runtime.rs` writes for a chat on this computer, and what `broker.ts` tests
/// for before it builds a `remote-<machineId>:` prefix.
pub(crate) const LOCAL_MACHINE_ID: &str = "local";

/// One retained chat.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ChatIdentity {
    /// `None` on this computer, the remote machine's settings id otherwise.
    pub(super) machine_id: Option<String>,
    pub(super) project_id: String,
    pub(super) session_id: String,
}

impl ChatIdentity {
    /// Reads the identity out of the config the view hands its runtime.
    ///
    /// `machineId` is the spelling `session_chat_runtime.rs` puts on the wire: the literal
    /// `"local"` for this computer and the saved machine's settings id otherwise. Both it and an
    /// absent field mean "local", because `broker.ts` took its prefix from exactly that test
    /// (`machineId === 'local' ? '' : ...`) and a chat that read `remote-local:` here would open on
    /// a different draft than the one the TypeScript brain wrote.
    pub(super) fn from_config(config: &Value) -> Self {
        Self {
            machine_id: config
                .get("machineId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && *id != LOCAL_MACHINE_ID)
                .map(str::to_string),
            project_id: config
                .get("projectId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            session_id: config
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }
    }

    /// The identity a retention key spells, for a chat the store has already let go.
    pub(super) fn from_retention_key(key: &str) -> Option<Self> {
        let [machine, project_id, session_id]: [String; 3] = serde_json::from_str(key).ok()?;
        Some(Self {
            machine_id: (machine != LOCAL_MACHINE_ID).then_some(machine),
            project_id,
            session_id,
        })
    }

    /// `JSON.stringify([machineId, projectId, sessionId])`, the retention key.
    ///
    /// `SessionChatRuntimeIdentity.machineId` was a plain string on the TypeScript side, never
    /// absent, so this computer spells itself `"local"` here too rather than `null`.
    pub(super) fn retention_key(&self) -> String {
        let machine = self.machine_id.as_deref().unwrap_or(LOCAL_MACHINE_ID);
        Value::Array(vec![
            Value::String(machine.to_string()),
            Value::String(self.project_id.clone()),
            Value::String(self.session_id.clone()),
        ])
        .to_string()
    }

    /// `<projectId>:<sessionId>`, prefixed `remote-<machineId>:` off this computer.
    pub(super) fn storage_session_key(&self) -> String {
        match &self.machine_id {
            Some(id) => format!("remote-{id}:{}:{}", self.project_id, self.session_id),
            None => format!("{}:{}", self.project_id, self.session_id),
        }
    }
}
