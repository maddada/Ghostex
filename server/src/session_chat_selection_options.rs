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

/// How many of the bottom non-empty lines may hold Codex's model segment.
const CODEX_FOOTER_LINES: usize = 3;

/// The part of a busy screen that holds the agent's footer, when it can be found: Claude's lines
/// below its input box (the `❯` row between two rules, as `claude_ultracode_on_lines` finds it, so
/// a dialog's diff with rules in it is not read as the footer), Codex's bottom non-empty lines.
fn busy_footer(screen: &str, agent: SessionChatOptionAgent) -> Option<String> {
    let lines = screen_lines(screen);
    let lines: Vec<&String> = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let footer: Vec<&String> = if agent == SessionChatOptionAgent::Claude {
        let is_rule = |line: &&String| crate::session_chat_options::is_divider_line(line);
        let bottom = lines.iter().rposition(is_rule)?;
        let top = lines[..bottom].iter().rposition(is_rule)?;
        if !lines.get(top + 1)?.trim_start().starts_with('\u{276f}') {
            return None;
        }
        lines[bottom + 1..].to_vec()
    } else {
        lines[lines.len().saturating_sub(CODEX_FOOTER_LINES)..].to_vec()
    };
    let footer = footer
        .into_iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    (!footer.trim().is_empty()).then_some(footer)
}

/// The requested options a busy agent already shows in effect (`options_in_effect_while_busy`).
#[derive(Default)]
struct OptionsInEffect {
    fast: bool,
    mode: bool,
}

impl PickerDriver<'_> {
    /// CDXC:SessionChat 2026-11-18 WHY:
    /// GitHub #666: a Plan or Fast mode change that met a busy agent was retried for as long as it worked (a day for one session), even when the agent already showed the asked-for setting, because the current value was only read once the input box was ready. While the input box is not ready, a requested mode or Fast mode the agent already shows counts as done; the rest wait for the input box as before. Only a value the agent names counts: Claude's mode badge, Codex's plan marker, Codex's `fast` word, or the Fast mode in Claude's status line payload. Codex's default mode and Fast off are read from what is missing, which a busy screen cannot prove, and Codex permissions are left to the ready path. The footer must be found where it belongs, or prose and scrollback could pass for it: Claude's below the screen's last rule, Codex's model segment on the bottom lines. Claude's status line payload counts only when it was stored after the CLI last started.
    async fn options_in_effect_while_busy(
        &self,
        plan: &CodexPickerPlan,
        agent: SessionChatOptionAgent,
    ) -> OptionsInEffect {
        if plan.options.mode.is_none() && plan.options.fast_mode.is_none()
            || !matches!(
                agent,
                SessionChatOptionAgent::Claude | SessionChatOptionAgent::Codex
            )
        {
            return OptionsInEffect::default();
        }
        let Some(screen) = self.capture().await else {
            return OptionsInEffect::default();
        };
        // A ready screen, or Claude's Fast mode panel left open, goes through the usual steps.
        if ready(&screen, agent) || claude_fast_panel_open(&screen) {
            return OptionsInEffect::default();
        }
        let selection = busy_footer(&screen, agent)
            .and_then(|footer| detect_session_chat_selection(agent, &footer))
            .filter(|selection| {
                agent == SessionChatOptionAgent::Claude || selection.model.is_some()
            });
        let fast = plan.options.fast_mode.as_deref().is_some_and(|target| {
            let target = target == "on";
            let shown = if agent == SessionChatOptionAgent::Claude {
                plan.claude_statusline
                    .as_ref()
                    .and_then(|(directory, session_id)| {
                        crate::agent_hooks::statusline::read_claude_statusline_payload(
                            directory, session_id,
                        )
                    })
                    .filter(|stored| {
                        plan.claude_started_at
                            .zip(stored.modified)
                            .is_some_and(|(started, stored)| stored > started)
                    })
                    .and_then(|stored| stored.payload.get("fast_mode").and_then(Value::as_bool))
            } else {
                selection.as_ref().and_then(|selection| selection.fast)
            };
            shown == Some(target)
        });
        let mode = plan.options.mode.as_deref().is_some_and(|target| {
            selection
                .as_ref()
                .and_then(|selection| selection.mode.as_ref())
                .is_some_and(|mode| mode.value == target)
        });
        OptionsInEffect { fast, mode }
    }

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
        if agent == SessionChatOptionAgent::Claude {
            self.type_claude_command("type option command", command)
                .await?;
            return self.write(CODEX_SUBMIT).await;
        }
        let payload = build_session_chat_paste_bytes(command);
        // Slash completion can replace the ordinary ready verdict while the exact command is staged.
        let typed = |screen: &str| {
            screen_lines(screen)
                .iter()
                .rev()
                .find_map(|line| {
                    line.strip_prefix(CODEX_CURSOR)
                        .or_else(|| line.strip_prefix(CODEX_ULTRA_CURSOR))
                        .map(|text| text.trim().to_string())
                })
                .is_some_and(|text| text.trim() == command)
                .then_some(())
        };
        if agent == SessionChatOptionAgent::Codex {
            self.type_command_or_erase(agent, "type option command", command, &payload, typed)
                .await?;
        } else {
            self.write(&payload).await?;
            self.wait_for("type option command", typed).await?;
        }
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
        let in_effect = self.options_in_effect_while_busy(plan, agent).await;
        let fast_result: Result<(), DomainStateError> = async {
            if let Some(target) = plan
                .options
                .fast_mode
                .as_deref()
                .filter(|_| !in_effect.fast)
            {
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
                    state_unreadable("Ghostex could not read the agent's Fast mode state.")
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
        let mode_result = if in_effect.mode {
            Ok(())
        } else {
            self.drive_mode_option(plan, agent).await
        };
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
                    state_unreadable("Ghostex could not read the agent's current mode.")
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
