//! The text only an open row shows: a tool's arguments and result, a file card's diff.
//!
//! CDXC:SessionChat 2026-09-21 DECISION: User chose to stop sending text a collapsed row never shows. A row asks for its detail while it is drawn open (or, with File edit previews on, while a file card is on screen), and the host sends only those (`rowDetails` in gx-chat-core).

use super::state::NativeChatView;
use gpui::{Context, Window};
use serde_json::{Value, json};

impl NativeChatView {
    /// Ask for a row's detail for as long as the row keeps being drawn needing it; `Null` until it arrives.
    pub(super) fn row_detail(
        &mut self,
        key: &str,
        kind: &str,
        message_id: &str,
        index: u64,
    ) -> Value {
        self.detail_demand.insert(
            key.to_string(),
            json!({"key":key,"kind":kind,"messageId":message_id,"index":index}),
        );
        self.row_details[key].clone()
    }

    /// After this frame's rows are drawn, tell the host which details they asked for, if that changed.
    pub(super) fn schedule_row_detail_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.detail_sync_scheduled {
            return;
        }
        self.detail_sync_scheduled = true;
        let chat = cx.weak_entity();
        window.on_next_frame(move |_, cx| {
            let _ = chat.update(cx, |chat, cx| {
                chat.detail_sync_scheduled = false;
                let demand = std::mem::take(&mut chat.detail_demand);
                if demand == chat.detail_sent {
                    return;
                }
                let open: Vec<Value> = demand.values().cloned().collect();
                chat.detail_sent = demand;
                chat.invoke(json!({"type":"rowDetails","open":open}), cx);
            });
        });
    }
}
