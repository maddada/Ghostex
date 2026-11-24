//! Calls to a CLIProxyAPI hub: the client key check when a hub is added, and the v8 management
//! API (with v0 fallback for older hubs) that lists the hub's Claude and Codex accounts
//! (`/v8/management/credentials` / `/v0/management/auth-files`), reads each one's usage through
//! the hub (`/v0/management/api-call`, which sends the request with that account's token in place
//! of `$TOKEN$`, so no token ever reaches Ghostex), runs browser OAuth sign-ins for Claude and
//! Codex (`/v8/management/oauth/...` with `/v0/management/...` fallback), toggles or removes
//! credentials, resets cooldowns, and reads or enables session affinity (`/v8/management/config`)
//! for prompt caching.
//!
//! CDXC:AgentProviders 2026-11-18 WHY:
//! Ghostex stays hands-off with CLIProxyAPI: it never installs, starts or reconfigures CLIProxyAPI
//! itself, never downloads or copies Claude Swap or Codex Swap logins into a hub, and never
//! redeems a Codex reset credit through it.
//!
//! CDXC:AgentProviders 2026-11-24 DECISION:
//! Amended for user-triggered OAuth sign-in from Settings > Accounts: Ghostex on this computer
//! briefly listens on 127.0.0.1:54545 (Claude /callback) or 127.0.0.1:1455 (Codex /auth/callback),
//! asks the hub's management API for the OAuth URL without starting a listener on the hub, catches
//! the single redirect, validates state, forwards the code and state to the hub's oauth-callback
//! endpoint, and polls status until the hub finishes saving the account.

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

fn management_v8_post(
    url: &str,
    key: Option<&str>,
    path: &str,
    body: Value,
) -> Result<Answer, String> {
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
        404 => Err(match hub_error(&answer.body) {
            Some(text) => format!("The hub returned not found (HTTP 404): {text}."),
            None => "The hub's management API is turned off or this hub is older than v8. Set a secret-key under management in its config.".to_string(),
        }),
        status => Err(format!(
            "The hub's management API answered with an error (HTTP {status}){}.",
            hub_error(&answer.body).map(|text| format!(": {text}")).unwrap_or_default()
        )),
    }
}

/// True when a v8 endpoint returned a plain route-missing 404 (older v0-only hub) rather than a
/// structured v8 JSON error such as "credential not found".
fn is_missing_v8_route(answer: &Answer) -> bool {
    answer.status == 404 && hub_error(&answer.body).is_none()
}

/// The hub's accounts (`files` of `/v8/management/credentials`, falling back to `/v0/management/auth-files`).
pub(crate) fn auth_files(url: &str, key: &str) -> Result<Vec<Value>, String> {
    let v8 = management_v8_get(url, key, "credentials")?;
    let body = if is_missing_v8_route(&v8) {
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
/// `GET /v8/management/config`. Returns `session_affinity: None` only when the hub does not
/// expose v8 config (404); on a 200 OK without a `routing` block, CLIProxyAPI defaults
/// `session-affinity` to `false`, so `Some(false)` is returned.
pub(crate) fn read_routing(url: &str, key: &str) -> Result<RoutingInfo, String> {
    let response = management_v8_get(url, key, "config")?;
    if is_missing_v8_route(&response) {
        return Ok(RoutingInfo::default());
    }
    let body = management_answer(response)?;
    let routing = body.get("routing");
    let session_affinity = Some(
        routing
            .and_then(|r| r.get("session-affinity"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    );
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
    pub local_callback: bool,
}

fn validate_oauth_auth_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 2048 || trimmed.chars().any(char::is_control)
    {
        return Err("The hub returned an invalid sign-in URL.".to_string());
    }
    let parsed = url::Url::parse(trimmed)
        .map_err(|_| "The hub returned an invalid sign-in URL.".to_string())?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("The hub returned an invalid sign-in URL.".to_string());
    }
    Ok(trimmed.to_string())
}

fn management_v0_post(url: &str, key: &str, path: &str, body: Value) -> Result<Answer, String> {
    answer(
        agent()
            .post(format!("{url}/v0/management/{path}"))
            .header("Authorization", format!("Bearer {key}"))
            .send_json(body),
    )
}

fn management_v0_delete(url: &str, key: &str, path: &str) -> Result<Answer, String> {
    answer(
        agent()
            .delete(format!("{url}/v0/management/{path}"))
            .header("Authorization", format!("Bearer {key}"))
            .call(),
    )
}

/// Starts a browser OAuth sign-in on the hub for `claude` or `codex` without asking the hub to
/// bind a callback port (`is_webui` is omitted so Ghostex owns the 127.0.0.1 callback listener).
/// Tries `/v8/management/oauth/auth-url` and falls back to `/v0/management/anthropic-auth-url` or
/// `/v0/management/codex-auth-url` for older hubs.
pub(crate) fn oauth_start(
    url: &str,
    key: &str,
    provider: &str,
    local_callback: bool,
) -> Result<OAuthStart, String> {
    let v0_path = match provider {
        "claude" => "anthropic-auth-url",
        "codex" => "codex-auth-url",
        _ => {
            return Err(
                "Only Claude and Codex accounts can be added to a CLIProxyAPI hub.".to_string(),
            );
        }
    };
    let response = management_v8_get(url, key, &format!("oauth/auth-url?provider={provider}"))?;
    let body = if is_missing_v8_route(&response) {
        management_get(url, key, v0_path)?
    } else {
        management_answer(response)?
    };
    if body.get("status").and_then(Value::as_str) == Some("error") {
        return Err(hub_error(&body)
            .unwrap_or_else(|| "The hub refused to start the sign-in.".to_string()));
    }
    let raw_auth_url = body
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "The hub did not return a sign-in URL.".to_string())?;
    let auth_url = validate_oauth_auth_url(raw_auth_url)?;
    let state = body
        .get("state")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.chars().count() <= 2048
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| "The hub did not return an OAuth state.".to_string())?
        .to_string();
    if let Ok(parsed) = url::Url::parse(&auth_url) {
        if let Some((_, url_state)) = parsed.query_pairs().find(|(k, _)| k == "state") {
            if url_state.as_ref() != state {
                return Err("The hub returned a mismatched OAuth state.".to_string());
            }
        }
    }
    Ok(OAuthStart {
        url: auth_url,
        state,
        local_callback,
    })
}

#[derive(Clone, Debug)]
pub(crate) struct OAuthStatus {
    pub status: String,
    pub error: Option<String>,
}

/// Polls a pending OAuth login session (`GET /v8/management/oauth/status?state=...`, falling back
/// to `GET /v0/management/get-auth-status?state=...`).
pub(crate) fn oauth_status(url: &str, key: &str, state: &str) -> Result<OAuthStatus, String> {
    let encoded = encode_query(state);
    let response = management_v8_get(url, key, &format!("oauth/status?state={encoded}"))?;
    let body = if is_missing_v8_route(&response) {
        management_get(url, key, &format!("get-auth-status?state={encoded}"))?
    } else {
        management_answer(response)?
    };
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

fn send_oauth_callback(
    url: &str,
    key: &str,
    provider: &str,
    v8_body: Value,
) -> Result<(), String> {
    let response = management_v8_post(url, Some(key), "oauth/callback", v8_body.clone())?;
    let body = if is_missing_v8_route(&response) {
        let mut v0_body = v8_body;
        if provider == "claude" {
            if let Some(obj) = v0_body.as_object_mut() {
                obj.insert("provider".into(), json!("anthropic"));
            }
        }
        management_answer(management_v0_post(url, key, "oauth-callback", v0_body)?)?
    } else {
        management_answer(response)?
    };
    if body.get("status").and_then(Value::as_str) == Some("error") {
        return Err(hub_error(&body)
            .unwrap_or_else(|| "The hub rejected the sign-in callback.".to_string()));
    }
    Ok(())
}

/// Forwards an authorization code captured by Ghostex's local loopback callback listener to the
/// hub (`/v8/management/oauth/callback` with `/v0/management/oauth-callback` fallback).
pub(crate) fn oauth_callback_code(
    url: &str,
    key: &str,
    provider: &str,
    state: &str,
    code: &str,
) -> Result<(), String> {
    if !matches!(provider, "claude" | "codex") {
        return Err(
            "Only Claude and Codex accounts can be added to a CLIProxyAPI hub.".to_string(),
        );
    }
    send_oauth_callback(
        url,
        key,
        provider,
        json!({
            "provider": provider,
            "state": state,
            "code": code,
        }),
    )
}

/// Submits a pasted redirect URL or authorization code to `POST /v8/management/oauth/callback`
/// (falling back to `POST /v0/management/oauth-callback`).
pub(crate) fn oauth_callback(
    url: &str,
    key: &str,
    provider: &str,
    state: &str,
    input: &str,
) -> Result<(), String> {
    if !matches!(provider, "claude" | "codex") {
        return Err(
            "Only Claude and Codex accounts can be added to a CLIProxyAPI hub.".to_string(),
        );
    }
    let state = state.trim();
    if state.is_empty() || state.chars().any(char::is_control) {
        return Err("Missing or invalid OAuth state.".to_string());
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Paste the redirect URL or authorization code from your browser.".to_string());
    }
    if trimmed.chars().count() > 2048 || trimmed.chars().any(char::is_control) {
        return Err(
            "That redirect URL or code is invalid. Paste the URL or code from your browser."
                .to_string(),
        );
    }
    let body = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let parsed = url::Url::parse(trimmed).map_err(|_| {
            "That isn't a valid redirect URL. Copy the full address from your browser bar."
                .to_string()
        })?;
        if parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(
                "That isn't a valid redirect URL. Copy the full address from your browser bar."
                    .to_string(),
            );
        }
        let mut has_code_or_error = false;
        let mut url_state: Option<String> = None;
        for (k, v) in parsed.query_pairs() {
            match k.as_ref() {
                "code" | "error" if !v.trim().is_empty() => has_code_or_error = true,
                "state" if !v.trim().is_empty() => url_state = Some(v.trim().to_string()),
                _ => {}
            }
        }
        if !has_code_or_error {
            return Err(
                "That redirect URL has no authorization code. Copy the URL after signing in."
                    .to_string(),
            );
        }
        if let Some(pasted_state) = url_state {
            if pasted_state != state {
                return Err(
                    "That redirect URL belongs to a different sign-in attempt. Paste the URL from the current browser window."
                        .to_string(),
                );
            }
        }
        json!({
            "provider": provider,
            "state": state,
            "redirect_url": trimmed,
        })
    } else if let Some((code_part, state_part)) = trimmed.split_once('#') {
        let code = code_part.trim();
        let pasted_state = state_part.trim();
        if code.is_empty() || code.chars().any(char::is_whitespace) {
            return Err(
                "Paste the redirect URL or authorization code from your browser.".to_string(),
            );
        }
        if !pasted_state.is_empty() && pasted_state != state {
            return Err(
                "That code belongs to a different sign-in attempt. Paste the code from the current browser window."
                    .to_string(),
            );
        }
        json!({
            "provider": provider,
            "state": state,
            "code": code,
        })
    } else {
        if trimmed.chars().any(char::is_whitespace) {
            return Err("Paste the redirect URL or authorization code without spaces.".to_string());
        }
        json!({
            "provider": provider,
            "state": state,
            "code": trimmed,
        })
    };
    send_oauth_callback(url, key, provider, body)
}

/// Cancels a pending OAuth login session (`DELETE /v8/management/oauth/session?state=...`,
/// falling back to `DELETE /v0/management/oauth-session?state=...`).
pub(crate) fn oauth_cancel(url: &str, key: &str, state: &str) -> Result<(), String> {
    let encoded = encode_query(state);
    let response = management_v8_delete(url, key, &format!("oauth/session?state={encoded}"))?;
    if is_missing_v8_route(&response) {
        let fallback = management_v0_delete(url, key, &format!("oauth-session?state={encoded}"))?;
        if fallback.status == 404 {
            return Ok(());
        }
        management_answer(fallback)?;
        return Ok(());
    }
    if response.status == 404 {
        return Ok(());
    }
    management_answer(response)?;
    Ok(())
}

/// Enables or disables a hub credential (`PATCH /v8/management/credentials/status`, with v0 fallback).
pub(crate) fn set_account_disabled(
    url: &str,
    key: &str,
    file_name: &str,
    auth_index: &str,
    disabled: bool,
) -> Result<(), String> {
    let payload = json!({
        "name": file_name,
        "auth_index": auth_index,
        "disabled": disabled,
    });
    let response = management_v8_patch(url, key, "credentials/status", payload.clone())?;
    if is_missing_v8_route(&response) {
        let fallback = answer(
            agent()
                .patch(format!("{url}/v0/management/auth-files/status"))
                .header("Authorization", format!("Bearer {key}"))
                .send_json(payload),
        )?;
        management_answer(fallback)?;
        return Ok(());
    }
    management_answer(response)?;
    Ok(())
}

/// Deletes a credential file from the hub (`DELETE /v8/management/credentials?name=...`, with v0 fallback).
pub(crate) fn delete_account(url: &str, key: &str, file_name: &str) -> Result<(), String> {
    let query = format!("name={}", encode_query(file_name));
    let response = management_v8_delete(url, key, &format!("credentials?{query}"))?;
    if is_missing_v8_route(&response) {
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

/// Clears a credential's cooldown state (`POST /v8/management/routing/cooldown/reset`, with v0 fallback).
pub(crate) fn reset_cooldown(url: &str, key: &str, auth_index: &str) -> Result<(), String> {
    let payload = json!({ "auth_index": auth_index });
    let response = management_v8_post(url, Some(key), "routing/cooldown/reset", payload.clone())?;
    if is_missing_v8_route(&response) {
        let fallback = answer(
            agent()
                .post(format!("{url}/v0/management/reset-quota"))
                .header("Authorization", format!("Bearer {key}"))
                .send_json(payload),
        )?;
        management_answer(fallback)?;
        return Ok(());
    }
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
