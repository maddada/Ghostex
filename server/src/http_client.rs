use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::{
    config::read_selected_local_api_port,
    constants::{
        GXSERVER_LOCAL_API_HOST, GXSERVER_PRODUCT, GXSERVER_PROTOCOL_HEADER,
        GXSERVER_PROTOCOL_VERSION,
    },
    protocol::ServerHealthResponse,
};

pub fn fetch_server_health(
    token: Option<&str>,
    timeout_ms: u64,
) -> Result<Option<ServerHealthResponse>> {
    let Some(value) = fetch_local_json("/api/health/server", "GET", token, timeout_ms)? else {
        return Ok(None);
    };
    if !is_gxserver_health_payload(&value)? {
        return Ok(None);
    }
    let health: ServerHealthResponse =
        serde_json::from_value(value).with_context(|| "parse gxserver health")?;
    Ok(Some(health))
}

/*
CDXC:GxserverCli 2026-06-22-04:47:
CLI lifecycle probes must match TypeScript before deserializing health. Ignore other products on the selected port, and surface same-product protocol mismatches as command errors instead of reporting a misleading running status.

CDXC:GxserverCli 2026-06-22-05:01:
Startup and status probes must parse both Rust content-length responses and TypeScript chunked responses. Otherwise a selected-port TypeScript gxserver looks like a non-gxserver conflict, which blocks Rust replacement instead of stopping the compatible old control plane.
*/
fn is_gxserver_health_payload(value: &Value) -> Result<bool> {
    if value.get("product").and_then(Value::as_str) != Some(GXSERVER_PRODUCT) {
        return Ok(false);
    }
    let actual_protocol_version = value.get("protocolVersion");
    if actual_protocol_version.and_then(Value::as_u64) != Some(GXSERVER_PROTOCOL_VERSION) {
        let actual = actual_protocol_version
            .cloned()
            .map(js_string_for_protocol_version)
            .unwrap_or_else(|| "undefined".to_string());
        bail!(
            "gxserver protocol mismatch. Expected protocol {GXSERVER_PROTOCOL_VERSION}, got {actual}. Update Ghostex and gxserver so their protocol versions match."
        );
    }
    Ok(true)
}

fn js_string_for_protocol_version(value: Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => {
            if value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Number(value) => value.to_string(),
        Value::String(value) => value,
        Value::Array(values) => values
            .into_iter()
            .map(|value| match value {
                Value::Null => String::new(),
                other => js_string_for_protocol_version(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

pub fn request_server_stop(token: Option<&str>, timeout_ms: u64) -> Result<bool> {
    let response = fetch_local_json("/api/control/stop", "POST", token, timeout_ms)?;
    Ok(response
        .and_then(|value| value.get("ok").and_then(Value::as_bool))
        .unwrap_or(false))
}

pub fn request_server_stop_all(token: Option<&str>, timeout_ms: u64) -> Result<Option<Value>> {
    let response = fetch_local_json("/api/control/stopAll", "POST", token, timeout_ms)?;
    Ok(response.filter(|value| value.get("ok").and_then(Value::as_bool) == Some(true)))
}

/*
CDXC:GhostexRustCli 2026-07-13:
The Rust ghostex CLI bridges arbitrary local gxserver API endpoints the same
way the bundled Node CLI does. Callers pass a params object; the protocol
version is merged in here so every bridge request carries the contract the
server enforces on POST bodies.
*/
pub fn post_local_api(
    path: &str,
    params: Option<&Value>,
    token: Option<&str>,
    timeout_ms: u64,
) -> Result<Option<Value>> {
    let mut body = match params {
        Some(Value::Object(map)) => Value::Object(map.clone()),
        Some(other) => other.clone(),
        None => Value::Object(serde_json::Map::new()),
    };
    if let Value::Object(map) = &mut body {
        map.insert(
            "protocolVersion".to_string(),
            Value::from(GXSERVER_PROTOCOL_VERSION),
        );
    }
    fetch_local_json_with_body(path, "POST", token, timeout_ms, &body.to_string())
}

fn fetch_local_json(
    path: &str,
    method: &str,
    token: Option<&str>,
    timeout_ms: u64,
) -> Result<Option<Value>> {
    let body = if method == "POST" {
        format!(r#"{{"protocolVersion":{GXSERVER_PROTOCOL_VERSION}}}"#)
    } else {
        String::new()
    };
    fetch_local_json_with_body(path, method, token, timeout_ms, &body)
}

fn fetch_local_json_with_body(
    path: &str,
    method: &str,
    token: Option<&str>,
    timeout_ms: u64,
    body: &str,
) -> Result<Option<Value>> {
    let address = format!(
        "{GXSERVER_LOCAL_API_HOST}:{}",
        read_selected_local_api_port()?
    );
    let timeout = Duration::from_millis(timeout_ms);
    let Ok(mut stream) = TcpStream::connect(&address) else {
        return Ok(None);
    };
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{GXSERVER_PROTOCOL_HEADER}: {GXSERVER_PROTOCOL_VERSION}\r\n"
    );
    if let Some(token) = token {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if method == "POST" {
        request.push_str("Content-Type: application/json\r\n");
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    request.push_str(&body);
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (headers, body) = match response.split_once("\r\n\r\n") {
        Some(parts) => parts,
        None => return Ok(None),
    };
    let status_ok = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .map(|status| (200..300).contains(&status))
        .unwrap_or(false);
    if !status_ok {
        return Ok(None);
    }
    let Ok(body) = decode_http_body(headers, body) else {
        return Ok(None);
    };
    match serde_json::from_str(body.trim()) {
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None),
    }
}

fn decode_http_body(headers: &str, body: &str) -> Result<String> {
    if !has_chunked_transfer_encoding(headers) {
        return Ok(body.to_string());
    }
    decode_chunked_body(body)
}

fn has_chunked_transfer_encoding(headers: &str) -> bool {
    headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.trim().eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
    })
}

fn decode_chunked_body(body: &str) -> Result<String> {
    let bytes = body.as_bytes();
    let mut index = 0;
    let mut output = Vec::new();
    loop {
        let Some(line_end) = find_crlf(bytes, index) else {
            bail!("Invalid chunked gxserver response.");
        };
        let size_text = std::str::from_utf8(&bytes[index..line_end])
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        let size = usize::from_str_radix(size_text, 16)
            .with_context(|| "parse chunked gxserver response size")?;
        index = line_end + 2;
        if size == 0 {
            return String::from_utf8(output)
                .with_context(|| "decode chunked gxserver response body");
        }
        let chunk_end = index
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| anyhow::anyhow!("Invalid chunked gxserver response."))?;
        output.extend_from_slice(&bytes[index..chunk_end]);
        index = chunk_end;
        if bytes.get(index..index + 2) != Some(b"\r\n") {
            bail!("Invalid chunked gxserver response.");
        }
        index += 2;
    }
}

fn find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|offset| start + offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn health_payload_filter_ignores_other_products() {
        let payload = json!({
            "ok": true,
            "product": "other",
            "protocolVersion": GXSERVER_PROTOCOL_VERSION
        });

        assert!(!is_gxserver_health_payload(&payload).expect("filter"));
    }

    #[test]
    fn health_payload_filter_reports_protocol_mismatch_like_typescript() {
        let payload = json!({
            "ok": true,
            "product": GXSERVER_PRODUCT,
            "protocolVersion": 999
        });

        let error = is_gxserver_health_payload(&payload).expect_err("protocol mismatch");
        assert_eq!(
            error.to_string(),
            "gxserver protocol mismatch. Expected protocol 1, got 999. Update Ghostex and gxserver so their protocol versions match."
        );
    }

    #[test]
    fn chunked_health_response_body_decodes_like_typescript_server() {
        let decoded = decode_http_body(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n",
            "7\r\n{\"ok\":1\r\n1\r\n}\r\n0\r\n\r\n",
        )
        .expect("decode chunked body");

        assert_eq!(decoded, r#"{"ok":1}"#);
    }
}
