//! The Main Hub card on the desktop: a non-activating child window beside the sidebar, level with
//! the Main Hub row the pointer rests on, like the floating links card (session_links_card.rs).
//! It only explains, so it takes no input: it opens after the tooltip delay and closes when the
//! pointer leaves the row (native_sidebar/chief.rs `hover_main_hub_row`). The web build draws the
//! same card in its page (apps/gpui-web/src/app/web_host/main_hub_card.rs).
//!
//! SEE-ALSO: the Q1991 DECISION on `main_hub_tooltip_content` in
//! apps/desktop/src/app/native_sidebar/chief_row_view.rs.

use gpui::{
    AnyElement, AppContext as _, Bounds, Context, IntoElement, ParentElement as _, Render,
    Styled as _, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div,
    point, px, size,
};

use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::native_sidebar::chief_row_view::MAIN_HUB_TOOLTIP_WIDTH;

/// The gap between the sidebar's edge and the card.
const CARD_GAP: f32 = 6.0;
/// The card's padding around its text column and its border (tooltips.rs `main_hub_card`).
const CARD_CHROME_X: f32 = 2.0 * 12.0 + 2.0;
/// The height the window opens at before it fits its content.
const FIRST_HEIGHT: f32 = 190.0;

pub(crate) struct MainHubCardWindow {
    frosted: bool,
}

impl Render for MainHubCardWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // The window fits the card's measured height, so nothing of its frosted backdrop shows
        // around it.
        div()
            .w_full()
            .on_children_prepainted(|bounds, window, _| {
                let Some(card) = bounds.first() else {
                    return;
                };
                let height = card.size.height.as_f32().round();
                let current = window.viewport_size();
                if (current.height.as_f32() - height).abs() >= 1.0 {
                    window.resize(size(current.width, px(height)));
                }
            })
            .child(crate::app::native_sidebar::tooltips::main_hub_card(
                1.0,
                self.frosted,
                self.frosted,
            ))
    }
}

impl GhostexGpuiApp {
    /// Opens the card for the Main Hub row `key` whose top is `top` in the main window.
    pub(crate) fn open_main_hub_card(&mut self, key: String, top: f32, cx: &mut Context<Self>) {
        self.defer_in_main_window(cx, move |app, window, cx| {
            app.open_main_hub_card_in(key, top, window, cx);
        });
    }

    fn open_main_hub_card_in(
        &mut self,
        key: String,
        top: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_main_hub_card(cx);
        let main = window.bounds();
        let sidebar = self.native_sidebar.bounds;
        let width = MAIN_HUB_TOOLTIP_WIDTH + CARD_CHROME_X;
        // Beside the sidebar, on the work area's side, level with the row.
        let sidebar_on_right = sidebar.left().as_f32() > main.size.width.as_f32() / 2.0;
        let left = if sidebar_on_right {
            sidebar.left().as_f32() - CARD_GAP - width
        } else {
            sidebar.right().as_f32() + CARD_GAP
        };
        let top = top
            .min(main.size.height.as_f32() - 8.0 - FIRST_HEIGHT)
            .max(8.0);
        let frame = Bounds {
            origin: point(main.origin.x + px(left), main.origin.y + px(top)),
            size: size(px(width), px(FIRST_HEIGHT)),
        };
        let display_id =
            crate::app::window::popup_frame::PopupOwner::of(window, cx).display_for(frame, cx);
        let frosted = window_glass_active();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(frame)),
            display_id,
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            window_background: if frosted {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Transparent
            },
            ..Default::default()
        };
        let opened = cx.open_window(options, move |card_window, cx| {
            card_window.set_background_corner_radius(px(8.0));
            if frosted {
                apply_frosted_menu_blur(card_window);
            }
            crate::app::window::titlebar_popup_chrome::prepare_gpui_titlebar_popup_window_chrome(
                card_window,
            );
            cx.new(|_| MainHubCardWindow { frosted })
        });
        if let Ok(handle) = opened {
            self.native_sidebar.main_hub_card_open = Some((key, top));
            self.native_sidebar.main_hub_card_window = Some(handle.into());
        }
    }

    /// Closes the card.
    pub(crate) fn close_main_hub_card(&mut self, cx: &mut Context<Self>) {
        self.native_sidebar.main_hub_card_open = None;
        if let Some(handle) = self.native_sidebar.main_hub_card_window.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    /// The desktop draws the card in its own window, never in the sidebar.
    pub(crate) fn render_main_hub_page_card(&self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        None
    }
}
