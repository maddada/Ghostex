//! What every provider adapter reads its answers into, so home gxserver handles Instabox, Boat and
//! `local` boxes alike.

use ghostex_gx_protocol::cloud_box::{CloudBoxState, CloudProviderLimits};

use crate::metadata::BoxMetadata;

/// One machine in a provider's list.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderMachine {
    pub provider_box_id: String,
    pub state: CloudBoxState,
    /// Ghostex's metadata; `None` for a machine Ghostex did not make.
    pub metadata: Option<BoxMetadata>,
    pub created_at: Option<String>,
    /// When the current run started (the chip's timer).
    pub started_at: Option<String>,
    pub stopped_at: Option<String>,
    /// The provider's own dollars for this box this month (Boat), when it reports them.
    pub cost_month_usd: Option<f64>,
    /// [`ghostex_gx_protocol::cloud_box::CloudBoxSize::id`] of its current size.
    pub size: Option<String>,
    /// Why it failed or stopped, as the provider says.
    pub error: Option<String>,
    /// The provider operation still running on it (Instabox), if any.
    pub operation_id: Option<String>,
}

/// A command's result.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ExecOutput {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl ExecOutput {
    /// The one JSON object a `ghostex box` verb prints: the whole stdout, else its last JSON line.
    pub fn json_object(&self) -> Option<serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(self.stdout.trim())
            .ok()
            .filter(serde_json::Value::is_object)
            .or_else(|| {
                self.stdout.lines().rev().find_map(|line| {
                    serde_json::from_str::<serde_json::Value>(line.trim())
                        .ok()
                        .filter(serde_json::Value::is_object)
                })
            })
    }

    /// The last lines of stderr (else stdout), for a failure message.
    pub fn failure_tail(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            self.stdout.trim()
        } else {
            self.stderr.trim()
        };
        let lines: Vec<&str> = text.lines().collect();
        lines[lines.len().saturating_sub(4)..].join("\n")
    }
}

/// One snapshot at the provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderSnapshot {
    pub provider_snapshot_id: String,
    pub name: String,
    /// `pending`, `ready` or `failed`.
    pub status: String,
    pub source_box_id: Option<String>,
    pub size_bytes: Option<u64>,
    pub created_at: Option<String>,
    pub error: Option<String>,
}

/// Who a key belongs to and what its plan allows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ProviderAccount {
    pub account_label: Option<String>,
    pub account_email: Option<String>,
    pub org_id: Option<String>,
    pub plan: Option<String>,
    pub limits: Option<CloudProviderLimits>,
    /// This month's dollars for the whole account, when the same answer carries them (Instabox).
    pub month_usd: Option<f64>,
}

/// Tokens of a signed-in session (Instabox) and the key minted from it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ProviderTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub access_expires_at: Option<String>,
    pub refresh_expires_at: Option<String>,
}
