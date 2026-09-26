//! Going to the session for a conversation, from the desktop: gxserver resolves it
//! (`/api/openConversation`: the live owner, else restore, else resume) and this file focuses the
//! answer. A live session is focused the way a row click focuses it; a restored or resumed one is
//! a session this app just made, opened the way a create opens it.
//!
//! SEE-ALSO: server/src/open_conversation.rs, apps/desktop/src/app/stashed_prompt_jump.rs.

use ghostex_gx_core::SessionKey;
use serde_json::{Map, Value, json};

use super::session_calls::open_conversation;
use crate::GhostexGpuiApp;

/// What to open, and what to say when it cannot be opened.
pub(crate) struct ConversationTarget {
    pub(crate) agent_session_id: Option<String>,
    pub(crate) project_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) restore_reason: &'static str,
    pub(crate) resume_reason: &'static str,
    pub(crate) failure_title: &'static str,
}

impl GhostexGpuiApp {
    pub(crate) fn gx_store_open_conversation(
        &mut self,
        target: ConversationTarget,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut params = Map::new();
        if let Some(agent_session_id) = target.agent_session_id {
            params.insert("agentSessionId".into(), json!(agent_session_id));
        }
        if let Some(project_id) = target.project_id {
            params.insert("projectId".into(), json!(project_id));
        }
        if let Some(session_id) = target.session_id {
            params.insert("sessionId".into(), json!(session_id));
        }
        params.insert("restoreReason".into(), json!(target.restore_reason));
        params.insert("resumeReason".into(), json!(target.resume_reason));
        let failure_title = target.failure_title;
        cx.spawn(async move |this, cx| {
            let result = open_conversation(Value::Object(params)).await;
            let _ = this.update(cx, |this, cx| {
                let answer = result.ok().and_then(|answer| {
                    let text = |key: &str| answer.get(key)?.as_str().map(str::to_string);
                    Some((text("outcome")?, text("projectId")?, text("sessionId")?))
                });
                match answer {
                    Some((outcome, project_id, session_id)) if outcome == "focus" => {
                        let sidebar_session_id =
                            SessionKey::local(&project_id, &session_id).to_sidebar_session_id();
                        this.dispatch_native_sidebar_ui(
                            json!({
                                "type": "selectSession",
                                "mode": "focus",
                                "sessionId": sidebar_session_id,
                            }),
                            cx,
                        );
                    }
                    Some((_, project_id, session_id)) => {
                        this.gx_store_focus_created_session(
                            &project_id,
                            &session_id,
                            false,
                            None,
                            cx,
                        );
                    }
                    // The daemon answers a removed session row or an unresumable agent with an
                    // error: a conversation that cannot be reopened, not a failure worth showing
                    // raw. Prompt text, paths and ids stay out of the notice.
                    None => this.dispatch_gpui_app_modal_toast("warning", failure_title, "", cx),
                }
            });
        })
        .detach();
    }
}
