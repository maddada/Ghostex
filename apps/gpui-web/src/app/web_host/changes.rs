//! The page's side of the Changes view (the desktop's `app/native_changes/`, compiled here
//! unchanged): a panel beside the chat that the header's right-panel button shows, and the page's
//! answers to what the view asks for. Comments go into the open chat's composer; opening a file in
//! Files or its folder needs the desktop, so those answer with a toast.

use gpui::{
    AnyElement, AppContext as _, Context, IntoElement, ParentElement as _, Styled as _, Window,
    div, px,
};

use crate::GhostexGpuiApp;
use crate::app::native_changes::{
    ChangesHostFn, ChangesHostInfo, ChangesPreset, ChangesRequest, ChangesView,
};

impl GhostexGpuiApp {
    /// The Changes panel, when the header's button has it open.
    pub(crate) fn render_web_changes_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.changes_open {
            return None;
        }
        let view = match self.native_changes.clone() {
            Some(view) => view,
            None => {
                let app = cx.weak_entity();
                let host: ChangesHostFn = std::rc::Rc::new(move |request, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.web_handle_changes_request(request, window, cx)
                    });
                });
                let view = cx.new(|cx| ChangesView::new(host, cx));
                self.native_changes = Some(view.clone());
                view
            }
        };
        let (agents, default_agent_id) = self.changes_commit_agents();
        let info = ChangesHostInfo {
            palette: self.gpui_native_modal_palette(),
            agents,
            default_agent_id,
        };
        let target = self.changes_target_for_active_project();
        let preset = self.native_changes_preset.take();
        view.update(cx, |view, cx| view.sync(target, info, preset, window, cx));
        Some(
            div()
                .h_full()
                .w(px(self.changes_width))
                .flex_none()
                .border_l_1()
                .border_color(crate::app::helpers::titlebar_popup_menu_border_color())
                .child(view)
                .into_any_element(),
        )
    }

    /// What the Git entry points call on the desktop to open the Changes tab.
    pub(crate) fn open_changes_view(
        &mut self,
        preset: Option<ChangesPreset>,
        cx: &mut Context<Self>,
    ) {
        self.native_changes_preset = preset;
        self.changes_open = true;
        cx.notify();
    }

    pub(crate) fn web_toggle_changes(&mut self, cx: &mut Context<Self>) {
        self.changes_open = !self.changes_open;
        cx.notify();
    }

    fn web_handle_changes_request(
        &mut self,
        request: ChangesRequest,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            ChangesRequest::Commit {
                project_key,
                action,
                message,
                agent_id,
                file_paths,
                new_branch,
            } => self.changes_commit(
                &project_key,
                action,
                message,
                agent_id,
                file_paths,
                new_branch,
                cx,
            ),
            ChangesRequest::MultipleCommits {
                project_key,
                agent_id,
            } => self.changes_multiple_commits(&project_key, agent_id, cx),
            ChangesRequest::MergeIntoMain {
                project_key,
                message,
                agent_id,
                file_paths,
            } => self.changes_merge_into_main(&project_key, message, agent_id, file_paths, cx),
            ChangesRequest::Comment { text, pill, done } => {
                let chat = self
                    .open_session
                    .as_ref()
                    .and_then(|session| self.native_chats.get(session))
                    .map(|(_, view)| view.clone());
                match chat {
                    Some(view) => {
                        view.update(cx, |view, cx| view.append_prompt(&pill, cx));
                        done("the open chat".to_string(), cx);
                    }
                    None => {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                        self.dispatch_gpui_app_modal_toast(
                            "info",
                            "Comment copied to the clipboard",
                            "Open a session's chat to add comments to its composer.",
                            cx,
                        );
                        done("the clipboard".to_string(), cx);
                    }
                }
            }
            ChangesRequest::OpenInFiles { .. } | ChangesRequest::RevealFile { .. } => {
                self.dispatch_gpui_app_modal_toast(
                    "info",
                    "Open the desktop app to open files",
                    "",
                    cx,
                );
            }
            ChangesRequest::AgentPicked(_) | ChangesRequest::Copied => {}
            ChangesRequest::Toast {
                title,
                description,
                error,
            } => self.dispatch_gpui_app_modal_toast(
                if error { "error" } else { "info" },
                &title,
                description.as_deref().unwrap_or(""),
                cx,
            ),
            ChangesRequest::WorkingTreeChanged { project_key } => {
                self.git_refresh_scoped(&project_key, cx)
            }
        }
    }
}
