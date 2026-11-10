//! The queue behind the notice modal: every app toast that carries a button comes here instead of
//! the toast stack, and waits in the centred modal (window/notice_modal.rs) until it is answered.
//! The desktop and the GPUI web build compile this file; each opens the modal's window its own way
//! (`open_app_notice_window`: app/notice_modal_window.rs here, web_host/toasts.rs on the page).
//!
//! SEE-ALSO: apps/desktop/src/app/window/notice_modal.rs (the CDXC:AppModal 2026-11-08 DECISION).

use gpui::{
    AnyElement, Entity, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, WindowHandle, px,
};
use gpui_component::tooltip::Tooltip;
use gpui_component::{Root, h_flex};

use crate::GhostexGpuiApp;
use crate::app::consts::TITLEBAR_CONTROL_HEIGHT;
use crate::app::helpers::{
    chrome_uses_light_appearance, titlebar_active_text_color, titlebar_popup_menu_hover_color,
    titlebar_svg_icon,
};
use crate::app::window::notice_modal::{
    GpuiNoticeModalWindow, NoticeModalCommand, NoticeModalHost, NoticeModalItem, NoticeModalTone,
};
use crate::app::window::toast::{
    GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel, gpui_app_toast_without_repeats,
};

/// The notices waiting for an answer and the modal that shows them.
#[derive(Default)]
pub(crate) struct AppNotices {
    /// Oldest first; the first one not put aside is shown.
    pub(crate) queue: Vec<GpuiAppToast>,
    /// Approvals closed without an answer: still waiting, back in the modal with the next notice.
    pub(crate) set_aside: Vec<String>,
    pub(crate) window: Option<(WindowHandle<Root>, Entity<GpuiNoticeModalWindow>)>,
}

impl GpuiAppToast {
    /// A toast with a button (an action, or Copy details) is shown as a notice modal instead.
    pub(crate) fn has_buttons(&self) -> bool {
        self.action.is_some() || self.copy_text.is_some()
    }

    fn is_approval(&self) -> bool {
        matches!(
            self.action,
            Some(GpuiAppToastAction::SecurityApproval { .. })
        )
    }
}

/// "Let the agents on studio and this computer reach each other" ends with a full stop.
fn sentence(text: &str) -> String {
    let text = text.trim();
    if text.ends_with(['.', '!', '?']) {
        text.to_string()
    } else {
        format!("{text}.")
    }
}

/// How the modal draws `toast`.
fn notice_item(toast: &GpuiAppToast) -> NoticeModalItem {
    if let Some(GpuiAppToastAction::SecurityApproval {
        request,
        computer,
        require_presence,
        ..
    }) = &toast.action
    {
        let asker = match request.requested_by.kind.as_str() {
            "owner" => "Ghostex".to_string(),
            _ if request.requested_by.label.trim().is_empty() => "An agent".to_string(),
            _ => request.requested_by.label.trim().to_string(),
        };
        let mut message = sentence(&request.summary);
        if let Some(detail) = request
            .detail
            .as_deref()
            .map(str::trim)
            .filter(|detail| !detail.is_empty())
        {
            message.push(' ');
            message.push_str(&sentence(detail));
        }
        return NoticeModalItem {
            id: toast.id.clone(),
            tone: NoticeModalTone::Approval,
            title: toast.title.clone(),
            message: Some(message),
            source: vec![
                ("Asked by".to_string(), asker),
                (
                    "Computer".to_string(),
                    computer
                        .clone()
                        .unwrap_or_else(|| "This computer".to_string()),
                ),
            ],
            action_label: Some("Approve".to_string()),
            copy_text: None,
            // Only when it will (Settings > Security, or an older gxserver): a plain Approve is one click.
            hint: require_presence.then(|| {
                if cfg!(target_os = "windows") {
                    "Approve asks for Windows Hello first."
                } else if cfg!(target_os = "macos") {
                    "Approve asks for Touch ID or your password first."
                } else {
                    "Approve asks for your password first."
                }
                .to_string()
            }),
        };
    }
    NoticeModalItem {
        id: toast.id.clone(),
        tone: match toast.level {
            GpuiAppToastLevel::Info => NoticeModalTone::Info,
            GpuiAppToastLevel::Success => NoticeModalTone::Success,
            GpuiAppToastLevel::Warning => NoticeModalTone::Warning,
            GpuiAppToastLevel::Error => NoticeModalTone::Error,
        },
        title: toast.title.clone(),
        message: toast.description.clone(),
        source: Vec::new(),
        action_label: toast
            .action
            .as_ref()
            .map(|action| action.label().to_string()),
        copy_text: toast.copy_text.clone(),
        hint: None,
    }
}

impl GhostexGpuiApp {
    /// Called by `upsert_gpui_app_toast` first: a toast with a button joins the notice modal and
    /// `None` comes back; a plain toast comes back to be shown, and ends a waiting notice of the
    /// same id (the daemon's "couldn't start" notice once it started after all).
    pub(crate) fn app_notice_take(
        &mut self,
        toast: GpuiAppToast,
        cx: &mut gpui::Context<Self>,
    ) -> Option<GpuiAppToast> {
        if !toast.has_buttons() {
            self.app_notice_remove(&toast.id, cx);
            return Some(toast);
        }
        // The notice ends a toast of its id ("Committing…" becomes "Committing didn't finish").
        // A notice already waiting under the id has no toast left to end, and stays where it is.
        if !self
            .app_notices
            .queue
            .iter()
            .any(|notice| notice.id == toast.id)
        {
            self.remove_gpui_app_toast(&toast.id, cx);
        }
        self.app_notice_push(toast, cx);
        None
    }

    /// Adds `toast` to the notices, or updates the one with its id. The repeat rule of toasts
    /// (CDXC:AppModal 2026-10-24 in window/toast.rs) holds here too, so a refusal raised again
    /// and again never stacks notices; approvals are told apart by their request id instead.
    pub(crate) fn app_notice_push(&mut self, toast: GpuiAppToast, cx: &mut gpui::Context<Self>) {
        let toast = if toast.is_approval() {
            toast
        } else {
            match gpui_app_toast_without_repeats(&self.app_notices.queue, toast) {
                Some(toast) => toast,
                None => return,
            }
        };
        self.app_notice_insert(toast, cx);
    }

    /// `app_notice_push` without the repeat rule: for a toast already shown, which was given a
    /// button after it was raised.
    pub(crate) fn app_notice_insert(&mut self, toast: GpuiAppToast, cx: &mut gpui::Context<Self>) {
        let notices = &mut self.app_notices;
        if let Some(waiting) = notices
            .queue
            .iter_mut()
            .find(|waiting| waiting.id == toast.id)
        {
            *waiting = toast;
        } else {
            notices.queue.push(toast);
            // Approvals put aside come back with the next new notice.
            notices.set_aside.clear();
        }
        self.sync_app_notice_window(cx);
    }

    /// The notice `id` is over: answered somewhere else, run out, or replaced by a plain toast.
    pub(crate) fn app_notice_remove(&mut self, id: &str, cx: &mut gpui::Context<Self>) {
        let notices = &mut self.app_notices;
        let before = notices.queue.len();
        notices.queue.retain(|notice| notice.id != id);
        notices.set_aside.retain(|aside| aside != id);
        if notices.queue.len() != before {
            self.sync_app_notice_window(cx);
        }
    }

    /// Shows the waiting notices that are not put aside, or closes the modal when none is left.
    pub(crate) fn sync_app_notice_window(&mut self, cx: &mut gpui::Context<Self>) {
        // The header's Approvals waiting pill counts the approvals put aside.
        cx.notify();
        let notices = &self.app_notices;
        let items: Vec<NoticeModalItem> = notices
            .queue
            .iter()
            .filter(|notice| !notices.set_aside.contains(&notice.id))
            .map(notice_item)
            .collect();
        if items.is_empty() {
            if let Some((handle, _)) = self.app_notices.window.take() {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
                self.app_notice_window_closed(cx);
            }
            return;
        }
        let palette = self.gpui_native_dialog_palette();
        if let Some((handle, view)) = self.app_notices.window.clone() {
            let shown = handle.update(cx, |_, _, cx| {
                view.update(cx, |view, cx| {
                    view.set_palette(palette, cx);
                    view.set_items(items.clone(), cx);
                })
            });
            if shown.is_ok() {
                return;
            }
            self.app_notices.window = None;
        }
        let host: NoticeModalHost =
            self.native_app_modal_host(cx, |app, command, cx| app.answer_app_notice(command, cx));
        self.app_notices.window = self.open_app_notice_window(palette, host, items, cx);
    }

    fn answer_app_notice(&mut self, command: NoticeModalCommand, cx: &mut gpui::Context<Self>) {
        let (id, run) = match &command {
            NoticeModalCommand::Act { id } => (id.clone(), Some(false)),
            NoticeModalCommand::Deny { id } => (id.clone(), Some(true)),
            NoticeModalCommand::Close { id } => (id.clone(), None),
        };
        let Some(index) = self.app_notices.queue.iter().position(|n| n.id == id) else {
            return;
        };
        let Some(deny) = run else {
            // Closed without an answer: an approval keeps waiting, anything else is done.
            if self.app_notices.queue[index].is_approval() {
                self.app_notices.set_aside.push(id);
            } else {
                self.app_notices.queue.remove(index);
            }
            self.sync_app_notice_window(cx);
            return;
        };
        let notice = self.app_notices.queue.remove(index);
        self.sync_app_notice_window(cx);
        let Some(mut action) = notice.action else {
            return;
        };
        if let GpuiAppToastAction::SecurityApproval { deny: denied, .. } = &mut action {
            *denied = deny;
        } else if deny {
            return;
        }
        self.run_gpui_app_toast_action(&action, cx);
    }

    /// The approvals put aside with Escape, Close or the corner X that still wait.
    pub(crate) fn app_notices_set_aside_count(&self) -> usize {
        let notices = &self.app_notices;
        notices
            .set_aside
            .iter()
            .filter(|id| notices.queue.iter().any(|notice| &notice.id == *id))
            .count()
    }

    /// The Approvals waiting pill: every approval put aside comes back in the notice modal.
    pub(crate) fn app_notices_reopen(&mut self, cx: &mut gpui::Context<Self>) {
        self.app_notices.set_aside.clear();
        self.sync_app_notice_window(cx);
    }

    /// "2 approvals waiting" in the work area header while approvals put aside still wait; nothing
    /// when none does. `compact` (a narrow header) keeps the icon and the number.
    ///
    /// CDXC:AppModal 2026-11-08 DECISION: User (via the coordinator, answering whether set-aside
    /// approvals need a place to reopen them): "Yes to the pill". An approval closed without an
    /// answer stays waiting; this pill, desktop and browser alike, reopens the notice modal on
    /// it, and goes when no approval is put aside.
    pub(crate) fn render_approvals_waiting_pill(
        &self,
        compact: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let count = self.app_notices_set_aside_count();
        if count == 0 {
            return None;
        }
        let text = titlebar_active_text_color();
        let hover = titlebar_popup_menu_hover_color();
        // The notice modal's warning amber, so the waiting approval catches the eye.
        let amber: gpui::Hsla = if chrome_uses_light_appearance() {
            gpui::rgb(0xb45309).into()
        } else {
            gpui::rgb(0xfbbf24).into()
        };
        let label: SharedString = if compact {
            count.to_string().into()
        } else if count == 1 {
            "1 approval waiting".into()
        } else {
            format!("{count} approvals waiting").into()
        };
        Some(
            h_flex()
                .id("header-approvals-waiting")
                .flex_none()
                .h(px(TITLEBAR_CONTROL_HEIGHT))
                .px(px(8.0))
                .gap(px(5.0))
                .items_center()
                .rounded_full()
                .bg(hover.opacity(1.6))
                .text_size(px(12.0))
                .text_color(text)
                .whitespace_nowrap()
                .cursor_pointer()
                .hover(move |pill| pill.bg(hover.opacity(2.6)))
                .child(titlebar_svg_icon("titlebar/shield-check.svg", 14.0, amber))
                .child(label)
                .tooltip(|window, cx| {
                    Tooltip::new("Approvals you closed are still waiting. Click to answer them.")
                        .build(window, cx)
                })
                .on_click(cx.listener(|app, _, _, cx| app.app_notices_reopen(cx)))
                .into_any_element(),
        )
    }
}
