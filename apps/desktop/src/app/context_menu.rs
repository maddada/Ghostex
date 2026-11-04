use gpui::{AnyWindowHandle, SharedString, WeakEntity};
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
    /// Drawn in the destructive red and bold: an armed "Confirm delete" (see `destructive`).
    destructive: bool,
    action: Box<dyn gpui::Action>,
}

impl ContextMenuRow {
    fn cloned(&self) -> Self {
        Self {
            label: self.label.clone(),
            icon: self.icon,
            checked: self.checked,
            disabled: self.disabled,
            destructive: self.destructive,
            action: self.action.boxed_clone(),
        }
    }
}

/// CDXC:ContextMenus 2026-09-27 WHY:
/// A submenu is a row kind rather than a second menu type so sizing, dispatch and dismissal stay in
/// one place, and it holds the same entries as the menu itself (ticks and separators included, for a
/// view tab's `Show in ▸`). Picking a submenu row reopens the menu in the same place showing that
/// submenu's rows, like the Kanban filters and Quick Access's Tag… do: the menu draws in a popup window
/// sized to itself, so gpui-component's flyout, which opens to the right of its row, was drawn outside
/// that window and never showed. Supersedes 2026-09-26, when submenus flew out.
enum ContextMenuEntry {
    Row(ContextMenuRow),
    Separator,
    Submenu {
        label: SharedString,
        icon: Option<&'static str>,
        disabled: bool,
        entries: Vec<ContextMenuEntry>,
    },
}

impl ContextMenuEntry {
    fn cloned(&self) -> Self {
        match self {
            Self::Row(row) => Self::Row(row.cloned()),
            Self::Separator => Self::Separator,
            Self::Submenu {
                label,
                icon,
                disabled,
                entries,
            } => Self::Submenu {
                label: label.clone(),
                icon: *icon,
                disabled: *disabled,
                entries: entries.iter().map(Self::cloned).collect(),
            },
        }
    }
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
    /// Where the menu opened and for which app, so a submenu row can reopen it in the same place.
    app: Option<WeakEntity<GhostexGpuiApp>>,
    trigger_bounds: Option<Bounds<Pixels>>,
    /// Keeps the windows the menu was opened from free of tooltips while it is up.
    tooltips: Vec<gpui::TooltipSuppression>,
}

impl GpuiContextMenu {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Also hide `window`'s tooltips while the menu is up, for a trigger drawn in a child window
    /// (Docs' files list) whose menu is hosted over the main window.
    pub(crate) fn suppress_tooltips_in(mut self, window: &mut Window, cx: &mut App) -> Self {
        self.tooltips.push(Root::suppress_tooltips(window, cx));
        self
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
            destructive: false,
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
            destructive: false,
            action,
        }));
        self
    }

    /// Draws the row just added in the destructive red, bold: the armed second step of a
    /// destructive action ("Confirm delete").
    ///
    /// CDXC:ContextMenus 2026-10-10 DECISION:
    /// User: "please when i click on delete in the files list make confirm delete text show red color text not stay normal color and make it bold so it's noticeable". A menu row asking to confirm a destructive action draws its label and icon in the destructive red the native modals use for their destructive buttons (`titlebar_popup_menu_destructive`), bold, until it is confirmed or the menu closes.
    pub(crate) fn destructive(mut self) -> Self {
        if let Some(ContextMenuEntry::Row(row)) = self.entries.last_mut() {
            row.destructive = true;
        }
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
            destructive: false,
            action,
        }));
        self
    }

    /// A row that can be both ticked and greyed out: the current choice of a list that cannot
    /// change right now.
    pub(crate) fn menu_with_check_and_disabled(
        mut self,
        label: impl Into<SharedString>,
        checked: bool,
        disabled: bool,
        action: Box<dyn gpui::Action>,
    ) -> Self {
        self.entries.push(ContextMenuEntry::Row(ContextMenuRow {
            label: label.into(),
            icon: None,
            checked,
            disabled,
            destructive: false,
            action,
        }));
        self
    }

    /// A nested menu. An empty `rows` draws the parent row disabled rather than a submenu that opens
    /// onto nothing.
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
            entries: rows
                .into_iter()
                .map(|(label, action)| {
                    ContextMenuEntry::Row(ContextMenuRow {
                        label,
                        icon: None,
                        checked: false,
                        disabled: false,
                        destructive: false,
                        action,
                    })
                })
                .collect(),
        });
        self
    }

    /// A nested menu built with the same row builders as this one, so its rows can carry ticks and
    /// separators. A submenu with nothing in it is left out rather than drawn disabled.
    pub(crate) fn submenu_menu(
        mut self,
        label: impl Into<SharedString>,
        mut submenu: GpuiContextMenu,
    ) -> Self {
        submenu.trim_trailing_separators();
        if submenu.entries.is_empty() {
            return self;
        }
        self.entries.push(ContextMenuEntry::Submenu {
            label: label.into(),
            icon: None,
            disabled: false,
            entries: submenu.entries,
        });
        self
    }

    fn trim_trailing_separators(&mut self) {
        while self
            .entries
            .last()
            .is_some_and(|entry| matches!(entry, ContextMenuEntry::Separator))
        {
            self.entries.pop();
        }
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
        self.trim_trailing_separators();
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
        self.app = Some(app.downgrade());
        self.trigger_bounds = Some(trigger_bounds);
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
                    self.tooltips.push(Root::suppress_tooltips(window, cx));
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
        let mut style = window.text_style();
        // A destructive row draws bold, which runs wider.
        if row.destructive {
            style.font_weight = gpui::FontWeight::BOLD;
        }
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
                        destructive: false,
                        action: Box::new(gpui::NoAction {}),
                    },
                    window,
                    18.0,
                ),
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
                .entries
                .iter()
                .map(|entry| match entry {
                    ContextMenuEntry::Separator => TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT,
                    _ => TITLEBAR_POPUP_MENU_ROW_HEIGHT,
                })
                .collect::<Vec<_>>(),
        )
    }

    fn row_element(
        label: SharedString,
        icon: Option<&'static str>,
        disabled: bool,
        chevron: bool,
        destructive: bool,
    ) -> PopupMenuItem {
        PopupMenuItem::element(move |_, _| {
            let ink = if destructive {
                titlebar_popup_menu_destructive()
            } else {
                titlebar_popup_menu_foreground()
            };
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .items_center()
                .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
                .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                .text_color(ink)
                .when(destructive, |row| row.font_weight(gpui::FontWeight::BOLD))
                .gap(px(8.0))
                .when(disabled, |row| row.opacity(0.42))
                .when_some(icon, |row, icon| {
                    row.child(titlebar_svg_icon(
                        icon,
                        TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                        ink,
                    ))
                })
                // The menu is sized to its widest label, so a label keeps its full width rather
                // than shrinking into an ellipsis ("Browser…" for "Browser Tab").
                .child(div().flex_none().child(label.clone()))
                .when(chevron, |row| {
                    row.child(div().flex_1())
                        .child(div().flex_none().opacity(0.6).child(titlebar_svg_icon(
                            TITLEBAR_ICON_CHEVRON_RIGHT,
                            12.0,
                            titlebar_popup_menu_foreground(),
                        )))
                })
        })
    }

    /// CDXC:ContextMenus 2026-09-28 WHY:
    /// A row dispatches from the element that had focus when the menu opened, so handlers local to
    /// that element still see it. When that element is no longer drawn (the Code tab was closed
    /// while it held focus) or nothing had focus, GPUI starts the dispatch at the window root, above
    /// the app's `on_action` listeners, and the row did nothing: `+` → Code after closing Code. Those
    /// rows dispatch from the app's always-drawn `root_action_focus_handle` instead; menus opened
    /// from another window keep the old path because that handle is not drawn there. Whether the
    /// opener still reaches a handler is asked with `is_action_available_in` (an undrawn handle
    /// resolves to the window root), because the fallback handle sits on a zero-size leaf and no
    /// longer contains the rest of the body.
    fn popup_menu_item(&self, row: &ContextMenuRow) -> PopupMenuItem {
        let action = row.action.boxed_clone();
        let source_window = self.source_window;
        let source_focus = self.source_focus.clone();
        let app = self.app.clone();
        Self::row_element(
            row.label.clone(),
            row.icon,
            row.disabled,
            false,
            row.destructive,
        )
        .disabled(row.disabled)
        .checked(row.checked)
        .on_click(move |_, _, cx| {
            let action = action.boxed_clone();
            let source_focus = source_focus.clone();
            let app = app.clone();
            // PopupMenu dismisses after this callback; dispatch afterward so an action
            // that opens another popup cannot have it closed by this menu's dismissal.
            cx.defer(move |cx| {
                let Some(source_window) = source_window else {
                    return;
                };
                let _ = source_window.update(cx, |_, window, cx| {
                    // Only the window the app itself draws has the handle to fall back to.
                    let root_focus = app.and_then(|app| app.upgrade()).filter(|app| {
                        window
                            .root::<Root>()
                            .flatten()
                            .is_some_and(|root| root.read(cx).view().entity_id() == app.entity_id())
                    });
                    let root_focus =
                        root_focus.map(|app| app.read(cx).root_action_focus_handle.clone());
                    let drawn_source_focus = source_focus.filter(|focus| {
                        root_focus.is_none()
                            || window.is_action_available_in(action.as_ref(), focus)
                    });
                    match (drawn_source_focus, root_focus) {
                        (Some(focus), _) => {
                            focus.focus(window, cx);
                            window.dispatch_action(action, cx);
                        }
                        (None, Some(root)) => root.dispatch_action(action.as_ref(), window, cx),
                        (None, None) => window.dispatch_action(action, cx),
                    }
                });
            });
        })
    }

    /// A submenu row: picking it reopens this menu where it was, showing the submenu's rows.
    fn popup_submenu_item(
        &self,
        label: &SharedString,
        icon: Option<&'static str>,
        entries: &[ContextMenuEntry],
    ) -> PopupMenuItem {
        let entries = entries
            .iter()
            .map(ContextMenuEntry::cloned)
            .collect::<Vec<_>>();
        let source_window = self.source_window;
        let source_focus = self.source_focus.clone();
        let app = self.app.clone();
        let trigger_bounds = self.trigger_bounds;
        Self::row_element(label.clone(), icon, false, true, false).on_click(move |_, _, cx| {
            let (Some(source_window), Some(app), Some(trigger_bounds)) = (
                source_window,
                app.as_ref().and_then(WeakEntity::upgrade),
                trigger_bounds,
            ) else {
                return;
            };
            let submenu = GpuiContextMenu {
                entries: entries.iter().map(ContextMenuEntry::cloned).collect(),
                ..GpuiContextMenu::default()
            };
            let source_focus = source_focus.clone();
            // PopupMenu dismisses after this callback, so the submenu opens afterward.
            cx.defer(move |cx| {
                let _ = source_window.update(cx, |_, window, cx| {
                    if let Some(focus) = source_focus {
                        focus.focus(window, cx);
                    }
                    submenu.show_for_app_anchored(app, trigger_bounds, false, window, cx);
                });
            });
        })
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
        for entry in &self.entries {
            match entry {
                ContextMenuEntry::Separator => menu = menu.separator(),
                ContextMenuEntry::Row(row) => menu = menu.item(self.popup_menu_item(row)),
                ContextMenuEntry::Submenu {
                    label,
                    icon,
                    disabled,
                    entries,
                } => {
                    menu = if *disabled {
                        menu.item(
                            Self::row_element(label.clone(), *icon, true, false, false)
                                .disabled(true),
                        )
                    } else {
                        menu.item(self.popup_submenu_item(label, *icon, entries))
                    };
                }
            }
        }
        menu
    }
}
