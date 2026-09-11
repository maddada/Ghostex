use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Provider {
    Claude,
    Codex,
}
impl Provider {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
    pub(crate) fn helper(self) -> &'static str {
        match self {
            Self::Claude => "cswap",
            Self::Codex => "xswap",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Policy {
    pub enabled: bool,
    pub at_limit: LimitAction,
    pub priority: Priority,
    pub retry_errors: bool,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LimitAction {
    Wait,
    Switch,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Priority {
    LeastUsed,
    MostUsed,
    SoonestReset,
    LatestReset,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            at_limit: LimitAction::Wait,
            priority: Priority::SoonestReset,
            retry_errors: true,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedAccount {
    pub id: String,
    pub provider: Provider,
    pub selector: String,
    pub identity: String,
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub indicator: String,
    #[serde(default)]
    pub show_in_titlebar: bool,
    pub eligible: bool,
    pub shared_history: bool,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Registry {
    #[serde(default)]
    pub accounts: Vec<SavedAccount>,
    #[serde(default)]
    pub defaults: BTreeMap<Provider, Policy>,
    /// CDXC:AgentProviders 2026-09-11 DECISION:
    /// User: the new-session account choice is stored under `newSessionAccounts`, a different key from the pre-9.3 `defaultAccounts`, so every choice saved before the automatic rules existed is dropped on read and those users migrate to Most limit remaining. A missing entry means Most limit remaining.
    #[serde(default)]
    pub new_session_accounts: BTreeMap<Provider, NewSessionAccount>,
    /// The account of the most recent session launched or switched per provider, kept for the Same as last session rule.
    #[serde(default)]
    pub last_used_accounts: BTreeMap<Provider, String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "rule")]
pub(crate) enum NewSessionAccount {
    MostRemaining,
    SoonestReset,
    MostUsed,
    LastUsed,
    Pinned { id: String },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageWindow {
    pub id: String,
    pub label: String,
    pub used_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_window_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}
#[derive(Clone)]
pub(crate) struct DiscoveredAccount {
    pub provider: Provider,
    pub selector: String,
    pub identity: String,
    pub name: String,
    pub email: String,
    pub status: String,
    pub shared_history: bool,
    pub usage: Vec<UsageWindow>,
    pub reset_credits: Option<u64>,
    pub reset_credit_details: Option<Vec<ResetCredit>>,
    pub reset_credits_error: Option<String>,
    pub usage_updated_at: Option<String>,
    pub usage_error: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResetCredit {
    pub id: String,
    pub expires_at: Option<String>,
}
#[derive(Clone, Default)]
pub(crate) struct Snapshot {
    pub accounts: Vec<DiscoveredAccount>,
    pub errors: BTreeMap<Provider, String>,
    pub fetched_at: Option<std::time::Instant>,
}
pub(crate) fn color_hex(color: &str) -> Option<&'static str> {
    Some(match color {
        "neutral" => "#dddddd",
        "slate" => "#a8b4c3",
        "coral" => "#db967e",
        "rose" => "#d598b2",
        "lavender" => "#b5a0d6",
        "sky" => "#8db7dc",
        "teal" => "#81b8b2",
        "sage" => "#a6bc91",
        "sand" => "#d1bd8b",
        _ => return None,
    })
}
