//! The page's side chat panel: the side chat a chat opened (`openSideChat`), drawn beside the chat
//! where the desktop's side panel tab shows it, with a close control. The page has no view panel
//! and shows one side chat at a time.
//!
//! SEE-ALSO: apps/desktop/src/app/side_chat_panel/ (the desktop's side chat tabs).

use ghostex_gx_core::SessionKey;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::h_flex;

use crate::GhostexGpuiApp;
use crate::app::consts::*;
use crate::app::helpers::*;

/// The side chat panel's width; the chat beside it keeps the rest.
const WEB_SIDE_CHAT_WIDTH: f32 = 520.0;

impl GhostexGpuiApp {
    /// Opens `session`'s side chat in the panel, in place of the one it showed.
    pub(crate) fn web_open_side_chat(&mut self, session: SessionKey, cx: &mut Context<Self>) {
        self.side_chat_open = Some(session);
        self.web_report_shown_sessions(cx);
        cx.notify();
    }

    /// Closes the panel when it shows `session`, or whatever it shows with `None`.
    pub(crate) fn web_close_side_chat(
        &mut self,
        session: Option<&SessionKey>,
        cx: &mut Context<Self>,
    ) {
        if session.is_none() || self.side_chat_open.as_ref() == session {
            self.side_chat_open = None;
            self.web_report_shown_sessions(cx);
            cx.notify();
        }
    }

    /// The side chat the panel shows, for the shown-sessions and chat grid reports.
    pub(crate) fn web_side_chat_shown(&self) -> Option<&SessionKey> {
        self.side_chat_open.as_ref()
    }

    /// The side chat's title as the store has it now, or "Side chat".
    fn web_side_chat_title(&self, session: &SessionKey) -> String {
        self.gx_store
            .core
            .presentation()
            .session(session)
            .and_then(|record| {
                record
                    .display_title
                    .as_deref()
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .or_else(|| Some(record.title.trim()).filter(|title| !title.is_empty()))
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Side chat".to_string())
    }

    /// The panel, while a side chat is open.
    pub(crate) fn render_web_side_chat_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.side_chat_open.clone()?;
        let view = self.ensure_native_chat(&session, cx);
        let title = self.web_side_chat_title(&session);
        let text = titlebar_active_text_color();
        Some(
            div()
                .h_full()
                .w(px(WEB_SIDE_CHAT_WIDTH))
                .flex_none()
                .flex()
                .flex_col()
                .border_l_1()
                .border_color(titlebar_popup_menu_border_color())
                .child(
                    h_flex()
                        .h(px(32.0))
                        .flex_none()
                        .px(px(8.0))
                        .gap(px(6.0))
                        .items_center()
                        .border_b_1()
                        .border_color(titlebar_popup_menu_border_color())
                        .text_size(px(12.5))
                        .text_color(text)
                        .child(titlebar_svg_icon("titlebar/message-circle.svg", 14.0, text))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(title),
                        )
                        .child(
                            div()
                                .id("web-side-chat-close")
                                .size(px(22.0))
                                .flex()
                                .flex_none()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.0))
                                .cursor_pointer()
                                .hover(|button| button.bg(titlebar_popup_menu_hover_color()))
                                .child(titlebar_svg_icon(TITLEBAR_ICON_X, 11.0, text))
                                .tooltip(|window, cx| {
                                    gpui_component::tooltip::Tooltip::new("Close side chat")
                                        .build(window, cx)
                                })
                                .on_click(
                                    cx.listener(|app, _, _, cx| app.web_close_side_chat(None, cx)),
                                ),
                        ),
                )
                .child(div().flex_1().min_h_0().overflow_hidden().child(view))
                .into_any_element(),
        )
    }
}
