use super::{
    helpers, launch,
    model::{Provider, ResetCredit},
    store,
};
use crate::{domain::DomainStateError, server::AppState};
use serde_json::{json, Value};

/// CDXC:AgentProviders 2026-09-11 WHY:
/// The usage response includes only a reset count. Expiry dates require the separate read-only credit list; failure to read that list must not hide account limits.
pub(crate) fn read(row: &Value) -> Result<Vec<ResetCredit>, String> {
    parse(
        &helpers::codex_get(row, "rate-limit-reset-credits")?,
        chrono::Utc::now(),
    )
}

fn parse(value: &Value, now: chrono::DateTime<chrono::Utc>) -> Result<Vec<ResetCredit>, String> {
    let rows = value["credits"]
        .as_array()
        .ok_or("Reset expiry details are unavailable.")?;
    let mut credits = Vec::new();
    for row in rows {
        if row["status"].as_str().is_some_and(|s| s != "available") {
            continue;
        }
        if row["reset_type"]
            .as_str()
            .is_some_and(|s| s != "codex_rate_limits")
        {
            continue;
        }
        let id = row["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("A reset has no identifier.")?;
        let expires = if row["expires_at"].is_null() {
            None
        } else {
            Some(
                row["expires_at"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|date| date.with_timezone(&chrono::Utc))
                    .or_else(|| {
                        row["expires_at"]
                            .as_i64()
                            .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                    })
                    .ok_or("A reset expiry could not be read.")?,
            )
        };
        if expires.is_some_and(|date| date <= now) {
            continue;
        }
        credits.push(ResetCredit {
            id: id.to_string(),
            expires_at: expires.map(|date| date.to_rfc3339()),
        });
    }
    credits.sort_by(|a, b| match (&a.expires_at, &b.expires_at) {
        (Some(a), Some(b)) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    });
    Ok(credits)
}

pub(crate) fn prepare(state: &AppState, id: &str) -> Result<Value, DomainStateError> {
    let _gate = state.accounts.mutations.lock().map_err(store::error)?;
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
    let registry = store::read(&db)?;
    let account = registry
        .accounts
        .iter()
        .find(|a| a.id == id && a.provider == Provider::Codex)
        .ok_or_else(|| DomainStateError::bad_request("Choose a saved Codex account."))?;
    launch::validate_identity(&state.paths.home_dir, account)?;
    let accounts = helpers::json_command(&state.paths.home_dir, "xswap", &["list", "--json"])
        .map_err(DomainStateError::bad_request)?;
    let row = accounts["accounts"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["number"].as_u64().map(|n| n.to_string()).as_deref()
                    == Some(account.selector.as_str())
            })
        })
        .ok_or_else(|| {
            DomainStateError::bad_request("The selected account is no longer available.")
        })?;
    if row["accountId"].as_str() != Some(account.identity.as_str()) {
        return Err(DomainStateError::bad_request(
            "The saved account changed. Refresh Accounts before redeeming a reset.",
        ));
    }
    let credits = read(row).map_err(DomainStateError::bad_request)?;
    let credits: Vec<_> = credits
        .iter()
        .map(|credit| {
            let picker_expiry = credit
                .expires_at
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|date| {
                    format!(
                        "Expires {}.",
                        date.with_timezone(&chrono::Local)
                            .format("%H:%M on %-d %b %Y")
                    )
                });
            json!({"id":credit.id,"expiresAt":credit.expires_at,"pickerExpiry":picker_expiry})
        })
        .collect();
    Ok(json!({"accountId":account.id,"credits":credits}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn available_resets_use_expiry_order_and_ignore_consumed_or_expired() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-11T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let credits = parse(
            &json!({"credits":[
                {"id":"later","expires_at":"2026-10-05T08:18:00Z"},
                {"id":"unlimited","expires_at":null},
                {"id":"first","expires_at":"2026-10-04T06:02:00Z","status":"available"},
                {"id":"used","status":"redeemed"},
                {"id":"expired","expires_at":"2026-09-10T00:00:00Z"}
            ]}),
            now,
        )
        .unwrap();
        assert_eq!(
            credits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["first", "later", "unlimited"]
        );
        assert!(parse(
            &json!({"credits":[{"id":"broken","expires_at":"bad"}]}),
            now
        )
        .is_err());
    }
}
