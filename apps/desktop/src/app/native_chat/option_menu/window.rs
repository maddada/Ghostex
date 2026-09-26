use super::super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, Pixels, Point, ScrollHandle,
    Styled as _, Subscription, Window, WindowBounds, WindowOptions, px, size,
};
use gpui_component::Root;
use serde_json::Value;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// How many times one menu takes key status back before a key loss closes it after all, so a
/// program that refocuses the main window on every change cannot keep the two fighting.
const MAX_REKEYS: u32 = 20;
/// How long after a panel opens its key status is checked once.
const SETTLE_KEY_AFTER: Duration = Duration::from_millis(150);

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn GhostexGpuiPointerPressedRecently() -> bool;
    fn GhostexGpuiApplicationIsActive() -> bool;
}

/// Whether the key status a menu just lost went where the user sent it: to a pointer press
/// somewhere in the app, or away with the app itself. Anything else took it without the user.
fn key_loss_is_dismissal() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { GhostexGpuiPointerPressedRecently() || !GhostexGpuiApplicationIsActive() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Starts watching for presses before the first menu can lose key status to one.
fn watch_pointer_presses() {
    #[cfg(target_os = "macos")]
    unsafe {
        GhostexGpuiPointerPressedRecently();
    }
}

pub(in crate::app::native_chat) struct ChatOptionMenu {
    pub(super) chat: gpui::WeakEntity<NativeChatView>,
    pub(super) source: gpui::AnyWindowHandle,
    pub(super) source_focus: Option<FocusHandle>,
    pub(super) source_bounds: Bounds<Pixels>,
    pub(super) appearance: ChatAppearance,
    pub(super) windows: Vec<gpui::WindowHandle<Root>>,
    pub(super) opening: bool,
    pub(super) closed: bool,
    /// Where each depth's panel was anchored, so a panel can reopen in place.
    anchors: Vec<(Bounds<Pixels>, f32, bool)>,
    parent: *mut std::ffi::c_void,
    /// Opened at the pointer (right-click menus), drawn with `MenuMetrics::CONTEXT`.
    compact: bool,
    /// How many times this menu has taken key status back (`retake_key`).
    rekeys: u32,
    /// True for a menu anchored to another surface than the chat's own pane, which today is the
    /// model pop-up over a terminal session's model pill: the chat view behind it is only its host,
    /// so the pane going off screen must not take it down.
    outside_pane: bool,
    /// The model pop-up's reasoning levels Left and Right (or the Reasoning list) moved to this visit, by row key.
    /// Kept on the menu so the Reasoning side list, a panel of its own, can set them too.
    pub(super) model_efforts: std::collections::HashMap<String, String>,
}

pub(super) struct ChatOptionMenuPanel {
    pub(super) menu: Entity<ChatOptionMenu>,
    pub(super) depth: usize,
    pub(super) rows: Arc<Vec<Value>>,
    pub(super) heights: Vec<f32>,
    pub(super) focus: FocusHandle,
    pub(super) selected: Option<usize>,
    pub(super) scroll: ScrollHandle,
    pub(super) child: Option<usize>,
    pub(super) hover_task: Option<gpui::Task<()>>,
    /// The Switch Account panel's Customize state (`SessionAccountsPanel`'s local `customize`).
    pub(super) accounts_customize: bool,
    /// Screen bounds of the panel's account preference select, where its dropdown opens.
    pub(super) select_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The model picker's cursor, search field and open side list, when this panel is that picker.
    pub(super) model_menu: Option<super::model_menu::ModelMenuState>,
    was_active: bool,
    _activation: Subscription,
    _chat_subscription: Option<Subscription>,
}

impl ChatOptionMenu {
    pub(super) fn metrics(&self) -> super::geometry::MenuMetrics {
        if self.compact {
            super::geometry::MenuMetrics::CONTEXT
        } else {
            super::geometry::MenuMetrics::REGULAR
        }
    }

    /// True until the menu's first panel is on screen.
    pub(in crate::app::native_chat) fn is_opening(&self) -> bool {
        self.opening
    }

    pub(in crate::app::native_chat) fn is_outside_pane(&self) -> bool {
        self.outside_pane
    }

    pub(in crate::app::native_chat) fn close(
        &mut self,
        command: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        self.close_with_focus(command, true, cx);
    }

    pub(in crate::app::native_chat) fn close_with_focus(
        &mut self,
        command: Option<Value>,
        restore_focus: bool,
        cx: &mut Context<Self>,
    ) {
        if self.closed {
            return;
        }
        self.closed = true;
        let windows = std::mem::take(&mut self.windows);
        let source = self.source;
        let focus = self.source_focus.clone();
        let chat = self.chat.clone();
        let identity = cx.entity_id();
        cx.defer(move |cx| {
            for handle in windows.into_iter().rev() {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            let _ = source.update(cx, |_, window, cx| {
                if restore_focus {
                    window.activate_window();
                    if let Some(focus) = focus {
                        focus.focus(window, cx);
                    }
                }
                let _ = chat.update(cx, |chat, cx| {
                    if chat
                        .option_menu
                        .as_ref()
                        .is_some_and(|menu| menu.entity_id() == identity)
                    {
                        chat.option_menu = None;
                        chat.menu_toggle.note_closed();
                    }
                    if let Some(command) = command {
                        chat.handle_action(
                            &super::super::actions::NativeChatAction { command },
                            window,
                            cx,
                        );
                    }
                    cx.notify();
                });
            });
        });
    }

    fn any_window_active(&self, cx: &mut Context<Self>) -> bool {
        self.windows.iter().any(|handle| {
            handle
                .update(cx, |_, window, _| window.is_window_active())
                .unwrap_or(false)
        })
    }

    /// CDXC:SessionChat 2026-09-22 WHY:
    /// A key loss closes the menu only when the user caused it. On one customer's machine every
    /// chat menu (model, mode, options) showed for a frame and closed, on the same code that works
    /// elsewhere: the main window took key status back the moment the menu had it. AppKit does that
    /// on its own when the app's activation is still completing as the menu opens (a press on an
    /// inactive Ghostex both activates it and opens the menu), and window-focus utilities do it on
    /// any focus change. Neither is a press, so unless a mouse button is down, a press was seen
    /// within the last moments, or the app itself is no longer active, the menu takes key status
    /// back instead of closing, up to `MAX_REKEYS` times.
    fn check_active(&mut self, cx: &mut Context<Self>) {
        if self.opening || self.closed || self.any_window_active(cx) {
            return;
        }
        if self.retake_key(cx) {
            return;
        }
        // The press that took the window away is the one a trigger is about to report as a
        // click, so the trigger it belonged to is remembered before the menu goes (menu_toggle.rs).
        let _ = self
            .chat
            .update(cx, |chat, _| chat.menu_toggle.note_dismissed());
        self.close_with_focus(None, false, cx);
    }

    /// Makes the deepest panel key again when nothing the user did took key status away.
    fn retake_key(&mut self, cx: &mut Context<Self>) -> bool {
        if self.rekeys >= MAX_REKEYS || key_loss_is_dismissal() {
            return false;
        }
        let Some(handle) = self.windows.last().copied() else {
            return false;
        };
        self.rekeys += 1;
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        true
    }

    /// A panel that opened while the app was still being activated never became key, and so was
    /// never told it lost it: checked once after it opened, it takes key status the same way.
    fn settle_key(&mut self, cx: &mut Context<Self>) {
        if self.opening || self.closed || self.any_window_active(cx) {
            return;
        }
        self.retake_key(cx);
    }

    /// Runs a chat command without closing the menu (Switch Account panel controls).
    pub(super) fn dispatch(&mut self, command: Value, cx: &mut Context<Self>) {
        let chat = self.chat.clone();
        cx.defer(move |cx| {
            let _ = chat.update(cx, |chat, cx| chat.invoke(command, cx));
        });
    }

    pub(super) fn truncate(&mut self, depth: usize, focus_parent: bool, cx: &mut Context<Self>) {
        if self.windows.len() <= depth {
            return;
        }
        let windows = self.windows.split_off(depth);
        let parent = self.windows.last().copied();
        cx.defer(move |cx| {
            for handle in windows.into_iter().rev() {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            if focus_parent && let Some(parent) = parent {
                let _ = parent.update(cx, |_, window, _| window.activate_window());
            }
        });
    }

    pub(super) fn open_panel(
        &mut self,
        rows: Vec<Value>,
        anchor: Bounds<Pixels>,
        width: f32,
        depth: usize,
        cx: &mut Context<Self>,
    ) {
        self.open_panel_at(rows, anchor, width, depth, false, cx);
    }

    /// A select's list: below its trigger (above when there is no room), left edges aligned.
    pub(super) fn open_dropdown(
        &mut self,
        rows: Vec<Value>,
        anchor: Bounds<Pixels>,
        width: f32,
        depth: usize,
        cx: &mut Context<Self>,
    ) {
        self.open_panel_at(rows, anchor, width, depth, true, cx);
    }

    /// Reopens the panel at `depth` with new rows, re-placed against its original anchor.
    /// The Switch Account panel uses this when it grows past the room below its top edge.
    pub(super) fn reopen(&mut self, depth: usize, rows: Vec<Value>, cx: &mut Context<Self>) {
        if let Some(&(anchor, width, below)) = self.anchors.get(depth) {
            self.open_panel_at(rows, anchor, width, depth, below, cx);
        }
    }

    fn open_panel_at(
        &mut self,
        mut rows: Vec<Value>,
        anchor: Bounds<Pixels>,
        width: f32,
        depth: usize,
        below: bool,
        cx: &mut Context<Self>,
    ) {
        if self.closed || rows.is_empty() {
            return;
        }
        self.anchors.truncate(depth);
        self.anchors.push((anchor, width, below));
        for row in &mut rows {
            if let Some(action) = row["hotkeyAction"].as_str() {
                row["detail"] = crate::app::hotkeys::gpui_configured_hotkey_label(action).into();
            }
        }
        self.truncate(depth, false, cx);
        let scale = self.appearance.scale;
        let available = self.source_bounds;
        let metrics = self.metrics();
        let width = if self.compact {
            super::geometry::fit_width(&rows, width, &self.appearance, cx)
        } else {
            width
        };
        let width = px(width * scale).min(available.size.width - px(24.0 * scale));
        let heights = match super::geometry::measure_rows(
            &rows,
            f32::from(width) / scale,
            metrics,
            &self.appearance,
            cx,
        ) {
            Ok(heights) => heights,
            Err(error) => {
                let _ = self.chat.update(cx, |chat, cx| {
                    chat.error = Some(error.to_string());
                    cx.notify();
                });
                return;
            }
        };
        let height = px((metrics.chrome()
            + heights.iter().sum::<f32>()
            + metrics.gap * (rows.len().saturating_sub(1)) as f32)
            * scale)
        .min(available.size.height - px(24.0 * scale));
        let margin = px(12.0 * scale);
        let x = if below {
            anchor.left()
        } else if depth == 0 {
            anchor.right() - width
        } else if anchor.right() + px(4.0 * scale) + width < available.right() - margin {
            anchor.right() + px(4.0 * scale)
        } else {
            anchor.left() - width - px(4.0 * scale)
        };
        let y = if depth == 0 || below {
            if anchor.bottom() + px(4.0 * scale) + height < available.bottom() - margin {
                anchor.bottom() + px(4.0 * scale)
            } else {
                anchor.top() - height - px(4.0 * scale)
            }
        } else {
            anchor.top()
        };
        let bounds = Bounds::new(
            Point::new(
                x.max(available.left() + margin)
                    .min(available.right() - width - margin),
                y.max(available.top() + margin)
                    .min(available.bottom() - height - margin),
            ),
            size(width, height),
        );
        let menu = cx.entity();
        let parent = self.parent;
        #[cfg(target_os = "linux")]
        let owner = self.source;
        let accounts_customize = rows.first().is_some_and(|row| row["customize"] == true);
        self.opening = true;
        // Under window glass the menu's window blurs what is behind it, rounded to the card.
        let glass = crate::app::helpers::window_glass_active();
        let corner_radius = px(
            if rows.first().is_some_and(|row| row["modelMenu"].is_object()) {
                super::model_menu::CARD_RADIUS
            } else {
                metrics.radius
            } * scale,
        );
        let display_id = crate::app::window::popup_frame::display_at(bounds.center(), cx);
        cx.defer(move |cx| {
            let result = cx.open_window(
                WindowOptions {
                    kind: crate::app::window::popup_frame::child_window_kind(),
                    #[cfg(target_os = "linux")]
                    x11_parent: Some(owner),
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    titlebar: None,
                    focus: true,
                    show: true,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    window_background: if glass {
                        gpui::WindowBackgroundAppearance::Blurred
                    } else {
                        gpui::WindowBackgroundAppearance::Transparent
                    },
                    ..Default::default()
                },
                {
                    let menu = menu.clone();
                    move |window, cx| {
                        window.set_background_corner_radius(corner_radius);
                        crate::app::helpers::apply_frosted_menu_blur(window);
                        crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
                        crate::app::window::attach_gpui_app_modal_window_to_main_window(
                            window, parent,
                        );
                        let panel = cx.new(|cx| {
                            let focus = cx.focus_handle();
                            focus.focus(window, cx);
                            let activation = cx.observe_window_activation(
                                window,
                                |panel: &mut ChatOptionMenuPanel, window, cx| {
                                    if window.is_window_active() {
                                        panel.was_active = true;
                                    } else if panel.was_active {
                                        let menu = panel.menu.clone();
                                        cx.defer(move |cx| {
                                            menu.update(cx, |menu, cx| menu.check_active(cx))
                                        });
                                    }
                                },
                            );
                            let chat_subscription = if rows
                                .first()
                                .is_some_and(|row| row["accounts"].is_object())
                            {
                                menu.read(cx).chat.upgrade().map(|chat| {
                                    cx.observe_in(
                                        &chat,
                                        window,
                                        |panel: &mut ChatOptionMenuPanel, chat, window, cx| {
                                            let next = &chat.read(cx).snapshot["accountPanel"];
                                            if !next.is_object()
                                                || panel
                                                    .rows
                                                    .first()
                                                    .is_some_and(|row| &row["accounts"] == next)
                                            {
                                                return;
                                            }
                                            panel.rows = Arc::new(vec![
                                                serde_json::json!({"accounts":next.clone()}),
                                            ]);
                                            panel.refit_accounts(window, cx);
                                        },
                                    )
                                })
                            } else if rows.first().is_some_and(|row| row["context"].is_object()) {
                                menu.read(cx).chat.upgrade().map(|chat| {
                                    cx.observe_in(
                                        &chat,
                                        window,
                                        |panel: &mut ChatOptionMenuPanel, chat, window, cx| {
                                            let next = &chat.read(cx).snapshot["contextMeter"];
                                            if panel
                                                .rows
                                                .first()
                                                .is_some_and(|row| &row["context"] == next)
                                            {
                                                return;
                                            }
                                            let next = next.clone();
                                            panel.rows =
                                                Arc::new(vec![serde_json::json!({"context":next})]);
                                            let appearance = &panel.menu.read(cx).appearance;
                                            let height = match super::context::height(
                                                &next,
                                                window.bounds().size.width.as_f32()
                                                    / appearance.scale,
                                                appearance,
                                                cx,
                                            ) {
                                                Ok(height) => height,
                                                Err(error) => {
                                                    let chat = panel.menu.read(cx).chat.clone();
                                                    cx.defer(move |cx| {
                                                        let _ = chat.update(cx, |chat, cx| {
                                                            chat.error = Some(error.to_string());
                                                            cx.notify();
                                                        });
                                                    });
                                                    return;
                                                }
                                            };
                                            panel.heights = vec![height];
                                            let available = panel.menu.read(cx).source_bounds;
                                            let desired = px((height + 14.0) * appearance.scale);
                                            window.resize(size(
                                                window.bounds().size.width,
                                                desired.min(
                                                    available.bottom()
                                                        - window.bounds().top()
                                                        - px(12.0 * appearance.scale),
                                                ),
                                            ));
                                            cx.notify();
                                        },
                                    )
                                })
                            } else if rows.first().is_some_and(|row| row["modelMenu"].is_object()) {
                                menu.read(cx).chat.upgrade().map(|chat| {
                                    cx.observe(&chat, |panel: &mut ChatOptionMenuPanel, _, cx| {
                                        panel.model_menu_changed(cx)
                                    })
                                })
                            } else {
                                None
                            };
                            let model_menu =
                                super::model_menu::ModelMenuState::new(&rows, &menu, window, cx);
                            ChatOptionMenuPanel {
                                menu,
                                depth,
                                rows: Arc::new(rows),
                                heights,
                                focus,
                                selected: None,
                                scroll: Default::default(),
                                child: None,
                                hover_task: None,
                                accounts_customize,
                                select_bounds: Rc::new(Cell::new(Bounds::default())),
                                model_menu,
                                was_active: window.is_window_active(),
                                _activation: activation,
                                _chat_subscription: chat_subscription,
                            }
                        });
                        cx.new(|cx| Root::new(panel, window, cx).bg(gpui::transparent_black()))
                    }
                },
            );
            let settle = menu.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(SETTLE_KEY_AFTER).await;
                let _ = cx.update(|cx| settle.update(cx, |menu, cx| menu.settle_key(cx)));
            })
            .detach();
            menu.update(cx, |menu, cx| {
                menu.opening = false;
                match result {
                    Ok(handle) if !menu.closed => menu.windows.push(handle),
                    Ok(handle) => {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                    Err(error) => {
                        let _ = menu.chat.update(cx, |chat, cx| {
                            chat.error = Some(error.to_string());
                            cx.notify();
                        });
                        menu.close(None, cx);
                    }
                }
            });
        });
    }
}

impl NativeChatView {
    pub(crate) fn active_option_menu_source(&self, cx: &gpui::App) -> Option<gpui::WindowId> {
        let active = cx.active_window()?.window_id();
        let menu = self.option_menu.as_ref()?.read(cx);
        (!menu.closed
            && menu
                .windows
                .iter()
                .any(|handle| handle.window_id() == active))
        .then_some(menu.source.window_id())
    }

    pub(in crate::app::native_chat) fn show_option_menu(
        &mut self,
        kind: &str,
        trigger: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rows) = self.snapshot["optionMenus"][kind]
            .as_array()
            .filter(|rows| !rows.is_empty())
            .cloned()
        else {
            return;
        };
        if self.chat_menu_toggled_shut(super::super::menu_toggle::option_pill_trigger_id(kind), cx)
        {
            return;
        }
        self.show_chat_menu(
            rows,
            trigger,
            if kind == "model" { 256.0 } else { 240.0 },
            window,
            cx,
        );
    }

    /// A menu opened at the pointer instead of under a trigger.
    ///
    /// React's context menus drop down and to the right of the press, which is
    /// the placement a select's list uses, not the right-aligned placement a
    /// toolbar button's menu uses.
    pub(in crate::app::native_chat) fn show_chat_menu_at(
        &mut self,
        rows: Vec<Value>,
        at: Point<Pixels>,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_chat_menu(
            rows,
            Bounds::new(at, size(px(0.0), px(0.0))),
            width,
            true,
            window,
            cx,
        );
    }

    pub(in crate::app::native_chat) fn show_chat_menu(
        &mut self,
        rows: Vec<Value>,
        trigger: Bounds<Pixels>,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_chat_menu(rows, trigger, width, false, window, cx);
    }

    fn open_chat_menu(
        &mut self,
        rows: Vec<Value>,
        trigger: Bounds<Pixels>,
        width: f32,
        below: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if rows.is_empty() {
            return;
        }
        watch_pointer_presses();
        if let Some(menu) = self.option_menu.take() {
            menu.update(cx, |menu, cx| menu.close_with_focus(None, false, cx));
        }
        let appearance = ChatAppearance::current(&self.snapshot);
        // `trigger` is in content coordinates; a window frame with a titlebar (Chat Lab) put every
        // menu that titlebar's height above its trigger or the pointer.
        let content_bounds = super::super::child_window::content_bounds(window);
        // Menus are kept inside the chat itself. On the desktop it fills its own window, so this is
        // that window's content; in a page it is one pane beside the sidebar, and clamping to the
        // page let a menu opened from the model pill cover the sidebar.
        let view = self.bounds.get();
        let source_bounds = if view.size.width > px(0.0) && view.size.height > px(0.0) {
            Bounds::new(content_bounds.origin + view.origin, view.size)
        } else {
            content_bounds
        };
        let chat = cx.weak_entity();
        let source = window.window_handle();
        let source_focus = window.focused(cx);
        let parent = self.config.parent_native_view;
        let outside_pane = std::mem::take(&mut self.menu_outside_pane);
        let menu = cx.new(|_| ChatOptionMenu {
            chat,
            source,
            source_focus,
            source_bounds,
            appearance,
            windows: vec![],
            opening: false,
            closed: false,
            anchors: vec![],
            parent,
            compact: below,
            rekeys: 0,
            outside_pane,
            model_efforts: Default::default(),
        });
        let anchor = Bounds::new(content_bounds.origin + trigger.origin, trigger.size);
        menu.update(cx, |menu, cx| {
            if below {
                menu.open_dropdown(rows, anchor, width, 0, cx);
            } else {
                menu.open_panel(rows, anchor, width, 0, cx);
            }
        });
        self.option_menu = Some(menu);
        self.menu_toggle.note_opened();
        cx.notify();
    }
}
