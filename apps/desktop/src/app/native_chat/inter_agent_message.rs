use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div, px};
use serde_json::Value;

impl NativeChatView {
    /// CDXC:SessionChat 2026-09-18 DECISION:
    /// User: a message another agent sent with `ghostex agents send` reads as a message from that agent, not as the user's own prompt bubble, and its sender header is never a heading.
    /// SEE-ALSO: packages/gx-chat-core/src/transcript/agent_message.rs parses the header.
    pub(super) fn inter_agent_message_card(
        &self,
        id: &str,
        message: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let sent = &message["interAgentMessage"];
        let session = text(sent, "sessionTitle");
        let body = text(sent, "body");
        let header = div()
            .flex()
            .items_start()
            .gap(px(8.0 * s))
            .child(
                gpui::svg()
                    .path("titlebar/message.svg")
                    .size(px(14.0 * s))
                    .mt(px(4.0 * s))
                    .text_color(p.muted)
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(8.0 * s))
                    .child(
                        div()
                            .text_color(p.foreground)
                            .child(format!("Message from {}", text(sent, "agentName"))),
                    )
                    .when(!session.is_empty(), |heading| {
                        heading.child(div().text_color(p.muted).child(session))
                    }),
            )
            .when(message["queued"] == true, |header| {
                header.child(
                    div()
                        .mt(px(4.0 * s))
                        .text_size(px(11.0 * s))
                        .text_color(p.muted)
                        .child("QUEUED"),
                )
            })
            .into_any_element();
        let body = if body.is_empty() {
            Vec::new()
        } else {
            vec![self.markdown(
                format!("inter-agent:{id}"),
                body,
                &message["markdownReferences"],
                p,
                cx,
            )]
        };
        // React footed this card with the send's delivery status; a waiting send says so here too.
        let actions = self
            .render_startup_delivery(message, p, cx)
            .map_or_else(Vec::new, |status| vec![status]);
        self.status_card_with_header(header, body, actions, p)
    }
}
