//! CDXC:AgentProviders 2026-10-28 DECISION:
//! User: DeepSeek Harness's chat "please also expose the models there". dsh-tui 0.17's `/model <provider>/<id>` switches the session's route at once (the same in-session switch as its picker's Enter) and `/effort <level>` sets the reasoning level directly; an unknown model or level is answered with a notice and changes nothing. Its status line names the route's model id and the level (`deepseek-flash · Max · standard · Workspace read/write`), so a pick counts once that line shows it.
//! WHY: the composer is pasted into, not typed, as for Pi: a typed `/` opens dsh-tui's command completion, where Enter would fill the completion instead of running the command.
//! SEE-ALSO: agent-model-catalog.json (`agents.dsh`), packages/gx-chat-core/src/menus/option_catalog.rs (the menu).

use super::*;

/// The model id and the lower-cased level dsh-tui's status line shows, read from the lowest line
/// with at least three ` · ` fields whose first field ends in a model id.
pub(crate) fn dsh_status(screen: &str) -> Option<(String, Option<String>)> {
    screen.lines().rev().find_map(|line| {
        let line = crate::session_chat_options::strip_ansi_sgr(line);
        let fields: Vec<&str> = line.split(" \u{b7} ").map(str::trim).collect();
        if fields.len() < 3 {
            return None;
        }
        let model = fields[0]
            .split_whitespace()
            .find(|word| word.starts_with("deepseek-") || word.contains('/'))?;
        let effort = fields[1]
            .split_whitespace()
            .next()
            .map(str::to_ascii_lowercase);
        Some((model.to_string(), effort))
    })
}

fn shows_model(screen: &str, id: &str) -> bool {
    dsh_status(screen).is_some_and(|(model, _)| model == id || model.ends_with(&format!("/{id}")))
}

fn shows_effort(screen: &str, effort: &str) -> bool {
    dsh_status(screen).is_some_and(|(_, shown)| shown.as_deref() == Some(effort))
}

impl PickerDriver<'_> {
    async fn run_dsh_command(&self, command: &str) -> Result<(), DomainStateError> {
        self.write(&crate::session_chat_send::wrap_terminal_bracketed_paste_text(command))
            .await?;
        // Let the paste land before Enter, so Enter runs the command instead of ending the paste.
        tokio::time::sleep(Duration::from_millis(150)).await;
        self.write(CODEX_SUBMIT).await
    }

    pub(super) async fn drive_dsh(&self, plan: &CodexPickerPlan) -> Result<(), DomainStateError> {
        let id = plan
            .model
            .split_once('/')
            .map(|(_, id)| id)
            .filter(|id| !id.is_empty() && !plan.model.starts_with('/'))
            .ok_or_else(|| invalid_params("A DeepSeek Harness model is picked as provider/id."))?;
        let screen = self.capture().await.unwrap_or_default();
        if !shows_model(&screen, id) {
            self.run_dsh_command(&format!("/model {}", plan.model))
                .await?;
            self.wait_for("applied DeepSeek Harness model", |screen| {
                shows_model(screen, id).then_some(())
            })
            .await
            .map_err(|_| {
                unsupported_selection(format!(
                    "DeepSeek Harness did not switch to {}. Check that your account or API key offers it.",
                    plan.model
                ))
            })?;
        }
        if plan.effort.is_empty()
            || self
                .capture()
                .await
                .is_some_and(|screen| shows_effort(&screen, &plan.effort))
        {
            return Ok(());
        }
        self.run_dsh_command(&format!("/effort {}", plan.effort))
            .await?;
        self.wait_for("applied DeepSeek Harness effort", |screen| {
            shows_effort(screen, &plan.effort).then_some(())
        })
        .await
        .map_err(|_| {
            unsupported_selection(format!(
                "DeepSeek Harness did not set the reasoning effort to {}.",
                plan.effort
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_names_the_model_and_level() {
        let screen = "  Say something, or type / for commands…\n\n deepseek-flash · Max · standard · Workspace read/write\n";
        assert_eq!(
            dsh_status(screen),
            Some(("deepseek-flash".to_string(), Some("max".to_string())))
        );
        assert!(shows_model(screen, "deepseek-flash"));
        assert!(!shows_model(screen, "deepseek-v4-flash"));
        assert!(shows_effort(screen, "max"));
        assert!(!shows_effort(screen, "high"));
    }

    #[test]
    fn prose_with_middle_dots_is_not_a_status_line() {
        assert_eq!(
            dsh_status("Sessions & workspaces · Settings · Kernel · DSH · Help"),
            None
        );
    }
}
