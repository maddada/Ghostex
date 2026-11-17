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

/// The switch Claude Code 2.1.284 draws beside the rail ("Ultracode  on", "Tab to toggle").
const CLAUDE_ULTRACODE_SWITCH: &str = "Ultracode";
/// The effort the chat calls Ultracode.
const CLAUDE_ULTRACODE_EFFORT: &str = "ultracode";
const CLAUDE_TOGGLE_KEY: &str = "\t";
pub(super) const CLAUDE_CONFIRM_KEY: &str = "\r";
pub(super) const CLAUDE_ESCAPE: &str = "\u{1b}";

/// The slider's level labels, left to right, the one its marker stands over, and the Ultracode
/// switch when this build draws one.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ClaudeEffortSlider {
    levels: Vec<String>,
    current: usize,
    ultracode: Option<bool>,
}

/// `Ultracode  on` → `Some(true)`; the description line "Ultracode: dynamic workflows…" is not it.
fn ultracode_switch(line: &str) -> Option<bool> {
    line.match_indices(CLAUDE_ULTRACODE_SWITCH)
        .find_map(|(index, _)| {
            let rest = &line[index + CLAUDE_ULTRACODE_SWITCH.len()..];
            if !rest.starts_with(char::is_whitespace) {
                return None;
            }
            match rest.split_whitespace().next()? {
                "on" => Some(true),
                "off" => Some(false),
                _ => None,
            }
        })
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
    // CDXC:SessionChat 2026-09-29 WHY:
    // Claude Code 2.1.284 ends the label row with "Tab to toggle" for its Ultracode switch, so only
    // the leading run of level names is the rail. Requiring every word to be a level left the
    // slider unreadable: a session-only effort pick opened `/effort`, timed out, pressed Escape
    // ("Cancelled") and retried every five seconds.
    let labels_index = (0..footer)
        .rev()
        .take(6)
        .find(|&index| rail_words(&lines[index]).len() >= 2)?;
    let marker = lines[labels_index.checked_sub(1)?]
        .chars()
        .position(|ch| ch == CLAUDE_EFFORT_SLIDER_MARKER)?;
    let (levels, centres): (Vec<String>, Vec<usize>) =
        rail_words(&lines[labels_index]).into_iter().unzip();
    let ultracode = lines[labels_index.saturating_sub(1)..footer]
        .iter()
        .find_map(|line| ultracode_switch(line));
    let current = centres
        .iter()
        .enumerate()
        .min_by_key(|(_, centre)| centre.abs_diff(marker))?
        .0;
    Some(ClaudeEffortSlider {
        levels,
        current,
        ultracode,
    })
}

/// The leading level names of a label row, each with the column of its centre.
fn rail_words(line: &str) -> Vec<(String, usize)> {
    let mut words = Vec::new();
    let mut start = None;
    let chars: Vec<char> = line.chars().collect();
    for (column, ch) in chars.iter().copied().chain([' ']).enumerate() {
        match (ch.is_whitespace(), start) {
            (false, None) => start = Some(column),
            (true, Some(begin)) => {
                let word: String = chars[begin..column].iter().collect();
                if !CLAUDE_EFFORT_LEVELS.contains(&word.as_str()) {
                    break;
                }
                words.push((word, begin + (column - begin) / 2));
                start = None;
            }
            _ => {}
        }
    }
    words
}

/// CDXC:SessionChat 2026-09-26 WHY:
/// A custom status line prints "Opus 5.5" for both context sizes, so a session on `opus[1m]` never read as already on its model and every effort change walked the model list for a row Claude 2.1.283 does not have. The terminal still wins (the 2026-09-08 decision in session_chat_options/selection_sources.rs); the status line JSON Claude pipes to Ghostex only adds the context size the footer cannot print, when both name the same model (`claude_long_context_twin`, which chat's detection shares), and answers for a value the footer shows none of.
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
        .and_then(crate::session_chat_options::claude_statusline_model_choice)
        .map(|choice| choice.value);
    let model = match (terminal_model, payload_model) {
        (Some(terminal), payload) => Some(
            crate::session_chat_options::claude_long_context_twin(&terminal, payload.as_deref())
                .unwrap_or(terminal),
        ),
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

/// Claude's reply to the slider, below the `/effort` it echoed: "Set effort level to max (this
/// session only)" when the rail moved, "Ultracode on (this session only)…" or "Ultracode off.
/// Effort stays high." when only the switch did. A session-only pick must say so; Enter saves the
/// level as the default, except Max, which Claude never saves.
fn claude_effort_slider_applied(
    screen: &str,
    level: &str,
    rail_moved: bool,
    ultracode: Option<bool>,
    session_only: bool,
) -> bool {
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
    let Some(reply) = lines[command_index + 1..]
        .iter()
        .find(|line| !line.is_empty())
    else {
        return false;
    };
    let replied = if rail_moved {
        reply
            .strip_prefix(CLAUDE_EFFORT_APPLIED_PREFIX)
            .is_some_and(|rest| {
                rest.split_whitespace().next() == Some(level)
                    && (!session_only || rest.contains(CLAUDE_EFFORT_SESSION_ONLY))
            })
    } else {
        match ultracode {
            Some(true) => reply.starts_with("⎿ Ultracode on"),
            Some(false) => reply.starts_with("⎿ Ultracode off"),
            None => false,
        }
    };
    replied
        && ultracode.is_none_or(|on| {
            crate::session_chat_options::claude_ultracode_on(screen).is_none_or(|shown| shown == on)
        })
}

impl PickerDriver<'_> {
    /// CDXC:SessionChat 2026-09-29 WHY:
    /// Claude Code 2.1.284 turned Ultracode from the rail's last level into a switch beside it that
    /// keeps the level ("Ultracode on (this session only): dynamic workflows on every task. Effort
    /// stays max."), and `/effort high` leaves it on. The chat keeps Ultracode as the level after
    /// Max, so it means Max with the switch on, and every other level means the switch off. Every
    /// change that touches the switch goes through the slider, the only place that can turn it
    /// off; `confirm` is `s` for this session only or Enter to save the level as the default.
    ///
    /// Opens the slider, walks its marker to the level, sets the switch, and confirms.
    pub(super) async fn drive_claude_effort_slider(
        &self,
        effort: &str,
        confirm: &str,
    ) -> Result<(), DomainStateError> {
        self.type_claude_command("type Claude effort command", CLAUDE_EFFORT_COMMAND)
            .await?;
        self.write(CODEX_SUBMIT).await?;
        let mut slider = self
            .wait_for("open Claude effort slider", claude_effort_slider)
            .await?;
        // An older rail lists Ultracode as its own level and has no switch.
        let (level, ultracode) = match slider.ultracode {
            Some(_) if effort == CLAUDE_ULTRACODE_EFFORT => ("max", Some(true)),
            Some(_) => (effort, Some(false)),
            None => (effort, None),
        };
        let Some(target) = slider.levels.iter().position(|shown| shown == level) else {
            let _ = self.write(CLAUDE_ESCAPE).await;
            return Err(unsupported_selection(format!(
                "Claude's effort slider does not offer {effort} for this model."
            )));
        };
        let rail_moved = slider.current != target;
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
        let switched = ultracode.is_some() && slider.ultracode != ultracode;
        if switched {
            self.write(CLAUDE_TOGGLE_KEY).await?;
            self.wait_for("toggle Claude Ultracode", |screen| {
                claude_effort_slider(screen)
                    .filter(|toggled| toggled.ultracode == ultracode)
                    .map(|_| ())
            })
            .await?;
        }
        if !rail_moved && !switched {
            // Already there: close the slider rather than confirm a change that is not one.
            self.write(CLAUDE_ESCAPE).await?;
            return Ok(());
        }
        let session_only = confirm == CLAUDE_SESSION_ONLY_KEY;
        self.write(confirm).await?;
        self.await_claude_switch("applied Claude effort", |screen| {
            claude_effort_slider_applied(
                screen,
                level,
                rail_moved,
                switched.then_some(ultracode).flatten(),
                session_only && level != "max",
            )
        })
        .await
        .map(|_| ())
    }
}
