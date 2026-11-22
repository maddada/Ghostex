//! Signing a workspace in to `gh` or `gws`: Ghostex runs the tool's own login against the
//! workspace's config folder, reads the link it prints (and `gh`'s one-time code), and leaves it
//! waiting in the background for the browser to finish; Settings and the CLI open the link.
//!
//! CDXC:Connectors 2026-10-16 WHY:
//! Both logins print their link and wait when they have no terminal (`gh auth login --web` prints a device code and github.com/login/device; `gws auth login` prints Google's page and waits for its redirect to a local port), and neither opens a browser then (checked on gh 2.97.0 and gws 0.22.5), so Ghostex opens it once. `gws` needs the user's own Google Cloud OAuth client, which is copied from their usual `gws` folder. `gh` keeps the workspace's token in the workspace's folder (`--insecure-storage`, as the user's own `gh` does here) rather than the system keychain, which `gh` shares between folders signed in as the same account: a workspace's sign-out would have signed their usual `gh` out too. Signing out is deleting the folder, for the same reason.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::paths::GxserverPaths;

use super::tools::{config_dir, executable, tool_env, CliTool};

/// How long Ghostex waits for the login to print its link.
const LINK_WITHIN: Duration = Duration::from_secs(20);
/// A login nobody finishes is stopped after this.
const LOGIN_WITHIN: Duration = Duration::from_secs(15 * 60);

/// Logins waiting for the browser, by config folder.
static ACTIVE: Mutex<Option<HashMap<PathBuf, Child>>> = Mutex::new(None);

fn with_active<T>(f: impl FnOnce(&mut HashMap<PathBuf, Child>) -> T) -> T {
    let mut guard = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(HashMap::new))
}

type ChangeListener = Arc<dyn Fn(&str) + Send + Sync>;
static CHANGE_LISTENER: Mutex<Option<ChangeListener>> = Mutex::new(None);

/// Called with the workspace id when one of its logins finishes or stops.
pub(crate) fn set_changed_listener(listener: ChangeListener) {
    *CHANGE_LISTENER
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(listener);
}

fn notify_changed(workspace: &str) {
    let listener = CHANGE_LISTENER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(listener) = listener {
        listener(workspace);
    }
}

/// Whether a login for the tool is waiting for the browser.
pub(crate) fn signing_in(paths: &GxserverPaths, workspace: &str, tool: CliTool) -> bool {
    let dir = config_dir(paths, workspace, tool);
    with_active(|active| match active.get_mut(&dir) {
        Some(child) => matches!(child.try_wait(), Ok(None)),
        None => false,
    })
}

/// Stops a login and what it started: `gws` is a Node wrapper that runs the real `gws` as its
/// child, so the whole process group goes.
fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    // SAFETY: signals the login's own process group, which `sign_in` created for it.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn stop_login(dir: &Path) {
    if let Some(mut child) = with_active(|active| active.remove(dir)) {
        stop_child(&mut child);
    }
}

fn create_private_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("Couldn't make the sign-in folder: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// What a started login shows the user.
pub(crate) struct CliSignIn {
    pub(crate) url: String,
    /// `gh`'s one-time code, typed on github.com/login/device.
    pub(crate) code: Option<String>,
}

/// What the login printed, line by line, until it gave its link.
fn read_link(tool: CliTool, lines: &mpsc::Receiver<String>) -> Option<CliSignIn> {
    let deadline = std::time::Instant::now() + LINK_WITHIN;
    let mut url = None;
    let mut code = None;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        let Ok(line) = lines.recv_timeout(left) else {
            break;
        };
        let line = line.trim();
        if let Some(rest) = line.split("one-time code:").nth(1) {
            code = Some(rest.trim().to_string());
        }
        if let Some(start) = line.find("https://") {
            url = Some(
                line[start..]
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            );
        }
        let complete = match tool {
            CliTool::Gh => url.is_some() && code.is_some(),
            CliTool::Gws => url.is_some(),
        };
        if complete {
            return url.map(|url| CliSignIn { url, code });
        }
    }
    None
}

/// Starts the tool's login for the workspace and returns the link to open (and `gh`'s code). A
/// login already waiting is replaced, so the newest link is the one that works.
pub(crate) fn sign_in(
    paths: &GxserverPaths,
    workspace: &str,
    tool: CliTool,
) -> Result<CliSignIn, String> {
    let program = executable(tool).ok_or_else(|| {
        format!(
            "{} isn't installed on this computer. Install it first, then sign in again.",
            tool.name()
        )
    })?;
    let dir = config_dir(paths, workspace, tool);
    create_private_dir(&dir)?;
    if tool == CliTool::Gws {
        super::client::ensure_client(paths, &dir)?;
    }
    stop_login(&dir);
    let mut command = crate::platform::process::background_command(&program);
    match tool {
        CliTool::Gh => command.args([
            "auth",
            "login",
            "--hostname",
            "github.com",
            "--git-protocol",
            "https",
            "--web",
            "--insecure-storage",
        ]),
        CliTool::Gws => command.args(["auth", "login"]),
    };
    command
        .env("PATH", crate::platform::live_path::value())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    for (name, value) in tool_env(paths, workspace, tool) {
        command.env(name, value);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Couldn't start {}: {error}", tool.name()))?;
    let (sender, lines) = mpsc::channel::<String>();
    let readers: Vec<Box<dyn Read + Send>> = [
        child
            .stdout
            .take()
            .map(|out| Box::new(out) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|err| Box::new(err) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    for reader in readers {
        let sender = sender.clone();
        // Read to the end, so the login never blocks on a full pipe while it waits.
        std::thread::spawn(move || {
            for line in BufReader::new(reader).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        });
    }
    drop(sender);
    let Some(started) = read_link(tool, &lines) else {
        stop_child(&mut child);
        return Err(format!(
            "{} didn't show a sign-in link. Try again, or sign in from a terminal.",
            tool.name()
        ));
    };
    with_active(|active| active.insert(dir.clone(), child));
    let workspace = workspace.to_string();
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + LOGIN_WITHIN;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let done = with_active(|active| match active.get_mut(&dir) {
                Some(child) => !matches!(child.try_wait(), Ok(None)),
                None => true,
            });
            if done || std::time::Instant::now() >= deadline {
                stop_login(&dir);
                notify_changed(&workspace);
                return;
            }
        }
    });
    Ok(started)
}

/// Signs the workspace out of the tool: its folder goes away (a `gws` client the workspace chose
/// stays), so its agents use the user's usual sign-in again.
pub(crate) fn sign_out(paths: &GxserverPaths, workspace: &str, tool: CliTool) {
    let dir = config_dir(paths, workspace, tool);
    stop_login(&dir);
    let kept = super::client::kept_client(&dir);
    let _ = fs::remove_dir_all(&dir);
    if let Some(client) = kept {
        super::client::restore_client(&dir, &client);
    }
    notify_changed(workspace);
}
