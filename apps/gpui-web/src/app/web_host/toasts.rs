//! App toasts in the page. The desktop draws them in a bottom-centre popup window because native terminal and CEF views cover its work area; the page is one canvas, so the same toast stack (`window/toast.rs`, the desktop's file) is drawn as an overlay at the bottom centre of the shell.
use gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Styled as _,
    div, px,
};

use crate::GhostexGpuiApp;
use crate::app::window::toast::{
    GPUI_APP_TOAST_BOTTOM_MARGIN, GPUI_APP_TOAST_DEFAULT_DURATION_MS, GPUI_APP_TOAST_MAX_VISIBLE,
    GPUI_APP_TOAST_WINDOW_WIDTH, GpuiAppToast, GpuiAppToastLevel, GpuiAppToastWindow,
    gpui_app_toast_from_bridge_message, gpui_app_toast_stack_height,
    gpui_normalized_app_toast_description,
};

#[derive(Default)]
pub(crate) struct WebToasts {
    toasts: Vec<GpuiAppToast>,
    view: Option<Entity<GpuiAppToastWindow>>,
    id_counter: u64,
    epoch: u64,
}

impl GhostexGpuiApp {
    /// The desktop sends this to the open app modal's toast layer, or to the app toast window when no modal is open. The page has one toast stack for both.
    pub(crate) fn dispatch_gpui_app_modal_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast(level, title, description, cx);
    }

    pub(crate) fn dispatch_gpui_workspace_action_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        cx: &mut Context<Self>,
    ) {
        let toasts = &mut self.web_host.toasts;
        toasts.id_counter = toasts.id_counter.wrapping_add(1);
        let id = format!("gpui-app-toast-{}", toasts.id_counter);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id,
                copy_text: None,
                level: GpuiAppToastLevel::from_raw(Some(level)),
                title: title.to_string(),
                description: (!description.is_empty()).then(|| description.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    pub(crate) fn receive_gpui_app_toast_bridge_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let toasts = &mut self.web_host.toasts;
        toasts.id_counter = toasts.id_counter.wrapping_add(1);
        let generated_id = format!("gpui-app-toast-{}", toasts.id_counter);
        if let Some(toast) = gpui_app_toast_from_bridge_message(message, generated_id) {
            self.upsert_gpui_app_toast(toast, cx);
        }
    }

    /// The desktop's `upsert_gpui_app_toast`: same id replaces in place, at most four shown, auto-dismiss guarded by the epoch.
    pub(crate) fn upsert_gpui_app_toast(
        &mut self,
        mut toast: GpuiAppToast,
        cx: &mut Context<Self>,
    ) {
        toast.description =
            gpui_normalized_app_toast_description(&toast.title, toast.description.as_deref());
        let toasts = &mut self.web_host.toasts;
        toasts.epoch = toasts.epoch.wrapping_add(1);
        toast.epoch = toasts.epoch;
        let auto_dismiss =
            (!toast.persistent).then(|| (toast.id.clone(), toast.epoch, toast.duration_ms));
        if let Some(existing) = toasts
            .toasts
            .iter_mut()
            .find(|existing| existing.id == toast.id)
        {
            *existing = toast;
        } else {
            toasts.toasts.push(toast);
            while toasts.toasts.len() > GPUI_APP_TOAST_MAX_VISIBLE {
                toasts.toasts.remove(0);
            }
        }
        if let Some((toast_id, epoch, duration_ms)) = auto_dismiss {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(duration_ms))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this
                        .web_host
                        .toasts
                        .toasts
                        .iter()
                        .any(|toast| toast.id == toast_id && toast.epoch == epoch)
                    {
                        this.remove_gpui_app_toast(&toast_id, cx);
                    }
                });
            })
            .detach();
        }
        self.sync_web_toasts(cx);
    }

    pub(crate) fn remove_gpui_app_toast(&mut self, toast_id: &str, cx: &mut Context<Self>) {
        let before = self.web_host.toasts.toasts.len();
        self.web_host
            .toasts
            .toasts
            .retain(|toast| toast.id != toast_id);
        if self.web_host.toasts.toasts.len() != before {
            self.sync_web_toasts(cx);
        }
    }

    fn sync_web_toasts(&mut self, cx: &mut Context<Self>) {
        let toasts = self.web_host.toasts.toasts.clone();
        match &self.web_host.toasts.view {
            Some(view) => view.update(cx, |view, cx| view.set_toasts(toasts, cx)),
            None if !toasts.is_empty() => {
                let app = cx.weak_entity();
                self.web_host.toasts.view = Some(cx.new(|_| GpuiAppToastWindow {
                    app,
                    toasts,
                    hovered_toast_id: None,
                    blur_region: Vec::new(),
                }));
            }
            None => {}
        }
        cx.notify();
    }

    /// The toast stack, bottom-centre over the shell; nothing while there is no toast.
    pub(crate) fn render_web_toasts(&self) -> Option<AnyElement> {
        let toasts = &self.web_host.toasts;
        if toasts.toasts.is_empty() {
            return None;
        }
        let view = toasts.view.clone()?;
        Some(
            div()
                .absolute()
                .bottom(px(GPUI_APP_TOAST_BOTTOM_MARGIN))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .w(px(GPUI_APP_TOAST_WINDOW_WIDTH))
                        .h(px(gpui_app_toast_stack_height(&toasts.toasts)))
                        .child(view),
                )
                .into_any_element(),
        )
    }
}
