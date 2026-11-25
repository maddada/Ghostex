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

/// CDXC:Browser 2026-11-26 DECISION:
/// User (Q4999, #762): `ghostex_click` clicks like a person: the mouse is pressed and released
/// through DevTools at the element's centre, so the page gets a trusted click (`isTrusted`), which
/// some sites need before they react. When that point does not reach the element (another element
/// covers it, or it is outside the view) it falls back to the script click it always used. A file
/// upload field (or its label) is refused: a person's click there opens the file chooser on the
/// user's screen, and `ghostex_set_input_files` fills it without one. A page that is not on screen
/// opens the popups such a click starts beside it, never in front (browser_pane.rs).
pub(super) fn browser_mcp_click(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    let selector = resolve_browser_element_selector(args, state, &page_id)?;
    let point = evaluate_function(
        state,
        &page_id,
        GHOSTEX_CLICK_POINT_SCRIPT,
        json!([selector]),
    )?
    .unwrap_or(Value::Null);
    if point.get("fileUpload") == Some(&Value::Bool(true)) {
        return Err(CliError::Other(format!(
            "{selector} is a file upload field. Use ghostex_set_input_files with absolute paths: clicking it would open a file chooser on the user's screen."
        )));
    }
    let reachable = point.get("reachable") == Some(&Value::Bool(true));
    let mut clicked = if reachable && press_mouse_at(state, &page_id, &point)? {
        json!({ "method": "mouse" })
    } else {
        let mut clicked =
            evaluate_function(state, &page_id, GHOSTEX_CLICK_SCRIPT, json!([selector]))?
                .unwrap_or_else(|| json!({}));
        clicked["method"] = json!("script");
        clicked["reason"] = json!(if point.get("inView") == Some(&Value::Bool(false)) {
            "The element has no size or is outside the view"
        } else if reachable {
            "The page did not take the mouse"
        } else {
            "Another element covers the element's centre"
        });
        clicked
    };
    if clicked.get("bounds").is_none() {
        clicked["bounds"] = point.get("bounds").cloned().unwrap_or(Value::Null);
    }
    clicked["selector"] = json!(selector);
    Ok(json!({ "clicked": clicked, "page": cdp_page_summary(&page) }))
}

/// Moves the mouse to `point` and presses and releases the left button there. False when the page
/// would not take the first move, before anything was pressed, so the caller's script click
/// cannot click twice.
fn press_mouse_at(state: &mut McpState, page_id: &str, point: &Value) -> CliResult<bool> {
    let (Some(x), Some(y)) = (
        point.get("x").and_then(Value::as_f64),
        point.get("y").and_then(Value::as_f64),
    ) else {
        return Ok(false);
    };
    let event = |kind: &str, button: &str, buttons: u64, click_count: u64| {
        json!({
            "type": kind,
            "x": x,
            "y": y,
            "button": button,
            "buttons": buttons,
            "clickCount": click_count,
            "pointerType": "mouse",
        })
    };
    if client_call(
        state,
        page_id,
        "Input.dispatchMouseEvent",
        event("mouseMoved", "none", 0, 0),
    )
    .is_err()
    {
        return Ok(false);
    }
    client_call(
        state,
        page_id,
        "Input.dispatchMouseEvent",
        event("mousePressed", "left", 1, 1),
    )?;
    client_call(
        state,
        page_id,
        "Input.dispatchMouseEvent",
        event("mouseReleased", "left", 0, 1),
    )?;
    Ok(true)
}

/// The most files one `ghostex_set_input_files` call may put in a field.
const MAX_UPLOAD_FILES: usize = 20;

/// CDXC:Browser 2026-11-26 DECISION:
/// User (Q4999, #762): `ghostex_set_input_files` puts local files in a page's file upload field
/// (`DOM.setFileInputFiles`), the one thing the other tools could not do. It is kept narrow: only an
/// `input type=file` that is enabled (and takes several files when given several), only absolute
/// paths of existing regular files (symlinks resolved, at most `MAX_UPLOAD_FILES`), and never a
/// file inside a folder that holds credentials (`upload_refused_folder`). It sends the files to the
/// page's site exactly as a person choosing them would; an agent that can run commands could send
/// them the long way already. The browser tools only ever talk to this computer's gxserver, so the
/// paths checked here are the ones the Browser reads.
pub(super) fn browser_mcp_set_input_files(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let files = upload_paths(defined(args.get("files")).or_else(|| defined(args.get("paths"))))?;
    let page = get_ghostex_cdp_client(args, state)?;
    let page_id = page_id_of(&page);
    let selector = resolve_browser_element_selector(args, state, &page_id)?;
    let call_args = serde_json::to_string(&json!([selector, files.len()]))
        .map_err(|error| CliError::Other(error.to_string()))?;
    let found = client_call(
        state,
        &page_id,
        "Runtime.evaluate",
        json!({
            "expression": format!("({GHOSTEX_FILE_INPUT_SCRIPT})(...{call_args})"),
            "returnByValue": false,
        }),
    )?;
    if let Some(details) = defined(found.get("exceptionDetails")) {
        let message = details
            .get("exception")
            .and_then(|exception| exception.get("description"))
            .and_then(Value::as_str)
            .and_then(|description| description.lines().next())
            .or_else(|| details.get("text").and_then(Value::as_str))
            .unwrap_or("The file upload field could not be found");
        return Err(CliError::Other(
            message.trim_start_matches("Error: ").to_string(),
        ));
    }
    let object_id = found
        .get("result")
        .and_then(|remote| remote.get("objectId"))
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::Other("The file upload field could not be found".to_string()))?
        .to_string();
    let set = client_call(
        state,
        &page_id,
        "DOM.setFileInputFiles",
        json!({ "files": files, "objectId": object_id }),
    );
    let _ = client_call(
        state,
        &page_id,
        "Runtime.releaseObject",
        json!({ "objectId": object_id }),
    );
    set?;
    Ok(json!({
        "files": files,
        "page": cdp_page_summary(&page),
        "selector": selector,
        "set": true,
    }))
}

/// The paths `ghostex_set_input_files` was given, each resolved to the file it names, or why one
/// may not be used.
fn upload_paths(value: Option<&Value>) -> CliResult<Vec<String>> {
    let usage = || {
        CliError::Other(
            "ghostex_set_input_files requires files: a list of absolute file paths".to_string(),
        )
    };
    let given: Vec<String> = match value {
        Some(Value::String(path)) => vec![path.clone()],
        Some(Value::Array(paths)) => paths
            .iter()
            .map(|path| path.as_str().map(str::to_string))
            .collect::<Option<_>>()
            .ok_or_else(usage)?,
        _ => return Err(usage()),
    };
    if given.is_empty() {
        return Err(usage());
    }
    if given.len() > MAX_UPLOAD_FILES {
        return Err(CliError::Other(format!(
            "ghostex_set_input_files takes at most {MAX_UPLOAD_FILES} files at a time"
        )));
    }
    given
        .iter()
        .map(|path| {
            let path = std::path::Path::new(path.trim());
            if !path.is_absolute() {
                return Err(CliError::Other(format!(
                    "{} is not an absolute path; give the file's full path",
                    path.display()
                )));
            }
            let resolved = std::fs::canonicalize(path).map_err(|_| {
                CliError::Other(format!("{} does not exist", path.display()))
            })?;
            if !resolved.is_file() {
                return Err(CliError::Other(format!(
                    "{} is not a file",
                    path.display()
                )));
            }
            if let Some(folder) = upload_refused_folder(&resolved) {
                return Err(CliError::Other(format!(
                    "{} is inside {}, which holds credentials; the browser tools never upload from there",
                    path.display(),
                    folder.display()
                )));
            }
            Ok(resolved.to_string_lossy().to_string())
        })
        .collect()
}

/// The credentials folder or file `resolved` is in, if any: SSH, GPG, cloud and container
/// logins, the macOS keychains, saved Git and netrc passwords, and Ghostex's own state (its
/// gxserver token among it).
fn upload_refused_folder(resolved: &std::path::Path) -> Option<std::path::PathBuf> {
    let canonical = |path: std::path::PathBuf| std::fs::canonicalize(&path).unwrap_or(path);
    let mut refused = vec![canonical(crate::ghostex_cli::rpc::ghostex_home())];
    let home = canonical(crate::ghostex_cli::rpc::home_dir());
    if !home.as_os_str().is_empty() {
        for entry in [
            ".ssh",
            ".gnupg",
            ".aws",
            ".azure",
            ".kube",
            ".docker",
            ".config/gcloud",
            ".config/gh",
            ".password-store",
            ".netrc",
            ".git-credentials",
            "Library/Keychains",
        ] {
            refused.push(home.join(entry));
        }
    }
    refused
        .into_iter()
        .find(|folder| resolved.starts_with(folder))
}

/// CDXC:Browser 2026-11-26 DECISION:
/// User (Q4999, #762): the browser tools' way to open a page, always in the background: it opens or
/// reuses a tab of this agent's own project without showing it, focusing it or asking (gxserver
/// pins it to the agent's project, switch_requests.rs), and the tools drive that page next.
pub(super) fn browser_mcp_open_page(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let url = string_flag(defined(args.get("url")))
        .ok_or_else(|| CliError::Other("ghostex_open_page requires url".to_string()))?;
    let reuse = match string_flag(defined(args.get("reuse"))).as_deref() {
        None => "similar".to_string(),
        Some(reuse @ ("similar" | "exact" | "none")) => reuse.to_string(),
        Some(other) => {
            return Err(CliError::Other(format!(
                "ghostex_open_page reuse must be similar, exact or none, not {other}"
            )))
        }
    };
    let scope = state.options.scope.clone();
    let mut payload = serde_json::Map::new();
    payload.insert("url".to_string(), json!(url));
    payload.insert("reuse".to_string(), json!(reuse));
    payload.insert("background".to_string(), json!(true));
    if let Some(session) = &scope.agent_session {
        payload.insert("agentSessionId".to_string(), json!(session));
    }
    match (
        &scope.project_id,
        &scope.project_path,
        &scope.session_project_id,
    ) {
        (Some(project_id), _, _) | (None, None, Some(project_id)) => {
            payload.insert("projectId".to_string(), json!(project_id));
        }
        (None, Some(project_path), _) => {
            payload.insert("projectPath".to_string(), json!(project_path));
        }
        (None, None, None) => {}
    }
    let payload = Value::Object(payload);
    let flags = crate::ghostex_cli::args::Flags::default();
    let send = |payload: &Value| {
        crate::ghostex_cli::actions::send_gxserver_cli_action("openBrowserPane", payload, &flags)
    };
    let opened = match send(&payload) {
        Err(error) if error.to_string().contains("No matching project") => {
            match browser_open_in_session_project(
                &payload,
                false,
                scope.session_project_id.as_deref(),
            ) {
                Some(retry) => send(&retry)?,
                None => return Err(error),
            }
        }
        other => other?,
    };
    if opened.get("ok") == Some(&Value::Bool(false)) {
        let reason = opened
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("The page could not be opened");
        return Err(CliError::Other(reason.to_string()));
    }
    // The page just opened is the one the tools drive next (agent_pages.rs).
    state.selected_page_id = None;
    if let Some(session) = &scope.agent_session {
        remember_agent_page(session, None);
    }
    Ok(json!({
        "opened": opened,
        "note": "The page opened in the background, in a tab of this agent's project that the user is not shown. The other tools now drive it; ghostex_list_pages gives its pageId.",
    }))
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
