/*!
The loading hold a chat shows while its transcript is still being read, and the fade the
transcript arrives with once it is ready.

CDXC:SessionChat 2026-09-24 DECISION:
User: no skeleton when a GPUI chat view is focused, because the transcript loads within 500ms; the chat fades in as soon as it is ready instead. The transcript area stays blank during the hold (the real composer keeps its place at the bottom) and only the core's `loadingNotice` (a read running long, with Try now) can appear in it. This supersedes the 2026-09-19 decision to draw a skeleton the moment a transcript starts loading; React chat keeps its skeleton.
The one exception is the user's 2026-10-08 decision (CDXC:SessionChat in packages/gx-chat-core/src/session/constants.rs): while Ghostex's service is not answering, the core sets `transcriptSkeleton` and the hold draws the shared skeleton rows, with no text, above the usual composer.
*/

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::LazyLock;
use std::time::Duration;
use web_time::Instant;

/// The shared transcript skeleton geometry, the rows React's SessionChatLoadingState drew.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkeletonGeometry {
    top_padding: f32,
    row_gap: f32,
    bar_height: f32,
    bar_gap: f32,
    bubble_height: f32,
    bubble_radius: f32,
    tint: f32,
    pulse_ms: u64,
    pulse_min_opacity: f32,
    rows: Vec<SkeletonRow>,
}

#[derive(Deserialize)]
struct SkeletonRow {
    role: String,
    widths: Vec<f32>,
}

static SKELETON: LazyLock<SkeletonGeometry> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/transcript-skeleton.json"
    ))
    .expect("shared transcript skeleton")
});

const FADE: Duration = Duration::from_millis(250);
/// A hold shorter than this was not seen as a blank, so its content needs no fade.
const FADE_AFTER_HOLD: Duration = Duration::from_millis(60);

/// When the loading hold went up, and when the transcript that replaced it began fading in.
///
/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: fade in only when it is actually needed; a session whose chat is already loaded switches instantly. So content fades only after the blank hold was on screen for `FADE_AFTER_HOLD`; a view created with its transcript already read, or a re-read that finishes within a frame or two, draws at once.
/// WHY: the fade's clock lives here, not in a gpui `with_animation`: that keeps its start in per-frame element state, which gpui drops for a chat that is not drawn, so every switch back to an already loaded chat replayed the fade.
#[derive(Default)]
pub(crate) struct TranscriptReveal {
    hold_since: Option<Instant>,
    fade_started: Option<Instant>,
}

impl NativeChatView {
    /// The hold stage while the transcript is still being read. A chat whose host has not published
    /// its first snapshot yet is reading its transcript too.
    pub(super) fn transcript_loading_stage<'a>(&self, state: &'a Value) -> Option<&'a str> {
        state["loadingStage"]
            .as_str()
            .or_else(|| (state["status"].is_null() && self.error.is_none()).then_some("indicator"))
    }

    /// The loading hold: an empty transcript region, the skeleton rows while Ghostex's service is
    /// not answering (`transcriptSkeleton`), or the core's notice and its Try now button once a read
    /// runs long (`loadingNotice`).
    pub(super) fn render_loading_hold(
        &mut self,
        state: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.transcript_reveal
            .hold_since
            .get_or_insert_with(Instant::now);
        let s = p.scale;
        if state["transcriptSkeleton"] == true {
            return transcript_skeleton(p);
        }
        // `size_full`, not `flex_1`: this element is the cached transcript view's root, whose parent
        // is not a flex column, so `flex_1` left it content-height and pinned to the top.
        div()
            .size_full()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .when_some(state["loadingNotice"].as_object(), |this, notice| {
                let text = |key: &str| notice.get(key).and_then(Value::as_str).unwrap_or_default();
                let title = text("title").to_owned();
                let detail = text("detail").to_owned();
                this.child(
                    div()
                        .id("chat-transcript-loading")
                        .role(gpui::Role::Status)
                        .aria_label(title.clone())
                        .max_w_full()
                        .min_w_0()
                        .px(px(24.0 * s))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(6.0 * s))
                        .text_center()
                        .text_color(p.muted)
                        .child(title)
                        .when(!detail.is_empty(), |column| {
                            column.child(div().text_color(p.muted.opacity(0.75)).child(detail))
                        })
                        .child(div().mt(px(8.0 * s)).child(self.chat_button(
                            "retry-chat".into(),
                            text("action").to_owned(),
                            json!({"type":"retry"}),
                            p,
                            cx,
                        ))),
                )
            })
            .into_any_element()
    }

    /// The transcript region, fading in when it replaces a hold that was on screen.
    pub(super) fn reveal_transcript(&mut self, region: Div, window: &mut Window) -> AnyElement {
        let reveal = &mut self.transcript_reveal;
        if let Some(hold_since) = reveal.hold_since.take() {
            reveal.fade_started = (hold_since.elapsed() >= FADE_AFTER_HOLD
                && !crate::app::helpers::gpui_macos_reduce_motion_enabled())
            .then(Instant::now);
        }
        let Some(started) = reveal.fade_started else {
            return region.into_any_element();
        };
        let progress = started.elapsed().as_secs_f32() / FADE.as_secs_f32();
        if progress >= 1.0 {
            reveal.fade_started = None;
            return region.into_any_element();
        }
        window.request_animation_frame();
        // Not ease-out-quint: that is ~95% opaque a third of the way in, which reads as a pop.
        region
            .opacity(gpui::ease_in_out(progress))
            .into_any_element()
    }
}

/// The shared skeleton look for the chat's other loading skeletons (the Session details panel's
/// cards): its tint over `p`, its pulse period and the opacity the pulse dips to.
pub(super) fn skeleton_look(p: &ChatAppearance) -> (gpui::Hsla, Duration, f32) {
    let g = &*SKELETON;
    (
        p.foreground.opacity(g.tint),
        Duration::from_millis(g.pulse_ms),
        g.pulse_min_opacity,
    )
}

/// The skeleton rows, in the transcript's own column, pulsing like the other skeletons.
fn transcript_skeleton(p: &ChatAppearance) -> AnyElement {
    use crate::app::helpers::ThrottledAnimationExt as _;
    let s = p.scale;
    let g = &*SKELETON;
    let tint = p.foreground.opacity(g.tint);
    let column = div()
        .w_full()
        .max_w(px(768.0 * s))
        .px(px(16.0 * s))
        .pt(px(g.top_padding * s))
        .when_some(p.transcript_width, |this, width| {
            this.max_w(gpui::relative(1.0)).w(gpui::relative(width))
        })
        .flex()
        .flex_col()
        .gap(px(g.row_gap * s))
        .children(g.rows.iter().map(|row| {
            if row.role == "user" {
                div().w_full().flex().justify_end().child(
                    div()
                        .w(gpui::relative(row.widths.first().copied().unwrap_or(0.4)))
                        .h(px(g.bubble_height * s))
                        .rounded(px(g.bubble_radius * s))
                        .bg(tint),
                )
            } else {
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(g.bar_gap * s))
                    .children(row.widths.iter().map(|width| {
                        div()
                            .w(gpui::relative(*width))
                            .h(px(g.bar_height * s))
                            .rounded(px(g.bar_height * s / 2.0))
                            .bg(tint)
                    }))
            }
        }));
    let region = div()
        .id("chat-transcript-skeleton")
        .role(gpui::Role::Status)
        .aria_label("Loading conversation\u{2026}")
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .overflow_hidden()
        .child(column);
    if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
        return region.into_any_element();
    }
    let min = g.pulse_min_opacity;
    region
        .with_throttled_animation(
            "chat-transcript-skeleton-pulse",
            Duration::from_millis(g.pulse_ms),
            move |region, frame| {
                let dip = gpui::ease_in_out(if frame < 0.5 {
                    frame * 2.0
                } else {
                    (1.0 - frame) * 2.0
                });
                region.opacity(1.0 - (1.0 - min) * dip)
            },
        )
        .into_any_element()
}
