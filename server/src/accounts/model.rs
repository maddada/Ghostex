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
    /// CDXC:AgentProviders 2026-09-18 DECISION:
    /// User: Claude Swap and Codex Swap must install on Windows too, under both the PowerShell and the WSL terminal backend.
    /// The backend setting picks which gxserver runs — the native Windows build or the Linux build inside the distribution — so the platform this code is compiled for is already the user's choice and no setting is read here.
    /// Codex Swap has no Linux install script, so Linux and WSL keep Homebrew (with the tap-trust step its README requires) and fall back to a source build only where brew is absent, which is the common case inside WSL.
    /// Claude Swap is a Python tool and installs the same way everywhere.
    /// SEE-ALSO: packages/shared/ghostex-settings/types.ts (deleted 2026-10-01) (windowsTerminalBackend), apps/desktop/src/windows_terminal_backend/platform/.
    pub(crate) fn install_command(self, home: &std::path::Path) -> String {
        match self {
            // CDXC:AgentProviders 2026-10-16 DECISION in claude_swap_windows.rs: Ghostex's own build on Windows.
            Self::Claude if super::claude_swap_windows::applies() => format!(
                "uv tool install --force {}",
                super::claude_swap_windows::URL
            ),
            Self::Claude => "uv tool install claude-swap".to_string(),
            Self::Codex if cfg!(windows) => {
                "irm https://github.com/maddada/codex-swap/releases/latest/download/install.ps1 | iex"
                    .to_string()
            }
            Self::Codex if super::helpers::executable(home, "brew").is_some() => {
                "brew tap maddada/tap && brew trust --formula maddada/tap/codex-swap && brew install maddada/tap/codex-swap".to_string()
            }
            Self::Codex => {
                "cargo install --git https://github.com/maddada/codex-swap --locked".to_string()
            }
        }
    }
    pub(crate) fn login_command(self) -> &'static str {
        match self {
            Self::Claude => "ghostex account-login claude",
            Self::Codex => "xswap add --login --share-history",
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
/// CDXC:AgentProviders 2026-10-08 DECISION:
/// User: "please turn it on and make it default to on". Continue automatically starts on with At a limit set to Use another account for both providers, so a session moves to another account set to Automatic when its account hits a limit. Supersedes the off-and-wait default from 2026-09-06; a provider whose Settings were already saved keeps its saved choice. SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/accounts/data.rs (`defaults`), packages/shared/agent-accounts.ts (`DEFAULT_ACCOUNT_POLICY`).
impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: true,
            at_limit: LimitAction::Switch,
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
    /// Set when the login stopped working for good (see `disabled.rs`); cleared when the user sets the account back to Automatic or reconnects it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<DisabledAccount>,
    /// The user turned the account off in Settings > Accounts (Enabled switch). Unrelated to
    /// `disabled`, which gxserver sets after a refusal.
    ///
    /// CDXC:AgentProviders 2026-10-14 DECISION:
    /// User: "that account is deaded (not subscribed on it) so i didn't continue the connection. we need an Enable Disable account toggle so i can keep an account there but disable it". An account turned off stays saved but out of use: no rule, default, workspace or automatic switch picks it and a manual switch to it is refused; it is not polled for usage or sign-in state (gxserver reports it as status `disabled` with no usage, so nothing asks to reconnect it); its usage leaves the sidebar; clients leave it out of their switch menus (`userDisabled`). Sessions already on it keep running. Kept apart from the automatic Subscription access disabled state (`disabled`).
    /// SEE-ALSO: helpers.rs (`discover_row` skips it), store.rs (`user_disabled`), default_account.rs, recovery.rs, limit_wait.rs, launch.rs, endpoint.rs (`setEnabled`, `select`, `state_value`), packages/gx-core/src/sidebar_accounts/data.rs, packages/gx-chat-core/src/menus/native_accounts.rs, apps/desktop/src/app/new_thread_picker_lifecycle.rs, Settings > Accounts (`editor.rs`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user_disabled: bool,
    /// When a session on this account last reported that its login expired (login_expired.rs);
    /// cleared when the account signs in again or a session on it answers afterwards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_expired_at: Option<String>,
    /// When the account last signed in through Ghostex or was last seen working after a mark: a
    /// login error from before then never marks it again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_confirmed_at: Option<String>,
}
impl SavedAccount {
    /// Automatic switching and the automatic new-session rules may pick this account.
    pub(crate) fn auto_switchable(&self) -> bool {
        self.eligible && !self.user_disabled && self.login_expired_at.is_none()
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DisabledAccount {
    pub reason: String,
    pub at: String,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Registry {
    #[serde(default)]
    pub accounts: Vec<SavedAccount>,
    #[serde(default)]
    pub defaults: BTreeMap<Provider, Policy>,
    /// CDXC:AgentProviders 2026-09-11 DECISION:
    /// User: the new-session account choice is stored under `newSessionAccounts`, a different key from the pre-9.3 `defaultAccounts`, so every choice saved before the automatic rules existed is dropped on read and those users migrate to Auto. A missing entry means Auto.
    #[serde(default)]
    pub new_session_accounts: BTreeMap<Provider, NewSessionAccount>,
    /// The account of the most recent session launched or switched per provider, kept for the Same as last session rule.
    #[serde(default)]
    pub last_used_accounts: BTreeMap<Provider, String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "rule")]
pub(crate) enum NewSessionAccount {
    Auto,
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
    /// `usage` is the last reading that refreshed successfully, not a current one; `usage_updated_at` is when that reading was taken.
    pub usage_stale: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResetCredit {
    pub id: String,
    pub expires_at: Option<String>,
    /// Why this reset may not work right now (paused, or only usable at a limit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Anthropic paused the grant: it cannot be claimed now.
    #[serde(skip)]
    pub paused: bool,
    /// The claim succeeds only while the account is at a usage limit.
    #[serde(skip)]
    pub requires_limit: bool,
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
