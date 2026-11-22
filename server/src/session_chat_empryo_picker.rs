//! CDXC:AgentProviders 2026-10-06 DECISION:
//! Sven (Empryo harness spec): "Model and effort. … Mid-session: a picker driver that types `/models` and `/effort <level>`. The model list comes from `empryo --list-models`."
//!
//! CDXC:AgentProviders 2026-10-06 WHY:
//! Empryo 3.9.0-beta's `/models` ignores an argument and opens its model panel, whose search is fuzzy and whose rows show display names, never ids: searching `subscriptions/claude-opus-5` highlights Claude Opus 5.5 first. So the driver searches the exact `provider/id`, moves the highlight to the one row whose name spells that id (`Claude Opus 5` is `claude-opus-5`, a dated id such as `claude-haiku-4-5-20251001` is `Claude Haiku 4.5`), presses Enter, and counts the pick only once the statusline names the model. Enter there also saves the model as the default of the config scope that holds `defaultModel` (measured: the repo's `.empryo/config.json`), which is why Empryo's picker is not session-only. `/effort <level>` sets the tab's effort for the model's family at once; a level the model lacks never reaches the statusline, so the refusal names the ladder Empryo's `/effort` panel shows. Keys are typed, never pasted: Empryo answers a bracketed paste by also attaching the clipboard's image.
//! SEE-ALSO: server/src/session_chat_pi_models.rs (the lineup), server/src/session_chat_options/agent_matchers.rs `match_empryo_statusline`.

use super::*;
use crate::session_chat_pi_models::{
    empryo_name_spells_id, empryo_shown_names_value, pi_family_terminal_labels, PiFamilyAgent,
};

const EMPRYO_MODELS_COMMAND: &str = "/models\r";
const EMPRYO_EFFORT_PANEL_COMMAND: &str = "/effort\r";
/// The model panel lists providers live, and opening it can take a moment on a cold engine.
const EMPRYO_PANEL_TIMEOUT_MS: u64 = 10_000;
/// One press per search result, with slack.
const EMPRYO_ROW_STEP_LIMIT: usize = 12;
/// The first Escape clears the panel's search, the second closes it.
const EMPRYO_PANEL_CANCEL_ESCAPES: usize = 2;

struct EmpryoResultRow {
    name: String,
    focused: bool,
}

/// The search query and result rows of Empryo's open model panel (`🔍 <query>`, then
/// `▾ Results N` and one `├─`/`╰─` row each, `▸` on the focused one), or `None` while it is closed.
fn empryo_model_search(screen: &str) -> Option<(String, Vec<EmpryoResultRow>)> {
    let lines: Vec<String> = screen
        .lines()
        .map(crate::session_chat_options::strip_ansi_sgr)
        .collect();
    let title = lines
        .iter()
        .rposition(|line| line.contains("Select Model"))?;
    let search = title
        + lines[title..]
            .iter()
            .position(|line| line.contains('\u{1f50d}'))?;
    let query = lines[search]
        .rsplit_once('\u{1f50d}')?
        .1
        .split("  ")
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let header = search
        + lines[search..]
            .iter()
            .position(|line| line.contains(" Results "))?;
    let mut rows = Vec::new();
    for line in &lines[header + 1..] {
        // CDXC:AgentScreenDetection 2026-10-07 WHY: the panel is a drawer right of a `┃` border, and the conversation beside it draws its own `├─`/`╰─` tool and event rows on the same screen lines; reading the whole line took those for results (seen live 2026-10-07: "Waiting for Genome…" read as the only row, so no row was ever focused and every model pick timed out).
        let line = line
            .rsplit_once('\u{2503}')
            .map_or(line.as_str(), |(_, panel)| panel);
        let Some(at) = line
            .rfind("\u{251c}\u{2500}")
            .or_else(|| line.rfind("\u{2570}\u{2500}"))
        else {
            break;
        };
        let lead = &line[..at];
        let focused = lead
            .rsplit_once('\u{2502}')
            .map_or(lead, |(_, lead)| lead)
            .contains('\u{25b8}');
        let text: String = line[at + "\u{251c}\u{2500}".len()..]
            .chars()
            .map(|ch| {
                if crate::session_chat_options::is_nerd_font_icon(ch) {
                    ' '
                } else {
                    ch
                }
            })
            .collect();
        let name = text
            .split("  ")
            .map(str::trim)
            .find(|chunk| !chunk.is_empty())
            .unwrap_or_default()
            .to_string();
        rows.push(EmpryoResultRow { name, focused });
    }
    Some((query, rows))
}

/// Whether Empryo's model panel is open.
fn empryo_model_panel_open(screen: &str) -> bool {
    screen
        .lines()
        .any(|line| line.contains("Select Model") && !line.contains("/models"))
        && screen.contains('\u{1f50d}')
}

/// The levels Empryo's `/effort` panel offers for the current model
/// (`· off ─ ∙ low ─ ● medium ─ ◉ high ─ ◈ xhigh ─ ✶ max•`).
fn empryo_effort_ladder(screen: &str) -> Option<Vec<String>> {
    let lines = screen_lines(screen);
    let title = lines
        .iter()
        .rposition(|line| line.contains("Effort \u{2301}"))?;
    let row = lines.get(title + 1)?;
    let row = row.trim().trim_matches('\u{2502}').trim();
    let ladder: Vec<String> = row
        .split('\u{2500}')
        .filter_map(|step| {
            let word = step.split_whitespace().last()?.trim_end_matches('\u{2022}');
            (!word.is_empty() && word.chars().all(|ch| ch.is_ascii_lowercase()))
                .then(|| word.to_string())
        })
        .collect();
    (ladder.len() >= 2).then_some(ladder)
}

/// The model and effort Empryo's statusline shows.
fn empryo_footer(screen: &str) -> Option<(String, Option<String>)> {
    let selection = detect_session_chat_selection(SessionChatOptionAgent::Empryo, screen)?;
    Some((
        selection.model?.value,
        selection.effort.map(|effort| effort.value),
    ))
}

/// Whether the screen's model, `<provider name>/<id>` (3.9.0-beta) or `<vendor>/<display name>`
/// (3.9.1-beta), is `model` (`provider/id`).
fn empryo_shows_model(shown: &str, model: &str, labels: &[String]) -> bool {
    labels.iter().any(|label| label.eq_ignore_ascii_case(shown))
        || empryo_shown_names_value(shown, model)
}

/// Empryo only: the session's `session.jsonl`, whose `meta.json` names the tab's model.
pub(crate) fn plan_session_log(
    state: &AppState,
    agent: Option<&str>,
    session: &Value,
) -> Option<std::path::PathBuf> {
    (agent == Some("empryo"))
        .then(|| crate::storage::open_gxserver_database(&state.paths).ok())
        .flatten()
        .and_then(|db| {
            let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
            crate::session_chat_pi_models::empryo_session_log(&repository, session)
        })
}

impl PickerDriver<'_> {
    pub(super) async fn drive_empryo(
        &self,
        plan: &CodexPickerPlan,
    ) -> Result<(), DomainStateError> {
        let id = plan
            .model
            .split_once('/')
            .map(|(_, id)| id)
            .filter(|id| !id.is_empty() && !plan.model.starts_with('/'))
            .ok_or_else(|| invalid_params("An Empryo model is picked as provider/id."))?;
        let labels = pi_family_terminal_labels(PiFamilyAgent::Empryo, &plan.model);
        self.composer_ready("empryo").await?;
        let shown = self
            .capture()
            .await
            .and_then(|screen| empryo_footer(&screen));
        // CDXC:AgentProviders 2026-10-06 WHY:
        // Empryo paints its config's default model for a moment while it restores a `--session` tab, so a pick read off that first frame typed `/models` for a model the tab already had and saved it as the default (seen live on a seeded launch). The tab's own `meta.json` is exact, so it decides first and the statusline only when there is none.
        let recorded = plan
            .empryo_session_log
            .as_deref()
            .and_then(crate::session_chat_pi_models::empryo_tab_model_at);
        let switched = match recorded {
            Some(recorded) => recorded != plan.model,
            None => !shown
                .as_ref()
                .is_some_and(|(model, _)| empryo_shows_model(model, &plan.model, &labels)),
        };
        if switched {
            let result = self.empryo_switch_model(plan, id, &labels).await;
            if result.is_err() {
                self.empryo_close_panel().await;
            }
            result?;
        }
        if plan.effort.is_empty() {
            return Ok(());
        }
        let level = self
            .capture()
            .await
            .and_then(|screen| empryo_footer(&screen))
            .and_then(|(_, effort)| effort);
        if level.as_deref() == Some(plan.effort.as_str()) {
            return Ok(());
        }
        self.composer_ready("empryo").await?;
        self.write(&format!("/effort {}\r", plan.effort)).await?;
        match self
            .wait_for("applied effort", |screen| {
                empryo_footer(screen)
                    .is_some_and(|(_, effort)| effort.as_deref() == Some(plan.effort.as_str()))
                    .then_some(())
            })
            .await
        {
            Ok(()) => Ok(()),
            Err(_) => Err(self.empryo_effort_refusal(plan, switched).await),
        }
    }

    async fn empryo_switch_model(
        &self,
        plan: &CodexPickerPlan,
        id: &str,
        labels: &[String],
    ) -> Result<(), DomainStateError> {
        self.write(EMPRYO_MODELS_COMMAND).await?;
        self.wait_for_within("Empryo model panel", EMPRYO_PANEL_TIMEOUT_MS, |screen| {
            empryo_model_panel_open(screen).then_some(())
        })
        .await?;
        self.write(&plan.model).await?;
        // The results draw a frame before their focus marker, so wait for both.
        let mut rows = self
            .wait_for("Empryo model search", |screen| {
                empryo_model_search(screen)
                    .filter(|(query, rows)| {
                        query == &plan.model && rows.iter().any(|row| row.focused)
                    })
                    .map(|(_, rows)| rows)
            })
            .await?;
        for _ in 0..EMPRYO_ROW_STEP_LIMIT {
            let matching: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| empryo_name_spells_id(&row.name, id))
                .map(|(index, _)| index)
                .collect();
            let target = match matching.as_slice() {
                [target] => *target,
                [] => {
                    return Err(unsupported_selection(format!(
                        "Empryo's model list does not show {}.",
                        plan.model
                    )));
                }
                _ => {
                    return Err(unsupported_selection(format!(
                        "Empryo's model list shows more than one row named like {}.",
                        plan.model
                    )));
                }
            };
            let focused = rows
                .iter()
                .position(|row| row.focused)
                .ok_or_else(|| agent_busy("Empryo's model panel lost its highlighted row."))?;
            if focused == target {
                self.write(CODEX_SUBMIT).await?;
                return self
                    .wait_for_within("applied model", EMPRYO_PANEL_TIMEOUT_MS, |screen| {
                        (!empryo_model_panel_open(screen)
                            && empryo_footer(screen).is_some_and(|(model, _)| {
                                empryo_shows_model(&model, &plan.model, labels)
                            }))
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
                .wait_for("Empryo model focus", |screen| {
                    let (_, rows) = empryo_model_search(screen)?;
                    matches!(rows.iter().position(|row| row.focused), Some(now) if now != focused)
                        .then_some(rows)
                })
                .await?;
        }
        Err(picker_timeout("Empryo model focus"))
    }

    /// Why Empryo kept its effort: the levels its `/effort` panel offers for the current model,
    /// read and closed again, or that the model takes none. Says so when this pick did switch the
    /// model, which Empryo keeps.
    async fn empryo_effort_refusal(
        &self,
        plan: &CodexPickerPlan,
        switched: bool,
    ) -> DomainStateError {
        if (self.cancelled)() || self.composer_ready("empryo").await.is_err() {
            return picker_timeout("applied effort");
        }
        if self.write(EMPRYO_EFFORT_PANEL_COMMAND).await.is_err() {
            return picker_timeout("applied effort");
        }
        let ladder = self
            .wait_for("Empryo effort panel", empryo_effort_ladder)
            .await;
        if self
            .capture()
            .await
            .is_some_and(|screen| screen.contains("Effort \u{2301}"))
        {
            let _ = self.write(PI_SELECTOR_CANCEL).await;
            tokio::time::sleep(Duration::from_millis(PICKER_CANCEL_SETTLE_MS)).await;
        }
        let lead = if switched {
            format!("Empryo switched to {}, but kept its effort: ", plan.model)
        } else {
            String::new()
        };
        match ladder {
            Ok(ladder) => unsupported_selection(format!(
                "{lead}Empryo offers {} for {}, not {}.",
                ladder.join(", "),
                plan.model,
                plan.effort
            )),
            Err(_) => unsupported_selection(format!(
                "{lead}Empryo has no effort control for {}.",
                plan.model
            )),
        }
    }

    /// Closes a model panel a failed pick left open, without applying anything.
    async fn empryo_close_panel(&self) {
        if (self.cancelled)() {
            return;
        }
        for _ in 0..EMPRYO_PANEL_CANCEL_ESCAPES {
            if !self
                .capture()
                .await
                .is_some_and(|screen| empryo_model_panel_open(&screen))
            {
                return;
            }
            let _ = self.write(PI_SELECTOR_CANCEL).await;
            tokio::time::sleep(Duration::from_millis(PICKER_CANCEL_SETTLE_MS)).await;
        }
    }
}
