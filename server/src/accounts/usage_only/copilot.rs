//! GitHub Copilot: GitHub's device sign-in (a code to type on github.com), and the account's
//! Copilot quota for the month.

use super::http::{self, agent, answer, Failure};
use super::signin::{Progress, SignedIn};
use super::store::{Credential, UsageAccount};
use super::{Detail, Reading};
use crate::accounts::model::UsageWindow;

const NAME: &str = "Copilot";
/// The client id GitHub Copilot's own editor sign-in uses; its tokens can read the Copilot quota.
const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

pub(crate) fn sign_in(progress: &Progress) -> Result<SignedIn, Failure> {
    let start = answer(
        agent()
            .post("https://github.com/login/device/code")
            .header("Accept", "application/json")
            .send_form([("client_id", CLIENT_ID), ("scope", "read:user")]),
        NAME,
    )?;
    let (Some(device_code), Some(user_code)) = (
        start.body["device_code"].as_str(),
        start.body["user_code"].as_str(),
    ) else {
        return Err(start.unavailable("GitHub sign-in"));
    };
    let url = start.body["verification_uri"]
        .as_str()
        .unwrap_or("https://github.com/login/device");
    progress.show_code(url, user_code);
    let mut interval = start.body["interval"].as_u64().unwrap_or(5).max(1);
    let expires = start.body["expires_in"].as_u64().unwrap_or(900);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(expires);
    let token = loop {
        if progress.wait(std::time::Duration::from_secs(interval)) {
            return Err(Failure::Unavailable("Sign-in cancelled.".into()));
        }
        if std::time::Instant::now() > deadline {
            return Err(Failure::Unavailable("The code expired. Try again.".into()));
        }
        let reply = answer(
            agent()
                .post("https://github.com/login/oauth/access_token")
                .header("Accept", "application/json")
                .send_form([
                    ("client_id", CLIENT_ID),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ]),
            NAME,
        )?;
        if let Some(token) = reply.body["access_token"].as_str() {
            break token.to_string();
        }
        match reply.body["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some("expired_token") => {
                return Err(Failure::Unavailable("The code expired. Try again.".into()))
            }
            Some("access_denied") => {
                return Err(Failure::Unavailable(
                    "The sign-in was declined on GitHub.".into(),
                ))
            }
            _ => return Err(reply.unavailable("GitHub sign-in")),
        }
    };
    progress.saving();
    let user = answer(
        agent()
            .get("https://api.github.com/user")
            .header("Authorization", &format!("token {token}"))
            .header("Accept", "application/json")
            .call(),
        NAME,
    )?;
    let id = user.body["id"]
        .as_u64()
        .ok_or_else(|| user.unavailable("GitHub"))?;
    let login = user.body["login"].as_str().unwrap_or("GitHub");
    Ok(SignedIn {
        identity: format!("copilot:{id}"),
        name: login.to_string(),
        email: user.body["email"].as_str().unwrap_or_default().to_string(),
        credential: Credential {
            access_token: token,
            ..Default::default()
        },
    })
}

pub(crate) fn read(account: &UsageAccount) -> Result<(Reading, Option<Credential>), Failure> {
    let reply = answer(
        agent()
            .get("https://api.github.com/copilot_internal/user")
            .header(
                "Authorization",
                &format!("token {}", account.credential.access_token),
            )
            .header("Accept", "application/json")
            .header("Editor-Version", "vscode/1.96.2")
            .header("Editor-Plugin-Version", "copilot-chat/0.26.7")
            .header("X-Github-Api-Version", "2025-04-01")
            .call(),
        NAME,
    )?;
    if reply.status == 401 {
        return Err(Failure::SignInExpired);
    }
    if reply.status == 404 || reply.status == 403 {
        return Err(Failure::Unavailable(
            "This GitHub account has no Copilot plan.".into(),
        ));
    }
    if !reply.ok() {
        return Err(reply.unavailable(NAME));
    }
    Ok((map(&reply.body)?, None))
}

/// The quota's reset day (`2026-12-01` or a full timestamp) as RFC 3339.
fn reset(value: &serde_json::Value) -> Option<String> {
    let text = value.as_str()?.trim();
    if let Ok(at) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(at.to_rfc3339());
    }
    let day = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?;
    Some(day.and_hms_opt(0, 0, 0)?.and_utc().to_rfc3339())
}

/// Percent used of one quota snapshot; `None` for an unlimited or empty one.
fn snapshot_percent(snapshot: &serde_json::Value) -> Option<f64> {
    if !snapshot.is_object() || snapshot["unlimited"] == true {
        return None;
    }
    let entitlement = http::number(&snapshot["entitlement"]);
    let remaining = http::number(&snapshot["remaining"]);
    if entitlement == Some(-1.0) || remaining == Some(-1.0) || entitlement == Some(0.0) {
        return None;
    }
    if let Some(left) = http::number(&snapshot["percent_remaining"]) {
        return Some(http::clamp_percent(100.0 - left));
    }
    let (entitlement, remaining) = (entitlement.filter(|e| *e > 0.0)?, remaining?);
    Some(http::clamp_percent(100.0 - remaining / entitlement * 100.0))
}

/// Copilot's quotas reset each month, like Cursor's, so the headline window is `Mo`
/// (CDXC:AgentProviders 2026-11-05 on `cursor::map`): premium requests, or chat on a plan without them.
fn map(body: &serde_json::Value) -> Result<Reading, Failure> {
    let resets_at =
        reset(&body["quota_reset_date"]).or_else(|| reset(&body["limited_user_reset_date"]));
    let plan = body["copilot_plan"]
        .as_str()
        .filter(|plan| !plan.trim().is_empty())
        .map(|plan| {
            plan.split(['_', ' ', '-'])
                .filter(|word| !word.is_empty())
                .map(|word| {
                    let mut chars = word.chars();
                    chars.next().map_or(String::new(), |first| {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    })
                })
                .collect::<Vec<_>>()
                .join(" ")
        });
    let snapshots = &body["quota_snapshots"];
    let mut pools: Vec<(&str, f64)> = Vec::new();
    for (key, label) in [
        ("premium_interactions", "Premium requests"),
        ("chat", "Chat"),
        ("completions", "Completions"),
    ] {
        if let Some(percent) = snapshot_percent(&snapshots[key]) {
            pools.push((label, percent));
        }
    }
    if pools.is_empty() {
        // Free plans report what is left and the monthly totals instead of snapshots.
        for (key, label) in [("chat", "Chat"), ("completions", "Completions")] {
            let total = http::number(&body["monthly_quotas"][key]).filter(|t| *t > 0.0);
            let left = http::number(&body["limited_user_quotas"][key]);
            if let (Some(total), Some(left)) = (total, left) {
                pools.push((
                    label,
                    http::clamp_percent((total - left).max(0.0) / total * 100.0),
                ));
            }
        }
    }
    let Some((_, headline)) = pools.first().copied() else {
        return Err(Failure::Unavailable(
            "Copilot didn't report a limit for this plan.".into(),
        ));
    };
    let mut details: Vec<Detail> = pools
        .iter()
        .map(|(label, percent)| Detail::new(label, format!("{}%", percent.round())))
        .collect();
    let premium = &snapshots["premium_interactions"];
    if premium["overage_permitted"] == true {
        if let Some(overage) = http::number(&premium["overage_count"]).filter(|o| *o > 0.0) {
            details.push(Detail::new(
                "Extra requests",
                format!("{}", overage.round()),
            ));
        }
    }
    Ok(Reading {
        usage: vec![UsageWindow {
            id: "month".into(),
            label: "Mo".into(),
            used_percent: headline,
            limit_window_seconds: None,
            resets_at,
            model: None,
        }],
        plan,
        details,
    })
}
