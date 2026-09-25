use std::{cell::Cell, rc::Rc};

use gpui_component::{
    ElementExt as _, Side,
    menu::{PopupMenu, PopupMenuItem},
};

use super::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;
use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Browser 2026-09-08 DECISION:
    /// User: make the Browser dropdown work like the GPUI "Open in an app" dropdown and share it across Linux, macOS, and Windows.
    /// The shared popup window keeps the menu above windowed CEF, whose X11 child covers menus painted in the main window.
    pub(crate) fn show_browser_pane_actions_menu(
        &mut self,
        pane_id: BrowserPaneId,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.browser_tabs.find_leaf(pane_id).is_none() {
            return;
        }
        let kind = GpuiTitlebarPopupKind::BrowserActions(pane_id);
        self.set_gpui_titlebar_popup_open(
            kind,
            !self.titlebar_popup_menu_open(kind),
            trigger_bounds,
            window,
            cx,
        );
    }

    pub(crate) fn render_browser_pane_actions_button(
        &self,
        pane_id: BrowserPaneId,
        in_toolbar: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let trigger_bounds = Rc::new(Cell::new(None));
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::BrowserActions(pane_id));
        div()
            .id(format!(
                "ghostex-gpui-browser-{}-overflow-{}",
                if in_toolbar { "toolbar" } else { "tab-action" },
                pane_id.0,
            ))
            .flex()
            .flex_shrink_0()
            .when(in_toolbar, |this| {
                this.h(px(TITLEBAR_CONTROL_HEIGHT))
                    .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH))
                    .rounded(px(TITLEBAR_BUTTON_RADIUS))
            })
            .when(!in_toolbar, |this| {
                this.h_full()
                    .w(px(BROWSER_TAB_ACTION_BUTTON_SIZE))
                    .border_l_1()
                    .border_color(browser_tab_separator_color())
                    .bg(browser_tab_action_cluster_color())
            })
            .items_center()
            .justify_center()
            .cursor_default()
            .when(open, |this| this.bg(titlebar_active_segment_color()))
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(MouseButton::Left, {
                let trigger_bounds = trigger_bounds.clone();
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_browser_pane_actions_menu(pane_id, trigger_bounds.get(), window, cx);
                })
            })
            .on_prepaint({
                let trigger_bounds = trigger_bounds.clone();
                move |bounds, _, _| trigger_bounds.set(Some(bounds))
            })
            .child(self.render_browser_tab_overflow_icon())
            .into_any_element()
    }

    pub(crate) fn build_gpui_browser_actions_popup_menu(
        &self,
        menu: PopupMenu,
        pane_id: BrowserPaneId,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right)
                .menu_element(
                    Box::new(OpenBrowserPaneInExternalBrowser { pane_id: pane_id.0 }),
                    |_, _| {
                        titlebar_popup_standard_menu_row(
                            BROWSER_ICON_WORLD,
                            TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                            "Open in External Browser".to_string(),
                            false,
                        )
                    },
                )
                .separator()
                .item(
                    PopupMenuItem::element(|_, _| titlebar_popup_git_section_label("Appearance"))
                        .disabled(true),
                );
        for appearance in cef::BrowserPageAppearance::ALL {
            menu = menu.menu_element_with_check(
                cef::browser_page_appearance() == appearance,
                Box::new(SetBrowserPageAppearance { appearance }),
                move |_, _| {
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
                        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                        .text_color(titlebar_text_color())
                        .child(appearance.label())
                },
            );
        }
        menu
    }

    pub(crate) fn browser_actions_popup_content_height(&self) -> f32 {
        let mut rows = vec![
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT,
            TITLEBAR_POPUP_MENU_MIN_ITEM_HEIGHT,
        ];
        rows.extend(std::iter::repeat_n(
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            cef::BrowserPageAppearance::ALL.len(),
        ));
        titlebar_popup_menu_height_for_rows(&rows)
    }
}
