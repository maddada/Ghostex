//! Desired option delivery inside the session's serialized terminal worker.

use super::*;
use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, SessionChatComposerState,
};
use crate::session_chat_send::{build_session_chat_paste_bytes, SESSION_CHAT_SHIFT_TAB};

fn agent_name(agent: SessionChatOptionAgent) -> &'static str {
    if agent == SessionChatOptionAgent::Claude {
        "claude"
    } else {
        "codex"
    }
}

fn ready(screen: &str, agent: SessionChatOptionAgent) -> bool {
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

impl PickerDriver<'_> {
    async fn option_screen(
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

    async fn type_option_command(
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
    /// Claude permission modes are cycled one step at a time with fresh footer confirmation instead of trusting a stale client-side cycle index.
    pub(super) async fn drive_options(
        &self,
        plan: &CodexPickerPlan,
        agent: SessionChatOptionAgent,
    ) -> Result<(), DomainStateError> {
        let fast_result: Result<(), DomainStateError> = async {
            if let Some(target) = plan.options.fast_mode.as_deref() {
                let target = target == "on";
                let screen = self.option_screen(agent).await?;
                let current = current_fast(&screen, agent, plan).ok_or_else(|| {
                    agent_busy("Waiting for the agent to report its Fast mode state.")
                })?;
                if current != target {
                    self.type_option_command(agent, "/fast").await?;
                    self.wait_for("Fast mode", |screen| {
                        (current_fast(screen, agent, plan) == Some(target)).then_some(())
                    })
                    .await?;
                }
            }
            Ok(())
        }
        .await;
        let mode_result = self.drive_mode_option(plan, agent).await;
        fast_result.and(mode_result)
    }

    async fn drive_mode_option(
        &self,
        plan: &CodexPickerPlan,
        agent: SessionChatOptionAgent,
    ) -> Result<(), DomainStateError> {
        if let Some(target) = plan.options.mode.as_deref() {
            for _ in 0..5 {
                let screen = self.option_screen(agent).await?;
                let current = current_mode(&screen, agent).ok_or_else(|| {
                    agent_busy("Waiting for the agent to report its current mode.")
                })?;
                if current == target {
                    return Ok(());
                }
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
