use std::collections::HashMap;

use serde_json::{json, Value};

use crate::ghostex_cli::rpc::{CliError, CliResult};

use super::*;

/// getGhostexCdpClient: resolve the page, remember it as selected, and make
/// sure a live CDP client exists for it. Returns the resolved page target.
pub(super) fn get_ghostex_cdp_client(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let (page, _port) = resolve_ghostex_cdp_page(args, state)?;
    state.selected_page_id = page.get("id").and_then(Value::as_str).map(str::to_string);
    let page_id = page_id_of(&page);
    let needs_connect = match state.clients.get(&page_id) {
        Some(client) => client.closed,
        None => true,
    };
    if needs_connect {
        let ws_url = page
            .get("webSocketDebuggerUrl")
            .and_then(Value::as_str)
            .unwrap_or("");
        // A page served through gxserver needs its token; CEF's own port takes none.
        let bearer = if ws_url.contains("/api/cefDevtools/") {
            Some(crate::ghostex_cli::rpc::read_local_gxserver_auth_token()?)
        } else {
            None
        };
        let client = CdpClient::connect(ws_url, state.options.timeout_ms, bearer.as_deref())?;
        state.clients.insert(page_id, client);
    }
    Ok(page)
}

pub(super) fn page_id_of(page: &Value) -> String {
    page.get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Perform one CDP call on the page's client, then record any CDP events that
/// arrived on the socket while waiting (the JS client did this via onEvent).
pub(super) fn client_call(
    state: &mut McpState,
    page_id: &str,
    method: &str,
    params: Value,
) -> CliResult<Value> {
    let client = state
        .clients
        .get_mut(page_id)
        .ok_or_else(|| CliError::Other("CDP connection is closed".to_string()))?;
    let result = client.call(method, params);
    let events = std::mem::take(&mut client.events);
    for event in &events {
        record_ghostex_cdp_event(page_id, event, &mut state.captures);
    }
    result
}

pub(super) fn ensure_capture_enabled(state: &mut McpState, page_id: &str) -> CliResult<()> {
    let already_enabled = state
        .clients
        .get(page_id)
        .map(|client| client.capture_enabled)
        .unwrap_or(false);
    if already_enabled {
        return Ok(());
    }
    client_call(state, page_id, "Runtime.enable", json!({}))?;
    client_call(state, page_id, "Log.enable", json!({}))?;
    client_call(state, page_id, "Page.enable", json!({}))?;
    if let Some(client) = state.clients.get_mut(page_id) {
        client.capture_enabled = true;
    }
    Ok(())
}

/// Drain CDP events already buffered on the socket without blocking, so
/// console entries emitted between tool calls are captured like the evented
/// Node client captured them.
pub(super) fn pump_client_events(state: &mut McpState, page_id: &str) {
    let Some(client) = state.clients.get_mut(page_id) else {
        return;
    };
    client.pump_events();
    let events = std::mem::take(&mut client.events);
    for event in &events {
        record_ghostex_cdp_event(page_id, event, &mut state.captures);
    }
}

pub(super) fn resolve_ghostex_cdp_page(args: &Value, state: &McpState) -> CliResult<(Value, u64)> {
    let (pages, port) = discover_ghostex_cdp_pages(&state.options)?;
    if pages.is_empty() {
        return Err(CliError::Other(format!(
            "No Ghostex CEF pages found on 127.0.0.1:{port}"
        )));
    }
    let page_id = string_flag(
        defined(args.get("pageId"))
            .cloned()
            .or_else(|| defined(args.get("page")).cloned())
            .or_else(|| defined(args.get("target")).cloned())
            .or_else(|| state.selected_page_id.clone().map(Value::String))
            .as_ref(),
    );
    let mut page: Option<&Value> = match &page_id {
        Some(page_id) => pages
            .iter()
            .find(|candidate| candidate.get("id").and_then(Value::as_str) == Some(page_id)),
        None => None,
    };
    if page.is_none() {
        if let Some(index) = args.get("index").and_then(Value::as_f64) {
            if index.fract() == 0.0 && index >= 0.0 && (index as usize) < pages.len() {
                page = pages.get(index as usize);
            }
        }
    }
    let title_contains = string_flag(defined(args.get("titleContains")));
    let url_contains = string_flag(defined(args.get("urlContains")));
    if page.is_none() {
        if let Some(title_contains) = &title_contains {
            page = pages.iter().find(|candidate| {
                js_string_of(Some(&defaulted(candidate, "title"))).contains(title_contains.as_str())
            });
        }
    }
    if page.is_none() {
        if let Some(url_contains) = &url_contains {
            page = pages.iter().find(|candidate| {
                js_string_of(Some(&defaulted(candidate, "url"))).contains(url_contains.as_str())
            });
        }
    }
    let page = page.unwrap_or(&pages[0]);
    let has_ws_url = page
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .map(|url| !url.is_empty())
        .unwrap_or(false);
    if !has_ws_url {
        let id = page
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        return Err(CliError::Other(format!(
            "Ghostex CEF page {id} does not expose a DevTools WebSocket URL"
        )));
    }
    Ok((page.clone(), port))
}

/// The pages among DevTools targets, without DevTools windows.
fn page_targets(targets: &Value) -> Vec<Value> {
    targets
        .as_array()
        .map(|targets| {
            targets
                .iter()
                .filter(|target| {
                    target.get("type").and_then(Value::as_str) == Some("page")
                        && !js_string_of(Some(&defaulted(target, "url"))).starts_with("devtools://")
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/*
CDXC:Browser 2026-10-25 DECISION:
User (security audit, Q11 "also move the skill off the open port"): Ghostex's pages are reached through the local gxserver with its token (server/src/server/cef_devtools.rs), and CEF's own port is closed outside development. The CEF port scan stays as the fallback for a Ghostex from before this, for an app started with a debugging port, and for an explicit --port.
The live check (2026-10-26) found the scan taking over when this Ghostex's window was closed: the skill, pointed at one Ghostex, attached to another Ghostex that had its port open. A gxserver that answers that no app is connected (503) has the relay, so its own app is simply not running; the skill says so instead of scanning. Only a gxserver without the route, or none at all, leads to the scan.
*/
fn discover_pages_through_gxserver() -> Result<(Vec<Value>, u64), GxserverPagesError> {
    let token = crate::ghostex_cli::rpc::read_local_gxserver_auth_token()
        .map_err(|error| GxserverPagesError::Unreachable(error.to_string()))?;
    let port = crate::ghostex_cli::rpc::local_gxserver_api_port();
    let targets = http_json_with_bearer(
        &format!("http://127.0.0.1:{port}/api/cefDevtools/json"),
        &token,
        3000,
    )
    .map_err(|(status, message)| match status {
        Some(503) => GxserverPagesError::NoApp(message),
        _ => GxserverPagesError::Unreachable(message),
    })?;
    Ok((page_targets(&targets), u64::from(port)))
}

/// Why gxserver could not list the pages.
enum GxserverPagesError {
    /// This gxserver relays pages, but no Ghostex app is connected to it.
    NoApp(String),
    /// No gxserver with the relay answered: an older Ghostex, or none running.
    Unreachable(String),
}

pub(super) fn discover_ghostex_cdp_pages(
    options: &McpServerOptions,
) -> CliResult<(Vec<Value>, u64)> {
    let mut gxserver_error: Option<String> = None;
    if options.port.is_none() {
        match discover_pages_through_gxserver() {
            Ok(found) => return Ok(found),
            Err(GxserverPagesError::NoApp(message)) => {
                return Err(CliError::Other(format!(
                    "{message} Open Ghostex, or wait for it to finish starting, then try again."
                )));
            }
            Err(GxserverPagesError::Unreachable(error)) => gxserver_error = Some(error),
        }
    }
    let ports: Vec<u64> = match options.port {
        Some(explicit_port) => vec![explicit_port],
        None => unique_numbers(&[
            normalize_positive_integer_opt(env_value("GHOSTEX_CEF_REMOTE_DEBUGGING_PORT")),
            Some(9333),
            Some(9334),
            Some(9335),
            Some(9336),
            Some(9337),
            Some(9338),
            Some(9339),
            Some(9340),
            Some(9341),
            Some(9342),
            Some(9343),
        ]),
    };
    let mut last_error: Option<String> = None;
    for port in &ports {
        match http_json(&format!("http://127.0.0.1:{port}/json"), 450) {
            Ok(targets) => return Ok((page_targets(&targets), *port)),
            Err(error) => {
                last_error = Some(error);
            }
        }
    }
    let ports_text = ports
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let suffix = last_error
        .map(|message| format!(": {message}"))
        .unwrap_or_default();
    let through_gxserver = gxserver_error
        .map(|error| format!("Could not reach Ghostex's pages through gxserver ({error}). "))
        .unwrap_or_default();
    Err(CliError::Other(format!(
        "{through_gxserver}Could not reach Ghostex CEF DevTools on ports {ports_text}{suffix}"
    )))
}

fn record_ghostex_cdp_event(
    page_id: &str,
    event: &Value,
    captures: &mut HashMap<String, Vec<Value>>,
) {
    let method = event.get("method").and_then(Value::as_str).unwrap_or("");
    if method.is_empty() {
        return;
    }
    let params = event.get("params").cloned().unwrap_or(Value::Null);
    let mut push_entry = |mut entry: serde_json::Map<String, Value>| {
        entry.insert(
            "timestamp".to_string(),
            Value::String(
                chrono::Utc::now()
                    .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                    .to_string(),
            ),
        );
        let entries = captures.entry(page_id.to_string()).or_default();
        entries.push(Value::Object(entry));
        if entries.len() > 1000 {
            let excess = entries.len() - 1000;
            entries.drain(..excess);
        }
    };
    if method == "Runtime.consoleAPICalled" {
        let event_args: Vec<Value> = params
            .get("args")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut entry = serde_json::Map::new();
        entry.insert(
            "args".to_string(),
            Value::Array(
                event_args
                    .iter()
                    .map(|value| normalize_remote_object(Some(value)))
                    .collect(),
            ),
        );
        entry.insert(
            "level".to_string(),
            defined(params.get("type"))
                .cloned()
                .unwrap_or_else(|| json!("log")),
        );
        entry.insert("source".to_string(), json!("console"));
        entry.insert(
            "stackTrace".to_string(),
            params.get("stackTrace").cloned().unwrap_or(Value::Null),
        );
        entry.insert(
            "text".to_string(),
            Value::String(
                event_args
                    .iter()
                    .map(remote_object_text)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        );
        push_entry(entry);
    } else if method == "Runtime.exceptionThrown" {
        let exception_details = params
            .get("exceptionDetails")
            .cloned()
            .unwrap_or(Value::Null);
        let mut entry = serde_json::Map::new();
        entry.insert("exception".to_string(), exception_details.clone());
        entry.insert("level".to_string(), json!("error"));
        entry.insert("source".to_string(), json!("exception"));
        entry.insert(
            "text".to_string(),
            defined(exception_details.get("text"))
                .cloned()
                .unwrap_or_else(|| json!("JavaScript exception")),
        );
        push_entry(entry);
    } else if method == "Log.entryAdded" {
        let log_entry = params.get("entry").cloned().unwrap_or(Value::Null);
        let mut entry = serde_json::Map::new();
        entry.insert(
            "level".to_string(),
            defined(log_entry.get("level"))
                .cloned()
                .unwrap_or_else(|| json!("info")),
        );
        entry.insert(
            "source".to_string(),
            defined(log_entry.get("source"))
                .cloned()
                .unwrap_or_else(|| json!("browser")),
        );
        entry.insert(
            "text".to_string(),
            defined(log_entry.get("text"))
                .cloned()
                .unwrap_or_else(|| json!("")),
        );
        entry.insert(
            "url".to_string(),
            log_entry.get("url").cloned().unwrap_or(Value::Null),
        );
        push_entry(entry);
    }
}

/// evaluateFunction: run one of the verbatim page scripts with JSON args and
/// return `result.result.value` (None mirrors JS `undefined`).
pub(super) fn evaluate_function(
    state: &mut McpState,
    page_id: &str,
    function_source: &str,
    args: Value,
) -> CliResult<Option<Value>> {
    let args_json =
        serde_json::to_string(&args).map_err(|error| CliError::Other(error.to_string()))?;
    let expression = format!("({function_source})(...{args_json})");
    let result = client_call(
        state,
        page_id,
        "Runtime.evaluate",
        json!({
            "awaitPromise": true,
            "expression": expression,
            "returnByValue": true,
        }),
    )?;
    if let Some(exception_details) = defined(result.get("exceptionDetails")) {
        let message = exception_details
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("Browser evaluation failed")
            .to_string();
        return Err(CliError::Other(message));
    }
    Ok(result
        .get("result")
        .and_then(|remote| remote.get("value"))
        .cloned())
}

pub(super) fn resolve_browser_element_selector(
    args: &Value,
    state: &McpState,
    page_id: &str,
) -> CliResult<String> {
    if let Some(selector) = string_flag(defined(args.get("selector"))) {
        return Ok(selector);
    }
    let element_ref =
        string_flag(defined(args.get("ref")).or_else(|| defined(args.get("element"))))
            .ok_or_else(|| CliError::Other("Expected selector or ref".to_string()))?;
    let mapped = state
        .ref_maps
        .get(page_id)
        .and_then(|ref_map| ref_map.get(&element_ref));
    match mapped {
        Some(selector) => Ok(selector.clone()),
        None => Err(CliError::Other(format!(
            "Unknown element ref {element_ref}. Run ghostex_snapshot again for fresh refs."
        ))),
    }
}

pub(super) fn normalize_remote_object(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    if !js_truthy(value) {
        return Value::Null;
    }
    if let Some(object) = value.as_object() {
        if let Some(inner) = object.get("value") {
            return inner.clone();
        }
        if let Some(unserializable) = object.get("unserializableValue") {
            if js_truthy(unserializable) {
                return unserializable.clone();
            }
        }
        if let Some(description) = defined(object.get("description")) {
            return description.clone();
        }
        if let Some(object_type) = defined(object.get("type")) {
            return object_type.clone();
        }
    }
    Value::Null
}

pub(super) fn remote_object_text(value: &Value) -> String {
    match normalize_remote_object(Some(value)) {
        Value::String(text) => text,
        normalized => serde_json::to_string(&normalized).unwrap_or_else(|_| "null".to_string()),
    }
}

pub(super) fn key_event_for_browser_mcp(key: &str) -> Value {
    let special: Option<(&str, u32)> = match key {
        "ArrowDown" => Some(("ArrowDown", 40)),
        "ArrowLeft" => Some(("ArrowLeft", 37)),
        "ArrowRight" => Some(("ArrowRight", 39)),
        "ArrowUp" => Some(("ArrowUp", 38)),
        "Backspace" => Some(("Backspace", 8)),
        "Delete" => Some(("Delete", 46)),
        "Enter" => Some(("Enter", 13)),
        "Escape" => Some(("Escape", 27)),
        "Tab" => Some(("Tab", 9)),
        _ => None,
    };
    if let Some((name, virtual_key_code)) = special {
        return json!({
            "code": name,
            "key": name,
            "windowsVirtualKeyCode": virtual_key_code,
        });
    }
    // JS `key.length === 1` counts UTF-16 code units.
    let is_single_unit = key.encode_utf16().count() == 1;
    let text = if is_single_unit {
        key.to_string()
    } else {
        String::new()
    };
    let upper = key.to_uppercase();
    let code = if !text.is_empty() {
        format!("Key{upper}")
    } else {
        key.to_string()
    };
    let virtual_key_code: u16 = if !text.is_empty() {
        upper.encode_utf16().next().unwrap_or(0)
    } else {
        0
    };
    json!({
        "code": code,
        "key": key,
        "text": text,
        "windowsVirtualKeyCode": virtual_key_code,
    })
}

pub(super) fn cdp_page_summary(page: &Value) -> Value {
    let mut summary = serde_json::Map::new();
    if let Some(id) = page.get("id") {
        summary.insert("id".to_string(), id.clone());
    }
    summary.insert("title".to_string(), defaulted(page, "title"));
    summary.insert("url".to_string(), defaulted(page, "url"));
    Value::Object(summary)
}

/// `/^[a-z][a-z0-9+.-]*:/i`
pub(super) fn normalize_browser_navigation_url(value: &str) -> String {
    let mut chars = value.chars();
    let has_scheme = match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => loop {
            match chars.next() {
                Some(':') => break true,
                Some(next)
                    if next.is_ascii_alphanumeric()
                        || next == '+'
                        || next == '.'
                        || next == '-' =>
                {
                    continue;
                }
                _ => break false,
            }
        },
        _ => false,
    };
    if has_scheme {
        value.to_string()
    } else {
        format!("https://{value}")
    }
}
