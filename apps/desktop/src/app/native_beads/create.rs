//! The new-bead form, beside the tree or the board: title, description, type, priority, epic,
//! labels (an epic's to start with) and what it waits on; on the Board also the estimate and
//! Create & Start.

use ghostex_gx_protocol::beads_view::capability;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, ClickEvent, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use serde_json::{Value, json};

use super::menus::{BeadsMenu, BeadsMenuKind};
use super::render::{section_label, small_button};
use super::tree::label_chip;
use super::view::{BeadsRequest, BeadsView};
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::model::TSHIRT_OPTIONS;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_kanban::text::draft_title;
use crate::app::native_kanban::widgets::{
    KanbanButtonKind, choice_pill, kanban_button, text_area, text_field,
};

const TYPES: [(&str, &str); 5] = [
    ("task", "Task"),
    ("bug", "Bug"),
    ("feature", "Feature"),
    ("chore", "Chore"),
    ("epic", "Epic"),
];

pub(crate) struct CreateForm {
    pub(crate) title: Entity<InputState>,
    pub(crate) description: Entity<TextareaState>,
    pub(crate) label_input: Entity<InputState>,
    pub(crate) issue_type: &'static str,
    pub(crate) priority: i64,
    pub(crate) epic: Option<String>,
    pub(crate) labels: Vec<String>,
    pub(crate) blocked_by: Vec<String>,
    /// Opened from the Board: the estimate and Create & Start show, and `status` is the lane.
    pub(crate) from_board: bool,
    pub(crate) status: Option<String>,
    pub(crate) estimate: Option<&'static str>,
    _subscriptions: Vec<Subscription>,
}

impl BeadsView {
    /// Opens the form; `epic` presets the epic (an epic's "+ Add child"), `board_status` opens it
    /// for the Board with that lane's status.
    pub(crate) fn open_create(
        &mut self,
        epic: Option<String>,
        board_status: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.details = None;
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("What should happen"));
        let description = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What needs to happen, and how you'll know it's done.")
        });
        let label_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Add a label and press Enter"));
        let subscriptions = vec![
            cx.subscribe(&title, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&description, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe_in(
                &label_input,
                window,
                |this: &mut Self, input, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        let label = input.read(cx).value().trim().to_string();
                        if let Some(form) = this.create.as_mut() {
                            if !label.is_empty() && !form.labels.contains(&label) {
                                form.labels.push(label);
                            }
                            form.label_input
                                .update(cx, |input, cx| input.set_value("", window, cx));
                        }
                        cx.notify();
                    }
                },
            ),
        ];
        title.update(cx, |input, cx| input.focus(window, cx));
        self.create = Some(CreateForm {
            title,
            description,
            label_input,
            issue_type: "task",
            priority: 2,
            epic: None,
            labels: Vec::new(),
            blocked_by: Vec::new(),
            from_board: board_status.is_some(),
            status: board_status,
            estimate: None,
            _subscriptions: subscriptions,
        });
        self.set_create_epic(epic, cx);
        cx.notify();
    }

    pub(crate) fn close_create(&mut self, cx: &mut Context<Self>) {
        if self.create.take().is_some() {
            cx.notify();
        }
    }

    /// Picking an epic starts the labels from the epic's, as bd's own `--parent` does; the user
    /// can remove any of them.
    pub(crate) fn set_create_epic(&mut self, epic: Option<String>, cx: &mut Context<Self>) {
        let epic_labels: Vec<String> = epic
            .as_deref()
            .and_then(|id| self.row(id))
            .map(|row| {
                row.labels
                    .iter()
                    .filter(|label| label.as_str() != "human")
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if let Some(form) = self.create.as_mut() {
            for label in epic_labels {
                if !form.labels.contains(&label) {
                    form.labels.push(label);
                }
            }
            form.epic = epic;
        }
        cx.notify();
    }

    pub(crate) fn toggle_create_blocker(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(form) = self.create.as_mut() {
            if let Some(index) = form.blocked_by.iter().position(|blocker| blocker == id) {
                form.blocked_by.remove(index);
            } else {
                form.blocked_by.push(id.to_string());
            }
        }
        cx.notify();
    }

    fn submit_create(&mut self, start_after: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.create.as_ref() else {
            return;
        };
        let typed_title = form.title.read(cx).value().trim().to_string();
        let description = form.description.read(cx).value().trim().to_string();
        let can_generate = self.has_capability(capability::GENERATE_TITLE);
        if typed_title.is_empty() && (description.is_empty() || !can_generate) {
            return;
        }
        let generate_title = typed_title.is_empty();
        let title = if generate_title {
            draft_title(&description)
        } else {
            typed_title
        };
        let full = self.has_capability(capability::CREATE_TYPE_PARENT);
        let blockers_ok = self.has_capability(capability::CREATE_BLOCKED_BY);
        let mut params = json!({
            "action": "create",
            "title": title,
            "description": description,
            "priority": form.priority.to_string(),
            "labels": form.labels,
        });
        if full {
            params["issueType"] = json!(form.issue_type);
            if let Some(epic) = form.epic.as_ref() {
                params["parent"] = json!(epic);
                params["noInheritLabels"] = json!(true);
            }
        }
        if blockers_ok && !form.blocked_by.is_empty() {
            params["blockedBy"] = json!(form.blocked_by);
        }
        if let Some(minutes) = form.estimate.and_then(|label| {
            TSHIRT_OPTIONS
                .iter()
                .find(|(option, _)| *option == label)
                .map(|(_, minutes)| *minutes)
        }) {
            params["estimate"] = json!(minutes);
        }
        let status = form.status.clone().filter(|status| status != "open");
        self.create = None;
        self.run_action(
            params,
            "Couldn't create the bead",
            window,
            cx,
            move |this, reply, window, cx| {
                let Some(id) = created_id(reply) else {
                    return;
                };
                if let Some(status) = status.filter(|_| !start_after) {
                    this.simple_action(
                        "updateStatus",
                        &id,
                        json!({ "status": status }),
                        "Couldn't move the new bead",
                        window,
                        cx,
                    );
                }
                if start_after {
                    (this.host.clone())(
                        BeadsRequest::StartWork {
                            issue_id: id.clone(),
                        },
                        window,
                        cx,
                    );
                }
                if generate_title {
                    this.generate_title(&id, &description, cx);
                }
            },
        );
    }

    /// A bead created without a title gets one from the Title Generation agent on the computer
    /// that owns the project; until then it keeps a title cut from its description.
    fn generate_title(&mut self, id: &str, description: &str, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        let id = id.to_string();
        let prompt = description.to_string();
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let answer = crate::app::gx_store::gx_rpc(
                target.remote.clone(),
                "/api/beadsGenerateTitle",
                json!({ "projectId": target.rpc_project_id, "prompt": prompt }),
            )
            .await;
            let Some(title) = answer.ok().and_then(|answer| {
                answer
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .map(str::to_string)
            }) else {
                return;
            };
            let _ = crate::app::gx_store::gx_rpc(
                target.remote.clone(),
                "/api/runBeadsAction",
                json!({
                    "action": "updateTitle",
                    "issueId": id,
                    "title": title,
                    "projectId": target.rpc_project_id,
                    "projectPath": target.project_path,
                    "projectBoardScope": true,
                }),
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    if let Some(fetch) = this.fetch_now(false, cx) {
                        fetch.detach();
                    }
                }
            });
        })
        .detach();
    }

    pub(crate) fn render_create(
        &mut self,
        p: &KanbanPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let form = self.create.as_ref()?;
        let full = self.has_capability(capability::CREATE_TYPE_PARENT) && !self.legacy;
        let blockers_ok = self.has_capability(capability::CREATE_BLOCKED_BY) && !self.legacy;
        let can_generate = self.has_capability(capability::GENERATE_TITLE);
        let epic_title = form
            .epic
            .as_deref()
            .and_then(|id| self.row(id))
            .map(|row| (row.id.clone(), row.title.clone()));
        let blockers: Vec<(String, String)> = form
            .blocked_by
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    self.row(id)
                        .map(|row| row.title.clone())
                        .unwrap_or_default(),
                )
            })
            .collect();
        let title_empty = form.title.read(cx).value().trim().is_empty();
        let description_empty = form.description.read(cx).value().trim().is_empty();
        let can_submit = !title_empty || (can_generate && !description_empty);
        let busy = self.writes_in_flight > 0;
        let heading = match &epic_title {
            Some((_, title)) => format!("New bead in {title}"),
            None => "New bead".to_string(),
        };
        let field = |label: &'static str, body: AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(section_label(label, p))
                .child(body)
        };
        let type_row = div()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .children(TYPES.iter().map(|(value, label)| {
                let value: &'static str = value;
                choice_pill(
                    SharedString::from(format!("beads-type-{value}")),
                    (*label).into(),
                    form.issue_type == value,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(form) = this.create.as_mut() {
                        form.issue_type = value;
                    }
                    cx.notify();
                }))
            }))
            .into_any_element();
        let priority_row = div()
            .flex()
            .gap(px(6.0))
            .children((0..=4).map(|priority| {
                choice_pill(
                    SharedString::from(format!("beads-new-priority-{priority}")),
                    format!("P{priority}").into(),
                    form.priority == priority,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(form) = this.create.as_mut() {
                        form.priority = priority;
                    }
                    cx.notify();
                }))
            }))
            .into_any_element();
        let picker = |id: &'static str,
                      text: String,
                      placeholder: bool,
                      kind: BeadsMenuKind,
                      cx: &mut Context<Self>| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .h(px(32.0))
                .px(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.control)
                .cursor_pointer()
                .text_size(px(13.0))
                .text_color(if placeholder { p.muted } else { p.foreground })
                .child(div().truncate().child(text))
                .child(titlebar_svg_icon(
                    "titlebar/chevron-down.svg",
                    12.0,
                    p.muted,
                ))
                .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    this.open_menu(BeadsMenu::new(kind.clone(), event.position()), cx)
                }))
                .into_any_element()
        };
        let labels = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .children(form.labels.iter().map(|label| {
                        let remove = label.clone();
                        label_chip(label, p).child(
                            div()
                                .id(SharedString::from(format!("beads-new-label-x-{label}")))
                                .cursor_pointer()
                                .child(titlebar_svg_icon("titlebar/x.svg", 11.0, p.faint))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(form) = this.create.as_mut() {
                                        form.labels.retain(|label| *label != remove);
                                    }
                                    cx.notify();
                                })),
                        )
                    }))
                    .child(div().flex_1().min_w(px(160.0)).child(text_field(
                        &form.label_input,
                        p,
                        window,
                        cx,
                    ))),
            )
            .when(epic_title.is_some() && full, |this| {
                this.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.faint)
                        .child("Takes the epic's labels to start with."),
                )
            })
            .into_any_element();
        let mut body =
            div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(field("Title", text_field(&form.title, p, window, cx)))
                .child(field(
                    "Description",
                    text_area(&form.description, 96.0, p, window, cx),
                ))
                .when(title_empty && can_generate, |this| {
                    this.child(div().text_size(px(12.0)).text_color(p.faint).child(
                        "Leave the title empty and Ghostex writes one from the description.",
                    ))
                });
        if full {
            body = body.child(field("Type", type_row)).child(
                div()
                    .flex()
                    .gap(px(16.0))
                    .child(div().flex_none().child(field("Priority", priority_row)))
                    .child(
                        div().flex_1().min_w_0().child(field(
                            "Epic",
                            picker(
                                "beads-new-epic",
                                epic_title
                                    .as_ref()
                                    .map(|(id, title)| format!("{id}  {title}"))
                                    .unwrap_or_else(|| "No epic".to_string()),
                                epic_title.is_none(),
                                BeadsMenuKind::CreateEpic,
                                cx,
                            ),
                        )),
                    ),
            );
        } else {
            body = body.child(field("Priority", priority_row));
        }
        body = body.child(field("Labels", labels));
        if blockers_ok {
            let text = if blockers.is_empty() {
                "Pick beads this one waits on".to_string()
            } else {
                blockers
                    .iter()
                    .map(|(_, title)| title.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            body = body.child(field(
                "Blocked by (optional)",
                picker(
                    "beads-new-blocked-by",
                    text,
                    blockers.is_empty(),
                    BeadsMenuKind::CreateBlockedBy,
                    cx,
                ),
            ));
        }
        if form.from_board {
            let estimate = form.estimate;
            body = body.child(field(
                "Estimate",
                div()
                    .flex()
                    .gap(px(6.0))
                    .child(
                        choice_pill(
                            "beads-new-estimate-none",
                            "None".into(),
                            estimate.is_none(),
                            p,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(form) = this.create.as_mut() {
                                form.estimate = None;
                            }
                            cx.notify();
                        })),
                    )
                    .children(TSHIRT_OPTIONS.iter().map(|(label, _)| {
                        let label: &'static str = label;
                        choice_pill(
                            SharedString::from(format!("beads-new-estimate-{label}")),
                            label.into(),
                            estimate == Some(label),
                            p,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(form) = this.create.as_mut() {
                                form.estimate = Some(label);
                            }
                            cx.notify();
                        }))
                    }))
                    .into_any_element(),
            ));
        }
        let from_board = form.from_board && self.can_start_work;
        Some(
            div()
                .id("beads-create")
                .flex()
                .flex_col()
                .w(px(440.0))
                .flex_none()
                .h_full()
                .min_h_0()
                .pl(px(18.0))
                .pr(px(6.0))
                .border_l_1()
                .border_color(p.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .pb(px(14.0))
                        .mb(px(14.0))
                        .border_b_1()
                        .border_color(p.border)
                        .child(titlebar_svg_icon("titlebar/plus.svg", 14.0, p.muted))
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .child(heading),
                        )
                        .child(
                            small_button("beads-create-close", Some("titlebar/x.svg"), None, p)
                                .on_click(cx.listener(|this, _, _, cx| this.close_create(cx))),
                        ),
                )
                .child(
                    div()
                        .id("beads-create-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(body),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .pt(px(14.0))
                        .mt(px(10.0))
                        .border_t_1()
                        .border_color(p.border)
                        .child(
                            kanban_button(
                                "beads-create-cancel",
                                None,
                                Some("Cancel".into()),
                                KanbanButtonKind::Secondary,
                                false,
                                p,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.close_create(cx))),
                        )
                        .when(from_board, |this| {
                            this.child(
                                kanban_button(
                                    "beads-create-start",
                                    Some("titlebar/player-play.svg"),
                                    Some("Create & Start".into()),
                                    KanbanButtonKind::Secondary,
                                    busy || !can_submit,
                                    p,
                                )
                                .when(
                                    !busy && can_submit,
                                    |this| {
                                        this.on_click(cx.listener(|this, _, window, cx| {
                                            this.submit_create(true, window, cx)
                                        }))
                                    },
                                ),
                            )
                        })
                        .child(
                            kanban_button(
                                "beads-create-submit",
                                None,
                                Some("Create bead".into()),
                                KanbanButtonKind::Primary,
                                busy || !can_submit,
                                p,
                            )
                            .when(!busy && can_submit, |this| {
                                this.on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_create(false, window, cx)
                                }))
                            }),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// The new bead's id from bd's `create --json` reply.
fn created_id(reply: &Value) -> Option<String> {
    let stdout = reply.get("stdout").and_then(Value::as_str)?;
    let parsed: Value = serde_json::from_str(stdout.trim()).ok()?;
    let data = parsed.get("data").cloned().unwrap_or(parsed);
    let data = match data {
        Value::Array(mut items) if !items.is_empty() => items.remove(0),
        other => other,
    };
    data.get("id").and_then(Value::as_str).map(str::to_string)
}
