//! Calls to a CLIProxyAPI hub: the client key check when a hub is added, and the v8 management
//! API (with v0 fallback for older hubs) that lists the hub's Claude and Codex accounts
//! (`/v8/management/credentials`), reads each one's usage through the hub
//! (`/v0/management/api-call`, which sends the request with that account's token in place of
//! `$TOKEN$`, so no token ever reaches Ghostex), runs browser OAuth sign-ins for Claude and Codex
//! (`/v8/management/oauth/...`), toggles or removes credentials, resets cooldowns, and reads or
//! enables session affinity (`/v8/management/config`) for prompt caching.

use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Map, Value};

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
        format!("The hub can't be reached ({error}). Check that it is running and its address is right.")
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

fn management_v8_get(url: &str, key: &str, path: &str) -> Result<Answer, String> {
    answer(
        agent()
            .get(format!("{url}/v8/management/{path}"))
            .header("Authorization", format!("Bearer {key}"))
            .call(),
    )
}

fn management_v8_post(url: &str, key: Option<&str>, path: &str, body: Value) -> Result<Answer, String> {
    let request = agent().post(format!("{url}/v8/management/{path}"));
    let request = match key {
        Some(key) => request.header("Authorization", format!("Bearer {key}")),
        None => request,
    };
    answer(request.send_json(body))
}

fn management_v8_patch(url: &str, key: &str, path: &str, body: Value) -> Result<Answer, String> {
    answer(
        agent()
            .patch(format!("{url}/v8/management/{path}"))
            .header("Authorization", format!("Bearer {key}"))
            .send_json(body),
    )
}

fn management_v8_delete(url: &str, key: &str, path: &str) -> Result<Answer, String> {
    answer(
        agent()
            .delete(format!("{url}/v8/management/{path}"))
            .header("Authorization", format!("Bearer {key}"))
            .call(),
    )
}

fn encode_query(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
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

/// The hub's accounts (`files` of `/v8/management/credentials`, falling back to `/v0/management/auth-files`).
pub(crate) fn auth_files(url: &str, key: &str) -> Result<Vec<Value>, String> {
    let v8 = management_v8_get(url, key, "credentials")?;
    let body = if v8.status == 404 {
        management_get(url, key, "auth-files")?
    } else {
        management_answer(v8)?
    };
    Ok(body
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RoutingInfo {
    pub session_affinity: Option<bool>,
    pub strategy: Option<String>,
}

/// Reads the hub's routing configuration (`routing.session-affinity` and `routing.strategy`) via
/// `GET /v8/management/config`. Returns empty info if the hub does not expose v8 config.
pub(crate) fn read_routing(url: &str, key: &str) -> Result<RoutingInfo, String> {
    let response = management_v8_get(url, key, "config")?;
    if response.status == 404 {
        return Ok(RoutingInfo::default());
    }
    let body = management_answer(response)?;
    let routing = body.get("routing");
    let session_affinity = routing
        .and_then(|r| r.get("session-affinity"))
        .and_then(Value::as_bool)
        .or_else(|| routing.map(|_| false));
    let strategy = routing
        .and_then(|r| r.get("strategy"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(RoutingInfo {
        session_affinity,
        strategy,
    })
}

/// Sets `routing.session-affinity` on the hub (`PATCH /v8/management/config`), retrying once on a
/// concurrent config change (409 / 412).
pub(crate) fn set_session_affinity(url: &str, key: &str, enabled: bool) -> Result<(), String> {
    for attempt in 0..2 {
        let response = management_v8_patch(
            url,
            key,
            "config",
            json!({ "routing": { "session-affinity": enabled } }),
        )?;
        if matches!(response.status, 409 | 412) && attempt == 0 {
            continue;
        }
        management_answer(response)?;
        return Ok(());
    }
    Err("The hub's config changed while saving session affinity. Try again.".to_string())
}

#[derive(Clone, Debug)]
pub(crate) struct OAuthStart {
    pub url: String,
    pub state: String,
}

/// Starts a v8 browser OAuth sign-in on the hub for `claude` or `codex` only.
pub(crate) fn oauth_start(
    url: &str,
    key: &str,
    provider: &str,
    local_callback: bool,
) -> Result<OAuthStart, String> {
    if !matches!(provider, "claude" | "codex") {
        return Err("Only Claude and Codex accounts can be added to a CLIProxyAPI hub.".to_string());
    }
    let path = if local_callback {
        format!("oauth/auth-url?provider={provider}&is_webui=true")
    } else {
        format!("oauth/auth-url?provider={provider}")
    };
    let mut response = management_v8_get(url, key, &path)?;
    // If the hub could not bind its local callback port (e.g. port busy), retry without is_webui
    // so the user can still complete sign-in by pasting the redirect URL.
    if local_callback && response.status >= 500 {
        response = management_v8_get(url, key, &format!("oauth/auth-url?provider={provider}"))?;
    }
    let body = management_answer(response)?;
    let auth_url = body
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "The hub did not return a sign-in URL.".to_string())?
        .to_string();
    let state = body
        .get("state")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "The hub did not return an OAuth state.".to_string())?
        .to_string();
    Ok(OAuthStart {
        url: auth_url,
        state,
    })
}

#[derive(Clone, Debug)]
pub(crate) struct OAuthStatus {
    pub status: String,
    pub error: Option<String>,
}

/// Polls a pending v8 OAuth login session (`GET /v8/management/oauth/status?state=...`).
pub(crate) fn oauth_status(url: &str, key: &str, state: &str) -> Result<OAuthStatus, String> {
    let response = management_v8_get(
        url,
        key,
        &format!("oauth/status?state={}", encode_query(state)),
    )?;
    let body = management_answer(response)?;
    let status = body
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("error")
        .to_string();
    let error = body
        .get("error")
        .and_then(Value::as_str)
        .map(masked)
        .filter(|value| !value.is_empty());
    Ok(OAuthStatus { status, error })
}

/// Submits a pasted redirect URL or authorization code to `POST /v8/management/oauth/callback`.
pub(crate) fn oauth_callback(
    url: &str,
    key: &str,
    provider: &str,
    state: &str,
    input: &str,
) -> Result<(), String> {
    if !matches!(provider, "claude" | "codex") {
        return Err("Only Claude and Codex accounts can be added to a CLIProxyAPI hub.".to_string());
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Paste the redirect URL or authorization code from your browser.".to_string());
    }
    let body = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        json!({
            "provider": provider,
            "state": state,
            "redirect_url": trimmed,
        })
    } else if let Some((code_part, state_part)) = trimmed.split_once('#') {
        let code = code_part.trim();
        let effective_state = if state_part.trim().is_empty() {
            state
        } else {
            state_part.trim()
        };
        json!({
            "provider": provider,
            "state": effective_state,
            "code": code,
        })
    } else {
        json!({
            "provider": provider,
            "state": state,
            "code": trimmed,
        })
    };
    let response = management_v8_post(url, Some(key), "oauth/callback", body)?;
    management_answer(response)?;
    Ok(())
}

/// Cancels a pending v8 OAuth login session (`DELETE /v8/management/oauth/session?state=...`).
pub(crate) fn oauth_cancel(url: &str, key: &str, state: &str) -> Result<(), String> {
    let response = management_v8_delete(
        url,
        key,
        &format!("oauth/session?state={}", encode_query(state)),
    )?;
    if response.status == 404 {
        return Ok(());
    }
    management_answer(response)?;
    Ok(())
}

/// Enables or disables a hub credential (`PATCH /v8/management/credentials/status`).
pub(crate) fn set_account_disabled(
    url: &str,
    key: &str,
    file_name: &str,
    auth_index: &str,
    disabled: bool,
) -> Result<(), String> {
    let response = management_v8_patch(
        url,
        key,
        "credentials/status",
        json!({
            "name": file_name,
            "auth_index": auth_index,
            "disabled": disabled,
        }),
    )?;
    management_answer(response)?;
    Ok(())
}

/// Deletes a credential file from the hub (`DELETE /v8/management/credentials?name=...`, with v0 fallback).
pub(crate) fn delete_account(url: &str, key: &str, file_name: &str) -> Result<(), String> {
    let query = format!("name={}", encode_query(file_name));
    let response = management_v8_delete(url, key, &format!("credentials?{query}"))?;
    if response.status == 404 {
        let fallback = answer(
            agent()
                .delete(format!("{url}/v0/management/auth-files?{query}"))
                .header("Authorization", format!("Bearer {key}"))
                .call(),
        )?;
        management_answer(fallback)?;
        return Ok(());
    }
    management_answer(response)?;
    Ok(())
}

/// Clears a credential's cooldown state (`POST /v8/management/routing/cooldown/reset`).
pub(crate) fn reset_cooldown(url: &str, key: &str, auth_index: &str) -> Result<(), String> {
    let response = management_v8_post(
        url,
        Some(key),
        "routing/cooldown/reset",
        json!({ "auth_index": auth_index }),
    )?;
    management_answer(response)?;
    Ok(())
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
