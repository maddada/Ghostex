//! `ghostex connectors executor`: the computer's Executor CLI (crate::connectors): its state,
//! installing it with Executor's own installer, signing it in (`executor login`) or out, and
//! removing what the old built-in Executor left. Apps and their accounts are managed in Executor.

use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::args::Flags;
use super::connectors::slow;
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};

/// How long `install` follows the installer and `sign-in` waits for the browser.
const WAIT_AT_MOST: Duration = Duration::from_secs(15 * 60);

fn call(path: &str, params: &Map<String, Value>, flags: &Flags) -> CliResult<Value> {
    rpc::call_gxserver_rpc(path, &Value::Object(params.clone()), &slow(flags))
}

fn executor_status(flags: &Flags) -> CliResult<Value> {
    let result = rpc::call_gxserver_rpc("/api/readConnectors", &json!({}), flags)?;
    Ok(result.get("executor").cloned().unwrap_or(Value::Null))
}

/// `ghostex connectors executor [status] | install | sign-in [--host <url>] [--no-open] |
/// sign-out | remove-old`.
pub(super) fn executor_command(
    rest: &[String],
    params: Map<String, Value>,
    flags: &Flags,
    open_in_browser: impl Fn(&str) -> CliResult<()>,
) -> CliResult<()> {
    let subcommand = rest.first().map(String::as_str).unwrap_or("status");
    match subcommand {
        "status" => {
            print_json(&executor_status(flags)?);
            Ok(())
        }
        "install" => {
            call("/api/installExecutorCli", &params, flags)?;
            wait_for_install(flags)
        }
        "sign-in" | "login" => {
            let mut params = params;
            if let Some(host) = flags.text("host") {
                params.insert("host".to_string(), json!(host));
            }
            let started = call("/api/signInExecutorCli", &params, flags)?;
            let url = started.get("url").and_then(Value::as_str).unwrap_or_default();
            let code = started.get("code").and_then(Value::as_str).unwrap_or_default();
            println!("Open {url}\nand check it shows the code {code}, then approve.");
            if !flags.contains("no-open") {
                open_in_browser(url)?;
            }
            wait_for_sign_in(flags)
        }
        "sign-out" | "logout" => {
            call("/api/signOutExecutorCli", &params, flags)?;
            println!("The Executor CLI is signed out on this computer.");
            Ok(())
        }
        "remove-old" => {
            call("/api/removeOldExecutor", &params, flags)?;
            println!("Removed what the old built-in Executor left on this computer.");
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown executor command \"{other}\". Use status, install, sign-in, sign-out or remove-old."
        ))),
    }
}

/// Follows the installer, printing its output.
fn wait_for_install(flags: &Flags) -> CliResult<()> {
    let deadline = Instant::now() + WAIT_AT_MOST;
    let mut printed = 0usize;
    loop {
        let status = executor_status(flags)?;
        if let Some(output) = status.pointer("/install/output").and_then(Value::as_str) {
            if output.len() > printed {
                print!("{}", &output[printed..]);
                printed = output.len();
            }
        }
        match status.pointer("/install/status").and_then(Value::as_str) {
            Some("queued" | "running") => {}
            _ if status.get("installed") == Some(&Value::Bool(true)) => {
                println!("The Executor CLI is installed. Sign it in with: ghostex connectors executor sign-in");
                return Ok(());
            }
            _ => {
                let error = status
                    .pointer("/install/error")
                    .and_then(Value::as_str)
                    .unwrap_or("it isn't found afterwards");
                return Err(CliError::Other(format!(
                    "The Executor CLI couldn't be installed: {error}"
                )));
            }
        }
        if Instant::now() >= deadline {
            return Err(CliError::Other(
                "The installer is still running. Check with ghostex connectors executor status."
                    .to_string(),
            ));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Waits until the browser approval finished and the CLI reports its sign-in.
fn wait_for_sign_in(flags: &Flags) -> CliResult<()> {
    let deadline = Instant::now() + WAIT_AT_MOST;
    loop {
        std::thread::sleep(Duration::from_secs(2));
        let status = executor_status(flags)?;
        if status.get("signedIn") == Some(&Value::Bool(true)) {
            match status.get("error").and_then(Value::as_str) {
                Some(error) => println!("Signed in, but: {error}"),
                None => println!("The Executor CLI is signed in."),
            }
            return Ok(());
        }
        if status.get("signingIn") != Some(&Value::Bool(true)) {
            return Err(CliError::Other(
                "The sign-in ended without signing in. Try again.".to_string(),
            ));
        }
        if Instant::now() >= deadline {
            return Err(CliError::Other(
                "Nobody approved the sign-in in time.".to_string(),
            ));
        }
    }
}
