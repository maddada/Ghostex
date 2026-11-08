//! The one-time 11.0.1 security notice (server/src/security_upgrade.rs): when this computer's
//! gxserver comes up and says the notice is due, a toast that stays until closed says Ghostex got
//! more secure and which phones and computers must pair again, with Open Remote settings. It is
//! marked seen as it shows, so it shows once.

use serde_json::{Value, json};

use super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::window::toast::{GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel};

const ENDPOINT: &str = "/api/securityUpgradeNotice";
const TOAST_ID: &str = "gpui-security-upgrade-notice";

/// The toast's words: "Pixel 9 and Studio" for two names, "Pixel 9, Studio and 2 more" past three.
pub(crate) fn notice_text(answer: &Value) -> Option<(String, String)> {
    if answer["show"].as_bool() != Some(true) {
        return None;
    }
    let names: Vec<String> = ["phones", "computers"]
        .iter()
        .flat_map(|key| answer[*key].as_array().cloned().unwrap_or_default())
        .filter_map(|name| name.as_str().map(str::to_string))
        .collect();
    let listed = match names.len() {
        0 => return None,
        1 => names[0].clone(),
        2 | 3 => format!(
            "{} and {}",
            names[..names.len() - 1].join(", "),
            names[names.len() - 1]
        ),
        more => format!("{}, {} and {} more", names[0], names[1], more - 2),
    };
    Some((
        "Ghostex got more secure".to_string(),
        format!(
            "Phones and computers that connect through Easy Connect must pair again: {listed}. Pair them again in Settings > Remote."
        ),
    ))
}

impl GhostexGpuiApp {
    /// Asks this computer's gxserver whether the notice is due, and shows it once.
    pub(super) fn gx_store_check_security_upgrade_notice(&mut self, cx: &mut gpui::Context<Self>) {
        if self.security_upgrade_notice_checked {
            return;
        }
        self.security_upgrade_notice_checked = true;
        cx.spawn(async move |this, cx| {
            let Ok(answer) = gx_rpc(None, ENDPOINT, json!({ "op": "read" })).await else {
                // Asked again at the next connection.
                let _ = this.update(cx, |this, _| this.security_upgrade_notice_checked = false);
                return;
            };
            let Some((title, description)) = notice_text(&answer) else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.upsert_gpui_app_toast(
                    GpuiAppToast {
                        id: TOAST_ID.to_string(),
                        level: GpuiAppToastLevel::Info,
                        title,
                        description: Some(description),
                        copy_text: None,
                        action: Some(GpuiAppToastAction::OpenRemoteSettings),
                        loading: false,
                        persistent: true,
                        duration_ms: 0,
                        epoch: 0,
                    },
                    cx,
                );
            });
            let _ = gx_rpc(None, ENDPOINT, json!({ "op": "dismiss" })).await;
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_names_what_must_pair_again() {
        let (title, text) = notice_text(&json!({
            "show": true, "phones": ["Pixel 9"], "computers": ["Studio"],
        }))
        .unwrap();
        assert_eq!(title, "Ghostex got more secure");
        assert!(text.contains("must pair again: Pixel 9 and Studio."));
        let (_, many) = notice_text(&json!({
            "show": true, "phones": ["A", "B", "C"], "computers": ["D", "E"],
        }))
        .unwrap();
        assert!(many.contains("A, B and 3 more."));
    }

    #[test]
    fn nothing_when_not_due() {
        assert!(notice_text(&json!({ "show": false, "phones": ["A"] })).is_none());
        assert!(notice_text(&json!({ "show": true, "phones": [], "computers": [] })).is_none());
    }
}
