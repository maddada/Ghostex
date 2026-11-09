//! Export Team: the whole team, one department or one GBot (as a preset), with or without starter
//! notes; then where it went and what was left out.
use super::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{ClickEvent, FontWeight, StatefulInteractiveElement as _};
use gpui_component::v_flex;
use serde_json::json;

const SCOPES: [&str; 3] = ["Whole team", "A department", "One GBot"];
const NEVER_IN: &str = "Secrets are redacted. Chats, the agent and model each GBot runs on, approval levels, computers, keys and sign-ins never go in a team file.";

/// What Export writes.
#[derive(Default)]
pub(crate) struct ExportChoice {
    /// An index into [`SCOPES`].
    pub(crate) scope: usize,
    /// The department or GBot picked for the scope.
    pub(crate) pick: Option<String>,
    pub(crate) notes: bool,
}

impl GpuiGbotTeamModalWindow {
    /// The departments or GBots the scope picks from: `(id, name, detail, look)`.
    fn choices(&self) -> Vec<(String, String, String, Option<Value>)> {
        match self.export.scope {
            1 => self.list["departments"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|department| {
                    let members = department["members"].as_array().map(Vec::len).unwrap_or(0);
                    (
                        text_of(department, "departmentId").to_string(),
                        text_of(department, "name").to_string(),
                        plural(members, "GBot", "GBots"),
                        None,
                    )
                })
                .collect(),
            2 => self.list["bots"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|bot| {
                    (
                        text_of(bot, "botId").to_string(),
                        text_of(bot, "name").to_string(),
                        text_of(bot, "title").to_string(),
                        Some(bot.clone()),
                    )
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn export_params(&self, cx: &App) -> Value {
        let mut params = json!({ "op": "teamExport", "notes": self.export.notes });
        match (self.export.scope, &self.export.pick) {
            (1, Some(department)) => params["department"] = json!(department),
            (2, Some(bot)) => params["preset"] = json!(bot),
            _ => {
                let name = self.name_input.read(cx).value().trim().to_string();
                if !name.is_empty() {
                    params["name"] = json!(name);
                }
            }
        }
        params
    }

    fn ready(&self) -> bool {
        self.busy.is_none() && (self.export.scope == 0 || self.export.pick.is_some())
    }

    pub(super) fn render_export(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, AnyElement) {
        if let Stage::Exported { answer, saved } = &self.stage {
            let (answer, saved) = (answer.clone(), *saved);
            return self.render_exported(&answer, saved, cx);
        }
        let p = self.palette;
        let items: Vec<ModalSegmentedItem> = SCOPES
            .iter()
            .map(|label| ModalSegmentedItem { icon: None, label })
            .collect();
        let mut body: Vec<AnyElement> = vec![modal_segmented_control(
            &p,
            "gbot-team-scope",
            &items,
            self.export.scope,
            |this: &mut Self, index, _window, cx| {
                this.export.scope = index;
                this.export.pick = None;
                this.fit.refit();
                cx.notify();
            },
            cx,
        )];
        if self.export.scope == 0 {
            let bots = self.list["bots"].as_array().map(Vec::len).unwrap_or(0);
            let departments = self.list["departments"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            body.push(
                v_flex()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, "Team name"))
                    .child(modal_text_input(&p, &self.name_input, false, window, cx))
                    .child(modal_hint(
                        &p,
                        format!(
                            "{} and {}, with their routines.",
                            plural(bots, "GBot", "GBots"),
                            plural(departments, "department", "departments")
                        ),
                    ))
                    .into_any_element(),
            );
        } else {
            let choices = self.choices();
            if choices.is_empty() {
                body.push(
                    modal_hint(
                        &p,
                        if self.export.scope == 1 {
                            "There are no departments yet."
                        } else {
                            "There are no GBots yet."
                        },
                    )
                    .into_any_element(),
                );
            } else {
                body.push(
                    modal_panel(&p)
                        .children(choices.into_iter().enumerate().map(
                            |(index, (id, name, detail, look))| {
                                let picked = self.export.pick.as_deref() == Some(id.as_str());
                                h_flex()
                                    .id(("gbot-team-pick", index))
                                    .w_full()
                                    .gap(px(10.0))
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .cursor_pointer()
                                    .when(index > 0, |row| {
                                        row.border_t_1().border_color(p.hairline)
                                    })
                                    .when(picked, |row| row.bg(p.raised))
                                    .hover(|row| row.bg(p.raised_hover))
                                    .children(look.as_ref().map(bot_icon))
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(name),
                                            )
                                            .when(!detail.is_empty(), |column| {
                                                column.child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .text_color(p.muted)
                                                        .child(detail),
                                                )
                                            }),
                                    )
                                    .when(picked, |row| {
                                        row.child(modal_icon(
                                            "titlebar/check.svg",
                                            14.0,
                                            p.foreground,
                                        ))
                                    })
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _window, cx| {
                                            this.export.pick = Some(id.clone());
                                            cx.notify();
                                        },
                                    ))
                            },
                        ))
                        .into_any_element(),
                );
            }
            if self.export.scope == 2 {
                body.push(
                    modal_hint(&p, "One GBot is written as a preset: New GBot can start from it on any Ghostex.")
                        .into_any_element(),
                );
            }
        }
        let notes = self.export.notes;
        body.push(
            modal_panel(&p)
                .child(
                    div()
                        .id("gbot-team-notes")
                        .cursor_pointer()
                        .child(modal_panel_row(
                            &p,
                            "Include starter notes",
                            Some("Each GBot's memory and topic notes, with secrets redacted."),
                            modal_switch(&p, notes, false),
                            false,
                        ))
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.export.notes = !this.export.notes;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        );
        body.push(modal_hint(&p, NEVER_IN).into_any_element());
        body.extend(self.error.clone().map(|error| modal_error(&p, error)));
        let content = vec![
            modal_header(
                &p,
                "Export Team",
                Some(
                    "Write your GBots to one JSON file you can import on another Ghostex, or keep as a copy.",
                ),
            ),
            v_flex().gap(px(12.0)).children(body).into_any_element(),
        ];
        let ready = self.ready();
        let mut buttons = vec![modal_action_button(
            &p,
            "gbot-team-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            false,
            |this: &mut Self, window, cx| this.close(window, cx),
            cx,
        )];
        if self.export.scope == 2 {
            buttons.push(modal_action_button(
                &p,
                "gbot-team-keep-bot-preset",
                if self.busy == Some(TeamOp::KeepBotPreset) {
                    "Keeping…"
                } else {
                    "Keep as Preset"
                },
                None,
                ModalButtonTone::Neutral,
                !ready,
                |this: &mut Self, window, cx| {
                    let bot = this.export.pick.clone().unwrap_or_default();
                    let notes = this.export.notes;
                    this.send(
                        TeamOp::KeepBotPreset,
                        |reply| GbotTeamModalCommand::KeepBotPreset { bot, notes, reply },
                        window,
                        cx,
                    );
                },
                cx,
            ));
        }
        buttons.push(modal_action_button(
            &p,
            "gbot-team-copy",
            if self.busy == Some(TeamOp::Export { saved: false }) {
                "Copying…"
            } else {
                "Copy"
            },
            None,
            if self.can_save {
                ModalButtonTone::Neutral
            } else {
                ModalButtonTone::Primary
            },
            !ready,
            |this: &mut Self, window, cx| {
                let params = this.export_params(cx);
                this.send(
                    TeamOp::Export { saved: false },
                    |reply| GbotTeamModalCommand::Export {
                        params,
                        save: false,
                        reply,
                    },
                    window,
                    cx,
                );
            },
            cx,
        ));
        if self.can_save {
            buttons.push(modal_action_button(
                &p,
                "gbot-team-save",
                if self.busy == Some(TeamOp::Export { saved: true }) {
                    "Saving…"
                } else {
                    "Save…"
                },
                None,
                ModalButtonTone::Primary,
                !ready,
                |this: &mut Self, window, cx| {
                    let params = this.export_params(cx);
                    this.send(
                        TeamOp::Export { saved: true },
                        |reply| GbotTeamModalCommand::Export {
                            params,
                            save: true,
                            reply,
                        },
                        window,
                        cx,
                    );
                },
                cx,
            ));
        }
        (content, modal_footer(buttons))
    }

    fn render_exported(
        &mut self,
        answer: &Value,
        saved: bool,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, AnyElement) {
        let p = self.palette;
        let what = if text_of(answer, "kind") == "preset" {
            "1 GBot as a preset".to_string()
        } else {
            let mut what = plural(
                answer["bots"].as_u64().unwrap_or(0) as usize,
                "GBot",
                "GBots",
            );
            let departments = answer["departments"].as_u64().unwrap_or(0) as usize;
            if departments > 0 {
                what.push_str(&format!(
                    ", {}",
                    plural(departments, "department", "departments")
                ));
            }
            let routines = answer["routines"].as_u64().unwrap_or(0) as usize;
            if routines > 0 {
                what.push_str(&format!(", {}", plural(routines, "routine", "routines")));
            }
            what
        };
        let (title, description) = if saved {
            (
                "Team file saved".to_string(),
                format!("{what} written to {}.", text_of(answer, "savedTo")),
            )
        } else {
            (
                "Team file copied".to_string(),
                format!(
                    "{what} copied as JSON. Paste it into a file named {}.",
                    text_of(answer, "fileName")
                ),
            )
        };
        let mut body = Vec::new();
        body.extend(line_list(
            &p,
            "Left out",
            &strings(&answer["leftOut"]),
            None,
        ));
        if answer["routinesArrivePaused"].as_bool() == Some(true) {
            body.push(
                modal_hint(&p, "Its routines arrive paused where it is imported.")
                    .into_any_element(),
            );
        }
        body.push(modal_hint(&p, NEVER_IN).into_any_element());
        let content = vec![
            modal_header(&p, title, Some(description)),
            v_flex().gap(px(10.0)).children(body).into_any_element(),
        ];
        (content, modal_footer(vec![self.done_button(cx)]))
    }
}
