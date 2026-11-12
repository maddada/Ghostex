//! A session row dragged from the sidebar onto the page's chat box types its link at the caret, as
//! the desktop's chat pane does (`session_composer_drop.rs`). The page has no panes to split, so a
//! release anywhere else on the chat does nothing.
use gpui::{AnyElement, Context, CursorStyle, DragMoveEvent, InteractiveElement as _, IntoElement};
use gpui::{ParentElement as _, Styled as _, Window, div};

use crate::GhostexGpuiApp;
use crate::app::native_sidebar::drag::SidebarDrag;
use ghostex_gx_core::SessionKey;

impl GhostexGpuiApp {
    /// The chat surface of `session`, taking session rows dropped on its chat box.
    pub(crate) fn web_chat_with_composer_drop(
        &self,
        session: &SessionKey,
        surface: AnyElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let hover = session.clone();
        let drop = session.clone();
        div()
            .size_full()
            .on_drag_move::<SidebarDrag>(cx.listener(
                move |this, event: &DragMoveEvent<SidebarDrag>, window, cx| {
                    this.web_composer_drag_move(&hover, event, window, cx);
                },
            ))
            .on_drop(cx.listener(move |this, drag: &SidebarDrag, window, cx| {
                this.web_composer_drop(&drop, drag, window, cx);
            }))
            .child(surface)
            .into_any_element()
    }

    fn web_composer_drag_move(
        &mut self,
        session: &SessionKey,
        event: &DragMoveEvent<SidebarDrag>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.drag(cx).kind != "session" {
            return;
        }
        let Some((_, chat)) = self.native_chats.get(session).cloned() else {
            return;
        };
        let position = event.event.position;
        let over = event.bounds.contains(&position) && chat.read(cx).composer_drop_target(position);
        if chat.update(cx, |chat, cx| chat.set_composer_drop_hover(over, cx)) {
            let cursor = if over {
                CursorStyle::DragCopy
            } else {
                CursorStyle::Arrow
            };
            cx.set_active_drag_cursor_style(cursor, window);
        }
    }

    fn web_composer_drop(
        &mut self,
        session: &SessionKey,
        drag: &SidebarDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((_, chat)) = self.native_chats.get(session).cloned() else {
            return;
        };
        let on_box = chat.read(cx).composer_drop_target(window.mouse_position());
        let text = on_box
            .then(|| self.sidebar_drag_session_mentions(drag))
            .flatten();
        chat.update(cx, |chat, cx| match text {
            Some(text) => chat.insert_composer_drop_text(&text, window, cx),
            None => {
                chat.set_composer_drop_hover(false, cx);
            }
        });
        cx.notify();
    }
}
