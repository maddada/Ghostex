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
    /// The session is a thread a coordinator runs (`SessionInfo::thread_of`): a call about all
    /// sessions leaves it out (`reaches_all_sessions_call`).
    pub(in crate::voice_agent) sub_thread: bool,
    /// What the model is told instead of `text`, when it needs more (a focused call's finished
    /// turn carries the reply).
    pub(in crate::voice_agent) model_text: Option<String>,
}

impl Announcement {
    /// Whether a call about all sessions says it.
    ///
    /// CDXC:VoiceMode 2026-11-26 DECISION: User: the voice agent should not announce a
    /// coordinator's sub-threads; it kept saying "<thread> finished its turn" although its
    /// instructions count threads as the coordinator's work, because gxserver handed it every
    /// thread's update as an ordinary one to say. A call about all sessions now drops every update
    /// about a thread, approvals included (the coordinator, the sidebar and a call about the
    /// coordinator still have them). To announce a thread's approvals again, return true here for
    /// `kind == "waitingOnApproval"`.
    pub(in crate::voice_agent) fn reaches_all_sessions_call(&self) -> bool {
        !self.sub_thread
    }

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
