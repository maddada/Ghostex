//! `ghostex account-login codex`: the Settings > Accounts sign-in for a Codex account, run through
//! Codex Swap (xswap) with its result written to a file, and its "already saved" refusal repaired
//! without a second browser sign-in.
//!
//! CDXC:AgentProviders 2026-11-09 WHY:
//! A Windows user (Discord #windows, Ghostex 11.2.1) could not add a second Codex account: the
//! browser said "Successfully logged in", then Settings said "Login did not complete" and listed
//! nothing, every time, until `xswap purge` wiped Codex Swap's registry. Codex Swap refuses to add
//! a login whose owner is already in its registry ("this account is already registered"), and an
//! owner stays there after Ghostex forgets it: Remove account in Settings leaves the Codex Swap
//! slot, and on Windows ConPTY can drop xswap's last output so a saved login was never read back
//! (the same loss Claude logins had in #347). Settings hides a slot it has not added unless the
//! slot is signed in, so nothing on screen explained it, and the terminal never showed xswap's
//! reason. The result now goes to the file gxserver names (`--result-file`), and the login itself is
//! kept: xswap starts `ghostex` as its Codex (`XSWAP_CODEX_BIN`), which runs the real Codex and keeps
//! a copy of the new `auth.json` (see [`shim`]). When xswap refuses an owner it already has, the
//! slot holding it is used (refreshed with the new login) or, when that slot's login is broken,
//! removed on its own, and the save runs once more from the kept copy, with no browser.
//! SEE-ALSO: `reported_outcome` in server/src/accounts/setup.rs reads the result file.
use super::account_login::LoginFailure;
use crate::accounts::{codex_identity, helpers, model::Provider};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) const HELP: &str = "Usage: ghostex account-login codex [--account NUMBER] [--email EMAIL] [--device-auth] [--json] [--result-file PATH]\n\nSign in to a Codex account and save it with Codex Swap (xswap).\n--device-auth signs in with a one-time code on another device instead of a browser on this computer.\nWithout --account, saves the account you choose in the browser; --email requires that account.\nWith --account, signs in again to that saved slot.\n--result-file writes the saved account, or the reason the login failed, to PATH as JSON.";

/// The folder the shim keeps the new login in; set only on the xswap that the wrapper starts.
const SHIM_KEEP: &str = "GHOSTEX_CODEX_LOGIN_KEEP";
/// The real Codex the shim runs.
const SHIM_CODEX: &str = "GHOSTEX_CODEX_LOGIN_CODEX";
/// Set to `1` when the shim should hand xswap the kept login instead of opening the browser.
const SHIM_REPLAY: &str = "GHOSTEX_CODEX_LOGIN_REPLAY";
const KEPT_LOGIN: &str = "auth.json";
/// codex-swap's refusal for an owner its registry already has (`ensure_unique_identity`).
const ALREADY_SAVED: &str = "already registered";

/// Runs as xswap's Codex when the wrapper started it. `None` for every other `ghostex` call.
///
/// xswap runs `<codex> -c … login` with `CODEX_HOME` set to its own staging folder and reads
/// `auth.json` from there once Codex exits. The shim runs the real Codex with the same arguments and
/// keeps a copy of that `auth.json`; on replay it writes the kept copy there instead, so xswap saves
/// the same login again without a browser.
pub(crate) fn shim() -> Option<i32> {
    let keep = PathBuf::from(std::env::var_os(SHIM_KEEP)?);
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if !args.iter().any(|arg| arg == "login") {
        return None;
    }
    let home = PathBuf::from(std::env::var_os("CODEX_HOME")?);
    let kept = keep.join(KEPT_LOGIN);
    if std::env::var_os(SHIM_REPLAY).is_some_and(|value| value == "1") {
        return Some(match std::fs::copy(&kept, home.join(KEPT_LOGIN)) {
            Ok(_) => {
                eprintln!("Saving the sign-in from a moment ago.");
                0
            }
            Err(error) => {
                eprintln!("The sign-in from a moment ago is gone: {error}");
                1
            }
        });
    }
    let codex = std::env::var_os(SHIM_CODEX)?;
    let mut command = Command::new(codex);
    command
        .args(&args)
        .env_remove(SHIM_KEEP)
        .env_remove(SHIM_CODEX)
        .env_remove(SHIM_REPLAY);
    let status = match command.status() {
        Ok(status) => status,
        Err(error) => {
            eprintln!("Could not start Codex: {error}");
            return Some(1);
        }
    };
    if status.success() {
        // Best effort: without the copy the login still saves; only the repair has nothing to replay.
        let _ = std::fs::copy(home.join(KEPT_LOGIN), &kept);
    }
    Some(status.code().unwrap_or(1))
}

/// The saved account as `{"schemaVersion":1,"account":{"number","email"}}`, or `None` for `--help`.
pub(super) fn login(args: &[String]) -> Result<Option<Value>, LoginFailure> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("{HELP}");
        return Ok(None);
    }
    let value_after = |flag: &str| -> Result<Option<String>, LoginFailure> {
        match args.iter().position(|arg| arg == flag) {
            None => Ok(None),
            Some(index) => args
                .get(index + 1)
                .filter(|value| !value.starts_with("--"))
                .map(|value| Some(value.trim().to_string()))
                .ok_or_else(|| LoginFailure::plain(format!("{flag} requires a value"))),
        }
    };
    let account = value_after("--account")?;
    let email = value_after("--email")?;
    let device_auth = args.iter().any(|arg| arg == "--device-auth");
    if args.first().map(String::as_str) != Some("codex")
        || account
            .as_deref()
            .is_some_and(|slot| !slot.parse::<u32>().is_ok_and(|number| number > 0))
        || email
            .as_deref()
            .is_some_and(|email| !email.contains('@') || email.chars().any(char::is_whitespace))
    {
        return Err(LoginFailure::plain(HELP));
    }
    // Windows has no HOME; USERPROFILE is the home the account helpers themselves resolve.
    let home = ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|path| path.is_absolute())
        .ok_or_else(|| LoginFailure::plain("The home directory is unavailable."))?;
    let xswap = helpers::executable(&home, "xswap")
        .ok_or_else(|| LoginFailure::plain("Install Codex Swap in Settings > Accounts first."))?;
    let keep = tempfile::Builder::new()
        .prefix("ghostex-codex-login-")
        .tempdir()
        .map_err(|error| LoginFailure::plain(format!("Could not prepare the sign-in: {error}")))?;
    // Only `add` runs through the shim: it never checks for running Codex. A `login` that replaces
    // the main Codex home's login does, by the configured Codex's name, which the shim makes
    // `ghostex` (and so this very process).
    let shim = if account.is_none() {
        shim_environment(&home, keep.path())
    } else {
        Vec::new()
    };
    let save_args: Vec<String> = match (&account, &email) {
        (Some(slot), _) => vec!["login".into(), slot.clone()],
        (None, Some(email)) => [
            "add",
            "--login",
            "--share-history",
            "--email",
            email,
            "--json",
        ]
        .map(String::from)
        .to_vec(),
        (None, None) => [
            "add",
            "--login",
            "--share-history",
            "--any-account",
            "--json",
        ]
        .map(String::from)
        .to_vec(),
    };
    // The sign-in was asked for from another device (accounts/setup.rs, `fromAnotherDevice`).
    let save_args = if device_auth {
        [save_args, vec!["--device-auth".into()]].concat()
    } else {
        save_args
    };
    let first = run_xswap(&xswap, &save_args, &shim, false);
    if first.ok {
        return saved(&first, account.as_deref()).map(Some);
    }
    let reason = xswap_reason(&first);
    let kept = keep.path().join(KEPT_LOGIN);
    if account.is_none() && reason.contains(ALREADY_SAVED) && kept.is_file() {
        return repair_already_saved(&home, &xswap, &save_args, &shim, &kept, &reason).map(Some);
    }
    Err(LoginFailure::from_xswap(&reason))
}

/// xswap refused the new login because its registry already has that owner. The slot holding it
/// is this login's account: a slot whose own login works is refreshed with the new login and kept;
/// a slot whose login is missing, unreadable or another account's is the stale entry, so only that
/// slot is removed (its folder stays on disk, as `xswap remove` leaves it) and the save runs once
/// more from the kept login. Every other saved account is left as it is.
fn repair_already_saved(
    home: &Path,
    xswap: &Path,
    save_args: &[String],
    shim: &[(&'static str, OsString)],
    kept: &Path,
    reason: &str,
) -> Result<Value, LoginFailure> {
    let unrepaired = || {
        LoginFailure {
        message: "This Codex account is already saved in Codex Swap, and Ghostex couldn't repair that saved copy. Copy details has what Codex Swap said.".into(),
        detail: reason.to_string(),
    }
    };
    let identity = std::fs::read(kept)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .and_then(|auth| codex_identity::codex_auth_identity(&auth))
        .ok_or_else(unrepaired)?;
    let roster = helpers::json_command(home, "xswap", &["list", "--json"]).map_err(|error| {
        LoginFailure {
            message: unrepaired().message,
            detail: format!("{reason}\n{error}"),
        }
    })?;
    let holders: Vec<&Value> = roster["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| {
            let account_id = row["accountId"].as_str().unwrap_or_default();
            !account_id.is_empty()
                && codex_identity::identity_matches(
                    Provider::Codex,
                    &codex_identity::codex_identity(account_id, row["userId"].as_str()),
                    &identity,
                )
        })
        .collect();
    let [slot] = holders.as_slice() else {
        return Err(unrepaired());
    };
    let number = slot["number"].as_u64().ok_or_else(unrepaired)?.to_string();
    if slot["loginStatus"] == "present" {
        println!("This account is already saved in Codex Swap slot {number}.");
        // A slot in its own folder takes the new login beside running sessions (codex-swap's
        // `login` skips its Codex check there); one whose login is the main Codex home's is kept
        // as it is, since that check would count this `ghostex` as Codex.
        if slot["managed"] == true && slot["home"] == slot["savedHome"] {
            println!("Saving the new sign-in there.");
            if !run_xswap(xswap, &["login".into(), number.clone()], shim, true).ok {
                // The slot's own login works, so the account is usable without the newer copy.
                println!("Kept the login already in slot {number}.");
            }
        }
        return Ok(report(&number, slot["email"].as_str()));
    }
    println!(
        "Codex Swap slot {number} holds this account with a login that no longer works ({}). Removing that slot and saving again.",
        slot["loginStatus"].as_str().unwrap_or("unknown")
    );
    let removed = run_xswap(xswap, &["remove".into(), number], &[], false);
    if !removed.ok {
        return Err(LoginFailure {
            message: unrepaired().message,
            detail: format!("{reason}\n{}", xswap_reason(&removed)),
        });
    }
    let retry = run_xswap(xswap, save_args, shim, true);
    if retry.ok {
        return saved(&retry, None);
    }
    let again = xswap_reason(&retry);
    Err(LoginFailure {
        detail: format!("{reason}\n{again}"),
        ..LoginFailure::from_xswap(&again)
    })
}

/// The account a successful xswap call saved: `add --json` prints it; `login N` prints nothing and
/// saved slot N.
fn saved(run: &XswapRun, slot: Option<&str>) -> Result<Value, LoginFailure> {
    if let Some(slot) = slot {
        return Ok(report(slot, None));
    }
    first_json_account(&run.stdout)
        .map(|account| {
            report(
                &account["number"].as_u64().unwrap_or_default().to_string(),
                account["email"].as_str(),
            )
        })
        .ok_or_else(|| LoginFailure {
            message: "Codex Swap saved the login but didn't say which slot. Update Codex Swap in Settings > Accounts and try again.".into(),
            detail: run.stdout.clone(),
        })
}

fn report(number: &str, email: Option<&str>) -> Value {
    let number: Value = number.parse::<u64>().map_or(Value::Null, Value::from);
    let report = json!({"schemaVersion":1,"account":{"number":number,"email":email}});
    println!("{report}");
    report
}

/// The first JSON object on `text` with `account.number`.
fn first_json_account(text: &str) -> Option<Value> {
    text.char_indices()
        .filter(|(_, c)| *c == '{')
        .filter_map(|(index, _)| {
            serde_json::Deserializer::from_str(&text[index..])
                .into_iter::<Value>()
                .next()?
                .ok()
        })
        .find_map(|value| {
            value
                .pointer("/account/number")
                .is_some_and(Value::is_u64)
                .then(|| value["account"].clone())
        })
}

/// The shim's environment: xswap starts this `ghostex` as its Codex, which runs the real one.
/// Empty (xswap runs Codex itself, and a refusal cannot be repaired) when either is not found.
fn shim_environment(home: &Path, keep: &Path) -> Vec<(&'static str, OsString)> {
    let ghostex = std::env::args_os()
        .next()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file())
        .or_else(|| helpers::executable(home, "ghostex"));
    // xswap's own choice of Codex: XSWAP_CODEX_BIN, else its `codex-bin` setting, else `codex`.
    let configured = std::env::var_os("XSWAP_CODEX_BIN")
        .or_else(|| {
            helpers::json_command(home, "xswap", &["config", "--json"])
                .ok()?
                .pointer("/config/codex-bin")?
                .as_str()
                .map(OsString::from)
        })
        .unwrap_or_else(|| "codex".into());
    let codex = if Path::new(&configured).components().count() > 1 {
        Some(PathBuf::from(&configured))
    } else {
        helpers::executable(home, &configured.to_string_lossy())
    }
    // A PowerShell script is not something xswap (or `Command`) can start.
    .filter(|path| {
        !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ps1"))
    });
    match (ghostex, codex) {
        (Some(ghostex), Some(codex)) => vec![
            ("XSWAP_CODEX_BIN", ghostex.into_os_string()),
            (SHIM_KEEP, keep.as_os_str().to_owned()),
            (SHIM_CODEX, codex.into_os_string()),
        ],
        _ => Vec::new(),
    }
}

struct XswapRun {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// Runs xswap with its output shown on this terminal as it comes and kept for reading after.
fn run_xswap(
    xswap: &Path,
    args: &[String],
    shim: &[(&'static str, OsString)],
    replay: bool,
) -> XswapRun {
    let mut command = Command::new(xswap);
    command
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.envs(shim.iter().map(|(key, value)| (*key, value)));
    if replay && !shim.is_empty() {
        command.env(SHIM_REPLAY, "1");
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return XswapRun {
                ok: false,
                stdout: String::new(),
                stderr: format!("xswap: could not start Codex Swap: {error}"),
            }
        }
    };
    // Cancelling the login ends this process; on Windows xswap and its Codex must end with it.
    crate::platform::process::end_with_server(child.id());
    let stdout = child
        .stdout
        .take()
        .map(|pipe| std::thread::spawn(move || copy_through(pipe, std::io::stdout())));
    let stderr = child
        .stderr
        .take()
        .map(|pipe| std::thread::spawn(move || copy_through(pipe, std::io::stderr())));
    let ok = child.wait().is_ok_and(|status| status.success());
    let collect = |reader: Option<std::thread::JoinHandle<String>>| {
        reader
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default()
    };
    XswapRun {
        ok,
        stdout: collect(stdout),
        stderr: collect(stderr),
    }
}

/// Copies `pipe` to `out` as it arrives and returns the last 256 KB of it.
fn copy_through(mut pipe: impl Read, mut out: impl Write) -> String {
    const KEEP: usize = 256 * 1024;
    let mut kept = Vec::new();
    let mut buffer = [0; 4096];
    while let Ok(read) = pipe.read(&mut buffer) {
        if read == 0 {
            break;
        }
        let _ = out.write_all(&buffer[..read]);
        let _ = out.flush();
        kept.extend_from_slice(&buffer[..read]);
        if kept.len() > 2 * KEEP {
            kept.drain(..kept.len() - KEEP);
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// What xswap said when it failed: its `xswap: <error>` line (codex-swap `main`), else the last
/// line it printed.
fn xswap_reason(run: &XswapRun) -> String {
    let lines = || {
        run.stderr
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    };
    lines()
        .filter_map(|line| line.strip_prefix("xswap: "))
        .last()
        .or_else(|| lines().last())
        .unwrap_or("Codex Swap stopped without saying why.")
        .to_string()
}

impl LoginFailure {
    /// The sentence Settings shows for an xswap error; `detail` keeps xswap's own words.
    fn from_xswap(reason: &str) -> Self {
        let lower = reason.to_lowercase();
        let message = if lower.contains("codex is running (pid")
            || lower.contains("codex is still running (pid")
        {
            // setup.rs reads the PIDs from the detail and offers to pause those sessions.
            "Codex is still running and has to stop before this login can finish."
        } else if lower.contains("signed in to a different email")
            || lower.contains("signed into a different account")
        {
            "The browser signed in to a different account."
        } else if lower.contains("codex login did not complete") {
            "The Codex sign-in was cancelled or did not finish. No account was added."
        } else if lower.contains(ALREADY_SAVED) {
            "This Codex account is already saved in another Codex Swap slot."
        } else if lower.contains("could not find codex") || lower.contains("could not start codex")
        {
            "Codex isn't installed where Codex Swap can find it. Install Codex, then try again."
        } else if lower.contains("main codex home changed") {
            "Codex's main login changed while you were signing in. Try again."
        } else if lower.contains("xswap registry") || lower.contains("missing from registry") {
            "Codex Swap's list of saved accounts is damaged, so no account could be added."
        } else {
            "Codex Swap couldn't save this login. Copy details has what it said."
        };
        Self {
            message: message.into(),
            detail: reason.to_string(),
        }
    }
}
