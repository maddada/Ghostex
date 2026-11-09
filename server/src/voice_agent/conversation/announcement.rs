//! What the conversation tells the client and the model when a session changes state while it is
//! open, and the `announcement` frame the client shows for it.

use serde_json::{json, Value};

/// A session changed state while the conversation was open.
#[derive(Clone, Debug)]
pub(in crate::voice_agent) struct Announcement {
    pub(in crate::voice_agent) text: String,
    pub(in crate::voice_agent) kind: &'static str,
    pub(in crate::voice_agent) project_id: String,
    pub(in crate::voice_agent) session_id: String,
    /// The connected computer the session runs on, when it is not this one.
    pub(in crate::voice_agent) server_id: Option<String>,
    /// What the model is told instead of `text`, when it needs more (a focused call's finished
    /// turn carries the reply).
    pub(in crate::voice_agent) model_text: Option<String>,
}

impl Announcement {
    /// The `announcement` frame the client shows.
    pub(super) fn client_frame(&self) -> Value {
        let mut frame = json!({
            "type": "announcement",
            "text": self.text,
            "kind": self.kind,
            "projectId": self.project_id,
            "sessionId": self.session_id,
        });
        if let Some(server_id) = &self.server_id {
            frame["serverId"] = json!(server_id);
        }
        frame
    }
}
