use std::collections::HashMap;

use serde_json::{json, Value};

use crate::ghostex_cli::rpc::{CliError, CliResult};

use super::*;

/// The agent's project's pages (agent_pages.rs), in the endpoint's order, with the page it
/// opened marked `agentPage` and the one its project's Browser shows marked `front`. Pages of other
/// projects are not listed; a pageId still reaches them.
pub(super) fn browser_mcp_list_pages(state: &mut McpState) -> CliResult<Value> {
    let (pages, port) = discover_ghostex_cdp_pages(&state.options)?;
    let agent_pages = fetch_agent_pages(&state.options.scope)?;
    let selected_page_id = state
        .selected_page_id
        .clone()
        .map(Value::String)
        .unwrap_or(Value::Null);
    let (project_id, page_entries) = match &agent_pages {
        AgentPages::Project(project) => {
            let entries: Vec<Value> = pages
                .iter()
                .filter_map(|page| {
                    let id = page_id_of(page);
                    let own = project
                        .pages
                        .iter()
                        .find(|own| own.target_id.as_deref() == Some(id.as_str()))?;
                    Some((page, own))
                })
                .enumerate()
                .map(|(index, (page, own))| {
                    let mut entry = serde_json::Map::new();
                    entry.insert("index".to_string(), json!(index));
                    if let Some(id) = page.get("id") {
                        entry.insert("id".to_string(), id.clone());
                    }
                    entry.insert("title".to_string(), defaulted(page, "title"));
                    entry.insert("type".to_string(), defaulted(page, "type"));
                    entry.insert("url".to_string(), defaulted(page, "url"));
                    entry.insert("agentPage".to_string(), json!(own.agent_page));
                    entry.insert("front".to_string(), json!(own.front));
                    let selected =
                        state.selected_page_id.as_deref() == page.get("id").and_then(Value::as_str);
                    entry.insert("selected".to_string(), Value::Bool(selected));
                    Value::Object(entry)
                })
                .collect();
            (json!(project.project_id), entries)
        }
        AgentPages::NoProject => (Value::Null, Vec::new()),
    };
    let mut answer = json!({
        "port": port,
        "projectId": project_id,
        "selectedPageId": selected_page_id,
        "pages": page_entries,
    });
    if page_entries_is_empty(&answer) {
        answer["note"] = json!(no_agent_page_error(&agent_pages, &state.options.scope).to_string());
    }
    Ok(answer)
}

fn page_entries_is_empty(answer: &Value) -> bool {
    answer
        .get("pages")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
}

pub(super) fn browser_mcp_select_page(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let (page, _port) = resolve_ghostex_cdp_page(args, state)?;
    state.selected_page_id = page.get("id").and_then(Value::as_str).map(str::to_string);
    // The agent's choice stays its page for the bridges after this one (agent_pages.rs).
    if let (Some(session), Some(page_id)) = (
        state.options.scope.agent_session.as_deref(),
        state.selected_page_id.as_deref(),
    ) {
        remember_agent_page(session, Some(page_id));
    }
    Ok(json!({ "selected": cdp_page_summary(&page) }))
}

pub(super) fn browser_mcp_navigate(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let url = string_flag(defined(args.get("url")))
        .ok_or_else(|| CliError::Other("ghostex_navigate requires url".to_string()))?;
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    client_call(state, &page_id, "Page.enable", json!({}))?;
    let result = client_call(
        state,
        &page_id,
        "Page.navigate",
        json!({ "url": normalize_browser_navigation_url(&url) }),
    )?;
    let mut response = serde_json::Map::new();
    if let Some(frame_id) = result.get("frameId") {
        response.insert("frameId".to_string(), frame_id.clone());
    }
    response.insert("page".to_string(), cdp_page_summary(&page));
    Ok(Value::Object(response))
}

pub(super) fn browser_mcp_evaluate(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let script = string_flag(defined(args.get("script")))
        .ok_or_else(|| CliError::Other("ghostex_evaluate requires script".to_string()))?;
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    let await_promise = args.get("awaitPromise") != Some(&Value::Bool(false));
    let result = client_call(
        state,
        &page_id,
        "Runtime.evaluate",
        json!({
            "awaitPromise": await_promise,
            "expression": script,
            "returnByValue": true,
        }),
    )?;
    if let Some(exception) = defined(result.get("exceptionDetails")) {
        return Ok(json!({
            "exception": exception,
            "ok": false,
            "page": cdp_page_summary(&page),
        }));
    }
    Ok(json!({
        "ok": true,
        "page": cdp_page_summary(&page),
        "result": normalize_remote_object(result.get("result")),
    }))
}

pub(super) fn browser_mcp_console_logs(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    ensure_capture_enabled(state, &page_id)?;
    pump_client_events(state, &page_id);
    let entries = state.captures.get(&page_id).cloned().unwrap_or_default();
    let limit =
        normalize_positive_integer_opt(defined(args.get("limit")).cloned()).unwrap_or(200) as usize;
    let selected: Vec<Value> = entries
        .iter()
        .skip(entries.len().saturating_sub(limit))
        .cloned()
        .collect();
    if args.get("clear") == Some(&Value::Bool(true)) {
        state.captures.insert(page_id.clone(), Vec::new());
    }
    Ok(json!({
        "entries": selected,
        "page": cdp_page_summary(&page),
        "total": entries.len(),
    }))
}

pub(super) fn browser_mcp_snapshot(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    let limit = normalize_positive_integer_opt(defined(args.get("limit")).cloned()).unwrap_or(120);
    let snapshot = evaluate_function(state, &page_id, GHOSTEX_SNAPSHOT_SCRIPT, json!([limit]))?;
    let mut ref_map: HashMap<String, String> = HashMap::new();
    if let Some(elements) = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.get("elements"))
        .and_then(Value::as_array)
    {
        for element in elements {
            let element_ref = element.get("ref").and_then(Value::as_str).unwrap_or("");
            let selector = element
                .get("selector")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !element_ref.is_empty() && !selector.is_empty() {
                ref_map.insert(element_ref.to_string(), selector.to_string());
            }
        }
    }
    state.ref_maps.insert(page_id.clone(), ref_map);
    let mut response = serde_json::Map::new();
    response.insert("page".to_string(), cdp_page_summary(&page));
    if let Some(snapshot) = snapshot {
        response.insert("snapshot".to_string(), snapshot);
    }
    Ok(Value::Object(response))
}

pub(super) fn browser_mcp_click(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    let selector = resolve_browser_element_selector(args, state, &page_id)?;
    let result = evaluate_function(state, &page_id, GHOSTEX_CLICK_SCRIPT, json!([selector]))?;
    let mut response = serde_json::Map::new();
    if let Some(result) = result {
        response.insert("clicked".to_string(), result);
    }
    response.insert("page".to_string(), cdp_page_summary(&page));
    Ok(Value::Object(response))
}

pub(super) fn browser_mcp_fill(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let text = string_flag(defined(args.get("text")))
        .ok_or_else(|| CliError::Other("ghostex_fill requires text".to_string()))?;
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    let selector = resolve_browser_element_selector(args, state, &page_id)?;
    let result = evaluate_function(
        state,
        &page_id,
        GHOSTEX_FILL_SCRIPT,
        json!([selector, text]),
    )?;
    let mut response = serde_json::Map::new();
    if let Some(result) = result {
        response.insert("filled".to_string(), result);
    }
    response.insert("page".to_string(), cdp_page_summary(&page));
    Ok(Value::Object(response))
}

pub(super) fn browser_mcp_press_key(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let key = string_flag(defined(args.get("key")))
        .ok_or_else(|| CliError::Other("ghostex_press_key requires key".to_string()))?;
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    let event = key_event_for_browser_mcp(&key);
    let mut key_down = event.as_object().cloned().unwrap_or_default();
    key_down.insert("type".to_string(), json!("keyDown"));
    client_call(
        state,
        &page_id,
        "Input.dispatchKeyEvent",
        Value::Object(key_down),
    )?;
    let mut key_up = event.as_object().cloned().unwrap_or_default();
    key_up.insert("type".to_string(), json!("keyUp"));
    client_call(
        state,
        &page_id,
        "Input.dispatchKeyEvent",
        Value::Object(key_up),
    )?;
    Ok(json!({
        "key": key,
        "page": cdp_page_summary(&page),
        "pressed": true,
    }))
}

pub(super) fn browser_mcp_screenshot(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    client_call(state, &page_id, "Page.enable", json!({}))?;
    let result = client_call(
        state,
        &page_id,
        "Page.captureScreenshot",
        json!({ "format": "png", "fromSurface": true }),
    )?;
    Ok(json!({
        "data": result.get("data").cloned().unwrap_or(Value::Null),
        "page": cdp_page_summary(&page),
        "size": { "encoding": "base64", "mimeType": "image/png" },
    }))
}
