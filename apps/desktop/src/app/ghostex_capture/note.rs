//! The short note under the button after a prompt was sent.
//!
//! CDXC:GhostexCapture 2026-09-30 DECISION:
//! User: after sending, a note under the button says where the prompt went, with Open, and fades
//! after a few seconds; the user stays in the app they were in.

use std::time::Duration;

use ghostex_gx_core::SessionKey;
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, SharedString, WeakEntity, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, point, px, rgb, size,
};

use super::placement;
use super::platform;
use crate::GhostexGpuiApp;

const NOTE_WIDTH: f32 = 330.0;
const NOTE_HEIGHT: f32 = 44.0;
const NOTE_HOLD: Duration = Duration::from_secs(5);

pub(crate) struct NoteView {
    app: WeakEntity<GhostexGpuiApp>,
    text: SharedString,
    session: SessionKey,
}

impl GhostexGpuiApp {
    pub(super) fn show_ghostex_capture_note(
        &mut self,
        text: String,
        session: SessionKey,
        cx: &mut Context<Self>,
    ) {
        let Some(icon) = self.ghostex_capture.icon.as_ref() else {
            return;
        };
        let Some(screen) = placement::screen_at(icon.frame.center(), cx) else {
            return;
        };
        let panel = placement::panel_frame(&screen, icon.frame, NOTE_HEIGHT);
        let x = if icon.frame.center().x > screen.visible.center().x {
            icon.frame.right() - px(NOTE_WIDTH)
        } else {
            icon.frame.left()
        };
        let frame = Bounds::new(
            point(x.max(screen.visible.left()), panel.origin.y),
            size(px(NOTE_WIDTH), px(NOTE_HEIGHT)),
        );
        let app = cx.weak_entity();
        App::defer(cx, move |cx| {
            let owner = app.clone();
            let (bounds, display_id) = crate::app::window::popup_frame::place_global(frame, cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    titlebar: None,
                    focus: false,
                    show: true,
                    kind: WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                move |window, cx| {
                    crate::app::window::popup_frame::round_popup_window(window, px(10.0), false);
                    cx.new(|_| NoteView {
                        app: owner,
                        text: text.into(),
                        session,
                    })
                },
            );
            let Ok(handle) = result else {
                return;
            };
            if let Some(native) = handle
                .update(cx, |_, window, _| {
                    crate::app::helpers::cef_parent_native_view(window)
                        .ok()
                        .map(|view| view as usize)
                })
                .ok()
                .flatten()
            {
                platform::prepare_floating_window(native, false);
            }
            cx.spawn(async move |cx| {
                cx.background_executor().timer(NOTE_HOLD).await;
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            })
            .detach();
        });
    }
}

impl Render for NoteView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // CDXC:Theming 2026-11-20: the theme's lifted colour, opaque, in dark mode under glass and
        // in light mode (`SolidPalette`); dark with glass off keeps its solid colours.
        // The note shares the picker's step (#1b1b1b with glass off).
        let c = super::glass::SolidPalette::resolve();
        let fill = c.menu;
        div()
            .id("ghostex-capture-sent-note")
            .size_full()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .rounded(px(10.0))
            .bg(fill)
            .border_1()
            .border_color(c.edge)
            .child(div().size(px(8.0)).rounded(px(1.0)).bg(rgb(0xc68a06)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(12.5))
                    .text_color(c.strong)
                    .child(self.text.clone()),
            )
            .child(
                div()
                    .id("ghostex-capture-sent-open")
                    .text_size(px(12.5))
                    .text_color(c.link)
                    .cursor_pointer()
                    .child("Open")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let session = this.session.clone();
                        if let Some(app) = this.app.upgrade() {
                            app.update(cx, |app, cx| {
                                app.open_ghostex_capture_session(&session, cx)
                            });
                        }
                        window.remove_window();
                    })),
            )
    }
}
