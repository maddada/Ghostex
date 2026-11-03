//! Boat (`https://boat.dev/api/v1`, OpenAPI and auth.md fetched 2026-11-02): request builders and
//! answer readers.
//!
//! Boat has no OAuth for third-party apps; its browser path is the WorkOS agent-registration
//! protocol (`boat.dev/auth.md`): `service_auth` with the user's email answers a claim page and a
//! 6-digit code the user types there, and polling the token endpoint then gives a `boat_` API key.
//! Stop archives the box; a resume runs it on a new machine (programs end, new IP and host key);
//! there is no idle timer, only a time-to-live, so Ghostex creates boxes with `ttlSeconds: null`
//! and the box stops itself when idle (box mode).

use ghostex_gx_protocol::cloud_box::{CloudBoxState, CloudProviderLimits};
use serde_json::{json, Map, Value};

use crate::http::{encode_query, json_or_error, HttpRequest, HttpResponse, Method, ProviderError};
use crate::metadata::{BoxMetadata, MARKER_KEY, MARKER_VALUE};
use crate::provider::{ExecOutput, ProviderAccount, ProviderMachine, ProviderSnapshot};

pub const SITE: &str = "https://boat.dev";
pub const API_BASE: &str = "https://boat.dev/api/v1";
const LABEL: &str = "Boat";
pub const CLAIM_GRANT: &str = "urn:workos:agent-auth:grant-type:claim";

fn url(path: &str) -> String {
    format!("{API_BASE}{path}")
}

fn read(response: &HttpResponse) -> Result<Value, ProviderError> {
    json_or_error(response, LABEL)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

// ---- Sizes ---------------------------------------------------------------------------------

/// Boat's size word for a `<vcpus>x<memGiB>` id.
pub fn machine_type(size_id: &str) -> &'static str {
    match size_id {
        "2x4" => "small",
        "8x16" => "large",
        "16x32" => "xlarge",
        _ => "default",
    }
}

fn size_id_for_type(machine_type: &str) -> &'static str {
    match machine_type {
        "small" => "2x4",
        "large" => "8x16",
        "xlarge" => "16x32",
        _ => "4x8",
    }
}

// ---- Sign-in (agent registration) ----------------------------------------------------------

/// `POST /api/boat/agent/identity` with `service_auth`: answers a claim token and the claim
/// page, the 6-digit code, its lifetime and the poll interval.
pub fn identity_request(email: &str) -> HttpRequest {
    HttpRequest::new(Method::Post, format!("{SITE}/api/boat/agent/identity"))
        .json(json!({ "type": "service_auth", "login_hint": email }))
}

/// A started claim: hold `claim_token` in memory only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub claim_token: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

pub fn parse_identity(response: &HttpResponse) -> Result<Claim, ProviderError> {
    let value = read(response)?;
    let claim = value.get("claim").cloned().unwrap_or(Value::Null);
    Ok(Claim {
        claim_token: text(&value, "claim_token")
            .ok_or_else(|| ProviderError::unexpected("Boat sent no claim token."))?,
        user_code: text(&claim, "user_code").unwrap_or_default(),
        verification_uri: text(&claim, "verification_uri")
            .ok_or_else(|| ProviderError::unexpected("Boat sent no claim page."))?,
        expires_in: claim
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(600),
        interval: claim
            .get("interval")
            .and_then(Value::as_u64)
            .unwrap_or(5)
            .max(1),
    })
}

/// `POST /api/boat/oauth2/token` with the claim grant.
pub fn claim_token_request(claim_token: &str) -> HttpRequest {
    HttpRequest::new(Method::Post, format!("{SITE}/api/boat/oauth2/token"))
        .form(&[("grant_type", CLAIM_GRANT), ("claim_token", claim_token)])
}

/// One poll of the claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimPoll {
    /// The `boat_` key.
    Key(String),
    Pending,
    /// Wait this many more seconds between polls.
    SlowDown,
    Expired,
}

pub fn parse_claim_poll(response: &HttpResponse) -> Result<ClaimPoll, ProviderError> {
    if (200..300).contains(&response.status) {
        let value = read(response)?;
        return text(&value, "access_token")
            .map(ClaimPoll::Key)
            .ok_or_else(|| ProviderError::unexpected("Boat sent no API key."));
    }
    let value: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
    match value.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => Ok(ClaimPoll::Pending),
        Some("slow_down") => Ok(ClaimPoll::SlowDown),
        Some("expired_token") => Ok(ClaimPoll::Expired),
        _ => Err(crate::http::error_from_body(response, LABEL)),
    }
}

/// `POST /api/boat/oauth2/revoke`: drops a key the claim minted.
pub fn revoke_request(key: &str) -> HttpRequest {
    HttpRequest::new(Method::Post, format!("{SITE}/api/boat/oauth2/revoke"))
        .form(&[("token", key), ("token_type_hint", "access_token")])
}

// ---- Account -------------------------------------------------------------------------------

pub fn me_request(key: &str) -> HttpRequest {
    HttpRequest::new(Method::Get, url("/me")).bearer(key)
}

pub fn limits_request(key: &str) -> HttpRequest {
    HttpRequest::new(Method::Get, url("/limits")).bearer(key)
}

/// `GET /me` (who) and `GET /limits` (plan) into an account.
pub fn parse_account(
    me: &HttpResponse,
    limits: Option<&HttpResponse>,
) -> Result<ProviderAccount, ProviderError> {
    let me = read(me)?;
    let user = me.get("user").cloned().unwrap_or(Value::Null);
    let limits_value = limits
        .and_then(|limits| read(limits).ok())
        .unwrap_or(Value::Null);
    let plan = text(&limits_value, "plan").or_else(|| {
        limits_value
            .get("plan")
            .and_then(|plan| text(plan, "name").or_else(|| text(plan, "id")))
    });
    let trial = plan.as_deref().is_some_and(|plan| plan.contains("trial"));
    Ok(ProviderAccount {
        account_label: text(&user, "displayName").or_else(|| text(&user, "login")),
        account_email: text(&user, "email"),
        org_id: text(&limits_value, "teamId"),
        plan,
        limits: Some(CloudProviderLimits {
            max_box_run_seconds: trial.then_some(7200),
            note: trial.then(|| "Boat trial: boxes stop after at most 2 hours.".to_string()),
            ..Default::default()
        }),
        month_usd: None,
    })
}

// ---- Sandboxes -----------------------------------------------------------------------------

/// `GET /sandboxes?label=ghostex=1&include=cost`: Ghostex's boxes with this month's dollars.
pub fn list_sandboxes_request(key: &str, cursor: Option<&str>) -> HttpRequest {
    let mut path = format!(
        "/sandboxes?limit=100&include=cost&label={}",
        encode_query(&format!("{MARKER_KEY}={MARKER_VALUE}"))
    );
    if let Some(cursor) = cursor {
        path.push_str(&format!("&cursor={}", encode_query(cursor)));
    }
    HttpRequest::new(Method::Get, url(&path)).bearer(key)
}

/// One page: the boxes, the next cursor, and this month's dollars of deleted Ghostex boxes.
pub fn parse_sandbox_list(
    response: &HttpResponse,
) -> Result<(Vec<ProviderMachine>, Option<String>, f64), ProviderError> {
    let value = read(response)?;
    let machines = value
        .get("sandboxes")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(sandbox_from_value).collect())
        .unwrap_or_default();
    let next = value
        .get("pageInfo")
        .and_then(|page| text(page, "nextCursor"));
    let deleted = value
        .get("deletedCost")
        .and_then(|cost| cost.get("dollars"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    Ok((machines, next, deleted))
}

pub fn get_sandbox_request(key: &str, sandbox_id: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Get,
        url(&format!("/sandboxes/{}", encode_query(sandbox_id))),
    )
    .bearer(key)
}

/// `GET /sandboxes/{id}`, a create, stop or resume answer: the box inside it.
pub fn parse_sandbox(response: &HttpResponse) -> Result<ProviderMachine, ProviderError> {
    let value = read(response)?;
    value
        .get("sandbox")
        .and_then(sandbox_from_value)
        .ok_or_else(|| ProviderError::unexpected("Boat sent no sandbox."))
}

pub fn box_state(state: &str) -> CloudBoxState {
    match state {
        "init" | "provisioning" | "provisioned" | "cloning" => CloudBoxState::Creating,
        "ready" | "idle" | "running" => CloudBoxState::Running,
        "archiving" | "archived" => CloudBoxState::Stopped,
        "error" | "cancelled" => CloudBoxState::Failed,
        other => CloudBoxState::Other(other.to_string()),
    }
}

fn sandbox_from_value(value: &Value) -> Option<ProviderMachine> {
    let labels = value
        .get("labels")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
                .collect()
        })
        .and_then(|map| BoxMetadata::from_map(&map));
    Some(ProviderMachine {
        provider_box_id: text(value, "id")?,
        state: box_state(
            value
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        metadata: labels,
        created_at: text(value, "createdAt"),
        started_at: text(value, "hydratedAt").or_else(|| text(value, "updatedAt")),
        stopped_at: text(value, "stoppedAt"),
        cost_month_usd: value
            .get("cost")
            .and_then(|cost| cost.get("dollars"))
            .and_then(Value::as_f64),
        size: text(value, "type").map(|kind| size_id_for_type(&kind).to_string()),
        error: text(value, "error"),
        operation_id: None,
    })
}

/// `POST /sandboxes`: from a saved snapshot (`from`) or blank, no auto-stop, Ghostex's labels.
pub fn create_sandbox_request(
    key: &str,
    idempotency_key: &str,
    metadata: &BoxMetadata,
    from_snapshot: Option<&str>,
    size_id: &str,
) -> HttpRequest {
    let labels: Map<String, Value> = metadata
        .to_pairs()
        .into_iter()
        .map(|(key, value)| (key.to_string(), Value::String(value)))
        .collect();
    let mut body = json!({
        "name": crate::instabox::machine_name(&metadata.box_id),
        "type": machine_type(size_id),
        "ttlSeconds": Value::Null,
        "labels": labels,
    });
    if let Some(snapshot) = from_snapshot {
        body["from"] = json!(snapshot);
    }
    HttpRequest::new(Method::Post, url("/sandboxes"))
        .bearer(key)
        .header("Idempotency-Key", idempotency_key)
        .json(body)
}

/// `POST /sandboxes/{id}/stop` (archives) or `/resume` (a new machine, no auto-stop).
pub fn sandbox_action_request(key: &str, sandbox_id: &str, action: &str) -> HttpRequest {
    let body = if action == "resume" {
        json!({ "ttlSeconds": Value::Null })
    } else {
        json!({})
    };
    HttpRequest::new(
        Method::Post,
        url(&format!("/sandboxes/{}/{action}", encode_query(sandbox_id))),
    )
    .bearer(key)
    .json(body)
}

pub fn delete_sandbox_request(key: &str, sandbox_id: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Delete,
        url(&format!("/sandboxes/{}", encode_query(sandbox_id))),
    )
    .bearer(key)
    .header("X-Ascii-Confirm-Delete", sandbox_id)
}

/// Single-quotes a word for `sh`.
pub fn shell_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// `POST /sandboxes/{id}/commands`: Boat runs a shell command line, so argv is quoted; stdin, when
/// given, is piped from a here-document.
pub fn command_request(
    key: &str,
    sandbox_id: &str,
    argv: &[&str],
    stdin: Option<&str>,
    timeout_seconds: u64,
) -> HttpRequest {
    let mut command = argv
        .iter()
        .map(|word| shell_quote(word))
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(stdin) = stdin {
        command = format!("{command} <<'GHOSTEX_STDIN_EOF'\n{stdin}\nGHOSTEX_STDIN_EOF");
    }
    HttpRequest::new(
        Method::Post,
        url(&format!("/sandboxes/{}/commands", encode_query(sandbox_id))),
    )
    .bearer(key)
    .json(json!({ "command": command, "timeoutSeconds": timeout_seconds.clamp(1, 600) }))
    .timeout(timeout_seconds.clamp(1, 600) + 30)
}

pub fn parse_command(response: &HttpResponse) -> Result<ExecOutput, ProviderError> {
    let value = read(response)?;
    Ok(ExecOutput {
        exit_code: value.get("exitCode").and_then(Value::as_i64).unwrap_or(-1),
        stdout: text(&value, "stdout").unwrap_or_default(),
        stderr: text(&value, "stderr").unwrap_or_default(),
        timed_out: value
            .get("timedOut")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// `PUT /sandboxes/{id}/files` with base64 content (Boat allows `/home/user` and `/tmp`).
pub fn write_file_request(key: &str, sandbox_id: &str, path: &str, bytes: &[u8]) -> HttpRequest {
    HttpRequest::new(
        Method::Put,
        url(&format!("/sandboxes/{}/files", encode_query(sandbox_id))),
    )
    .bearer(key)
    .json(json!({
        "path": path,
        "content": standard_base64(bytes),
        "encoding": "base64",
    }))
}

fn standard_base64(bytes: &[u8]) -> String {
    let url_safe = crate::ticket::base64url_encode(bytes);
    let mut out: String = url_safe
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .collect();
    while out.len() % 4 != 0 {
        out.push('=');
    }
    out
}

/// `GET /sandboxes/{id}/usage`: the box's whole-life dollars at list price.
pub fn usage_request(key: &str, sandbox_id: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Get,
        url(&format!("/sandboxes/{}/usage", encode_query(sandbox_id))),
    )
    .bearer(key)
}

pub fn parse_usage_dollars(response: &HttpResponse) -> Result<f64, ProviderError> {
    Ok(read(response)?
        .get("dollars")
        .and_then(Value::as_f64)
        .unwrap_or(0.0))
}

// ---- Saved snapshots -----------------------------------------------------------------------

/// Boat snapshot names are 1-63 of `a-z0-9-`, starting with a letter: `ghostex-<snapshot id body>`.
pub fn snapshot_name(snapshot_id: &str) -> String {
    format!(
        "ghostex-{}",
        snapshot_id.trim_start_matches(crate::ids::GX_ID_PREFIX)
    )
}

pub fn save_snapshot_request(key: &str, sandbox_id: &str, name: &str) -> HttpRequest {
    HttpRequest::new(Method::Post, url("/saved-snapshots"))
        .bearer(key)
        .json(json!({ "sandboxId": sandbox_id, "name": name }))
}

pub fn get_snapshot_request(key: &str, name: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Get,
        url(&format!("/saved-snapshots/{}", encode_query(name))),
    )
    .bearer(key)
}

pub fn delete_snapshot_request(key: &str, name: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Delete,
        url(&format!("/saved-snapshots/{}", encode_query(name))),
    )
    .bearer(key)
}

/// A save, get or list-row answer (`snapshot` wraps it in save and get).
pub fn parse_snapshot(response: &HttpResponse) -> Result<ProviderSnapshot, ProviderError> {
    let value = read(response)?;
    let snapshot = value.get("snapshot").cloned().unwrap_or(value);
    let status = match text(&snapshot, "status").as_deref() {
        Some("ready") => "ready",
        Some("failed") => "failed",
        _ => "pending",
    };
    Ok(ProviderSnapshot {
        provider_snapshot_id: text(&snapshot, "name")
            .ok_or_else(|| ProviderError::unexpected("Boat sent no snapshot."))?,
        name: text(&snapshot, "name").unwrap_or_default(),
        status: status.to_string(),
        source_box_id: text(&snapshot, "sourceSandboxId"),
        size_bytes: snapshot.get("sizeBytes").and_then(Value::as_u64),
        created_at: text(&snapshot, "createdAt"),
        error: text(&snapshot, "error"),
    })
}

/// The size a box gets, for the price table.
pub fn size_of_type(machine_type: &str) -> String {
    size_id_for_type(machine_type).to_string()
}
