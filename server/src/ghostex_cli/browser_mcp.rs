use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::ghostex_cli::args::{parse_args, FlagValue, Flags};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{CliError, CliResult};

/*
CDXC:Cli 2026-07-13:
Faithful port of the Node CLI's `browser` namespace: subcommand dispatch,
browser usage text, the openBrowserPane bridge payload, and the full stdio MCP
server (Content-Length framed JSON-RPC) that drives Ghostex's embedded CEF
pages over the Chrome DevTools Protocol. The page-side scripts (snapshot,
click, fill) are the verbatim JS function sources the Node CLI produced via
`fn.toString()`; they still execute inside the page's JS engine through
Runtime.evaluate, so they must not be "translated" to Rust.
*/

pub fn browser_command(args: &[String]) -> CliResult<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("help");
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    /*
     * CDXC:Browser 2026-05-27-01:59:
     * Agents should discover embedded CEF control through `gx browser --help`.
     * Keep browser MCP, skill install, pane opening, and browser visibility under
     * the `browser` namespace so "browser" is the durable keyword for this control
     * surface instead of a scattered set of top-level command names.
     */
    if rest.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{}", browser_usage());
        return Ok(());
    }
    match subcommand {
        "help" | "-h" | "--help" => {
            println!("{}", browser_usage());
            Ok(())
        }
        "mcp" | "devtools-mcp" | "browser-devtools-mcp" => browser_devtools_mcp_command(&rest),
        "install-skill" | "install-browser-skill" | "install-mcp-skill" => {
            crate::ghostex_cli::skills::install_browser_skill_command(&rest)
        }
        "open" | "open-pane" | "pane" => browser_open_bridge_action(&rest),
        other => Err(CliError::Other(format!(
            "Unknown browser command: {other}\n\n{}",
            browser_usage()
        ))),
    }
}

pub fn browser_devtools_mcp_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let port = normalize_positive_integer_opt(
        flag_json(&flags, "port").or_else(|| env_value("GHOSTEX_CEF_REMOTE_DEBUGGING_PORT")),
    );
    let target_value = flag_json(&flags, "target")
        .or_else(|| flag_json(&flags, "page"))
        .or_else(|| flag_json(&flags, "pageId"));
    let target = string_flag(target_value.as_ref());
    let timeout_ms = normalize_positive_integer_opt(
        flag_json(&flags, "timeout").or_else(|| env_value("GHOSTEX_BROWSER_MCP_TIMEOUT_MS")),
    )
    .unwrap_or(10_000);
    run_browser_devtools_mcp_server(McpServerOptions {
        port,
        target,
        timeout_ms,
    })
}

fn env_value(name: &str) -> Option<Value> {
    std::env::var(name).ok().map(Value::String)
}

fn flag_json(flags: &Flags, key: &str) -> Option<Value> {
    flags.0.get(key).map(FlagValue::as_json)
}

/// Private port of `bridgeAction("openBrowserPane", parseBrowserOpen)`.
/// actions::Parser has no BrowserOpen variant, so the browser subcommand owns
/// this payload shape locally and sends it through sendGxserverCliAction.
fn browser_open_bridge_action(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let mut flags = parsed.flags;
    let payload = parse_browser_open(&parsed.rest, &flags);
    // bridgeAction: payload.wait === true with no explicit --timeout disables
    // the bridge timeout. parseBrowserOpen never sets wait; kept for parity.
    if payload.get("wait") == Some(&Value::Bool(true)) && !flags.contains("timeout") {
        flags.insert_text("timeout", "0");
    }
    let result =
        crate::ghostex_cli::actions::send_gxserver_cli_action("openBrowserPane", &payload, &flags)?;
    print_json(&result);
    Ok(())
}

fn parse_browser_open(rest: &[String], flags: &Flags) -> Value {
    let mut payload = serde_json::Map::new();
    if let Some(value) = flag_json(flags, "groupId") {
        payload.insert("groupId".to_string(), value);
    }
    if let Some(value) = flag_json(flags, "projectId") {
        payload.insert("projectId".to_string(), value);
    }
    if let Some(value) = flag_json(flags, "projectName").or_else(|| flag_json(flags, "name")) {
        payload.insert("projectName".to_string(), value);
    }
    let project_path = flag_json(flags, "projectPath")
        .or_else(|| flag_json(flags, "path"))
        .or_else(|| {
            let active_project = flags
                .0
                .get("activeProject")
                .map(crate::ghostex_cli::args::parse_boolean)
                .unwrap_or(false);
            if active_project {
                None
            } else {
                Some(Value::String(
                    std::env::current_dir()
                        .map(|dir| dir.to_string_lossy().to_string())
                        .unwrap_or_default(),
                ))
            }
        });
    if let Some(value) = project_path {
        payload.insert("projectPath".to_string(), value);
    }
    let reuse = if flags.truthy("new") {
        Value::String("none".to_string())
    } else {
        flag_json(flags, "reuse").unwrap_or_else(|| Value::String("similar".to_string()))
    };
    payload.insert("reuse".to_string(), reuse);
    if let Some(value) =
        flag_json(flags, "url").or_else(|| rest.first().map(|value| Value::String(value.clone())))
    {
        payload.insert("url".to_string(), value);
    }
    Value::Object(payload)
}

fn format_help_command(signature: &str, description: &str) -> String {
    const COMMAND_COLUMN_WIDTH: usize = 58;
    let gap = " ".repeat(COMMAND_COLUMN_WIDTH.saturating_sub(signature.len()).max(2));
    format!("  {signature}{gap}{description}")
}

fn browser_usage() -> String {
    /*
     * CDXC:Browser 2026-05-27-01:59:
     * `gx browser --help` is the agent-facing entry point for embedded CEF
     * control. Document the MCP command, install command, tool names, and common
     * debugging workflow here so agents do not need to infer browser setup from
     * the general Ghostex CLI help.
     *
     * CDXC:Browser 2026-05-27-06:43:
     * Browser help must prevent agents from creating duplicate tabs and from
     * opening panes in whichever project is currently active. Document project
     * scoping flags, cwd-based defaults, reuse behavior, and page-id reuse so
     * agents keep working in their own worktree and reuse similar browser tabs.
     */
    let setup_commands = [
        format_help_command(
            "browser mcp [--port n] [--target id|--page id]",
            "Run the stdio MCP server for CEF DevTools control",
        ),
        format_help_command(
            "browser install-skill [--json]",
            "Install the $ghostex-embedded-browser-use skill with the external skills CLI",
        ),
        format_help_command(
            "browser open [url] [project/reuse flags]",
            "Open or reuse an embedded browser pane",
        ),
        format_help_command(
            "browser open-pane [url] [project/reuse flags]",
            "Alias for browser open",
        ),
    ]
    .join("\n");

    let mcp_tools = [
        format_help_command(
            "ghostex_list_pages",
            "List CEF DevTools targets and current page ids",
        ),
        format_help_command(
            "ghostex_select_page",
            "Choose the target page for later tool calls",
        ),
        format_help_command("ghostex_navigate", "Navigate the selected CEF page"),
        format_help_command(
            "ghostex_console_logs",
            "Read console messages, Log entries, and exceptions captured after attach",
        ),
        format_help_command(
            "ghostex_snapshot",
            "Get an accessibility-like DOM snapshot with @e element refs",
        ),
        format_help_command(
            "ghostex_click / ghostex_fill",
            "Interact with @e refs or CSS selectors",
        ),
        format_help_command(
            "ghostex_press_key",
            "Send Enter, Tab, Escape, arrows, or printable keys",
        ),
        format_help_command(
            "ghostex_evaluate",
            "Run JavaScript in the selected page for inspection",
        ),
        format_help_command(
            "ghostex_screenshot",
            "Capture a PNG screenshot as base64 MCP image content",
        ),
    ]
    .join("\n");

    format!(
        r#"Ghostex Embedded Browser Use - control embedded CEF panes from agents

Usage:
  gx browser --help
  gx browser mcp [--port n] [--target id|--page id] [--timeout ms]
  gx browser install-skill [--json]
  gx browser open [url] [--project-path path|--project-id id] [--reuse similar|exact|none]
  gx browser open-pane [url] [--project-path path|--project-id id] [--reuse similar|exact|none]
Agent MCP config:
  [mcp_servers.ghostex-browser]
  command = "ghostex"
  args = ["browser", "mcp"]

Commands:
{setup_commands}

Project scoping:
  browser open/open-pane default to the CLI process cwd as --project-path.
  Agents running in a worktree should keep that default, or pass --project-path "$PWD".
  Use --project-id when you already know the Ghostex project id from ghostex sessions --json.
  Use --group-id to place the browser in a specific project group.
  Use --active-project only for intentional manual control of the currently focused Ghostex project.

Tab reuse:
  browser open/open-pane default to --reuse similar, so an existing browser pane in the same project with the same origin is reused instead of creating a duplicate tab.
  Use --reuse exact when only the exact same URL should be reused.
  Use --reuse none or --new only when a separate browser pane is required.
  When a pane is reused for a different URL on the same origin, Ghostex focuses that pane and navigates it instead of creating another tab.
  After creating or selecting a page, keep the returned session id and the MCP page id from ghostex_list_pages; pass --target <pageId> to gx browser mcp or call ghostex_select_page before follow-up actions.

MCP tools exposed to the agent:
{mcp_tools}

Recommended agent workflow:
  1. Run ghostex_list_pages to find browser targets.
  2. Run ghostex_select_page when more than one page is open.
  3. Run ghostex_console_logs before reproducing a bug, then again after the action.
  4. Run ghostex_snapshot and use @e refs with ghostex_click or ghostex_fill.
  5. Use ghostex_screenshot for visual proof and ghostex_evaluate for focused inspection.

Connection details:
  The MCP server talks directly to Ghostex's embedded CEF Chrome DevTools Protocol endpoint.
  It scans the default Ghostex CEF ports automatically. Pass --port or set
  GHOSTEX_CEF_REMOTE_DEBUGGING_PORT only when the app is using a non-default port.

Legacy aliases:
  browser-devtools-mcp and browser-mcp still run the MCP server.
  install-browser-skill still installs the skill, but new docs should use browser install-skill.
"#
    )
}

// ---------------------------------------------------------------------------
// MCP stdio server
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct McpServerOptions {
    port: Option<u64>,
    target: Option<String>,
    timeout_ms: u64,
}

struct McpState {
    captures: HashMap<String, Vec<Value>>,
    clients: HashMap<String, CdpClient>,
    options: McpServerOptions,
    ref_maps: HashMap<String, HashMap<String, String>>,
    selected_page_id: Option<String>,
}

impl McpState {
    fn new(options: McpServerOptions) -> Self {
        let selected_page_id = options.target.clone();
        McpState {
            captures: HashMap::new(),
            clients: HashMap::new(),
            options,
            ref_maps: HashMap::new(),
            selected_page_id,
        }
    }
}

fn run_browser_devtools_mcp_server(options: McpServerOptions) -> CliResult<()> {
    let mut state = McpState::new(options);
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    loop {
        let read = match stdin.read(&mut chunk) {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        for message in extract_mcp_messages(&mut buffer) {
            if let Some(response) = handle_browser_mcp_message(&message, &mut state) {
                send_mcp_message(&response);
            }
        }
    }
    Ok(())
}

fn send_mcp_message(message: &Value) {
    let body = serde_json::to_string(message).unwrap_or_else(|_| "null".to_string());
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let _ = write!(stdout, "Content-Length: {}\r\n\r\n", body.as_bytes().len());
    let _ = stdout.write_all(body.as_bytes());
    let _ = stdout.flush();
}

/// Content-Length framed message extraction (exact McpStdioTransport.read
/// port): a header block without a Content-Length line drops the buffer.
fn extract_mcp_messages(buffer: &mut Vec<u8>) -> Vec<Value> {
    let mut messages = Vec::new();
    loop {
        let Some(header_end) = find_subsequence(buffer, b"\r\n\r\n") else {
            return messages;
        };
        let header = String::from_utf8_lossy(&buffer[..header_end]).to_string();
        let Some(length) = content_length_from_header(&header) else {
            buffer.clear();
            return messages;
        };
        let body_start = header_end + 4;
        if buffer.len() < body_start + length {
            return messages;
        }
        let body = String::from_utf8_lossy(&buffer[body_start..body_start + length]).to_string();
        buffer.drain(..body_start + length);
        if let Some(message) = serde_json::from_str::<Value>(&body).ok() {
            messages.push(message);
        }
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// `/^Content-Length:\s*(\d+)/im` over the header block.
fn content_length_from_header(header: &str) -> Option<usize> {
    for line in header.split("\r\n") {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            let rest_original = &line[line.len() - rest.len()..];
            let trimmed = rest_original.trim_start();
            let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                continue;
            }
            if let Ok(length) = digits.parse::<usize>() {
                return Some(length);
            }
        }
    }
    None
}

fn handle_browser_mcp_message(message: &Value, state: &mut McpState) -> Option<Value> {
    if !message.is_object() {
        return None;
    }
    let id = message.get("id").cloned();
    let method = js_string_of(message.get("method"));
    let method = method.as_str();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let id = match id {
        None | Some(Value::Null) => {
            return None;
        }
        Some(id) => id,
    };
    if method == "notifications/initialized" {
        return None;
    }
    let outcome: CliResult<Value> = match method {
        "initialize" => {
            let protocol_version = params
                .get("protocolVersion")
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or_else(|| Value::String("2024-11-05".to_string()));
            Ok(json!({
                "capabilities": { "tools": {} },
                "protocolVersion": protocol_version,
                "serverInfo": { "name": "ghostex-browser-devtools", "version": "1.0.0" },
            }))
        }
        "tools/list" => Ok(json!({ "tools": browser_mcp_tools() })),
        "tools/call" => {
            let name = params.get("name").cloned();
            let arguments = params
                .get("arguments")
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or_else(|| json!({}));
            call_browser_mcp_tool(name.as_ref(), &arguments, state)
        }
        _ => {
            return Some(json!({
                "error": { "code": -32601, "message": format!("Unknown MCP method: {method}") },
                "id": id,
                "jsonrpc": "2.0",
            }));
        }
    };
    match outcome {
        Ok(result) => Some(json!({ "id": id, "jsonrpc": "2.0", "result": result })),
        Err(error) => Some(json!({
            "error": { "code": -32000, "message": error.to_string() },
            "id": id,
            "jsonrpc": "2.0",
        })),
    }
}

fn browser_mcp_tools() -> Value {
    let page_selector_properties = json!({
        "pageId": { "description": "CDP target id. Defaults to the selected page, then the first Ghostex CEF page.", "type": "string" },
        "titleContains": { "description": "Select a page whose title contains this text.", "type": "string" },
        "urlContains": { "description": "Select a page whose URL contains this text.", "type": "string" },
    });
    let props = |extra: Value| -> Value {
        let mut merged = page_selector_properties
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(extra) = extra.as_object() {
            for (key, value) in extra {
                merged.insert(key.clone(), value.clone());
            }
        }
        Value::Object(merged)
    };
    json!([
        {
            "name": "ghostex_list_pages",
            "description": "List embedded Ghostex CEF pages available over the local DevTools endpoint.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "ghostex_select_page",
            "description": "Select the embedded page used by subsequent Ghostex browser tools.",
            "inputSchema": { "type": "object", "properties": props(json!({ "index": { "type": "number" } })) },
        },
        {
            "name": "ghostex_navigate",
            "description": "Navigate a Ghostex embedded browser page.",
            "inputSchema": { "type": "object", "required": ["url"], "properties": props(json!({ "url": { "type": "string" } })) },
        },
        {
            "name": "ghostex_evaluate",
            "description": "Evaluate JavaScript in the selected embedded browser page.",
            "inputSchema": {
                "type": "object",
                "required": ["script"],
                "properties": props(json!({ "awaitPromise": { "type": "boolean" }, "script": { "type": "string" } })),
            },
        },
        {
            "name": "ghostex_console_logs",
            "description": "Read captured console, exception, and browser log entries for the selected embedded page.",
            "inputSchema": {
                "type": "object",
                "properties": props(json!({ "clear": { "type": "boolean" }, "limit": { "type": "number" } })),
            },
        },
        {
            "name": "ghostex_snapshot",
            "description": "Return an agent-friendly snapshot of visible interactive elements and assign @e refs.",
            "inputSchema": { "type": "object", "properties": props(json!({ "limit": { "type": "number" } })) },
        },
        {
            "name": "ghostex_click",
            "description": "Click an element by @e ref from ghostex_snapshot or a CSS selector.",
            "inputSchema": {
                "type": "object",
                "properties": props(json!({ "ref": { "type": "string" }, "selector": { "type": "string" } })),
            },
        },
        {
            "name": "ghostex_fill",
            "description": "Fill an input, textarea, select, or contenteditable element by @e ref or CSS selector.",
            "inputSchema": {
                "type": "object",
                "required": ["text"],
                "properties": props(json!({ "ref": { "type": "string" }, "selector": { "type": "string" }, "text": { "type": "string" } })),
            },
        },
        {
            "name": "ghostex_press_key",
            "description": "Send a keyboard key to the selected embedded browser page.",
            "inputSchema": { "type": "object", "required": ["key"], "properties": props(json!({ "key": { "type": "string" } })) },
        },
        {
            "name": "ghostex_screenshot",
            "description": "Capture the selected embedded browser viewport as a PNG image.",
            "inputSchema": { "type": "object", "properties": props(json!({})) },
        },
    ])
}

fn call_browser_mcp_tool(
    name: Option<&Value>,
    args: &Value,
    state: &mut McpState,
) -> CliResult<Value> {
    let name = js_string_of(name);
    match name.as_str() {
        "ghostex_list_pages" => Ok(text_tool_result(&browser_mcp_list_pages(state)?)),
        "ghostex_select_page" => Ok(text_tool_result(&browser_mcp_select_page(args, state)?)),
        "ghostex_navigate" => Ok(text_tool_result(&browser_mcp_navigate(args, state)?)),
        "ghostex_evaluate" => Ok(text_tool_result(&browser_mcp_evaluate(args, state)?)),
        "ghostex_console_logs" => Ok(text_tool_result(&browser_mcp_console_logs(args, state)?)),
        "ghostex_snapshot" => Ok(text_tool_result(&browser_mcp_snapshot(args, state)?)),
        "ghostex_click" => Ok(text_tool_result(&browser_mcp_click(args, state)?)),
        "ghostex_fill" => Ok(text_tool_result(&browser_mcp_fill(args, state)?)),
        "ghostex_press_key" => Ok(text_tool_result(&browser_mcp_press_key(args, state)?)),
        "ghostex_screenshot" => {
            let value = browser_mcp_screenshot(args, state)?;
            Ok(image_tool_result(&value))
        }
        other => Err(CliError::Other(format!(
            "Unknown Ghostex browser MCP tool: {other}"
        ))),
    }
}

fn text_tool_result(value: &Value) -> Value {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string());
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn image_tool_result(value: &Value) -> Value {
    let summary = json!({
        "page": value.get("page").cloned().unwrap_or(Value::Null),
        "size": value.get("size").cloned().unwrap_or(Value::Null),
    });
    let text = serde_json::to_string_pretty(&summary).unwrap_or_else(|_| "null".to_string());
    json!({
        "content": [
            { "type": "text", "text": text },
            {
                "type": "image",
                "data": value.get("data").cloned().unwrap_or(Value::Null),
                "mimeType": "image/png",
            },
        ],
    })
}

fn browser_mcp_list_pages(state: &mut McpState) -> CliResult<Value> {
    let (pages, port) = discover_ghostex_cdp_pages(&state.options)?;
    let selected_page_id = state
        .selected_page_id
        .clone()
        .map(Value::String)
        .unwrap_or(Value::Null);
    let page_entries: Vec<Value> = pages
        .iter()
        .enumerate()
        .map(|(index, page)| {
            let mut entry = serde_json::Map::new();
            entry.insert("index".to_string(), json!(index));
            if let Some(id) = page.get("id") {
                entry.insert("id".to_string(), id.clone());
            }
            entry.insert("title".to_string(), defaulted(page, "title"));
            entry.insert("type".to_string(), defaulted(page, "type"));
            entry.insert("url".to_string(), defaulted(page, "url"));
            let selected = match (
                &state.selected_page_id,
                page.get("id").and_then(Value::as_str),
            ) {
                (Some(selected_id), Some(page_id)) => selected_id == page_id,
                _ => false,
            };
            entry.insert("selected".to_string(), Value::Bool(selected));
            Value::Object(entry)
        })
        .collect();
    Ok(json!({
        "port": port,
        "selectedPageId": selected_page_id,
        "pages": page_entries,
    }))
}

fn browser_mcp_select_page(args: &Value, state: &mut McpState) -> CliResult<Value> {
    let (page, _port) = resolve_ghostex_cdp_page(args, state)?;
    state.selected_page_id = page.get("id").and_then(Value::as_str).map(str::to_string);
    Ok(json!({ "selected": cdp_page_summary(&page) }))
}

fn browser_mcp_navigate(args: &Value, state: &mut McpState) -> CliResult<Value> {
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

fn browser_mcp_evaluate(args: &Value, state: &mut McpState) -> CliResult<Value> {
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

fn browser_mcp_console_logs(args: &Value, state: &mut McpState) -> CliResult<Value> {
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

fn browser_mcp_snapshot(args: &Value, state: &mut McpState) -> CliResult<Value> {
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

fn browser_mcp_click(args: &Value, state: &mut McpState) -> CliResult<Value> {
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

fn browser_mcp_fill(args: &Value, state: &mut McpState) -> CliResult<Value> {
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

fn browser_mcp_press_key(args: &Value, state: &mut McpState) -> CliResult<Value> {
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

fn browser_mcp_screenshot(args: &Value, state: &mut McpState) -> CliResult<Value> {
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

/// getGhostexCdpClient: resolve the page, remember it as selected, and make
/// sure a live CDP client exists for it. Returns the resolved page target.
fn get_ghostex_cdp_client(args: &Value, state: &mut McpState) -> CliResult<Value> {
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
        let client = CdpClient::connect(ws_url, state.options.timeout_ms)?;
        state.clients.insert(page_id, client);
    }
    Ok(page)
}

fn page_id_of(page: &Value) -> String {
    page.get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Perform one CDP call on the page's client, then record any CDP events that
/// arrived on the socket while waiting (the JS client did this via onEvent).
fn client_call(
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

fn ensure_capture_enabled(state: &mut McpState, page_id: &str) -> CliResult<()> {
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
fn pump_client_events(state: &mut McpState, page_id: &str) {
    let Some(client) = state.clients.get_mut(page_id) else {
        return;
    };
    client.pump_events();
    let events = std::mem::take(&mut client.events);
    for event in &events {
        record_ghostex_cdp_event(page_id, event, &mut state.captures);
    }
}

fn resolve_ghostex_cdp_page(args: &Value, state: &McpState) -> CliResult<(Value, u64)> {
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

fn discover_ghostex_cdp_pages(options: &McpServerOptions) -> CliResult<(Vec<Value>, u64)> {
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
            Ok(targets) => {
                let pages: Vec<Value> = targets
                    .as_array()
                    .map(|targets| {
                        targets
                            .iter()
                            .filter(|target| {
                                target.get("type").and_then(Value::as_str) == Some("page")
                                    && !js_string_of(Some(&defaulted(target, "url")))
                                        .starts_with("devtools://")
                            })
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                return Ok((pages, *port));
            }
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
    Err(CliError::Other(format!(
        "Could not reach Ghostex CEF DevTools on ports {ports_text}{suffix}"
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
fn evaluate_function(
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

fn resolve_browser_element_selector(
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

fn normalize_remote_object(value: Option<&Value>) -> Value {
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

fn remote_object_text(value: &Value) -> String {
    match normalize_remote_object(Some(value)) {
        Value::String(text) => text,
        normalized => serde_json::to_string(&normalized).unwrap_or_else(|_| "null".to_string()),
    }
}

fn key_event_for_browser_mcp(key: &str) -> Value {
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

fn cdp_page_summary(page: &Value) -> Value {
    let mut summary = serde_json::Map::new();
    if let Some(id) = page.get("id") {
        summary.insert("id".to_string(), id.clone());
    }
    summary.insert("title".to_string(), defaulted(page, "title"));
    summary.insert("url".to_string(), defaulted(page, "url"));
    Value::Object(summary)
}

/// `/^[a-z][a-z0-9+.-]*:/i`
fn normalize_browser_navigation_url(value: &str) -> String {
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

/// stringFlag: non-strings become String(value) (nullish -> null); strings are
/// trimmed and empty trims become null.
fn string_flag(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(other) => Some(js_string_of(Some(other))),
    }
}

/// normalizePositiveInteger: JS Number coercion, then Number.isInteger && > 0.
fn normalize_positive_integer_opt(value: Option<Value>) -> Option<u64> {
    let number = js_number_of_value(value.as_ref())?;
    if number.fract() == 0.0 && number > 0.0 && number <= u64::MAX as f64 {
        Some(number as u64)
    } else {
        None
    }
}

/// JS Number(value) for JSON values; None represents NaN.
fn js_number_of_value(value: Option<&Value>) -> Option<f64> {
    match value {
        None => None,
        Some(Value::Null) => Some(0.0),
        Some(Value::Bool(flag)) => Some(if *flag { 1.0 } else { 0.0 }),
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => crate::ghostex_cli::args::js_number(text),
        Some(_) => None,
    }
}

fn unique_numbers(values: &[Option<u64>]) -> Vec<u64> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for value in values.iter().flatten() {
        if *value > 0 && seen.insert(*value) {
            unique.push(*value);
        }
    }
    unique
}

fn defined(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !value.is_null())
}

fn defaulted(object: &Value, key: &str) -> Value {
    defined(object.get(key))
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()))
}

fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().map(|n| n != 0.0).unwrap_or(false),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// String(value) for the JSON values the MCP handler sees.
fn js_string_of(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| match value {
                Value::Null => String::new(),
                other => js_string_of(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".to_string(),
    }
}

fn http_json(url: &str, timeout_ms: u64) -> Result<Value, String> {
    match ureq::get(url)
        .timeout(Duration::from_millis(timeout_ms))
        .call()
    {
        Ok(response) => response
            .into_json::<Value>()
            .map_err(|error| error.to_string()),
        Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {code}")),
        Err(error) => Err(error.to_string()),
    }
}

// ---------------------------------------------------------------------------
// CDP client (GhostexCdpClient over tungstenite instead of SimpleWebSocket)
// ---------------------------------------------------------------------------

struct CdpClient {
    socket: tungstenite::WebSocket<TcpStream>,
    next_id: u64,
    capture_enabled: bool,
    closed: bool,
    timeout_ms: u64,
    /// CDP events (messages without an id) collected while waiting on calls;
    /// drained by the caller into per-page capture buffers.
    events: Vec<Value>,
}

impl CdpClient {
    fn connect(ws_url: &str, timeout_ms: u64) -> CliResult<CdpClient> {
        let (host, port) = parse_ws_host_port(ws_url)
            .ok_or_else(|| CliError::Other("Invalid WebSocket handshake".to_string()))?;
        let address = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port))
            .map_err(|error| CliError::Other(error.to_string()))?
            .next()
            .ok_or_else(|| CliError::Other("Invalid WebSocket handshake".to_string()))?;
        let stream =
            TcpStream::connect_timeout(&address, Duration::from_secs(5)).map_err(|error| {
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) {
                    CliError::Other("Timed out opening CDP WebSocket".to_string())
                } else {
                    CliError::Other(error.to_string())
                }
            })?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let (socket, _response) = tungstenite::client::client(ws_url, stream)
            .map_err(|error| CliError::Other(error.to_string()))?;
        Ok(CdpClient {
            socket,
            next_id: 1,
            capture_enabled: false,
            closed: false,
            timeout_ms,
            events: Vec::new(),
        })
    }

    fn call(&mut self, method: &str, params: Value) -> CliResult<Value> {
        if self.closed {
            return Err(CliError::Other("CDP connection is closed".to_string()));
        }
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({ "id": id, "method": method, "params": params }).to_string();
        if let Err(error) = self.socket.send(tungstenite::Message::Text(payload)) {
            self.closed = true;
            return Err(CliError::Other(error.to_string()));
        }
        let timeout_ms = if self.timeout_ms > 0 {
            self.timeout_ms
        } else {
            10_000
        };
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(CliError::Other(format!(
                    "Timed out waiting for CDP method {method}"
                )));
            }
            let remaining = deadline - now;
            let _ = self
                .socket
                .get_mut()
                .set_read_timeout(Some(remaining.max(Duration::from_millis(1))));
            match self.socket.read() {
                Ok(tungstenite::Message::Text(text)) => {
                    let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    match defined(parsed.get("id")).and_then(Value::as_u64) {
                        Some(message_id) if message_id == id => {
                            if let Some(error) =
                                parsed.get("error").filter(|error| js_truthy(error))
                            {
                                let message = error
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .map(str::to_string)
                                    .unwrap_or_else(|| {
                                        serde_json::to_string(error)
                                            .unwrap_or_else(|_| "null".to_string())
                                    });
                                return Err(CliError::Other(message));
                            }
                            return Ok(defined(parsed.get("result"))
                                .cloned()
                                .unwrap_or_else(|| json!({})));
                        }
                        Some(_) => {}
                        None => self.events.push(parsed),
                    }
                }
                Ok(tungstenite::Message::Close(_)) => {
                    self.closed = true;
                    return Err(CliError::Other("CDP connection closed".to_string()));
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    return Err(CliError::Other(format!(
                        "Timed out waiting for CDP method {method}"
                    )));
                }
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    self.closed = true;
                    return Err(CliError::Other("CDP connection closed".to_string()));
                }
                Err(error) => {
                    self.closed = true;
                    return Err(CliError::Other(error.to_string()));
                }
            }
        }
    }

    /// Drain already-buffered CDP events without blocking.
    fn pump_events(&mut self) {
        if self.closed {
            return;
        }
        if self.socket.get_mut().set_nonblocking(true).is_err() {
            return;
        }
        loop {
            match self.socket.read() {
                Ok(tungstenite::Message::Text(text)) => {
                    if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                        if defined(parsed.get("id")).is_none() {
                            self.events.push(parsed);
                        }
                    }
                }
                Ok(tungstenite::Message::Close(_)) => {
                    self.closed = true;
                    break;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(_) => {
                    self.closed = true;
                    break;
                }
            }
        }
        let _ = self.socket.get_mut().set_nonblocking(false);
    }
}

/// Parse host/port from a `ws://host:port/path` DevTools URL (JS used
/// `new URL(...)` with a port-80 default).
fn parse_ws_host_port(ws_url: &str) -> Option<(String, u16)> {
    let rest = ws_url.strip_prefix("ws://")?;
    let host_port = rest.split(['/', '?']).next().unwrap_or(rest);
    match host_port.split_once(':') {
        Some((host, port)) => Some((host.to_string(), port.parse::<u16>().ok()?)),
        None => Some((host_port.to_string(), 80)),
    }
}

// ---------------------------------------------------------------------------
// Page-side scripts. These are the VERBATIM sources of the Node CLI's
// ghostexSnapshotScript / ghostexClickScript / ghostexFillScript functions
// (which it serialized with fn.toString()); they run in the page's JS engine
// via Runtime.evaluate as `(<source>)(...<jsonArgs>)`.
// ---------------------------------------------------------------------------

const GHOSTEX_SNAPSHOT_SCRIPT: &str = r##"function ghostexSnapshotScript(limit) {
  const selectors = [
    "a[href]",
    "button",
    "input",
    "textarea",
    "select",
    "[role='button']",
    "[role='link']",
    "[role='textbox']",
    "[contenteditable='true']",
    "[tabindex]:not([tabindex='-1'])",
  ].join(",");
  const cssPath = (element) => {
    if (!element || element.nodeType !== 1) return "";
    const parts = [];
    let cursor = element;
    while (cursor && cursor.nodeType === 1 && cursor !== document.documentElement) {
      let part = cursor.nodeName.toLowerCase();
      if (cursor.id) {
        part += `#${CSS.escape(cursor.id)}`;
        parts.unshift(part);
        break;
      }
      const parent = cursor.parentElement;
      if (parent) {
        const siblings = Array.from(parent.children).filter((child) => child.nodeName === cursor.nodeName);
        if (siblings.length > 1) {
          part += `:nth-of-type(${siblings.indexOf(cursor) + 1})`;
        }
      }
      parts.unshift(part);
      cursor = parent;
    }
    return parts.join(" > ");
  };
  const labelFor = (element) => {
    const aria = element.getAttribute("aria-label");
    if (aria) return aria.trim();
    if (element.id) {
      const label = document.querySelector(`label[for="${CSS.escape(element.id)}"]`);
      if (label?.innerText) return label.innerText.trim();
    }
    return (element.innerText || element.value || element.placeholder || element.title || "").trim().replace(/\s+/g, " ");
  };
  const elements = [];
  for (const element of Array.from(document.querySelectorAll(selectors))) {
    const rect = element.getBoundingClientRect();
    const style = window.getComputedStyle(element);
    if (rect.width <= 0 || rect.height <= 0 || style.visibility === "hidden" || style.display === "none") {
      continue;
    }
    elements.push({
      bounds: { height: Math.round(rect.height), width: Math.round(rect.width), x: Math.round(rect.x), y: Math.round(rect.y) },
      disabled: Boolean(element.disabled || element.getAttribute("aria-disabled") === "true"),
      label: labelFor(element).slice(0, 240),
      placeholder: element.getAttribute("placeholder") || "",
      ref: `@e${elements.length + 1}`,
      role: element.getAttribute("role") || element.nodeName.toLowerCase(),
      selector: cssPath(element),
      tag: element.nodeName.toLowerCase(),
      type: element.getAttribute("type") || "",
      value: "value" in element ? String(element.value ?? "").slice(0, 240) : "",
    });
    if (elements.length >= limit) break;
  }
  return { elements, title: document.title, url: location.href };
}"##;

const GHOSTEX_CLICK_SCRIPT: &str = r##"function ghostexClickScript(selector) {
  const element = document.querySelector(selector);
  if (!element) throw new Error(`Element not found: ${selector}`);
  element.scrollIntoView({ block: "center", inline: "center" });
  element.focus?.();
  element.click();
  const rect = element.getBoundingClientRect();
  return { bounds: { height: rect.height, width: rect.width, x: rect.x, y: rect.y }, selector };
}"##;

const GHOSTEX_FILL_SCRIPT: &str = r##"function ghostexFillScript(selector, text) {
  const element = document.querySelector(selector);
  if (!element) throw new Error(`Element not found: ${selector}`);
  element.scrollIntoView({ block: "center", inline: "center" });
  element.focus?.();
  if (element.isContentEditable) {
    element.textContent = text;
  } else if (element.tagName === "SELECT") {
    element.value = text;
  } else if ("value" in element) {
    element.value = text;
  } else {
    throw new Error(`Element cannot be filled: ${selector}`);
  }
  element.dispatchEvent(new Event("input", { bubbles: true }));
  element.dispatchEvent(new Event("change", { bubbles: true }));
  return { selector, value: text };
}"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> McpState {
        McpState::new(McpServerOptions {
            port: None,
            target: None,
            timeout_ms: 10_000,
        })
    }

    #[test]
    fn key_event_maps_special_keys() {
        assert_eq!(
            key_event_for_browser_mcp("Enter"),
            json!({ "code": "Enter", "key": "Enter", "windowsVirtualKeyCode": 13 })
        );
        assert_eq!(
            key_event_for_browser_mcp("ArrowLeft"),
            json!({ "code": "ArrowLeft", "key": "ArrowLeft", "windowsVirtualKeyCode": 37 })
        );
        assert_eq!(
            key_event_for_browser_mcp("Escape"),
            json!({ "code": "Escape", "key": "Escape", "windowsVirtualKeyCode": 27 })
        );
    }

    #[test]
    fn key_event_maps_printable_and_multi_char_keys() {
        assert_eq!(
            key_event_for_browser_mcp("a"),
            json!({ "code": "KeyA", "key": "a", "text": "a", "windowsVirtualKeyCode": 65 })
        );
        assert_eq!(
            key_event_for_browser_mcp("Z"),
            json!({ "code": "KeyZ", "key": "Z", "text": "Z", "windowsVirtualKeyCode": 90 })
        );
        // Multi-character non-special keys keep the key as code, empty text, vk 0.
        assert_eq!(
            key_event_for_browser_mcp("F5"),
            json!({ "code": "F5", "key": "F5", "text": "", "windowsVirtualKeyCode": 0 })
        );
    }

    #[test]
    fn tools_list_shape_matches_node_cli() {
        let tools = browser_mcp_tools();
        let tools = tools.as_array().expect("tools array");
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool.get("name").and_then(Value::as_str).unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "ghostex_list_pages",
                "ghostex_select_page",
                "ghostex_navigate",
                "ghostex_evaluate",
                "ghostex_console_logs",
                "ghostex_snapshot",
                "ghostex_click",
                "ghostex_fill",
                "ghostex_press_key",
                "ghostex_screenshot",
            ]
        );
        for tool in tools {
            assert!(tool.get("description").and_then(Value::as_str).is_some());
            assert_eq!(
                tool.pointer("/inputSchema/type").and_then(Value::as_str),
                Some("object")
            );
        }
        // list_pages has no page-selector properties; every other tool does.
        assert_eq!(
            tools[0].pointer("/inputSchema/properties"),
            Some(&json!({}))
        );
        for tool in &tools[1..] {
            assert!(tool
                .pointer("/inputSchema/properties/pageId/description")
                .is_some());
        }
        assert_eq!(
            tools[2].pointer("/inputSchema/required"),
            Some(&json!(["url"]))
        );
        assert_eq!(
            tools[3].pointer("/inputSchema/required"),
            Some(&json!(["script"]))
        );
        assert_eq!(
            tools[7].pointer("/inputSchema/required"),
            Some(&json!(["text"]))
        );
        assert_eq!(
            tools[8].pointer("/inputSchema/required"),
            Some(&json!(["key"]))
        );
    }

    #[test]
    fn handle_initialize_and_tools_list() {
        let mut state = test_state();
        let response = handle_browser_mcp_message(
            &json!({ "id": 1, "jsonrpc": "2.0", "method": "initialize", "params": {} }),
            &mut state,
        )
        .expect("initialize response");
        assert_eq!(response.get("id"), Some(&json!(1)));
        assert_eq!(response.get("jsonrpc"), Some(&json!("2.0")));
        assert_eq!(
            response.pointer("/result/protocolVersion"),
            Some(&json!("2024-11-05"))
        );
        assert_eq!(
            response.pointer("/result/serverInfo/name"),
            Some(&json!("ghostex-browser-devtools"))
        );
        assert_eq!(
            response.pointer("/result/serverInfo/version"),
            Some(&json!("1.0.0"))
        );
        assert_eq!(
            response.pointer("/result/capabilities/tools"),
            Some(&json!({}))
        );

        let echoed = handle_browser_mcp_message(
            &json!({ "id": 2, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }),
            &mut state,
        )
        .expect("initialize response");
        assert_eq!(
            echoed.pointer("/result/protocolVersion"),
            Some(&json!("2025-03-26"))
        );

        let listed =
            handle_browser_mcp_message(&json!({ "id": 3, "method": "tools/list" }), &mut state)
                .expect("tools/list response");
        assert_eq!(listed.pointer("/result/tools"), Some(&browser_mcp_tools()));
    }

    #[test]
    fn handle_ignores_notifications_and_id_less_messages() {
        let mut state = test_state();
        assert!(handle_browser_mcp_message(
            &json!({ "method": "notifications/initialized" }),
            &mut state
        )
        .is_none());
        assert!(handle_browser_mcp_message(
            &json!({ "id": 9, "method": "notifications/initialized" }),
            &mut state
        )
        .is_none());
        assert!(
            handle_browser_mcp_message(&json!({ "method": "tools/list" }), &mut state).is_none()
        );
        assert!(handle_browser_mcp_message(
            &json!({ "id": null, "method": "tools/list" }),
            &mut state
        )
        .is_none());
        assert!(handle_browser_mcp_message(&json!("not an object"), &mut state).is_none());
    }

    #[test]
    fn handle_unknown_method_and_unknown_tool() {
        let mut state = test_state();
        let response =
            handle_browser_mcp_message(&json!({ "id": 4, "method": "resources/list" }), &mut state)
                .expect("error response");
        assert_eq!(response.pointer("/error/code"), Some(&json!(-32601)));
        assert_eq!(
            response.pointer("/error/message"),
            Some(&json!("Unknown MCP method: resources/list"))
        );

        let response = handle_browser_mcp_message(
            &json!({ "id": 5, "method": "tools/call", "params": { "name": "nope" } }),
            &mut state,
        )
        .expect("error response");
        assert_eq!(response.pointer("/error/code"), Some(&json!(-32000)));
        assert_eq!(
            response.pointer("/error/message"),
            Some(&json!("Unknown Ghostex browser MCP tool: nope"))
        );

        let response = handle_browser_mcp_message(
            &json!({ "id": 6, "method": "tools/call", "params": {} }),
            &mut state,
        )
        .expect("error response");
        assert_eq!(
            response.pointer("/error/message"),
            Some(&json!("Unknown Ghostex browser MCP tool: undefined"))
        );
    }

    #[test]
    fn extract_mcp_messages_parses_content_length_frames() {
        let body = r#"{"id":1,"method":"initialize"}"#;
        let mut buffer = format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes();
        let messages = extract_mcp_messages(&mut buffer);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].get("id"), Some(&json!(1)));
        assert!(buffer.is_empty());

        // Split frame arrives across chunks.
        let mut buffer =
            format!("Content-Length: {}\r\n\r\n{}", body.len(), &body[..10]).into_bytes();
        assert!(extract_mcp_messages(&mut buffer).is_empty());
        buffer.extend_from_slice(body[10..].as_bytes());
        assert_eq!(extract_mcp_messages(&mut buffer).len(), 1);

        // Case-insensitive header, extra headers, and two frames back to back.
        let mut buffer = format!(
            "X-Other: 1\r\ncontent-length: {}\r\n\r\n{}CONTENT-LENGTH: {}\r\n\r\n{}",
            body.len(),
            body,
            body.len(),
            body
        )
        .into_bytes();
        assert_eq!(extract_mcp_messages(&mut buffer).len(), 2);

        // Header block without Content-Length drops the buffer.
        let mut buffer = b"X-Other: 1\r\n\r\nleftover".to_vec();
        assert!(extract_mcp_messages(&mut buffer).is_empty());
        assert!(buffer.is_empty());
    }

    #[test]
    fn normalize_browser_navigation_url_adds_https_when_missing_scheme() {
        assert_eq!(
            normalize_browser_navigation_url("https://x.test/a"),
            "https://x.test/a"
        );
        assert_eq!(
            normalize_browser_navigation_url("chrome-extension://abc"),
            "chrome-extension://abc"
        );
        assert_eq!(
            normalize_browser_navigation_url("about:blank"),
            "about:blank"
        );
        assert_eq!(
            normalize_browser_navigation_url("example.com/path"),
            "https://example.com/path"
        );
        assert_eq!(
            normalize_browser_navigation_url("127.0.0.1:8080"),
            "https://127.0.0.1:8080"
        );
    }

    #[test]
    fn string_flag_and_normalize_positive_integer_match_js() {
        assert_eq!(string_flag(Some(&json!("  x  "))), Some("x".to_string()));
        assert_eq!(string_flag(Some(&json!("   "))), None);
        assert_eq!(string_flag(Some(&json!(true))), Some("true".to_string()));
        assert_eq!(string_flag(Some(&json!(7))), Some("7".to_string()));
        assert_eq!(string_flag(Some(&Value::Null)), None);
        assert_eq!(string_flag(None), None);

        assert_eq!(
            normalize_positive_integer_opt(Some(json!(9333))),
            Some(9333)
        );
        assert_eq!(
            normalize_positive_integer_opt(Some(json!("9334"))),
            Some(9334)
        );
        assert_eq!(normalize_positive_integer_opt(Some(json!(1.5))), None);
        assert_eq!(normalize_positive_integer_opt(Some(json!(0))), None);
        assert_eq!(normalize_positive_integer_opt(Some(json!(-2))), None);
        assert_eq!(normalize_positive_integer_opt(Some(json!("abc"))), None);
        assert_eq!(normalize_positive_integer_opt(Some(Value::Null)), None);
        assert_eq!(normalize_positive_integer_opt(None), None);
    }

    #[test]
    fn normalize_remote_object_prefers_value_then_unserializable_then_description() {
        assert_eq!(
            normalize_remote_object(Some(&json!({ "value": 5, "description": "5" }))),
            json!(5)
        );
        assert_eq!(
            normalize_remote_object(Some(&json!({ "value": null }))),
            Value::Null
        );
        assert_eq!(
            normalize_remote_object(Some(&json!({ "unserializableValue": "Infinity" }))),
            json!("Infinity")
        );
        assert_eq!(
            normalize_remote_object(Some(&json!({ "description": "Object", "type": "object" }))),
            json!("Object")
        );
        assert_eq!(
            normalize_remote_object(Some(&json!({ "type": "undefined" }))),
            json!("undefined")
        );
        assert_eq!(normalize_remote_object(Some(&Value::Null)), Value::Null);
        assert_eq!(normalize_remote_object(None), Value::Null);
        assert_eq!(
            remote_object_text(&json!({ "value": { "a": 1 } })),
            "{\"a\":1}"
        );
        assert_eq!(remote_object_text(&json!({ "value": "hi" })), "hi");
    }

    #[test]
    fn unique_numbers_dedupes_and_keeps_order() {
        assert_eq!(
            unique_numbers(&[None, Some(9334), Some(9333), Some(9334), Some(9335)]),
            vec![9334, 9333, 9335]
        );
    }

    #[test]
    fn parse_browser_open_defaults_match_js() {
        let parsed = parse_args(&[
            "--url".to_string(),
            "example.com".to_string(),
            "--new".to_string(),
            "--active-project".to_string(),
        ]);
        let payload = parse_browser_open(&parsed.rest, &parsed.flags);
        assert_eq!(payload.get("url"), Some(&json!("example.com")));
        assert_eq!(payload.get("reuse"), Some(&json!("none")));
        // --active-project boolean true suppresses the cwd projectPath default.
        assert_eq!(payload.get("projectPath"), None);

        let parsed = parse_args(&["http://x.test".to_string()]);
        let payload = parse_browser_open(&parsed.rest, &parsed.flags);
        assert_eq!(payload.get("url"), Some(&json!("http://x.test")));
        assert_eq!(payload.get("reuse"), Some(&json!("similar")));
        assert!(payload.get("projectPath").and_then(Value::as_str).is_some());
    }

    #[test]
    fn parse_ws_host_port_handles_devtools_urls() {
        assert_eq!(
            parse_ws_host_port("ws://127.0.0.1:9333/devtools/page/AB12"),
            Some(("127.0.0.1".to_string(), 9333))
        );
        assert_eq!(
            parse_ws_host_port("ws://localhost/devtools/page/AB12"),
            Some(("localhost".to_string(), 80))
        );
        assert_eq!(parse_ws_host_port("http://x.test/"), None);
    }
}
