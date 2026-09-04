//! What happens after the picker returns: resume, copy, view, or fork.
//! Port of the action half of zehn's `src/main.zig` plus `src/fork.zig`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::agent::Agent;
use crate::scan::Record;

/// Clipboard copy commands to try in order. Different platforms (and Linux
/// display servers) ship different tools, so each is attempted until one works.
/// The prompt text is fed on stdin, never as an argv element, so even huge or
/// shell-special prompts are safe.
pub fn clipboard_candidates() -> Vec<Vec<&'static str>> {
    if cfg!(target_os = "macos") {
        vec![vec!["pbcopy"]]
    } else {
        vec![
            vec!["wl-copy"],                          // wayland
            vec!["xclip", "-selection", "clipboard"], // x11
            vec!["xsel", "--clipboard", "--input"],   // x11 alt
        ]
    }
}

/// Copy `text` to the system clipboard. Returns the tool that succeeded.
pub fn copy_to_clipboard(text: &str) -> Result<String, String> {
    for argv in clipboard_candidates() {
        let Ok(mut child) = Command::new(argv[0])
            .args(&argv[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        match child.wait() {
            Ok(status) if status.success() => return Ok(argv[0].to_string()),
            _ => continue,
        }
    }
    Err("no clipboard tool found (need pbcopy, wl-copy, xclip, or xsel)".to_string())
}

/// Write the prompt to a temp file and open it in `$EDITOR`.
pub fn view_prompt(rec: &Record) -> Result<(), String> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "nvim".to_string());
    let tmp = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
    let path = Path::new(&tmp).join(format!("zehn-prompt-{}.md", std::process::id()));
    std::fs::write(&path, &rec.text).map_err(|e| format!("failed to write preview file ({e})"))?;
    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|e| format!("failed to launch editor `{editor}` ({e}); set $EDITOR if needed"));
    let _ = std::fs::remove_file(&path);
    status.map(|_| ())
}

/// A project directory that still exists, so a spawn can start there.
fn usable_project(project: &str) -> Option<&Path> {
    if project.is_empty() {
        return None;
    }
    let path = Path::new(project);
    if path.is_dir() {
        Some(path)
    } else {
        None
    }
}

/// Fork the prompt into a fresh session in `agent` (possibly different from the
/// one it came from), starting in the recorded project dir when it still exists.
pub fn fork_session(rec: &Record, agent: Agent, accept_all: bool) -> Result<(), String> {
    let argv = agent.fresh_session_argv(&rec.text, accept_all);
    let project = usable_project(&rec.project);
    let mut note = format!(
        "\x1b[90m→ forking prompt into a new {} session",
        agent.label()
    );
    if let Some(path) = project {
        note.push_str(&format!(" in {}", path.display()));
    }
    note.push_str("\x1b[0m");
    println!("{note}");

    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    if let Some(path) = project {
        cmd.current_dir(path);
    }
    cmd.status()
        .map(|_| ())
        .map_err(|e| format!("failed to launch {} ({e})", argv[0]))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GhostexFocusOutcome {
    NotConfigured,
    Focused,
    NotRunning,
    Failed,
}

pub fn ghostex_focus_outcome_for_exit_code(code: i32) -> GhostexFocusOutcome {
    match code {
        0 => GhostexFocusOutcome::Focused,
        3 => GhostexFocusOutcome::NotRunning,
        _ => GhostexFocusOutcome::Failed,
    }
}

// CDXC:PromptSearch 2026-08-07-09:18:
// Zehn owns agent-history selection while Ghostex owns live workspace identity.
// `ghostex find` provides its exact CLI executable so selecting history that
// already has a live Ghostex owner focuses that pane instead of asking the agent
// to open a forbidden second writer. Resume normally only when Ghostex
// explicitly reports that no live owner exists; a focus/control-plane failure
// must not launch a duplicate agent process.
pub fn focus_live_ghostex_session(rec: &Record) -> GhostexFocusOutcome {
    let Ok(cli) = std::env::var("GHOSTEX_CLI_EXECUTABLE") else {
        return GhostexFocusOutcome::NotConfigured;
    };
    if cli.is_empty() {
        return GhostexFocusOutcome::NotConfigured;
    }
    match Command::new(&cli)
        .args([
            "focus",
            "--agent-session-id",
            &rec.session,
            "--agent",
            rec.agent.label(),
            "--if-running",
        ])
        .status()
    {
        Ok(status) => match status.code() {
            Some(code) => ghostex_focus_outcome_for_exit_code(code),
            None => GhostexFocusOutcome::Failed,
        },
        Err(_) => GhostexFocusOutcome::Failed,
    }
}

/// Resume the recorded session in its owning agent, from its project directory
/// when that still exists.
pub fn resume_session(rec: &Record, accept_all: bool) -> Result<(), String> {
    if rec.session.is_empty() {
        return Err(format!(
            "no session id recorded for this {} entry",
            rec.agent.label()
        ));
    }

    match focus_live_ghostex_session(rec) {
        GhostexFocusOutcome::NotConfigured | GhostexFocusOutcome::NotRunning => {}
        GhostexFocusOutcome::Focused => return Ok(()),
        GhostexFocusOutcome::Failed => {
            return Err(
                "Ghostex could not focus the live agent session; not starting a duplicate writer."
                    .to_string(),
            )
        }
    }

    let argv = rec.agent.resume_argv(&rec.session, accept_all);
    // Resume from the recorded project dir, but fall back to the current dir if
    // it no longer exists (moved/deleted), rather than failing the spawn.
    let project = usable_project(&rec.project);

    let mut note = format!(
        "\x1b[90m→ resuming {} session {}",
        rec.agent.label(),
        rec.session
    );
    if let Some(path) = project {
        note.push_str(&format!(" in {}", path.display()));
    } else if !rec.project.is_empty() {
        note.push_str(&format!(
            " (project {} missing — using current dir)",
            rec.project
        ));
    }
    note.push_str("\x1b[0m");
    println!("{note}");

    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    if let Some(path) = project {
        cmd.current_dir(path);
    }
    match cmd.status() {
        Ok(_) => Ok(()),
        Err(e) => {
            let manual = argv.join(" ");
            let dir = if rec.project.is_empty() {
                "."
            } else {
                &rec.project
            };
            Err(format!(
                "failed to launch {} ({e})\nRun manually:\n  cd {dir} && {manual}",
                argv[0]
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_candidates_are_platform_shaped() {
        let c = clipboard_candidates();
        assert!(!c.is_empty());
        assert!(c.iter().all(|cmd| !cmd.is_empty()));
        if cfg!(target_os = "macos") {
            assert_eq!(c[0][0], "pbcopy");
        } else {
            assert_eq!(c[0][0], "wl-copy");
        }
    }

    #[test]
    fn focus_exit_codes_map_to_outcomes() {
        assert_eq!(
            ghostex_focus_outcome_for_exit_code(0),
            GhostexFocusOutcome::Focused
        );
        assert_eq!(
            ghostex_focus_outcome_for_exit_code(3),
            GhostexFocusOutcome::NotRunning
        );
        assert_eq!(
            ghostex_focus_outcome_for_exit_code(1),
            GhostexFocusOutcome::Failed
        );
    }
}
