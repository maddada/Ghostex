/*!
The loading hold a chat shows while its transcript is still being read, and the fade the
transcript arrives with once it is ready.

CDXC:SessionChat 2026-09-24 DECISION:
User: no skeleton when a GPUI chat view is focused, because the transcript loads within 500ms; the chat fades in as soon as it is ready instead. The transcript area stays blank during the hold (the real composer keeps its place at the bottom) and only the late Retry row can appear in it. This supersedes the 2026-09-19 decision to draw a skeleton the moment a transcript starts loading; React chat keeps its skeleton.
*/

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde_json::{Value, json};
use std::time::Duration;
use web_time::Instant;

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

    /// The loading hold: an empty transcript region, with the Retry row once the read runs long.
    pub(super) fn render_loading_hold(
        &mut self,
        stage: &str,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.transcript_reveal
            .hold_since
            .get_or_insert_with(Instant::now);
        let s = p.scale;
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .when(stage == "retry", |this| {
                this.child(
                    div()
                        .id("chat-transcript-loading")
                        .role(gpui::Role::Status)
                        .aria_label("Loading conversation…")
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(12.0 * s))
                        .text_color(p.muted)
                        .child("Still loading this conversation.")
                        .child(self.chat_button(
                            "retry-chat".into(),
                            "Retry".into(),
                            json!({"type":"retry"}),
                            p,
                            cx,
                        )),
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
