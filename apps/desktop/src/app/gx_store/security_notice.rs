//! When the 11.0.1 security notice opens, and the rows it shows (server/src/security_upgrade.rs):
//! when this computer's gxserver comes up and says the notice is due, it opens in the middle of
//! the screen (window/security_upgrade_modal.rs). Phone rows come from gxserver; each computer's
//! "last connected" is this app's own record of when it last had that computer's rows.

use serde_json::{Value, json};

use super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::window::{
    NoticeDevice, SecurityNoticeRow, SecurityNoticeRowStatus, leftover_reason,
};

pub(crate) const SECURITY_NOTICE_ENDPOINT: &str = "/api/securityUpgradeNotice";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// Computers not connected for longer than this before the update leave the notice (Q1728).
const UNUSED_FOR_MS: i64 = 30 * DAY_MS;

/// "just now", "12 minutes ago", "3 hours ago", "yesterday", "5 days ago".
pub(crate) fn ago(at_ms: i64, now_ms: i64) -> String {
    let minutes = (now_ms - at_ms).max(0) / 60_000;
    match minutes {
        0 => "just now".to_string(),
        1 => "a minute ago".to_string(),
        2..=59 => format!("{minutes} minutes ago"),
        60..=119 => "an hour ago".to_string(),
        120..=1439 => format!("{} hours ago", minutes / 60),
        1440..=2879 => "yesterday".to_string(),
        _ => format!("{} days ago", minutes / 1440),
    }
}

fn status(row: &Value) -> SecurityNoticeRowStatus {
    match row["status"].as_str() {
        Some("paired") => SecurityNoticeRowStatus::Paired,
        // An old pairing nobody needs to pair again: not counted, offers Forget.
        Some("leftover") => SecurityNoticeRowStatus::Leftover,
        // Pairing v2: waiting on the owner's approval, with its words.
        Some("pending") if row["approvalId"].as_str().is_some_and(|id| !id.is_empty()) => {
            SecurityNoticeRowStatus::Pending
        }
        _ => SecurityNoticeRowStatus::Waiting,
    }
}

fn words(row: &Value) -> Vec<String> {
    row["words"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|word| word.as_str().map(str::to_string))
        .collect()
}

fn approval_id(row: &Value) -> Option<String> {
    row["approvalId"].as_str().map(str::to_string)
}

fn pending_id(row: &Value) -> Option<String> {
    row["pendingId"].as_str().map(str::to_string)
}

/// The notice's rows from `/api/securityUpgradeNotice`. `last_connected` answers a computer's
/// last connection by its Settings > Remote id; a computer last connected over a month before the
/// update is left out.
pub(crate) fn notice_rows(
    view: &Value,
    last_connected: &dyn Fn(&str) -> Option<i64>,
    now_ms: i64,
) -> (Vec<SecurityNoticeRow>, Vec<SecurityNoticeRow>) {
    let text = |row: &Value, key: &str| row[key].as_str().unwrap_or_default().to_string();
    let taken_at = view["takenAtMs"].as_i64().unwrap_or(now_ms);
    let phones = view["phones"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let kind = text(row, "kind");
            let used = row["lastUsedAt"]
                .as_str()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| format!("used {}", ago(at.timestamp_millis(), now_ms)));
            let kind_name = if kind.is_empty() {
                "Phone"
            } else {
                kind.as_str()
            };
            let status = status(row);
            SecurityNoticeRow {
                id: text(row, "id"),
                name: text(row, "name"),
                kind_label: if status == SecurityNoticeRowStatus::Leftover {
                    format!(
                        "{kind_name} \u{b7} {}",
                        leftover_reason(row["leftover"].as_str().unwrap_or_default())
                    )
                } else {
                    format!("{kind_name} \u{b7} Easy Connect")
                },
                device: NoticeDevice::EasyConnectPhone,
                pending_id: pending_id(row),
                detail: match used {
                    Some(used) if !kind.is_empty() => format!("{kind} \u{b7} {used}"),
                    Some(used) => used,
                    None => kind,
                },
                status,
                words: words(row),
                approval_id: approval_id(row),
            }
        })
        .collect();
    let computers = view["computers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let id = text(row, "id");
            let last = last_connected(&id);
            if last.is_some_and(|at| at < taken_at - UNUSED_FOR_MS) {
                return None;
            }
            Some(SecurityNoticeRow {
                kind_label: "Computer \u{b7} Easy Connect".to_string(),
                device: NoticeDevice::EasyConnectComputer,
                pending_id: None,
                detail: match last {
                    Some(at) => format!("Easy Connect \u{b7} connected {}", ago(at, now_ms)),
                    None => "Easy Connect".to_string(),
                },
                id,
                name: text(row, "name"),
                status: status(row),
                words: Vec::new(),
                approval_id: None,
            })
        })
        .collect();
    (phones, computers)
}

/// The rows for phones that sign in over SSH (one row for them all, or the one waiting with its
/// words) and for computers reached over SSH (nothing to do), from `sshPhones` and `sshComputers`.
pub(crate) fn ssh_rows(view: &Value) -> (Vec<SecurityNoticeRow>, Vec<SecurityNoticeRow>) {
    let text = |row: &Value, key: &str| row[key].as_str().unwrap_or_default().to_string();
    let phones = view["sshPhones"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let count = row["count"].as_u64().unwrap_or(1).max(1);
            let pending_name = text(row, "name");
            let status = status(row);
            SecurityNoticeRow {
                kind_label: match row["kind"].as_str().filter(|kind| !kind.is_empty()) {
                    Some(kind) => format!("{kind} \u{b7} Tailscale or SSH"),
                    None => "Phone \u{b7} Tailscale or SSH".to_string(),
                },
                device: NoticeDevice::SshPhone,
                pending_id: pending_id(row),
                id: crate::app::window::SSH_PHONES_ROW_ID.to_string(),
                name: if status == SecurityNoticeRowStatus::Pending && !pending_name.is_empty() {
                    pending_name
                } else if count == 1 {
                    "Your phone".to_string()
                } else {
                    format!("{count} phones")
                },
                detail: match status {
                    SecurityNoticeRowStatus::Pending => {
                        "Approve if the phone shows the same words".to_string()
                    }
                    SecurityNoticeRowStatus::Paired => "Signs in over SSH".to_string(),
                    _ => "Open Ghostex on it to pair".to_string(),
                },
                status,
                words: words(row),
                approval_id: approval_id(row),
            }
        })
        .collect();
    let computers = view["sshComputers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| SecurityNoticeRow {
            id: text(row, "id"),
            name: text(row, "name"),
            kind_label: "Computer \u{b7} SSH".to_string(),
            device: NoticeDevice::SshComputer,
            pending_id: None,
            detail: "May ask once: Touch ID to let its agents connect".to_string(),
            status: SecurityNoticeRowStatus::Info,
            words: Vec::new(),
            approval_id: None,
        })
        .collect();
    (phones, computers)
}

impl GhostexGpuiApp {
    /// Asks this computer's gxserver whether the notice is due and opens it. Asked once per run:
    /// Remind me later brings it back on a later day, so a run never asks twice.
    pub(super) fn gx_store_check_security_upgrade_notice(&mut self, cx: &mut gpui::Context<Self>) {
        if self.security_upgrade_notice_checked {
            return;
        }
        self.security_upgrade_notice_checked = true;
        cx.spawn(async move |this, cx| {
            let Ok(view) = gx_rpc(None, SECURITY_NOTICE_ENDPOINT, json!({ "op": "read" })).await
            else {
                // Asked again at the next connection.
                let _ = this.update(cx, |this, _| this.security_upgrade_notice_checked = false);
                return;
            };
            if view["show"].as_bool() != Some(true) {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.open_gpui_security_upgrade_modal(&view, cx)
            });
        })
        .detach();
    }
}
