//! The Work page's "current session": the session this window has selected, which the page's
//! Start chat ▾ menu offers to link the ticket to. The host sends it with `work.ready` and again
//! as a `ghostex-work-current-session` event whenever the sidebar's selection, the session's
//! title or its links change.
//!
//! CDXC:WorkMode 2026-10-10 DECISION:
//! User: "implement b please" (the Link to current session mockup, option B). Start chat is a
//! split button whose menu offers Start chat, Link to current session ("'<title>' · <project> ·
//! <agent>") and Start in cloud. The current session is the one selected in the window that hosts
//! the Work view; linking adds the ticket to its links (a session may carry several tickets) and
//! never removes another, a toast offers Undo, and the item is shown disabled with the reason when
//! there is no current session or it is a remote machine's.
//!
//! SEE-ALSO: apps/desktop/views/work/start-actions.tsx (the menu), server/src/work_mode/link_add.rs
//! (the add and its undo), apps/gpui-web/src/app/web_host/work_view.rs (the browser's answer).

use ghostex_gx_core::SessionKey;
use serde_json::{Value, json};

use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// `{ projectId, sessionId, title, projectName, agentName, remote, machineName?, links }`, or
    /// null when the window has no session selected.
    pub(crate) fn work_view_current_session(&self) -> Value {
        let Some(snapshot) = self.native_sidebar.snapshot.as_ref() else {
            return Value::Null;
        };
        // The active group's focused row first: that is the session the window shows.
        let mut groups: Vec<_> = snapshot.groups.iter().collect();
        groups.sort_by_key(|group| !group.is_active);
        let Some((group, session)) = groups.into_iter().find_map(|group| {
            group
                .sessions
                .iter()
                .find(|session| session.is_focused && !session.is_browser())
                .map(|session| (group, session))
        }) else {
            return Value::Null;
        };
        let key = SessionKey::parse_remote_scoped_session_id(&session.session_id)
            .or_else(|| SessionKey::parse_sidebar_session_id(&session.session_id));
        let Some(key) = key else {
            return Value::Null;
        };
        let agent_name = self
            .gx_store
            .core
            .presentation()
            .session(&key)
            .and_then(|record| record.agent_name.clone().or(record.agent_id.clone()));
        let work = session.work.as_deref();
        let links = json!({
            "pullRequest": work
                .and_then(|work| work.pull_request.as_ref())
                .map(|pr| json!({ "number": pr.number, "url": pr.url })),
            "linearIssues": work
                .map(|work| work.linear_issues.iter().map(|issue| issue.identifier.clone()).collect::<Vec<_>>())
                .unwrap_or_default(),
            "githubIssues": work
                .map(|work| work.github_issues.iter().map(|issue| issue.number).collect::<Vec<_>>())
                .unwrap_or_default(),
        });
        let machine_name = group
            .remote_machine_context
            .as_ref()
            .and_then(|context| context.get("machineName").and_then(Value::as_str))
            .map(str::to_string);
        json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "title": session.title(),
            "projectName": group.title,
            "agentName": agent_name,
            "remote": !key.machine.is_local(),
            "machineName": machine_name,
            "workMode": session.work.is_some() || group.work_mode_project_id().is_some(),
            "links": links,
        })
    }

    /// Tells a loaded Work page about a new current session (or a change to it). Called with
    /// every sidebar snapshot; sends only when something the page shows changed.
    pub(crate) fn work_view_sync_current_session(&mut self, cx: &mut gpui::Context<Self>) {
        if !self
            .project_workarea_runtime_cef_surfaces
            .contains_key(&ProjectWorkareaCefSurfaceSlotKey::Work)
        {
            return;
        }
        let current = self.work_view_current_session();
        if self.work_view.current_session_sent.as_ref() == Some(&current) {
            return;
        }
        self.work_view.current_session_sent = Some(current.clone());
        self.dispatch_project_workarea_json_event(
            ProjectWorkareaCefSurfaceSlotKey::Work,
            "ghostex-work-current-session",
            &current.to_string(),
            cx,
        );
    }
}
