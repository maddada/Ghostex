//! The Cloud Box part of the new chat's tray: the boot progress card the first message waits with
//! (Q1009). The box picker opens from the bar's last part (`worktree_tray.rs`). gx-chat-core builds
//! both (`packages/gx-chat-core/src/menus/cloud_box.rs`): the picker is a list of the chat menu's
//! own rows and the card a list of steps, so this file only lays them out. The phone draws the same
//! in `apps/mobile/app/src/chat/native/composer/BoxPicker.tsx`.
use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, IntoElement, ParentElement as _, Styled as _, div, px, rgb, svg};
use serde_json::Value;

const DONE_COLOR: u32 = 0x22c55e;
const FAILED_COLOR: u32 = 0xef4444;

impl NativeChatView {
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
