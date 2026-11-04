//! CDXC:SessionChat 2026-09-09 DECISION:
//! User: Cursor, Grok Build and Antigravity support the quick picker, and Grok's models and efforts change directly from chat.
//! CDXC:SessionChat 2026-09-09 WHY:
//! Grok accepts `/model <label> <effort>` and Antigravity accepts a flattened model-effort id.
//! Cursor opens its filtered picker while /model is still being typed; Enter already confirms the model, so edit parameters before sending it.
//! Cursor's parameter editor must be read before every step so changing effort preserves Context, Thinking and Fast.
//! All three run inside the serialized send worker and confirm the footer before releasing the durable selection.

use super::*;

#[path = "session_chat_empryo_picker.rs"]
pub(super) mod empryo_picker;

#[path = "session_chat_dsh_picker.rs"]
pub(super) mod dsh_picker;

use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, session_chat_composer_input, SessionChatComposerState,
};
use crate::session_chat_send::{
    build_session_chat_paste_bytes, capture_session_terminal_text_vt, AGENT_TUI_CLEAR_INPUT_LINE,
};

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

/// CDXC:SessionChat 2026-10-17 WHY:
/// Antigravity CLI answers a `/model` it does not offer with `⎿  Invalid model "<id>" (available: …)` (and a bare id that needs an effort with `requires effort`) and stays on its model. Waiting only for the footer timed out, which the queue treats as worth retrying, so the refused `/model` was retyped every five seconds and the chat kept showing the refused model as the one heading in. The refusal is final: the pick fails with agy's reason, and the chat goes back to the model agy is really on.
/// Returns the reason of every refusal on screen for the id this command types, oldest first.
fn antigravity_refusals(screen: &str, command: &str) -> Vec<String> {
    let Some(id) = command.strip_prefix("/model ").map(str::trim) else {
        return Vec::new();
    };
    let quoted = format!("\"{id}\"");
    screen_lines(screen)
        .iter()
        .filter_map(|line| {
            let text = line.trim().trim_start_matches('⎿').trim();
            let refused = (text.starts_with("Invalid model") && text.contains(&quoted))
                || (text.contains(id) && text.contains("requires effort"));
            refused.then(|| {
                text.split(" (available:")
                    .next()
                    .unwrap_or(text)
                    .trim()
                    .to_string()
            })
        })
        .collect()
}

/// CDXC:SessionChat 2026-10-07 WHY: Cursor's picker names Claude models "Claude Fable 5" where the catalog says "Fable 5", and its fuzzy filter can highlight a longer name first ("Claude Fable 5.1" for "Fable 5"), so waiting for the highlighted row to be the catalog label timed out ("did not confirm the model picker's Cursor model row step"). A row matches with or without the "Claude " prefix, and the highlight is moved onto the matching row before it is confirmed.
fn cursor_row_name_matches(row: &str, label: &str) -> bool {
    let matches = |name: &str| {
        name == label
            || name
                .strip_prefix(label)
                .is_some_and(|rest| rest.starts_with(' '))
    };
    matches(row) || row.strip_prefix("Claude ").is_some_and(matches)
}

/// The rows of Cursor's picker filtered by `label`, in screen order, with the highlighted one marked.
fn cursor_filtered_rows(screen: &str, label: &str) -> Option<Vec<(String, bool)>> {
    let lines = screen_lines(screen);
    let title = lines
        .iter()
        .rposition(|line| line.starts_with(&format!("Models matching \"{label}\"")))?;
    let rows: Vec<(String, bool)> = lines[title + 1..]
        .iter()
        .take_while(|line| !line.starts_with("Edit prompt to filter"))
        .filter(|line| !line.is_empty())
        .map(|line| match line.strip_prefix('→') {
            Some(row) => (row.trim().to_string(), true),
            None => (line.trim().to_string(), false),
        })
        .collect();
    (!rows.is_empty()).then_some(rows)
}

/// How far the highlight has to move to reach the row for `label`: rows down when positive.
fn cursor_row_distance(screen: &str, label: &str) -> Option<isize> {
    let rows = cursor_filtered_rows(screen, label)?;
    let target = rows
        .iter()
        .position(|(row, _)| cursor_row_name_matches(row, label))?;
    let current = rows.iter().position(|(_, highlighted)| *highlighted)?;
    Some(target as isize - current as isize)
}

fn cursor_model_row(screen: &str, label: &str) -> Option<String> {
    let lines = screen_lines(screen);
    let title = lines.iter().rposition(|line| {
        line.starts_with("Models matching \"") || line.starts_with("Available models")
    })?;
    let row = lines[title + 1..]
        .iter()
        .find_map(|line| line.strip_prefix('→').map(str::trim))?;
    cursor_row_name_matches(row, label).then(|| row.to_string())
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
    let title = lines.iter().rposition(|line| {
        line.contains("Edit Parameters") && cursor_row_name_matches(line, label)
    })?;
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

/// Whether the input box holds exactly `command`. A plain capture cannot tell Hermes' italic
/// placeholder from typed text, so the other readings compare against the command instead.
fn hermes_input_is(composer_agent: &str, screen: &str, command: &str) -> bool {
    session_chat_composer_input(composer_agent, screen)
        .is_some_and(|input| collapse_spaces(&input.text) == command)
}

/// Whether the status bar names `model`.
fn hermes_shows_model(screen: &str, model: &str) -> bool {
    detect_session_chat_selection(SessionChatOptionAgent::Hermes, screen).is_some_and(|selection| {
        selection.model.is_some_and(|shown| {
            crate::session_chat_hermes_status::hermes_status_bar_shows(&shown.value, model)
        })
    })
}

/// The command left the input box and the status bar names the requested model.
fn hermes_applied(
    composer_agent: &str,
    screen: &str,
    plan: &CodexPickerPlan,
    command: &str,
) -> bool {
    !hermes_input_is(composer_agent, screen, command) && hermes_shows_model(screen, &plan.model)
}

/// What Hermes printed in answer to a `/model` command.
#[derive(Debug, PartialEq)]
enum HermesModelReply {
    /// `✓ Model switched: <model>`.
    Switched,
    /// `✗ <reason>`.
    Refused(String),
    /// `Model switch cancelled.`: its expensive-model question was answered Cancel.
    Cancelled,
}

/// Every answer to a `/model` command on screen, oldest first.
fn hermes_model_replies(screen: &str) -> Vec<HermesModelReply> {
    screen_lines(screen)
        .into_iter()
        .filter_map(|line| {
            if line.starts_with("\u{2713} Model switched:") {
                Some(HermesModelReply::Switched)
            } else if let Some(reason) = line.strip_prefix('\u{2717}') {
                Some(HermesModelReply::Refused(reason.trim().to_string()))
            } else if line == "Model switch cancelled." {
                Some(HermesModelReply::Cancelled)
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Pi and OMP
// ---------------------------------------------------------------------------

/// Pi's `/model` falls back to its selector and may refresh its catalogs first (up to 15s).
const PI_MODEL_SWITCH_TIMEOUT_MS: u64 = 20_000;
/// One press per visible selector row, with slack.
const PI_SELECTOR_STEP_LIMIT: usize = 14;
const PI_SELECTOR_CANCEL: &str = "\u{1b}";

/// A row of Pi's model selector: `→ ✓ gpt-5.5 [openai-codex] · default`.
#[derive(Debug, PartialEq)]
struct PiSelectorRow {
    id: String,
    provider: String,
    selected: bool,
}

impl PiSelectorRow {
    fn is(&self, provider: &str, id: &str) -> bool {
        self.provider == provider && self.id == id
    }
}

fn parse_pi_selector_row(line: &str) -> Option<PiSelectorRow> {
    let (selected, rest) = match line.strip_prefix('\u{2192}') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, line),
    };
    let rest = rest.strip_prefix('\u{2713}').map_or(rest, str::trim_start);
    let (id, tail) = rest.split_once(" [")?;
    let (provider, _) = tail.split_once(']')?;
    (!id.is_empty() && !id.contains(' ') && !provider.is_empty()).then(|| PiSelectorRow {
        id: id.to_string(),
        provider: provider.to_string(),
        selected,
    })
}

/// The rows of Pi's open model selector (between its scope or provider header and its key hint),
/// or `None` while it is closed.
fn pi_model_selector_rows(screen: &str) -> Option<Vec<PiSelectorRow>> {
    let lines = screen_lines(screen);
    let hint = lines.iter().rposition(|line| {
        line.contains(" to select \u{b7} ") && line.contains(" to set as default")
    })?;
    let top = lines[..hint].iter().rposition(|line| {
        line.starts_with("Scope: ")
            || line.starts_with("Only showing models from configured providers")
    })?;
    Some(
        lines[top + 1..hint]
            .iter()
            .filter_map(|line| parse_pi_selector_row(line))
            .collect(),
    )
}

/// Every error line Pi and OMP print in answer to a command, oldest first.
fn pi_family_errors(screen: &str) -> Vec<String> {
    screen_lines(screen)
        .into_iter()
        .filter_map(|line| {
            line.strip_prefix("Error: ")
                .map(str::to_string)
                .or_else(|| line.contains("Unknown model:").then(|| line.clone()))
                .or_else(|| {
                    line.contains("Failed to switch model:")
                        .then(|| line.clone())
                })
        })
        .collect()
}

/// The model and level Pi's footer shows.
fn pi_footer(screen: &str) -> Option<(String, Option<String>)> {
    let selection = detect_session_chat_selection(SessionChatOptionAgent::Pi, screen)?;
    Some((
        selection.model?.value,
        selection.effort.map(|effort| effort.value),
    ))
}

/// Whether Pi's footer reading names `model` (`provider/id`): the footer drops the provider when
/// only one is logged in or the line is narrow.
fn pi_names_model(shown: &str, model: &str) -> bool {
    shown == model || model.split_once('/').is_some_and(|(_, id)| id == shown)
}

/// Whether OMP's composer head names `model` (`provider/id`) at `effort` (any level when empty).
fn omp_shows(screen: &str, model: &str, effort: &str) -> bool {
    let Some(selection) = detect_session_chat_selection(SessionChatOptionAgent::Omp, screen) else {
        return false;
    };
    let labels = crate::session_chat_pi_models::pi_family_terminal_labels(
        crate::session_chat_pi_models::PiFamilyAgent::Omp,
        model,
    );
    selection.model.is_some_and(|shown| {
        labels
            .iter()
            .any(|label| label.eq_ignore_ascii_case(&shown.value))
    }) && (effort.is_empty() || selection.effort.is_some_and(|shown| shown.value == effort))
}

impl PickerDriver<'_> {
    /// CDXC:AgentProviders 2026-09-27 WHY:
    /// Hermes' status bar never shows the reasoning effort, so an effort-only change cannot be recognised as already applied and the command is always typed. It counts once Hermes answers it: the first `✓ Model switched`, `✗ <reason>` or `Model switch cancelled.` line past the answers already on screen. The status bar alone is not proof, because a same-model pick leaves it unchanged while Hermes can still refuse the command or hold it behind its expensive-model question (verified in Hermes Agent v0.21.4); it only counts for a new model once older answers have scrolled out of the capture. Delivery is verified before Enter, so a switch with no answer within the step is final instead of retried: retyping the same command cannot change Hermes' answer.
    async fn drive_hermes(&self, plan: &CodexPickerPlan) -> Result<(), DomainStateError> {
        if !plan.effort.is_empty()
            && !crate::session_chat_hermes_status::HERMES_PICKER_EFFORTS
                .contains(&plan.effort.as_str())
        {
            return Err(invalid_params(
                "Hermes' model picker offers low, medium and high.",
            ));
        }
        let mut command = format!("/model {}", plan.model);
        if let Some(provider) = &plan.hermes_provider {
            command.push_str(&format!(" --provider {provider}"));
        }
        if !plan.effort.is_empty() {
            command.push_str(&format!(" --reasoning {}", plan.effort));
        }
        // The composer table spells the agent `hermes-agent`.
        let composer_agent =
            crate::agents::identity::normalize_agent_id(Some(&plan.provider)).unwrap_or_default();
        let screen = capture_session_terminal_text_vt(self.zmx_name)
            .await
            .ok_or_else(|| session_not_running("Waiting for the agent's terminal."))?;
        if (self.cancelled)()
            || detect_session_chat_composer_readiness(Some(&plan.provider), &screen, None).state
                != SessionChatComposerState::Ready
        {
            return Err(agent_busy(
                "Waiting for Hermes to accept the model command.",
            ));
        }
        // A VT capture, so the italic placeholder reads as empty.
        if !session_chat_composer_input(&composer_agent, &screen)
            .is_some_and(|input| input.is_empty())
        {
            return Err(agent_busy(
                "Waiting for the terminal input to be sent or cleared.",
            ));
        }
        let result = async {
            self.write(&build_session_chat_paste_bytes(&command))
                .await?;
            // Read off the screen that shows the command typed, the last one before Enter.
            let (replies_before, model_was_shown) = self
                .wait_for("type model command", |screen| {
                    hermes_input_is(&composer_agent, screen, &command).then(|| {
                        (
                            hermes_model_replies(screen).len(),
                            hermes_shows_model(screen, &plan.model),
                        )
                    })
                })
                .await?;
            self.write(CODEX_SUBMIT).await?;
            let outcome = self
                .wait_for("applied model", |screen| {
                    match hermes_model_replies(screen).into_iter().nth(replies_before) {
                        Some(HermesModelReply::Switched) => Some(Ok(())),
                        Some(HermesModelReply::Refused(reason)) => Some(Err(reason)),
                        Some(HermesModelReply::Cancelled) => {
                            Some(Err("the switch was cancelled in its terminal.".to_string()))
                        }
                        None => (!model_was_shown
                            && hermes_applied(&composer_agent, screen, plan, &command))
                        .then_some(Ok(())),
                    }
                })
                .await;
            match outcome {
                Ok(Ok(())) => Ok(()),
                Ok(Err(reason)) => Err(unsupported_selection(format!(
                    "Hermes did not switch to {}: {reason}",
                    plan.model
                ))),
                Err(error) if error.code == "timeout" => Err(unsupported_selection(format!(
                    "Hermes did not switch to {}. Check its terminal for a question or an error.",
                    plan.model
                ))),
                Err(error) => Err(error),
            }
        }
        .await;
        if result.is_err() && !(self.cancelled)() {
            if let Some(screen) = self.capture().await {
                if hermes_input_is(&composer_agent, &screen, &command) {
                    // Hermes' verified clear: one Ctrl+C, only while a draft is on screen.
                    let _ = crate::session_chat_send::clear_session_chat_composer(
                        self.project_id,
                        self.session_id,
                        self.zmx_name,
                        self.source,
                        &plan.provider,
                        self.cancelled,
                    )
                    .await;
                }
            }
        }
        result
    }

    /// Waits until `agent`'s composer is ready and empty, the state every typed pick starts from.
    async fn composer_ready(&self, agent: &str) -> Result<(), DomainStateError> {
        let screen = capture_session_terminal_text_vt(self.zmx_name)
            .await
            .ok_or_else(|| session_not_running("Waiting for the agent's terminal."))?;
        if (self.cancelled)()
            || detect_session_chat_composer_readiness(Some(agent), &screen, None).state
                != SessionChatComposerState::Ready
        {
            return Err(agent_busy(
                "Waiting for the agent to accept the model command.",
            ));
        }
        if !session_chat_composer_input(agent, &screen).is_some_and(|input| input.is_empty()) {
            return Err(agent_busy(
                "Waiting for the terminal input to be sent or cleared.",
            ));
        }
        Ok(())
    }

    /// Pastes `command` and waits for the input box to hold it; answers how many error lines were
    /// on that screen, so the command's own answer is the next one.
    ///
    /// CDXC:AgentProviders 2026-09-30 WHY:
    /// A real bracketed paste, not typed keys: typed `/model cursor/grok-4.6` opens Pi's argument autocomplete for a model in its scope, and the Enter then only accepts the suggestion and leaves the command in the input (measured on Pi 0.87; every in-scope pick timed out while out-of-scope ones worked). A paste leaves the autocomplete closed.
    async fn type_pi_family_command(
        &self,
        agent: &str,
        command: &str,
    ) -> Result<usize, DomainStateError> {
        self.write(&crate::session_chat_send::wrap_terminal_bracketed_paste_text(command))
            .await?;
        self.wait_for("type model command", |screen| {
            session_chat_composer_input(agent, screen)
                .is_some_and(|input| collapse_spaces(&input.text) == command)
                .then(|| pi_family_errors(screen).len())
        })
        .await
    }

    /// Clears a pick left behind: Pi's selector, then the typed command.
    async fn abandon_pi_family_command(&self, agent: &str, command: &str) {
        if (self.cancelled)() {
            return;
        }
        if self
            .capture()
            .await
            .is_some_and(|screen| pi_model_selector_rows(&screen).is_some())
        {
            let _ = self.write(PI_SELECTOR_CANCEL).await;
            tokio::time::sleep(Duration::from_millis(PICKER_CANCEL_SETTLE_MS)).await;
        }
        if let Some(screen) = self.capture().await {
            if session_chat_composer_input(agent, &screen)
                .is_some_and(|input| collapse_spaces(&input.text) == command)
            {
                let _ = self.write(AGENT_TUI_CLEAR_INPUT_LINE).await;
            }
        }
    }

    /// CDXC:AgentProviders 2026-09-30 WHY:
    /// Pi's `/model <provider>/<id>` switches at once only when the reference is in the session's model scope (`enabledModels`, measured on Pi 0.87 with a Cursor-only scope); otherwise it opens its selector filtered to the reference, on the scoped list, where Tab shows every model. The selector's Enter applies to the session like the direct switch (Ctrl+S would save a default), so the driver reads its rows, widens the scope once when the model is missing, and presses Enter only on the row naming the exact provider and id. `/thinking <level>` answers a level the model lacks with `Error: Unknown thinking level`, which ends the pick at once.
    async fn drive_pi(&self, plan: &CodexPickerPlan) -> Result<(), DomainStateError> {
        let (provider, id) = plan
            .model
            .split_once('/')
            .filter(|(provider, id)| !provider.is_empty() && !id.is_empty())
            .ok_or_else(|| invalid_params("A Pi model is picked as provider/id."))?;
        self.composer_ready("pi").await?;
        let shown = self.capture().await.and_then(|screen| pi_footer(&screen));
        if !shown
            .as_ref()
            .is_some_and(|(model, _)| pi_names_model(model, &plan.model))
        {
            let command = format!("/model {}", plan.model);
            let result = self.pi_switch_model(plan, provider, id, &command).await;
            if result.is_err() {
                self.abandon_pi_family_command("pi", &command).await;
            }
            result?;
        }
        if plan.effort.is_empty() {
            return Ok(());
        }
        let level = self
            .capture()
            .await
            .and_then(|screen| pi_footer(&screen))
            .and_then(|(_, effort)| effort);
        if level.as_deref() == Some(plan.effort.as_str()) {
            return Ok(());
        }
        self.composer_ready("pi").await?;
        let command = format!("/thinking {}", plan.effort);
        let result = async {
            let errors_before = self.type_pi_family_command("pi", &command).await?;
            self.write(CODEX_SUBMIT).await?;
            let outcome = self
                .wait_for("applied thinking level", |screen| {
                    if let Some(reason) = pi_family_errors(screen).into_iter().nth(errors_before) {
                        return Some(Err(reason));
                    }
                    pi_footer(screen)
                        .is_some_and(|(_, effort)| effort.as_deref() == Some(plan.effort.as_str()))
                        .then_some(Ok(()))
                })
                .await?;
            outcome.map_err(|reason| {
                unsupported_selection(format!(
                    "Pi did not set the thinking level to {}: {reason}",
                    plan.effort
                ))
            })
        }
        .await;
        if result.is_err() {
            self.abandon_pi_family_command("pi", &command).await;
        }
        result
    }

    async fn pi_switch_model(
        &self,
        plan: &CodexPickerPlan,
        provider: &str,
        id: &str,
        command: &str,
    ) -> Result<(), DomainStateError> {
        enum Answer {
            Applied,
            Selector(Vec<PiSelectorRow>),
            Refused(String),
        }
        let errors_before = self.type_pi_family_command("pi", command).await?;
        self.write(CODEX_SUBMIT).await?;
        let answer = self
            .wait_for_within("Pi model switch", PI_MODEL_SWITCH_TIMEOUT_MS, |screen| {
                if let Some(rows) = pi_model_selector_rows(screen) {
                    return Some(Answer::Selector(rows));
                }
                if let Some(reason) = pi_family_errors(screen).into_iter().nth(errors_before) {
                    return Some(Answer::Refused(reason));
                }
                let typed = session_chat_composer_input("pi", screen)
                    .is_some_and(|input| collapse_spaces(&input.text) == command);
                (!typed
                    && pi_footer(screen)
                        .is_some_and(|(model, _)| pi_names_model(&model, &plan.model)))
                .then_some(Answer::Applied)
            })
            .await?;
        let mut rows = match answer {
            Answer::Applied => return Ok(()),
            Answer::Refused(reason) => {
                return Err(unsupported_selection(format!(
                    "Pi did not switch to {}: {reason}",
                    plan.model
                )));
            }
            Answer::Selector(rows) => rows,
        };
        let mut widened = false;
        for _ in 0..PI_SELECTOR_STEP_LIMIT {
            let Some(target) = rows.iter().position(|row| row.is(provider, id)) else {
                if widened {
                    return Err(unsupported_selection(format!(
                        "Pi's model selector does not list {}.",
                        plan.model
                    )));
                }
                widened = true;
                self.write("\t").await?;
                rows = self
                    .wait_for("Pi selector scope", |screen| {
                        let rows = pi_model_selector_rows(screen)?;
                        rows.iter().any(|row| row.is(provider, id)).then_some(rows)
                    })
                    .await?;
                continue;
            };
            let focused = rows
                .iter()
                .position(|row| row.selected)
                .ok_or_else(|| dialog_mismatch("Pi model selector", "no focused row"))?;
            if focused == target {
                self.write(CODEX_SUBMIT).await?;
                return self
                    .wait_for("applied model", |screen| {
                        (pi_model_selector_rows(screen).is_none()
                            && pi_footer(screen)
                                .is_some_and(|(model, _)| pi_names_model(&model, &plan.model)))
                        .then_some(())
                    })
                    .await;
            }
            self.write(if target > focused {
                CODEX_ARROW_DOWN
            } else {
                CODEX_ARROW_UP
            })
            .await?;
            rows = self
                .wait_for("Pi selector focus", |screen| {
                    let rows = pi_model_selector_rows(screen)?;
                    (rows.iter().position(|row| row.selected) != Some(focused)).then_some(rows)
                })
                .await?;
        }
        Err(picker_timeout("Pi selector focus"))
    }

    /// CDXC:AgentProviders 2026-09-30 WHY:
    /// OMP's `/switch <provider>/<id>[:<level>]` changes the model and level for this session in one command (`/model` only opens its selector, ignoring arguments). Its composer head names the model by display name, which the lineup's `terminalLabels` map back to the row.
    async fn drive_omp(&self, plan: &CodexPickerPlan) -> Result<(), DomainStateError> {
        if !plan.model.contains('/') {
            return Err(invalid_params("An OMP model is picked as provider/id."));
        }
        self.composer_ready("omp").await?;
        if self
            .capture()
            .await
            .is_some_and(|screen| omp_shows(&screen, &plan.model, &plan.effort))
        {
            return Ok(());
        }
        let command = if plan.effort.is_empty() {
            format!("/switch {}", plan.model)
        } else {
            format!("/switch {}:{}", plan.model, plan.effort)
        };
        let result = async {
            let errors_before = self.type_pi_family_command("omp", &command).await?;
            self.write(CODEX_SUBMIT).await?;
            let outcome = self
                .wait_for("applied model", |screen| {
                    if let Some(reason) = pi_family_errors(screen).into_iter().nth(errors_before) {
                        return Some(Err(reason));
                    }
                    let typed = session_chat_composer_input("omp", screen)
                        .is_some_and(|input| collapse_spaces(&input.text) == command);
                    (!typed && omp_shows(screen, &plan.model, &plan.effort)).then_some(Ok(()))
                })
                .await?;
            outcome.map_err(|reason| {
                unsupported_selection(format!("OMP did not switch to {}: {reason}", plan.model))
            })
        }
        .await;
        if result.is_err() {
            self.abandon_pi_family_command("omp", &command).await;
        }
        result
    }

    async fn drive_provider(&self, plan: &CodexPickerPlan) -> Result<(), DomainStateError> {
        let row = crate::agent_model_catalog::catalog_model(&plan.provider, &plan.model)
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
        let refusals_before = antigravity_refusals(&screen, &command);
        let mut opened_cursor = false;
        let result = async {
            self.write(&build_session_chat_paste_bytes(&command))
                .await?;
            if plan.provider == "cursor" {
                let distance = self
                    .wait_for("Cursor model rows", |screen| {
                        cursor_row_distance(screen, label)
                    })
                    .await?;
                opened_cursor = true;
                let arrow = if distance > 0 { "\x1b[B" } else { "\x1b[A" };
                for _ in 0..distance.unsigned_abs() {
                    self.write(arrow).await?;
                }
                self.wait_for("Cursor model row", |screen| cursor_model_row(screen, label))
                    .await?;
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
            let refused = self
                .wait_for("applied model and effort", |screen| {
                    if applied(screen, plan) {
                        return Some(None);
                    }
                    (plan.provider == "antigravity")
                        .then(|| antigravity_refusals(screen, &command))
                        .filter(|refusals| refusals.len() > refusals_before.len())
                        .and_then(|refusals| refusals.last().cloned())
                        .map(Some)
                })
                .await?;
            match refused {
                Some(reason) => Err(unsupported_selection(format!(
                    "Antigravity CLI refused the model: {reason}."
                ))),
                None => Ok(()),
            }
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
    /*
    CDXC:SessionChat 2026-10-06 WHY:
    A picker types the agent's own model and effort commands, and their result rows (Empryo's "Model switched · …", "Effort: …") were read as output by a slash command sent from the chat earlier, so the picks showed under that command (seen live under `/checkpoint undo`). A pick closes those captures first, as a chat send does.
    */
    crate::session_chat_app_command::stop_local_command_output(project_id, session_id);
    let driver = PickerDriver {
        project_id,
        session_id,
        zmx_name,
        source,
        cancelled,
    };
    let outcome = match plan.provider.as_str() {
        "hermes" => driver.drive_hermes(&plan).await,
        "pi" => driver.drive_pi(&plan).await,
        "omp" => driver.drive_omp(&plan).await,
        "empryo" => driver.drive_empryo(&plan).await,
        "dsh" => driver.drive_dsh(&plan).await,
        "antigravity" => {
            async {
                if !plan.model.is_empty() {
                    driver.drive_provider(&plan).await?;
                }
                driver
                    .drive_options(&plan, SessionChatOptionAgent::Antigravity)
                    .await
            }
            .await
        }
        _ => driver.drive_provider(&plan).await,
    };
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
