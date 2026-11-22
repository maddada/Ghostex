//! Which saved Claude account a cloud session runs on, as the accounts menus read and change it
//! (`/api/agentAccounts` `session` and `select`, accounts/endpoint.rs): the sidebar row's Switch
//! Account flyout and the header's Claude cloud menu.
//!
//! CDXC:ClaudeCloud 2026-10-27 DECISION:
//! User: "please also allow me to pick which account to run the cloud session on somehow (like we allow for claude code regular local sessions)". A cloud session that has not started on claude.ai yet (waiting for its task, or its start failed) can move to another saved Claude account; its start then runs `claude --cloud` on that account's login (start.rs `runner_for`) and its claude.ai page opens in that account's Browser profile. A running session stays on its account: it lives there on claude.ai, so its accounts read reports the account and refuses a pick.

use rusqlite::Connection;
use serde_json::{json, Value};

use super::record::{session_cloud_record, CloudState};
use super::start::save;
use crate::accounts::model::Provider;
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

/// Whether the account of a cloud session in `state` can still change.
fn can_switch(state: CloudState) -> bool {
    matches!(state, CloudState::Draft | CloudState::Failed)
}

/// The `session` half of an accounts answer for a cloud session; `None` for every other session.
pub(crate) fn cloud_session_accounts(session: &Value) -> Option<Value> {
    let record = session_cloud_record(session)?;
    Some(json!({
        "provider": Provider::Claude,
        "accountId": record.raw.get("accountId").and_then(Value::as_str),
        "claudeCloud": true,
        "canSwitch": can_switch(record.state),
    }))
}

/// `select` on a cloud session: runs it on `account_id` from its start on. `None` for every other
/// session.
pub(crate) fn select_cloud_account(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    session: &Value,
    account_id: &str,
) -> Option<Result<(), DomainStateError>> {
    let mut record = session_cloud_record(session)?;
    Some((|| {
        if !can_switch(record.state) {
            return Err(DomainStateError::bad_request(
                "This session already runs in Claude's cloud on its account. Start a copy on another account instead.",
            ));
        }
        let registry = crate::accounts::store::read(db)?;
        registry
            .accounts
            .iter()
            .find(|account| {
                account.id == account_id
                    && account.provider == Provider::Claude
                    && !account.user_disabled
            })
            .ok_or_else(|| DomainStateError::not_found("That Claude account is not saved."))?;
        for key in ["accountId", "accountName", "accountEmail", "limitNotice"] {
            record.raw.remove(key);
        }
        record
            .raw
            .insert("accountId".to_string(), json!(account_id));
        record
            .raw
            .extend(super::accounts::account_facts(db, account_id));
        let text = |key: &str| session.get(key).and_then(Value::as_str).unwrap_or_default();
        save(
            state,
            db,
            repository,
            text("projectId"),
            text("sessionId"),
            &record,
        )
    })())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(state: &str, account: Option<&str>) -> Value {
        let mut record = json!({ "state": state });
        if let Some(account) = account {
            record["accountId"] = json!(account);
        }
        json!({
            "projectId": "P1",
            "sessionId": "S1",
            "runtimeSettings": { "claudeCloud": record },
        })
    }

    #[test]
    fn a_cloud_session_reports_its_account_and_whether_it_can_move() {
        let draft = cloud_session_accounts(&session("draft", Some("claude-a"))).unwrap();
        assert_eq!(draft["provider"], "claude");
        assert_eq!(draft["accountId"], "claude-a");
        assert_eq!(draft["canSwitch"], true);
        assert_eq!(
            cloud_session_accounts(&session("failed", None)).unwrap()["canSwitch"],
            true
        );
        for state in ["starting", "running", "moved"] {
            assert_eq!(
                cloud_session_accounts(&session(state, Some("claude-a"))).unwrap()["canSwitch"],
                false,
                "{state}"
            );
        }
        assert!(cloud_session_accounts(&json!({ "runtimeSettings": {} })).is_none());
    }
}
