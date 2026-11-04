use super::{
    claude_resets, helpers, launch,
    model::{Provider, SavedAccount},
    reset_credits, store,
};
use crate::{domain::DomainStateError, server::AppState};
use serde_json::{json, Map, Value};
use std::{collections::HashSet, sync::Mutex, time::Instant};

/// A claim result as the usage panel renders it: `success` spent the reset, `nothingToReset` and `noCredit` spent nothing, and `failed` may need a check before retrying.
pub(crate) struct Outcome {
    kind: &'static str,
    message: String,
}

impl Outcome {
    pub(crate) fn success() -> Self {
        Self {
            kind: "success",
            message: "Reset used. Your limits are fresh again.".into(),
        }
    }
    pub(crate) fn nothing_to_reset(message: &str) -> Self {
        Self {
            kind: "nothingToReset",
            message: message.into(),
        }
    }
    pub(crate) fn no_credit(message: &str) -> Self {
        Self {
            kind: "noCredit",
            message: message.into(),
        }
    }
    pub(crate) fn failed(message: &str) -> Self {
        Self {
            kind: "failed",
            message: message.into(),
        }
    }
    pub(crate) fn kind(&self) -> &'static str {
        self.kind
    }
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

static IN_FLIGHT: Mutex<Option<HashSet<String>>> = Mutex::new(None);

struct InFlight(String);
impl Drop for InFlight {
    fn drop(&mut self) {
        if let Some(set) = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            set.remove(&self.0);
        }
    }
}

/// CDXC:AgentProviders 2026-09-24 DECISION:
/// User: redeeming a reset must not open a CLI chat (the Codex `/usage` picker got stuck on its first screen); it works exactly like OpenUsage, claiming the chosen reset directly from the usage panel, and Claude resets are claimable the same way. This supersedes the 2026-09-11 decision that redemption opens a terminal chat in the active project.
/// The claim targets the reset the user picked, carries the panel's idempotency key, and refreshes account usage before answering so the result appears over the updated limits.
pub(crate) fn redeem(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let field = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty() && s.len() <= 128)
            .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
    };
    let (id, credit_id, request_id) = (field("id")?, field("creditId")?, field("requestId")?);
    if request_id.len() > 64
        || !request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(DomainStateError::bad_request("requestId is invalid."));
    }
    let account = {
        let _gate = state.accounts.mutation_gate();
        let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
        store::read(&db)?
            .accounts
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| DomainStateError::bad_request("Choose a saved account."))?
    };
    let outcome = claim_for_account(state, &account, credit_id, request_id)?;
    Ok(json!({"outcome":outcome.kind,"message":outcome.message}))
}

/// Claims `credit_id` for a saved account: the usage panel's redeem and the reset watcher's automatic redeem both come through here, so the identity check, the one-claim-per-account guard and the post-claim usage refresh are the same for both.
pub(crate) fn claim_for_account(
    state: &AppState,
    account: &SavedAccount,
    credit_id: &str,
    request_id: &str,
) -> Result<Outcome, DomainStateError> {
    let home = &state.paths.home_dir;
    launch::validate_identity(home, account)?;
    let _in_flight = {
        let mut set = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        if !set
            .get_or_insert_with(HashSet::new)
            .insert(account.id.clone())
        {
            return Err(DomainStateError::bad_request(
                "A reset is already being used for this account.",
            ));
        }
        InFlight(account.id.clone())
    };
    let started = Instant::now();
    let outcome = match account.provider {
        Provider::Codex => {
            let accounts = helpers::json_command(home, "xswap", &["list", "--json"])
                .map_err(DomainStateError::bad_request)?;
            let row = accounts["accounts"]
                .as_array()
                .and_then(|rows| {
                    rows.iter().find(|row| {
                        row["number"].as_u64().map(|n| n.to_string()).as_deref()
                            == Some(account.selector.as_str())
                    })
                })
                .filter(|row| row["accountId"].as_str() == Some(account.identity.as_str()))
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "The saved account changed. Refresh Accounts before using a reset.",
                    )
                })?;
            reset_credits::claim(row, credit_id, request_id)
        }
        Provider::Claude => {
            let slot = claude_resets::ClaudeSlot::read(home, &account.selector)
                .map_err(DomainStateError::bad_request)?;
            if slot.identity() != account.identity {
                return Err(DomainStateError::bad_request(
                    "The saved account changed. Refresh Accounts before using a reset.",
                ));
            }
            claude_resets::claim(home, &slot, credit_id, request_id)
        }
    };
    state.accounts.refresh_since(home, started);
    Ok(outcome)
}
