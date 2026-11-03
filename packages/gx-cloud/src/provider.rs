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

/// [`ExecOutput::missing_box_command`]'s message.
pub const MISSING_BOX_COMMAND: &str =
    "The Ghostex on this box is too old for Cloud Boxes (it has no `ghostex box` command).";

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

    /// What a box whose `ghostex` predates Cloud Boxes says instead of answering a `ghostex box`
    /// verb, in words for the box's error line, or `None`.
    ///
    /// CDXC:CloudBoxes 2026-11-02 WHY: an old `ghostex` answers `Unknown command: box` followed by
    /// its whole usage, and the last lines of that were the box's error ("quick-actions --help
    /// Show focused project quick-action help"), which said nothing about what went wrong.
    pub fn missing_box_command(&self) -> Option<&'static str> {
        [&self.stdout, &self.stderr]
            .iter()
            .any(|text| {
                text.lines()
                    .any(|line| line.trim() == "Unknown command: box")
            })
            .then_some(MISSING_BOX_COMMAND)
    }

    /// Why a `ghostex box` verb printed no JSON answer.
    pub fn box_verb_failure(&self, verb: &str) -> String {
        match self.missing_box_command() {
            Some(message) => message.to_string(),
            None => format!(
                "ghostex box {verb} printed no answer: {}",
                self.failure_tail()
            ),
        }
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
