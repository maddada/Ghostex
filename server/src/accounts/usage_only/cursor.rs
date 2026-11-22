//! Cursor: a browser sign-in Ghostex waits on (nothing to paste), and the plan usage of the
//! current billing month from Cursor's dashboard service.

use serde_json::{json, Value};

use super::http::{self, agent, answer, Failure};
use super::signin::{Progress, SignedIn};
use super::store::{Credential, UsageAccount};
use super::{Detail, Reading};
use crate::accounts::model::UsageWindow;

const NAME: &str = "Cursor";
/// The client id Cursor's own CLI signs in and renews with.
const CLIENT_ID: &str = "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB";
const DASHBOARD: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService";
/// Cursor's tokens last about 60 days; renew a week before they end.
const RENEW_BEFORE_MS: i64 = 7 * 24 * 60 * 60 * 1000;

pub(crate) fn sign_in(progress: &Progress) -> Result<SignedIn, Failure> {
    let (verifier, challenge) = http::pkce();
    let uuid = uuid::Uuid::new_v4().to_string();
    progress.show_url(
        &format!(
            "https://cursor.com/loginDeepControl?challenge={challenge}&uuid={uuid}&mode=login&redirectTarget=cli"
        ),
        false,
    );
    let poll = format!("https://api2.cursor.sh/auth/poll?uuid={uuid}&verifier={verifier}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    let (access, refresh) = loop {
        if progress.wait(std::time::Duration::from_secs(2)) {
            return Err(Failure::Unavailable("Sign-in cancelled.".into()));
        }
        if std::time::Instant::now() > deadline {
            return Err(Failure::Unavailable(
                "The sign-in took too long. Try again.".into(),
            ));
        }
        let reply = answer(agent().get(&poll).call(), NAME)?;
        if matches!(reply.status, 202 | 404) {
            continue;
        }
        let access = reply.body["accessToken"].as_str().unwrap_or_default();
        let refresh = reply.body["refreshToken"].as_str().unwrap_or_default();
        if !reply.ok() || access.is_empty() || refresh.is_empty() {
            return Err(Failure::Unavailable(
                "Cursor sign-in failed. Try again.".into(),
            ));
        }
        break (access.to_string(), refresh.to_string());
    };
    let user_id = user_id(&access)
        .ok_or_else(|| Failure::Unavailable("Cursor sent a sign-in Ghostex can't read.".into()))?;
    progress.saving();
    let profile = answer(
        agent()
            .get("https://cursor.com/api/auth/me")
            .header("Cookie", &session_cookie(&user_id, &access))
            .call(),
        NAME,
    )
    .ok()
    .filter(|reply| reply.ok())
    .map(|reply| reply.body)
    .unwrap_or(Value::Null);
    let email = profile["email"].as_str().unwrap_or_default().to_string();
    let name = profile["name"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Cursor".to_string());
    Ok(SignedIn {
        identity: format!("cursor:{user_id}"),
        name,
        email,
        credential: Credential {
            expires_at: expiry(&access),
            access_token: access,
            refresh_token: Some(refresh),
            extra: Default::default(),
        },
    })
}

/// The user id in the token's `sub` (`auth0|user_…` or the id alone).
fn user_id(token: &str) -> Option<String> {
    let sub = http::jwt_claims(token)?["sub"].as_str()?.trim().to_string();
    let id = sub.split('|').nth(1).unwrap_or(&sub).to_string();
    (!id.is_empty()).then_some(id)
}

fn expiry(token: &str) -> Option<i64> {
    http::jwt_claims(token)?["exp"]
        .as_i64()
        .map(|seconds| seconds * 1000)
}

fn session_cookie(user_id: &str, token: &str) -> String {
    format!("WorkosCursorSessionToken={user_id}%3A%3A{token}")
}

/// Trades the refresh token for a new access token, which also serves as the next refresh token.
fn renew(credential: &Credential) -> Result<Credential, Failure> {
    let refresh = credential
        .refresh_token
        .as_deref()
        .ok_or(Failure::SignInExpired)?;
    let reply = answer(
        agent()
            .post("https://api2.cursor.sh/oauth/token")
            .send_json(json!({"grant_type": "refresh_token", "client_id": CLIENT_ID, "refresh_token": refresh})),
        NAME,
    )?;
    if reply.body["shouldLogout"] == true || reply.refused() || reply.status == 400 {
        return Err(Failure::SignInExpired);
    }
    let access = reply.body["access_token"]
        .as_str()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| reply.unavailable(NAME))?;
    Ok(Credential {
        access_token: access.to_string(),
        refresh_token: Some(
            reply.body["refresh_token"]
                .as_str()
                .unwrap_or(access)
                .to_string(),
        ),
        expires_at: expiry(access),
        extra: Default::default(),
    })
}

fn dashboard(method: &str, token: &str) -> Result<http::Answer, Failure> {
    answer(
        agent()
            .post(&format!("{DASHBOARD}/{method}"))
            .header("Authorization", &format!("Bearer {token}"))
            .header("Connect-Protocol-Version", "1")
            .content_type("application/json")
            .send("{}"),
        NAME,
    )
}

pub(crate) fn read(account: &UsageAccount) -> Result<(Reading, Option<Credential>), Failure> {
    let mut credential = account.credential.clone();
    let mut renewed = false;
    if credential
        .expires_at
        .is_some_and(|at| at - http::now_ms() < RENEW_BEFORE_MS)
    {
        credential = renew(&credential)?;
        renewed = true;
    }
    let mut usage = dashboard("GetCurrentPeriodUsage", &credential.access_token)?;
    if usage.refused() && !renewed {
        credential = renew(&credential)?;
        renewed = true;
        usage = dashboard("GetCurrentPeriodUsage", &credential.access_token)?;
    }
    if usage.refused() {
        return Err(Failure::SignInExpired);
    }
    if !usage.ok() {
        return Err(usage.unavailable(NAME));
    }
    let plan = dashboard("GetPlanInfo", &credential.access_token)
        .ok()
        .filter(|reply| reply.ok())
        .and_then(|reply| {
            reply
                .body
                .pointer("/planInfo/planName")?
                .as_str()
                .map(title_case)
        });
    Ok((map(&usage.body, plan)?, renewed.then_some(credential)))
}

fn title_case(text: &str) -> String {
    text.split(|c: char| c == '_' || c == ' ' || c == '-')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or(String::new(), |first| {
                first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn dollars(cents: f64) -> String {
    format!("${:.2}", cents / 100.0)
}

/// CDXC:AgentProviders 2026-11-05 DECISION:
/// User picked "Mo" for Cursor's meter (Q1332): Cursor's plan has no 5h or 7d window, only the billing month, so its one usage window is labelled `Mo` (sidebar, usage bars, Settings), with the reset at the end of the billing cycle. The Auto and API split and on-demand spend are details under it.
fn map(usage: &Value, plan: Option<String>) -> Result<Reading, Failure> {
    if usage["enabled"] == false {
        return Err(Failure::Unavailable(
            "This Cursor account has no active plan.".into(),
        ));
    }
    let plan_usage = &usage["planUsage"];
    if !plan_usage.is_object() {
        return Err(Failure::Unavailable(
            "This Cursor account has no active plan.".into(),
        ));
    }
    let limit = http::number(&plan_usage["limit"]);
    let spent = http::number(&plan_usage["totalSpend"])
        .or_else(|| Some(limit? - http::number(&plan_usage["remaining"]).unwrap_or(0.0)));
    let percent = http::number(&plan_usage["totalPercentUsed"]).or_else(|| {
        let limit = limit.filter(|limit| *limit > 0.0)?;
        Some(spent? / limit * 100.0)
    });
    let resets_at =
        http::number(&usage["billingCycleEnd"]).and_then(|ms| http::rfc3339_from_ms(ms as i64));
    let mut windows = Vec::new();
    if let Some(percent) = percent {
        windows.push(UsageWindow {
            id: "month".into(),
            label: "Mo".into(),
            used_percent: http::clamp_percent(percent),
            limit_window_seconds: None,
            resets_at,
            model: None,
        });
    }
    let mut details = Vec::new();
    for (key, label) in [("autoPercentUsed", "Auto"), ("apiPercentUsed", "API")] {
        if let Some(value) = http::number(&plan_usage[key]) {
            details.push(Detail::new(label, format!("{}%", value.round())));
        }
    }
    if let (Some(spent), Some(limit)) = (spent, limit.filter(|limit| *limit > 0.0)) {
        details.push(Detail::new(
            "Plan spend",
            format!("{} of {}", dollars(spent), dollars(limit)),
        ));
    }
    let spend = &usage["spendLimitUsage"];
    let team = spend["limitType"].as_str() == Some("team")
        || http::number(&spend["pooledLimit"]).is_some_and(|limit| limit > 0.0);
    let (used, limit) = if team {
        (spend["pooledUsed"].clone(), spend["pooledLimit"].clone())
    } else {
        (
            spend["individualUsed"].clone(),
            spend["individualLimit"].clone(),
        )
    };
    if let (Some(used), Some(limit)) = (http::number(&used), http::number(&limit)) {
        if limit > 0.0 {
            details.push(Detail::new(
                if team { "Team spend" } else { "Extra usage" },
                format!("{} of {}", dollars(used), dollars(limit)),
            ));
        }
    }
    if windows.is_empty() && details.is_empty() {
        return Err(Failure::Unavailable(
            "Cursor didn't report this plan's usage.".into(),
        ));
    }
    Ok(Reading {
        usage: windows,
        plan,
        details,
    })
}
