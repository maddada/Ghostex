//! Settings › Dictation's Install Hex and Open Hex on this computer, run by the desktop app.
//!
//! The Settings page asks with `hexInstallRun` once the user has read the confirm dialog (it names
//! the command); this asks the system for the owner's presence (Touch ID / Windows Hello /
//! polkit, `ghostex_owner_keys::confirm_presence`), runs Hex's own installer, and tells the page
//! how it goes with `hexInstallState` payloads: `asking`, `installing`, then `done`, `failed`
//! (with the installer's last lines), `noHomebrew` (macOS without Homebrew) or `declined`.
//!
//! CDXC:Dictate 2026-11-08 DECISION: User: Hex's official installers, run from the desktop app
//! (not gxserver), never as root, from a temporary folder, after the same presence prompt Settings
//! already uses. A failed install shows its last error lines and the command to run by hand.
//! SEE-ALSO: window/settings_modal/tabs/voice_dictate/hex_install_plan.rs (the commands) and
//! hex.rs (the row and the dialog).

use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use crate::app::window::settings_modal::tabs::voice_dictate::hex_install_plan::{
    HexInstallPlan, last_lines, plan, presence_reason,
};

/// Hex's installer finishes in a minute or two; a stuck one is stopped after this.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// How many of the installer's last lines a failure shows.
const FAILURE_LINES: usize = 6;
/// Homebrew's usual homes (a Dock-launched app has no Homebrew on its PATH).
const BREW_PATHS: [&str; 3] = [
    "/opt/homebrew/bin/brew",
    "/usr/local/bin/brew",
    "/home/linuxbrew/.linuxbrew/bin/brew",
];

/// How a run ended, for the page.
enum Outcome {
    Done,
    NoHomebrew,
    Failed(String),
}

fn state_payload(phase: &str, extra: Value) -> Value {
    let mut payload = json!({ "type": "hexInstallState", "phase": phase });
    if let (Some(object), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
        object.extend(extra.clone());
    }
    payload
}

/// Never as root: the installers are for the user's own account, and Homebrew refuses root.
fn running_as_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn brew_path() -> Option<PathBuf> {
    BREW_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

/// A new empty folder the installer runs in (Windows' script is saved beside its cwd), removed
/// afterwards; never the user's own current folder.
fn scratch_folder() -> std::io::Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let folder = std::env::temp_dir().join(format!(
        "ghostex-hex-install-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&folder)?;
    Ok(folder)
}

/// Runs the installer in `folder` and says how it went, with its last output lines on failure.
fn run_installer(plan: &HexInstallPlan, folder: &Path) -> Outcome {
    let program = if plan.needs_homebrew {
        match brew_path() {
            Some(path) => path,
            None => return Outcome::NoHomebrew,
        }
    } else {
        PathBuf::from(plan.program)
    };
    let mut command = Command::new(program);
    command
        .args(plan.args)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NONINTERACTIVE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // CREATE_NO_WINDOW: no console flashes up for a background installer.
        command.creation_flags(0x0800_0000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return Outcome::Failed(format!("The installer couldn't start: {error}")),
    };
    let output = Arc::new(Mutex::new(String::new()));
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|pipe| {
        let output = output.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                if let Ok(mut output) = output.lock() {
                    output.push_str(&strip_escapes(&line));
                    output.push('\n');
                }
            }
        })
    })
    .collect();
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() > INSTALL_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(error) => return Outcome::Failed(format!("The installer stopped: {error}")),
        }
    };
    for reader in readers {
        let _ = reader.join();
    }
    let text = output.lock().map(|text| text.clone()).unwrap_or_default();
    match status {
        Some(status) if status.success() => Outcome::Done,
        Some(_) => Outcome::Failed(last_lines(&text, FAILURE_LINES)),
        None => Outcome::Failed(format!(
            "The installer didn't finish in {} minutes, so Ghostex stopped it.\n{}",
            INSTALL_TIMEOUT.as_secs() / 60,
            last_lines(&text, FAILURE_LINES)
        )),
    }
}

/// Removes ANSI colour and cursor sequences the installers print.
fn strip_escapes(line: &str) -> String {
    let mut clean = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if ch != '\r' {
            clean.push(ch);
        }
    }
    clean
}

/// Starts Hex on this computer (macOS: by its bundle identifier).
fn open_hex() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("/usr/bin/open")
            .args(["-b", "anomaly.hex"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("Hex couldn't be opened: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err("Hex couldn't be opened. Open it from Applications.".to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Ghostex can't open Hex on this computer. Open it yourself.".to_string())
    }
}

impl crate::GhostexGpuiApp {
    /// `hexInstallRun`: the presence prompt, then the installer, reported as `hexInstallState`.
    pub(crate) fn run_hex_install(&mut self, cx: &mut gpui::Context<Self>) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let send = |phase: &'static str, extra: Value, cx: &mut gpui::AsyncApp| {
                let payload = state_payload(phase, extra);
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_open_gpui_app_modal_message(payload, cx)
                });
            };
            let plan = plan();
            if running_as_root() {
                send(
                    "failed",
                    json!({ "tail": "Ghostex is running as the administrator account, and Hex's installer is for your own account. Run the command below yourself." }),
                    cx,
                );
                return;
            }
            if plan.needs_homebrew && brew_path().is_none() {
                send("noHomebrew", json!({}), cx);
                return;
            }
            send("asking", json!({}), cx);
            let reason = presence_reason();
            let presence = background
                .spawn(async move { ghostex_owner_keys::confirm_presence(&reason) })
                .await;
            if let Err(error) = presence {
                send("declined", json!({ "tail": error.to_string() }), cx);
                return;
            }
            send("installing", json!({}), cx);
            let outcome = background
                .spawn(async move {
                    let folder = match scratch_folder() {
                        Ok(folder) => folder,
                        Err(error) => {
                            return Outcome::Failed(format!(
                                "A temporary folder for the installer couldn't be made: {error}"
                            ));
                        }
                    };
                    let outcome = run_installer(&plan, &folder);
                    let _ = std::fs::remove_dir_all(&folder);
                    outcome
                })
                .await;
            match outcome {
                Outcome::Done => send("done", json!({}), cx),
                Outcome::NoHomebrew => send("noHomebrew", json!({}), cx),
                Outcome::Failed(tail) => send("failed", json!({ "tail": tail }), cx),
            }
        })
        .detach();
    }

    /// `hexOpen`: opens Hex; a failure comes back as `hexOpenFailed`.
    pub(crate) fn open_hex_app(&mut self, cx: &mut gpui::Context<Self>) {
        let opened = cx.background_executor().spawn(async { open_hex() });
        cx.spawn(async move |this, cx| {
            if let Err(error) = opened.await {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_open_gpui_app_modal_message(
                        json!({ "type": "hexOpenFailed", "message": error }),
                        cx,
                    )
                });
            }
        })
        .detach();
    }
}
