//! The completed-work row while its history is still being read, or after the
//! read failed. React showed the same line and Retry button in
//! `session-chat-deferred-work.tsx`; the core publishes the per-row state as
//! `deferredWork` (gx-chat-core `transcript/deferred_work.rs`).

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::{Value, json};

impl NativeChatView {
    pub(super) fn deferred_work_notice(
        &self,
        item: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let id = text(item, "id");
        let state = &self.snapshot["deferredWork"][&id];
        if !state.is_object() {
            return None;
        }
        let s = p.scale;
        let error = text(state, "error");
        let failed = !error.is_empty();
        let retry = json!({"type":"loadWork","id":item["id"],"work":item["deferred"]});
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.0 * s))
                .px(px(12.0 * s))
                .py(px(8.0 * s))
                .text_color(if failed { p.error() } else { p.muted })
                .child(if failed {
                    error
                } else {
                    "Loading work details…".to_string()
                })
                .when(failed, |this| {
                    this.child(
                        div()
                            .id(format!("retry-work:{id}"))
                            .flex_shrink_0()
                            .px(px(8.0 * s))
                            .py(px(2.0 * s))
                            .rounded(px(6.0 * s))
                            .text_color(p.muted)
                            .chat_cursor_pointer()
                            .hover(|style| style.bg(p.border.opacity(0.4)))
                            .child("Retry")
                            .on_click(
                                cx.listener(move |view, _, _, cx| view.invoke(retry.clone(), cx)),
                            ),
                    )
                })
                .into_any_element(),
        )
    }
}
