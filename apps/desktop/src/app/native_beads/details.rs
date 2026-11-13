//! The details panel beside the tree: the bead's actions, fields, who is on it, what blocks
//! what, its description and comments.

use ghostex_gx_core::beads_view::{
    BeadAgentState, bead_agent_key, parse_bd_time_ms, relative_ago, status_label, type_label,
};
use ghostex_gx_protocol::beads_view::capability;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, ClickEvent, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, px, relative,
};
use gpui_component::date_picker::{DatePicker, DatePickerEvent, DatePickerState};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use serde_json::{Value, json};

use super::menus::{BeadsMenu, BeadsMenuKind};
use super::render::{section_label, small_button};
use super::tree::{label_chip, priority_tag};
use super::view::{BeadsRequest, BeadsView};
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_kanban::widgets::{KanbanButtonKind, kanban_button, text_area, text_field};

pub(crate) struct Details {
    pub(crate) id: String,
    /// bd's `show` answer (description, comments).
    pub(crate) shown: Option<Value>,
    pub(crate) loading: bool,
    /// The bead changed since `shown` was read.
    pub(crate) stale: bool,
    pub(crate) comment: Entity<TextareaState>,
    pub(crate) answer: Option<Entity<TextareaState>>,
    pub(crate) label: Option<Entity<InputState>>,
    pub(crate) date: Option<Entity<DatePickerState>>,
    /// The Links row's open input (links.rs).
    pub(crate) link: Option<(super::links::LinkKind, Entity<InputState>)>,
    pub(crate) _subscriptions: Vec<Subscription>,
}

fn shown_text(shown: Option<&Value>, key: &str) -> String {
    shown
        .and_then(|shown| shown.get(key))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl BeadsView {
    pub(crate) fn open_details(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.create = None;
        if self
            .details
            .as_ref()
            .is_some_and(|details| details.id == id)
        {
            return;
        }
        let comment = cx.new(|cx| TextareaState::new(window, cx).placeholder("Add a comment"));
        let subscription = cx.subscribe(&comment, |_, _, _: &InputEvent, cx| cx.notify());
        self.details = Some(Details {
            id: id.to_string(),
            shown: None,
            loading: true,
            stale: false,
            comment,
            answer: None,
            label: None,
            date: None,
            link: None,
            _subscriptions: vec![subscription],
        });
        self.load_details(cx);
        cx.notify();
    }

    pub(crate) fn close_details(&mut self, cx: &mut Context<Self>) {
        if self.details.take().is_some() {
            cx.notify();
        }
    }

    /// Reads the bead's description and comments (`show`).
    pub(crate) fn load_details(&mut self, cx: &mut Context<Self>) {
        let (Some(details), Some(target)) = (self.details.as_mut(), self.target.clone()) else {
            return;
        };
        details.stale = false;
        let id = details.id.clone();
        let generation = self.generation;
        let params = json!({
            "action": "show",
            "issueId": id,
            "projectId": target.rpc_project_id,
            "projectPath": target.project_path,
            "projectBoardScope": true,
        });
        cx.spawn(async move |this, cx| {
            let answer =
                crate::app::gx_store::gx_rpc(target.remote.clone(), "/api/runBeadsAction", params)
                    .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                let Some(details) = this.details.as_mut().filter(|details| details.id == id) else {
                    return;
                };
                details.loading = false;
                match answer {
                    Ok(reply) => {
                        let issue = reply.get("issue").cloned().or_else(|| {
                            reply
                                .get("stdout")
                                .and_then(Value::as_str)
                                .and_then(|stdout| serde_json::from_str::<Value>(stdout).ok())
                                .map(|value| value.get("data").cloned().unwrap_or(value))
                                .map(|value| match value {
                                    Value::Array(mut items) if !items.is_empty() => items.remove(0),
                                    other => other,
                                })
                        });
                        if reply.get("exitCode").and_then(Value::as_i64) != Some(0) {
                            let said = [reply.get("stderr"), reply.get("stdout")]
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(" ")
                                .to_lowercase();
                            if said.contains("no issue found") || said.contains("no issues found") {
                                // CDXC:ProjectBoard 2026-11-11 WHY: only bd's own "no issue found" closes the details (the bead was deleted elsewhere); any other failure (a timeout, a refusal) keeps the panel and its unsent comment or answer, and says the read failed.
                                this.details = None;
                                this.notice = Some(super::view::BeadsNotice {
                                    title: "That bead is gone".to_string(),
                                    message: "It was deleted or renamed elsewhere.".to_string(),
                                    details: None,
                                    pull_now: false,
                                });
                            } else {
                                this.notice = Some(super::cards::write_failure(
                                    "Couldn't read this bead",
                                    &reply,
                                ));
                            }
                        } else {
                            details.shown = issue;
                        }
                    }
                    Err(error) => {
                        this.notice = Some(super::view::BeadsNotice {
                            title: "Couldn't read this bead".to_string(),
                            message: error.message,
                            details: None,
                            pull_now: false,
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn start_answer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(details) = self.details.as_mut() else {
            return;
        };
        if details.answer.is_none() {
            let answer = cx.new(|cx| TextareaState::new(window, cx).placeholder("Your answer"));
            details
                ._subscriptions
                .push(cx.subscribe(&answer, |_, _, _: &InputEvent, cx| cx.notify()));
            answer.update(cx, |input, cx| input.focus(window, cx));
            details.answer = Some(answer);
        }
        cx.notify();
    }

    fn send_answer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(details) = self.details.as_ref() else {
            return;
        };
        let Some(answer) = details.answer.as_ref() else {
            return;
        };
        let text = answer.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let id = details.id.clone();
        self.run_action(
            json!({ "action": "humanRespond", "issueId": id, "response": text }),
            "Couldn't send the answer",
            window,
            cx,
            |this, _, _, cx| {
                if let Some(details) = this.details.as_mut() {
                    details.answer = None;
                }
                this.load_details(cx);
            },
        );
    }

    fn send_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(details) = self.details.as_ref() else {
            return;
        };
        let text = details.comment.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let id = details.id.clone();
        self.run_action(
            json!({ "action": "comment", "issueId": id, "comment": text }),
            "Couldn't add the comment",
            window,
            cx,
            |this, _, window, cx| {
                if let Some(details) = this.details.as_ref() {
                    details
                        .comment
                        .update(cx, |input, cx| input.set_value("", window, cx));
                }
                this.load_details(cx);
            },
        );
    }

    fn start_label(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(details) = self.details.as_mut() else {
            return;
        };
        if details.label.is_none() {
            let label = cx.new(|cx| InputState::new(window, cx).placeholder("Label, then Enter"));
            details._subscriptions.push(cx.subscribe_in(
                &label,
                window,
                |this: &mut Self, input, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        let label = input.read(cx).value().trim().to_string();
                        if let (false, Some(id)) = (
                            label.is_empty(),
                            this.details.as_ref().map(|details| details.id.clone()),
                        ) {
                            if let Some(details) = this.details.as_mut() {
                                details.label = None;
                            }
                            this.simple_action(
                                "addLabel",
                                &id,
                                json!({ "label": label }),
                                "Couldn't add the label",
                                window,
                                cx,
                            );
                        }
                    }
                },
            ));
            label.update(cx, |input, cx| input.focus(window, cx));
            details.label = Some(label);
        }
        cx.notify();
    }

    pub(crate) fn defer(
        &mut self,
        id: &str,
        until: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(details) = self.details.as_mut() {
            details.date = None;
        }
        let extra = match until {
            Some(until) => json!({ "until": until }),
            None => json!({}),
        };
        self.simple_action("defer", id, extra, "Couldn't defer this bead", window, cx);
    }

    pub(crate) fn open_defer_picker(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.details.as_ref().is_none_or(|details| details.id != id) {
            self.open_details(id, window, cx);
        }
        let Some(details) = self.details.as_mut() else {
            return;
        };
        let picker = cx.new(|cx| DatePickerState::new(window, cx).date_format("%Y-%m-%d"));
        let picked_id = id.to_string();
        details._subscriptions.push(cx.subscribe_in(
            &picker,
            window,
            move |this: &mut Self, picker, _: &DatePickerEvent, window, cx| {
                let date = picker.read(cx).date().start();
                if let Some(date) = date {
                    this.defer(
                        &picked_id,
                        Some(date.format("%Y-%m-%d").to_string()),
                        window,
                        cx,
                    );
                }
            },
        ));
        details.date = Some(picker);
        cx.notify();
    }

    pub(crate) fn render_details(
        &mut self,
        p: &KanbanPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let details = self.details.as_ref()?;
        let id = details.id.clone();
        let row = self.row(&id)?.clone();
        if details.stale && !details.loading {
            self.load_details(cx);
        }
        let details = self.details.as_ref()?;
        let shown = details.shown.clone();
        let closed = row.status == "closed";
        let deferred = row.status == "deferred";
        let defer_ok = self.has_capability(capability::DEFER);
        let reopen_ok = self.has_capability(capability::REOPEN);
        let human_ok = self.has_capability(capability::HUMAN);
        let actor = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.workspace.actor.clone());
        let epic = row
            .parent
            .as_deref()
            .and_then(|parent| self.row(parent))
            .filter(|parent| parent.issue_type == "epic")
            .cloned();
        let epic_progress = epic.as_ref().map(|epic| {
            let children: Vec<_> = self
                .rows
                .iter()
                .filter(|child| child.parent.as_deref() == Some(epic.id.as_str()))
                .collect();
            let done = children
                .iter()
                .filter(|child| child.status == "closed")
                .count();
            (done, children.len())
        });
        let agents = self
            .agents
            .get(&bead_agent_key(&id))
            .cloned()
            .unwrap_or_default();
        let titles = |ids: &[String]| -> Vec<(String, String, String)> {
            ids.iter()
                .map(|other| {
                    let found = self.row(other);
                    (
                        other.clone(),
                        found.map(|row| row.title.clone()).unwrap_or_default(),
                        found
                            .map(|row| status_label(&row.status))
                            .unwrap_or_default(),
                    )
                })
                .collect()
        };
        let blocked_by = titles(row.blocked_by.as_deref().unwrap_or_default());
        let blocks = titles(&row.blocks);
        let description = shown_text(shown.as_ref(), "description");
        let comments: Vec<Value> = shown
            .as_ref()
            .and_then(|shown| shown.get("comments"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let clock = self.clock;
        let answer = details.answer.clone();
        let label_input = details.label.clone();
        let date = details.date.clone();
        let comment_box = details.comment.clone();
        let loading = details.loading;

        let close_id = id.clone();
        let reopen_id = id.clone();
        let defer_id = id.clone();
        let undefer_id = id.clone();
        let menu_id = id.clone();
        let priority_id = id.clone();
        let copy_id = id.clone();
        let hover = p.card_hover;

        let field_row = |label: &'static str, value: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .min_h(px(30.0))
                .child(
                    div()
                        .w(px(96.0))
                        .flex_none()
                        .text_size(px(13.0))
                        .text_color(p.muted)
                        .child(label),
                )
                .child(div().flex_1().min_w_0().child(value))
        };

        let relation_rows = |items: Vec<(String, String, String)>, cx: &mut Context<Self>| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .children(items.into_iter().map(|(other, title, status)| {
                    let select = other.clone();
                    div()
                        .id(SharedString::from(format!("beads-rel-{other}")))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(10.0))
                        .py(px(7.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(p.border)
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .child(titlebar_svg_icon(
                            "titlebar/barrier-block.svg",
                            13.0,
                            p.muted,
                        ))
                        .child(
                            div()
                                .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                                .text_size(px(11.0))
                                .text_color(p.faint)
                                .child(other.clone()),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(title))
                        .child(div().text_size(px(12.0)).text_color(p.muted).child(status))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_details(&select, window, cx)
                        }))
                }))
        };

        let header = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .pb(px(14.0))
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                            .text_size(px(12.0))
                            .text_color(p.muted)
                            .child(id.clone()),
                    )
                    .child(div().text_color(p.faint).child("·"))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(p.muted)
                            .child(type_label(&row.issue_type)),
                    )
                    .child(
                        small_button("beads-details-copy", Some("titlebar/copy.svg"), None, p)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    copy_id.clone(),
                                ))
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .px(px(10.0))
                            .h(px(24.0))
                            .rounded_full()
                            .border_1()
                            .border_color(p.border)
                            .text_size(px(12.0))
                            .text_color(p.foreground)
                            .child(titlebar_svg_icon(
                                super::tree::glyph_icon(
                                    self.tree_glyph_for(&id)
                                        .unwrap_or(ghostex_gx_core::beads_view::BeadGlyph::Open),
                                ),
                                12.0,
                                p.muted,
                            ))
                            .child(status_label(&row.status)),
                    )
                    // In a narrow panel the back bar above returns to the tree, so no second ✕.
                    .when(!self.is_narrow(), |this| {
                        this.child(
                            small_button("beads-details-close", Some("titlebar/x.svg"), None, p)
                                .on_click(cx.listener(|this, _, _, cx| this.close_details(cx))),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(row.title.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(!closed, |this| {
                        this.child(
                            kanban_button(
                                "beads-details-close-bead",
                                Some("titlebar/check.svg"),
                                Some("Close".into()),
                                KanbanButtonKind::Primary,
                                self.writes_in_flight > 0,
                                p,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.simple_action(
                                        "close",
                                        &close_id,
                                        json!({}),
                                        "Couldn't close this bead",
                                        window,
                                        cx,
                                    )
                                },
                            )),
                        )
                    })
                    .when(closed && reopen_ok, |this| {
                        this.child(
                            kanban_button(
                                "beads-details-reopen",
                                Some("titlebar/rotate-clockwise.svg"),
                                Some("Reopen".into()),
                                KanbanButtonKind::Primary,
                                self.writes_in_flight > 0,
                                p,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.simple_action(
                                        "reopen",
                                        &reopen_id,
                                        json!({}),
                                        "Couldn't reopen this bead",
                                        window,
                                        cx,
                                    )
                                },
                            )),
                        )
                    })
                    .when(!closed && !deferred && defer_ok, |this| {
                        this.child(
                            kanban_button(
                                "beads-details-defer",
                                Some("titlebar/clock.svg"),
                                Some("Defer".into()),
                                KanbanButtonKind::Secondary,
                                false,
                                p,
                            )
                            .child(titlebar_svg_icon(
                                "titlebar/chevron-down.svg",
                                12.0,
                                p.muted,
                            ))
                            .on_click(cx.listener(
                                move |this, event: &ClickEvent, _, cx| {
                                    this.open_menu(
                                        BeadsMenu::new(
                                            BeadsMenuKind::Defer(defer_id.clone()),
                                            event.position(),
                                        ),
                                        cx,
                                    )
                                },
                            )),
                        )
                    })
                    .when(deferred && defer_ok, |this| {
                        this.child(
                            kanban_button(
                                "beads-details-undefer",
                                Some("titlebar/player-play.svg"),
                                Some("Bring back".into()),
                                KanbanButtonKind::Secondary,
                                false,
                                p,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.simple_action(
                                        "undefer",
                                        &undefer_id,
                                        json!({}),
                                        "Couldn't bring this bead back",
                                        window,
                                        cx,
                                    )
                                },
                            )),
                        )
                    })
                    .child(
                        small_button("beads-details-menu", Some("titlebar/dots.svg"), None, p)
                            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                                this.open_menu(
                                    BeadsMenu::new(
                                        BeadsMenuKind::Row(menu_id.clone()),
                                        event.position(),
                                    ),
                                    cx,
                                )
                            })),
                    ),
            )
            .when_some(date, |this, date| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(13.0))
                        .text_color(p.muted)
                        .child("Defer until")
                        .child(div().w(px(180.0)).child(DatePicker::new(&date))),
                )
            });

        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .pt(px(14.0))
            .child(field_row(
                "Priority",
                div()
                    .id("beads-details-priority")
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .child(priority_tag(
                        &row.priority
                            .map(|priority| format!("P{priority}"))
                            .unwrap_or_else(|| "—".to_string()),
                        p,
                    ))
                    .child(titlebar_svg_icon(
                        "titlebar/chevron-down.svg",
                        12.0,
                        p.muted,
                    ))
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        this.open_menu(
                            BeadsMenu::new(
                                BeadsMenuKind::Priority(priority_id.clone()),
                                event.position(),
                            ),
                            cx,
                        )
                    }))
                    .into_any_element(),
            ))
            .child(field_row(
                "Labels",
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .children(
                        row.labels
                            .iter()
                            .filter(|label| label.as_str() != "human")
                            .map(|label| {
                                let remove_id = id.clone();
                                let remove_label = label.clone();
                                label_chip(label, p).child(
                                    div()
                                        .id(SharedString::from(format!("beads-label-x-{label}")))
                                        .cursor_pointer()
                                        .child(titlebar_svg_icon("titlebar/x.svg", 11.0, p.faint))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.simple_action(
                                                "removeLabel",
                                                &remove_id,
                                                json!({ "label": remove_label }),
                                                "Couldn't remove the label",
                                                window,
                                                cx,
                                            )
                                        })),
                                )
                            }),
                    )
                    .when_some(label_input.clone(), |this, input| {
                        this.child(div().w(px(160.0)).child(text_field(&input, p, window, cx)))
                    })
                    .when(label_input.is_none(), |this| {
                        this.child(
                            div()
                                .id("beads-add-label")
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .px(px(8.0))
                                .h(px(22.0))
                                .rounded(px(6.0))
                                .border_1()
                                .border_dashed()
                                .border_color(p.border_strong)
                                .text_color(p.muted)
                                .cursor_pointer()
                                .child(titlebar_svg_icon("titlebar/plus.svg", 11.0, p.muted))
                                .child("Add label")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.start_label(window, cx)),
                                ),
                        )
                    })
                    .into_any_element(),
            ));
        if let Some(links) = self.render_links(shown.as_ref(), p, window, cx) {
            body = body.child(field_row("Links", links));
        }
        if let (Some(epic), Some((done, total))) = (epic.clone(), epic_progress) {
            let epic_id = epic.id.clone();
            body = body.child(field_row(
                "Epic",
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .id("beads-details-epic")
                            .cursor_pointer()
                            .underline()
                            .child(epic.title.clone())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_details(&epic_id, window, cx)
                            })),
                    )
                    .child(
                        div()
                            .w(px(110.0))
                            .h(px(4.0))
                            .rounded_full()
                            .bg(p.control_hover)
                            .child(div().h_full().rounded_full().bg(p.muted).w(relative(
                                if total == 0 {
                                    0.0
                                } else {
                                    done as f32 / total as f32
                                },
                            ))),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(p.muted)
                            .child(format!("{done}/{total} done")),
                    )
                    .into_any_element(),
            ));
        }
        let claimed = match (row.assignee.as_deref(), actor.as_deref()) {
            (Some(assignee), Some(actor)) if assignee == actor => "You".to_string(),
            (Some(assignee), _) => assignee.to_string(),
            (None, _) => "Nobody yet".to_string(),
        };
        body = body.child(field_row(
            "Claimed by",
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(titlebar_svg_icon("titlebar/user.svg", 13.0, p.muted))
                .child(claimed)
                .into_any_element(),
        ));
        if row.needs_decision && !closed && human_ok {
            body = body
                .child(section_label("Needs a decision", p))
                .child(match answer {
                    Some(answer) => div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(text_area(&answer, 80.0, p, window, cx))
                        .child(
                            div().flex().justify_end().child(
                                kanban_button(
                                    "beads-answer-send",
                                    Some("titlebar/send.svg"),
                                    Some("Answer and close".into()),
                                    KanbanButtonKind::Primary,
                                    self.writes_in_flight > 0,
                                    p,
                                )
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.send_answer(window, cx)),
                                ),
                            ),
                        )
                        .into_any_element(),
                    None => kanban_button(
                        "beads-answer-start",
                        Some("titlebar/message.svg"),
                        Some("Answer".into()),
                        KanbanButtonKind::Secondary,
                        false,
                        p,
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.start_answer(window, cx)))
                    .into_any_element(),
                });
        }
        let live_agents: Vec<_> = agents
            .into_iter()
            .filter(|agent| agent.state != BeadAgentState::Gone)
            .collect();
        if !live_agents.is_empty() {
            body =
                body.child(section_label("Who's on it", p))
                    .children(live_agents.into_iter().map(|agent| {
                        let (word, color) = match agent.state {
                            BeadAgentState::Working => ("Working now", gpui::rgb(0x3ccf8e).into()),
                            BeadAgentState::Idle => ("Idle", p.faint),
                            BeadAgentState::Sleeping => ("Asleep", p.faint),
                            BeadAgentState::Gone => ("Ended", p.faint),
                        };
                        let project_id = agent.project_id.clone();
                        let session_id = agent.session_id.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .p(px(12.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.card)
                            .child(titlebar_svg_icon("titlebar/robot.svg", 16.0, p.muted))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(px(2.0))
                                    .child(div().truncate().font_weight(FontWeight::MEDIUM).child(
                                        if agent.session_title.is_empty() {
                                            agent.agent.clone()
                                        } else {
                                            format!("{} · {}", agent.agent, agent.session_title)
                                        },
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .text_size(px(12.0))
                                            .text_color(p.muted)
                                            .child(div().size(px(7.0)).rounded_full().bg(color))
                                            .child(word),
                                    ),
                            )
                            .child(
                                kanban_button(
                                    SharedString::from(format!("beads-go-{session_id}")),
                                    Some("titlebar/arrow-up-right.svg"),
                                    Some("Go to agent".into()),
                                    KanbanButtonKind::Secondary,
                                    false,
                                    p,
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        (this.host.clone())(
                                            BeadsRequest::OpenSession {
                                                project_id: project_id.clone(),
                                                session_id: session_id.clone(),
                                            },
                                            window,
                                            cx,
                                        )
                                    },
                                )),
                            )
                    }));
        }
        if !blocked_by.is_empty() {
            body = body
                .child(section_label("Blocked by", p))
                .child(relation_rows(blocked_by, cx));
        }
        if !blocks.is_empty() {
            body = body
                .child(section_label("Blocks", p))
                .child(relation_rows(blocks, cx));
        }
        body = body.child(section_label("Description", p)).child(
            div()
                .text_size(px(13.0))
                .text_color(if description.is_empty() {
                    p.faint
                } else {
                    p.foreground
                })
                .child(if loading && description.is_empty() {
                    "Loading…".to_string()
                } else if description.is_empty() {
                    "No description.".to_string()
                } else {
                    description
                }),
        );
        body = body
            .child(section_label(format!("Comments {}", comments.len()), p))
            .children(comments.iter().map(|comment| {
                let author = shown_text(Some(comment), "author");
                let when = parse_bd_time_ms(&shown_text(Some(comment), "created_at"))
                    .map(|time| relative_ago(time, &clock))
                    .unwrap_or_default();
                let author = match actor.as_deref() {
                    Some(actor) if actor == author => "You".to_string(),
                    _ => author,
                };
                div()
                    .flex()
                    .gap(px(10.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(p.border)
                    .child(titlebar_svg_icon("titlebar/user-circle.svg", 18.0, p.muted))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.0))
                                    .text_size(px(12.0))
                                    .child(div().font_weight(FontWeight::MEDIUM).child(author))
                                    .child(div().text_color(p.muted).child(format!("· {when}"))),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .child(shown_text(Some(comment), "text")),
                            ),
                    )
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(text_area(&comment_box, 72.0, p, window, cx))
                    .child(
                        div().flex().justify_end().child(
                            kanban_button(
                                "beads-comment-send",
                                None,
                                Some("Comment".into()),
                                KanbanButtonKind::Secondary,
                                self.writes_in_flight > 0,
                                p,
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.send_comment(window, cx)),
                            ),
                        ),
                    ),
            );
        // Beside the tree the column's width and its divider come from render.rs; in a narrow
        // panel it takes the whole panel (layout.rs).
        let beside = !self.is_narrow();
        Some(
            div()
                .id("beads-details")
                .flex()
                .flex_col()
                .w_full()
                .h_full()
                .min_h_0()
                .overflow_y_scroll()
                .when(beside, |this| this.pl(px(12.0)))
                .pr(px(6.0))
                .child(header)
                .child(body)
                .into_any_element(),
        )
    }

    fn tree_glyph_for(&self, id: &str) -> Option<ghostex_gx_core::beads_view::BeadGlyph> {
        self.tree
            .sections
            .iter()
            .flat_map(|section| {
                let epic = match &section.kind {
                    ghostex_gx_core::beads_view::BeadsSectionKind::Epic(epic) => {
                        Some(epic.as_ref())
                    }
                    _ => None,
                };
                section
                    .rows
                    .iter()
                    .chain(section.done_rows.iter())
                    .chain(epic)
            })
            .find(|row| row.id == id)
            .map(|row| row.glyph)
    }
}
