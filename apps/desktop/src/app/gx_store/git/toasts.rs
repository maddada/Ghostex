//! The toasts the Git, worktree and export flows show: the same `toast` message the old runtime
//! posted to the app modal host (`createAppToastRequest`), handed to the same receiver.
//!
//! CDXC:Git 2026-06-24-15:22:
//! Git mutations and agent workflows must not depend on toast-host availability. A missing toast
//! presentation is not a reason to fake success or skip gxserver-owned Git state changes.

use std::sync::atomic::{AtomicU64, Ordering};

use ghostex_gx_core::git_menu::{GitToast, GitToastLevel};
use serde_json::{Map, Value, json};

use crate::GhostexGpuiApp;

static TOAST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A fresh id for a toast that is replaced in place (`createGpuiGitToastId`,
/// `createGpuiWorktreeToastId`).
pub(crate) fn new_toast_id(prefix: &str) -> String {
    let millis = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    let sequence = TOAST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("toast-gpui-{prefix}-{millis:x}-{sequence}")
}

/// One toast's options.
#[derive(Clone, Debug, Default)]
pub(crate) struct ToastOptions {
    pub(crate) description: Option<String>,
    pub(crate) persistent: bool,
    pub(crate) toast_id: Option<String>,
}

impl ToastOptions {
    pub(crate) fn described(description: impl Into<String>) -> Self {
        Self {
            description: Some(description.into()),
            ..Self::default()
        }
    }

    pub(crate) fn keyed(toast_id: &str) -> Self {
        Self {
            toast_id: Some(toast_id.to_string()),
            ..Self::default()
        }
    }

    pub(crate) fn persistent(toast_id: &str) -> Self {
        Self {
            persistent: true,
            toast_id: Some(toast_id.to_string()),
            ..Self::default()
        }
    }

    pub(crate) fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl GhostexGpuiApp {
    pub(crate) fn git_toast(
        &mut self,
        level: GitToastLevel,
        title: &str,
        options: ToastOptions,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut message = Map::new();
        if let Some(description) = options
            .description
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            message.insert("description".into(), json!(description));
        }
        message.insert("level".into(), json!(level.as_str()));
        if options.persistent {
            message.insert("persistent".into(), json!(true));
        }
        message.insert("title".into(), json!(title.trim()));
        if let Some(toast_id) = options.toast_id {
            message.insert("toastId".into(), json!(toast_id));
        }
        message.insert("type".into(), json!("toast"));
        self.receive_gpui_app_toast_bridge_message(&Value::Object(message), cx);
    }

    /// A toast a gx-core decision ended with.
    pub(crate) fn git_planned_toast(&mut self, toast: &GitToast, cx: &mut gpui::Context<Self>) {
        let options = ToastOptions {
            description: toast.description.map(str::to_string),
            ..ToastOptions::default()
        };
        self.git_toast(toast.level, toast.title, options, cx);
    }
}
