//! The Computer tab: where this GBot's chats run (this computer, a connected computer or one of
//! your Cloud Boxes) and whether it may use that computer's screen and apps. Picking applies at
//! once, like the Department tab: its new chats start there; open chats stay where they are.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! Coordinator brief for M12: "Show the GBot's computer in GBot Settings (desktop/web and phone) and on its sidebar row", and "Only the user can give a GBot a computer". This tab is where the user gives it one; gxserver takes the pick only from the user's own devices (gbots/computer/ops.rs), so the same tab on the web build or another computer's window shows gxserver's refusal instead.
//! SEE-ALSO: server/src/gbots/computer/ops.rs (`computer`, `computerSet`), apps/mobile/app/src/screens/sessions-screen/GbotComputerView.tsx (the phone's tab).
use super::super::native_modal_kit::*;
use super::parts::{page_header, row, section};
use super::window::{GbotSettingsModalCommand, GpuiGbotSettingsModalWindow};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyWindowHandle, App, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, WeakEntity,
    Window, div, px, svg,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// The Computer tab's state inside the window.
#[derive(Default)]
pub(crate) struct GbotComputerTab {
    /// gxserver's `computer` answer; `None` until the first read answers.
    pub(super) answer: Option<Value>,
    pub(super) requested: bool,
    pub(super) busy: bool,
    pub(super) error: Option<String>,
}

/// Where a computer op's answer goes: the window that asked.
pub(crate) struct GbotComputerReply {
    view: WeakEntity<GpuiGbotSettingsModalWindow>,
    window: AnyWindowHandle,
}

impl GbotComputerReply {
    /// Hands the window gxserver's `computer` answer after the op, or its refusal.
    pub(crate) fn finish(self, result: Result<Value, String>, cx: &mut App) {
        let view = self.view;
        let _ = self.window.update(cx, |_root, _window, cx| {
            view.update(cx, |modal, cx| modal.finish_computer(result, cx))
        });
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A box's state in words.
fn box_state(state: &str) -> &'static str {
    match state {
        "running" => "Running",
        "idle" => "Idle",
        "paused" => "Paused",
        "stopped" => "Stopped",
        "failed" => "Failed",
        "creating" | "booting" | "pending" | "resuming" => "Starting",
        _ => "Not running",
    }
}

fn dollars(value: Option<f64>) -> Option<String> {
    value.map(|usd| format!("${usd:.2}"))
}

impl GpuiGbotSettingsModalWindow {
    /// Reads where the GBot works the first time the tab shows.
    pub(super) fn ensure_computer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.computer.requested {
            return;
        }
        self.computer.requested = true;
        self.computer_op(None, window, cx);
    }

    /// Sends `op` (none for a plain read), then reads the tab again.
    fn computer_op(&mut self, op: Option<Value>, window: &mut Window, cx: &mut Context<Self>) {
        self.computer.busy = true;
        self.computer.error = None;
        cx.notify();
        let reply = GbotComputerReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
        };
        (self.host)(
            GbotSettingsModalCommand::Computer {
                bot_id: self.original.bot_id.clone(),
                op,
                reply,
            },
            cx,
        );
    }

    fn finish_computer(&mut self, result: Result<Value, String>, cx: &mut Context<Self>) {
        self.computer.busy = false;
        match result {
            Ok(answer) => self.computer.answer = Some(answer),
            Err(message) => self.computer.error = Some(message),
        }
        cx.notify();
    }

    fn pick_computer(&mut self, wanted: String, window: &mut Window, cx: &mut Context<Self>) {
        let op = json!({ "op": "computerSet", "bot": self.original.bot_id, "computer": wanted });
        self.computer_op(Some(op), window, cx);
    }

    fn toggle_computer_use(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let on = self
            .computer
            .answer
            .as_ref()
            .is_some_and(|answer| answer["computer"]["computerUse"] == true);
        let op = json!({ "op": "computerSet", "bot": self.original.bot_id, "computerUse": !on });
        self.computer_op(Some(op), window, cx);
    }

    /// One place the GBot can run, with a check on the one it runs on.
    fn computer_choice(
        &self,
        wanted: String,
        name: String,
        detail: String,
        chosen: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let off = disabled || self.computer.busy;
        h_flex()
            .id(SharedString::from(format!("gbot-computer-choice-{wanted}")))
            .role(gpui::Role::RadioButton)
            .aria_label(name.clone())
            .aria_selected(chosen)
            .w_full()
            .min_h(px(44.0))
            .px(px(20.0))
            .py(px(8.0))
            .gap(px(12.0))
            .items_center()
            .when(disabled, |this| this.opacity(0.55))
            .when(!off && !chosen, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(switch_row_wash(p.foreground))))
                    .on_press(cx, move |this: &mut Self, window, cx| {
                        this.pick_computer(wanted.clone(), window, cx)
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

    pub(super) fn render_computer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let kit = self.kit;
        let bot_name = match self.original.name.trim() {
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
                "Computer",
                "Where this GBot's chats run. Its folder, memory and routines stay on this computer.",
            ));
        if let Some(error) = &self.computer.error {
            column = column.child(modal_error(&kit, error.clone()));
        }
        let Some(answer) = &self.computer.answer else {
            return self.computer_frame(column.child(section(
                &p,
                "Runs on",
                vec![row(&p, "Reading where it works…", div())],
            )));
        };
        let current = &answer["computer"];
        let wire = text(current, "computer");
        let this_name = match text(answer, "thisComputer") {
            "" => "This computer".to_string(),
            name => format!("This computer ({name})"),
        };
        let mut choices = vec![self.computer_choice(
            "this".to_string(),
            this_name,
            "Its chats run here, beside its folder.".to_string(),
            wire == "this" || wire.is_empty(),
            false,
            cx,
        )];
        for computer in answer["choices"]["computers"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let id = text(computer, "computer").to_string();
            let chosen = id == wire;
            let access = computer["agentAccess"] == true;
            let detail = if !access {
                "Agent access is off for it in Settings > Remote.".to_string()
            } else if chosen {
                format!("Its chats work in {} there.", text(current, "folder"))
            } else {
                format!(
                    "Connected. Its chats would work in ~/GBots/{} there.",
                    bot_name
                )
            };
            choices.push(self.computer_choice(
                id,
                text(computer, "name").to_string(),
                detail,
                chosen,
                !access,
                cx,
            ));
        }
        // The one it was given, while that computer is away.
        if text(current, "kind") == "computer"
            && !answer["choices"]["computers"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| text(row, "computer") == wire))
        {
            choices.push(self.computer_choice(
                wire.to_string(),
                text(current, "name").to_string(),
                "Not connected now: its new chats can't start until it is.".to_string(),
                true,
                true,
                cx,
            ));
        }
        let boxes: Vec<&Value> = answer["choices"]["cloudBoxes"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        let mut box_choices = Vec::new();
        for cloud_box in &boxes {
            let id = text(cloud_box, "computer").to_string();
            let mut detail = format!(
                "{} · {} · {}",
                text(cloud_box, "projectName"),
                text(cloud_box, "provider"),
                box_state(text(cloud_box, "state"))
            );
            if let Some(month) = dollars(cloud_box["monthUsd"].as_f64()) {
                detail.push_str(&format!(" · {month} this month"));
            }
            box_choices.push(self.computer_choice(
                id.clone(),
                text(cloud_box, "name").to_string(),
                detail,
                id == wire,
                false,
                cx,
            ));
        }
        column = column.child(section(&p, "Runs on", choices));
        if !box_choices.is_empty() {
            let spend = &answer["cloudSpend"];
            let mut title = "Cloud Boxes".to_string();
            if let Some(total) = dollars(spend["totalUsd"].as_f64()) {
                title = match dollars(spend["capUsd"].as_f64()) {
                    Some(cap) => format!("Cloud Boxes · {total} of {cap} this month"),
                    None => format!("Cloud Boxes · {total} this month"),
                };
            }
            column = column.child(section(&p, &title, box_choices)).child(
                div()
                    .px(px(2.0))
                    .mt(px(-14.0))
                    .text_size(px(12.5))
                    .line_height(px(17.5))
                    .text_color(hsla(p.muted))
                    .child(format!(
                        "On a Cloud Box each chat gets its own worktree of that project, and the box runs (and bills) while it works. A chat Chief of Staff or another GBot starts there asks for your approval first."
                    )),
            );
        }
        let on_box = text(current, "kind") == "cloudBox";
        let computer_use = current["computerUse"] == true;
        let busy = self.computer.busy;
        let use_label = "Use the screen and apps";
        let use_row = h_flex()
            .id("gbot-computer-use")
            .role(gpui::Role::Switch)
            .aria_toggled(a11y_toggled(computer_use))
            .aria_label(use_label)
            .w_full()
            .switch_row_press(
                switch_row_wash(p.foreground),
                busy || on_box,
                bind_press(cx, |this: &mut Self, window, cx| {
                    this.toggle_computer_use(window, cx)
                }),
            )
            .child(row(
                &p,
                use_label,
                modal_switch(&kit, computer_use, busy || on_box),
            ))
            .into_any_element();
        column = column.child(section(&p, "Computer use", vec![use_row])).child(
            div()
                .px(px(2.0))
                .mt(px(-14.0))
                .text_size(px(12.5))
                .line_height(px(17.5))
                .text_color(hsla(p.muted))
                .child(if on_box {
                    "A Cloud Box has no screen to use.".to_string()
                } else {
                    format!(
                        "{bot_name} may open and drive apps on the computer it works on, on its own virtual display (SpaceO) or in the background (cua-driver). macOS only. Off starts its chats without those tools; a Codex GBot at Full access can still run their command-line tools from a shell."
                    )
                }),
        );
        let elsewhere = answer["remoteChats"].as_array().map(Vec::len).unwrap_or(0);
        if elsewhere > 0 {
            column = column.child(
                div()
                    .px(px(2.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(hsla(p.muted))
                    .child(if elsewhere == 1 {
                        "1 chat runs on another computer; it shows under the GBot's row.".to_string()
                    } else {
                        format!("{elsewhere} chats run on other computers; they show under the GBot's row.")
                    }),
            );
        }
        self.computer_frame(column)
    }

    fn computer_frame(&self, column: gpui::Div) -> AnyElement {
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .id("gbot-settings-computer-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.page_scroll)
                    .child(column),
            )
            .child(modal_edge_scrollbar(&self.page_scroll))
            .into_any_element()
    }
}
