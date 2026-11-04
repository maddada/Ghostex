use super::{helpers, model::ResetCredit, reset_claim::Outcome};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Mutex};

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
            note: None,
            paused: false,
            requires_limit: false,
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

/// CDXC:AgentProviders 2026-09-24 WHY:
/// A retried idempotency key replays the credit it first matched instead of re-reading the list: after a consume whose reply was lost the credit is already gone from the list, and only the replay lets the server answer `already_redeemed`.
static MATCHED: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Claims one Codex reset credit, the protocol the Codex CLI uses: a fresh read of the credit list confirms the chosen credit is still available, then the consume targets exactly that credit under the caller's idempotency key.
pub(crate) fn claim(row: &Value, credit_id: &str, request_id: &str) -> Outcome {
    let replay = MATCHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(request_id)
        .cloned();
    let credit_id = match replay {
        Some(id) => id,
        None => {
            let credits = match read(row) {
                Ok(credits) => credits,
                Err(error) => return Outcome::failed(&error),
            };
            if !credits.iter().any(|credit| credit.id == credit_id) {
                return Outcome::no_credit("That reset is no longer available.");
            }
            MATCHED
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(HashMap::new)
                .insert(request_id.to_string(), credit_id.to_string());
            credit_id.to_string()
        }
    };
    let request =
        match helpers::codex_request(row, ureq::Agent::post, "rate-limit-reset-credits/consume") {
            Ok(request) => request,
            Err(error) => return Outcome::failed(&error),
        };
    let response = match request
        .header("Content-Type", "application/json")
        .send_json(json!({"redeem_request_id":request_id,"credit_id":credit_id}))
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(401 | 403)) => {
            return Outcome::failed("Sign in again to use this reset.")
        }
        Err(_) => {
            return Outcome::failed(
                "The reset could not be confirmed. Check your limits before trying again.",
            )
        }
    };
    let code = crate::http_agent::read_json::<Value>(response)
        .ok()
        .and_then(|body| body["code"].as_str().map(str::to_string));
    match code.as_deref() {
        Some("reset" | "already_redeemed") => Outcome::success(),
        Some("nothing_to_reset") => {
            Outcome::nothing_to_reset("Your usage doesn't need a reset yet. Nothing was used.")
        }
        Some("no_credit") => Outcome::no_credit("That reset is no longer available."),
        _ => Outcome::failed(
            "The reset could not be confirmed. Check your limits before trying again.",
        ),
    }
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
