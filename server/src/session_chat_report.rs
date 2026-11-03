//! `/api/readSessionChatReport`: the chat's Copy Report, a Markdown report of what Ghostex sees in
//! one session's terminal, for the user to paste into Send Bug / Feedback or a GitHub issue.

use serde_json::{json, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

mod mask;

use mask::{report_screen, ReportMask};

/// Send problems the report lists, newest last.
const RECENT_SEND_PROBLEMS: usize = 8;
/// A logged message longer than this is cut, so one entry cannot crowd out the screen.
const PROBLEM_MESSAGE_MAX_CHARS: usize = 240;

/// CDXC:SessionChat 2026-10-25 DECISION:
/// User: "i want a way to report such issues where it goes and captures the terminal for me into clipboard when the chat composer shows red so ppl can share it… should copy full screen of terminal with 200 width basically as seen by ghostex", with "useful info" about the current state. The report is built here, on the machine that runs the session, so every chat (desktop, web, phone, a remote session) copies the same text: the session, the input-box verdict and the notice the chat acts on, the terminal's size and clients, the queue, the session's recent refused or held sends, and the whole live grid as gxserver captures it. Since 2026-10-29 it names nothing that identifies the user, the machine or the work (CDXC:SessionChat 2026-10-29 in session_chat_report/mask.rs).
/// SEE-ALSO: packages/gx-chat-core/src/extras/report.rs (Copy Report: copy, toast, then Send Bug / Feedback), server/src/session_chat_composer.rs (`read_session_terminal_tail`, the same verdict for the not-ready card).
pub(crate) fn read_session_chat_report(
    state: &AppState,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!("Session {session_id} does not exist."))
        })?;
    let zmx_name = crate::zmx::provider_zmx_session_name(&session)?;
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(&session);
    let global_ref = format!("{}:{project_id}:{session_id}", state.metadata.server_id);
    let mask = report_mask(
        repository,
        &session,
        &global_ref,
        &zmx_name,
        project_id,
        session_id,
    );
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z");
    let grid = crate::zmx::read_zmx_grid(&zmx_name)
        .and_then(|grid| serde_json::from_str::<Value>(&grid).ok());
    // The live grid alone, the screen the user sees, without scrollback.
    let capture = crate::zmx::read_zmx_session_grid_capture(&zmx_name);
    /*
    CDXC:SessionChat 2026-10-27 WHY:
    The verdict was judged on the live grid while the Terminal View glyph, the not-ready card and the send gate judge the screen with its scrollback tail, so a user who saw the glyph red, clicked Copy Report and pasted it got "Input box: on screen and ready" under a red light (session G14iv, 2026-10-27). The report now states the verdict the chat acts on, from the same capture; the screen it prints stays the live grid.
    */
    let verdict = crate::session_chat_composer::read_session_composer_verdict_capture(
        &zmx_name,
        agent.as_deref(),
    );
    let sees = match (verdict.as_ref(), capture.as_ref()) {
        (Some(verdict), _) => verdict_lines(agent.as_deref(), &verdict.text),
        (None, Err(error)) => format!("- Ghostex could not read the terminal: {error}\n"),
        (None, Ok(_)) => {
            "- Input box: unreadable (the terminal capture lost its tail)\n".to_string()
        }
    };
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot(
        &state.paths,
        project_id,
        session_id,
    );
    let report = render_report(
        &ReportInput {
            header: format!(
                "Ghostex {} \u{b7} {} \u{b7} copied {now}",
                crate::telemetry::base::SERVER_MARKETING_VERSION,
                os_line(),
            ),
            agent: agent.unwrap_or_else(|| "unknown".to_string()),
            state: format!(
                "{} \u{b7} {}",
                crate::presentation::effective_lifecycle_state(&session),
                crate::presentation::presentation_activity(
                    &session,
                    &crate::presentation::now_iso()
                ),
            ),
            grid,
            sees,
            queue: queue_line(&queue.queue),
            problems: recent_send_problems(&state.paths.logs_dir, project_id, session_id),
            screen: capture.ok().map(|capture| screen_rows(&capture.text)),
        },
        &mask,
    );
    Ok(json!({ "report": report, "projectId": project_id, "sessionId": session_id }))
}

/// What the report is made of, read off the session, its terminal and gxserver's logs, before the
/// mask (`render_report`).
struct ReportInput {
    header: String,
    agent: String,
    state: String,
    grid: Option<Value>,
    /// The input-box and notice lines (`verdict_lines`), or why the screen could not be read.
    sees: String,
    queue: String,
    problems: Vec<String>,
    /// The live grid's rows (`screen_rows`), when it could be read.
    screen: Option<String>,
}

/// The report's Markdown, every line that could carry the user's text masked.
fn render_report(input: &ReportInput, mask: &ReportMask) -> String {
    let mut report = String::new();
    report.push_str("## Ghostex session report\n\n");
    report.push_str(&format!("{}\n\n", input.header));
    report.push_str("### Session\n\n");
    report.push_str(&format!("- Agent: {}\n", mask.apply(&input.agent)));
    report.push_str(&format!("- State: {}\n", input.state));
    report.push_str(&format!(
        "- Terminal: {}\n",
        grid_line(input.grid.as_ref()).trim_start_matches(" \u{b7} ")
    ));
    report.push_str("\n### What Ghostex sees\n\n");
    report.push_str(&mask.apply(&input.sees));
    report.push_str(&format!("- Queue: {}\n", mask.apply(&input.queue)));
    report.push_str("\n### Recent send problems\n\n");
    if input.problems.is_empty() {
        report.push_str("None logged.\n");
    } else {
        for problem in &input.problems {
            report.push_str(&format!("- {}\n", mask.apply(problem)));
        }
    }
    if let Some(screen) = input.screen.as_deref() {
        let screen = report_screen(mask, screen);
        let fence = code_fence(&screen);
        report.push_str(&format!(
            "\n### Terminal screen{}\n\n{fence}text\n{screen}\n{fence}\n",
            input
                .grid
                .as_ref()
                .and_then(grid_size)
                .map(|(rows, cols)| format!(" ({rows} \u{d7} {cols}, as Ghostex reads it)"))
                .unwrap_or_default(),
        ));
    }
    report
}

/// What the report hides: the session's ref, ids, terminal name and title, its project's name and
/// folder, the agent's conversation file and account, and the user's home folder and name (the mask
/// also catches any other ref, id, UUID, email, URL or path by its shape).
fn report_mask(
    repository: &DomainRepository<'_>,
    session: &Value,
    global_ref: &str,
    zmx_name: &str,
    project_id: &str,
    session_id: &str,
) -> ReportMask {
    let text = |value: &Value, pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default()
    };
    let project = repository
        .get_project(project_id)
        .ok()
        .flatten()
        .unwrap_or(Value::Null);
    let home = crate::resume_lookup::home_dir();
    let user = home
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut values = vec![
        (global_ref.to_string(), "<session ref>"),
        (zmx_name.to_string(), "<terminal>"),
        (text(session, "/sessionPersistenceName"), "<terminal>"),
        (
            text(session, "/runtimeSettings/sessionPersistenceName"),
            "<terminal>",
        ),
        (project_id.to_string(), "<id>"),
        (session_id.to_string(), "<id>"),
        (text(session, "/runtimeSettings/agentSessionId"), "<uuid>"),
        (text(session, "/runtimeSettings/agentSessionPath"), "<path>"),
        (text(session, "/runtimeSettings/accountName"), "<account>"),
        (text(session, "/runtimeSettings/accountEmail"), "<email>"),
        (text(session, "/cwd"), "<path>"),
        (text(&project, "/path"), "<path>"),
        (text(&project, "/name"), "<project>"),
        (text(&project, "/title"), "<project>"),
        (home.to_string_lossy().to_string(), "<path>"),
        (user, "<user>"),
        (std::env::var("USER").unwrap_or_default(), "<user>"),
    ];
    for key in ["/title", "/displayTitle", "/primaryTitle", "/terminalTitle"] {
        values.push((text(session, key), "<session name>"));
    }
    ReportMask::new(values)
}

/// The input-box and notice lines, as the Terminal View glyph and the send gate judge `screen`.
fn verdict_lines(agent: Option<&str>, screen: &str) -> String {
    use crate::session_chat_composer::SessionChatComposerState;
    let readiness = crate::session_chat_composer::session_composer_verdict(agent, screen);
    let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(agent, screen);
    let mut lines = format!(
        "- Input box: {}\n",
        match readiness.state {
            SessionChatComposerState::Ready => "on screen and ready".to_string(),
            SessionChatComposerState::NotReady => format!(
                "not ready: {}",
                readiness.reason.as_deref().unwrap_or("no reason given")
            ),
            SessionChatComposerState::Unknown => {
                "unknown (Ghostex has no input-box signature for this agent)".to_string()
            }
        }
    );
    if readiness.should_dismiss() {
        lines.push_str("- A send closes the panel over the input box first.\n");
    }
    lines.push_str(&format!(
        "- Terminal notice: {}\n",
        notice
            .map(|notice| format!("{} ({})", notice.title, notice.kind))
            .unwrap_or_else(|| "none".to_string())
    ));
    lines
}

/// `macOS (aarch64)`: the platform without spawning a process for its version.
fn os_line() -> String {
    let name = match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    format!("{name} ({})", std::env::consts::ARCH)
}

fn grid_size(grid: &Value) -> Option<(u64, u64)> {
    Some((grid.get("rows")?.as_u64()?, grid.get("cols")?.as_u64()?))
}

/// ` · 24 × 200 · clients: visible 24 × 200, parked 41 × 82`.
fn grid_line(grid: Option<&Value>) -> String {
    let Some(grid) = grid else {
        return " · its size could not be read".to_string();
    };
    let mut line = grid_size(grid)
        .map(|(rows, cols)| format!(" · {rows} × {cols}"))
        .unwrap_or_default();
    let clients: Vec<String> = grid
        .get("clients")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|client| {
            let state = client
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            match client.get("last_size").and_then(grid_size) {
                Some((rows, cols)) => format!("{state} {rows} × {cols}"),
                None => state.to_string(),
            }
        })
        .collect();
    line.push_str(&if clients.is_empty() {
        " · no clients attached".to_string()
    } else {
        format!(" · clients: {}", clients.join(", "))
    });
    line
}

/// `3 waiting (2 held for the input box), 1 failed: <reason>`; message text stays out.
fn queue_line(queue: &[crate::session_chat_queue::SessionChatQueuedPrompt]) -> String {
    if queue.is_empty() {
        return "empty".to_string();
    }
    let waiting = queue.iter().filter(|row| row.state != "failed").count();
    let held = queue
        .iter()
        .filter(|row| row.state != "failed" && row.startup_send)
        .count();
    let mut parts = vec![if held > 0 {
        format!("{waiting} waiting ({held} held for the input box)")
    } else {
        format!("{waiting} waiting")
    }];
    let oldest = queue.iter().map(|row| row.created_at.as_str()).min();
    if let Some(oldest) = oldest {
        parts.push(format!("oldest from {oldest}"));
    }
    for failed in queue.iter().filter(|row| row.state == "failed") {
        parts.push(format!(
            "1 failed: {}",
            failed.error_message.as_deref().unwrap_or("no reason given")
        ));
    }
    parts.join(", ")
}

/// This session's newest entries in gxserver's send log (`session-chat-send-failures.jsonl`),
/// one line each, without the screens they carry.
fn recent_send_problems(
    logs_dir: &std::path::Path,
    project_id: &str,
    session_id: &str,
) -> Vec<String> {
    let Ok(log) = std::fs::read_to_string(logs_dir.join("session-chat-send-failures.jsonl")) else {
        return Vec::new();
    };
    let mut problems: Vec<String> = log
        .lines()
        .filter(|line| line.contains(session_id))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|entry| {
            entry.get("sessionId").and_then(Value::as_str) == Some(session_id)
                && entry.get("projectId").and_then(Value::as_str) == Some(project_id)
        })
        .map(|entry| {
            let text = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or_default();
            // A failed model change keeps its reason under `selection` (GitHub #400's report
            // listed `sessionChatModelSelectionFailed` with no reason).
            let selection_message = entry
                .pointer("/selection/message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let message = [text("message"), text("reason"), selection_message]
                .into_iter()
                .find(|message| !message.is_empty())
                .unwrap_or_default();
            let message: String = message.chars().take(PROBLEM_MESSAGE_MAX_CHARS).collect();
            let code = text("code");
            format!(
                "{} {}{}{}",
                text("ts"),
                text("event"),
                if code.is_empty() {
                    String::new()
                } else {
                    format!(" ({code})")
                },
                if message.is_empty() {
                    String::new()
                } else {
                    format!(": {message}")
                },
            )
        })
        .collect();
    let keep = problems.len().saturating_sub(RECENT_SEND_PROBLEMS);
    problems.drain(..keep);
    problems
}

/// The grid's rows with trailing spaces and trailing blank rows removed, and control characters
/// other than tabs dropped.
fn screen_rows(text: &str) -> String {
    let rows: Vec<String> = text
        .lines()
        .map(|row| {
            row.chars()
                .filter(|ch| !ch.is_control() || *ch == '\t')
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();
    let end = rows
        .iter()
        .rposition(|row| !row.is_empty())
        .map_or(0, |last| last + 1);
    rows[..end].join("\n")
}

/// A fence longer than any backtick run on the screen, so the screen cannot close it.
fn code_fence(screen: &str) -> String {
    let longest = screen
        .split(|ch| ch != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

#[cfg(test)]
mod tests {
    use super::verdict_lines;
    use super::{mask::ReportMask, render_report, ReportInput};
    use serde_json::json;

    /// Every section of a report carrying the session's ref, its name, a path, an email and a UUID
    /// comes out with none of them, and keeps what helps debug.
    #[test]
    fn a_copied_report_names_nobody() {
        let polluted = "S90:P3lv0:G1pif \"Main Coordinator\" /Users/madda/dev/_active/Ghostex \
                        me@example.com ff8d2575-b069-47b5-b375-69750cc8353f";
        let mask = ReportMask::new([
            ("S90:P3lv0:G1pif".to_string(), "<session ref>"),
            ("S90-P3lv0-G1pif".to_string(), "<terminal>"),
            ("Main Coordinator".to_string(), "<session name>"),
            ("Ghostex".to_string(), "<project>"),
            ("/Users/madda/dev/_active/Ghostex".to_string(), "<path>"),
            ("/Users/madda".to_string(), "<path>"),
            ("madda".to_string(), "<user>"),
        ]);
        let report = render_report(
            &ReportInput {
                header: "Ghostex 10.39.1 \u{b7} macOS (aarch64) \u{b7} copied 2026-10-29 02:34:00 +04:00"
                    .to_string(),
                agent: "claude".to_string(),
                state: "running \u{b7} idle".to_string(),
                grid: Some(json!({
                    "rows": 42, "cols": 200,
                    "clients": [
                        {"state": "visible", "last_size": {"rows": 42, "cols": 200}},
                        {"state": "chat", "last_size": {"rows": 24, "cols": 200}},
                    ],
                })),
                sees: format!(
                    "- Input box: not ready: {polluted}\n- Terminal notice: {polluted}\n"
                ),
                queue: format!("1 waiting, 1 failed: {polluted}"),
                problems: vec![format!(
                    "2026-10-28T11:09:27Z sessionChatSendLatePasteSubmitted: {polluted}"
                )],
                screen: Some(format!(
                    "\u{2500}\u{2500}\u{2500} Main Coordinator \u{2500}\n\u{276f} {polluted}\n  ff8d2575-b069-47b5-b375-69750cc8353f | \u{2026}/Ghostex"
                )),
            },
            &mask,
        );
        for gone in [
            "S90",
            "P3lv0",
            "G1pif",
            "Main Coordinator",
            "Ghostex/",
            "/Users",
            "madda",
            "me@example.com",
            "ff8d2575",
        ] {
            assert!(!report.contains(gone), "{gone} survived:\n{report}");
        }
        for kept in [
            "Ghostex 10.39.1 \u{b7} macOS (aarch64)",
            "- Agent: claude",
            "- State: running \u{b7} idle",
            "- Terminal: 42 \u{d7} 200 \u{b7} clients: visible 42 \u{d7} 200, chat 24 \u{d7} 200",
            "- Input box: not ready: <session ref> \"<session name>\" <path> <email> <uuid>",
            "- Queue: 1 waiting, 1 failed: <session ref>",
            "2026-10-28T11:09:27Z sessionChatSendLatePasteSubmitted: <session ref>",
            "### Terminal screen (42 \u{d7} 200, as Ghostex reads it)",
            "\u{2500}\u{2500}\u{2500} <session name> \u{2500}",
            "  <uuid> | <path>",
        ] {
            assert!(report.contains(kept), "{kept} missing:\n{report}");
        }
    }

    #[test]
    fn a_failed_model_change_is_listed_with_its_reason() {
        let dir = std::env::temp_dir().join(format!(
            "ghostex-report-model-failure-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let line = json!({
            "ts": "2026-11-03T05:51:23Z",
            "event": "sessionChatModelSelectionFailed",
            "projectId": "P1abc",
            "sessionId": "G1abc",
            "selection": {"code": "internalError", "message": "The claude input box is not on screen yet."},
        });
        std::fs::write(
            dir.join("session-chat-send-failures.jsonl"),
            format!("{line}\n"),
        )
        .unwrap();
        let problems = super::recent_send_problems(&dir, "P1abc", "G1abc");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            problems,
            vec!["2026-11-03T05:51:23Z sessionChatModelSelectionFailed: The claude input box is not on screen yet."]
        );
    }

    /// Session G14iv's screen when its report was copied on 2026-10-27 (200 columns, trimmed).
    fn idle_screen() -> String {
        [
            "✻ Churned for 6s · done 8:33 AM".to_string(),
            format!("{:>100}", "◉ xhigh · /effort"),
            format!("{} Mr Big Chungus ─", "─".repeat(100)),
            "❯ ".to_string(),
            "─".repeat(120),
            "  Opus 5.5 (1M) | xhigh | Ctx 57% | $757.85".to_string(),
            "  ff8d2575-b069-47b5-b375-69750cc8353f | …/Ghostex".to_string(),
            "  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents · 2 feedback drafts"
                .to_string(),
        ]
        .join("\n")
    }

    #[test]
    fn the_report_states_the_verdict_the_chat_acts_on() {
        let screen = idle_screen();
        let readiness =
            crate::session_chat_composer::session_composer_verdict(Some("claude"), &screen);
        assert_eq!(readiness.state.as_str(), "ready");
        assert_eq!(
            verdict_lines(Some("claude"), &screen),
            "- Input box: on screen and ready\n- Terminal notice: none\n"
        );
        // Mid-repaint the input box is gone and only the transcript is left.
        let transcript = screen.lines().take(2).collect::<Vec<_>>().join("\n");
        assert_eq!(
            verdict_lines(Some("claude"), &transcript),
            "- Input box: not ready: The claude input box is not on screen yet.\n- Terminal notice: none\n"
        );
    }
}
