//! `ghostex connectors executor`: a workspace's Executor (crate::connectors): turning it on
//! (downloading Executor the first time), its apps from Executor's catalog or an MCP server URL,
//! their sign-ins and their tools.

use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::args::Flags;
use super::connectors::slow;
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};

/// How long `enable` waits for the download and the first start.
const ENABLE_WAIT: Duration = Duration::from_secs(15 * 60);

fn call(path: &str, params: &Map<String, Value>, flags: &Flags) -> CliResult<Value> {
    rpc::call_gxserver_rpc(path, &Value::Object(params.clone()), &slow(flags))
}

fn executor_status(params: &Map<String, Value>, flags: &Flags) -> CliResult<Value> {
    let result =
        rpc::call_gxserver_rpc("/api/readConnectors", &Value::Object(params.clone()), flags)?;
    Ok(result
        .pointer("/workspaces/0/executor")
        .cloned()
        .unwrap_or(Value::Null))
}

/// `ghostex connectors executor [status] | enable | disable | catalog [words] | add <catalog-app>
/// | add-mcp <name> <url> | connect <app> [--no-open] | remove <app> | tools [app] | call <app>
/// <tool> [--args <json>]`.
pub(super) fn executor_command(
    rest: &[String],
    mut params: Map<String, Value>,
    flags: &Flags,
    open_in_browser: impl Fn(&str) -> CliResult<()>,
) -> CliResult<()> {
    let subcommand = rest.first().map(String::as_str).unwrap_or("status");
    let args = rest.get(1..).unwrap_or_default();
    let open = !flags.contains("no-open");
    match subcommand {
        "status" => {
            print_json(&executor_status(&params, flags)?);
            Ok(())
        }
        "enable" | "on" => {
            call("/api/enableExecutor", &params, flags)?;
            wait_until_running(&params, flags)
        }
        "disable" | "off" => {
            call("/api/disableExecutor", &params, flags)?;
            println!("Executor is off in this workspace. Its apps and sign-ins are kept for when you turn it on again.");
            Ok(())
        }
        "catalog" | "search" => {
            params.insert("query".to_string(), json!(args.join(" ")));
            let result = call("/api/searchExecutorCatalog", &params, flags)?;
            let apps = result.get("apps").and_then(Value::as_array).cloned().unwrap_or_default();
            if apps.is_empty() {
                println!("No catalog app matches.");
            }
            for app in apps {
                let text = |key: &str| app.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
                let sign_in = app
                    .get("signIn")
                    .and_then(Value::as_array)
                    .map(|kinds| kinds.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
                    .filter(|kinds| !kinds.is_empty())
                    .unwrap_or_else(|| "no sign-in".to_string());
                println!("{}  {}  ({sign_in})\n    {}", text("id"), text("name"), text("description"));
            }
            Ok(())
        }
        "add" => {
            let app = args.join(" ");
            if app.trim().is_empty() {
                return Err(CliError::Other(
                    "Name the catalog app: ghostex connectors executor add <app>. Find it with ghostex connectors executor catalog <words>.".to_string(),
                ));
            }
            params.insert("app".to_string(), json!(app.trim()));
            let result = call("/api/addExecutorApp", &params, flags)?;
            print_json(&result.get("app").cloned().unwrap_or(Value::Null));
            Ok(())
        }
        "add-mcp" => {
            let (Some(name), Some(url)) = (args.first(), args.get(1)) else {
                return Err(CliError::Other(
                    "Give a name and the server's address: ghostex connectors executor add-mcp <name> <https://…>.".to_string(),
                ));
            };
            params.insert("name".to_string(), json!(name));
            params.insert("url".to_string(), json!(url));
            let result = call("/api/addExecutorMcpServer", &params, flags)?;
            print_json(&result.get("app").cloned().unwrap_or(Value::Null));
            Ok(())
        }
        "connect" => {
            params.insert("app".to_string(), json!(args.join(" ")));
            let result = call("/api/connectExecutorApp", &params, flags)?;
            let name = result.get("name").and_then(Value::as_str).unwrap_or("The app");
            match result.get("url").and_then(Value::as_str) {
                Some(url) => {
                    println!("Sign in to {name} in your browser:\n{url}");
                    if open {
                        open_in_browser(url)?;
                    }
                }
                None => println!("{name} needs no sign-in, or is already signed in."),
            }
            Ok(())
        }
        "remove" => {
            params.insert("app".to_string(), json!(args.join(" ")));
            let result = call("/api/removeExecutorApp", &params, flags)?;
            println!(
                "Removed {}.",
                result.get("removed").and_then(Value::as_str).unwrap_or("the app")
            );
            Ok(())
        }
        "tools" => {
            if !args.is_empty() {
                params.insert("app".to_string(), json!(args.join(" ")));
            }
            let result = call("/api/listExecutorTools", &params, flags)?;
            print_json(&result.get("apps").cloned().unwrap_or(Value::Null));
            Ok(())
        }
        "call" => {
            let (Some(app), Some(tool)) = (args.first(), args.get(1)) else {
                return Err(CliError::Other(
                    "Name the app and the tool: ghostex connectors executor call <app> <tool> [--args '<json>'].".to_string(),
                ));
            };
            let input = match flags.string_value("args") {
                Some(text) => serde_json::from_str::<Value>(text)
                    .map_err(|error| CliError::Other(format!("--args is not valid JSON: {error}")))?,
                None => json!({}),
            };
            params.insert("app".to_string(), json!(app));
            params.insert("tool".to_string(), json!(tool));
            params.insert("input".to_string(), input);
            let result = call("/api/callExecutorTool", &params, flags)?;
            println!("{}", result.get("output").and_then(Value::as_str).unwrap_or_default());
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown executor command \"{other}\". Use status, enable, disable, catalog, add, add-mcp, connect, remove, tools or call."
        ))),
    }
}

/// Follows the download and the first start, printing the download's progress.
fn wait_until_running(params: &Map<String, Value>, flags: &Flags) -> CliResult<()> {
    let deadline = Instant::now() + ENABLE_WAIT;
    let mut printed = 0usize;
    // A download finished earlier in this gxserver's life is not shown again.
    let mut following = false;
    loop {
        let status = executor_status(params, flags)?;
        let install_active = matches!(
            status.pointer("/install/status").and_then(Value::as_str),
            Some("queued" | "running")
        );
        following |= install_active;
        if let Some(output) = status
            .pointer("/install/output")
            .and_then(Value::as_str)
            .filter(|_| following)
        {
            if output.len() > printed {
                print!("{}", &output[printed..]);
                printed = output.len();
            }
        }
        if status.get("running").and_then(Value::as_bool) == Some(true) {
            println!("Executor is running in this workspace.");
            return Ok(());
        }
        if let Some(error) = status
            .pointer("/install/error")
            .and_then(Value::as_str)
            .filter(|_| following || status.get("installed") == Some(&Value::Bool(false)))
        {
            return Err(CliError::Other(format!(
                "Executor couldn't be installed: {error}"
            )));
        }
        let starting = status.get("starting").and_then(Value::as_bool) == Some(true);
        if let Some(error) = status.get("error").and_then(Value::as_str) {
            if !starting {
                return Err(CliError::Other(format!("Executor couldn't start: {error}")));
            }
        }
        if Instant::now() >= deadline {
            return Err(CliError::Other(
                "Executor is still getting ready. Check with ghostex connectors executor status."
                    .to_string(),
            ));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
