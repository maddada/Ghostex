//! Drawing the rail: a 40pt column at the window's left, the room under the window buttons at its
//! top, the pinned icons, an unpinned item while its view shows (under a dashed line), ⋯, and at
//! its foot the account usage toggle.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Window, div, px,
};
use gpui_component::ElementExt as _;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::v_flex;

use ghostex_gx_core::sidebar_rail::{RAIL_ITEMS, rail_item};

use super::model::*;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::native_sidebar::appearance::SidebarAppearance;

impl Render for RailDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size(px(RAIL_BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RAIL_BUTTON_RADIUS))
            .bg(self.fill)
            .border_1()
            .border_color(self.border)
            .child(titlebar_svg_icon(self.icon, self.icon_size, self.color))
    }
}

/// How one button is drawn.
#[derive(Clone, Copy)]
struct RailButtonLook {
    on: bool,
    temporary: bool,
}

impl GhostexGpuiApp {
    /// Where the notifications feed hangs from (the sidebar's bell's anchor); the web build has no
    /// feed.
    fn sidebar_rail_bell_anchor(&self) -> Option<super::RailBounds> {
        #[cfg(not(ghostex_web))]
        return Some(self.titlebar_notification_bell_bounds.clone());
        #[cfg(ghostex_web)]
        None
    }

    /// Whether this window draws the rail.
    pub(crate) fn sidebar_rail_shown(&self) -> bool {
        sidebar_rail_on()
    }

    /// The rail's width in this window: 0 without it.
    pub(crate) fn sidebar_rail_width(&self) -> f32 {
        if self.sidebar_rail_shown() {
            RAIL_WIDTH
        } else {
            0.0
        }
    }

    /// The rail column, left of the sidebar, from the window's top to its bottom.
    pub(crate) fn render_sidebar_rail(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let appearance = match self.native_sidebar.snapshot.as_ref() {
            Some(snapshot) => SidebarAppearance::from_hud(&snapshot.hud, window),
            None => {
                return div()
                    .w(px(RAIL_WIDTH))
                    .h_full()
                    .flex_shrink_0()
                    .into_any_element();
            }
        };
        // The room under the window buttons: the sidebar's top row, so the first icon starts level
        // with what is under that row.
        let top_inset =
            crate::app::native_sidebar::navigation::sidebar_corner_row_height(appearance.scale);
        let pins = sidebar_rail_pins();
        let mut buttons = Vec::new();
        for (index, key) in pins.iter().copied().enumerate() {
            let state = self.sidebar_rail_item_state(key, cx);
            if !state.available {
                continue;
            }
            let look = RailButtonLook {
                on: state.showing,
                temporary: false,
            };
            buttons.push(self.render_sidebar_rail_button(
                key,
                state,
                look,
                Some(index),
                &appearance,
                cx,
            ));
        }
        // An unpinned item opened from ⋯ shows here while its view or page is on screen.
        let temporary = RAIL_ITEMS.iter().find(|item| {
            !pins.contains(&item.key) && {
                let state = self.sidebar_rail_item_state(item.key, cx);
                state.available && state.showing
            }
        });
        let temporary = temporary.map(|item| {
            let state = self.sidebar_rail_item_state(item.key, cx);
            self.render_sidebar_rail_button(
                item.key,
                state,
                RailButtonLook {
                    on: true,
                    temporary: true,
                },
                None,
                &appearance,
                cx,
            )
        });
        let more = self.render_sidebar_rail_more_button(pins.len(), &appearance, cx);
        let workspace = self.render_sidebar_rail_workspace(&appearance, cx);
        let usage = (!self.account_usage_meters().is_empty())
            .then(|| self.render_sidebar_rail_usage_toggle(&appearance, cx));
        let scale = 1.0;
        v_flex()
            .id("ghostex-gpui-sidebar-rail")
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_shrink_0()
            .items_center()
            .pb(px(8.0))
            .bg(sidebar_chrome_fill(appearance.glass, 180.0))
            // A hairline on its right edge, so the rail reads as a column of its own beside the
            // sidebar and beside the work area when the sidebar is hidden.
            .border_r_1()
            .border_color(appearance.foreground.opacity(0.06))
            .child(crate::app::render::window_drag_region::window_drag_region(
                div()
                    .id("ghostex-gpui-sidebar-rail-top")
                    .w_full()
                    .h(px(top_inset))
                    .flex_shrink_0(),
            ))
            .child(
                v_flex()
                    .id("ghostex-gpui-sidebar-rail-icons")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .items_center()
                    .gap(px(RAIL_GAP * scale))
                    .overflow_y_scroll()
                    .children(buttons)
                    .when_some(temporary, |column, temporary| {
                        column
                            .child(rail_dashed_line(appearance.muted))
                            .child(temporary)
                    })
                    .child(more),
            )
            .children(workspace)
            .children(usage)
            .into_any_element()
    }

    /// The workspace at the rail's foot, where the sidebar's Spaces row drew it (Q4296 F, and the
    /// rail build brief: "the workspace at the foot"): the same disc, one click switches to the
    /// other workspace, a right click opens the workspace menu. Shown with two or more workspaces.
    fn render_sidebar_rail_workspace(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // As on the sidebar's tile: the workspace belongs to this computer, so not on another
        // computer's tab (native_sidebar/workspace_tile.rs).
        let local = self
            .native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.selected_machine_id == ghostex_gx_core::LOCAL_MACHINE_ID
            });
        if !local {
            return None;
        }
        let tile = self.gx_store_workspace_tile()?;
        let color = crate::app::native_sidebar::workspace_tile::workspace_color(
            &tile.color,
            appearance.muted,
        );
        let tooltip = match &tile.switch_to {
            Some((_, name)) => match tile.switch_hotkey.as_deref() {
                Some(hotkey) => format!("{}: click for {name} {hotkey}", tile.name),
                None => format!("{}: click for {name}", tile.name),
            },
            None => tile.name.clone(),
        };
        let switch_to = tile.switch_to.map(|(workspace_id, _)| workspace_id);
        let bounds = self.sidebar_rail.item_bounds("workspace");
        let hover = appearance.hover;
        let open_menu = {
            let bounds = bounds.clone();
            move |app: &mut Self,
                  pointer: gpui::Point<gpui::Pixels>,
                  window: &mut Window,
                  cx: &mut Context<Self>| {
                let Some(menu) = app.gx_store_workspace_menu(cx) else {
                    return;
                };
                let at = bounds
                    .get()
                    .map_or(pointer, |b| gpui::point(b.right() + px(4.0), b.top()));
                Self::show_native_sidebar_menu(&menu, at, 1.0, window, cx);
            }
        };
        let click_menu = open_menu.clone();
        Some(
            div()
                .id("ghostex-gpui-sidebar-rail-workspace")
                .role(gpui::Role::Button)
                .aria_label(format!("Workspace {}", tile.name))
                .mt(px(RAIL_GAP))
                .flex_shrink_0()
                .size(px(RAIL_BUTTON))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(RAIL_BUTTON_RADIUS))
                .cursor_pointer()
                .hover(move |button| button.bg(hover))
                .on_prepaint(move |rect, _, _| bounds.set(Some(rect)))
                .child(crate::app::native_sidebar::workspace_tile::workspace_badge(
                    &tile.icon, color, RAIL_ICON,
                ))
                .on_click(
                    cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        match switch_to.clone() {
                            Some(workspace_id) => app.gx_store_select_workspace(workspace_id, cx),
                            None => click_menu(app, event.position(), window, cx),
                        }
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        open_menu(app, event.position, window, cx);
                    }),
                )
                .managed_tooltip_with_placement(
                    ManagedTooltipPlacement::Right,
                    move |window, cx| {
                        crate::app::native_sidebar::link_chips::sidebar_style_tooltip(
                            tooltip.clone(),
                            window,
                            cx,
                        )
                    },
                )
                .into_any_element(),
        )
    }

    fn render_sidebar_rail_button(
        &self,
        key: &'static str,
        state: RailItemState,
        look: RailButtonLook,
        pin_index: Option<usize>,
        appearance: &SidebarAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = rail_item(key).map_or(key, |item| item.label);
        let tooltip = if look.temporary {
            format!("{label} (not pinned)")
        } else {
            label.to_string()
        };
        let icon = rail_item_icon(key);
        let icon_size = rail_item_icon_size(key);
        let color = if look.on {
            appearance.foreground
        } else {
            appearance.chrome_icon
        };
        let hover = appearance.hover;
        let selected = appearance.selected;
        let temporary_border = appearance.muted.opacity(0.55);
        let drop_line = appearance.foreground;
        let bounds = self.sidebar_rail.item_bounds(key);
        // The notifications feed hangs beside the rail's bell, which stands in for the sidebar's.
        let bell_anchor = (key == "notifications")
            .then(|| self.sidebar_rail_bell_anchor())
            .flatten();
        let button = div()
            .id(gpui::SharedString::from(format!(
                "ghostex-gpui-sidebar-rail-{key}"
            )))
            .role(gpui::Role::Button)
            .aria_label(label.to_string())
            .aria_selected(look.on)
            .relative()
            .flex_shrink_0()
            .size(px(RAIL_BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RAIL_BUTTON_RADIUS))
            .cursor_default()
            .when(look.on && !look.temporary, move |button| {
                button.bg(selected)
            })
            .when(look.temporary, move |button| {
                button.border_1().border_color(temporary_border)
            })
            .hover(move |button| button.bg(if look.on { selected } else { hover }))
            .on_prepaint(move |rect, _, _| {
                bounds.set(Some(rect));
                if let Some(anchor) = &bell_anchor {
                    anchor.set(Some(rail_popup_anchor(rect)));
                }
            })
            .child(
                div()
                    .size(px(RAIL_ICON))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(look.temporary, |icon| icon.opacity(0.8))
                    .child(titlebar_svg_icon(icon, icon_size, color)),
            )
            .when_some(state.badge, |button, badge| button.child(rail_badge(badge)))
            .on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                app.run_sidebar_rail_item(key, window, cx);
            }))
            // Move and unpin save the pins, which the web build cannot (menu.rs), so it offers
            // neither them nor dragging.
            .when(!cfg!(ghostex_web), |button| {
                button.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let menu = app.sidebar_rail_item_menu(key);
                        Self::show_native_sidebar_menu(&menu, event.position, 1.0, window, cx);
                    }),
                )
            })
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                crate::app::native_sidebar::link_chips::sidebar_style_tooltip(
                    tooltip.clone(),
                    window,
                    cx,
                )
            });
        let Some(index) = pin_index.filter(|_| !cfg!(ghostex_web)) else {
            return button.into_any_element();
        };
        let preview_fill = appearance.selected;
        let preview_border = appearance.muted.opacity(0.4);
        button
            .on_drag(
                DraggedRailItem {
                    key,
                    icon,
                    icon_size,
                },
                move |dragged, _offset, _window, cx| {
                    cx.new(|_| RailDragPreview {
                        icon: dragged.icon,
                        icon_size: dragged.icon_size,
                        fill: preview_fill,
                        border: preview_border,
                        color,
                    })
                },
            )
            .drag_over::<DraggedRailItem>(move |style, _, _, _| {
                style.border_t_2().border_color(drop_line)
            })
            .on_drop(
                cx.listener(move |app, dragged: &DraggedRailItem, _window, cx| {
                    app.move_sidebar_rail_pin(dragged.key, index, cx);
                }),
            )
            .into_any_element()
    }

    /// ⋯, which also takes a dragged pin dropped on it: after the last pin.
    fn render_sidebar_rail_more_button(
        &self,
        pin_count: usize,
        appearance: &SidebarAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bounds = self.sidebar_rail.more_bounds.clone();
        let hover = appearance.hover;
        let drop_line = appearance.foreground;
        // Without the bell on the rail, the feed (from ⋯ or its hotkey) hangs beside ⋯.
        let bell_on_rail = sidebar_rail_pins().contains(&"notifications");
        let bell_anchor = (!bell_on_rail)
            .then(|| self.sidebar_rail_bell_anchor())
            .flatten();
        div()
            .id("ghostex-gpui-sidebar-rail-more")
            .role(gpui::Role::Button)
            .aria_label("More")
            .flex_shrink_0()
            .size(px(RAIL_BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RAIL_BUTTON_RADIUS))
            .cursor_default()
            .hover(move |button| button.bg(hover))
            .on_prepaint({
                let bounds = bounds.clone();
                move |rect, _, _| {
                    bounds.set(Some(rect));
                    if let Some(anchor) = &bell_anchor {
                        anchor.set(Some(rail_popup_anchor(rect)));
                    }
                }
            })
            .child(titlebar_svg_icon(
                "titlebar/dots.svg",
                RAIL_ICON,
                appearance.chrome_icon,
            ))
            .on_click(
                cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    app.open_sidebar_rail_more_menu(
                        bounds.get(),
                        event.position(),
                        1.0,
                        window,
                        cx,
                    );
                }),
            )
            .drag_over::<DraggedRailItem>(move |style, _, _, _| {
                style.border_t_2().border_color(drop_line)
            })
            .on_drop(
                cx.listener(move |app, dragged: &DraggedRailItem, _window, cx| {
                    app.move_sidebar_rail_pin(dragged.key, pin_count, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, |window, cx| {
                crate::app::native_sidebar::link_chips::sidebar_style_tooltip(
                    "More: pin anything to the rail".to_string(),
                    window,
                    cx,
                )
            })
            .into_any_element()
    }

    /// The account usage tiles' switch: one click shows or hides them at the sidebar's foot.
    ///
    /// CDXC:Sidebar 2026-11-20 DECISION:
    /// User (rail page): "account usage pinning at the bottom of the sidebar can be a toggle there in that bar at the bottom of it instead of the hidden button we have now". With the rail on, the chart at the rail's foot shows and hides the tiles in one click and is filled while they show; the sidebar's hidden handle and its hover peek are not drawn (native_sidebar/usage.rs). Classic keeps the handle.
    fn render_sidebar_rail_usage_toggle(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = self.sidebar_usage_visible;
        let hover = appearance.hover;
        let selected = appearance.selected;
        let tooltip = if on {
            "Hide account usage"
        } else {
            "Show account usage"
        };
        div()
            .id("ghostex-gpui-sidebar-rail-usage")
            .role(gpui::Role::Button)
            .aria_label(tooltip)
            .aria_selected(on)
            .mt(px(RAIL_GAP))
            .flex_shrink_0()
            .size(px(RAIL_BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RAIL_BUTTON_RADIUS))
            .cursor_default()
            .when(on, move |button| button.bg(selected))
            .hover(move |button| button.bg(if on { selected } else { hover }))
            .child(titlebar_svg_icon(
                "titlebar/chart-bar.svg",
                RAIL_ICON,
                if on {
                    appearance.foreground
                } else {
                    appearance.chrome_icon
                },
            ))
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                app.toggle_native_sidebar_usage_visible(cx);
            }))
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                crate::app::native_sidebar::link_chips::sidebar_style_tooltip(
                    tooltip.to_string(),
                    window,
                    cx,
                )
            })
            .into_any_element()
    }
}

/// A popup anchor just right of a rail button, at its top: a popover opens there beside the rail,
/// as from a context menu.
fn rail_popup_anchor(button: gpui::Bounds<gpui::Pixels>) -> gpui::Bounds<gpui::Pixels> {
    gpui::Bounds {
        origin: gpui::point(button.right() + px(4.0), button.top()),
        size: gpui::size(px(0.0), px(0.0)),
    }
}

/// The dashed line above an unpinned item shown for now: short dashes in the muted ink.
fn rail_dashed_line(color: Hsla) -> AnyElement {
    gpui::div()
        .flex()
        .flex_shrink_0()
        .my(px(3.0))
        .gap(px(2.0))
        .children((0..5).map(move |_| div().w(px(2.0)).h(px(1.0)).bg(color.opacity(0.7))))
        .into_any_element()
}

fn rail_badge(badge: RailBadge) -> AnyElement {
    match badge {
        RailBadge::Count(count, color) => div()
            .absolute()
            .top(px(2.0))
            .right(px(1.0))
            .min_w(px(14.0))
            .h(px(14.0))
            .px(px(3.0))
            .rounded_full()
            .bg(color)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(9.5))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(gpui::white())
            .child(if count > 99 {
                "99+".to_string()
            } else {
                count.to_string()
            })
            .into_any_element(),
        RailBadge::Dot(color) => div()
            .absolute()
            .right(px(4.0))
            .bottom(px(4.0))
            .size(px(7.0))
            .rounded_full()
            .bg(color)
            .into_any_element(),
    }
}
