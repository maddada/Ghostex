//! The Links pill in the chat box's pill strip (pills_strip.rs): the session's links in the sidebar
//! badge's text (gx-core `SessionLinks::chat_pill`, where the user's decision lives), drawn faintly,
//! the strip's last pill. A click opens the session's links list above the chat box (the desktop's
//! links popup, the web build's card), which the app owns.
//!
//! CDXC:SessionLinks 2026-10-29 DECISION:
//! User, of the Links pill in the strip: "let's please make the green here much more faint" and "move this pill to be rightmost one of the pills (the Links one)". It is the strip's last pill, after Background, the ID in a medium weight. Supersedes its place between Workers and Tasks (2026-10-28). Its colours are the sidebar badge's, muted to the project header's diff numbers (`session_link_badge_color` in native_sidebar/link_chips.rs, where the user's 2026-10-29 words on that green live), and it reads "#270+" with more links.
//!
//! CDXC:SessionLinks 2026-10-28 SEE-ALSO:
//! The phone draws the same pill in apps/mobile/app/src/chat/native/cards/ChatPills.tsx (`LinksPill`), from apps/mobile/app/src/chat/native/sessionLinks.ts `sessionLinksChatPill`.

use super::{appearance::ChatAppearance, cursor::ChatCursor as _, state::NativeChatView};
use crate::app::native_sidebar::link_chips::session_link_badge_color;
use ghostex_gx_core::{SessionKey, SessionLinks, SessionLinksChatPill};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, canvas, div, px, svg,
};
use serde_json::json;

impl NativeChatView {
    /// The gxserver session this chat shows, as the sidebar store keys it.
    fn links_session_key(&self) -> SessionKey {
        if self.config.machine_id == crate::app::gx_chat::LOCAL_MACHINE_ID {
            SessionKey::local(
                self.config.project_id.clone(),
                self.config.session_id.clone(),
            )
        } else {
            SessionKey::remote(
                self.config.machine_id.clone(),
                self.config.project_id.clone(),
                self.config.session_id.clone(),
            )
        }
    }

    /// Reads the session's links from the sidebar store, in a deferred call because the app entity
    /// may be mid-update while the chat renders (as `refresh_issue_links` does).
    pub(super) fn refresh_links_pill(&self, cx: &mut Context<Self>) {
        let Some(app) = self.config.app.clone() else {
            return;
        };
        let key = self.links_session_key();
        let chat = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(app) = app.upgrade() else {
                return;
            };
            let pill = app
                .read(cx)
                .gx_store
                .core
                .presentation()
                .session(&key)
                .and_then(|session| session.links.as_ref().map(SessionLinks::from_presentation))
                .and_then(|links| links.chat_pill());
            let _ = chat.update(cx, |chat, cx| {
                if chat.links_pill != pill {
                    chat.links_pill = pill;
                    cx.notify();
                }
            });
        });
    }

    /// The pill, in the chat pills' shape with the sidebar badge's text and colours: the link icon,
    /// one link's ID and "+N" for the others in the most pressing link's colour, its border that
    /// colour at 45%; muted and dashed ("3 suggested") while the session only has suggestions.
    pub(super) fn links_pill_element(
        &self,
        pill: &SessionLinksChatPill,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let color = if pill.suggested {
            p.card_muted
        } else {
            session_link_badge_color(pill.tone)
        };
        let bounds = self.pill_bounds.links.clone();
        let measured = bounds.clone();
        div()
            .id("chat-pill-links")
            .relative()
            .role(gpui::Role::Button)
            .aria_label(pill.aria_label.clone())
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(4.0 * s))
            .px(px(8.0 * s))
            .py(px(2.0 * s))
            .rounded_full()
            .border(px(s))
            .when(pill.suggested, |pill| pill.border_dashed())
            .border_color(color.opacity(0.45))
            .text_size(px(11.5 * s))
            .line_height(px(16.0 * s))
            .whitespace_nowrap()
            .text_color(color)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .child(
                svg()
                    .path("titlebar/link.svg")
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(color),
            )
            .child(
                div()
                    .when(!pill.suggested, |id| {
                        id.font_weight(gpui::FontWeight::MEDIUM)
                    })
                    // "#270+" with more links (link_chips.rs has the user's words).
                    .child(if pill.more > 0 {
                        format!("{}+", pill.label)
                    } else {
                        pill.label.clone()
                    }),
            )
            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                cx.stop_propagation();
                // The list opens above the chat box lined up with this pill, as the other pills'
                // menus do (`PillsStripCells::menu_anchor`; the popup's geometry keeps it above and
                // puts its left edge on the anchor's).
                let chat_box = this.pills_strip.chat_box();
                let left = super::pills_strip::strip_menu_left(
                    bounds.get(),
                    chat_box,
                    px(crate::app::native_sidebar::links_list::SESSION_LINKS_LIST_WIDTH),
                );
                let anchor = json!({
                    "x": f32::from(left),
                    // The popup's 4px gap above the box, as the other menus keep.
                    "y": f32::from(chat_box.top()) - 4.0,
                    "width": 1.0,
                    "height": 1.0,
                });
                this.host("sessionLinks", json!({ "anchor": anchor }), cx);
            }))
            // The pill's border box, measured as `chat_pill` measures the others.
            .child(
                canvas(move |painted, _, _| measured.set(painted), |_, _, _, _| {})
                    .absolute()
                    .inset(px(-s)),
            )
            .into_any_element()
    }
}
