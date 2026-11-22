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
use std::sync::atomic::{AtomicBool, Ordering};
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
/// How much of `claude setup-token`'s raw output is kept to find the page's link in.
const RAW_OUTPUT_LIMIT: usize = 256 * 1024;
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
    Waiting {
        auth_url: Option<String>,
        /// Set by Cancel; the run's thread stops the process when it sees it. Also tells this run
        /// apart from a newer one for the same account.
        cancel: Arc<AtomicBool>,
    },
    Failed(String),
}

/// What [`run_setup_token`] answers when Cancel stopped it.
const CANCELLED: &str = "Cancelled.";

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

/// Cancel: stops the account's waiting `claude setup-token` run (its thread kills the process
/// within a poll) and forgets it, so the row goes back to how it was before Allow. True when a run
/// was waiting.
fn cancel_run(account_id: &str) -> bool {
    let mut map = runs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let waiting = match map.get(account_id) {
        Some(Run::Waiting { cancel, .. }) => {
            cancel.store(true, Ordering::SeqCst);
            true
        }
        _ => false,
    };
    map.remove(account_id);
    waiting
}

/// The end of the run that `cancel` belongs to: a failure is kept for the row, unless the run was
/// cancelled or a newer run has taken its place.
fn settle_run(account_id: &str, cancel: &Arc<AtomicBool>, outcome: Result<(), String>) {
    let mut map = runs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let current = matches!(
        map.get(account_id),
        Some(Run::Waiting { cancel: own, .. }) if Arc::ptr_eq(own, cancel)
    );
    if !current {
        return;
    }
    match outcome {
        Err(message) if !cancel.load(Ordering::SeqCst) => {
            map.insert(account_id.to_string(), Run::Failed(message));
        }
        _ => {
            map.remove(account_id);
        }
    }
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
                (Some(Run::Waiting { auth_url, .. }), _) => {
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
        // Cancel while the approval waits: stop `claude setup-token` and leave the saved login as
        // it was. Remove (nothing waiting) forgets the account's box login.
        if cancel_run(account_id) {
            return list(&state);
        }
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
                let cancel = Arc::new(AtomicBool::new(false));
                set_run(
                    account_id,
                    Some(Run::Waiting {
                        auth_url: None,
                        cancel: cancel.clone(),
                    }),
                );
                let account_id = account_id.to_string();
                let state = state.clone();
                std::thread::spawn(move || {
                    let outcome = run_setup_token(&argv, &account_id, &cancel).and_then(|token| {
                        // Cancel came as the token arrived: don't keep it.
                        if cancel.load(Ordering::SeqCst) {
                            return Err(CANCELLED.to_string());
                        }
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
                    settle_run(&account_id, &cancel, outcome);
                    super::events::broadcast_scope(&state, &["agentLogins"]);
                });
            }
        }
    }
    list(&state)
}

/// Where `claude setup-token` hands its approval page: `$BROWSER` is a two-line script that writes
/// the address to a file here (0700, removed when the run ends), so Ghostex opens the page itself.
/// Claude's own opener went nowhere from gxserver (a browser picker as the default browser, or no
/// window session), and the page it prints instead is hard-wrapped at the terminal's width and
/// asks for a code to be pasted into a terminal nobody sees.
struct BrowserHandoff {
    dir: PathBuf,
}

impl BrowserHandoff {
    fn new() -> Option<Self> {
        if !cfg!(unix) {
            return None;
        }
        let mut entropy = [0u8; 8];
        rand::Rng::fill_bytes(&mut rand::rng(), &mut entropy);
        let suffix: String = entropy.iter().map(|byte| format!("{byte:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("ghostex-box-token-{suffix}"));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&dir).ok()?;
        let handoff = Self { dir };
        let script = handoff.script();
        std::fs::write(&script, browser_script(&handoff.url_file())).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).ok()?;
        }
        Some(handoff)
    }

    fn script(&self) -> PathBuf {
        self.dir.join("browser")
    }

    fn url_file(&self) -> PathBuf {
        self.dir.join("url")
    }

    /// The page Claude asked to open, once it has.
    fn url(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.url_file()).ok()?;
        let url = text.lines().next()?.trim();
        url.starts_with("https://").then(|| url.to_string())
    }
}

impl Drop for BrowserHandoff {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The `$BROWSER` script: saves the address it is given, opens nothing.
fn browser_script(url_file: &std::path::Path) -> String {
    let quoted = url_file.to_string_lossy().replace('\'', "'\\''");
    format!("#!/bin/sh\nprintf '%s\\n' \"$1\" > '{quoted}'\n")
}

/// Runs `claude setup-token` on the account in a terminal Ghostex reads, until it prints the
/// token. The approval page Claude opens comes to Ghostex through `$BROWSER` ([`BrowserHandoff`])
/// and Ghostex opens it in this computer's browser; its address is kept for Settings' Open the
/// Page Again. Cancel (`cancel` set) stops it within a poll and answers [`CANCELLED`].
fn run_setup_token(
    argv: &[String],
    account_id: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let (program, arguments) = argv
        .split_first()
        .ok_or_else(|| "Couldn't find Claude for this account.".to_string())?;
    let mut command = CommandBuilder::new(program);
    command.args(arguments);
    command.arg("setup-token");
    let handoff = BrowserHandoff::new();
    if let Some(handoff) = &handoff {
        command.env("BROWSER", handoff.script());
    }
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
    // The raw output, for the page's whole address in its OSC 8 link (the screen wraps it).
    let mut raw: Vec<u8> = Vec::new();
    let mut opened = false;
    let started = Instant::now();
    let outcome = loop {
        while let Ok(chunk) = output.try_recv() {
            let replies = terminal.process(&chunk);
            if !replies.is_empty() {
                let _ = writer.write_all(&replies);
                let _ = writer.flush();
            }
            if raw.len() < RAW_OUTPUT_LIMIT {
                raw.extend_from_slice(&chunk);
            }
        }
        let screen = terminal.text();
        if let Some(token) = box_token(&screen) {
            break Ok(token);
        }
        let handed = handoff.as_ref().and_then(BrowserHandoff::url);
        if let Some(url) = handed.clone().or_else(|| approval_url(&raw)) {
            let mut map = runs()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(Run::Waiting { auth_url, .. }) = map.get_mut(account_id) {
                // The handed-over page finishes by itself; the printed one is only a fallback.
                if auth_url.is_none() || handed.is_some() {
                    *auth_url = Some(url);
                }
            }
        }
        if let Some(url) = handed.filter(|_| !opened) {
            opened = true;
            super::connect::open_browser(&url);
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
        if cancel.load(Ordering::SeqCst) {
            break Err(CANCELLED.to_string());
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

/// The approval page Claude prints, whole: from its OSC 8 link (`ESC ] 8 ; params ; URL BEL`),
/// since the text under it is hard-wrapped at the terminal's width.
fn approval_url(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    text.split("\u{1b}]8;").skip(1).find_map(|link| {
        let (_, rest) = link.split_once(';')?;
        let end = rest.find(['\u{7}', '\u{1b}'])?;
        let url = &rest[..end];
        (url.starts_with("https://") && url.contains("oauth")).then(|| url.to_string())
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What `claude setup-token` printed in a 160-column terminal (2026-11-07): the page's address
    /// whole in its OSC 8 link, then hard-wrapped at 160 columns as text.
    const PRINTED: &str = "Browser didn't open? Use the url below to sign in (c to copy)\x1b[39m\r\r\n\r\r\n\x1b]8;id=173tp6t;https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=user%3Ainference&code_challenge=C1uM&code_challenge_method=S256&state=A96i\x07\x1b[38;2;102;102;102mhttps://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.co\r\r\nm%2Foauth%2Fcode%2Fcallback&scope=user%3Ainference\x1b]8;;\x07\r\r\n";

    #[test]
    fn the_printed_page_is_read_whole_from_its_link_not_the_wrapped_text() {
        let url = approval_url(PRINTED.as_bytes()).unwrap();
        assert!(
            url.ends_with("code_challenge=C1uM&code_challenge_method=S256&state=A96i"),
            "{url}"
        );
        assert!(url.contains("platform.claude.com%2Foauth%2Fcode%2Fcallback"));
        assert_eq!(
            approval_url(b"no link here https://claude.com/cai/oauth/x"),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_browser_script_saves_the_page_claude_hands_it() {
        let dir = tempfile::tempdir().unwrap();
        // A quote in the folder's name must not break the script.
        let folder = dir.path().join("it's here");
        std::fs::create_dir(&folder).unwrap();
        let url_file = folder.join("url");
        let script = folder.join("browser");
        std::fs::write(&script, browser_script(&url_file)).unwrap();
        let page = "https://claude.com/cai/oauth/authorize?code=true&redirect_uri=http%3A%2F%2Flocalhost%3A52308%2Fcallback&state=a b";
        let status = std::process::Command::new("sh")
            .arg(&script)
            .arg(page)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            std::fs::read_to_string(&url_file).unwrap(),
            format!("{page}\n")
        );
    }

    /// A stand-in for `claude setup-token`: `sh -c <script> setup-token` notes its pid, then waits
    /// for an approval that never comes.
    #[cfg(unix)]
    #[test]
    fn cancel_stops_a_waiting_setup_token_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let script = format!("echo $$ > '{}'; exec sleep 600", pid_file.display());
        let argv = vec!["sh".to_string(), "-c".to_string(), script];
        let account_id = "test-cancel-stops-setup-token";
        let cancel = Arc::new(AtomicBool::new(false));
        set_run(
            account_id,
            Some(Run::Waiting {
                auth_url: None,
                cancel: cancel.clone(),
            }),
        );
        let run = {
            let cancel = cancel.clone();
            std::thread::spawn(move || run_setup_token(&argv, account_id, &cancel))
        };
        let started = Instant::now();
        let pid = loop {
            if let Some(pid) = std::fs::read_to_string(&pid_file)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
            {
                break pid;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the fake never started"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) } == 0;
        assert!(alive(pid));

        assert!(cancel_run(account_id));
        let cancelled_at = Instant::now();
        let outcome = run.join().unwrap();
        assert_eq!(outcome, Err(CANCELLED.to_string()));
        assert!(
            cancelled_at.elapsed() < Duration::from_secs(3),
            "took {:?}",
            cancelled_at.elapsed()
        );
        assert!(!alive(pid), "setup-token was left running");

        // The row goes back to how it was: no waiting run, and no error.
        settle_run(account_id, &cancel, outcome.map(|_| ()));
        assert!(runs().lock().unwrap().get(account_id).is_none());
    }

    #[test]
    fn a_run_that_ends_settles_only_its_own_row() {
        let account_id = "test-settle-own-row";
        let waiting = |cancel: &Arc<AtomicBool>| Run::Waiting {
            auth_url: None,
            cancel: cancel.clone(),
        };
        let state = || match runs().lock().unwrap().get(account_id) {
            None => "none".to_string(),
            Some(Run::Waiting { .. }) => "waiting".to_string(),
            Some(Run::Failed(message)) => format!("failed: {message}"),
        };

        // A failure is kept for the row.
        let first = Arc::new(AtomicBool::new(false));
        set_run(account_id, Some(waiting(&first)));
        settle_run(account_id, &first, Err("timed out".to_string()));
        assert_eq!(state(), "failed: timed out");
        // Cancel with nothing waiting does nothing to the saved login.
        assert!(!cancel_run(account_id));
        assert_eq!(state(), "none");

        // Cancelled, then allowed again: the old run's end leaves the new one waiting.
        let old = Arc::new(AtomicBool::new(false));
        set_run(account_id, Some(waiting(&old)));
        assert!(cancel_run(account_id));
        assert!(old.load(Ordering::SeqCst));
        let new = Arc::new(AtomicBool::new(false));
        set_run(account_id, Some(waiting(&new)));
        settle_run(account_id, &old, Err(CANCELLED.to_string()));
        assert_eq!(state(), "waiting");
        settle_run(account_id, &new, Ok(()));
        assert_eq!(state(), "none");
    }

    #[cfg(unix)]
    #[test]
    fn the_handoff_reads_the_page_and_cleans_up() {
        let handoff = BrowserHandoff::new().unwrap();
        assert_eq!(handoff.url(), None);
        std::fs::write(
            handoff.url_file(),
            "https://claude.com/cai/oauth/authorize?x=1\n",
        )
        .unwrap();
        assert_eq!(
            handoff.url().as_deref(),
            Some("https://claude.com/cai/oauth/authorize?x=1")
        );
        let dir = handoff.dir.clone();
        drop(handoff);
        assert!(!dir.exists());
    }
}
