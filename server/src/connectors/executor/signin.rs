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

/// A self-hosted Executor's address as its origin: `https://<host>`, or `http://` with this
/// computer's own address (`localhost`, `127.0.0.1`, `[::1]`, as Executor allows), read as a URL
/// and compared exactly, with no user name, path, query or fragment.
fn checked_host(host: &str) -> Result<String, String> {
    use std::net::{Ipv4Addr, Ipv6Addr};
    use url::Host;
    let refused = || {
        "Give the self-hosted Executor's address, like https://executor.example.com.".to_string()
    };
    let url = url::Url::parse(host.trim()).map_err(|_| refused())?;
    let this_computer = match url.host() {
        Some(Host::Domain(domain)) => domain == "localhost",
        Some(Host::Ipv4(ip)) => ip == Ipv4Addr::LOCALHOST,
        Some(Host::Ipv6(ip)) => ip == Ipv6Addr::LOCALHOST,
        None => false,
    };
    let scheme_ok = url.scheme() == "https" || (url.scheme() == "http" && this_computer);
    let bare = url.username().is_empty()
        && url.password().is_none()
        && matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none();
    if !(scheme_ok && bare && url.has_host()) {
        return Err(refused());
    }
    Ok(url.origin().ascii_serialization())
}

/// Starts `executor login` (to `host`, else hosted Executor) and returns its page and code.
pub(crate) fn sign_in(host: Option<&str>) -> Result<Value, String> {
    let found = cli::find()
        .filter(|found| found.usable())
        .ok_or("Install the Executor CLI first, then sign in.")?;
    let host = match host.map(str::trim).filter(|host| !host.is_empty()) {
        Some(host) => Some(checked_host(host)?),
        None => None,
    };
    let mut command = cli::command(&found.executable, host.as_deref());
    command.arg("login");
    if let Some(host) = &host {
        command.args(["--host", host]);
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
    let mut command = cli::command(&found.executable, None);
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

#[cfg(test)]
mod tests {
    use super::checked_host;

    #[test]
    fn self_hosted_addresses_are_compared_exactly() {
        assert_eq!(
            checked_host("https://executor.example.com/").unwrap(),
            "https://executor.example.com"
        );
        assert_eq!(
            checked_host(" http://localhost:4000 ").unwrap(),
            "http://localhost:4000"
        );
        assert_eq!(
            checked_host("http://127.0.0.1:8787").unwrap(),
            "http://127.0.0.1:8787"
        );
        assert_eq!(
            checked_host("http://[::1]:8787").unwrap(),
            "http://[::1]:8787"
        );
        for refused in [
            "http://localhost.evil.com",
            "http://127.0.0.1.evil.com",
            "http://localhost@evil.com",
            "http://evil.com",
            "https://user@executor.example.com",
            "https://executor.example.com/path",
            "https://executor.example.com/?q=1",
            "ftp://executor.example.com",
            "executor.example.com",
            "https://",
        ] {
            assert!(checked_host(refused).is_err(), "{refused}");
        }
    }
}
