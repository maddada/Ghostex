//! Instabox (`https://api.instabox.sh`, OpenAPI fetched 2026-11-02): request builders and answer
//! readers.
//!
//! Sign-in is RFC 8252 loopback + PKCE (`/auth/cli/start` → `redirect_uri?code=ibc_…` →
//! `POST /v1/auth/cli/token`); the session then mints a scoped `ibk_` key (`POST /v1/keys`, signed-in
//! sessions only) that Ghostex uses day to day. Mutations take an `Idempotency-Key` and answer an
//! operation to poll (`GET /v1/operations/{id}?wait=25s`).

use ghostex_gx_protocol::cloud_box::{CloudBoxSize, CloudBoxState, CloudProviderLimits};
use serde_json::{json, Map, Value};

use crate::http::{encode_query, json_or_error, HttpRequest, HttpResponse, Method, ProviderError};
use crate::metadata::{BoxMetadata, MARKER_KEY, MARKER_VALUE};
use crate::provider::{
    ExecOutput, ProviderAccount, ProviderMachine, ProviderSnapshot, ProviderTokens,
};

pub const API_BASE: &str = "https://api.instabox.sh";
const LABEL: &str = "Instabox";

/// The scopes of the key Ghostex mints: boxes and snapshots, nothing else.
pub const KEY_SCOPES: [&str; 6] = [
    "machines:read",
    "machines:write",
    "machines:delete",
    "images:read",
    "images:write",
    "images:delete",
];
/// Days the minted key lives (the most Instabox allows short of "never").
pub const KEY_EXPIRES_IN_DAYS: u64 = 365;
/// Instabox's idle pause, in seconds, from the idle minutes setting (Q1011: 15 minutes).
pub fn auto_suspend_seconds(idle_minutes: u64) -> u64 {
    idle_minutes.max(5) * 60
}

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

// ---- Sign-in -------------------------------------------------------------------------------

/// The redirect URI Instabox accepts: exactly `http://127.0.0.1:<port>/callback`.
pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/callback")
}

/// The page the user's browser opens to sign in.
pub fn sign_in_url(redirect_uri: &str, state: &str, code_challenge: &str) -> String {
    format!(
        "{API_BASE}/auth/cli/start?redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256",
        encode_query(redirect_uri),
        encode_query(state),
        encode_query(code_challenge)
    )
}

pub fn cli_token_request(code: &str, verifier: &str, redirect_uri: &str) -> HttpRequest {
    HttpRequest::new(Method::Post, url("/v1/auth/cli/token")).json(json!({
        "code": code,
        "codeVerifier": verifier,
        "redirectUri": redirect_uri,
    }))
}

pub fn refresh_request(refresh_token: &str) -> HttpRequest {
    HttpRequest::new(Method::Post, url("/v1/auth/refresh"))
        .json(json!({ "refreshToken": refresh_token }))
}

pub fn logout_request(access_token: &str) -> HttpRequest {
    HttpRequest::new(Method::Post, url("/v1/auth/logout")).bearer(access_token)
}

pub fn parse_tokens(response: &HttpResponse) -> Result<ProviderTokens, ProviderError> {
    let value = read(response)?;
    Ok(ProviderTokens {
        access_token: text(&value, "accessToken")
            .ok_or_else(|| ProviderError::unexpected("Instabox sent no access token."))?,
        refresh_token: text(&value, "refreshToken"),
        access_expires_at: text(&value, "accessExpiresAt"),
        refresh_expires_at: text(&value, "refreshExpiresAt"),
    })
}

/// `POST /v1/keys`: the scoped key Ghostex keeps (signed-in sessions only).
pub fn create_key_request(access_token: &str, name: &str) -> HttpRequest {
    HttpRequest::new(Method::Post, url("/v1/keys"))
        .bearer(access_token)
        .json(json!({
            "name": name,
            "scopes": KEY_SCOPES,
            "expiresInDays": KEY_EXPIRES_IN_DAYS,
        }))
}

/// The minted key and its id (to revoke it on disconnect).
pub fn parse_created_key(
    response: &HttpResponse,
) -> Result<(String, Option<String>), ProviderError> {
    let value = read(response)?;
    let key = text(&value, "key")
        .ok_or_else(|| ProviderError::unexpected("Instabox sent no API key."))?;
    let id = value.get("apiKey").and_then(|api_key| text(api_key, "id"));
    Ok((key, id))
}

pub fn revoke_key_request(access_token: &str, key_id: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Delete,
        url(&format!("/v1/keys/{}", encode_query(key_id))),
    )
    .bearer(access_token)
}

// ---- Account, plan and spend ---------------------------------------------------------------

/// `GET /v1/usage`: the org, its limits and this month's cost (readable with the key).
pub fn usage_request(key: &str) -> HttpRequest {
    HttpRequest::new(Method::Get, url("/v1/usage")).bearer(key)
}

pub fn parse_usage(response: &HttpResponse) -> Result<ProviderAccount, ProviderError> {
    let value = read(response)?;
    let org = value.get("org").cloned().unwrap_or(Value::Null);
    let plan = text(&org, "tier");
    let limits_value = value.get("limits").cloned().unwrap_or(Value::Null);
    let unlimited: Vec<String> = limits_value
        .get("unlimited")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let limit = |key: &str| {
        limits_value
            .get(key)
            .and_then(Value::as_u64)
            .filter(|count| *count > 0 && !unlimited.iter().any(|name| name == key))
    };
    let allowed_sizes = limits_value
        .get("machineSpecs")
        .and_then(Value::as_array)
        .map(|specs| {
            specs
                .iter()
                .filter_map(|spec| {
                    Some(CloudBoxSize::id_for(
                        spec.get("vcpus")?.as_u64()?,
                        spec.get("memMiB")?.as_u64()?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let max_boxes = limit("maxMachines");
    let note = match (plan.as_deref(), max_boxes) {
        (Some("free"), Some(1)) => {
            Some("Free plan: one box at a time, started only from a snapshot.".to_string())
        }
        (Some("free"), _) => Some("Free plan: boxes start only from a snapshot.".to_string()),
        _ => None,
    };
    Ok(ProviderAccount {
        account_label: text(&org, "name"),
        account_email: None,
        org_id: text(&org, "id"),
        plan,
        limits: Some(CloudProviderLimits {
            max_boxes,
            max_running_boxes: None,
            max_snapshots: limit("maxSnapshots"),
            max_box_run_seconds: None,
            allowed_sizes,
            note,
        }),
        month_usd: value
            .get("cost")
            .and_then(|cost| cost.get("costUSD"))
            .and_then(Value::as_f64),
    })
}

/// `GET /v1/me` (a signed-in session): the account's email.
pub fn me_request(access_token: &str) -> HttpRequest {
    HttpRequest::new(Method::Get, url("/v1/me")).bearer(access_token)
}

pub fn parse_me_email(response: &HttpResponse) -> Option<String> {
    read(response)
        .ok()
        .and_then(|value| text(&value, "email").or_else(|| text(&value, "login")))
}

// ---- Machines ------------------------------------------------------------------------------

/// `GET /v1/machines?metadata=ghostex:1`: Ghostex's boxes, one page.
pub fn list_machines_request(key: &str, cursor: Option<&str>) -> HttpRequest {
    let mut path = format!(
        "/v1/machines?limit=100&metadata={}",
        encode_query(&format!("{MARKER_KEY}:{MARKER_VALUE}"))
    );
    if let Some(cursor) = cursor {
        path.push_str(&format!("&cursor={}", encode_query(cursor)));
    }
    HttpRequest::new(Method::Get, url(&path)).bearer(key)
}

/// One page of machines and the next page's cursor.
pub fn parse_machine_list(
    response: &HttpResponse,
) -> Result<(Vec<ProviderMachine>, Option<String>), ProviderError> {
    let value = read(response)?;
    let machines = value
        .get("machines")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(machine_from_value).collect())
        .unwrap_or_default();
    Ok((machines, text(&value, "nextCursor")))
}

pub fn get_machine_request(key: &str, machine_id: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Get,
        url(&format!("/v1/machines/{}", encode_query(machine_id))),
    )
    .bearer(key)
}

pub fn parse_machine(response: &HttpResponse) -> Result<ProviderMachine, ProviderError> {
    machine_from_value(&read(response)?)
        .ok_or_else(|| ProviderError::unexpected("Instabox sent no machine."))
}

/// Instabox's state word as a box state.
pub fn box_state(state: &str) -> CloudBoxState {
    match state {
        "creating" | "starting" | "resuming" => CloudBoxState::Creating,
        "running" => CloudBoxState::Running,
        "suspending" | "suspended" => CloudBoxState::Paused,
        "stopping" | "stopped" => CloudBoxState::Stopped,
        "deleting" => CloudBoxState::Deleting,
        "deleted" => CloudBoxState::Deleted,
        "error" => CloudBoxState::Failed,
        other => CloudBoxState::Other(other.to_string()),
    }
}

fn machine_from_value(value: &Value) -> Option<ProviderMachine> {
    let metadata = value
        .get("metadata")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
                .collect()
        })
        .and_then(|map| BoxMetadata::from_map(&map));
    let spec = value.get("spec");
    Some(ProviderMachine {
        provider_box_id: text(value, "id")?,
        state: box_state(
            value
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        metadata,
        created_at: text(value, "createdAt"),
        started_at: text(value, "readyAt").or_else(|| text(value, "runStartedAt")),
        stopped_at: None,
        cost_month_usd: None,
        size: spec.and_then(|spec| {
            Some(CloudBoxSize::id_for(
                spec.get("vcpus")?.as_u64()?,
                spec.get("memMiB")?.as_u64()?,
            ))
        }),
        error: value
            .get("lastError")
            .and_then(|error| text(error, "message")),
        operation_id: text(value, "operationId"),
    })
}

/// Instabox names are 1-63 of `a-z0-9-`: `ghostex-<box id body>`.
pub fn machine_name(box_id: &str) -> String {
    format!(
        "ghostex-{}",
        box_id.trim_start_matches(crate::ids::GX_ID_PREFIX)
    )
}

/// What a create asks for.
pub struct CreateSpec<'a> {
    pub metadata: &'a BoxMetadata,
    /// The provider snapshot id; absent for a blank box (Instabox's default template).
    pub provider_snapshot_id: Option<&'a str>,
    pub vcpus: u64,
    pub mem_mib: u64,
    pub idle_minutes: u64,
}

/// `POST /v1/machines`: answers a `create` operation whose `machineId` is the new box.
pub fn create_machine_request(key: &str, idempotency_key: &str, spec: &CreateSpec) -> HttpRequest {
    let metadata: Map<String, Value> = spec
        .metadata
        .to_pairs()
        .into_iter()
        .map(|(key, value)| (key.to_string(), Value::String(value)))
        .collect();
    let mut body = json!({
        "name": machine_name(&spec.metadata.box_id),
        "vcpus": spec.vcpus,
        "memMiB": spec.mem_mib,
        "autoSuspendAfterSeconds": auto_suspend_seconds(spec.idle_minutes),
        "metadata": metadata,
    });
    if let Some(snapshot_id) = spec.provider_snapshot_id {
        body["snapshotId"] = json!(snapshot_id);
    }
    HttpRequest::new(Method::Post, url("/v1/machines"))
        .bearer(key)
        .header("Idempotency-Key", idempotency_key)
        .json(body)
}

/// A lifecycle call: `suspend`, `resume`, `stop`, `start` or `wake`.
pub fn machine_action_request(
    key: &str,
    machine_id: &str,
    action: &str,
    idempotency_key: &str,
) -> HttpRequest {
    HttpRequest::new(
        Method::Post,
        url(&format!(
            "/v1/machines/{}/{action}",
            encode_query(machine_id)
        )),
    )
    .bearer(key)
    .header("Idempotency-Key", idempotency_key)
    .json(json!({}))
}

pub fn delete_machine_request(key: &str, machine_id: &str, idempotency_key: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Delete,
        url(&format!("/v1/machines/{}", encode_query(machine_id))),
    )
    .bearer(key)
    .header("Idempotency-Key", idempotency_key)
    .header("X-Confirm-Delete", machine_id)
}

/// An Instabox operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operation {
    pub id: String,
    pub machine_id: Option<String>,
    /// `succeeded` and `failed` are terminal.
    pub state: String,
    pub phase: Option<String>,
    pub error: Option<String>,
}

impl Operation {
    pub fn is_done(&self) -> bool {
        matches!(self.state.as_str(), "succeeded" | "failed")
    }
}

pub fn parse_operation(response: &HttpResponse) -> Result<Operation, ProviderError> {
    let value = read(response)?;
    Ok(Operation {
        id: text(&value, "id")
            .ok_or_else(|| ProviderError::unexpected("Instabox sent no operation."))?,
        machine_id: text(&value, "machineId"),
        state: text(&value, "state").unwrap_or_default(),
        phase: text(&value, "phase"),
        error: value.get("error").and_then(|error| text(error, "message")),
    })
}

/// `GET /v1/operations/{id}?wait=25s`: long-polls up to 25 seconds.
pub fn operation_request(key: &str, operation_id: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Get,
        url(&format!(
            "/v1/operations/{}?wait=25s",
            encode_query(operation_id)
        )),
    )
    .bearer(key)
    .timeout(40)
}

// ---- Exec and files ------------------------------------------------------------------------

/// `POST /v1/machines/{id}/exec` with argv (no shell) and optional stdin; kept out of the box's
/// history so nothing Ghostex runs is listed there.
pub fn exec_request(
    key: &str,
    machine_id: &str,
    argv: &[&str],
    stdin: Option<&str>,
    timeout_seconds: u64,
) -> HttpRequest {
    let mut body = json!({
        "argv": argv,
        "timeoutSeconds": timeout_seconds.clamp(1, 600),
        "noHistory": true,
    });
    if let Some(stdin) = stdin {
        body["stdin"] = json!(stdin);
    }
    HttpRequest::new(
        Method::Post,
        url(&format!("/v1/machines/{}/exec", encode_query(machine_id))),
    )
    .bearer(key)
    .json(body)
    .timeout(timeout_seconds.clamp(1, 600) + 30)
}

pub fn parse_exec(response: &HttpResponse) -> Result<ExecOutput, ProviderError> {
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

/// `PUT /v1/machines/{id}/files?path=…&mode=0600&parents=true` with the raw bytes.
pub fn write_file_request(key: &str, machine_id: &str, path: &str, bytes: Vec<u8>) -> HttpRequest {
    let mut request = HttpRequest::new(
        Method::Put,
        url(&format!(
            "/v1/machines/{}/files?path={}&mode=0600&parents=true",
            encode_query(machine_id),
            encode_query(path)
        )),
    )
    .bearer(key);
    request.body = Some(crate::http::HttpBody::Bytes {
        content_type: "application/octet-stream".to_string(),
        bytes,
    });
    request
}

// ---- Snapshots -----------------------------------------------------------------------------

/// `POST /v1/machines/{id}/snapshots` (a live snapshot is `pending` until it uploads).
pub fn create_snapshot_request(
    key: &str,
    machine_id: &str,
    name: &str,
    idempotency_key: &str,
) -> HttpRequest {
    HttpRequest::new(
        Method::Post,
        url(&format!(
            "/v1/machines/{}/snapshots",
            encode_query(machine_id)
        )),
    )
    .bearer(key)
    .header("Idempotency-Key", idempotency_key)
    .json(json!({ "name": name, "description": "Ghostex Cloud Box snapshot" }))
}

pub fn get_snapshot_request(key: &str, snapshot_id: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Get,
        url(&format!("/v1/snapshots/{}", encode_query(snapshot_id))),
    )
    .bearer(key)
}

pub fn delete_snapshot_request(key: &str, snapshot_id: &str, idempotency_key: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Delete,
        url(&format!("/v1/snapshots/{}", encode_query(snapshot_id))),
    )
    .bearer(key)
    .header("Idempotency-Key", idempotency_key)
    .header("X-Confirm-Delete", snapshot_id)
}

pub fn parse_snapshot(response: &HttpResponse) -> Result<ProviderSnapshot, ProviderError> {
    let value = read(response)?;
    let size = |key: &str| value.get(key).and_then(Value::as_u64);
    Ok(ProviderSnapshot {
        provider_snapshot_id: text(&value, "id")
            .ok_or_else(|| ProviderError::unexpected("Instabox sent no snapshot."))?,
        name: text(&value, "name").unwrap_or_default(),
        status: text(&value, "status").unwrap_or_else(|| "pending".to_string()),
        source_box_id: text(&value, "sourceMachineId"),
        size_bytes: match (size("rootSizeBytes"), size("dataSizeBytes")) {
            (Some(root), Some(data)) => Some(root + data),
            (root, data) => root.or(data),
        },
        created_at: text(&value, "createdAt"),
        error: text(&value, "error"),
    })
}

/// `GET /v1/machines/{id}/usage?from=…`: running seconds since `from` (no dollars; R6).
pub fn machine_usage_request(key: &str, machine_id: &str, from: &str) -> HttpRequest {
    HttpRequest::new(
        Method::Get,
        url(&format!(
            "/v1/machines/{}/usage?from={}",
            encode_query(machine_id),
            encode_query(from)
        )),
    )
    .bearer(key)
}

pub fn parse_running_seconds(response: &HttpResponse) -> Result<u64, ProviderError> {
    Ok(read(response)?
        .get("runningSeconds")
        .and_then(Value::as_u64)
        .unwrap_or(0))
}
