//! Whether an agent CLI on this computer is signed in, and its own browser sign-in, for first-run
//! setup's Agents panel (`/api/agentSignIn`).
//!
//! CDXC:Onboarding 2026-11-11 DECISION:
//! User, on the first-run page (step 15, the agent is missing or signed out), picked "Check install and sign-in during setup": the Agents panel checks that the chosen agent is installed and signed in, says so, and offers the sign-in when it is not, so the first message does not end on a "Sign in to Claude" card. This reads the CLI's own status command (`claude auth status --json`, `codex login status`, `cursor-agent status`) and runs its own login command (`claude auth login`, `codex login`, `cursor-agent login`), which opens the browser; Ghostex never handles the credential. Agents without a status command answer `signedIn: null` (unknown) and the panel says nothing about them.
//! WHY: the status read is its own call, not a field of `/api/readAgentHookStatus`: that read runs before every agent launch and takes milliseconds, while a CLI status command can take seconds.
//! SEE-ALSO: apps/desktop/src/app/window/onboarding/agents.rs (the panel), accounts/default_login.rs (the same `claude auth status` read for cswap's default login), accounts/setup.rs (Settings > Accounts' sign-in of a saved account, which this does not touch).

use std::{
    collections::HashMap,
    io::Read,
    path::Path,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use serde_json::{json, Map, Value};

use crate::accounts::setup_terminal::{LoginTerminal, COLS, ROWS};
use crate::domain::DomainStateError;
use crate::server::AppState;

/// How long a status command may take before the answer is "unknown".
const STATUS_LIMIT: Duration = Duration::from_secs(12);
/// How long a sign-in waits for the browser before it is stopped.
const SIGN_IN_LIMIT: Duration = Duration::from_secs(15 * 60);

/// The agents whose CLI can say whether it is signed in, with the executable, its status
/// arguments and its login arguments.
struct SignInCommands {
    executable: &'static str,
    status: &'static [&'static str],
    login: &'static [&'static str],
}

fn commands(agent_id: &str) -> Option<SignInCommands> {
    Some(match agent_id {
        "claude" => SignInCommands {
            executable: "claude",
            status: &["auth", "status", "--json"],
            login: &["auth", "login"],
        },
        "codex" => SignInCommands {
            executable: "codex",
            status: &["login", "status"],
            login: &["login"],
        },
        "cursor" => SignInCommands {
            executable: "cursor-agent",
            status: &["status"],
            login: &["login"],
        },
        _ => return None,
    })
}

/// A sign-in started from setup, by agent id.
struct SignIn {
    started: Instant,
    killer: Option<Box<dyn ChildKiller + Send + Sync>>,
    /// The sign-in page the CLI printed, for when the browser did not open.
    url: Option<String>,
    /// Set once the login command exited: whether it said it succeeded.
    finished: Option<bool>,
}

static SIGN_INS: LazyLock<Mutex<HashMap<String, SignIn>>> = LazyLock::new(Default::default);

fn bad(message: impl Into<String>) -> DomainStateError {
    DomainStateError::bad_request(message.into())
}

/// `/api/agentSignIn` with `operation` `status`, `start` or `cancel` and an `agentId`.
pub(crate) fn dispatch(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let agent_id = params
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty() && id.len() <= 64)
        .ok_or_else(|| bad("agentId is required"))?
        .to_string();
    let home = state.paths.home_dir.clone();
    match params.get("operation").and_then(Value::as_str) {
        Some("status") => Ok(status(state, &home, &agent_id)),
        Some("start") => {
            start(&home, &agent_id)?;
            Ok(status(state, &home, &agent_id))
        }
        Some("cancel") => {
            stop(&agent_id);
            Ok(status(state, &home, &agent_id))
        }
        _ => Err(bad("Unknown agent sign-in operation.")),
    }
}

/// The agent's sign-in state: `signedIn` true, false or null (cannot tell), and while a sign-in
/// started here is running, `signingIn` with the page it opened.
fn status(state: &AppState, home: &Path, agent_id: &str) -> Value {
    let running = {
        let mut sign_ins = SIGN_INS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(sign_in) = sign_ins.get_mut(agent_id) {
            if sign_in.finished.is_none() && sign_in.started.elapsed() > SIGN_IN_LIMIT {
                if let Some(mut killer) = sign_in.killer.take() {
                    let _ = killer.kill();
                }
                sign_in.finished = Some(false);
            }
        }
        sign_ins
            .get(agent_id)
            .filter(|sign_in| sign_in.finished.is_none())
            .map(|sign_in| sign_in.url.clone())
    };
    let signed_in = if has_saved_account(state, agent_id) {
        Some(true)
    } else {
        read_signed_in(home, agent_id)
    };
    let failed = {
        let mut sign_ins = SIGN_INS.lock().unwrap_or_else(|e| e.into_inner());
        // A finished sign-in is reported once, then forgotten.
        match sign_ins.get(agent_id).and_then(|sign_in| sign_in.finished) {
            Some(_) => {
                sign_ins.remove(agent_id);
                signed_in != Some(true)
            }
            None => false,
        }
    };
    json!({
        "agentId": agent_id,
        "signedIn": signed_in,
        "signingIn": running.is_some() && signed_in != Some(true),
        "signInUrl": running.flatten(),
        "signInFailed": failed,
    })
}

/// WHY: a computer with a Claude or Codex account saved in Settings > Accounts starts sessions on
/// those, so its plain CLI login being signed out says nothing about the first message (the
/// default login of the computer setup was built on is exactly that).
fn has_saved_account(state: &AppState, agent_id: &str) -> bool {
    use crate::accounts::model::Provider;
    let provider = match agent_id {
        "claude" => Provider::Claude,
        "codex" => Provider::Codex,
        _ => return false,
    };
    crate::storage::open_gxserver_database(&state.paths)
        .ok()
        .and_then(|db| crate::accounts::store::read(&db).ok())
        .is_some_and(|registry| {
            registry
                .accounts
                .iter()
                .any(|account| account.provider == provider)
        })
}

/// What the CLI's own status command says, `None` when it cannot say (not installed, no status
/// command, too slow, or output it cannot read).
fn read_signed_in(home: &Path, agent_id: &str) -> Option<bool> {
    let commands = commands(agent_id)?;
    let executable = crate::accounts::helpers::probe_executable(home, commands.executable)?;
    let mut command = crate::platform::process::background_command(executable);
    command
        .args(commands.status)
        .current_dir(home)
        // The default login, the one a plain `claude` or `codex` in a new session uses.
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (success, text) = output_within(command, STATUS_LIMIT)?;
    parse_status(agent_id, success, &text)
}

fn parse_status(agent_id: &str, success: bool, text: &str) -> Option<bool> {
    match agent_id {
        "claude" => {
            let start = text.find('{')?;
            let status: Value = serde_json::from_str(text[start..].trim()).ok()?;
            status.get("loggedIn").and_then(Value::as_bool)
        }
        // `codex login status` prints "Logged in using …" and exits 0, or "Not logged in" and 1.
        "codex" | "cursor" => {
            let lower = text.to_ascii_lowercase();
            if lower.contains("not logged in") || lower.contains("not authenticated") {
                Some(false)
            } else if lower.contains("logged in") {
                Some(true)
            } else if agent_id == "codex" && !success {
                Some(false)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether `command` exited successfully and what it printed, if it finished within `limit`.
fn output_within(mut command: std::process::Command, limit: Duration) -> Option<(bool, String)> {
    let mut child = command.spawn().ok()?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < limit => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let mut text = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut text);
    }
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut text);
    }
    Some((status.success(), text))
}

/// Runs the CLI's own login command in a terminal of its own, which opens the browser. One per
/// agent at a time; a second start while one runs changes nothing.
fn start(home: &Path, agent_id: &str) -> Result<(), DomainStateError> {
    let commands =
        commands(agent_id).ok_or_else(|| bad("Ghostex can't sign in to this agent from here."))?;
    {
        let sign_ins = SIGN_INS.lock().unwrap_or_else(|e| e.into_inner());
        if sign_ins
            .get(agent_id)
            .is_some_and(|sign_in| sign_in.finished.is_none())
        {
            return Ok(());
        }
    }
    let executable = crate::accounts::helpers::probe_executable(home, commands.executable)
        .ok_or_else(|| bad(format!("{} isn't installed yet.", commands.executable)))?;
    let mut command = CommandBuilder::new(executable);
    command.args(commands.login);
    command.cwd(home);
    command.env("TERM", "xterm-256color");
    command.env_remove("CLAUDE_CONFIG_DIR");
    command.env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR");
    command.env_remove("CODEX_HOME");
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: ROWS,
            cols: COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| bad(error.to_string()))?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| bad(error.to_string()))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|error| bad(error.to_string()))?;
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| bad(error.to_string()))?;
    drop(pair.slave);
    SIGN_INS.lock().unwrap_or_else(|e| e.into_inner()).insert(
        agent_id.to_string(),
        SignIn {
            started: Instant::now(),
            killer: Some(child.clone_killer()),
            url: None,
            finished: None,
        },
    );
    let id = agent_id.to_string();
    let reader_id = id.clone();
    std::thread::spawn(move || {
        let mut terminal = LoginTerminal::new();
        let mut bytes = [0; 4096];
        while let Ok(n) = reader.read(&mut bytes) {
            if n == 0 {
                break;
            }
            // The terminal's answers to the CLI's own queries (cursor position and the like).
            let replies = terminal.process(&bytes[..n]);
            if !replies.is_empty() {
                let _ = std::io::Write::write_all(&mut writer, &replies);
            }
            if let Some(url) = sign_in_url(&terminal.text()) {
                if let Some(sign_in) = SIGN_INS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get_mut(&reader_id)
                {
                    sign_in.url.get_or_insert(url);
                }
            }
        }
    });
    std::thread::spawn(move || {
        let succeeded = child.wait().is_ok_and(|exit| exit.success());
        drop(pair.master);
        if let Some(sign_in) = SIGN_INS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            sign_in.killer = None;
            sign_in.finished = Some(succeeded);
        }
    });
    Ok(())
}

fn stop(agent_id: &str) {
    if let Some(mut sign_in) = SIGN_INS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(agent_id)
    {
        if let Some(mut killer) = sign_in.killer.take() {
            let _ = killer.kill();
        }
    }
}

/// The first sign-in page the login printed, on a host that belongs to the agent's maker.
fn sign_in_url(output: &str) -> Option<String> {
    output.split_whitespace().find_map(|word| {
        let start = word.find("https://")?;
        let candidate = word[start..].trim_end_matches(['\'', '"', '>', ')', '.']);
        let url = url::Url::parse(candidate).ok()?;
        let host = url.host_str()?;
        [
            "claude.com",
            "claude.ai",
            "anthropic.com",
            "openai.com",
            "chatgpt.com",
            "cursor.com",
        ]
        .iter()
        .any(|maker| host == *maker || host.ends_with(&format!(".{maker}")))
        .then(|| candidate.to_string())
    })
}
