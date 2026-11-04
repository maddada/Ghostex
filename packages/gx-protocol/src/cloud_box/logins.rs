//! Agent logins for boxes (Settings › Cloud Boxes › Agent logins, plan §2.5).
//!
//! D5: a Claude account is allowed on boxes with a one-year `claude setup-token` value ("box
//! token") that never rotates, so the computer's own login is never signed out. D6: a Codex
//! account is allowed as is; its `auth.json` is refreshed on the computer and copied to the box at
//! boot. Tokens live in home gxserver's vault and never appear in these rows.

use serde::{Deserialize, Serialize};

open_string_enum! {
    /// Where one account's box login stands.
    CloudAgentLoginState {
        /// Not offered on boxes.
        NotAllowed => "notAllowed",
        /// `claude setup-token` is waiting for the browser approval (`authUrl`).
        Waiting => "waiting",
        /// Offered on boxes (Claude: a box token is saved).
        Ready => "ready",
        /// The last try failed; `detail` says why.
        Error => "error",
    }
}

impl Default for CloudAgentLoginState {
    fn default() -> Self {
        Self::NotAllowed
    }
}

/// One saved Claude or Codex account as Cloud Boxes sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudAgentLogin {
    /// The account id of Settings › Accounts (`BoxInjectedAccount::account_id`).
    pub account_id: String,
    /// `claude` or `codex`.
    pub agent: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub name: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: CloudAgentLoginState,
    /// The page `claude setup-token` asks the user to approve, while `waiting`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    /// A sentence a user reads: the failure, or what a ready login can't do on a box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// ISO-8601: when the box token was saved (Claude) or the account was allowed (Codex).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_at: Option<String>,
}

/// `agentLogins` / `setAgentLogin` result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoginsResult {
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub logins: Vec<CloudAgentLogin>,
}

/// `setAgentLogin`: allow (Claude: start `claude setup-token`, which opens the approval page in
/// this computer's browser and is answered `waiting`) or stop offering an account on boxes (its
/// box token is forgotten).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAgentLoginParams {
    pub account_id: String,
    pub allowed: bool,
}
