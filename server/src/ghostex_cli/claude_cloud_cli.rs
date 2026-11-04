//! `ghostex claude-cloud`: Claude cloud sessions (Claude Code on the web) as sidebar sessions. The
//! work happens in gxserver (`/api/claudeCloud`, server/src/claude_cloud/); `attach` is the client
//! a cloud session's own terminal runs.

use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::time::Duration;

use crate::ghostex_cli::{
    args::{parse_args, Flags},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
    selector, sessions,
};
use serde_json::{json, Value};

const HELP: &str =
    "Claude sessions that run in Claude's cloud (Claude Code on the web) and show in the sidebar.

USAGE
  ghostex create-agent claude-cloud --project-id <id> [--prompt <task>]
  ghostex claude-cloud status <session> [--json]
  ghostex claude-cloud send <session> <message>
  ghostex claude-cloud bring-back <session> [--json]
  ghostex claude-cloud copy <session> --account <account id> [--json]

  create-agent claude-cloud starts a cloud session in the project's GitHub repo
  (with --prompt it starts right away; without, its terminal asks for the task).
  The row sits under the repo's main project folder. status prints its link,
  branch and latest status. send posts a follow-up message. bring-back continues
  the session on this computer in a new worktree (claude --teleport) and parks
  the cloud row. copy starts a new cloud session on another Claude account that
  carries on from the session's branch (for a session whose account is out of
  usage; `ghostex accounts` lists the ids). <session> is a title, session id or
  global ref, as in `ghostex sessions`.

  Needs Claude Code signed in with claude.ai (Pro, Max, Team or Enterprise) and
  the repo on GitHub with Claude's GitHub app. Cloud sessions use your Claude
  plan's limits.

EXAMPLES
  ghostex create-agent claude-cloud --project-id P1 --prompt \"Fix the flaky login test\"
  ghostex claude-cloud status \"Fix the flaky login test\"
  ghostex claude-cloud bring-back \"Fix the flaky login test\"
";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

fn resolve(reference: Option<&String>, flags: &Flags, verb: &str) -> CliResult<(Value, Value)> {
    let reference = reference.ok_or_else(|| {
        CliError::Other(format!("claude-cloud {verb} needs a session.\n\n{HELP}"))
    })?;
    let rows = sessions::fetch_session_list(flags, false)?;
    let session = selector::resolve_one_listed_session(reference, &rows, flags)?;
    Ok((session["projectId"].clone(), session["sessionId"].clone()))
}

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let mut flags = parsed.flags.clone();
    if flags.contains("help") || flags.contains("h") || parsed.rest.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    // A start and a teleport's worktree take a while; a follow-up up to 90 seconds.
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        flags.insert_text("timeout", "240000");
    }
    let json_output = flags.contains("json");
    let request = |params: Value| call_gxserver_rpc("/api/claudeCloud", &params, &flags);
    match parsed.rest[0].as_str() {
        "attach" => attach(&flags),
        "status" => {
            let (project_id, session_id) = resolve(parsed.rest.get(1), &flags, "status")?;
            let record = request(json!({
                "action": "get", "projectId": project_id, "sessionId": session_id,
            }))?;
            if json_output {
                print_json(&record);
            } else {
                for line in status_lines(&record) {
                    println!("{line}");
                }
            }
            Ok(())
        }
        "send" => {
            let (project_id, session_id) = resolve(parsed.rest.get(1), &flags, "send")?;
            let message = parsed.rest[2..].join(" ");
            if message.trim().is_empty() {
                return Err(CliError::Other(
                    "claude-cloud send needs a message.".to_string(),
                ));
            }
            request(json!({
                "action": "send", "projectId": project_id, "sessionId": session_id, "text": message,
            }))?;
            println!("Sent.");
            Ok(())
        }
        "bring-back" => {
            let (project_id, session_id) = resolve(parsed.rest.get(1), &flags, "bring-back")?;
            let result = request(json!({
                "action": "bringBack", "projectId": project_id, "sessionId": session_id,
            }))?;
            if json_output {
                print_json(&result);
            } else {
                println!(
                    "Continues on this computer in {} (session {}).",
                    text(&result, "worktreePath"),
                    text(&result, "sessionId")
                );
            }
            Ok(())
        }
        "copy" => {
            let (project_id, session_id) = resolve(parsed.rest.get(1), &flags, "copy")?;
            let account = flags.string_value("account").ok_or_else(|| {
                CliError::Other("claude-cloud copy needs --account <account id>.".to_string())
            })?;
            let result = request(json!({
                "action": "copy", "projectId": project_id, "sessionId": session_id,
                "accountId": account,
            }))?;
            if json_output {
                print_json(&result);
            } else {
                println!(
                    "Started a copy (session {}); it starts in Claude's cloud in the background.",
                    text(&result, "sessionId")
                );
            }
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown claude-cloud command \"{other}\".\n\n{HELP}"
        ))),
    }
}

/// What `status` and the client's screen say about a cloud session.
fn status_lines(record: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    match text(record, "state") {
        "draft" => lines.push("Claude cloud · waiting for its task".to_string()),
        "starting" => lines.push("Starting in Claude's cloud…".to_string()),
        "failed" => lines.push(format!(
            "Could not start in Claude's cloud: {}",
            text(record, "error")
        )),
        "moved" => lines.push("Moved to this computer.".to_string()),
        _ => lines.push("Running in Claude's cloud".to_string()),
    }
    for (label, key) in [
        ("Link", "url"),
        ("Branch", "branch"),
        ("Status", "statusText"),
        ("Pull request", "prUrl"),
        ("Account", "accountName"),
    ] {
        let value = text(record, key);
        if !value.is_empty() {
            lines.push(format!("{label:<13}{value}"));
        }
    }
    lines
}

/// The session this terminal belongs to, from the variables every Ghostex session exports.
fn own_session() -> CliResult<(String, String)> {
    let read = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    if let Some((project_id, session_id)) = read("GHOSTEX_GLOBAL_SESSION_REF")
        .map(|global| crate::agent_hooks::probing::parse_global_session_ref(&global))
        .and_then(|(project, session)| Some((project?, session?)))
    {
        return Ok((project_id, session_id));
    }
    read("GHOSTEX_NATIVE_SESSION_ID")
        .and_then(|value| {
            let (project, session) = value.split_once(':')?;
            Some((project.trim().to_string(), session.trim().to_string()))
        })
        .filter(|(project, session)| !project.is_empty() && !session.is_empty())
        .ok_or_else(|| {
            CliError::Other(
                "claude-cloud attach runs inside a Claude cloud session's own terminal."
                    .to_string(),
            )
        })
}

fn clear_screen() {
    print!("\x1b[2J\x1b[H");
    let _ = std::io::stdout().flush();
}

/// What the user typed, read on a thread so the screen can refresh while the prompt waits.
struct Input {
    requests: mpsc::Sender<()>,
    lines: mpsc::Receiver<Option<String>>,
    /// A read is under way: the thread is waiting for the user's next line.
    reading: bool,
}

/// One wait for a typed line.
enum Typed {
    Line(String),
    /// Nothing yet; the screen may refresh.
    Nothing,
    /// The terminal has no input left.
    Closed,
}

impl Input {
    fn new() -> Self {
        let (requests, wanted) = mpsc::channel::<()>();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            // Reads only when asked, so stdin stays free for `claude --cloud` while it runs.
            while wanted.recv().is_ok() {
                let mut line = String::new();
                let typed = match std::io::stdin().lock().read_line(&mut line) {
                    Ok(0) | Err(_) => None,
                    Ok(_) => Some(line.trim().to_string()),
                };
                if sender.send(typed).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            lines,
            reading: false,
        }
    }

    /// Waits up to `timeout` for the user's next line.
    fn line(&mut self, timeout: Duration) -> Typed {
        if !self.reading {
            let _ = std::io::stdout().flush();
            if self.requests.send(()).is_err() {
                return Typed::Closed;
            }
            self.reading = true;
        }
        match self.lines.recv_timeout(timeout) {
            Ok(Some(line)) => {
                self.reading = false;
                Typed::Line(line)
            }
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.reading = false;
                Typed::Closed
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Typed::Nothing,
        }
    }
}

/// How long the client waits for typing before it looks at the session again.
const REFRESH: Duration = Duration::from_secs(5);
/// The newest messages the screen shows; the claude.ai page has the rest.
const SHOWN_TURNS: usize = 30;

/// The conversation and what Claude needs, below the status lines of a running session.
/// `conversation` is the `/api/claudeCloud` `conversation` answer.
fn conversation_lines(conversation: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    let turns = conversation["turns"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if turns.len() > SHOWN_TURNS {
        lines.push(format!(
            "… {} earlier messages are on the session's claude.ai page.",
            turns.len() - SHOWN_TURNS
        ));
        lines.push(String::new());
    }
    for turn in &turns[turns.len().saturating_sub(SHOWN_TURNS)..] {
        lines.push(
            match text(turn, "from") {
                "user" => "You",
                "claude" => "Claude",
                _ => "Claude's cloud",
            }
            .to_string(),
        );
        for line in text(turn, "text").lines() {
            lines.push(format!("  {line}"));
        }
        lines.push(String::new());
    }
    let error = text(conversation, "error");
    if !error.is_empty() {
        lines.push(format!(
            "Ghostex could not read this session from claude.ai: {error}"
        ));
        lines.push(
            "Its replies show on its claude.ai page (click this session in the sidebar)."
                .to_string(),
        );
        lines.push("Type a follow-up and press Enter to send it.".to_string());
        return lines;
    }
    let asked = [
        text(conversation, "needsAction"),
        text(conversation, "detail"),
    ]
    .into_iter()
    .find(|line| !line.is_empty());
    match text(conversation, "status") {
        "working" => lines.push("Claude is working… its replies show here.".to_string()),
        "blocked" => {
            lines.push(match asked {
                Some(asked) => format!("Claude is waiting on claude.ai: {asked}"),
                None => "Claude is waiting on claude.ai.".to_string(),
            });
            lines.push(
                "Answer it on the session's claude.ai page (click this session in the sidebar)."
                    .to_string(),
            );
        }
        "needsInput" => {
            lines.push(match asked {
                Some(asked) => format!("Claude needs you: {asked}"),
                None => "Claude needs you.".to_string(),
            });
            lines.push("Type your answer and press Enter to send it.".to_string());
        }
        "failed" => {
            let failure = text(conversation, "failure");
            lines.push(if failure.is_empty() {
                "This session failed in Claude's cloud.".to_string()
            } else {
                format!("This session failed in Claude's cloud: {failure}")
            });
            lines.push(
                "Open its claude.ai page (click this session in the sidebar) to see why, or start a new Claude (cloud) session."
                    .to_string(),
            );
        }
        "archived" => lines.push("This session is archived on claude.ai.".to_string()),
        _ => lines.push(
            "Claude finished its turn. Type a follow-up and press Enter to send it.".to_string(),
        ),
    }
    lines
}

/// What decides a redraw of a running session's screen.
fn running_key(record: &Value) -> String {
    [
        "remoteUpdatedAt",
        "remoteStatus",
        "branch",
        "statusText",
        "url",
    ]
    .iter()
    .map(|key| text(record, key))
    .collect::<Vec<_>>()
    .join("|")
}

/// The client a Claude cloud session's terminal runs: asks for the task, starts the session, then
/// shows the session's conversation, what Claude needs and its status, and sends what the user
/// types as follow-ups.
///
/// CDXC:ClaudeCloud 2026-10-27 WHY: the client no longer runs `claude --cloud <link>` on its own
/// once the session runs. Claude Code refuses that attach on accounts where Claude has not turned it
/// on ("Attaching to an existing cloud session is not enabled for your account.", printed in red in
/// this terminal on every start), and where it works it turns the row into a plain terminal
/// session, while the desktop's view of a running cloud session is its claude.ai page in the chat
/// pane. Attaching stays where the user asks for it: the Work page's Open in terminal
/// (app/work_view/bridge.rs).
fn attach(flags: &Flags) -> CliResult<()> {
    let (project_id, session_id) = own_session()?;
    let call = |params: Value| -> CliResult<Value> {
        let mut params = params;
        params["projectId"] = json!(project_id);
        params["sessionId"] = json!(session_id);
        call_gxserver_rpc("/api/claudeCloud", &params, flags)
    };
    let mut input = Input::new();
    // The screen on show, so it is drawn again only when the session changed.
    let mut shown: Option<String> = None;
    // What became of the last line the user typed, kept on screen until the next one.
    let mut notice: Option<String> = None;
    loop {
        let record = match call(json!({ "action": "get" })) {
            Ok(record) => record,
            Err(error) => {
                println!("Ghostex is not answering ({error}). Trying again…");
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        let state = text(&record, "state").to_string();
        let key = format!(
            "{state}|{}|{}",
            text(&record, "error"),
            running_key(&record)
        );
        if shown.as_deref() != Some(key.as_str()) {
            // The session moved on since the user's last line: its outcome is on screen now.
            if shown.is_some() {
                notice = None;
            }
            let mut lines = status_lines(&record);
            lines.push(String::new());
            match state.as_str() {
                "draft" | "failed" => {
                    let previous = record["task"].as_str();
                    if state == "draft" && previous.is_some() {
                        // A draft created with its task starts on its own.
                    } else if previous.is_some() {
                        lines.push(
                            "Write the task again, or press Enter to retry the last one."
                                .to_string(),
                        );
                    } else {
                        lines.push(
                            "What should Claude do in the cloud? Write the task and press Enter."
                                .to_string(),
                        );
                    }
                }
                "starting" => {
                    lines.push("Starting in Claude's cloud… this takes up to a few minutes.".into())
                }
                "running" => {
                    lines.extend(conversation_lines(
                        &call(json!({ "action": "conversation" }))
                            .unwrap_or_else(|error| json!({ "error": error.to_string() })),
                    ));
                }
                _ => lines.push(
                    "This session continues on this computer; see its new row in the sidebar."
                        .to_string(),
                ),
            }
            clear_screen();
            for line in lines.iter().chain(notice.iter()) {
                println!("{line}");
            }
            if matches!(state.as_str(), "draft" | "failed" | "running") {
                print!("> ");
            }
            // A read may already be waiting, so nothing else flushes the prompt.
            let _ = std::io::stdout().flush();
            shown = Some(key);
        }
        match state.as_str() {
            "draft" if record["task"].as_str().is_some() && !input.reading => {
                println!("Starting in Claude's cloud… this takes up to a few minutes.");
                if let Err(error) = call(json!({ "action": "start" })) {
                    println!("{error}");
                    std::thread::sleep(Duration::from_secs(3));
                }
                shown = None;
            }
            "draft" | "failed" | "running" => match input.line(REFRESH) {
                Typed::Nothing => {}
                Typed::Closed => std::thread::sleep(Duration::from_secs(30)),
                // An empty line refreshes, or retries a failed start with its last task.
                Typed::Line(line) if line.is_empty() && state != "failed" => shown = None,
                Typed::Line(line) => {
                    let action = if state == "running" {
                        println!("Sending…");
                        json!({ "action": "send", "text": line })
                    } else if line.is_empty() {
                        println!("Starting in Claude's cloud… this takes up to a few minutes.");
                        json!({ "action": "start" })
                    } else {
                        println!("Starting in Claude's cloud… this takes up to a few minutes.");
                        json!({ "action": "start", "task": line })
                    };
                    notice = match call(action) {
                        Ok(_) if state != "running" => None,
                        // claude.ai accepts a message while a question waits on its page, but the
                        // conversation does not show it until that is answered (seen 2026-10-26).
                        Ok(_) if text(&record, "remoteStatus") == "blocked" => Some(
                            "Sent, but Claude is waiting on its claude.ai page: it may only read this once that question is answered there."
                                .to_string(),
                        ),
                        Ok(_) => Some("Sent. Claude's reply shows here.".to_string()),
                        Err(error) => Some(error.to_string()),
                    };
                    shown = None;
                }
            },
            "starting" => std::thread::sleep(Duration::from_secs(3)),
            _ => {
                if let Typed::Closed = input.line(Duration::from_secs(60)) {
                    std::thread::sleep(Duration::from_secs(60));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::conversation_lines;

    #[test]
    fn a_session_waiting_for_the_user_says_what_claude_needs() {
        let lines = conversation_lines(&json!({
            "turns": [
                { "from": "user", "text": "SPX-123" },
                { "from": "claude", "text": "I couldn't start on SPX-123.\nPaste the ticket." },
            ],
            "status": "needsInput",
            "needsAction": "paste the SPX-123 ticket title and description here",
        }));
        assert_eq!(
            lines,
            [
                "You",
                "  SPX-123",
                "",
                "Claude",
                "  I couldn't start on SPX-123.",
                "  Paste the ticket.",
                "",
                "Claude needs you: paste the SPX-123 ticket title and description here",
                "Type your answer and press Enter to send it.",
            ]
        );
    }

    #[test]
    fn an_unreadable_session_points_at_its_page() {
        let lines =
            conversation_lines(&json!({ "turns": [], "error": "claude.ai did not answer." }));
        assert_eq!(
            lines[0],
            "Ghostex could not read this session from claude.ai: claude.ai did not answer."
        );
    }
}
