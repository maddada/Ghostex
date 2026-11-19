//! The Run on row above a new thread's composer: the computer (its icon and name) or an agentbox box, drawn from the
//! document's `runLocation` key. gx-chat-core decides when it shows and what it offers
//! (`packages/gx-chat-core/src/menus/run_location.rs`); this file only lays it out and reports the
//! pick. The phone draws the same row in `apps/mobile/app/src/chat/native/RunLocationRow.tsx`.
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::json;

/// The chip glyphs, the New Thread picker's.
fn icon_path(icon: &str) -> &'static str {
    match icon {
        "computer" => "titlebar/device-desktop.svg",
        "server" => "titlebar/server.svg",
        "cloud" => "titlebar/cloud.svg",
        _ => "titlebar/box.svg",
    }
}

impl NativeChatView {
    pub(crate) fn render_run_location(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let row = self
            .snapshot
            .get("runLocation")
            .filter(|row| row.is_object())?;
        let options = row["options"].as_array()?;
        let s = p.scale;
        let selected = row["selected"].as_str().unwrap_or("local").to_owned();
        let busy = row["busy"] == true;
        let mut chips = div()
            .id("chat-run-location-chips")
            .flex()
            .items_center()
            .gap(px(4.0 * s))
            .min_w_0()
            .overflow_x_scroll();
        for (index, option) in options.iter().enumerate() {
            let Some(run_location) = option["runLocation"].as_str() else {
                continue;
            };
            let label = option["label"].as_str().unwrap_or(run_location).to_owned();
            let on = run_location == selected;
            let tooltip = option["tooltip"].as_str().map(str::to_owned);
            let target = run_location.to_owned();
            chips = chips.child(
                div()
                    .id(("chat-run-location", index))
                    .role(gpui::Role::Button)
                    .aria_label(format!("Run on {label}"))
                    .flex_shrink_0()
                    .h(px(24.0 * s))
                    .px(px(10.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(5.0 * s))
                    .rounded_full()
                    .border_1()
                    .border_color(if on {
                        p.border
                    } else {
                        gpui::transparent_black()
                    })
                    .when(on, |chip| chip.bg(p.border))
                    .text_color(if on { p.primary } else { p.muted })
                    .when(!on && !busy, |chip| {
                        chip.chat_cursor_pointer()
                            .hover(|style| style.bg(p.border.opacity(0.6)))
                    })
                    .when(busy && on, |chip| chip.opacity(0.6))
                    .child(
                        svg()
                            .path(match option["icon"].as_str().unwrap_or("box") {
                                // The computer wears its own icon, the one its computer tab draws.
                                "computer" => crate::app::gx_chat::session_directory::machine_icon(
                                    &self.config.machine_id,
                                ),
                                icon => icon_path(icon),
                            })
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(if on { p.primary } else { p.muted }),
                    )
                    .child(
                        div()
                            .max_w(px(120.0 * s))
                            .text_ellipsis()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(label),
                    )
                    .when_some(tooltip, |chip, tooltip| {
                        chip.tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                        })
                    })
                    .when(!on && !busy, |chip| {
                        chip.on_click(cx.listener(move |chat, _, _, cx| {
                            chat.invoke(
                                json!({"type": "switchDraftRunLocation", "runLocation": target}),
                                cx,
                            );
                        }))
                    }),
            );
        }
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.0 * s))
                .px(px(6.0 * s))
                .text_size(px(12.0 * s))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(p.muted)
                        .child(row["label"].as_str().unwrap_or("Run on").to_owned()),
                )
                .child(chips)
                .into_any_element(),
        )
    }
}
