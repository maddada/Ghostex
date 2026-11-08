//! The Routines tab: what this GBot does on its own, on a schedule. One row per routine with its
//! schedule in words, its next run (or Paused), the check it runs first and its last result, and
//! Pause or Resume, Run now and Remove. A routine is set by asking the GBot in its chat; its card
//! there has Edit and Undo.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (plan 3.2, Q2104 A): the GBot Settings window has a Routines tab, "list with Pause/Edit/Run now". Changing what a routine does or when happens in the Automate view (the routine card's Edit) or by asking the GBot, so this tab only lists, pauses, resumes, runs and removes.
//! SEE-ALSO: server/src/gbots/routines.rs (`routines`, `routinePause`, `routineResume`, `routineRunNow`, `routineRemove`), apps/mobile/app/src/screens/sessions-screen/GbotRoutinesView.tsx (the phone's list).
use super::super::native_modal_kit::*;
use super::parts::page_header;
use super::window::{GbotSettingsModalCommand, GpuiGbotSettingsModalWindow};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyWindowHandle, App, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, WeakEntity,
    Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// The Routines tab's state inside the window.
#[derive(Default)]
pub(crate) struct GbotRoutinesTab {
    /// The routines as gxserver last listed them; `None` until the first read answers.
    pub(super) list: Option<Vec<Value>>,
    pub(super) requested: bool,
    /// The routine an op is on its way for (`""` for a read).
    pub(super) busy: Option<String>,
    pub(super) error: Option<String>,
    /// A removal waiting for its second press.
    pub(super) confirm_remove: Option<String>,
}

/// Where a routine op's answer goes: the window that asked.
pub(crate) struct GbotRoutinesReply {
    view: WeakEntity<GpuiGbotSettingsModalWindow>,
    window: AnyWindowHandle,
}

impl GbotRoutinesReply {
    /// Hands the window gxserver's `routines` answer after the op, or its refusal.
    pub(crate) fn finish(self, result: Result<Value, String>, cx: &mut App) {
        let view = self.view;
        let _ = self.window.update(cx, |_root, _window, cx| {
            view.update(cx, |modal, cx| modal.finish_routines(result, cx))
        });
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A run's status in words.
fn run_status(status: &str) -> &'static str {
    match status {
        "findings" => "Found something",
        "no_findings" => "Nothing to report",
        "needs_attention" => "Needs you",
        "failed" => "Failed",
        "running" | "queued" => "Running now",
        "cancelled" => "Cancelled",
        _ => "Ran",
    }
}

impl GpuiGbotSettingsModalWindow {
    /// Reads the routines the first time the tab shows.
    pub(super) fn ensure_routines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.routines.requested {
            return;
        }
        self.routines.requested = true;
        self.routine_op(None, String::new(), window, cx);
    }

    /// Sends `op` (none for a plain read) for `routine_id`, then reads the list again.
    fn routine_op(
        &mut self,
        op: Option<Value>,
        routine_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.routines.busy = Some(routine_id);
        self.routines.error = None;
        cx.notify();
        let reply = GbotRoutinesReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
        };
        (self.host)(
            GbotSettingsModalCommand::Routines {
                bot_id: self.original.bot_id.clone(),
                op,
                reply,
            },
            cx,
        );
    }

    fn finish_routines(&mut self, result: Result<Value, String>, cx: &mut Context<Self>) {
        self.routines.busy = None;
        self.routines.confirm_remove = None;
        match result {
            Ok(answer) => {
                self.routines.list =
                    Some(answer["routines"].as_array().cloned().unwrap_or_default());
            }
            Err(message) => self.routines.error = Some(message),
        }
        cx.notify();
    }

    fn routine_button(
        &self,
        id: &'static str,
        label: &str,
        op: &'static str,
        routine_id: &str,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let routine_id = routine_id.to_string();
        let button = modal_action_button(
            &self.kit,
            id,
            label.to_string(),
            None,
            if op == "routineRemove" {
                ModalButtonTone::Danger
            } else {
                ModalButtonTone::Neutral
            },
            disabled,
            move |this: &mut Self, window, cx| {
                if op == "routineRemove"
                    && this.routines.confirm_remove.as_deref() != Some(routine_id.as_str())
                {
                    this.routines.confirm_remove = Some(routine_id.clone());
                    cx.notify();
                    return;
                }
                this.routine_op(
                    Some(json!({ "op": op, "routine": routine_id })),
                    routine_id.clone(),
                    window,
                    cx,
                );
            },
            cx,
        );
        h_flex()
            .flex_shrink_0()
            .w(px(96.0))
            .child(button)
            .into_any_element()
    }

    fn routine_row(&self, routine: &Value, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let id = text(routine, "id");
        let paused = routine["paused"] == true;
        let busy = self.routines.busy.is_some();
        let mut lines = vec![text(routine, "scheduleSentence").to_string()];
        lines.push(if paused {
            "Paused.".to_string()
        } else {
            match routine["nextLabel"].as_str() {
                Some(next) => format!("Next: {next}"),
                None => "No more runs.".to_string(),
            }
        });
        if let Some(check) = routine["precheckSentence"].as_str() {
            let failed = routine["check"]["lastCheckOutcome"] == "failed";
            lines.push(if failed {
                format!(
                    "Checks {check} first. Its last check failed: {}",
                    text(&routine["check"], "lastCheckNote")
                )
            } else {
                format!("Checks {check} first, and runs only when that changes.")
            });
        }
        if routine["deliver"] == "chief" {
            lines.push("Tells Chief of Staff when it finds something.".to_string());
        }
        if let Some(run) = routine["lastRun"].as_object() {
            let summary = run
                .get("summary")
                .and_then(Value::as_str)
                .and_then(|summary| summary.lines().find(|line| !line.trim().is_empty()))
                .unwrap_or_default();
            let status = run_status(run.get("status").and_then(Value::as_str).unwrap_or(""));
            lines.push(if summary.is_empty() {
                format!("Last run: {status}.")
            } else {
                format!("Last run: {status}. {summary}")
            });
        }
        let removing = self.routines.confirm_remove.as_deref() == Some(id);
        let buttons = h_flex()
            .flex_shrink_0()
            .gap(px(8.0))
            .child(self.routine_button(
                "gbot-routine-pause",
                if paused { "Resume" } else { "Pause" },
                if paused {
                    "routineResume"
                } else {
                    "routinePause"
                },
                id,
                busy,
                cx,
            ))
            .child(self.routine_button(
                "gbot-routine-run",
                "Run now",
                "routineRunNow",
                id,
                busy,
                cx,
            ))
            .child(self.routine_button(
                "gbot-routine-remove",
                if removing { "Remove?" } else { "Remove" },
                "routineRemove",
                id,
                busy,
                cx,
            ));
        // Each row is its own id scope, so its buttons' ids stay apart from the other rows'.
        v_flex()
            .id(SharedString::from(format!("gbot-routine-{id}")))
            .w_full()
            .min_w_0()
            .px(px(20.0))
            .py(px(14.0))
            .gap(px(10.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(19.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child(text(routine, "name").to_string()),
            )
            .child(v_flex().w_full().min_w_0().gap(px(3.0)).children(
                lines.into_iter().enumerate().map(|(index, line)| {
                    div()
                        .w_full()
                        .min_w_0()
                        .whitespace_normal()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(hsla(if index == 0 { p.foreground } else { p.muted }))
                        .child(line)
                }),
            ))
            .child(buttons)
            .into_any_element()
    }

    pub(super) fn render_routines(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut column = v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .pr(px(16.0))
            .gap(px(16.0))
            .child(page_header(
                &p,
                "Routines",
                "What this GBot does on its own, on a schedule. A routine's check runs first and costs nothing while nothing changes.",
            ));
        if let Some(error) = &self.routines.error {
            column = column.child(modal_error(&self.kit, error.clone()));
        }
        let card = |content: AnyElement| {
            div()
                .w_full()
                .rounded(px(MODAL_RADIUS_SECTION))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.raised))
                .overflow_hidden()
                .child(content)
                .into_any_element()
        };
        let message = |line: &str| {
            div()
                .w_full()
                .px(px(20.0))
                .py(px(18.0))
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(p.muted))
                .child(line.to_string())
                .into_any_element()
        };
        let body = match &self.routines.list {
            None => card(message("Reading its routines…")),
            Some(list) if list.is_empty() => card(message(&format!(
                "No routines yet. Ask {} in its chat, for example \"every weekday at 8, check my inbox folder and tell me what's new\".",
                match self.original.name.trim() {
                    "" => "the GBot",
                    name => name,
                }
            ))),
            Some(list) => {
                let hairline = hsla(p.hairline);
                let rows: Vec<AnyElement> = list
                    .iter()
                    .enumerate()
                    .map(|(index, routine)| {
                        div()
                            .w_full()
                            .when(index > 0, |this| this.border_t_1().border_color(hairline))
                            .child(self.routine_row(routine, cx))
                            .into_any_element()
                    })
                    .collect();
                card(v_flex().w_full().children(rows).into_any_element())
            }
        };
        column
            .child(
                div()
                    .id("gbot-settings-routines-scroll")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.page_scroll)
                    .child(body),
            )
            .into_any_element()
    }
}
