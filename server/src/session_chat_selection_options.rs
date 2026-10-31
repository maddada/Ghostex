//! Desired option delivery inside the session's serialized terminal worker.

use super::*;
use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, SessionChatComposerState,
};
use crate::session_chat_send::{build_session_chat_paste_bytes, SESSION_CHAT_SHIFT_TAB};

fn agent_name(agent: SessionChatOptionAgent) -> &'static str {
    match agent {
        SessionChatOptionAgent::Claude => "claude",
        SessionChatOptionAgent::Antigravity => "antigravity",
        _ => "codex",
    }
}

pub(super) fn ready(screen: &str, agent: SessionChatOptionAgent) -> bool {
    detect_session_chat_composer_readiness(Some(agent_name(agent)), screen, None).state
        == SessionChatComposerState::Ready
}

fn current_mode(screen: &str, agent: SessionChatOptionAgent) -> Option<String> {
    if !ready(screen, agent) {
        return None;
    }
    let selection = detect_session_chat_selection(agent, screen)?;
    if let Some(mode) = selection.mode {
        return Some(mode.value);
    }
    (agent == SessionChatOptionAgent::Codex && selection.model.is_some()).then(|| "default".into())
}

fn current_fast(
    screen: &str,
    agent: SessionChatOptionAgent,
    plan: &CodexPickerPlan,
) -> Option<bool> {
    if !ready(screen, agent) {
        return None;
    }
    if agent == SessionChatOptionAgent::Claude {
        let (directory, session_id) = plan.claude_statusline.as_ref()?;
        return crate::agent_hooks::statusline::read_claude_statusline_payload(
            directory, session_id,
        )?
        .payload
        .get("fast_mode")
        .and_then(Value::as_bool);
    }
    let selection = detect_session_chat_selection(agent, screen)?;
    selection.model.as_ref()?;
    Some(selection.fast.unwrap_or(false))
}

/// Claude's `/fast` panel ("↯ Fast mode (research preview)" … "Esc to cancel"), open at the tail.
fn claude_fast_panel_open(screen: &str) -> bool {
    let lines = screen_lines(screen);
    let tail: Vec<&String> = lines
        .iter()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(8)
        .collect();
    tail.first()
        .is_some_and(|line| line.trim() == "Esc to cancel")
        && tail
            .iter()
            .any(|line| line.trim_start().starts_with("↯ Fast mode"))
}

/// Claude's reason, when the panel says this account cannot turn Fast mode on.
fn claude_fast_unavailable(screen: &str) -> Option<String> {
    if !claude_fast_panel_open(screen) {
        return None;
    }
    screen_lines(screen)
        .iter()
        .rev()
        .take(8)
        .find(|line| line.contains("Fast mode requires"))
        .map(|line| line.trim().replace(" · ", ". Run ").to_string())
}

impl PickerDriver<'_> {
    pub(super) async fn option_screen(
        &self,
        agent: SessionChatOptionAgent,
    ) -> Result<String, DomainStateError> {
        if (self.cancelled)() {
            return Err(agent_busy(
                "The option change remains queued after another session action.",
            ));
        }
        let screen = self
            .capture()
            .await
            .ok_or_else(|| session_not_running("Waiting for the agent's terminal."))?;
        if !ready(&screen, agent) {
            return Err(agent_busy(
                "Waiting for the agent to accept an option change.",
            ));
        }
        Ok(screen)
    }

    pub(super) async fn type_option_command(
        &self,
        agent: SessionChatOptionAgent,
        command: &str,
    ) -> Result<(), DomainStateError> {
        let screen = self.option_screen(agent).await?;
        if agent == SessionChatOptionAgent::Claude
            && crate::session_chat_composer::claude_composer_input_text(&screen)
                .is_some_and(|text| !text.trim().is_empty())
            && self.claude_input_holds_draft().await
        {
            return Err(agent_busy(
                "Waiting for the text in the terminal input to be sent or cleared.",
            ));
        }
        self.write(&build_session_chat_paste_bytes(command)).await?;
        self.wait_for("type option command", |screen| {
            // Slash completion can replace the ordinary ready verdict while the exact command is staged.
            let typed = if agent == SessionChatOptionAgent::Claude {
                crate::session_chat_composer::claude_composer_input_text(screen)
            } else {
                screen_lines(screen).iter().rev().find_map(|line| {
                    line.strip_prefix(CODEX_CURSOR)
                        .or_else(|| line.strip_prefix(CODEX_ULTRA_CURSOR))
                        .map(|text| text.trim().to_string())
                })
            };
            typed
                .is_some_and(|text| text.trim() == command)
                .then_some(())
        })
        .await?;
        self.write(CODEX_SUBMIT).await
    }

    /// CDXC:SessionChat 2026-09-08 WHY:
    /// Queued toggles store their destination, not a count of keypresses: every retry reads the live state, so an interrupted confirmation cannot toggle a successful change back.
    /// Claude and Antigravity modes are cycled one step at a time with fresh footer confirmation instead of trusting a stale client-side cycle index.
    pub(super) async fn drive_options(
        &self,
        plan: &CodexPickerPlan,
        agent: SessionChatOptionAgent,
    ) -> Result<(), DomainStateError> {
        /*
        CDXC:SessionChat 2026-09-29 WHY:
        On an account without usage credits Claude Code 2.1.284 answers `/fast` with a panel ("Fast mode requires usage credits · /usage-credits to turn them on", "Esc to cancel") instead of toggling. The panel stayed open, so every retry found no input box and waited forever, holding every model change and message behind it. A panel left open is closed first, the credits answer ends the change with Claude's reason, and a failed toggle never leaves the panel open.
        */
        let fast_result: Result<(), DomainStateError> = async {
            if let Some(target) = plan.options.fast_mode.as_deref() {
                let target = target == "on";
                if agent == SessionChatOptionAgent::Claude
                    && self
                        .capture()
                        .await
                        .is_some_and(|screen| claude_fast_panel_open(&screen))
                {
                    self.write(claude_effort_slider::CLAUDE_ESCAPE).await?;
                    self.wait_for("close Claude Fast mode panel", |screen| {
                        ready(screen, agent).then_some(())
                    })
                    .await?;
                }
                let screen = self.option_screen(agent).await?;
                let current = current_fast(&screen, agent, plan).ok_or_else(|| {
                    agent_busy("Waiting for the agent to report its Fast mode state.")
                })?;
                if current != target {
                    self.type_option_command(agent, "/fast").await?;
                    let unavailable = self
                        .wait_for("Fast mode", |screen| {
                            if current_fast(screen, agent, plan) == Some(target) {
                                return Some(None);
                            }
                            claude_fast_unavailable(screen).map(Some)
                        })
                        .await;
                    let leftover = self
                        .capture()
                        .await
                        .is_some_and(|screen| claude_fast_panel_open(&screen));
                    if leftover {
                        let _ = self.write(claude_effort_slider::CLAUDE_ESCAPE).await;
                        // The mode change below needs the input box back.
                        let _ = self
                            .wait_for("close Claude Fast mode panel", |screen| {
                                ready(screen, agent).then_some(())
                            })
                            .await;
                    }
                    if let Some(reason) = unavailable? {
                        return Err(unsupported_selection(format!("{reason}.")));
                    }
                }
            }
            Ok(())
        }
        .await;
        let mode_result = self.drive_mode_option(plan, agent).await;
        let permissions_result = self.drive_permissions_option(plan, agent).await;
        fast_result.and(mode_result).and(permissions_result)
    }

    /// CDXC:SessionChat 2026-10-21 WHY: Claude Code's Shift+Tab cycle leaves out the modes a session cannot use (2.1.301: manual → accept edits → plan → bypass when allowed → auto when the account, model and settings allow it → manual), so a mode the picker lists can be missing from the ring. Five presses and an `agentBusy` "remains queued" answer made the durable queue retry for ever, each retry rotating the user's mode one step further and never landing. The walk now stops on the first mode it has already seen, which is the one it started from, and refuses the pick as unsupported with the cycle it saw.
    async fn drive_mode_option(
        &self,
        plan: &CodexPickerPlan,
        agent: SessionChatOptionAgent,
    ) -> Result<(), DomainStateError> {
        if let Some(target) = plan.options.mode.as_deref() {
            let mut seen: Vec<String> = Vec::new();
            for _ in 0..8 {
                let screen = self.option_screen(agent).await?;
                let current = current_mode(&screen, agent).ok_or_else(|| {
                    agent_busy("Waiting for the agent to report its current mode.")
                })?;
                if current == target {
                    return Ok(());
                }
                if seen.contains(&current) {
                    return Err(unsupported_selection(
                        crate::session_chat_mode_availability::refusal_message(target, &seen),
                    ));
                }
                seen.push(current.clone());
                if agent == SessionChatOptionAgent::Codex && target == "plan" {
                    self.type_option_command(agent, "/plan").await?;
                } else {
                    self.write(SESSION_CHAT_SHIFT_TAB).await?;
                }
                self.wait_for("mode change", |screen| {
                    current_mode(screen, agent).filter(|mode| mode != &current)
                })
                .await?;
            }
            let screen = self.option_screen(agent).await?;
            if current_mode(&screen, agent).as_deref() != Some(target) {
                return Err(agent_busy(
                    "The requested mode is not currently offered by the agent; the choice remains queued.",
                ));
            }
        }
        Ok(())
    }
}
