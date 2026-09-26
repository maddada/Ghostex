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
    /// A leading glyph. A menu either gives every row one or none, so labels stay aligned.
    icon: Option<&'static str>,
    checked: bool,
    disabled: bool,
    action: Box<dyn gpui::Action>,
}

impl ContextMenuRow {
    fn cloned(&self) -> Self {
        Self {
            label: self.label.clone(),
            icon: self.icon,
            checked: self.checked,
            disabled: self.disabled,
            action: self.action.boxed_clone(),
        }
    }
}

/// CDXC:ContextMenus 2026-09-20 WHY:
/// The view panel's `+` menu and a view tab's right-click menu both end in a `Hidden here` submenu
/// (ruling 2A), which is the first nested row this shared menu has had to draw. It is a row kind
/// rather than a second menu type so sizing, dispatch and dismissal stay in one place.
enum ContextMenuEntry {
    Row(ContextMenuRow),
    Separator,
    Submenu {
        label: SharedString,
        icon: Option<&'static str>,
        disabled: bool,
        rows: Vec<ContextMenuRow>,
    },
}

/// CDXC:ContextMenus 2026-09-11 DECISION:
/// User: convert titlebar, Agents-tab, command-tab, and other shell context menus to the shared GPUI popup so they stay above CEF on Linux, using the same menu across desktop platforms.
/// Size each menu to its labels so short menus such as Copy/Paste stay compact.
/// Menu contents and typed actions stay with their callers; the existing titlebar popup window owns layout, input, and dismissal.
#[derive(Default)]
pub(crate) struct GpuiContextMenu {
    entries: Vec<ContextMenuEntry>,
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
        self.entries.push(ContextMenuEntry::Row(ContextMenuRow {
            label: label.into(),
            icon: None,
            checked: false,
            disabled,
            action,
        }));
        self
    }

    pub(crate) fn menu_with_icon(
        mut self,
        label: impl Into<SharedString>,
        icon: &'static str,
        disabled: bool,
        action: Box<dyn gpui::Action>,
    ) -> Self {
        self.entries.push(ContextMenuEntry::Row(ContextMenuRow {
            label: label.into(),
            icon: Some(icon),
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
        self.entries.push(ContextMenuEntry::Row(ContextMenuRow {
            label: label.into(),
            icon: None,
            checked,
            disabled: false,
            action,
        }));
        self
    }

    /// A nested menu. An empty `rows` draws the parent row disabled rather than a submenu that opens
    /// onto nothing.
    pub(crate) fn submenu(
        self,
        label: impl Into<SharedString>,
        rows: Vec<(SharedString, Box<dyn gpui::Action>)>,
    ) -> Self {
        self.submenu_with_icon(label, None, rows)
    }

    pub(crate) fn submenu_with_icon(
        mut self,
        label: impl Into<SharedString>,
        icon: Option<&'static str>,
        rows: Vec<(SharedString, Box<dyn gpui::Action>)>,
    ) -> Self {
        let disabled = rows.is_empty();
        self.entries.push(ContextMenuEntry::Submenu {
            label: label.into(),
            icon,
            disabled,
            rows: rows
                .into_iter()
                .map(|(label, action)| ContextMenuRow {
                    label,
                    icon: None,
                    checked: false,
                    disabled: false,
                    action,
                })
                .collect(),
        });
        self
    }

    pub(crate) fn separator(mut self) -> Self {
        if self
            .entries
            .last()
            .is_some_and(|entry| !matches!(entry, ContextMenuEntry::Separator))
        {
            self.entries.push(ContextMenuEntry::Separator);
        }
        self
    }

    pub(crate) fn show(self, position: Point<Pixels>, window: &mut Window, cx: &mut App) {
        self.show_anchored(
            Bounds {
                origin: position,
                size: size(px(1.0), px(1.0)),
            },
            false,
            window,
            cx,
        );
    }

    /// CDXC:ContextMenus 2026-09-22 DECISION:
    /// User: the Browser profile button and the view strip's `+` open their menu like the "Browser pane actions menu" button: always below the button in a set position, never where the pointer landed, and clicking the button while its menu is open closes it.
    /// The button's bounds are the popup's trigger bounds, so the root's outside-click capture leaves that click to this toggle.
    pub(crate) fn toggle_below(
        self,
        trigger_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.show_anchored(trigger_bounds, true, window, cx);
    }

    fn show_anchored(
        mut self,
        trigger_bounds: Bounds<Pixels>,
        toggle: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        while self
            .entries
            .last()
            .is_some_and(|entry| matches!(entry, ContextMenuEntry::Separator))
        {
            self.entries.pop();
        }
        if self.entries.is_empty() {
            return;
        }
        let Some(root) = window.root::<Root>().flatten() else {
            return;
        };
        let Ok(app) = root.read(cx).view().clone().downcast::<GhostexGpuiApp>() else {
            return;
        };
        self.show_for_app_anchored(app, trigger_bounds, toggle, window, cx);
    }

    pub(crate) fn show_for_app(
        self,
        app: Entity<GhostexGpuiApp>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.show_for_app_anchored(
            app,
            Bounds {
                origin: position,
                size: size(px(1.0), px(1.0)),
            },
            false,
            window,
            cx,
        );
    }

    /// `toggle_below` for a trigger drawn in another window than the app's own root, given in
    /// `window`'s coordinates.
    pub(crate) fn toggle_below_for_app(
        self,
        app: Entity<GhostexGpuiApp>,
        trigger_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.show_for_app_anchored(app, trigger_bounds, true, window, cx);
    }

    fn show_for_app_anchored(
        mut self,
        app: Entity<GhostexGpuiApp>,
        trigger_bounds: Bounds<Pixels>,
        toggle: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let source_window = Window::window_handle(window);
        self.source_window = Some(source_window);
        self.source_focus = window.focused(cx);
        // Defer beyond the caller's entity borrow, including terminal-body callers.
        cx.defer(move |cx| {
            let _ = source_window.update(cx, |_, window, cx| {
                app.update(cx, |app, cx| {
                    let already_open = toggle
                        && app.titlebar_popup_menu.as_ref().is_some_and(|state| {
                            state.kind == GpuiTitlebarPopupKind::ContextMenu
                                && state.trigger_bounds == trigger_bounds
                        });
                    app.close_gpui_titlebar_popup(None, window, cx);
                    if already_open {
                        return;
                    }
                    app.context_menu = Some(self);
                    app.set_gpui_titlebar_popup_open(
                        GpuiTitlebarPopupKind::ContextMenu,
                        true,
                        Some(trigger_bounds),
                        window,
                        cx,
                    );
                })
            });
        });
    }

    fn row_width(row: &ContextMenuRow, window: &Window, extra: f32) -> f32 {
        let style = window.text_style();
        let line = window.text_system().shape_line(
            row.label.clone(),
            px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE),
            &[style.to_run(row.label.len())],
            None,
        );
        // Shared menu geometry: 10px row insets, 6px outer padding, and a 1px border.
        let check_width = if row.checked { 28.0 } else { 0.0 };
        let icon_width = if row.icon.is_some() {
            TITLEBAR_POPUP_MENU_ROW_ICON_SIZE + 8.0
        } else {
            0.0
        };
        // The 24px slack keeps the widest row whole: this measures with the main window's text style,
        // which can run narrower than the popup's, and with less slack "Browser Tab" drew as "Browser…".
        line.width.as_f32() + 34.0 + 24.0 + check_width + icon_width + extra
    }

    pub(crate) fn content_width(&self, window: &Window) -> f32 {
        let label_width = self
            .entries
            .iter()
            .map(|entry| match entry {
                ContextMenuEntry::Separator => 0.0,
                ContextMenuEntry::Row(row) => Self::row_width(row, window, 0.0),
                // A submenu row keeps room for its own chevron.
                ContextMenuEntry::Submenu { label, icon, .. } => Self::row_width(
                    &ContextMenuRow {
                        label: label.clone(),
                        icon: *icon,
                        checked: false,
                        disabled: false,
                        action: Box::new(gpui::NoAction {}),
                    },
                    window,
                    18.0,
                ),
            })
            .fold(0.0_f32, f32::max);
        // An openable submenu with an icon makes PopupMenu reserve a blank 12px icon plus a 4px gap
        // at the start of every other row, on top of that row's own icon.
        let reserved_icon_column = if self.entries.iter().any(|entry| {
            matches!(
                entry,
                ContextMenuEntry::Submenu {
                    icon: Some(_),
                    disabled: false,
                    ..
                }
            )
        }) {
            16.0
        } else {
            0.0
        };
        (label_width + reserved_icon_column)
            .ceil()
            .clamp(96.0, 400.0)
            .min((window.bounds().size.width.as_f32() - 16.0).max(0.0))
    }

    pub(crate) fn content_height(&self) -> f32 {
        titlebar_popup_menu_height_for_rows(
            &self
                .entries
                .iter()
                .map(|entry| match entry {
                    ContextMenuEntry::Separator => TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT,
                    _ => TITLEBAR_POPUP_MENU_ROW_HEIGHT,
                })
                .collect::<Vec<_>>(),
        )
    }

    fn popup_menu_item(&self, row: &ContextMenuRow) -> PopupMenuItem {
        let label = row.label.clone();
        let icon = row.icon;
        let disabled = row.disabled;
        let action = row.action.boxed_clone();
        let source_window = self.source_window;
        let source_focus = self.source_focus.clone();
        PopupMenuItem::element(move |_, _| {
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .items_center()
                .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
                .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                .text_color(titlebar_popup_menu_foreground())
                .gap(px(8.0))
                .when(disabled, |row| row.opacity(0.42))
                .when_some(icon, |row, icon| {
                    row.child(titlebar_svg_icon(
                        icon,
                        TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                        titlebar_popup_menu_foreground(),
                    ))
                })
                // The menu is sized to its widest label, so a label keeps its full width rather
                // than shrinking into an ellipsis ("Browser…" for "Browser Tab").
                .child(div().flex_none().child(label.clone()))
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
        })
    }

    pub(crate) fn build(
        &self,
        menu: PopupMenu,
        width: f32,
        max_height: f32,
        scrollable: bool,
        window: &mut Window,
        cx: &mut gpui::Context<PopupMenu>,
    ) -> PopupMenu {
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right);
        for entry in &self.entries {
            match entry {
                ContextMenuEntry::Separator => menu = menu.separator(),
                ContextMenuEntry::Row(row) => menu = menu.item(self.popup_menu_item(row)),
                ContextMenuEntry::Submenu {
                    label,
                    icon,
                    disabled,
                    rows,
                } => {
                    if *disabled {
                        menu = menu.item(self.popup_menu_item(&ContextMenuRow {
                            label: label.clone(),
                            icon: *icon,
                            checked: false,
                            disabled: true,
                            action: Box::new(gpui::NoAction {}),
                        }));
                        continue;
                    }
                    let nested = GpuiContextMenu {
                        entries: rows
                            .iter()
                            .map(|row| ContextMenuEntry::Row(row.cloned()))
                            .collect(),
                        source_window: self.source_window,
                        source_focus: self.source_focus.clone(),
                    };
                    menu = menu.submenu_with_icon(
                        icon.map(|icon| gpui_component::Icon::empty().path(icon)),
                        label.clone(),
                        window,
                        cx,
                        move |menu, window, cx| {
                            nested.build(menu, width, max_height, scrollable, window, cx)
                        },
                    );
                }
            }
        }
        menu
    }
}
