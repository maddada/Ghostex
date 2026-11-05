use serde_json::Value;

use crate::ghostex_cli::args::{parse_args, FlagValue, Flags};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{CliError, CliResult};

use super::*;

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
    // A portable copy's DevTools port is closed unless debugging mode is on, so it is no longer an
    // explicit port here: pages are reached through gxserver like any install's (review 2026-11-05).
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
    let text_flag = |key: &str| {
        string_flag(flag_json(&flags, key).as_ref()).filter(|value| !value.trim().is_empty())
    };
    let scope = AgentScope::from_env(
        text_flag("projectId"),
        text_flag("projectPath").or_else(|| text_flag("path")),
    );
    run_browser_devtools_mcp_server(McpServerOptions {
        port,
        target,
        timeout_ms,
        scope,
    })
}

pub(super) fn env_value(name: &str) -> Option<Value> {
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
    let mut payload = parse_browser_open(&parsed.rest, &flags);
    // The app remembers which agent opened the page, and the browser tools drive it next instead
    // of a page an earlier bridge remembered (agent_pages.rs).
    let agent_session = browser_agent_session_from_env();
    if let (Some(session), Some(object)) = (&agent_session, payload.as_object_mut()) {
        object.insert("agentSessionId".to_string(), Value::String(session.clone()));
    }
    // bridgeAction: payload.wait === true with no explicit --timeout disables
    // the bridge timeout. parseBrowserOpen never sets wait; kept for parity.
    if payload.get("wait") == Some(&Value::Bool(true)) && !flags.contains("timeout") {
        flags.insert_text("timeout", "0");
    }
    let named_folder = flags.contains("projectPath") || flags.contains("path");
    let result = match crate::ghostex_cli::actions::send_gxserver_cli_action(
        "openBrowserPane",
        &payload,
        &flags,
    ) {
        Err(error) if error.to_string().contains("No matching project") => {
            let session_project =
                AgentScope::from_env(None, Some(String::new())).session_project_id;
            match browser_open_in_session_project(
                &payload,
                named_folder,
                session_project.as_deref(),
            ) {
                Some(retry) => crate::ghostex_cli::actions::send_gxserver_cli_action(
                    "openBrowserPane",
                    &retry,
                    &flags,
                )?,
                None => return Err(error),
            }
        }
        other => other?,
    };
    if let Some(session) = &agent_session {
        remember_agent_page(session, None);
    }
    print_json(&result);
    Ok(())
}

/// CDXC:Projects 2026-10-31 WHY:
/// `ghostex browser open` names the agent's folder by default, and that folder is not always a project (a session's own worktree, a subfolder): the open then failed with "No matching project" while the agent's session plainly belongs to one. Such an open goes to the session's own project, as the browser tools already choose (agent_pages.rs). A folder the caller named with `--project-path`, and every other selector, keeps its error.
/// The retried payload: the session's project in place of the default folder, or `None` when the
/// open named its project some other way or the agent is in no session.
pub(super) fn browser_open_in_session_project(
    payload: &Value,
    named_folder: bool,
    session_project: Option<&str>,
) -> Option<Value> {
    let session_project = session_project?;
    let object = payload.as_object()?;
    let named_otherwise = ["projectId", "groupId", "projectName"]
        .iter()
        .any(|key| object.contains_key(*key));
    if named_folder || named_otherwise || !object.contains_key("projectPath") {
        return None;
    }
    let mut retry = object.clone();
    retry.remove("projectPath");
    retry.insert(
        "projectId".to_string(),
        Value::String(session_project.to_string()),
    );
    Some(Value::Object(retry))
}

pub(super) fn parse_browser_open(rest: &[String], flags: &Flags) -> Value {
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
  gx browser mcp [--port n] [--target id|--page id] [--project-path path|--project-id id] [--timeout ms]
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

Which page the tools drive:
  Without a pageId the tools drive this agent's own page: the one it selected or used last (remembered across gx browser mcp runs), else the one it last opened with gx browser open, else the page in front of its project's Browser.
  They only ever pick from the agent's project (the folder it runs in, like browser open, else its session's project; --project-path or --project-id choose another). ghostex_list_pages lists that project's pages.
  Switching tabs in Ghostex by hand does not move the agent to another page. Pass a pageId to drive any other page; with no page of its own the tools say so instead of using someone else's tab.

MCP tools exposed to the agent:
{mcp_tools}

Recommended agent workflow:
  1. Run ghostex_list_pages to find browser targets.
  2. Run ghostex_select_page when more than one page is open.
  3. Run ghostex_console_logs before reproducing a bug, then again after the action.
  4. Run ghostex_snapshot and use @e refs with ghostex_click or ghostex_fill.
  5. Use ghostex_screenshot for visual proof and ghostex_evaluate for focused inspection.

Connection details:
  The MCP server reaches Ghostex's embedded CEF pages through the local gxserver, with its token.
  It scans the CEF DevTools ports only for an older Ghostex whose gxserver lacks that route.
  Pass --port, or set GHOSTEX_CEF_REMOTE_DEBUGGING_PORT, to use a DevTools port the app was started with.

Legacy aliases:
  browser-devtools-mcp and browser-mcp still run the MCP server.
  install-browser-skill still installs the skill, but new docs should use browser install-skill.
"#
    )
}
