use crate::app::native_chat::appearance::ChatAppearance;
use crate::*;
use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div, px,
};
use serde::Deserialize;
use std::sync::LazyLock;
use std::time::Duration;

/// The part of the shared skeleton geometry the GPUI skeletons still read.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptSkeleton {
    tint: f32,
    pulse_ms: u64,
    pulse_min_opacity: f32,
}

/// CDXC:SessionChat 2026-09-24 SEE-ALSO:
/// The shared packages/gx-chat-core/visual/transcript-skeleton.json that React's SessionChatLoadingState
/// draws; the GPUI chat no longer draws transcript skeleton rows (native_chat/transcript_reveal.rs), and only its tint
/// and pulse are still read here, by the other GPUI skeletons.
static SKELETON: LazyLock<TranscriptSkeleton> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../packages/gx-chat-core/visual/transcript-skeleton.json"
    ))
    .expect("shared transcript skeleton")
});

/// The shared skeleton tint, for skeletons drawn outside the chat.
pub(crate) fn skeleton_tint() -> f32 {
    SKELETON.tint
}

/// The shared skeleton pulse: its period and the opacity it dips to.
pub(crate) fn skeleton_pulse() -> (Duration, f32) {
    (
        Duration::from_millis(SKELETON.pulse_ms),
        SKELETON.pulse_min_opacity,
    )
}

impl GhostexGpuiApp {
    /// The skeleton for a chat-mode tab that has no chat view yet (a staged session or a placeholder awaiting its created session).
    ///
    /// CDXC:Theming 2026-09-23 WHY:
    /// This is what an Agents pane shows at startup for the few seconds before its chat view mounts, and it is drawn by
    /// the app rather than by a chat view, so it never passed through `on_window_glass`: under window glass it painted
    /// the theme's chat colour fully opaque over the frosted work area. It takes the settled chat's glass layering: no
    /// fill of its own, and the composer and skeleton bars as washes.
    pub(crate) fn render_session_chat_skeleton(&self) -> AnyElement {
        let glass = crate::app::helpers::window_glass_active();
        session_chat_skeleton(
            &ChatAppearance::current(&serde_json::Value::Null).on_window_glass(glass),
            glass,
        )
    }
}

/// CDXC:SessionChat 2026-09-23 DECISION:
/// User: never show a transcript skeleton alone; keep the composer and status line at the bottom while the session is still being mapped. A mounted chat owns the editable input; this brief pre-view state reserves the same regions, including while a held next-tab key defers mounting.
/// Since the 2026-09-24 decision in native_chat/transcript_reveal.rs the transcript area above them stays empty: no skeleton rows, the chat fades in once ready.
fn session_chat_skeleton(p: &ChatAppearance, glass: bool) -> AnyElement {
    let s = p.scale;
    div()
        .size_full()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .overflow_hidden()
        .bg(if glass {
            gpui::transparent_black()
        } else {
            p.background
        })
        // The transcript area stays empty: the chat view fades its transcript in (transcript_reveal.rs).
        .child(
            div()
                .id("session-chat-pane-skeleton")
                .role(gpui::Role::Status)
                .aria_label("Loading conversation…")
                .w_full()
                .flex_1()
                .min_h_0(),
        )
        .child(
            div().w_full().max_w(px(768.0 * s)).flex_shrink_0()
                .flex().flex_col().gap(px(8.0 * s))
                .px(px(16.0 * s)).pt(px(16.0 * s)).pb(px(12.0 * s))
                .child(
                    div().w_full().rounded(px(22.0 * s)).border_1()
                        .border_color(p.composer_border).bg(p.composer_background)
                        .px(px(16.0 * s)).py(px(10.0 * s))
                        .flex().flex_col().gap(px(6.0 * s))
                        .child(div().h(px(72.0 * s)).text_size(px(14.0 * s))
                            .line_height(px(24.0 * s)).text_color(p.muted.opacity(0.6))
                            .child(ghostex_gx_chat_core::composer::policy::DESKTOP_COMPOSER_PLACEHOLDER))
                        .child(div().h(px(28.0 * s)).flex().items_center().justify_between()
                            .child(div().w(px(52.0 * s)).h(px(10.0 * s)).rounded_full().bg(p.primary.opacity(0.24)))
                            .child(div().size(px(24.0 * s)).rounded(px(6.0 * s)).bg(p.primary.opacity(0.24)))),
                )
                .child(crate::app::native_chat::context_meter::status_line_skeleton(p)),
        )
        .into_any_element()
}
