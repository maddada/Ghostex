//! Import Team: the paste box (web), the file's preview, then the import (or the preset kept)
//! with Undo.
use super::*;
use gpui::FontWeight;
use gpui::prelude::FluentBuilder as _;
use gpui_component::v_flex;

const ADDING_NEEDS_OK: &str =
    "Adding GBots needs your OK: approve it with Touch ID or on your phone.";

impl GpuiGbotTeamModalWindow {
    pub(super) fn render_import(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, AnyElement) {
        let p = self.palette;
        match &self.stage {
            Stage::Paste => self.render_paste(window, cx),
            Stage::Preview => self.render_preview(cx),
            Stage::Imported(answer) => {
                let answer = answer.clone();
                let bots = answer["bots"].as_array().cloned().unwrap_or_default();
                let routines = answer["routines"].as_array().map(Vec::len).unwrap_or(0);
                let mut body = vec![self.bot_panel(&bots, false)];
                if routines > 0 {
                    body.push(
                        modal_hint(
                            &p,
                            format!(
                                "{} arrived paused. Resume them in each GBot's settings, on its Routines tab.",
                                plural(routines, "routine", "routines")
                            ),
                        )
                        .into_any_element(),
                    );
                }
                body.extend(line_list(
                    &p,
                    "Not added",
                    &strings(&answer["skipped"]),
                    None,
                ));
                body.extend(self.error.clone().map(|error| modal_error(&p, error)));
                let mut content = vec![modal_header(
                    &p,
                    format!("Imported {}", text_of(&answer, "name")),
                    Some(format!(
                        "{} added under Chief of Staff in Main Hub.",
                        plural(bots.len(), "GBot", "GBots")
                    )),
                )];
                content.push(v_flex().gap(px(14.0)).children(body).into_any_element());
                let change = text_of(&answer, "changeId").to_string();
                let undo = modal_action_button(
                    &p,
                    "gbot-team-undo",
                    if self.busy == Some(TeamOp::Undo) {
                        "Undoing…"
                    } else {
                        "Undo Import"
                    },
                    None,
                    ModalButtonTone::Neutral,
                    self.busy.is_some(),
                    move |this: &mut Self, window, cx| {
                        let change = change.clone();
                        this.send(
                            TeamOp::Undo,
                            |reply| GbotTeamModalCommand::Undo { change, reply },
                            window,
                            cx,
                        );
                    },
                    cx,
                );
                (content, modal_footer(vec![undo, self.done_button(cx)]))
            }
            Stage::Undone(answer) => {
                let done = &answer["done"];
                let mut lines = Vec::new();
                let removed = strings(&done["removedBots"]);
                if !removed.is_empty() {
                    lines.push(format!(
                        "Removed {}. Their folders are in the Trash.",
                        removed.join(", ")
                    ));
                }
                let routines = done["removedRoutines"].as_u64().unwrap_or(0) as usize;
                if routines > 0 {
                    lines.push(format!(
                        "Removed {}.",
                        plural(routines, "routine", "routines")
                    ));
                }
                let departments = strings(&done["removedDepartments"]);
                if !departments.is_empty() {
                    lines.push(format!(
                        "Removed the department {}.",
                        departments.join(", ")
                    ));
                }
                let kept = strings(&done["keptDepartments"]);
                if !kept.is_empty() {
                    lines.push(format!(
                        "Kept {}: other GBots are in it now.",
                        kept.join(", ")
                    ));
                }
                let content = vec![
                    modal_header(&p, "Import undone", None::<&str>),
                    v_flex()
                        .gap(px(6.0))
                        .children(lines.into_iter().map(|line| modal_hint(&p, line)))
                        .into_any_element(),
                ];
                (content, modal_footer(vec![self.done_button(cx)]))
            }
            Stage::PresetKept(answer) => {
                let preset = &answer["preset"];
                let content = vec![
                    modal_header(
                        &p,
                        format!("Kept the preset {}", text_of(preset, "name")),
                        Some("New GBot offers it under Start from. Nothing was added yet."),
                    ),
                    self.bot_panel(std::slice::from_ref(preset), false),
                ];
                (content, modal_footer(vec![self.done_button(cx)]))
            }
            _ => (Vec::new(), modal_footer(vec![self.done_button(cx)])),
        }
    }

    pub(super) fn done_button(&self, cx: &mut Context<Self>) -> AnyElement {
        modal_action_button(
            &self.palette,
            "gbot-team-done",
            "Done",
            None,
            ModalButtonTone::Primary,
            self.busy.is_some(),
            |this: &mut Self, window, cx| this.close(window, cx),
            cx,
        )
    }

    fn cancel_button(&self, cx: &mut Context<Self>) -> AnyElement {
        modal_action_button(
            &self.palette,
            "gbot-team-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            false,
            |this: &mut Self, window, cx| this.close(window, cx),
            cx,
        )
    }

    fn render_paste(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, AnyElement) {
        let p = self.palette;
        let mut content = vec![
            modal_header(
                &p,
                "Import Team",
                Some(
                    "Paste a GBots team or preset file. You see what it adds before anything is added.",
                ),
            ),
            div()
                .h(px(220.0))
                .child(modal_text_area(
                    &p,
                    &self.paste_input,
                    Some(220.0),
                    self.busy.is_some(),
                    window,
                    cx,
                ))
                .into_any_element(),
        ];
        content.extend(self.error.clone().map(|error| modal_error(&p, error)));
        let empty = self.paste_input.read(cx).value().trim().is_empty();
        let preview = modal_action_button(
            &p,
            "gbot-team-preview",
            if self.busy.is_some() {
                "Reading…"
            } else {
                "Preview"
            },
            None,
            ModalButtonTone::Primary,
            empty || self.busy.is_some(),
            |this: &mut Self, window, cx| {
                let text = this.paste_input.read(cx).value().to_string();
                this.text = text.clone();
                this.send(
                    TeamOp::Preview,
                    |reply| GbotTeamModalCommand::Preview { text, reply },
                    window,
                    cx,
                );
            },
            cx,
        );
        (content, modal_footer(vec![self.cancel_button(cx), preview]))
    }

    /// The GBots of a preview or an import, one row each.
    pub(super) fn bot_panel(&self, bots: &[Value], preview: bool) -> AnyElement {
        let p = self.palette;
        modal_panel(&p)
            .children(bots.iter().enumerate().map(|(index, bot)| {
                let mut detail = Vec::new();
                if !text_of(bot, "title").is_empty() {
                    detail.push(text_of(bot, "title").to_string());
                }
                if let Some(department) = bot["department"].as_str() {
                    let lead = if bot["isLead"].as_bool() == Some(true) {
                        ", lead"
                    } else {
                        ""
                    };
                    detail.push(format!("{department}{lead}"));
                }
                if preview {
                    let notes = &bot["starterNotes"];
                    let lines = notes["memoryLines"].as_u64().unwrap_or(0) as usize;
                    let topics = notes["topics"].as_array().map(Vec::len).unwrap_or(0);
                    if lines + topics > 0 {
                        let mut parts = Vec::new();
                        if lines > 0 {
                            parts.push(plural(lines, "line", "lines"));
                        }
                        if topics > 0 {
                            parts.push(plural(topics, "topic", "topics"));
                        }
                        detail.push(format!("starter notes: {}", parts.join(", ")));
                    }
                    let routines = bot["routines"].as_u64().unwrap_or(0) as usize;
                    if routines > 0 {
                        detail.push(plural(routines, "routine", "routines"));
                    }
                }
                let renamed = bot["renamedFrom"]
                    .as_str()
                    .map(|from| format!("was {from}"));
                h_flex()
                    .w_full()
                    .gap(px(10.0))
                    .px(px(12.0))
                    .py(px(9.0))
                    .when(index > 0, |row| row.border_t_1().border_color(p.hairline))
                    .child(bot_icon(bot))
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap(px(2.0))
                            .child(
                                h_flex()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(text_of(bot, "name").to_string()),
                                    )
                                    .children(renamed.map(|renamed| {
                                        div().text_size(px(12.0)).text_color(p.muted).child(renamed)
                                    })),
                            )
                            .when(!detail.is_empty(), |column| {
                                column.child(
                                    div()
                                        .text_size(px(12.0))
                                        .line_height(px(17.0))
                                        .text_color(p.muted)
                                        .child(detail.join(" · ")),
                                )
                            }),
                    )
            }))
            .into_any_element()
    }

    fn render_preview(&mut self, cx: &mut Context<Self>) -> (Vec<AnyElement>, AnyElement) {
        let p = self.palette;
        let preview = self.preview.clone();
        let preset = text_of(&preview, "kind") == "preset";
        let bots = preview["bots"].as_array().cloned().unwrap_or_default();
        let departments = preview["departments"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let routines = preview["routines"].as_array().cloned().unwrap_or_default();
        let title = if preset {
            format!("Preset: {}", text_of(&preview, "name"))
        } else if text_of(&preview, "name").is_empty() {
            "Import Team".to_string()
        } else {
            format!("Import {}", text_of(&preview, "name"))
        };
        let description = if preset {
            "A GBot New GBot can start from. Keeping it adds no GBot yet.".to_string()
        } else if text_of(&preview, "summary").is_empty() {
            "Nothing is added until you press Import.".to_string()
        } else {
            text_of(&preview, "summary").to_string()
        };
        let mut body: Vec<AnyElement> = vec![
            modal_section_title(
                &p,
                if preset {
                    "GBot".to_string()
                } else {
                    format!("Adds {}", plural(bots.len(), "GBot", "GBots"))
                },
            )
            .into_any_element(),
            self.bot_panel(&bots, true),
        ];
        if !departments.is_empty() {
            body.push(modal_section_title(&p, "Departments").into_any_element());
            body.push(
                modal_panel(&p)
                    .children(departments.iter().enumerate().map(|(index, department)| {
                        let members = department["members"].as_array().map(Vec::len).unwrap_or(0);
                        let lead = department["lead"]
                            .as_str()
                            .map(|lead| format!("led by {lead}, "))
                            .unwrap_or_default();
                        modal_panel_row(
                            &p,
                            text_of(department, "name").to_string(),
                            Some(format!("{lead}{}", plural(members, "GBot", "GBots"))),
                            div().into_any_element(),
                            index > 0,
                        )
                    }))
                    .into_any_element(),
            );
        }
        if !routines.is_empty() {
            body.push(modal_section_title(&p, "Routines (arrive paused)").into_any_element());
            body.push(
                modal_panel(&p)
                    .children(routines.iter().enumerate().map(|(index, routine)| {
                        let mut detail = format!(
                            "{} · {}",
                            text_of(routine, "bot"),
                            text_of(routine, "scheduleSentence")
                        );
                        if let Some(check) = routine["precheckSentence"].as_str() {
                            detail.push_str(&format!(" · when {check} changes"));
                        }
                        modal_panel_row(
                            &p,
                            text_of(routine, "name").to_string(),
                            Some(detail),
                            div().into_any_element(),
                            index > 0,
                        )
                    }))
                    .into_any_element(),
            );
        }
        if !preset {
            body.push(
                modal_hint(&p, text_of(&preview, "approvalSentence").to_string())
                    .into_any_element(),
            );
        }
        body.extend(line_list(
            &p,
            "Read this",
            &strings(&preview["warnings"]),
            Some(p.destructive),
        ));
        body.extend(line_list(&p, "Names", &strings(&preview["notes"]), None));
        body.extend(line_list(
            &p,
            "Left out of the file",
            &strings(&preview["leftOut"]),
            None,
        ));
        body.extend(line_list(
            &p,
            "Ignored (this Ghostex doesn't know these fields)",
            &strings(&preview["ignored"]),
            None,
        ));
        body.extend(line_list(
            &p,
            "Can't import",
            &strings(&preview["problems"]),
            Some(p.destructive),
        ));
        if self.busy == Some(TeamOp::Import) {
            body.push(modal_hint(&p, ADDING_NEEDS_OK).into_any_element());
        }
        body.extend(self.error.clone().map(|error| modal_error(&p, error)));
        let content = vec![
            modal_header(&p, title, Some(description)),
            v_flex().gap(px(10.0)).children(body).into_any_element(),
        ];
        let problems = !strings(&preview["problems"]).is_empty();
        let primary = if preset {
            modal_action_button(
                &p,
                "gbot-team-keep-preset",
                if self.busy.is_some() {
                    "Keeping…"
                } else {
                    "Keep Preset"
                },
                None,
                ModalButtonTone::Primary,
                problems || self.busy.is_some(),
                |this: &mut Self, window, cx| {
                    let text = this.text.clone();
                    this.send(
                        TeamOp::KeepPreset,
                        |reply| GbotTeamModalCommand::KeepPreset { text, reply },
                        window,
                        cx,
                    );
                },
                cx,
            )
        } else {
            let label = if self.busy.is_some() {
                "Importing…".to_string()
            } else {
                format!("Import {}", plural(bots.len(), "GBot", "GBots"))
            };
            modal_action_button(
                &p,
                "gbot-team-import",
                label,
                None,
                ModalButtonTone::Primary,
                preview["canImport"].as_bool() != Some(true) || self.busy.is_some(),
                |this: &mut Self, window, cx| {
                    let text = this.text.clone();
                    this.send(
                        TeamOp::Import,
                        |reply| GbotTeamModalCommand::Import { text, reply },
                        window,
                        cx,
                    );
                },
                cx,
            )
        };
        (content, modal_footer(vec![self.cancel_button(cx), primary]))
    }
}
