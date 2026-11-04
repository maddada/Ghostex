//! The provider rules as one JSON call, for the phone's WebAssembly core (`gx_cloud_call` in
//! `packages/gx-chat-wasm`): JavaScript sends each request this builds with `fetch` and hands the
//! answer back to be read, so the phone starts, wakes, stops and deletes boxes with the computer
//! off on exactly the rules home gxserver uses (`server/src/cloud_boxes/ops.rs` runs the same
//! builders with its HTTP client).
//!
//! CDXC:CloudBoxes 2026-11-02 SEE-ALSO: apps/mobile/app/src/cloudBoxes/providerOps.ts (the
//! sequence, the twin of `Ops`), packages/gx-chat-wasm/src/lib.rs (`gx_cloud_call`).
//!
//! `call(name, arguments)` answers `{"ok": …}`, `{"failed": {status, code, message, retryable}}` for
//! a provider's refusal read by `parse`, or `{"error": {code, message}}` for bad arguments.
//! A built request is `{method, url, headers: [[name, value]], body?, timeoutSeconds}` with `body`
//! one of `{json}`, `{form: [[k, v]]}` or `{bytesBase64url, contentType}`. Headers carry the
//! provider key: never log a request.

use ghostex_gx_protocol::cloud_box::{
    BoxBootTicket, CloudBoxCreatedBy, CloudBoxState, CloudProviderId,
};
use serde_json::{json, Value};

use crate::http::{HttpBody, HttpRequest, HttpResponse, ProviderError};
use crate::metadata::BoxMetadata;
use crate::provider::{ExecOutput, ProviderMachine};
use crate::{boat, ids, instabox, install, prices, ticket};

fn ok(value: Value) -> Value {
    json!({ "ok": value })
}

fn fail(message: impl Into<String>) -> Value {
    json!({ "error": { "status": 0, "code": "badArguments", "message": message.into(), "retryable": false } })
}

/// A provider's refusal is an answer, not a failed call: `{"failed": {status, code, message,
/// retryable}}`, so the status (a 404 for a gone box) reaches the caller.
fn provider_error(error: ProviderError) -> Value {
    json!({ "failed": {
        "status": error.status,
        "code": error.code,
        "message": error.message,
        "retryable": error.retryable,
    } })
}

fn text<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments.get(key).and_then(Value::as_str)
}

fn need<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, Value> {
    text(arguments, key)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| fail(format!("`{key}` is missing.")))
}

fn provider_of(arguments: &Value) -> Result<CloudProviderId, Value> {
    serde_json::from_value(arguments.get("provider").cloned().unwrap_or(Value::Null))
        .map_err(|_| fail("`provider` is missing."))
}

fn request_json(request: HttpRequest) -> Value {
    let body = request.body.map(|body| match body {
        HttpBody::Json(value) => json!({ "json": value }),
        HttpBody::Form(fields) => json!({ "form": fields }),
        HttpBody::Bytes {
            content_type,
            bytes,
        } => json!({
            "bytesBase64url": ticket::base64url_encode(&bytes),
            "contentType": content_type,
        }),
    });
    let mut out = json!({
        "method": request.method.as_str(),
        "url": request.url,
        "headers": request.headers,
        "timeoutSeconds": request.timeout_seconds,
    });
    if let Some(body) = body {
        out["body"] = body;
    }
    out
}

fn response_of(arguments: &Value) -> Result<HttpResponse, Value> {
    let response = arguments
        .get("response")
        .ok_or_else(|| fail("`response` is missing."))?;
    Ok(HttpResponse {
        status: response.get("status").and_then(Value::as_u64).unwrap_or(0) as u16,
        body: response
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn state_text(state: &CloudBoxState) -> Value {
    serde_json::to_value(state).unwrap_or(Value::Null)
}

fn machine_json(machine: &ProviderMachine) -> Value {
    let metadata = machine.metadata.as_ref().map(|metadata| {
        json!({
            "boxId": metadata.box_id,
            "projectKey": metadata.project_key,
            "homeProjectId": metadata.home_project_id,
            "homeMachineId": metadata.home_machine_id,
            "snapshotId": metadata.snapshot_id,
            "size": metadata.size,
            "createdBy": metadata.created_by.as_ref().map(CloudBoxCreatedBy::as_str),
            "createdAt": metadata.created_at,
        })
    });
    json!({
        "providerBoxId": machine.provider_box_id,
        "state": state_text(&machine.state),
        "metadata": metadata,
        "createdAt": machine.created_at,
        "startedAt": machine.started_at,
        "stoppedAt": machine.stopped_at,
        "costMonthUsd": machine.cost_month_usd,
        "size": machine.size,
        "error": machine.error,
        "operationId": machine.operation_id,
    })
}

fn exec_json(output: ExecOutput) -> Value {
    json!({
        "exitCode": output.exit_code,
        "stdout": output.stdout,
        "stderr": output.stderr,
        "timedOut": output.timed_out,
        "json": output.json_object(),
        "failureTail": output.failure_tail(),
        "missingBoxCommand": output.missing_box_command(),
    })
}

fn metadata_of(arguments: &Value) -> Result<BoxMetadata, Value> {
    let metadata = arguments
        .get("metadata")
        .ok_or_else(|| fail("`metadata` is missing."))?;
    let text = |key: &str| {
        metadata
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Ok(BoxMetadata {
        box_id: text("boxId").ok_or_else(|| fail("`metadata.boxId` is missing."))?,
        project_key: text("projectKey").unwrap_or_default(),
        home_project_id: text("homeProjectId"),
        home_machine_id: text("homeMachineId"),
        snapshot_id: text("snapshotId"),
        size: text("size"),
        idempotency_key: text("idempotencyKey"),
        created_by: text("createdBy").map(|by| CloudBoxCreatedBy::from_wire(&by)),
        created_at: text("createdAt"),
    })
}

fn argv_of(arguments: &Value) -> Result<Vec<String>, Value> {
    arguments
        .get("argv")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .filter(|argv: &Vec<String>| !argv.is_empty())
        .ok_or_else(|| fail("`argv` is missing."))
}

/// One call by name. Unknown names answer an error, never panic.
pub fn call(name: &str, arguments: &str) -> String {
    let arguments: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
    let answer = match dispatch(name, &arguments) {
        Ok(value) => value,
        Err(error) => error,
    };
    answer.to_string()
}

fn dispatch(name: &str, arguments: &Value) -> Result<Value, Value> {
    match name {
        "newBoxId" => {
            let entropy = ticket::base64url_decode(need(arguments, "entropyBase64url")?)
                .filter(|bytes| bytes.len() >= 8)
                .ok_or_else(|| fail("`entropyBase64url` needs at least 8 bytes."))?;
            let box_id = ids::gx_id_from_entropy(&entropy);
            Ok(ok(
                json!({ "boxId": box_id, "name": ids::box_display_name(&box_id) }),
            ))
        }
        "encodeTicket" => {
            let ticket: BoxBootTicket =
                serde_json::from_value(arguments.get("ticket").cloned().unwrap_or(Value::Null))
                    .map_err(|error| fail(format!("The boot ticket is not valid: {error}")))?;
            Ok(ok(json!({ "ticket": ticket::encode_ticket(&ticket) })))
        }
        "installScript" => Ok(ok(
            json!({ "script": install::install_script(need(arguments, "version")?) }),
        )),
        "size" => {
            let provider = provider_of(arguments)?;
            let size = prices::size_or_default(&provider, text(arguments, "size"))
                .ok_or_else(|| fail("That provider offers no box sizes."))?;
            Ok(ok(serde_json::to_value(size).unwrap_or(Value::Null)))
        }
        "request" => build(arguments).map(|request| ok(request_json(request))),
        "parse" => parse(arguments),
        other => Err(fail(format!("Unknown cloud call {other}."))),
    }
}

/// Builds one provider request: `{provider, op, key, …}`.
fn build(arguments: &Value) -> Result<HttpRequest, Value> {
    let provider = provider_of(arguments)?;
    let op = need(arguments, "op")?;
    let key = need(arguments, "key")?;
    let box_ref = || need(arguments, "providerBoxId");
    let idempotency = || need(arguments, "idempotencyKey");
    match (&provider, op) {
        (CloudProviderId::Instabox, "list") => Ok(instabox::list_machines_request(
            key,
            text(arguments, "cursor"),
        )),
        (CloudProviderId::Instabox, "get") => Ok(instabox::get_machine_request(key, box_ref()?)),
        (CloudProviderId::Instabox, "create") => {
            let metadata = metadata_of(arguments)?;
            let size = prices::size_or_default(&provider, metadata.size.as_deref())
                .ok_or_else(|| fail("Instabox offers no box sizes."))?;
            Ok(instabox::create_machine_request(
                key,
                idempotency()?,
                &instabox::CreateSpec {
                    metadata: &metadata,
                    provider_snapshot_id: text(arguments, "providerSnapshotId"),
                    vcpus: size.vcpus,
                    mem_mib: size.mem_mib,
                    idle_minutes: arguments
                        .get("idleMinutes")
                        .and_then(Value::as_u64)
                        .unwrap_or(ghostex_gx_protocol::cloud_box::DEFAULT_IDLE_STOP_MINUTES),
                },
            ))
        }
        (CloudProviderId::Instabox, "operation") => Ok(instabox::operation_request(
            key,
            need(arguments, "operationId")?,
        )),
        (CloudProviderId::Instabox, "action") => Ok(instabox::machine_action_request(
            key,
            box_ref()?,
            need(arguments, "action")?,
            idempotency()?,
        )),
        (CloudProviderId::Instabox, "delete") => Ok(instabox::delete_machine_request(
            key,
            box_ref()?,
            idempotency()?,
        )),
        (CloudProviderId::Instabox, "exec") => {
            let argv = argv_of(arguments)?;
            let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
            Ok(instabox::exec_request(
                key,
                box_ref()?,
                &argv,
                text(arguments, "stdin"),
                arguments
                    .get("timeoutSeconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(120),
            ))
        }
        (CloudProviderId::Instabox, "writeFile") => Ok(instabox::write_file_request(
            key,
            box_ref()?,
            need(arguments, "path")?,
            text(arguments, "contents")
                .unwrap_or_default()
                .as_bytes()
                .to_vec(),
        )),
        (CloudProviderId::Boat, "list") => {
            Ok(boat::list_sandboxes_request(key, text(arguments, "cursor")))
        }
        (CloudProviderId::Boat, "get") => Ok(boat::get_sandbox_request(key, box_ref()?)),
        (CloudProviderId::Boat, "create") => {
            let metadata = metadata_of(arguments)?;
            let size = prices::size_or_default(&provider, metadata.size.as_deref())
                .ok_or_else(|| fail("Boat offers no box sizes."))?;
            Ok(boat::create_sandbox_request(
                key,
                idempotency()?,
                &metadata,
                text(arguments, "providerSnapshotId"),
                &size.id,
            ))
        }
        (CloudProviderId::Boat, "action") => Ok(boat::sandbox_action_request(
            key,
            box_ref()?,
            need(arguments, "action")?,
        )),
        (CloudProviderId::Boat, "delete") => Ok(boat::delete_sandbox_request(key, box_ref()?)),
        (CloudProviderId::Boat, "exec") => {
            let argv = argv_of(arguments)?;
            let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
            Ok(boat::command_request(
                key,
                box_ref()?,
                &argv,
                text(arguments, "stdin"),
                arguments
                    .get("timeoutSeconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(120),
            ))
        }
        (CloudProviderId::Boat, "writeFile") => Ok(boat::write_file_request(
            key,
            box_ref()?,
            need(arguments, "path")?,
            text(arguments, "contents").unwrap_or_default().as_bytes(),
        )),
        _ => Err(fail(format!(
            "{} has no `{op}` the phone can call.",
            provider.label()
        ))),
    }
}

/// Reads one provider answer: `{provider, op, response: {status, body}}`.
fn parse(arguments: &Value) -> Result<Value, Value> {
    let provider = provider_of(arguments)?;
    let op = need(arguments, "op")?;
    let response = response_of(arguments)?;
    let label = provider.label().to_string();
    let parsed = match (&provider, op) {
        (CloudProviderId::Instabox, "list") => instabox::parse_machine_list(&response).map(|(machines, next)| {
            json!({ "machines": machines.iter().map(machine_json).collect::<Vec<_>>(), "nextCursor": next })
        }),
        (CloudProviderId::Instabox, "get") => instabox::parse_machine(&response).map(|machine| machine_json(&machine)),
        (CloudProviderId::Instabox, "create" | "operation" | "action") => {
            instabox::parse_operation(&response).map(|operation| {
                json!({
                    "operationId": operation.id,
                    "providerBoxId": operation.machine_id,
                    "state": operation.state,
                    "phase": operation.phase,
                    "error": operation.error,
                    "done": operation.is_done(),
                })
            })
        }
        (CloudProviderId::Instabox, "exec") => instabox::parse_exec(&response).map(exec_json),
        (CloudProviderId::Boat, "list") => boat::parse_sandbox_list(&response).map(|(machines, next, _)| {
            json!({ "machines": machines.iter().map(machine_json).collect::<Vec<_>>(), "nextCursor": next })
        }),
        (CloudProviderId::Boat, "get" | "create") => boat::parse_sandbox(&response).map(|machine| machine_json(&machine)),
        (CloudProviderId::Boat, "exec") => boat::parse_command(&response).map(exec_json),
        // Delete, file writes and Boat's actions: only whether it worked (a gone box is fine for delete).
        (_, "delete") if response.status == 404 => Ok(Value::Null),
        (_, "delete" | "writeFile" | "action") => crate::http::json_or_error(&response, &label),
        _ => return Err(fail(format!("{label} has no `{op}` answer the phone reads."))),
    };
    Ok(match parsed {
        Ok(value) => ok(value),
        Err(error) => provider_error(error),
    })
}
