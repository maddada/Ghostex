//! Settings > Updates > Go back, the app's side: the offer the page shows and the swap it asks for
//! (go_back/mod.rs has the decision record).
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::consts::*;
use crate::go_back::{GoBackHow, offer, swap};
use crate::*;

/// `updatesState.goBack`: null when this computer can't go back.
pub(super) fn offer_payload() -> Value {
    let Some(offer) = offer() else {
        return Value::Null;
    };
    match &offer.how {
        GoBackHow::Swap => json!({
            "current": offer.current, "previous": offer.previous, "how": "swap",
        }),
        GoBackHow::Command(command) => json!({
            "current": offer.current, "previous": offer.previous, "how": "command",
            "command": command,
        }),
        GoBackHow::Download(url) => json!({
            "current": offer.current, "previous": offer.previous, "how": "download", "url": url,
        }),
    }
}

impl GhostexGpuiApp {
    /// Go back from Settings: the kept version is swapped in once Ghostex has quit, and starts.
    pub(super) fn go_back_from_settings(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(offer) = offer().filter(|offer| offer.how == GoBackHow::Swap) else {
            return;
        };
        match swap::start(&offer) {
            Ok(()) => {
                GPUI_APP_QUIT_IN_PROGRESS.store(true, std::sync::atomic::Ordering::Release);
                cx.quit();
            }
            Err(reason) => self.upsert_gpui_app_toast(
                GpuiAppToast {
                    copy_text: None,
                    id: "gpui-go-back-failed".to_string(),
                    level: GpuiAppToastLevel::from_raw(Some("warning")),
                    title: format!("Couldn't go back to {}", offer.previous),
                    description: Some(format!("{reason} You can download it from GitHub instead.")),
                    loading: false,
                    persistent: false,
                    duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                    action: None,
                    epoch: 0,
                },
                cx,
            ),
        }
    }
}
