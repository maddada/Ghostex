//! The Department tab: which department this GBot is in (or none), whether it leads it, and the
//! department itself: its name, the shared instructions every member follows, and Remove. A new
//! department is added here too, with this GBot moved into it. Each change applies at once (like
//! the Routines tab), so it never waits for the window's Save.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (plan piece 6): departments are created, renamed and removed "with a lead and shared instructions", and a GBot's department is edited "in GBot Settings". Only you and Chief of Staff change departments (server/src/gbots/departments.rs), so this tab is yours; a department's lead changes only its members' instructions, from its chats.
//! SEE-ALSO: server/src/gbots/department_ops.rs (`departments`, `departmentCreate`, `departmentEdit`, `departmentRemove`), apps/mobile/app/src/screens/sessions-screen/GbotDepartmentView.tsx (the phone's same tab).
use super::super::native_modal_kit::*;
use super::parts::{page_header, row, section, wide_row};
use super::window::{GbotSettingsModalCommand, GpuiGbotSettingsModalWindow};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px, svg,
};
use gpui_component::input::{InputState, TextareaState};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// The Department tab's state inside the window.
pub(crate) struct GbotDepartmentTab {
    /// gxserver's `departments` answer; `None` until the first read answers.
    pub(super) list: Option<Value>,
    /// This bot's department and whether it leads it, as gxserver last answered.
    pub(super) department_id: Option<String>,
    pub(super) is_lead: bool,
    pub(super) requested: bool,
    pub(super) busy: bool,
    pub(super) error: Option<String>,
    pub(super) confirm_remove: bool,
    /// The department the name and instructions fields were last filled from.
    filled_for: Option<String>,
    pub(super) name_input: Entity<InputState>,
    pub(super) instructions_input: Entity<TextareaState>,
    pub(super) new_name_input: Entity<InputState>,
}

impl GbotDepartmentTab {
    pub(super) fn new(
        department_id: Option<String>,
        is_lead: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        Self {
            list: None,
            department_id,
            is_lead,
            requested: false,
            busy: false,
            error: None,
            confirm_remove: false,
            filled_for: None,
            name_input: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Research")),
            instructions_input: cx.new(|cx| {
                TextareaState::new(window, cx).auto_grow(4, 10).placeholder(
                    "What every GBot in it follows, e.g. Cite a source for every fact.",
                )
            }),
            new_name_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("A new department's name")),
        }
    }

    /// Takes a `{departments, bot}` answer read before the window opened.
    pub(super) fn preload(&mut self, answer: Value) {
        let bot = &answer["bot"];
        self.department_id = bot["departmentId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        self.is_lead = bot["isLead"].as_bool() == Some(true);
        self.list = Some(answer["departments"].clone());
    }

    fn departments(&self) -> Vec<Value> {
        self.list
            .as_ref()
            .and_then(|list| list["departments"].as_array().cloned())
            .unwrap_or_default()
    }

    fn current(&self) -> Option<Value> {
        let id = self.department_id.as_deref()?;
        self.departments()
            .into_iter()
            .find(|department| text(department, "departmentId") == id)
    }
}

/// Where a department op's answer goes: the window that asked.
pub(crate) struct GbotDepartmentReply {
    view: WeakEntity<GpuiGbotSettingsModalWindow>,
    window: AnyWindowHandle,
}

impl GbotDepartmentReply {
    /// Hands the window `{departments, bot}` read after the ops, or the first refusal.
    pub(crate) fn finish(self, result: Result<Value, String>, cx: &mut App) {
        let view = self.view;
        let _ = self.window.update(cx, |_root, window, cx| {
            view.update(cx, |modal, cx| modal.finish_department(result, window, cx))
        });
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

impl GpuiGbotSettingsModalWindow {
    /// The Profile tab's Department row: the department's name (and "lead"), opening this tab.
    pub(super) fn render_department_link(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let name = self
            .department
            .current()
            .map(|department| text(&department, "name").to_string())
            .or_else(|| {
                self.original
                    .department_name
                    .clone()
                    .filter(|_| self.department.department_id == self.original.department_id)
            });
        let label = match (name, self.department.is_lead) {
            (Some(name), true) => format!("{name} (its lead)"),
            (Some(name), false) => name,
            (None, _) => "None".to_string(),
        };
        h_flex()
            .id("gbot-profile-department")
            .role(gpui::Role::Button)
            .aria_label(format!("Department: {label}"))
            .gap(px(6.0))
            .items_center()
            .cursor_pointer()
            .text_size(px(14.0))
            .text_color(hsla(p.muted))
            .hover(move |this| this.text_color(hsla(p.foreground)))
            .on_press(cx, |this: &mut Self, window, cx| {
                this.select_tab(super::window::GbotSettingsTab::Department, window, cx)
            })
            .child(label)
            .child(
                svg()
                    .path("titlebar/chevron-right.svg")
                    .size(px(14.0))
                    .text_color(hsla(p.muted)),
            )
            .into_any_element()
    }

    /// Reads the departments the first time the tab shows.
    pub(super) fn ensure_department(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.department.requested {
            return;
        }
        self.department.requested = true;
        self.department_ops(Vec::new(), window, cx);
    }

    /// Sends `ops` to `/api/gbots` in order, then reads the departments and this bot again.
    fn department_ops(&mut self, ops: Vec<Value>, window: &mut Window, cx: &mut Context<Self>) {
        self.department.busy = true;
        self.department.error = None;
        cx.notify();
        let reply = GbotDepartmentReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
        };
        (self.host)(
            GbotSettingsModalCommand::Departments {
                bot_id: self.original.bot_id.clone(),
                ops,
                reply,
            },
            cx,
        );
    }

    fn finish_department(
        &mut self,
        result: Result<Value, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = &mut self.department;
        tab.busy = false;
        tab.confirm_remove = false;
        match result {
            Ok(answer) => {
                let bot = &answer["bot"];
                tab.department_id = bot["departmentId"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_string);
                tab.is_lead = bot["isLead"].as_bool() == Some(true);
                tab.list = Some(answer["departments"].clone());
                // The window's own record follows, so Save never sends the department back.
                self.original.department_id = tab.department_id.clone();
                self.draft.department_id = tab.department_id.clone();
                self.fill_department_fields(true, window, cx);
            }
            Err(message) => tab.error = Some(message),
        }
        cx.notify();
    }

    /// Puts the current department's name and instructions in their fields (when it changed, or
    /// `force` after a save).
    fn fill_department_fields(&mut self, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.department.current();
        let id = current
            .as_ref()
            .map(|department| text(department, "departmentId").to_string());
        if !force && id == self.department.filled_for {
            return;
        }
        self.department.filled_for = id;
        let name = current
            .as_ref()
            .map(|department| text(department, "name").to_string())
            .unwrap_or_default();
        let instructions = current
            .as_ref()
            .map(|department| text(department, "instructions").to_string())
            .unwrap_or_default();
        self.department
            .name_input
            .update(cx, |input, cx| input.set_value(name, window, cx));
        self.department
            .instructions_input
            .update(cx, |input, cx| input.set_value(instructions, window, cx));
        self.department
            .new_name_input
            .update(cx, |input, cx| input.set_value(String::new(), window, cx));
    }

    /// Whether the name or instructions fields differ from the department as saved.
    pub(super) fn department_edited(&self, cx: &App) -> bool {
        let Some(current) = self.department.current() else {
            return false;
        };
        let name = self.department.name_input.read(cx).value().to_string();
        let instructions = self
            .department
            .instructions_input
            .read(cx)
            .value()
            .to_string();
        name.trim() != text(&current, "name").trim()
            || instructions.trim() != text(&current, "instructions").trim()
    }

    fn move_to(&mut self, department: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.department.busy || department == self.department.department_id {
            return;
        }
        let op = json!({
            "op": "edit",
            "bot": self.original.bot_id,
            "department": department.unwrap_or_default(),
        });
        self.department_ops(vec![op], window, cx);
    }

    fn toggle_lead(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.department.busy || self.department.department_id.is_none() {
            return;
        }
        let op =
            json!({ "op": "edit", "bot": self.original.bot_id, "lead": !self.department.is_lead });
        self.department_ops(vec![op], window, cx);
    }

    fn save_department(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.department.department_id.clone() else {
            return;
        };
        if self.department.busy || !self.department_edited(cx) {
            return;
        }
        let name = self.department.name_input.read(cx).value().to_string();
        let instructions = self
            .department
            .instructions_input
            .read(cx)
            .value()
            .to_string();
        let op = json!({
            "op": "departmentEdit",
            "department": id,
            "name": name,
            "instructions": instructions,
        });
        self.department_ops(vec![op], window, cx);
    }

    fn remove_department(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.department.department_id.clone() else {
            return;
        };
        if self.department.busy {
            return;
        }
        if !self.department.confirm_remove {
            self.department.confirm_remove = true;
            cx.notify();
            return;
        }
        self.department_ops(
            vec![json!({ "op": "departmentRemove", "department": id })],
            window,
            cx,
        );
    }

    fn add_department(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .department
            .new_name_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        if self.department.busy || name.is_empty() {
            return;
        }
        let ops = vec![
            json!({ "op": "departmentCreate", "name": name }),
            json!({ "op": "edit", "bot": self.original.bot_id, "department": name }),
        ];
        self.department_ops(ops, window, cx);
    }

    /// One choice of "In department": a row with a check on the current one.
    fn department_choice(
        &self,
        id: Option<String>,
        name: String,
        detail: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let chosen = id == self.department.department_id;
        let key = id.clone().unwrap_or_else(|| "none".to_string());
        h_flex()
            .id(SharedString::from(format!("gbot-department-choice-{key}")))
            .role(gpui::Role::RadioButton)
            .aria_label(name.clone())
            .aria_selected(chosen)
            .w_full()
            .min_h(px(44.0))
            .px(px(20.0))
            .py(px(8.0))
            .gap(px(12.0))
            .items_center()
            .when(!self.department.busy, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(switch_row_wash(p.foreground))))
                    .on_press(cx, move |this: &mut Self, window, cx| {
                        this.move_to(id.clone(), window, cx)
                    })
            })
            .child(div().size(px(16.0)).flex_shrink_0().when(chosen, |this| {
                this.child(
                    svg()
                        .path("titlebar/check.svg")
                        .size(px(16.0))
                        .text_color(hsla(p.foreground)),
                )
            }))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(19.0))
                            .when(chosen, |this| this.font_weight(FontWeight::MEDIUM))
                            .text_color(hsla(p.foreground))
                            .child(name),
                    )
                    .when(!detail.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(px(12.5))
                                .line_height(px(17.0))
                                .text_color(hsla(p.muted))
                                .child(detail),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_department(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.fill_department_fields(false, window, cx);
        let p = self.palette;
        let kit = self.kit;
        let busy = self.department.busy;
        let bot_name = match self.draft.name.trim() {
            "" => "this GBot".to_string(),
            name => name.to_string(),
        };
        let mut column = v_flex()
            .w_full()
            .pr(px(20.0))
            .pb(px(8.0))
            .gap(px(24.0))
            .child(page_header(
                &p,
                "Department",
                "Group GBots under a name. Every GBot in a department follows its shared instructions, and its lead hands the others work.",
            ));
        if let Some(error) = &self.department.error {
            column = column.child(modal_error(&kit, error.clone()));
        }
        let Some(_) = self.department.list else {
            return self.department_frame(column.child(section(
                &p,
                "In department",
                vec![row(&p, "Reading the departments…", div())],
            )));
        };
        let mut choices = vec![self.department_choice(
            None,
            "None".to_string(),
            "Chief of Staff hands it work.".to_string(),
            cx,
        )];
        for department in self.department.departments() {
            let members = department["members"].as_array().map(Vec::len).unwrap_or(0);
            let lead = text(&department["lead"], "name");
            let bots = if members == 1 {
                "1 GBot".to_string()
            } else {
                format!("{members} GBots")
            };
            let detail = match (lead.is_empty(), members) {
                (_, 0) => "No GBots yet".to_string(),
                (true, _) => format!("{bots}, no lead"),
                (false, _) => format!("{bots}, led by {lead}"),
            };
            choices.push(self.department_choice(
                Some(text(&department, "departmentId").to_string()),
                text(&department, "name").to_string(),
                detail,
                cx,
            ));
        }
        column = column.child(section(&p, "In department", choices));
        if let Some(current) = self.department.current() {
            let name = text(&current, "name").to_string();
            let leads = self.department.is_lead;
            let lead_label = format!("{bot_name} leads {name}");
            let lead_row = h_flex()
                .id("gbot-department-lead")
                .role(gpui::Role::Switch)
                .aria_toggled(a11y_toggled(leads))
                .aria_label(lead_label.clone())
                .w_full()
                .switch_row_press(
                    switch_row_wash(p.foreground),
                    busy,
                    bind_press(cx, |this: &mut Self, window, cx| {
                        this.toggle_lead(window, cx)
                    }),
                )
                .child(row(&p, &lead_label, modal_switch(&kit, leads, busy)))
                .into_any_element();
            let note = if leads {
                format!("{bot_name} hands {name}'s GBots work and may change their instructions.")
            } else {
                match text(&current["lead"], "name") {
                    "" => format!("{name} has no lead, so Chief of Staff hands its GBots work."),
                    lead => format!("{lead} leads {name} and hands {bot_name} its work."),
                }
            };
            let edited = self.department_edited(cx);
            let removing = self.department.confirm_remove;
            let buttons = h_flex()
                .w_full()
                .gap(px(8.0))
                .justify_end()
                .child(h_flex().w(px(150.0)).child(modal_action_button(
                    &kit,
                    "gbot-department-remove",
                    if removing {
                        "Remove? Its GBots stay"
                    } else {
                        "Remove Department"
                    },
                    None,
                    ModalButtonTone::Danger,
                    busy,
                    |this: &mut Self, window, cx| this.remove_department(window, cx),
                    cx,
                )))
                .child(h_flex().w(px(150.0)).child(modal_action_button(
                    &kit,
                    "gbot-department-save",
                    "Save Department",
                    None,
                    ModalButtonTone::Primary,
                    busy || !edited,
                    |this: &mut Self, window, cx| this.save_department(window, cx),
                    cx,
                )));
            column = column.child(section(
                &p,
                &name,
                vec![
                    v_flex()
                        .w_full()
                        .child(lead_row)
                        .child(
                            div()
                                .px(px(20.0))
                                .pb(px(12.0))
                                .mt(px(-6.0))
                                .text_size(px(12.5))
                                .line_height(px(17.5))
                                .text_color(hsla(p.muted))
                                .child(note),
                        )
                        .into_any_element(),
                    row(
                        &p,
                        "Name",
                        modal_text_input(&kit, &self.department.name_input, busy, window, cx),
                    ),
                    wide_row(
                        &p,
                        "Shared instructions",
                        modal_text_area(
                            &kit,
                            &self.department.instructions_input,
                            Some(96.0),
                            busy,
                            window,
                            cx,
                        ),
                        Some("Every GBot in it reads these under its own instructions.".into()),
                    ),
                    div()
                        .w_full()
                        .px(px(20.0))
                        .py(px(12.0))
                        .child(buttons)
                        .into_any_element(),
                ],
            ));
        }
        let can_add = !busy
            && !self
                .department
                .new_name_input
                .read(cx)
                .value()
                .trim()
                .is_empty();
        column = column.child(section(
            &p,
            "New department",
            vec![
                row(
                    &p,
                    "Name",
                    modal_text_input(&kit, &self.department.new_name_input, busy, window, cx),
                ),
                div()
                    .w_full()
                    .px(px(20.0))
                    .py(px(12.0))
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .child(h_flex().w(px(200.0)).child(modal_action_button(
                                &kit,
                                "gbot-department-add",
                                format!("Add and Move {bot_name}"),
                                None,
                                ModalButtonTone::Neutral,
                                !can_add,
                                |this: &mut Self, window, cx| this.add_department(window, cx),
                                cx,
                            ))),
                    )
                    .into_any_element(),
            ],
        ));
        self.department_frame(column)
    }

    /// The page's scroll frame, which fills the space above the window's buttons.
    fn department_frame(&self, column: gpui::Div) -> AnyElement {
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .id("gbot-settings-department-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.page_scroll)
                    .child(column),
            )
            .child(modal_edge_scrollbar(&self.page_scroll))
            .into_any_element()
    }
}
