//! A row's actions menu: the list the controller (gx-core `quick_access`) sends back for a right-click, for the footer's
//! Actions panel (Cmd+K), and for a submenu one of those items opens (Tag…).
//!
//! The pointer menu opens where the click landed. The Actions panel opens above the footer's
//! right edge, is driven from the keyboard, and filters as the user types, like Raycast's.
use super::chrome::{QuickAccessMenuPaint, quick_access_icon, quick_access_menu_stand_in};
use super::model::QuickAccessMenuItem;
use super::palette::{
    QUICK_ACCESS_FOOTER_HEIGHT, QUICK_ACCESS_ITEM_FONT_SIZE, QUICK_ACCESS_RADIUS_ACTIONS_MENU,
    QUICK_ACCESS_RADIUS_MENU_ITEM, QuickAccessPalette, hsla,
};
use super::rows::hotkey_keycaps;
use super::window::GpuiQuickAccessWindow;
use crate::app::window::native_modal_kit::MODAL_UI_FONT;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Anchor, AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, KeyDownEvent,
    MouseDownEvent, ParentElement as _, Pixels, Point, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div, point, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

#[derive(Clone, Copy)]
pub(crate) struct QuickAccessMenuRequest {
    position: Point<Pixels>,
    corner: Anchor,
    /// The Actions panel: keyboard-driven, with a type-to-filter line.
    panel: bool,
}

impl QuickAccessMenuRequest {
    pub(crate) fn at_pointer(position: Point<Pixels>) -> Self {
        Self {
            position,
            corner: Anchor::TopLeft,
            panel: false,
        }
    }

    pub(crate) fn actions_panel(window: &Window) -> Self {
        let viewport = window.viewport_size();
        Self {
            position: point(
                viewport.width - px(8.0),
                viewport.height - px(QUICK_ACCESS_FOOTER_HEIGHT + 6.0),
            ),
            corner: Anchor::BottomRight,
            panel: true,
        }
    }
}

pub(crate) struct QuickAccessOpenMenu {
    request: QuickAccessMenuRequest,
    items: Vec<QuickAccessMenuItem>,
    query: String,
    highlight: usize,
}

impl QuickAccessOpenMenu {
    pub(crate) fn is_panel(&self) -> bool {
        self.request.panel
    }

    /// The rows the filter leaves. Separators only make sense in the unfiltered list.
    fn visible(&self) -> Vec<&QuickAccessMenuItem> {
        let query = self.query.trim().to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                if query.is_empty() {
                    return true;
                }
                !item.separator && item.label.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn selectable(&self) -> Vec<usize> {
        self.visible()
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.separator && !item.disabled)
            .map(|(index, _)| index)
            .collect()
    }

    fn clamp_highlight(&mut self) {
        let selectable = self.selectable();
        if !selectable.contains(&self.highlight) {
            self.highlight = selectable.first().copied().unwrap_or(0);
        }
    }
}

impl GpuiQuickAccessWindow {
    /// The controller's answer to the last menu request. An empty list closes whatever is showing.
    pub(crate) fn apply_menu(&mut self, items: Vec<QuickAccessMenuItem>, cx: &mut Context<Self>) {
        self.context_menu = match self.menu_request {
            Some(request) if !items.is_empty() => {
                let mut menu = QuickAccessOpenMenu {
                    request,
                    items,
                    query: String::new(),
                    highlight: 0,
                };
                menu.clamp_highlight();
                Some(menu)
            }
            _ => None,
        };
        cx.notify();
    }

    pub(super) fn request_row_menu(
        &mut self,
        key: &str,
        request: QuickAccessMenuRequest,
        cx: &mut Context<Self>,
    ) {
        self.close_filter_menus();
        self.context_menu = None;
        self.menu_request = Some(request);
        self.post(
            json!({
                "type": "secondary",
                "key": key,
                "x": f32::from(request.position.x),
                "y": f32::from(request.position.y),
            }),
            cx,
        );
        cx.notify();
    }

    /// Cmd+K and the footer's Actions button: toggles the panel for the selected row.
    pub(super) fn open_actions_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .context_menu
            .as_ref()
            .is_some_and(QuickAccessOpenMenu::is_panel)
        {
            self.context_menu = None;
            cx.notify();
            return;
        }
        let key = self.selected_key();
        self.request_row_menu(&key, QuickAccessMenuRequest::actions_panel(window), cx);
    }

    pub(super) fn actions_panel_open(&self) -> bool {
        self.context_menu
            .as_ref()
            .is_some_and(QuickAccessOpenMenu::is_panel)
    }

    fn choose_menu_item(&mut self, id: String, cx: &mut Context<Self>) {
        // "New tag…" anchors the create-tag popover where the menu was.
        self.tag_composer_anchor = self.context_menu.as_ref().map(|menu| menu.request.position);
        self.context_menu = None;
        self.post(json!({ "type": "menuItem", "id": id }), cx);
        cx.notify();
    }

    /// An open menu owns the keyboard: Up/Down/Return/Escape, and the panel's typed filter.
    pub(super) fn actions_menu_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        match key {
            "escape" => {
                self.context_menu = None;
            }
            "k" if modifiers.secondary() => {
                self.context_menu = None;
            }
            "up" | "down" => {
                let selectable = menu.selectable();
                if let Some(current) = selectable.iter().position(|index| *index == menu.highlight)
                {
                    let count = selectable.len();
                    let next = if key == "down" {
                        (current + 1) % count
                    } else {
                        (current + count - 1) % count
                    };
                    menu.highlight = selectable[next];
                } else {
                    menu.clamp_highlight();
                }
            }
            "enter" => {
                let chosen = menu
                    .visible()
                    .get(menu.highlight)
                    .filter(|item| !item.separator && !item.disabled)
                    .map(|item| item.id.clone());
                if let Some(id) = chosen {
                    self.choose_menu_item(id, cx);
                }
            }
            "backspace" if menu.is_panel() => {
                menu.query.pop();
                menu.clamp_highlight();
            }
            _ => {
                if menu.is_panel()
                    && !modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && let Some(text) = event.keystroke.key_char.as_deref()
                    && text.chars().all(|character| !character.is_control())
                {
                    menu.query.push_str(text);
                    menu.highlight = 0;
                    menu.clamp_highlight();
                }
            }
        }
        cx.notify();
        true
    }

    pub(super) fn render_context_menu(
        &self,
        p: &QuickAccessPalette,
        menu: &QuickAccessOpenMenu,
        paint: &QuickAccessMenuPaint,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *p;
        let panel = menu.is_panel();
        let visible = menu.visible();
        let rows = visible
            .iter()
            .enumerate()
            .map(|(index, item)| {
                if item.separator {
                    return div()
                        .my(px(4.0))
                        .mx(px(4.0))
                        .h(px(1.0))
                        .bg(hsla(p.menu_border))
                        .into_any_element();
                }
                let id = item.id.clone();
                let color = if item.danger {
                    p.destructive
                } else {
                    p.foreground
                };
                let highlighted = panel && index == menu.highlight;
                h_flex()
                    .id(("quick-access-menu-item", index))
                    .w_full()
                    .min_h(px(32.0))
                    .px(px(8.0))
                    .gap(px(10.0))
                    .items_center()
                    .rounded(px(QUICK_ACCESS_RADIUS_MENU_ITEM + 1.0))
                    .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
                    .line_height(px(20.0))
                    .text_color(hsla(color))
                    .when(item.disabled, |this| this.opacity(0.45))
                    .when(highlighted && !item.disabled, |this| {
                        this.bg(hsla(p.menu_hover))
                    })
                    .when(!item.disabled, |this| {
                        this.cursor_pointer()
                            .hover(move |this| this.bg(hsla(p.menu_hover)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.choose_menu_item(id.clone(), cx);
                            }))
                    })
                    .child(quick_access_icon(
                        &item.icon,
                        15.0,
                        if item.danger { p.destructive } else { p.muted },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(SharedString::from(item.label.clone())),
                    )
                    .children((!item.hotkey.is_empty()).then(|| hotkey_keycaps(&p, &item.hotkey)))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let filter_line = panel.then(|| {
            h_flex()
                .w_full()
                .h(px(34.0))
                .mt(px(4.0))
                .px(px(9.0))
                .items_center()
                .border_t_1()
                .border_color(hsla(p.menu_border))
                .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
                .text_color(hsla(if menu.query.is_empty() {
                    p.muted
                } else {
                    p.foreground
                }))
                .child(SharedString::from(if menu.query.is_empty() {
                    "Search actions…".to_string()
                } else {
                    menu.query.clone()
                }))
        });
        let empty = (panel && visible.is_empty()).then(|| {
            div()
                .w_full()
                .px(px(9.0))
                .py(px(8.0))
                .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
                .text_color(hsla(p.muted))
                .child("No matching actions")
        });
        let surface = v_flex()
            .id("quick-access-context-menu")
            .p(px(5.0))
            .gap(px(1.0))
            .rounded(px(QUICK_ACCESS_RADIUS_ACTIONS_MENU))
            .border_1()
            .border_color(hsla(p.menu_border))
            .children(rows)
            .children(empty)
            .children(filter_line);
        if matches!(paint, QuickAccessMenuPaint::Hosted) {
            return surface
                .size_full()
                .font_family(MODAL_UI_FONT)
                .bg(hsla(p.hosted_menu_background))
                .into_any_element();
        }
        let surface = surface.w(px(if panel { 290.0 } else { 240.0 }));
        let on_mouse_down_out = cx.listener(|this, _: &MouseDownEvent, _window, cx| {
            this.context_menu = None;
            cx.notify();
        });
        let content = match paint {
            QuickAccessMenuPaint::StandIn(frames) => quick_access_menu_stand_in(
                surface.invisible(),
                "quick-access-context-menu",
                frames.clone(),
                on_mouse_down_out,
                cx,
            ),
            _ => surface
                .occlude()
                .bg(hsla(p.menu_background))
                .shadow_lg()
                .on_mouse_down_out(on_mouse_down_out)
                .into_any_element(),
        };
        deferred(
            anchored()
                .anchor(menu.request.corner)
                .position(menu.request.position)
                .snap_to_window_with_margin(px(8.0))
                .child(content),
        )
        .with_priority(2)
        .into_any_element()
    }
}
