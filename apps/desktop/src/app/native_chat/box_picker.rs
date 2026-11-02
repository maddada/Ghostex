//! The Cloud Box part of the new chat's tray: the chip beside the segments that opens the box
//! picker, and the boot progress card the first message waits with (Q1009). gx-chat-core builds
//! both (`packages/gx-chat-core/src/menus/cloud_box.rs`): the picker is a list of the chat menu's
//! own rows and the card a list of steps, so this file only lays them out. The phone draws the same
//! in `apps/mobile/app/src/chat/native/composer/BoxPicker.tsx`.
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb, size, svg,
};
use serde_json::Value;

const PICKER_TRIGGER: &str = "chat-cloud-box-picker";
const PICKER_WIDTH: f32 = 320.0;
const STAR_COLOR: u32 = 0xf5b301;
const DONE_COLOR: u32 = 0x22c55e;
const FAILED_COLOR: u32 = 0xef4444;

impl NativeChatView {
    /// The chip that names what the box starts from (★ Base, Blank box, a running box) and opens
    /// the picker.
    pub(crate) fn render_cloud_box_chip(
        &self,
        cloud: &Value,
        p: &ChatAppearance,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let chip = &cloud["chip"];
        let label = chip["label"].as_str().unwrap_or("Cloud Box").to_owned();
        let star = chip["star"] == true;
        let rows: Vec<Value> = cloud["menu"].as_array().cloned().unwrap_or_default();
        div()
            .id(PICKER_TRIGGER)
            .role(gpui::Role::Button)
            .aria_label(format!("Cloud Box: {label}"))
            .flex_shrink_0()
            .h(px(26.0 * s))
            .px(px(8.0 * s))
            .flex()
            .items_center()
            .gap(px(5.0 * s))
            .rounded(px(7.0 * s))
            .border_1()
            .border_color(p.composer_border)
            .text_color(p.primary)
            .when(!busy, |chip| {
                chip.chat_cursor_pointer()
                    .hover(|style| style.bg(p.border))
                    .on_click(cx.listener(move |chat, _, window, cx| {
                        if chat.chat_menu_toggled_shut(PICKER_TRIGGER, cx) {
                            return;
                        }
                        let at = window.mouse_position();
                        chat.show_chat_menu(
                            rows.clone(),
                            Bounds::new(at, size(px(0.0), px(0.0))),
                            PICKER_WIDTH,
                            window,
                            cx,
                        );
                    }))
            })
            .child(if star {
                svg()
                    .path("titlebar/star.svg")
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(rgb(STAR_COLOR))
                    .into_any_element()
            } else {
                svg()
                    .path("titlebar/cloud.svg")
                    .size(px(13.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted)
                    .into_any_element()
            })
            .child(
                div()
                    .max_w(px(200.0 * s))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(label),
            )
            .child(
                svg()
                    .path("titlebar/chevron-down.svg")
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .into_any_element()
    }

    /// The boot progress card under the tray's row: the waiting message, then one line per step
    /// with its time.
    pub(crate) fn render_cloud_box_card(&self, card: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        let mut lines = div().flex().flex_col().gap(px(3.0 * s)).pt(px(6.0 * s));
        if let Some(message) = card["message"].as_str().filter(|text| !text.is_empty()) {
            lines = lines.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .min_w_0()
                    .pb(px(2.0 * s))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(p.primary)
                            .child(message.to_owned()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(p.muted)
                            .child("· waiting for the box"),
                    ),
            );
        }
        for step in card["steps"].as_array().into_iter().flatten() {
            let state = step["state"].as_str().unwrap_or("waiting");
            let (icon, color) = match state {
                "done" => ("titlebar/check.svg", Some(rgb(DONE_COLOR))),
                "running" => ("titlebar/loader2.svg", None),
                "failed" => ("titlebar/alert-triangle.svg", Some(rgb(FAILED_COLOR))),
                _ => ("titlebar/circle.svg", None),
            };
            let seconds = step["seconds"]
                .as_f64()
                .filter(|_| state == "done")
                .map(|seconds| format!("{seconds:.1} s"));
            let label = match step["detail"].as_str().filter(|_| state == "failed") {
                Some(detail) => format!("{} · {detail}", step["label"].as_str().unwrap_or("")),
                None => step["label"].as_str().unwrap_or("").to_owned(),
            };
            lines = lines.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0 * s))
                    .min_w_0()
                    .text_color(if state == "waiting" {
                        p.muted
                    } else {
                        p.primary
                    })
                    .child({
                        let icon = svg().path(icon).size(px(12.0 * s)).flex_shrink_0();
                        match color {
                            Some(color) => icon.text_color(color),
                            None => icon.text_color(p.muted),
                        }
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(label),
                    )
                    .when_some(seconds, |line, seconds| {
                        line.child(div().flex_shrink_0().text_color(p.muted).child(seconds))
                    }),
            );
        }
        lines.into_any_element()
    }
}
