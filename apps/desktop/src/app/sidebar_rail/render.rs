//! Drawing the rail: a 52pt column at the window's left, Toggle sidebar in the slot at its top
//! (the sidebar's top row's height), the pinned icons, an unpinned item while its view shows
//! (under a dashed line), ⋯, and at its foot the workspace and the account usage toggle.

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
            .child(rail_button_artwork(&self.face, self.color))
    }
}

/// A button's artwork: its image (a link's site icon) or its outline icon.
fn rail_button_artwork(face: &RailButtonFace, color: Hsla) -> AnyElement {
    match &face.image {
        Some(image) => gpui::img(image.clone())
            .size(px(16.0))
            .rounded(px(3.0))
            .into_any_element(),
        None => titlebar_svg_icon(face.icon, face.icon_size, color).into_any_element(),
    }
}

/// The face of rail item `key`.
fn rail_item_face(key: &'static str) -> RailButtonFace {
    RailButtonFace {
        label: rail_item(key).map_or(key, |item| item.label).to_string(),
        icon: rail_item_icon(key),
        icon_size: rail_item_icon_size(key),
        image: None,
    }
}

/// Where the divider beside the docked sidebar starts, from the rail's top: the window's top edge.
/// The desktop's body row starts the rail one work area frame gutter down
/// (`WORK_AREA_FRAME_MARGIN` in helpers/work_area_frame.rs); the web page's rail starts at the top.
#[cfg(not(ghostex_web))]
const RAIL_DIVIDER_TOP: f32 = -crate::app::helpers::WORK_AREA_FRAME_MARGIN;
#[cfg(ghostex_web)]
const RAIL_DIVIDER_TOP: f32 = 0.0;

/// How one button is drawn.
#[derive(Clone, Copy)]
struct RailButtonLook {
    on: bool,
    temporary: bool,
}

impl GhostexGpuiApp {
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

    /// How far right of their Classic place the window's traffic lights sit: the rail's width on
    /// macOS while the rail shows, so they are at the sidebar's top left, not over the rail; 0
    /// elsewhere (Windows and Linux draw no lights on the left).
    ///
    /// CDXC:Sidebar 2026-11-21 DECISION:
    /// User (rail): "please shift the traffic lights to the right so they're top left of the actual sidebar not the rail". The lights keep Classic's inset from the sidebar's left edge, so every row that clears them (the docked and floating sidebar's top row, the hidden sidebar's header) adds this to its reserve while they show (native_sidebar/navigation.rs, render/workarea_header/breadcrumb.rs), and the rail's top slot, which they leave, holds Toggle sidebar. In full screen GPUI puts the lights back at AppKit's place in the title bar that slides down. The floating sidebar's holes follow the buttons' real frames (native/macos/GpuiSidebarReveal.m).
    pub(crate) fn window_lights_rail_shift(&self) -> f32 {
        if cfg!(target_os = "macos") {
            self.sidebar_rail_width()
        } else {
            0.0
        }
    }

    /// Moves the window's traffic lights by `window_lights_rail_shift` when it changes (the rail
    /// shown or hidden; `workspace_traffic_light_position` is their Classic place).
    #[cfg(all(target_os = "macos", not(ghostex_web)))]
    pub(crate) fn sync_window_lights_rail_shift(&mut self, window: &Window) {
        let shift = self.window_lights_rail_shift();
        if self.sidebar_rail.lights_shift == Some(shift) {
            return;
        }
        self.sidebar_rail.lights_shift = Some(shift);
        let mut position = crate::app::helpers::workspace_traffic_light_position();
        position.x += px(shift);
        window.set_traffic_light_position(position);
    }

    /// The work area frame's gutter left of the card: none beside the rail while the sidebar is
    /// hidden (the card starts at the rail's right edge), the frame's gutter otherwise, less the
    /// divider's 1pt that counts toward it while the sidebar is in the row.
    #[cfg(not(ghostex_web))]
    pub(crate) fn work_area_frame_left_gutter(&self, sidebar_in_row: bool) -> f32 {
        use crate::app::consts::SIDEBAR_DIVIDER_WIDTH;
        if sidebar_in_row {
            WORK_AREA_FRAME_MARGIN - SIDEBAR_DIVIDER_WIDTH
        } else if self.sidebar_rail_shown() {
            0.0
        } else {
            WORK_AREA_FRAME_MARGIN
        }
    }

    /// The docked Unified sidebar shows Main Hub's fixed Chief of Staff row above its list
    /// (native_sidebar/chief.rs), so the rail leaves out its own Main Hub button then.
    ///
    /// CDXC:Sidebar 2026-11-21 DECISION:
    /// User (main-hub-rail page): hide the rail's Main Hub button in the Unified sidebar only, where the fixed Chief of Staff row already shows above the list. The Project sidebar keeps it, and so does a hidden or floating sidebar: there it is the only one-click way to Chief of Staff and the only place its status dot shows. Only the rail button goes; the ⋯ menu still lists Main Hub with its pin.
    fn sidebar_rail_main_hub_in_sidebar(&self) -> bool {
        !self.sidebar_collapsed
            && self.native_sidebar.unified.is_some()
            && self
                .native_sidebar
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| {
                    crate::app::native_sidebar::chief_row_view::chief_row_shown(
                        snapshot.chief_enabled,
                        snapshot.bots_mode,
                    )
                })
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
        // CDXC:Sidebar 2026-11-21 DECISION:
        // User: the gap between Toggle sidebar at the rail's top and Search under it was smaller
        // than the gaps between the other rail buttons; make it the same. Toggle sidebar stays on
        // the window lights' line, and the icon column starts one rail pitch (`RAIL_BUTTON` +
        // `RAIL_GAP`, 44pt) below its centre, so Search is as far from it as every icon from the
        // next.
        let toggle_center =
            crate::app::native_sidebar::navigation::corner_row_top_padding(appearance.scale)
                + crate::app::consts::TITLEBAR_CONTROL_HEIGHT * appearance.scale / 2.0;
        let icons_top = toggle_center + RAIL_BUTTON / 2.0 + RAIL_GAP;
        let pins = sidebar_rail_pins();
        let main_hub_in_sidebar = self.sidebar_rail_main_hub_in_sidebar();
        let on_rail = |key: &str| !(main_hub_in_sidebar && key == "mainHub");
        let mut buttons = Vec::new();
        for (index, key) in pins.iter().enumerate() {
            // A pinned link (`link:<id>`): its site icon, its name, its page.
            if let Some(link) = self.sidebar_rail_link_pin(key) {
                let state = RailItemState {
                    available: true,
                    showing: link.showing,
                    badge: None,
                };
                let face = RailButtonFace {
                    label: link.label,
                    icon: "titlebar/world.svg",
                    icon_size: RAIL_ICON,
                    image: link.image,
                };
                buttons.push(self.render_sidebar_rail_button(
                    key.clone(),
                    face,
                    state,
                    RailButtonLook {
                        on: link.showing,
                        temporary: false,
                    },
                    Some(index),
                    &appearance,
                    cx,
                ));
                continue;
            }
            let Some(item) = rail_item(key) else {
                continue;
            };
            let state = self.sidebar_rail_item_state(item.key, cx);
            if !state.available || !on_rail(item.key) {
                continue;
            }
            let look = RailButtonLook {
                on: state.showing,
                temporary: false,
            };
            buttons.push(self.render_sidebar_rail_button(
                item.key.to_string(),
                rail_item_face(item.key),
                state,
                look,
                Some(index),
                &appearance,
                cx,
            ));
        }
        // An unpinned item opened from ⋯ shows here while its page is on screen. The side panel
        // views and the feed are no rail items (CDXC:Sidebar 2026-11-21 on gx-core's `RAIL_ITEMS`),
        // so an open view never shows here.
        let temporary = RAIL_ITEMS.iter().find(|item| {
            !pins.iter().any(|pin| pin == item.key) && on_rail(item.key) && {
                let state = self.sidebar_rail_item_state(item.key, cx);
                state.available && state.showing
            }
        });
        let temporary = temporary.map(|item| {
            let state = self.sidebar_rail_item_state(item.key, cx);
            self.render_sidebar_rail_button(
                item.key.to_string(),
                rail_item_face(item.key),
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
            .bg(sidebar_rail_fill(appearance.glass))
            // Which half has the pointer, for the hidden sidebar's reveal
            // (`sidebar_rail_reveal_hover`); a press starts its wait again.
            .on_mouse_move(
                cx.listener(|app, event: &gpui::MouseMoveEvent, window, cx| {
                    let top_half = event.position.y < window.viewport_size().height / 2.0;
                    app.sidebar_rail_reveal_hover(Some(top_half), cx);
                }),
            )
            .on_hover(cx.listener(|app, hovered: &bool, _, cx| {
                if !*hovered {
                    app.sidebar_rail_reveal_hover(None, cx);
                }
            }))
            .capture_any_mouse_down(cx.listener(|app, _, _, cx| {
                app.sidebar_rail_reveal_hover(None, cx);
            }))
            // CDXC:Sidebar 2026-11-21 DECISION:
            // User: "No right border on the rail: remove the rail's right-side border line, and
            // widen the main area's frame 7px to the left into that space (Rail layout; Classic
            // unchanged)." The rail has no hairline of its own: it shares the sidebar's fill, and
            // with the sidebar hidden the work area card starts at its right edge, taking the
            // frame's gutter and the old hairline (`work_area_frame_left_gutter`). Supersedes the
            // hairline that set the rail apart as a column of its own.
            //
            // CDXC:Sidebar 2026-11-21 DECISION:
            // User: "With the rail on AND the sidebar expanded (docked), draw a thin divider line
            // on the rail's right edge so the rail is separated from the sidebar. It must start at
            // the top of the rail's icon column (below the traffic lights / top bar row) and never
            // overlap the traffic lights; it runs down to the bottom." So the docked sidebar gets
            // the 1px line every other divider draws (`sidebar_divider_line_color`: the faint
            // glass line under glass, the Dividers theme role when set), from under the top row to
            // the rail's foot. With the sidebar hidden or floating the rail still has no line of
            // its own (the decision above).
            //
            // CDXC:Sidebar 2026-11-21 DECISION:
            // User (rail, after the traffic lights moved over the sidebar): the divider between
            // the rail and the docked sidebar reaches the very top of the window. The lights sit
            // at the sidebar's top left now (`window_lights_rail_shift`), so the line no longer
            // stops under the top row: it starts at the window's top edge, above the rail's own
            // top, across the body row's top gutter (`RAIL_DIVIDER_TOP`), beside Toggle sidebar in
            // the rail's top slot. Supersedes the start "below the traffic lights / top bar row"
            // above; hidden or floating, there is still no line.
            .relative()
            // Above the rail the body row's top gutter shows the sidebar's tint (the work area
            // frame's glass ring); the sidebar's colour at the rail's extra cover laid over it
            // makes the same tint as the rail's, so the rail's column runs to the window's top.
            .when(appearance.glass && RAIL_DIVIDER_TOP < 0.0, |rail| {
                rail.child(
                    div()
                        .absolute()
                        .top(px(RAIL_DIVIDER_TOP))
                        .left_0()
                        .right_0()
                        .h(px(-RAIL_DIVIDER_TOP))
                        .bg(rail_glass_gutter_tint(sidebar_glass_tint())),
                )
            })
            .when(!self.sidebar_collapsed, |rail| {
                rail.child(
                    div()
                        .id("ghostex-gpui-sidebar-rail-divider")
                        .absolute()
                        .top(px(RAIL_DIVIDER_TOP))
                        .bottom_0()
                        .right_0()
                        .w(px(1.0))
                        .bg(sidebar_divider_line_color()),
                )
            })
            // The top slot, the sidebar's top row's height: Toggle sidebar on that row's line, in
            // a drag band (`render_sidebar_rail_sidebar_toggle`). The divider runs past it to the
            // window's top.
            .child(crate::app::render::window_drag_region::window_drag_region(
                v_flex()
                    .id("ghostex-gpui-sidebar-rail-top")
                    .w_full()
                    .h(px(icons_top))
                    .flex_shrink_0()
                    .items_center()
                    .pt(px(
                        crate::app::native_sidebar::navigation::corner_row_top_padding(
                            appearance.scale,
                        ),
                    ))
                    .child(self.render_sidebar_rail_sidebar_toggle(&appearance, cx)),
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
    /// rail build brief: "the workspace at the foot"): its icon, plain, one click switches to the
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
        let a11y_menu = open_menu.clone();
        let rail = cx.entity().downgrade();
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
                // CDXC:Workspaces 2026-11-21 DECISION:
                // User: "Just make it like a person icon for personal and the bag icon for work...
                // without the background and color and all of that, it should also be matching the
                // size of the other icons, for example, the accounts icon right there next to it".
                // The rail draws the workspace's glyph (gx-protocol `icon_id`: the person for
                // Personal, the bag for Work, a custom workspace's picked icon or its kind's
                // default) plain, at the rail's icon size in its icon ink, like the account usage
                // switch below it; no disc and no workspace colour. The Classic tile and the menus
                // keep the disc (`workspace_badge`).
                .child(
                    gpui::svg()
                        .path(gpui_sidebar_command_icon_asset_path(Some(&tile.icon)))
                        .size(px(RAIL_ICON))
                        .text_color(appearance.chrome_icon),
                )
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
                // The right-click menu for VoiceOver and automation (AXShowMenu), hung where a
                // right click hangs it (CDXC:Workspaces 2026-11-20 in native_sidebar/workspace_tile.rs).
                .on_a11y_action(
                    gpui::AccessibleAction::ShowContextMenu,
                    move |_, window, cx| {
                        let _ = rail.update(cx, |app, cx| {
                            a11y_menu(app, gpui::point(px(0.0), px(0.0)), window, cx)
                        });
                    },
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
        key: String,
        face: RailButtonFace,
        state: RailItemState,
        look: RailButtonLook,
        pin_index: Option<usize>,
        appearance: &SidebarAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = face.label.clone();
        let tooltip = if look.temporary {
            format!("{label} (not pinned)")
        } else {
            label.clone()
        };
        let color = if look.on {
            appearance.foreground
        } else {
            appearance.chrome_icon
        };
        let hover = appearance.hover;
        let selected = appearance.selected;
        let temporary_border = appearance.muted.opacity(0.55);
        let drop_line = appearance.foreground;
        let bounds = self.sidebar_rail.item_bounds(&key);
        let button = div()
            .id(gpui::SharedString::from(format!(
                "ghostex-gpui-sidebar-rail-{key}"
            )))
            .role(gpui::Role::Button)
            .aria_label(label)
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
            })
            .child(
                div()
                    .size(px(RAIL_ICON))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(look.temporary, |icon| icon.opacity(0.8))
                    .child(rail_button_artwork(&face, color)),
            )
            .when_some(state.badge, |button, badge| button.child(rail_badge(badge)))
            .on_click({
                let key = key.clone();
                cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(item) = rail_item(&key) {
                        app.run_sidebar_rail_item(item.key, window, cx);
                    } else if let Some(id) = ghostex_gx_core::sidebar_rail::rail_link_pin_id(&key) {
                        app.run_sidebar_rail_link(id, window, cx);
                    }
                })
            })
            // Move and unpin save the pins, which the web build cannot (menu.rs), so it offers
            // neither them nor dragging.
            .when(!cfg!(ghostex_web), |button| {
                let key = key.clone();
                button.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let menu = app.sidebar_rail_item_menu(&key);
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
                    face: face.clone(),
                },
                move |dragged, _offset, _window, cx| {
                    cx.new(|_| RailDragPreview {
                        face: dragged.face.clone(),
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
                    app.move_sidebar_rail_pin(&dragged.key, index, cx);
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
        // ⋯ is no anchor of the notifications feed: it wrote the bell's on every frame and won
        // whenever the cached docked sidebar did not repaint (CDXC:Notifications 2026-11-21 on
        // `toggle_gpui_titlebar_notifications_popup_at`).
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
                move |rect, _, _| bounds.set(Some(rect))
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
                    app.move_sidebar_rail_pin(&dragged.key, pin_count, cx);
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
    /// User (rail page): "account usage pinning at the bottom of the sidebar can be a toggle there in that bar at the bottom of it instead of the hidden button we have now". With the rail on, the chart at the rail's foot shows and hides the tiles in one click; the sidebar's hidden handle and its hover peek are not drawn (native_sidebar/usage.rs). Classic keeps the handle.
    ///
    /// CDXC:Sidebar 2026-11-21 DECISION:
    /// User: "make this button for the usages not have a background when it is enabled or disabled. No need for that. Just make it a toggle. Should stay the same look, basically, if it is enabled, if it is pinned or not pinned." The chart looks the same whether the tiles show or not: no fill and the rail's usual icon ink, with the rail's hover fill under the pointer. This supersedes "filled while they show" above; the tooltip still says Show or Hide, and the button still reports its state to screen readers.
    fn render_sidebar_rail_usage_toggle(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = self.sidebar_usage_visible;
        let hover = appearance.hover;
        let tooltip = if on {
            "Hide account usage"
        } else {
            "Show account usage"
        };
        // Hovering peeks the unpinned strip at the sidebar's bottom
        // (`set_sidebar_rail_usage_hovered`). A GPUI element holds one hover listener and the
        // tooltip takes the button's, so the hover lives on this wrapper, as for the sidebar's
        // usage handle (native_sidebar/usage.rs).
        let button = div()
            .id("ghostex-gpui-sidebar-rail-usage")
            .role(gpui::Role::Button)
            .aria_label(tooltip)
            .aria_selected(on)
            .flex_shrink_0()
            .size(px(RAIL_BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RAIL_BUTTON_RADIUS))
            .cursor_default()
            .hover(move |button| button.bg(hover))
            .child(titlebar_svg_icon(
                "titlebar/chart-bar.svg",
                RAIL_ICON,
                appearance.chrome_icon,
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
            });
        div()
            .id("ghostex-gpui-sidebar-rail-usage-hover")
            .mt(px(RAIL_GAP))
            .flex_shrink_0()
            .on_hover(cx.listener(|app, hovered: &bool, _, cx| {
                app.set_sidebar_rail_usage_hovered(*hovered, cx);
            }))
            .child(button)
            .into_any_element()
    }
}

impl GhostexGpuiApp {
    /// Toggle sidebar, in the rail's top slot, above Search.
    ///
    /// CDXC:Sidebar 2026-11-21 DECISION:
    /// User (rail): "please move the toggle sidebar button to the top of the rail. on windows we need to move the toggle sidebar buttons to the rail in this case." With the rail on, Toggle sidebar is the rail's first button on every OS and in every state (sidebar docked, hidden or floating), in the slot the sidebar's top row's height leaves at the rail's top, centred on that row's line (the macOS lights' line); the lights moved to the sidebar's top left (`window_lights_rail_shift`), so the slot is free. The sidebar's top row, docked or floating, Claude's copy of it and the hidden sidebar's header draw no toggle then (native_sidebar/navigation.rs, render/workarea_header/breadcrumb.rs; the web page's header neither). The button is a rail button (the rail's width, the top row's height, the rail's icon size, ink and hover) with the same action, Toggle Sidebar hotkey and tooltip as the header's (`render_sidebar_collapse_button`). This supersedes the 2026-11-20 decision that put it back in the sidebar's top bar (native_sidebar/navigation.rs); Classic is unchanged.
    fn render_sidebar_rail_sidebar_toggle(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = appearance.hover;
        let tooltip = titlebar_tooltip_label("Toggle sidebar", "toggleSidebarCollapsed");
        div()
            .id("ghostex-gpui-sidebar-rail-sidebar-toggle")
            .role(gpui::Role::Button)
            .aria_label("Toggle sidebar")
            .flex_shrink_0()
            .w(px(RAIL_BUTTON))
            .h(px(
                crate::app::consts::TITLEBAR_CONTROL_HEIGHT * appearance.scale
            ))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RAIL_BUTTON_RADIUS))
            .cursor_default()
            .hover(move |button| button.bg(hover))
            .child(titlebar_svg_icon(
                crate::app::consts::TITLEBAR_ICON_LAYOUT_SIDEBAR,
                RAIL_ICON,
                appearance.chrome_icon,
            ))
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                // With Claude on, showing the sidebar leaves Claude (app/claude_web/takeover.rs).
                app.toggle_sidebar_from_user(cx);
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
