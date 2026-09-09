//! CDXC:SessionChat 2026-09-09 DECISION:
//! User: Cursor, Grok Build and Antigravity support the quick picker, and Grok's models and efforts change directly from chat.
//! CDXC:SessionChat 2026-09-09 WHY:
//! Grok accepts `/model <label> <effort>` and Antigravity accepts a flattened model-effort id.
//! Cursor opens its filtered picker while /model is still being typed; Enter already confirms the model, so edit parameters before sending it.
//! Cursor's parameter editor must be read before every step so changing effort preserves Context, Thinking and Fast.
//! All three run inside the serialized send worker and confirm the footer before releasing the durable selection.

use super::*;
use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, session_chat_composer_input, SessionChatComposerState,
};
use crate::session_chat_send::{
    build_session_chat_paste_bytes, capture_session_terminal_text_vt, AGENT_TUI_CLEAR_INPUT_LINE,
};

fn catalog_model(provider: &str, model: &str) -> Option<Value> {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../agent-model-catalog.json"))
                .expect("bundled model catalog")
        })
        .get("agents")?
        .get(provider)?
        .get("models")?
        .as_array()?
        .iter()
        .find(|row| row.get("value").and_then(Value::as_str) == Some(model))
        .cloned()
}

fn option_agent(provider: &str) -> SessionChatOptionAgent {
    match provider {
        "cursor" => SessionChatOptionAgent::Cursor,
        "grok" => SessionChatOptionAgent::Grok,
        "antigravity" => SessionChatOptionAgent::Antigravity,
        _ => unreachable!("validated picker provider"),
    }
}

fn applied(screen: &str, plan: &CodexPickerPlan) -> bool {
    if detect_session_chat_composer_readiness(Some(&plan.provider), screen, None).state
        != SessionChatComposerState::Ready
    {
        return false;
    }
    detect_session_chat_selection(option_agent(&plan.provider), screen).is_some_and(|selection| {
        selection
            .model
            .as_ref()
            .is_some_and(|value| value.value == plan.model)
            && (plan.effort.is_empty()
                || selection
                    .effort
                    .as_ref()
                    .is_some_and(|value| value.value == plan.effort))
    })
}

fn cursor_model_row(screen: &str, label: &str) -> Option<String> {
    let lines = screen_lines(screen);
    let title = lines.iter().rposition(|line| {
        line.starts_with("Models matching \"") || line.starts_with("Available models")
    })?;
    let row = lines[title + 1..]
        .iter()
        .find_map(|line| line.strip_prefix('→').map(str::trim))?;
    (row == label
        || row
            .strip_prefix(label)
            .is_some_and(|rest| rest.starts_with(' ')))
    .then(|| row.to_string())
}

#[derive(PartialEq)]
struct ParameterRow {
    section: String,
    label: String,
    focused: bool,
    selected: bool,
}

fn cursor_parameters(screen: &str, label: &str) -> Option<Vec<ParameterRow>> {
    let lines = screen_lines(screen);
    let title = lines
        .iter()
        .rposition(|line| line.starts_with(label) && line.contains("Edit Parameters"))?;
    let mut section = String::new();
    let mut rows = Vec::new();
    for line in &lines[title + 1..] {
        let focused = line.starts_with('→');
        let text = line.trim_start_matches('→').trim();
        if matches!(text, "Context" | "Effort" | "Reasoning") {
            section = text.to_string();
        } else if let Some(marker) = text
            .chars()
            .next()
            .filter(|ch| matches!(ch, '○' | '●' | '◯' | '◉'))
        {
            rows.push(ParameterRow {
                section: section.clone(),
                label: text[marker.len_utf8()..]
                    .trim()
                    .trim_end_matches('✓')
                    .trim()
                    .to_string(),
                focused,
                selected: matches!(marker, '●' | '◉'),
            });
        }
    }
    (!rows.is_empty()).then_some(rows)
}

impl PickerDriver<'_> {
    async fn drive_provider(&self, plan: &CodexPickerPlan) -> Result<(), DomainStateError> {
        let row = catalog_model(&plan.provider, &plan.model)
            .ok_or_else(|| invalid_params("The model is not in this server's catalog."))?;
        let efforts = row
            .get("efforts")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid_params("The model's effort catalog is missing."))?;
        if !plan.effort.is_empty()
            && !efforts
                .iter()
                .any(|value| value.as_str() == Some(&plan.effort))
        {
            return Err(invalid_params("This model does not support that effort."));
        }
        let label = row
            .get("pickerLabel")
            .or_else(|| row.get("label"))
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_params("The model label is missing."))?;
        let command = match plan.provider.as_str() {
            "cursor" => format!("/model {label}"),
            "grok" => format!(
                "/model {label}{}",
                if plan.effort.is_empty() {
                    String::new()
                } else {
                    format!(" {}", plan.effort)
                }
            ),
            "antigravity" => format!(
                "/model {}{}",
                plan.model,
                if plan.effort.is_empty() {
                    String::new()
                } else {
                    format!("-{}", plan.effort)
                }
            ),
            _ => return Err(invalid_params("Unsupported model picker provider.")),
        };
        let screen = capture_session_terminal_text_vt(self.zmx_name)
            .await
            .ok_or_else(|| session_not_running("Waiting for the agent's terminal."))?;
        if applied(&screen, plan) {
            return Ok(());
        }
        if (self.cancelled)()
            || detect_session_chat_composer_readiness(Some(&plan.provider), &screen, None).state
                != SessionChatComposerState::Ready
        {
            return Err(agent_busy(
                "Waiting for the agent to accept the model command.",
            ));
        }
        if !session_chat_composer_input(&plan.provider, &screen)
            .is_some_and(|input| input.is_empty())
        {
            return Err(agent_busy(
                "Waiting for the terminal input to be sent or cleared.",
            ));
        }
        let mut opened_cursor = false;
        let result = async {
            self.write(&build_session_chat_paste_bytes(&command))
                .await?;
            if plan.provider == "cursor" {
                self.wait_for("Cursor model row", |screen| cursor_model_row(screen, label))
                    .await?;
                opened_cursor = true;
                if !plan.effort.is_empty() {
                    self.write("\t").await?;
                    let mut rows = self
                        .wait_for("Cursor parameters", |screen| {
                            cursor_parameters(screen, label)
                        })
                        .await?;
                    let effort_label = if plan.effort == "none" {
                        "None"
                    } else {
                        effort_row_label(&plan.effort).unwrap_or(&plan.effort)
                    };
                    loop {
                        let target = rows
                            .iter()
                            .position(|row| {
                                matches!(row.section.as_str(), "Effort" | "Reasoning")
                                    && row.label.eq_ignore_ascii_case(effort_label)
                            })
                            .ok_or_else(|| {
                                invalid_params("Cursor did not offer the requested effort.")
                            })?;
                        let current = rows.iter().position(|row| row.focused).ok_or_else(|| {
                            agent_busy("Cursor's focused parameter could not be read.")
                        })?;
                        if current == target {
                            break;
                        }
                        let next = if target > current {
                            current + 1
                        } else {
                            current - 1
                        };
                        self.write(if target > current { "\x1b[B" } else { "\x1b[A" })
                            .await?;
                        rows = self
                            .wait_for("Cursor parameter focus", |screen| {
                                let updated = cursor_parameters(screen, label)?;
                                (updated.get(next).is_some_and(|row| row.focused))
                                    .then_some(updated)
                            })
                            .await?;
                    }
                    // Space selects only this enum value. Escape returns to the model list; Enter applies it.
                    self.write(" ").await?;
                    self.wait_for("Cursor effort selection", |screen| {
                        cursor_parameters(screen, label)?
                            .iter()
                            .any(|row| {
                                row.focused
                                    && row.selected
                                    && row.label.eq_ignore_ascii_case(effort_label)
                            })
                            .then_some(())
                    })
                    .await?;
                    self.write("\x1b").await?;
                    self.wait_for("Cursor model row", |screen| cursor_model_row(screen, label))
                        .await?;
                }
            } else {
                self.wait_for("type model command", |screen| {
                    session_chat_composer_input(&plan.provider, screen)
                        .is_some_and(|input| collapse_spaces(&input.text) == command)
                        .then_some(())
                })
                .await?;
            }
            self.write(CODEX_SUBMIT).await?;
            self.wait_for("applied model and effort", |screen| {
                applied(screen, plan).then_some(())
            })
            .await
        }
        .await;
        if result.is_err() && !(self.cancelled)() {
            if opened_cursor {
                for _ in 0..2 {
                    let Some(screen) = self.capture().await else {
                        break;
                    };
                    if cursor_parameters(&screen, label).is_none()
                        && cursor_model_row(&screen, label).is_none()
                    {
                        break;
                    }
                    let _ = self.write("\x1b").await;
                    tokio::time::sleep(Duration::from_millis(PICKER_CANCEL_SETTLE_MS)).await;
                }
            }
            if let Some(screen) = self.capture().await {
                if session_chat_composer_input(&plan.provider, &screen)
                    .is_some_and(|input| collapse_spaces(&input.text) == command)
                {
                    let _ = self.write(AGENT_TUI_CLEAR_INPUT_LINE).await;
                }
            }
        }
        result
    }
}

pub(crate) async fn run_provider_model_picker_job(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    job_id: u64,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) {
    let plan = picker_jobs()
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(&job_id).map(|job| job.plan.clone()));
    let Some(plan) = plan else {
        return;
    };
    let driver = PickerDriver {
        project_id,
        session_id,
        zmx_name,
        source,
        cancelled,
    };
    let outcome = driver.drive_provider(&plan).await;
    if let Err(error) = &outcome {
        log_picker(
            LogLevel::Error,
            "sessionChatProviderModelPickFailed",
            json!({
                "projectId": project_id, "sessionId": session_id, "provider": plan.provider,
                "model": plan.model, "effort": plan.effort, "code": error.code,
            }),
            Some(error.message.clone()),
        );
    }
    if let Ok(mut jobs) = picker_jobs().lock() {
        if let Some(job) = jobs.get_mut(&job_id) {
            job.outcome = Some(outcome);
        }
    }
}
