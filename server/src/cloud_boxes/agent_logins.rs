//! Agent logins for boxes (`agentLogins`, `setAgentLogin`): which saved Claude and Codex accounts a
//! box may run on, and each Claude account's box token.
//!
//! CDXC:CloudBoxes 2026-11-02 DECISION: the user chose a box token for Claude (D5 "box token"): Allow on cloud boxes runs `claude setup-token` once per account (one browser approval), and boxes get that one-year token as `CLAUDE_CODE_OAUTH_TOKEN`. It never rotates, so the computer's own login is never signed out; claude.ai connectors and Remote Control don't work on a box. Codex accounts are allowed as they are (D6 "copy and send back": `auth.json` refreshed on the computer, copied at boot, refreshed copies written back), so no sign-in runs for them here.
//!
//! The tokens are sealed with the vault (`server/src/vault.rs`) in `auth/cloud-box-logins.json`;
//! nothing here logs them, and they leave this file only through [`injected_claude_accounts`].
//! SEE-ALSO: packages/gx-protocol/src/cloud_box/logins.rs, apps/desktop/src/app/window/settings_modal/tabs/cloud_boxes/logins.rs.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ghostex_gx_protocol::cloud_box::{
    AgentLoginsResult, BoxInjectedAccount, CloudAgentLogin, CloudAgentLoginState,
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};

use super::params::internal;
use crate::accounts::model::Provider;
use crate::accounts::setup_terminal::{LoginTerminal, COLS, ROWS};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

const STORE_FILE: &str = "cloud-box-logins.json";
/// How long `claude setup-token` may wait for the browser approval.
const SETUP_TOKEN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const CLAUDE_TOKEN_PREFIX: &str = "sk-ant-oat01-";
const CLAUDE_READY_DETAIL: &str =
    "Boxes use a one-year box token, so this computer stays signed in. claude.ai connectors and Remote Control don't work on a box.";
const CODEX_READY_DETAIL: &str =
    "Boxes get a fresh copy of this login at boot and send refreshed sign-ins back to this computer.";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredLogin {
    agent: String,
    allowed_at: String,
    /// Claude only: the sealed box token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sealed_token: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Store {
    #[serde(default)]
    logins: BTreeMap<String, StoredLogin>,
}

/// A `claude setup-token` run in progress, or the failure of the last one.
#[derive(Clone)]
enum Run {
    Waiting { auth_url: Option<String> },
    Failed(String),
}

fn runs() -> &'static Mutex<HashMap<String, Run>> {
    static RUNS: OnceLock<Mutex<HashMap<String, Run>>> = OnceLock::new();
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_run(account_id: &str, run: Option<Run>) {
    let mut map = runs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match run {
        Some(run) => map.insert(account_id.to_string(), run),
        None => map.remove(account_id),
    };
}

fn store_path(paths: &GxserverPaths) -> PathBuf {
    paths.auth_dir.join(STORE_FILE)
}

/// One writer at a time: the file is read, changed and written back whole.
fn store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn read_store(paths: &GxserverPaths) -> Store {
    std::fs::read(store_path(paths))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_store(paths: &GxserverPaths, store: &Store) -> Result<(), String> {
    let path = store_path(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(store).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    std::fs::rename(&temporary, &path).map_err(|error| error.to_string())
}

fn update_store(paths: &GxserverPaths, change: impl FnOnce(&mut Store)) -> Result<(), String> {
    let _guard = store_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut store = read_store(paths);
    change(&mut store);
    write_store(paths, &store)
}

/// The saved Claude and Codex accounts the user has not turned off: (id, agent, name).
fn accounts(state: &AppState) -> Result<Vec<(String, Provider, String)>, DomainStateError> {
    let db = open_gxserver_database(&state.paths)
        .map_err(|error| internal(format!("SQLite gxserver state error: {error}")))?;
    let registry = crate::accounts::store::read(&db)?;
    Ok(registry
        .accounts
        .into_iter()
        .filter(|account| !account.user_disabled)
        .map(|account| (account.id, account.provider, account.name))
        .collect())
}

/// `agentLogins`. Blocking.
pub(super) fn list(state: &AppState) -> Result<AgentLoginsResult, DomainStateError> {
    let store = read_store(&state.paths);
    let runs = runs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let logins = accounts(state)?
        .into_iter()
        .map(|(account_id, provider, name)| {
            let saved = store.logins.get(&account_id);
            let mut row = CloudAgentLogin {
                agent: provider.id().to_string(),
                name,
                state: CloudAgentLoginState::NotAllowed,
                auth_url: None,
                detail: None,
                allowed_at: saved.map(|saved| saved.allowed_at.clone()),
                account_id: account_id.clone(),
            };
            match (runs.get(&account_id), saved) {
                (Some(Run::Waiting { auth_url }), _) => {
                    row.state = CloudAgentLoginState::Waiting;
                    row.auth_url = auth_url.clone();
                }
                (_, Some(saved)) if provider == Provider::Codex || saved.sealed_token.is_some() => {
                    row.state = CloudAgentLoginState::Ready;
                    row.detail = Some(
                        match provider {
                            Provider::Claude => CLAUDE_READY_DETAIL,
                            Provider::Codex => CODEX_READY_DETAIL,
                        }
                        .to_string(),
                    );
                }
                (Some(Run::Failed(message)), _) => {
                    row.state = CloudAgentLoginState::Error;
                    row.detail = Some(message.clone());
                }
                _ => {}
            }
            row
        })
        .collect();
    Ok(AgentLoginsResult { logins })
}

/// `setAgentLogin`. Blocking; a Claude allow returns at once with the account `waiting` and the
/// token arrives (or the failure) on a background thread, then `cloudBoxesChanged` with scope
/// `agentLogins`.
pub(super) fn set(
    state: Arc<AppState>,
    account_id: &str,
    allowed: bool,
) -> Result<AgentLoginsResult, DomainStateError> {
    let (_, provider, _) = accounts(&state)?
        .into_iter()
        .find(|(id, ..)| id == account_id)
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "No Claude or Codex account {account_id} in Settings › Accounts."
            ))
        })?;
    if !allowed {
        set_run(account_id, None);
        update_store(&state.paths, |store| {
            store.logins.remove(account_id);
        })
        .map_err(|error| internal(format!("Couldn't save the agent logins: {error}")))?;
        return list(&state);
    }
    match provider {
        Provider::Codex => {
            update_store(&state.paths, |store| {
                store.logins.insert(
                    account_id.to_string(),
                    StoredLogin {
                        agent: "codex".to_string(),
                        allowed_at: crate::domain::params::now_iso(),
                        sealed_token: None,
                    },
                );
            })
            .map_err(|error| internal(format!("Couldn't save the agent logins: {error}")))?;
        }
        Provider::Claude => {
            let already_waiting = matches!(
                runs()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(account_id),
                Some(Run::Waiting { .. })
            );
            if !already_waiting {
                let argv = {
                    let db = open_gxserver_database(&state.paths).map_err(|error| {
                        internal(format!("SQLite gxserver state error: {error}"))
                    })?;
                    crate::accounts::launch::claude_account_argv(&db, account_id).map_err(
                        |error| DomainStateError {
                            code: error.code,
                            message: format!(
                                "Couldn't start Claude on this account to make its box token: {}",
                                error.message
                            ),
                        },
                    )?
                };
                set_run(account_id, Some(Run::Waiting { auth_url: None }));
                let account_id = account_id.to_string();
                let state = state.clone();
                std::thread::spawn(move || {
                    let outcome = run_setup_token(&argv, &account_id).and_then(|token| {
                        let sealed = crate::vault::seal(&state.paths, &token)?;
                        update_store(&state.paths, |store| {
                            store.logins.insert(
                                account_id.clone(),
                                StoredLogin {
                                    agent: "claude".to_string(),
                                    allowed_at: crate::domain::params::now_iso(),
                                    sealed_token: Some(sealed),
                                },
                            );
                        })
                    });
                    set_run(&account_id, outcome.err().map(Run::Failed));
                    super::events::broadcast_scope(&state, &["agentLogins"]);
                });
            }
        }
    }
    list(&state)
}

/// Runs `claude setup-token` on the account in a terminal Ghostex reads, until it prints the
/// token. Claude opens the approval page in the browser itself; the page's address is kept for
/// Settings in case the browser did not open.
fn run_setup_token(argv: &[String], account_id: &str) -> Result<String, String> {
    let (program, arguments) = argv
        .split_first()
        .ok_or_else(|| "Couldn't find Claude for this account.".to_string())?;
    let mut command = CommandBuilder::new(program);
    command.args(arguments);
    command.arg("setup-token");
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: ROWS,
            cols: COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("Couldn't open a terminal for Claude: {error}"))?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("Couldn't read Claude's terminal: {error}"))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("Couldn't write to Claude's terminal: {error}"))?;
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("Couldn't run Claude ({error}). Is Claude Code installed?"))?;
    drop(pair.slave);
    let (sender, output) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        while let Ok(read) = reader.read(&mut buffer) {
            if read == 0 || sender.send(buffer[..read].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut terminal = LoginTerminal::new();
    let started = Instant::now();
    let outcome = loop {
        while let Ok(chunk) = output.try_recv() {
            let replies = terminal.process(&chunk);
            if !replies.is_empty() {
                let _ = writer.write_all(&replies);
                let _ = writer.flush();
            }
        }
        let screen = terminal.text();
        if let Some(token) = box_token(&screen) {
            break Ok(token);
        }
        if let Some(url) = approval_url(&screen) {
            let mut map = runs()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(Run::Waiting { auth_url }) = map.get_mut(account_id) {
                if auth_url.is_none() {
                    *auth_url = Some(url);
                }
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            // Output may still be in the channel when the process ends.
            std::thread::sleep(Duration::from_millis(200));
            while let Ok(chunk) = output.try_recv() {
                terminal.process(&chunk);
            }
            let screen = terminal.text();
            break box_token(&screen).ok_or_else(|| {
                format!(
                    "Claude didn't make a box token (exit {}): {}",
                    status.exit_code(),
                    last_line(&screen)
                )
            });
        }
        if started.elapsed() >= SETUP_TOKEN_TIMEOUT {
            break Err("The browser approval wasn't finished within 10 minutes.".to_string());
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    let _ = child.kill();
    let _ = child.wait();
    outcome
}

/// The `sk-ant-oat01-…` token `claude setup-token` prints when the approval is done.
fn box_token(screen: &str) -> Option<String> {
    let start = screen.find(CLAUDE_TOKEN_PREFIX)?;
    let token: String = screen[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .collect();
    (token.len() > CLAUDE_TOKEN_PREFIX.len() + 20).then_some(token)
}

/// The approval page Claude asks the user to open.
fn approval_url(screen: &str) -> Option<String> {
    screen
        .split_whitespace()
        .find(|word| word.starts_with("https://") && word.contains("oauth"))
        .map(str::to_string)
}

fn last_line(screen: &str) -> String {
    screen
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains(CLAUDE_TOKEN_PREFIX))
        .last()
        .unwrap_or("it stopped")
        .to_string()
}

/// The Claude accounts allowed on boxes with their unsealed box tokens, for a box's
/// `secrets.json` (`BoxSecretsFile::accounts`). Codex accounts are added by the boot path, which
/// refreshes their `auth.json` first (D6). Blocking.
pub(crate) fn injected_claude_accounts(state: &AppState) -> Vec<BoxInjectedAccount> {
    let store = read_store(&state.paths);
    let Ok(accounts) = accounts(state) else {
        return Vec::new();
    };
    accounts
        .into_iter()
        .filter(|(_, provider, _)| *provider == Provider::Claude)
        .filter_map(|(account_id, _, name)| {
            let sealed = store.logins.get(&account_id)?.sealed_token.as_ref()?;
            let value = crate::vault::unseal(&state.paths, sealed).ok()?;
            Some(BoxInjectedAccount {
                account_id,
                agent: "claude".to_string(),
                name,
                kind: "claudeBoxToken".to_string(),
                value,
            })
        })
        .collect()
}

/// The Codex accounts allowed on boxes (their ids), for the boot path. Blocking.
pub(crate) fn allowed_codex_account_ids(state: &AppState) -> Vec<String> {
    read_store(&state.paths)
        .logins
        .into_iter()
        .filter(|(_, login)| login.agent == "codex")
        .map(|(account_id, _)| account_id)
        .collect()
}
