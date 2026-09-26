//! Claude's bare `/effort` slider: the session-only way to change effort without touching the model.

use super::*;

/// CDXC:SessionChat 2026-09-26 WHY:
/// Verified live on Claude Code 2.1.283 (Windows): bare `/effort` opens a slider whose footer reads "←/→ to adjust · Enter to confirm · s for this session only · Esc to cancel", and `s` answered "Set effort level to xhigh (this session only)" with `~/.claude/settings.json` byte-identical afterwards. The `/model` list can only set effort on the row it applies, and 2.1.283 lists no row for a session started on `opus[1m]`, so every effort change from chat on such a session failed and snapped back.
pub(super) const CLAUDE_EFFORT_COMMAND: &str = "/effort";
const CLAUDE_EFFORT_SLIDER_FOOTER: &str = "s for this session only";
const CLAUDE_EFFORT_SLIDER_MARKER: char = '\u{25b2}';
const CLAUDE_EFFORT_APPLIED_PREFIX: &str = "⎿ Set effort level to ";
const CLAUDE_EFFORT_SESSION_ONLY: &str = "(this session only)";
const CLAUDE_ARROW_LEFT: &str = "\u{1b}[D";
const CLAUDE_EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultracode"];

/// The slider's level labels, left to right, and the one its marker stands over.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ClaudeEffortSlider {
    levels: Vec<String>,
    current: usize,
}

/// Only the footer at the tail proves the slider is open now; history keeps an old one visible.
pub(super) fn claude_effort_slider_open(screen: &str) -> bool {
    screen_lines(screen)
        .iter()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(CLAUDE_PICKER_TAIL_LINES)
        .any(|line| line.contains(CLAUDE_EFFORT_SLIDER_FOOTER) && line.contains("Enter to confirm"))
}

/// Reads the slider off its columns: the marker row sits right above the level labels, and the
/// level whose label centre is nearest the marker is the one Enter or `s` would apply.
pub(super) fn claude_effort_slider(screen: &str) -> Option<ClaudeEffortSlider> {
    if !claude_effort_slider_open(screen) {
        return None;
    }
    let lines = screen_lines_spaced(screen);
    let footer = lines
        .iter()
        .rposition(|line| line.contains(CLAUDE_EFFORT_SLIDER_FOOTER))?;
    let labels_index = (0..footer).rev().take(6).find(|&index| {
        let words: Vec<&str> = lines[index].split_whitespace().collect();
        words.len() >= 2 && words.iter().all(|word| CLAUDE_EFFORT_LEVELS.contains(word))
    })?;
    let marker = lines[labels_index.checked_sub(1)?]
        .chars()
        .position(|ch| ch == CLAUDE_EFFORT_SLIDER_MARKER)?;
    let mut levels = Vec::new();
    let mut centres = Vec::new();
    let mut start = None;
    for (column, ch) in lines[labels_index].chars().chain([' ']).enumerate() {
        match (ch.is_whitespace(), start) {
            (false, None) => start = Some(column),
            (true, Some(begin)) => {
                levels.push(
                    lines[labels_index]
                        .chars()
                        .skip(begin)
                        .take(column - begin)
                        .collect::<String>(),
                );
                centres.push(begin + (column - begin) / 2);
                start = None;
            }
            _ => {}
        }
    }
    let current = centres
        .iter()
        .enumerate()
        .min_by_key(|(_, centre)| centre.abs_diff(marker))?
        .0;
    Some(ClaudeEffortSlider { levels, current })
}

/// CDXC:SessionChat 2026-09-26 WHY:
/// A custom status line prints "Opus 5.5" for both context sizes, so a session on `opus[1m]` never read as already on its model and every effort change walked the model list for a row Claude 2.1.283 does not have. The terminal still wins (the 2026-09-08 decision in session_chat_options.rs); the status line JSON Claude pipes to Ghostex only adds the context size the footer cannot print, when both name the same model, and answers for a value the footer shows none of.
pub(super) fn claude_live_selection(
    plan: &CodexPickerPlan,
    screen: &str,
) -> (Option<String>, Option<String>) {
    let payload = plan
        .claude_statusline
        .as_ref()
        .and_then(|(directory, session_id)| {
            crate::agent_hooks::statusline::read_claude_statusline_payload(directory, session_id)
        })
        .map(|stored| stored.payload);
    let terminal = detect_session_chat_selection(SessionChatOptionAgent::Claude, screen);
    let terminal_model = terminal
        .as_ref()
        .and_then(|selection| selection.model.as_ref())
        .map(|choice| choice.value.clone());
    let payload_model = payload
        .as_ref()
        .and_then(|payload| payload.get("model").and_then(|model| model.get("id")))
        .and_then(Value::as_str)
        .and_then(crate::session_chat_options::claude_transcript_model_choice)
        .map(|choice| choice.value);
    let model = match (terminal_model, payload_model) {
        (Some(terminal), Some(payload))
            if !terminal.contains('[')
                && payload.split_once('[').map(|(base, _)| base) == Some(terminal.as_str()) =>
        {
            Some(payload)
        }
        (Some(terminal), _) => Some(terminal),
        (None, payload) => payload,
    };
    let effort = terminal
        .as_ref()
        .and_then(|selection| selection.effort.as_ref())
        .map(|choice| choice.value.clone())
        .or_else(|| {
            payload
                .as_ref()
                .and_then(|payload| payload.get("effort").and_then(|effort| effort.get("level")))
                .and_then(Value::as_str)
                .map(|level| level.trim().to_ascii_lowercase())
                .filter(|level| CLAUDE_EFFORT_LEVELS.contains(&level.as_str()))
        });
    (model, effort)
}

/// Claude's acknowledgement of a session-only effort, below the `/effort` it echoed.
fn claude_effort_session_only_applied(screen: &str, effort: &str) -> bool {
    if crate::session_chat_composer::detect_session_chat_composer_readiness(
        Some("claude"),
        screen,
        None,
    )
    .state
        != crate::session_chat_composer::SessionChatComposerState::Ready
    {
        return false;
    }
    let lines = screen_lines(screen);
    let command = format!("❯ {CLAUDE_EFFORT_COMMAND}");
    let Some(command_index) = lines.iter().rposition(|line| line == &command) else {
        return false;
    };
    lines[command_index + 1..]
        .iter()
        .find(|line| !line.is_empty())
        .and_then(|reply| reply.strip_prefix(CLAUDE_EFFORT_APPLIED_PREFIX))
        .is_some_and(|rest| {
            rest.split_whitespace().next() == Some(effort)
                && rest.contains(CLAUDE_EFFORT_SESSION_ONLY)
        })
}

impl PickerDriver<'_> {
    /// Opens the slider, walks its marker to `effort` one level at a time, and answers `s`.
    pub(super) async fn drive_claude_effort_session_only(
        &self,
        effort: &str,
    ) -> Result<(), DomainStateError> {
        self.write(&crate::session_chat_send::build_session_chat_paste_bytes(
            CLAUDE_EFFORT_COMMAND,
        ))
        .await?;
        self.wait_for("type Claude effort command", |screen| {
            crate::session_chat_composer::claude_composer_input_text(screen)
                .is_some_and(|text| text.trim() == CLAUDE_EFFORT_COMMAND)
                .then_some(())
        })
        .await?;
        self.write(CODEX_SUBMIT).await?;
        let mut slider = self
            .wait_for("open Claude effort slider", claude_effort_slider)
            .await?;
        let Some(target) = slider.levels.iter().position(|level| level == effort) else {
            return Err(unsupported_selection(format!(
                "Claude's effort slider does not offer {effort} for this model."
            )));
        };
        for _ in 0..CLAUDE_EFFORT_STEP_LIMIT {
            if (self.cancelled)() {
                return Err(agent_busy(
                    "The effort change was cancelled by another action on this session.",
                ));
            }
            if slider.current == target {
                break;
            }
            let from = slider.current;
            self.write(if target > from {
                CLAUDE_ARROW_RIGHT
            } else {
                CLAUDE_ARROW_LEFT
            })
            .await?;
            slider = self
                .wait_for("move Claude effort slider", |screen| {
                    claude_effort_slider(screen).filter(|moved| moved.current != from)
                })
                .await?;
        }
        if slider.current != target {
            return Err(dialog_mismatch(
                "choose Claude effort",
                "Claude's effort slider did not reach the requested level.",
            ));
        }
        self.write(CLAUDE_SESSION_ONLY_KEY).await?;
        self.wait_for("applied Claude session effort", |screen| {
            claude_effort_session_only_applied(screen, effort).then_some(())
        })
        .await
    }
}
