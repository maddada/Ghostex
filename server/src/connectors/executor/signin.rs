//! Signing the Executor CLI in and out: Ghostex runs the CLI's own `executor login`, which prints a
//! link and a one-time code and waits for the browser, and `executor logout`. The CLI keeps the
//! sign-in where it always does (the system credential store, else a private file in the user's
//! home); Ghostex never reads or stores it.
//!
//! CDXC:Connectors 2026-11-27 DECISION:
//! User (Executor CLI page, PLACEHOLDER-1): one CLI sign-in for the whole computer, shared by every workspace and every harness on it, as the Executor CLI does on its own. Ghostex shows `executor login`'s code in the sign-in panel the gh and gws rows use and opens its page in this computer's browser, which also works when gxserver runs on another computer.

use std::path::PathBuf;

use serde_json::{json, Value};

use super::super::cli::{https_link, login_waiting, read_link, run_login, stop_login};
use super::cli;
use super::status;

fn login_key() -> PathBuf {
    PathBuf::from("<executor login>")
}

/// Whether `executor login` is waiting for the browser.
pub(super) fn signing_in() -> bool {
    login_waiting(&login_key())
}

/// `executor login`'s link (`To sign in, open <url>`) and code (`and enter the code <code>`, or
/// `and check that it shows the code <code>` when the link already carries it).
fn login_link(line: &str, url: &mut Option<String>, code: &mut Option<String>) -> bool {
    if line.starts_with("To sign in, open") {
        if let Some(link) = https_link(line).or_else(|| {
            line.split_whitespace()
                .last()
                .filter(|word| word.starts_with("http://"))
                .map(str::to_string)
        }) {
            *url = Some(link);
        }
    }
    if line.contains("the code") {
        *code = line.split_whitespace().last().map(str::to_string);
    }
    url.is_some() && code.is_some()
}

/// A self-hosted Executor's address: `https://…`, or `http://` on this computer.
fn checked_host(host: &str) -> Result<String, String> {
    let host = host.trim().trim_end_matches('/');
    let local = ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|prefix| host.starts_with(prefix));
    let valid = (host.starts_with("https://") || local)
        && !host.contains(char::is_whitespace)
        && host
            .split("://")
            .nth(1)
            .is_some_and(|rest| !rest.is_empty());
    if valid {
        Ok(host.to_string())
    } else {
        Err("Give the self-hosted Executor's address, like https://executor.example.com.".into())
    }
}

/// Starts `executor login` (to `host`, else hosted Executor) and returns its page and code.
pub(crate) fn sign_in(host: Option<&str>) -> Result<Value, String> {
    let found = cli::find()
        .filter(|found| found.usable())
        .ok_or("Install the Executor CLI first, then sign in.")?;
    let mut command = cli::command(&found.executable);
    command.arg("login");
    if let Some(host) = host.map(str::trim).filter(|host| !host.is_empty()) {
        command.args(["--host", &checked_host(host)?]);
    }
    // The CLI opens its page itself unless it thinks it runs over SSH; Ghostex opens it in the
    // browser of the computer the user is at (gxserver may run on another one), so only once.
    command.env("SSH_TTY", "ghostex");
    // `executor login` that stops at once says why on an `Error: ...` line (a self-hosted
    // Executor without device sign-in, say); that reason replaces the generic one.
    let refused = std::cell::RefCell::new(None::<String>);
    let started = run_login(
        "The Executor CLI",
        command,
        login_key(),
        |lines| {
            read_link(lines, |line, url, code| {
                if let Some(reason) = line.strip_prefix("Error:") {
                    *refused.borrow_mut() = Some(reason.trim().to_string());
                }
                login_link(line, url, code)
            })
        },
        |ended| {
            if ended.is_some() {
                super::relay::stop();
                status::refresh_now();
            }
        },
    )
    .map_err(|error| match refused.take() {
        Some(reason) => format!("The Executor CLI couldn't sign in: {reason}"),
        None => error,
    })?;
    status::notify_changed();
    Ok(json!({ "url": started.url, "code": started.code }))
}

/// Stops a waiting `executor login`.
pub(crate) fn cancel_sign_in() {
    stop_login(&login_key());
    status::notify_changed();
}

/// `executor logout`: the CLI forgets its sign-in, for Ghostex and every other harness.
pub(crate) fn sign_out() -> Result<(), String> {
    stop_login(&login_key());
    let found = cli::find()
        .filter(|found| found.usable())
        .ok_or("The Executor CLI isn't installed on this computer.")?;
    super::relay::stop();
    let mut command = cli::command(&found.executable);
    command.arg("logout");
    let output =
        crate::managed_tools::run::output_within(command, std::time::Duration::from_secs(20))
            .map_err(|_| "executor logout didn't finish.".to_string())?;
    status::refresh_now();
    let said = format!("{}\n{}", output.stdout, output.stderr);
    // Signed out already is what Sign out asked for.
    if output.status.success() || said.contains("NotSignedIn") {
        return Ok(());
    }
    Err(said
        .lines()
        .find_map(|line| line.strip_prefix("Error:"))
        .map(|reason| format!("executor logout failed: {}", reason.trim()))
        .unwrap_or_else(|| "executor logout failed. Run it in a terminal to see why.".to_string()))
}
