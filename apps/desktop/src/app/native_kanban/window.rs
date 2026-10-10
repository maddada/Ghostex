//! The board in a window of its own. Open in New Window moves the board out of the view panel
//! into a plain top-level window drawn on the same board state, the panel shows a card that
//! brings it back, and closing the window brings it back too.
//!
//! CDXC:ProjectBoard 2026-10-10 WHY:
//! Kanban is drawn natively and its web page is gone (CDXC:ProjectBoard 2026-09-30 in render.rs),
//! so the pop-out that hands a view's page to the browser had nothing to hand out and stayed
//! disabled. The board's state lives on the app (the Beads bridge, conversation routing and
//! toasts are app methods), so a second window draws `render_native_kanban` against that state
//! instead of building a second board: the window is light (no sidebar, sessions column or
//! gxserver socket of its own), it can go to another screen, and the board keeps drag and drop,
//! Start work and the ticket panel. Nothing is persisted: after a launch the board is in the
//! panel again. A whole workspace window opened on the Kanban view was tried first (branch
//! feat/kanban-board-window-and-scroll) and set aside for this lighter shape.

use gpui::{
    AnyElement, AnyWindowHandle, AppContext as _, Bounds, Context, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, TitlebarOptions, WeakEntity,
    Window, WindowBounds, WindowOptions, div, px, rgb, size,
};
use gpui_component::Root;

use crate::GhostexGpuiApp;
use crate::app::helpers::{
    CHROME_LIGHT_APPEARANCE, chrome_ink, glass_clear, set_detached_board_glass_window,
    sync_detached_board_window_glass, titlebar_svg_icon, window_glass_background_appearance,
    workspace_background_color, workspace_column_background,
};
use crate::app::model::TitlebarMode;
use crate::app::render::sleeping_card::{view_card_button, view_card_frame};

/// The board's window never shrinks past this; the lanes scroll sideways inside it.
const DETACHED_BOARD_MIN_WIDTH: f32 = 640.0;
const DETACHED_BOARD_MIN_HEIGHT: f32 = 440.0;

/// The board's own window while it is out of the panel.
pub(crate) struct KanbanDetachedWindow {
    pub(crate) window: AnyWindowHandle,
}

/// The window's root: it draws the app's board and redraws whenever the app does.
pub(crate) struct KanbanWindowRoot {
    app: WeakEntity<GhostexGpuiApp>,
    _subscription: Subscription,
}

impl Render for KanbanWindowRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        sync_detached_board_window_glass(window);
        app.update(cx, |app, cx| {
            app.render_native_kanban_window_body(window, cx)
        })
    }
}

/// Where the board's window opens: over the workspace window and a little smaller, so it reads as
/// the board stepping out of it; from there the user drags it to the other screen.
fn detached_board_window_bounds(main: Bounds<Pixels>) -> Bounds<Pixels> {
    let width = (f32::from(main.size.width) * 0.86).max(DETACHED_BOARD_MIN_WIDTH);
    let height = (f32::from(main.size.height) * 0.86).max(DETACHED_BOARD_MIN_HEIGHT);
    Bounds::centered_at(main.center(), size(px(width), px(height)))
}

impl GhostexGpuiApp {
    pub(crate) fn native_kanban_detached(&self) -> bool {
        self.native_kanban.detached.is_some()
    }

    /// Whether `window` is the board's own window rather than a workspace window.
    pub(crate) fn native_kanban_in_own_window(&self, window: &Window) -> bool {
        self.native_kanban
            .detached
            .as_ref()
            .is_some_and(|detached| {
                detached.window.window_id() == window.window_handle().window_id()
            })
    }

    /// The control is offered while this context has a board to show.
    pub(crate) fn native_kanban_can_open_window(&self) -> bool {
        self.titlebar_mode_available(TitlebarMode::Kanban) && self.native_kanban_project().is_some()
    }

    fn native_kanban_window_title(&self) -> String {
        let name = self
            .native_kanban_project()
            .map(|project| project.display_name)
            .unwrap_or_default();
        if name.trim().is_empty() {
            "Kanban".to_string()
        } else {
            format!("Kanban – {name}")
        }
    }

    /// Open in New Window: moves the board to a window of its own, or brings that window forward
    /// when it is already open.
    pub(crate) fn native_kanban_open_window(&mut self, cx: &mut Context<Self>) {
        if let Some(detached) = &self.native_kanban.detached {
            let handle = detached.window;
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            });
            return;
        }
        if !self.native_kanban_can_open_window() {
            return;
        }
        // The board draws in its window whether or not the panel's tab is the active one.
        self.mark_project_editor_mode_awake(TitlebarMode::Kanban, cx);
        let title = self.native_kanban_window_title();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(detached_board_window_bounds(
                self.main_window_bounds,
            ))),
            display_id: self.main_window_display_id,
            titlebar: Some(TitlebarOptions {
                title: Some(title.clone().into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            // The board takes part in window glass as it does in the panel (window_glass.rs).
            window_background: window_glass_background_appearance(),
            window_min_size: Some(size(
                px(DETACHED_BOARD_MIN_WIDTH),
                px(DETACHED_BOARD_MIN_HEIGHT),
            )),
            app_id: crate::gpui_platform_window_app_id(),
            icon: crate::gpui_platform_window_icon(),
            focus: true,
            show: true,
            ..Default::default()
        };
        let app = cx.entity();
        // Deferred: the click arrives inside a window update, where opening a window is refused.
        cx.defer(move |cx| {
            let observed = app.clone();
            let on_close = app.downgrade();
            let opened = cx.open_window(options, move |window, cx| {
                window.set_window_title(&title);
                let root = cx.new(|cx| KanbanWindowRoot {
                    app: observed.downgrade(),
                    _subscription: cx.observe(&observed, |_, _, cx| cx.notify()),
                });
                // The window's own close control puts the board back in the panel.
                window.on_window_should_close(cx, move |_, cx| {
                    let _ = on_close.update(cx, |app, cx| app.native_kanban_window_closed(cx));
                    true
                });
                cx.new(|cx| Root::new(root, window, cx).bg(gpui::transparent_black()))
            });
            let Ok(handle) = opened else {
                return;
            };
            let handle: AnyWindowHandle = handle.into();
            set_detached_board_glass_window(handle.window_id(), true);
            app.update(cx, |app, cx| {
                app.native_kanban.detached = Some(KanbanDetachedWindow { window: handle });
                app.native_kanban_release_window_bound_inputs();
                // The board is the window's only surface, so it gets the keyboard.
                if let Some(focus) = app.native_kanban.focus.clone() {
                    let _ = handle.update(cx, |_, window, cx| window.focus(&focus, cx));
                }
                app.native_kanban_notify(cx);
            });
        });
    }

    /// Bring Back, from the panel's card or the strip's control: the window closes and the panel
    /// draws the board again.
    pub(crate) fn native_kanban_bring_back(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.native_kanban_take_detached_window() else {
            return;
        };
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
        self.native_kanban_notify(cx);
    }

    /// Forgets the board's window and returns it for the caller to close (a workspace window
    /// closing takes the board's window with it, app/workspace_windows/close.rs).
    pub(crate) fn native_kanban_take_detached_window(&mut self) -> Option<AnyWindowHandle> {
        let detached = self.native_kanban.detached.take()?;
        set_detached_board_glass_window(detached.window.window_id(), false);
        self.native_kanban_release_window_bound_inputs();
        Some(detached.window)
    }

    /// The search box and the side panel's fields subscribe to focus in the window they were made
    /// in (gpui-component's `InputState`), so a move between windows lets them go: the next render
    /// makes the search box again in the new window with the same text, and the panel closes.
    fn native_kanban_release_window_bound_inputs(&mut self) {
        self.native_kanban.search_subscription = None;
        self.native_kanban.search = None;
        self.native_kanban.panel = None;
    }

    /// The user closed the board's window: the board is back in the panel.
    fn native_kanban_window_closed(&mut self, cx: &mut Context<Self>) {
        let _ = self.native_kanban_take_detached_window();
        self.native_kanban_notify(cx);
    }

    /// The window's title follows the project the board shows.
    pub(crate) fn native_kanban_sync_window_title(&mut self, cx: &mut Context<Self>) {
        let Some(detached) = &self.native_kanban.detached else {
            return;
        };
        let handle = detached.window;
        let title = self.native_kanban_window_title();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.set_window_title(&title));
        });
    }

    /// What the board's window draws: the board, or a note while the window's project has none
    /// (the window stays open across a project switch and follows the active project).
    fn render_native_kanban_window_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = self.render_native_kanban(window, cx).unwrap_or_else(|| {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(chrome_ink().opacity(0.6))
                .child("This project has no board.")
                .into_any_element()
        });
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            // The workspace column's tint over the glass, the board's own background in the panel.
            .bg(workspace_column_background())
            .child(body)
            .into_any_element()
    }

    /// The panel's card while the board is in its own window, in the sleeping card's style.
    pub(crate) fn render_native_kanban_detached_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let ink = chrome_ink();
        div()
            .id("native-kanban-detached")
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(16.0))
            .bg(glass_clear(
                if CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
                    rgb(0xffffff).into()
                } else {
                    workspace_background_color()
                },
            ))
            .child(
                view_card_frame()
                    .text_center()
                    .child(titlebar_svg_icon(
                        TitlebarMode::Kanban.tab_icon(),
                        34.0,
                        ink.opacity(0.8).into(),
                    ))
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(15.0))
                            .line_height(px(21.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(ink.opacity(0.9))
                            .child("Kanban is open in its own window"),
                    )
                    .child(
                        div()
                            .mt(px(6.0))
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(ink.opacity(0.55))
                            .child("Closing that window brings the board back here."),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .flex()
                            .gap(px(8.0))
                            .child(
                                view_card_button("Show Window")
                                    .id("native-kanban-detached-show")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.native_kanban_open_window(cx);
                                    })),
                            )
                            .child(
                                view_card_button("Bring Back")
                                    .id("native-kanban-detached-back")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.native_kanban_bring_back(cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}
