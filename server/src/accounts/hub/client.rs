//! Calls to a CLIProxyAPI hub: the client key check when a hub is added, and the management API
//! that lists the hub's accounts (`/v0/management/auth-files`) and reads each one's usage through
//! the hub (`/v0/management/api-call`, which sends the request with that account's token in place
//! of `$TOKEN$`, so no token ever reaches Ghostex).
//!
//! CDXC:AgentProviders 2026-11-18 WHY:
//! Only reads go to the management API. Ghostex never downloads, uploads, deletes or refreshes a hub's credentials, never asks it to sign an account in, and never redeems a Codex reset credit through it: the hub is the user's, and it holds the tokens.

use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::accounts::model::UsageWindow;

/// Every status comes back as an answer, short timeouts: a hub is usually on this computer or
/// the user's network, and Settings waits on the first read.
fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        crate::http_agent::config()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent("Ghostex")
            .http_status_as_error(false)
            .build()
            .into()
    })
}

struct Answer {
    status: u16,
    body: Value,
}

fn answer(result: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Answer, String> {
    let mut response = result.map_err(|error| {
        format!(
            "The hub can't be reached ({error}). Check that it is running and its address is right."
        )
    })?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .with_config()
        .limit(8 * 1024 * 1024)
        .read_to_string()
        .unwrap_or_default();
    Ok(Answer {
        status,
        body: serde_json::from_str(&text).unwrap_or(Value::String(text)),
    })
}

/// The hub's own error text (`{"error": "…"}`), trimmed for a sentence.
fn hub_error(body: &Value) -> Option<String> {
    let text = body
        .get("error")
        .and_then(|error| error.as_str().or_else(|| error.get("message")?.as_str()))?;
    let text = masked(text);
    (!text.is_empty()).then_some(text)
}

/// Text a hub wrote (an error, an account's status message), trimmed for a sentence with every
/// long unbroken run of letters and digits cut out.
///
/// CDXC:AgentProviders 2026-11-19 WHY:
/// A message shows only the `error` field of a JSON answer, never a raw body (a proxy's page, or an answer that repeats a request), and any run of 16 or more letters, digits, dashes or underscores is cut out (CLIProxyAPI client keys, OAuth tokens and each part of a JWT are longer; a host name's dots keep it readable), so a key or token a hub or a proxy in front of it echoes never reaches Settings, the CLI or a log. The same goes for an account's `status_message`, which repeats what the provider answered.
pub(crate) fn masked(text: &str) -> String {
    let masked: Vec<String> = text
        .trim()
        .split(' ')
        .map(|word| {
            let run = word
                .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_')))
                .map(str::len)
                .max()
                .unwrap_or(0);
            if run >= 16 {
                "…".to_string()
            } else {
                word.to_string()
            }
        })
        .collect();
    masked.join(" ").chars().take(200).collect()
}

/// Checks that the hub answers its client API with `client_key` (`GET /v1/models`).
pub(crate) fn check_client_key(url: &str, client_key: &str) -> Result<(), String> {
    let answer = answer(
        agent()
            .get(format!("{url}/v1/models"))
            .header("Authorization", format!("Bearer {client_key}"))
            .call(),
    )?;
    match answer.status {
        200..=299 => Ok(()),
        401 | 403 => Err(
            "The hub refused the client key. Use one of the keys under api-keys in its config."
                .to_string(),
        ),
        404 => Err(
            "That address answers, but not like a CLIProxyAPI hub. Check the address and port."
                .to_string(),
        ),
        status => Err(format!(
            "The hub answered with an error (HTTP {status}){}.",
            hub_error(&answer.body)
                .map(|text| format!(": {text}"))
                .unwrap_or_default()
        )),
    }
}

fn management_get(url: &str, key: &str, path: &str) -> Result<Value, String> {
    let answer = answer(
        agent()
            .get(format!("{url}/v0/management/{path}"))
            .header("Authorization", format!("Bearer {key}"))
            .call(),
    )?;
    management_answer(answer)
}

fn management_answer(answer: Answer) -> Result<Value, String> {
    match answer.status {
        200..=299 => Ok(answer.body),
        401 => Err("The hub refused the management key.".to_string()),
        403 => Err(format!(
            "The hub's management API refused Ghostex{}. It answers this computer only when remote management is allowed in the hub's config, or when the hub runs here.",
            hub_error(&answer.body).map(|text| format!(" ({text})")).unwrap_or_default()
        )),
        404 => Err("The hub's management API is turned off. Set a secret-key under remote-management in its config.".to_string()),
        status => Err(format!(
            "The hub's management API answered with an error (HTTP {status}){}.",
            hub_error(&answer.body).map(|text| format!(": {text}")).unwrap_or_default()
        )),
    }
}

/// The hub's accounts (`files` of `/v0/management/auth-files`).
pub(crate) fn auth_files(url: &str, key: &str) -> Result<Vec<Value>, String> {
    let body = management_get(url, key, "auth-files")?;
    Ok(body
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// One request the hub makes with an account's token (`POST /v0/management/api-call`): the
/// provider's status and body.
fn api_call(
    url: &str,
    key: &str,
    auth_index: &str,
    target: &str,
    header: Map<String, Value>,
) -> Result<(u16, Value), String> {
    let answer = answer(
        agent()
            .post(format!("{url}/v0/management/api-call"))
            .header("Authorization", format!("Bearer {key}"))
            .send_json(json!({
                "auth_index": auth_index,
                "method": "GET",
                "url": target,
                "header": header,
            })),
    )?;
    let body = management_answer(answer)?;
    let status = body.get("status_code").and_then(Value::as_u64).unwrap_or(0) as u16;
    let text = body.get("body").and_then(Value::as_str).unwrap_or_default();
    Ok((status, serde_json::from_str(text).unwrap_or(Value::Null)))
}

/// Why one account's usage couldn't be read: the sentence to show, and whether the provider was
/// limiting reads (429), which waits longer before the next one.
pub(crate) struct UsageError {
    pub message: String,
    pub rate_limited: bool,
}

impl From<String> for UsageError {
    fn from(message: String) -> Self {
        Self {
            message,
            rate_limited: false,
        }
    }
}

/// One account's usage windows, read through the hub. `Ok(None)` for a provider Ghostex has no
/// usage read for (the account is listed without meters).
pub(crate) fn usage(
    url: &str,
    key: &str,
    file: &Value,
) -> Result<Option<Vec<UsageWindow>>, UsageError> {
    let provider = file
        .get("provider")
        .or_else(|| file.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let Some(auth_index) = file.get("auth_index").and_then(Value::as_str) else {
        return Ok(None);
    };
    let mut header = Map::new();
    header.insert("Authorization".into(), json!("Bearer $TOKEN$"));
    header.insert("Accept".into(), json!("application/json"));
    let (target, provider_name) = match provider.as_str() {
        "claude" => {
            header.insert("anthropic-beta".into(), json!("oauth-2025-04-20"));
            // Anthropic answers usage reads without Claude Code's agent with 429s.
            header.insert(
                "User-Agent".into(),
                json!(crate::accounts::claude_resets::USER_AGENT),
            );
            ("https://api.anthropic.com/api/oauth/usage", "Anthropic")
        }
        "codex" => {
            if let Some(account) = file
                .pointer("/id_token/chatgpt_account_id")
                .and_then(Value::as_str)
            {
                header.insert("ChatGPT-Account-Id".into(), json!(account));
            }
            header.insert("OpenAI-Beta".into(), json!("codex-1"));
            // The same headers Codex Swap usage reads send (helpers.rs `codex_get`).
            header.insert("originator".into(), json!("Codex Desktop"));
            ("https://chatgpt.com/backend-api/wham/usage", "ChatGPT")
        }
        _ => return Ok(None),
    };
    let (status, body) = api_call(url, key, auth_index, target, header)?;
    match status {
        200..=299 => {}
        401 | 403 => {
            return Err(format!(
                "{provider_name} refused this account's login. Sign it in again on the hub."
            )
            .into())
        }
        429 => {
            return Err(UsageError {
                message: format!(
                    "{provider_name} is limiting usage reads. Ghostex tries again in 30 minutes."
                ),
                rate_limited: true,
            })
        }
        0 => return Err("The hub couldn't read this account's usage.".to_string().into()),
        status => {
            return Err(format!(
                "{provider_name} usage is unavailable right now (HTTP {status}). Ghostex tries again later."
            )
            .into())
        }
    }
    Ok(Some(if provider == "claude" {
        claude_windows(&body)
    } else {
        let mut windows = Vec::new();
        crate::accounts::helpers::codex_windows(&mut windows, "", &body["rate_limit"]);
        if let Some(extras) = body.get("additional_rate_limits").and_then(Value::as_array) {
            for extra in extras {
                let model = extra
                    .get("limit_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                crate::accounts::helpers::codex_windows(&mut windows, model, &extra["rate_limit"]);
            }
        }
        windows
    }))
}

/// Anthropic's `/api/oauth/usage` answer as the same windows a Claude Swap reading has: `5h`,
/// `7d`, and one `<Model> 7d` per model-scoped weekly limit (`seven_day_opus`, …).
pub(crate) fn claude_windows(body: &Value) -> Vec<UsageWindow> {
    let window = |id: &str, label: String, value: &Value, seconds: i64, model: Option<String>| {
        let used = value.get("utilization").and_then(Value::as_f64)?;
        used.is_finite().then(|| UsageWindow {
            id: id.to_string(),
            label,
            used_percent: used.clamp(0., 100.),
            limit_window_seconds: Some(seconds),
            resets_at: value
                .get("resets_at")
                .and_then(Value::as_str)
                .map(str::to_string),
            model,
        })
    };
    let mut windows = Vec::new();
    windows.extend(window(
        "fiveHour",
        "5h".into(),
        &body["five_hour"],
        18_000,
        None,
    ));
    windows.extend(window(
        "sevenDay",
        "7d".into(),
        &body["seven_day"],
        604_800,
        None,
    ));
    if let Some(fields) = body.as_object() {
        for (key, value) in fields {
            let Some(model) = key.strip_prefix("seven_day_") else {
                continue;
            };
            // Not a model: the limits of OAuth apps and Cowork.
            if matches!(model, "oauth_apps" | "cowork") || !value.is_object() {
                continue;
            }
            let mut name = model.replace('_', " ");
            if let Some(first) = name.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            windows.extend(window(
                &name,
                format!("{name} 7d"),
                value,
                604_800,
                Some(name.clone()),
            ));
        }
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_usage_reads_like_a_claude_swap_reading() {
        let windows = claude_windows(&json!({
            "five_hour": {"utilization": 42.0, "resets_at": "2026-11-18T12:00:00Z"},
            "seven_day": {"utilization": 7.5, "resets_at": "2026-11-21T09:00:00Z"},
            "seven_day_oauth_apps": null,
            "seven_day_opus": {"utilization": 100.0, "resets_at": null},
        }));
        let summary: Vec<_> = windows
            .iter()
            .map(|w| {
                (
                    w.id.as_str(),
                    w.label.as_str(),
                    w.used_percent,
                    w.model.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("fiveHour", "5h", 42.0, None),
                ("sevenDay", "7d", 7.5, None),
                ("Opus", "Opus 7d", 100.0, Some("Opus")),
            ]
        );
    }
}
