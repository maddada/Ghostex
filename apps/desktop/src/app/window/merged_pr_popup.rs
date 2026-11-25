//! The merged-PR popup on the desktop: a child window beside the sidebar, level with the row whose
//! merge badge opened it, like the floating links card (session_links_card.rs). It takes focus so
//! a press anywhere else (the sidebar, the work area, a web page, another app) closes it, as the
//! app's other click-away popups do (popup_dismissal.rs). The web build draws the same card in its
//! page (apps/gpui-web/src/app/web_host/merged_pr_popup.rs).
//!
//! SEE-ALSO: CDXC:WorkMode 2026-11-25 in native_sidebar/work_chips.rs.

use std::rc::Rc;

use gpui::{
    AnyElement, AppContext as _, Bounds, Context, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, WeakEntity, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, point, px, size,
};

use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::native_sidebar::work_chips::{
    MERGED_PR_POPUP_GAP, MERGED_PR_POPUP_WIDTH, MergedAnswerHandler, MergedOffer, MergedPopupOpen,
    merged_pr_popup_card,
};

/// The height the window opens at before it fits its content.
const FIRST_HEIGHT: f32 = 150.0;

pub(crate) struct MergedPrPopupWindow {
    main_app: WeakEntity<GhostexGpuiApp>,
    offer: MergedOffer,
    frosted: bool,
    focus: FocusHandle,
    was_active: bool,
    _subscriptions: Vec<Subscription>,
}

impl MergedPrPopupWindow {
    fn new(
        main_app: WeakEntity<GhostexGpuiApp>,
        offer: MergedOffer,
        frosted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let activation = cx.observe_window_activation(window, |this: &mut Self, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active {
                crate::app::window::popup_dismissal::dismiss_on_focus_loss(
                    this,
                    window,
                    cx,
                    |this, window, cx| this.dismiss(window, cx),
                );
            }
        });
        let press = crate::app::window::popup_dismissal::observe_main_window_press(
            window,
            cx,
            |this: &mut Self, window, cx| this.dismiss(window, cx),
        );
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            main_app,
            offer,
            frosted,
            focus,
            was_active: false,
            _subscriptions: vec![activation, press],
        }
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let session_id = self.offer.session_id.clone();
        let _ = self.main_app.update(cx, |app, cx| {
            if app.merged_pr_popup_open_for(&session_id) {
                app.forget_merged_pr_popup(cx);
            }
        });
        window.remove_window();
    }
}

impl Render for MergedPrPopupWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main_app = self.main_app.clone();
        let offer = self.offer.clone();
        let on_answer: MergedAnswerHandler = Rc::new(move |answer, window, cx| {
            let offer = offer.clone();
            let _ = main_app.update(cx, |app, cx| app.forget_merged_pr_popup(cx));
            window.remove_window();
            let _ = main_app.update(cx, |app, cx| app.answer_merged_pr_popup(&offer, answer, cx));
        });
        // The window fits the card's measured height, so nothing of its backdrop shows around it.
        div()
            .w_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.dismiss(window, cx);
                }
            }))
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
            .child(merged_pr_popup_card(
                &self.offer,
                self.frosted,
                self.frosted,
                on_answer,
            ))
    }
}

impl GhostexGpuiApp {
    /// Opens the popup for `open`'s row, closing any other first.
    pub(crate) fn open_merged_pr_popup(&mut self, open: MergedPopupOpen, cx: &mut Context<Self>) {
        self.defer_in_main_window(cx, move |app, window, cx| {
            app.open_merged_pr_popup_in(open, window, cx);
        });
    }

    fn open_merged_pr_popup_in(
        &mut self,
        open: MergedPopupOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_merged_pr_popup_window(cx);
        let main = window.bounds();
        let sidebar = self.native_sidebar.bounds;
        let width = MERGED_PR_POPUP_WIDTH;
        // Beside the sidebar, on the work area's side, level with the row.
        let sidebar_on_right = sidebar.left().as_f32() > main.size.width.as_f32() / 2.0;
        let left = if sidebar_on_right {
            sidebar.left().as_f32() - MERGED_PR_POPUP_GAP - width
        } else {
            sidebar.right().as_f32() + MERGED_PR_POPUP_GAP
        };
        let top = open
            .top
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
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            app_id: crate::gpui_platform_window_app_id(),
            window_background: if frosted {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Transparent
            },
            ..Default::default()
        };
        let main_app = cx.weak_entity();
        let offer = open.offer.clone();
        let opened = cx.open_window(options, move |popup_window, cx| {
            popup_window.set_background_corner_radius(px(8.0));
            if frosted {
                apply_frosted_menu_blur(popup_window);
            }
            crate::app::window::titlebar_popup_chrome::prepare_gpui_titlebar_popup_window_chrome(
                popup_window,
            );
            cx.new(|cx| MergedPrPopupWindow::new(main_app, offer, frosted, popup_window, cx))
        });
        if let Ok(handle) = opened {
            self.native_sidebar.merged_popup = Some(open);
            self.native_sidebar.merged_popup_window = Some(handle.into());
            cx.notify();
        }
    }

    /// Closes the popup's window, if one is open.
    pub(crate) fn close_merged_pr_popup_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.native_sidebar.merged_popup_window.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    /// The desktop draws the popup in its own window, never in the sidebar.
    pub(crate) fn render_merged_pr_page_popup(
        &self,
        _cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        None
    }
}
