//! The transcript as its own cached view, so a chat frame that only moved an indicator or the
//! composer reuses the rows instead of laying every visible one out again.

use gpui::{
    AnyElement, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _,
    WeakEntity, Window, div, list, px,
};

use super::appearance::ChatAppearance;
use super::state::NativeChatView;
use crate::app::helpers::indicator_animation;

/// CDXC:SessionChat 2026-09-23 WHY:
/// gpui renders a notified view whole, and a view's redraw dirties its ancestors, so the working strip's pulse, the compaction bar, the composer tween and every spinner in the chat chrome re-laid out the whole transcript per frame; that is what kept the indicators throttled to seven frames per second and made them step.
/// The rows now live in this view, drawn cached inside the chat: a chat render asked for by an indicator frame alone (indicator_animation.rs) leaves it as it was, and every other chat render marks it for drawing again in the same frame, so nothing the rows read from the chat can go stale. Its own state stays on `NativeChatView`; this view only draws.
pub(crate) struct TranscriptHost {
    chat: WeakEntity<NativeChatView>,
}

impl Render for TranscriptHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.chat.upgrade() {
            Some(chat) => chat.update(cx, |chat, cx| chat.render_transcript_body(window, cx)),
            None => div().into_any_element(),
        }
    }
}

impl NativeChatView {
    /// The transcript region's content: the row list, or the welcome that stands in for an empty one.
    fn render_transcript_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = ChatAppearance::current(&self.snapshot).on_window_glass(
            crate::app::helpers::window_glass_active_for(self.main_window),
        );
        let content = if self.list.item_count() == 0 {
            let state = self.snapshot.clone();
            if let Some(stage) = self.transcript_loading_stage(&state) {
                return self.render_loading_hold(stage, &p, cx);
            }
            self.render_empty_transcript_region(&state, &p, cx)
        } else {
            let transcript = list(
                self.list.clone(),
                cx.processor(|this, index, window, cx| this.transcript_row(index, window, cx)),
            )
            .flex_1()
            .min_h_0()
            .w_full()
            .pb(px(self.transcript_inset));
            self.scrollable_transcript(transcript, cx)
        };
        self.reveal_transcript(
            div()
                .size_full()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .child(content),
            window,
        )
    }

    /// The cached transcript view, created on the first draw, marked for drawing again on every chat
    /// render that an indicator frame did not ask for by itself.
    pub(super) fn render_transcript_host(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat = cx.weak_entity();
        let host = self
            .transcript_host
            .get_or_insert_with(|| cx.new(|_| TranscriptHost { chat }))
            .clone();
        if !indicator_animation::take_animation_only_render(cx.entity_id()) {
            window.render_view_this_frame(host.entity_id());
        }
        gpui::AnyView::from(host)
            .cached(gpui::StyleRefinement::default().flex_1().min_h_0().w_full())
            .into_any_element()
    }
}
