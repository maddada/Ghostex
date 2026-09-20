// The titlebar's trailing "⋯" button and the menu behind it: the surfaces that are
// occasional rather than part of the active project. Each row opens the same native
// child window its own titlebar button used to open, re-anchored to this button.

use std::time::Duration;

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::Side;
use gpui_component::menu::PopupMenu;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use super::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

/// CDXC:Titlebar 2026-09-20 DECISION:
/// User: Ask Ghostex, Tips & Tricks, Resources, Dev servers and Extensions leave the titlebar for one trailing "⋯" menu, so the titlebar row holds only the active project's own controls.
/// Per ruling 12 Ask Ghostex stays in this menu once it is a view; so do Tips & Tricks, Resources and Dev servers, and all four now open their tab instead of a dropdown, which is what screen 06 says the menu becomes. Only Extensions is still a popup, because it is a menu of extensions rather than a page.
/// An item switched off in Settings, or scoped away from this project, is not listed at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarMoreMenuItem {
    AskGhostex,
    Tips,
    Resources,
    DevServers,
    Extensions,
}

impl GpuiTitlebarMoreMenuItem {
    /// Menu order, top to bottom.
    const ALL: [Self; 5] = [
        Self::AskGhostex,
        Self::Tips,
        Self::Resources,
        Self::DevServers,
        Self::Extensions,
    ];

    pub(crate) fn from_index(index: u64) -> Option<Self> {
        Self::ALL.get(usize::try_from(index).ok()?).copied()
    }

    fn index(self) -> u64 {
        Self::ALL
            .iter()
            .position(|item| *item == self)
            .unwrap_or_default() as u64
    }

    fn label(self) -> &'static str {
        match self {
            Self::AskGhostex => "Ask Ghostex",
            Self::Tips => "Tips & Tricks",
            Self::Resources => "Resources",
            Self::DevServers => "Dev servers",
            Self::Extensions => "Extensions",
        }
    }

    fn icon_path(self) -> &'static str {
        match self {
            Self::AskGhostex => TITLEBAR_ICON_HELP,
            Self::Tips => TITLEBAR_ICON_INFO,
            Self::Resources => TITLEBAR_ICON_DEVICE_DESKTOP,
            Self::DevServers => BROWSER_ICON_WORLD,
            Self::Extensions => TITLEBAR_ICON_EXTENSIONS,
        }
    }

    /// The Settings switch and the view scope key this item's titlebar
    /// button was gated by, so the one gate keeps deciding whether it is offered.
    fn visibility_gate(self) -> (&'static str, &'static str) {
        match self {
            Self::AskGhostex => (HELP_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "help"),
            Self::Tips => (TIPS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "tips"),
            Self::Resources => (RESOURCES_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "resources"),
            Self::DevServers => (
                DEV_SERVERS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                "devServers",
            ),
            Self::Extensions => (
                EXTENSIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                "extensionsButton",
            ),
        }
    }

    /// The Ghostex page this row opens as a view tab, for the three rows that became pages.
    fn ghostex_page(self) -> Option<GhostexPage> {
        match self {
            Self::AskGhostex => Some(GhostexPage::Ask),
            Self::Tips => Some(GhostexPage::Tips),
            Self::Resources => Some(GhostexPage::Resources),
            Self::DevServers | Self::Extensions => None,
        }
    }
}

impl GhostexGpuiApp {
    /// The rows the ⋯ menu offers right now, after the single titlebar-button gate. A row that is
    /// now a page asks the page's own availability instead, because an app-wide page is never
    /// scoped away from a project and the two answers must not disagree.
    pub(crate) fn titlebar_more_menu_items(&self) -> Vec<GpuiTitlebarMoreMenuItem> {
        GpuiTitlebarMoreMenuItem::ALL
            .into_iter()
            .filter(|item| match item.ghostex_page() {
                Some(page) => self.titlebar_mode_available(TitlebarMode::Ghostex(page)),
                None => {
                    let (settings_key, official_extension_id) = item.visibility_gate();
                    if self.titlebar_button_hidden(settings_key, official_extension_id) {
                        return false;
                    }
                    // Dev servers is the Browser view's start page now, so a project that cannot
                    // open Browser cannot reach the list either.
                    *item != GpuiTitlebarMoreMenuItem::DevServers
                        || self.titlebar_mode_available(TitlebarMode::Browser)
                }
            })
            .collect()
    }

    pub(crate) fn titlebar_more_menu_visible(&self) -> bool {
        !self.titlebar_more_menu_items().is_empty()
    }

    pub(crate) fn titlebar_more_popup_content_height(&self) -> f32 {
        let rows = vec![TITLEBAR_POPUP_MENU_ROW_HEIGHT; self.titlebar_more_menu_items().len()];
        titlebar_popup_menu_height_for_rows(&rows)
    }

    pub(crate) fn build_gpui_titlebar_more_popup_menu(
        &self,
        menu: PopupMenu,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right);
        for item in self.titlebar_more_menu_items() {
            menu = menu.menu_element(
                Box::new(OpenGpuiTitlebarMoreMenuItem {
                    item_index: item.index(),
                }),
                move |_, _| {
                    titlebar_popup_standard_menu_row(
                        item.icon_path(),
                        TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                        item.label().to_string(),
                        false,
                    )
                },
            );
        }
        menu
    }

    /// Opens one row: a tab for the three rows that are pages now, the Browser view's start page for
    /// Dev servers, and the Extensions popup anchored to the ⋯ button for the one row still a menu.
    /// The gate is re-checked because the menu row and the click are separated by a frame.
    pub(crate) fn open_titlebar_more_menu_item(
        &mut self,
        item: GpuiTitlebarMoreMenuItem,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_more_menu_items().contains(&item) {
            return;
        }
        if let Some(page) = item.ghostex_page() {
            self.open_view_tab(TitlebarMode::Ghostex(page), window, cx);
            return;
        }
        if item == GpuiTitlebarMoreMenuItem::DevServers {
            self.open_browser_start_page(window, cx);
            return;
        }
        let trigger_bounds = self.titlebar_more_button_bounds.get();
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Extensions,
            true,
            trigger_bounds,
            window,
            cx,
        );
    }

    pub(crate) fn toggle_gpui_titlebar_more_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_more_menu_visible() {
            return;
        }
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::More);
        let trigger_bounds = self.titlebar_more_button_bounds.get();
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::More,
            open,
            trigger_bounds,
            window,
            cx,
        );
    }

    /// The trailing ⋯ button. It records its own bounds because the Ghostex Help
    /// hotkey opens the Help panel without going through the menu and still has to
    /// anchor it somewhere real.
    pub(crate) fn render_titlebar_more_button(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::More);
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let show_badge = self
            .titlebar_more_menu_items()
            .contains(&GpuiTitlebarMoreMenuItem::Tips)
            && self.titlebar_tips_badge_count() > 0;
        let button_bounds = self.titlebar_more_button_bounds.clone();

        div()
            .id("ghostex-gpui-titlebar-button-more")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
            .items_center()
            .justify_center()
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .text_color(icon_color)
            .cursor_default()
            .when(open, |this| this.bg(titlebar_active_segment_color()))
            .hover(move |this| {
                if open {
                    this.bg(titlebar_active_segment_color())
                        .text_color(titlebar_icon_hover_color())
                } else {
                    this.bg(titlebar_button_hover_color())
                        .text_color(titlebar_icon_hover_color())
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::More,
                        "left",
                        "togglePopup",
                        open,
                        this.titlebar_more_button_bounds.get(),
                        event,
                        window,
                    );
                    this.toggle_gpui_titlebar_more_menu(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::More,
                        "right",
                        "togglePopup",
                        open,
                        this.titlebar_more_button_bounds.get(),
                        event,
                        window,
                    );
                    this.toggle_gpui_titlebar_more_menu(window, cx);
                }),
            )
            .when(!open, |this| {
                this.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    Duration::from_millis(300),
                    |window, cx| titlebar_tooltip(TITLEBAR_MORE_TOOLTIP, window, cx),
                )
            })
            .on_prepaint(move |bounds, window, _cx| {
                let previous = button_bounds.get();
                let first_capture = previous.is_none();
                let moved = previous != Some(bounds);
                button_bounds.set(Some(bounds));
                if first_capture || moved {
                    log_gpui_titlebar_popup_anchor(
                        GpuiTitlebarPopupKind::More,
                        bounds,
                        first_capture,
                        moved,
                        window,
                    );
                    window.request_animation_frame();
                }
            })
            .child(titlebar_svg_icon(TITLEBAR_ICON_DOTS, 16.0, icon_color))
            .when(show_badge, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(2.0))
                        .top(px(5.0))
                        .size(px(7.5))
                        .rounded_full()
                        .border_1()
                        .border_color(titlebar_background())
                        // Picked for dark chrome, where the header is near-black; on a light header
                        // the same pale blue reads as a smudge rather than a badge.
                        .bg(chrome_color(0x95d7f6, 0x1d7fb8)),
                )
            })
            .into_any_element()
    }
}
