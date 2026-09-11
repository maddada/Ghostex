use gpui::{AnyWindowHandle, SharedString};
use gpui_component::{
    Root, Side,
    menu::{PopupMenu, PopupMenuItem},
};

use crate::app::helpers::*;
use crate::app::titlebar::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;
use crate::*;

struct ContextMenuRow {
    label: SharedString,
    checked: bool,
    disabled: bool,
    action: Box<dyn gpui::Action>,
}

/// CDXC:ContextMenus 2026-09-11 DECISION:
/// User: convert titlebar, Agents-tab, command-tab, and other shell context menus to the shared GPUI popup so they stay above CEF on Linux, using the same menu across desktop platforms.
/// Size each menu to its labels so short menus such as Copy/Paste stay compact.
/// Menu contents and typed actions stay with their callers; the existing titlebar popup window owns layout, input, and dismissal.
#[derive(Default)]
pub(crate) struct GpuiContextMenu {
    rows: Vec<Option<ContextMenuRow>>,
    source_window: Option<AnyWindowHandle>,
    source_focus: Option<FocusHandle>,
}

impl GpuiContextMenu {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn menu(
        self,
        label: impl Into<SharedString>,
        action: Box<dyn gpui::Action>,
    ) -> Self {
        self.menu_with_disabled(label, false, action)
    }

    pub(crate) fn menu_with_disabled(
        mut self,
        label: impl Into<SharedString>,
        disabled: bool,
        action: Box<dyn gpui::Action>,
    ) -> Self {
        self.rows.push(Some(ContextMenuRow {
            label: label.into(),
            checked: false,
            disabled,
            action,
        }));
        self
    }

    pub(crate) fn menu_with_check(
        mut self,
        label: impl Into<SharedString>,
        checked: bool,
        action: Box<dyn gpui::Action>,
    ) -> Self {
        self.rows.push(Some(ContextMenuRow {
            label: label.into(),
            checked,
            disabled: false,
            action,
        }));
        self
    }

    pub(crate) fn separator(mut self) -> Self {
        if self.rows.last().is_some_and(Option::is_some) {
            self.rows.push(None);
        }
        self
    }

    pub(crate) fn show(mut self, position: Point<Pixels>, window: &mut Window, cx: &mut App) {
        while self.rows.last().is_some_and(Option::is_none) {
            self.rows.pop();
        }
        if self.rows.is_empty() {
            return;
        }
        let Some(root) = window.root::<Root>().flatten() else {
            return;
        };
        let Ok(app) = root.read(cx).view().clone().downcast::<GhostexGpuiApp>() else {
            return;
        };
        let source_window = Window::window_handle(window);
        self.source_window = Some(source_window);
        self.source_focus = window.focused(cx);
        // Defer beyond the caller's entity borrow, including terminal-body callers.
        cx.defer(move |cx| {
            let _ = source_window.update(cx, |_, window, cx| {
                app.update(cx, |app, cx| {
                    app.close_gpui_titlebar_popup(None, window, cx);
                    app.context_menu = Some(self);
                    app.set_gpui_titlebar_popup_open(
                        GpuiTitlebarPopupKind::ContextMenu,
                        true,
                        Some(Bounds {
                            origin: position,
                            size: size(px(1.0), px(1.0)),
                        }),
                        window,
                        cx,
                    );
                })
            });
        });
    }

    pub(crate) fn content_width(&self, window: &Window) -> f32 {
        let style = window.text_style();
        let label_width = self
            .rows
            .iter()
            .flatten()
            .map(|row| {
                let line = window.text_system().shape_line(
                    row.label.clone(),
                    px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE),
                    &[style.to_run(row.label.len())],
                    None,
                );
                // PopupMenu adds 8px row insets, 4px outer padding, and a 1px border.
                let check_width = if row.checked { 28.0 } else { 0.0 };
                line.width.as_f32() + 26.0 + check_width
            })
            .fold(0.0_f32, f32::max);
        label_width
            .ceil()
            .clamp(96.0, 400.0)
            .min((window.bounds().size.width.as_f32() - 16.0).max(0.0))
    }

    pub(crate) fn content_height(&self) -> f32 {
        titlebar_popup_menu_height_for_rows(
            &self
                .rows
                .iter()
                .map(|row| {
                    if row.is_some() {
                        TITLEBAR_POPUP_MENU_ROW_HEIGHT
                    } else {
                        TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT
                    }
                })
                .collect::<Vec<_>>(),
        )
    }

    pub(crate) fn build(
        &self,
        menu: PopupMenu,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right);
        for row in &self.rows {
            let Some(row) = row else {
                menu = menu.separator();
                continue;
            };
            let label = row.label.clone();
            let disabled = row.disabled;
            let action = row.action.boxed_clone();
            let source_window = self.source_window;
            let source_focus = self.source_focus.clone();
            menu = menu.item(
                PopupMenuItem::element(move |_, _| {
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .items_center()
                        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
                        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                        .text_color(titlebar_text_color())
                        .when(disabled, |row| row.opacity(0.5))
                        .child(label.clone())
                })
                .disabled(row.disabled)
                .checked(row.checked)
                .on_click(move |_, _, cx| {
                    let action = action.boxed_clone();
                    let source_focus = source_focus.clone();
                    // PopupMenu dismisses after this callback; dispatch afterward so an action
                    // that opens another popup cannot have it closed by this menu's dismissal.
                    cx.defer(move |cx| {
                        if let Some(source_window) = source_window {
                            let _ = source_window.update(cx, |_, window, cx| {
                                if let Some(focus) = source_focus {
                                    focus.focus(window, cx);
                                }
                                window.dispatch_action(action, cx);
                            });
                        }
                    });
                }),
            );
        }
        menu
    }
}
