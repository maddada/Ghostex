//! Where a cloud session runs: a Slack request's `cloud` session (team_sync/slack_request.rs) and
//! the Work page's "Start in cloud" (work_mode/cloud_work.rs). Outside `team_sync` because a
//! Personal workspace with work mode starts cloud sessions too, with no team.
//!
//! CDXC:WorkMode 2026-10-10 DECISION:
//! User: "i want to add start in cloud which is a dropdown the shows claude code there pls (we'll
//! add more soon)". `CLOUD_PROVIDERS` is that dropdown's list: a new cloud is one entry there with
//! its runner, and the Work page and `ghostex work-mode start --cloud` pick it up.
//!
//! CDXC:TeamSync 2026-10-09 DECISION:
//! User: "cloud" means Claude Code on the web for now, but the cloud runner must stay swappable so
//! the team's own cloud computer can be added later. Every cloud start and follow-up goes through
//! `CloudRunner`; `cloud_runner()` is the one place that picks the implementation.
//!
//! CDXC:TeamSync 2026-10-09 WHY:
//! A runner can start a session and send it follow-ups, but not read it back: Claude Code 2.1.295
//! has no headless way to read a cloud session's messages (`--cloud <id>` refuses
//! `--output-format stream-json`, its stream-json attach is behind a feature flag that is off, and
//! `--teleport` moves the session into a local terminal and needs a clean checkout). So a cloud
//! session's milestones cannot reach the working thread from here, and the session itself cannot
//! post them either (no `ghostex` and no member token on the cloud machine); the working thread
//! keeps the session's link instead. Add `read` here when the CLI (or an own cloud computer) can.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::Value;

use crate::accounts::setup_terminal::{LoginTerminal, COLS, ROWS};
use crate::platform::process::background_command;

/// Claude Code's `--cloud` takes the prompt as one command-line argument; Windows caps a command
/// line at 32k characters.
pub(crate) const MAX_CLOUD_PROMPT_CHARS: usize = 12_000;
/// How long a start may take before it counts as failed.
const START_TIMEOUT: Duration = Duration::from_secs(180);
/// After the session URL appears, how long the CLI gets to finish on its own.
const EXIT_GRACE: Duration = Duration::from_secs(5);
/// How long sending a follow-up may take.
const SEND_TIMEOUT: Duration = Duration::from_secs(90);

pub(crate) struct CloudStartRequest<'a> {
    /// The project's folder; its Git remote tells the runner which repo to work on.
    pub(crate) repo_dir: &'a Path,
    /// The first message, ticket and thread included.
    pub(crate) prompt: &'a str,
    /// An existing branch the session works on and pushes to (a PR's head branch).
    pub(crate) on_branch: Option<&'a str>,
}

pub(crate) struct CloudSession {
    pub(crate) url: String,
}

pub(crate) trait CloudRunner {
    /// Stable name recorded with the session (`claude-code-web`).
    fn name(&self) -> &'static str;
    /// Starts a session and returns where to open it. Blocking.
    fn start(&self, request: &CloudStartRequest<'_>) -> Result<CloudSession, String>;
    /// Sends a follow-up message to a session this runner started (by its URL). Blocking.
    fn send(&self, session_url: &str, text: &str) -> Result<(), String>;
}

/// A cloud the "Start in cloud" menu offers.
pub(crate) struct CloudProvider {
    /// Stable id the page and the CLI send (`claude-code`).
    pub(crate) id: &'static str,
    /// The menu row's words.
    pub(crate) name: &'static str,
    /// The agent whose icon the menu row shows.
    pub(crate) agent_id: &'static str,
    runner: fn(&Path) -> Box<dyn CloudRunner + Send + Sync>,
}

impl CloudProvider {
    pub(crate) fn runner(&self, home_dir: &Path) -> Box<dyn CloudRunner + Send + Sync> {
        (self.runner)(home_dir)
    }

    pub(crate) fn wire(&self) -> Value {
        serde_json::json!({ "id": self.id, "name": self.name, "agentId": self.agent_id })
    }
}

pub(crate) const CLOUD_PROVIDERS: &[CloudProvider] = &[CloudProvider {
    id: "claude-code",
    name: "Claude Code",
    agent_id: "claude",
    runner: |home_dir| {
        Box::new(ClaudeCodeOnTheWeb {
            home_dir: home_dir.to_path_buf(),
        })
    },
}];

pub(crate) fn cloud_provider(id: &str) -> Option<&'static CloudProvider> {
    CLOUD_PROVIDERS.iter().find(|provider| provider.id == id)
}

/// The cloud a Slack request's `cloud` session runs in.
pub(crate) fn cloud_runner(home_dir: &Path) -> Box<dyn CloudRunner + Send + Sync> {
    CLOUD_PROVIDERS[0].runner(home_dir)
}

/// Claude Code on the web through the requester's own `claude` CLI and login, which is why the
/// command runs on the requester's computer and not in Convex.
///
/// CDXC:TeamSync 2026-10-09 WHY:
/// The flag was `--remote` in earlier Claude Code releases; 2.1.295 has `--cloud
/// [description|session_id|url]`. Creating a session is interactive only ("--cloud requires an
/// interactive terminal" with piped output, and `-p --cloud <description>` is turned off), so the
/// start runs in a pseudo-terminal, where the CLI prints `View: https://claude.ai/code/session_…`
/// and exits. A follow-up is headless: `claude -p --cloud <url> --output-format json` with the
/// text on stdin answers `{"ok":true,"session_id":…}`.
struct ClaudeCodeOnTheWeb {
    home_dir: PathBuf,
}

impl ClaudeCodeOnTheWeb {
    fn program(&self) -> Result<PathBuf, String> {
        crate::accounts::helpers::executable(&self.home_dir, "claude")
            .ok_or_else(|| "Claude Code (`claude`) is not installed on this computer.".to_string())
    }
}

impl CloudRunner for ClaudeCodeOnTheWeb {
    fn name(&self) -> &'static str {
        "claude-code-web"
    }

    fn start(&self, request: &CloudStartRequest<'_>) -> Result<CloudSession, String> {
        let mut command = CommandBuilder::new(self.program()?);
        command.arg("--cloud");
        command.arg(request.prompt);
        // A headless start cannot answer the "send this machine's settings?" question.
        command.args(["--forward-home-settings", "false"]);
        if let Some(branch) = request.on_branch {
            command.args(["--on-branch", branch]);
        }
        command.cwd(request.repo_dir);
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Could not open a terminal for Claude Code: {error}"))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Could not read Claude Code's terminal: {error}"))?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Could not write to Claude Code's terminal: {error}"))?;
        let mut child = pair.slave.spawn_command(command).map_err(|error| {
            format!(
                "Could not run Claude Code ({error}). Is the `claude` CLI installed and signed in?"
            )
        })?;
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
        let mut url_seen_at: Option<Instant> = None;
        let outcome = loop {
            while let Ok(chunk) = output.try_recv() {
                let replies = terminal.process(&chunk);
                if !replies.is_empty() {
                    let _ = writer.write_all(&replies);
                    let _ = writer.flush();
                }
            }
            let screen = terminal.text();
            let url = session_url(&screen);
            if url.is_some() && url_seen_at.is_none() {
                url_seen_at = Some(Instant::now());
            }
            if let Ok(Some(status)) = child.try_wait() {
                break url.ok_or_else(|| {
                    format!(
                        "Claude Code did not start a cloud session (exit {}): {}",
                        status.exit_code(),
                        last_lines(&screen)
                    )
                });
            }
            if let (Some(url), Some(seen)) = (url.as_ref(), url_seen_at) {
                if seen.elapsed() >= EXIT_GRACE {
                    // The session lives on claude.ai; the local CLI only reported it.
                    break Ok(url.clone());
                }
            }
            if started.elapsed() >= START_TIMEOUT {
                break Err(format!(
                    "Claude Code did not report a cloud session within {}s: {}",
                    START_TIMEOUT.as_secs(),
                    last_lines(&screen)
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        let _ = child.kill();
        let _ = child.wait();
        drop(writer);
        drop(pair.master);
        outcome.map(|url| CloudSession { url })
    }

    fn send(&self, session_url: &str, text: &str) -> Result<(), String> {
        let mut command = background_command(self.program()?);
        command
            .args(["-p", "--cloud", session_url, "--output-format", "json"])
            .current_dir(&self.home_dir);
        // On stdin, so a long message never meets the command-line limit.
        let out = run_with_stdin(command, text, SEND_TIMEOUT)
            .map_err(|error| format!("Could not send the message to the cloud session: {error}"))?;
        let answer = out
            .lines()
            .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
            .ok_or_else(|| format!("Claude Code gave no answer: {}", last_lines(&out)))?;
        if answer.get("ok") == Some(&Value::Bool(true)) {
            Ok(())
        } else {
            Err(format!(
                "Claude Code could not send the message to the cloud session: {}",
                answer
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("it failed")
            ))
        }
    }
}

/// Runs a command with `input` on stdin and returns its stdout, killing it after `timeout`.
pub(crate) fn run_with_stdin(
    mut command: std::process::Command,
    input: &str,
    timeout: Duration,
) -> Result<String, String> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not run it ({error})"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.read_to_string(&mut out);
        }
        out
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().ok().flatten() {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("it did not finish within {}s", timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let out = reader.join().unwrap_or_default();
    if !status.success() && out.trim().is_empty() {
        return Err(format!("it exited with {}", status.code().unwrap_or(-1)));
    }
    Ok(out)
}

/// The first `https://claude.ai/code/…` URL on the screen.
fn session_url(text: &str) -> Option<String> {
    let start = text.find("https://claude.ai/code/")?;
    let url: String = text[start..]
        .chars()
        .take_while(|character| {
            !character.is_whitespace()
                && !character.is_control()
                && !matches!(character, '"' | '\'' | ')' | '>')
        })
        .collect();
    Some(url.trim_end_matches(['.', ',']).to_string())
}

fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(4)..];
    if tail.is_empty() {
        "no output".to_string()
    } else {
        tail.join(" / ")
    }
}
