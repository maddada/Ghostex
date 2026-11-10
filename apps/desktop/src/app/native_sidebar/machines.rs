use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarMachine, NativeSidebarSnapshot},
};
use crate::{GhostexGpuiApp, app::helpers::*};
use ghostex_gx_core::{
    MachineTabLayout, machine_tab_layout, machine_tab_reveal_offset, machine_tabs_out_of_view,
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::{ElementExt as _, h_flex};
use serde_json::json;

/// A tab's padding, icon and gap, what its label shares the tab with.
const MACHINE_TAB_CHROME: f32 = 12.0 + 14.0 + 4.0;
/// A waiting Ghostex update's glyph and its gap.
const MACHINE_TAB_UPDATE_GLYPH: f32 = 12.0 + 4.0;
const MACHINE_TABS_FADE_WIDTH: f32 = 18.0;
/// How far a tab's status dots hang below it.
const MACHINE_TAB_DOTS_ROOM: f32 = 6.0;

/// How wide one tab is.
#[derive(Clone, Copy)]
enum TabWidth {
    /// Sharing the row with the others, at about this width.
    Share(f32),
    Fixed(f32),
}

/// Where a tab sits in the control: after a divider, and at either rounded end.
#[derive(Clone, Copy)]
struct TabPlace {
    divided: bool,
    first: bool,
    last: bool,
}

impl TabPlace {
    fn new(index: usize, last: usize) -> Self {
        Self {
            divided: index > 0,
            first: index == 0,
            last: index == last,
        }
    }
}

impl TabWidth {
    fn px(self) -> f32 {
        match self {
            Self::Share(width) | Self::Fixed(width) => width,
        }
    }
}

impl GhostexGpuiApp {
    /// The machine tabs: see `machine_tab_layout` in gx-core for how they share the row
    /// (CDXC:RemoteMachines 2026-10-31 there). The Unified sidebar draws no tabs; its filter lists
    /// the machines.
    pub(crate) fn render_native_machine_tabs(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let count = snapshot.machines.len();
        if count <= 1 {
            return None;
        }
        let scale = appearance.scale;
        /*
        CDXC:RemoteMachines 2026-11-11 DECISION:
        User (sidebar top page, Q2423 B): the computers are one rounded control, a joined segmented control 28 high with corner 7 between the top rows' 8pt edges, not flat tabs across the sidebar's full width. The open computer's segment is filled like a selected Space tile; another computer's status dots still hang below its segment.
        */
        let edge = super::navigation::SIDEBAR_TOP_EDGE_INSET * scale;
        let radius = super::navigation::SIDEBAR_TOP_ROW_BUTTON_RADIUS * scale;
        // The control's width inside its outline, which its segments share.
        let inner_width = (self.sidebar_width - edge * 2.0 - 2.0).max(0.0);
        let control = h_flex()
            .size_full()
            .rounded(px(radius))
            .border_1()
            .border_color(appearance.foreground.opacity(0.08));
        let wrap = |control: gpui::Div| {
            div()
                .w_full()
                .h(px(super::navigation::SIDEBAR_TOP_ROW_BUTTON_HEIGHT * scale))
                .px(px(edge))
                .child(control)
                .into_any_element()
        };
        let last = count - 1;
        let MachineTabLayout::Pinned {
            pinned_width,
            viewport,
            tab_width,
            scrolls,
        } = machine_tab_layout(count, inner_width, scale)
        else {
            let share = inner_width / count as f32;
            return Some(wrap(
                control.children(
                    snapshot
                        .machines
                        .iter()
                        .enumerate()
                        .map(|(index, machine)| {
                            self.render_native_machine_tab(
                                snapshot,
                                machine,
                                TabPlace::new(index, last),
                                TabWidth::Share(share),
                                appearance,
                                window,
                                cx,
                            )
                        }),
                ),
            ));
        };
        // This computer stays at the left whatever the others do.
        let pinned_index = snapshot
            .machines
            .iter()
            .position(|machine| machine.id == ghostex_gx_core::LOCAL_MACHINE_ID)
            .unwrap_or(0);
        let others: Vec<_> = snapshot
            .machines
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != pinned_index)
            .map(|(_, machine)| machine)
            .collect();
        let handle = self.native_sidebar.machine_tabs_scroll.clone();
        let mut offset = -f32::from(handle.offset().x);
        let selected = others
            .iter()
            .position(|machine| machine.id == snapshot.selected_machine_id);
        let reveal_key = (snapshot.selected_machine_id.clone(), viewport as u32);
        if self.native_sidebar.machine_tabs_revealed.borrow().as_ref() != Some(&reveal_key) {
            *self.native_sidebar.machine_tabs_revealed.borrow_mut() = Some(reveal_key);
            if let Some(index) = selected {
                offset =
                    machine_tab_reveal_offset(index, others.len(), tab_width, viewport, offset);
                handle.set_offset(gpui::point(px(-offset), px(0.0)));
            }
        }
        let (before, after) = machine_tabs_out_of_view(others.len(), tab_width, viewport, offset);
        let edge_dots = |range: std::ops::Range<usize>, group: &'static str| {
            let hidden = &others[range];
            let sum = |count: fn(&NativeSidebarMachine) -> usize| -> usize {
                hidden
                    .iter()
                    .filter(|machine| machine.id != snapshot.selected_machine_id)
                    .map(|machine| count(machine))
                    .sum()
            };
            let (working, attention, background, question, approval) = (
                sum(|machine| machine.working_count),
                sum(|machine| machine.attention_count),
                sum(|machine| machine.background_work_count),
                sum(|machine| machine.question_count),
                sum(|machine| machine.approval_count),
            );
            (working + attention + background + question + approval > 0).then(|| {
                super::status::status_dot_stack(
                    working,
                    attention,
                    background,
                    question,
                    approval,
                    scale,
                    super::status::DotBackdrop::new(appearance, None, group.to_string()),
                )
            })
        };
        let dots_before = edge_dots(before, "native-machine-edge-before");
        let dots_after = edge_dots(after, "native-machine-edge-after");
        let content = others.len() as f32 * tab_width;
        let fade = |to_right: bool| {
            let color = crate::app::helpers::sidebar_chrome_gradient_top_color();
            let (from, to) = (color.opacity(0.0), color);
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(MACHINE_TABS_FADE_WIDTH * scale))
                .bg(gpui::linear_gradient(
                    if to_right { 90.0 } else { 270.0 },
                    gpui::linear_color_stop(from, 0.0),
                    gpui::linear_color_stop(to, 1.0),
                ))
        };
        let glass = appearance.glass;
        // The control's height inside its outline.
        let tab_height = super::navigation::SIDEBAR_TOP_ROW_BUTTON_HEIGHT * scale - 2.0;
        let others_last = others.len().saturating_sub(1);
        Some(wrap(
            control
                .child(self.render_native_machine_tab(
                    snapshot,
                    &snapshot.machines[pinned_index],
                    TabPlace::new(0, count),
                    TabWidth::Fixed(pinned_width),
                    appearance,
                    window,
                    cx,
                ))
                .child(
                    div()
                        .relative()
                        .flex_shrink_0()
                        .w(px(viewport))
                        .h_full()
                        .child(
                            h_flex()
                                .id("native-sidebar-machine-scroll")
                                .role(gpui::Role::TabList)
                                .w_full()
                                // Taller than the row: a scroller clips both ways, and a tab's status
                                // dots hang just below it.
                                .h(px(tab_height + MACHINE_TAB_DOTS_ROOM * scale))
                                .items_start()
                                .overflow_x_scroll()
                                .track_scroll(&handle)
                                .children(others.iter().enumerate().map(|(index, machine)| {
                                    div().flex_shrink_0().h(px(tab_height)).child(
                                        self.render_native_machine_tab(
                                            snapshot,
                                            machine,
                                            TabPlace {
                                                divided: true,
                                                first: false,
                                                last: index == others_last,
                                            },
                                            TabWidth::Fixed(tab_width),
                                            appearance,
                                            window,
                                            cx,
                                        ),
                                    )
                                })),
                        )
                        // A fade can only ramp into a solid colour, so window glass goes without, as
                        // the list's own does (scroll_fade.rs); the peeking tab still says it scrolls.
                        .when(scrolls && !glass && offset > 0.5, |clip| {
                            clip.child(fade(false).left_0())
                        })
                        .when(
                            scrolls && !glass && offset < content - viewport - 0.5,
                            |clip| clip.child(fade(true).right_0()),
                        )
                        // What the tabs out of view have to say shows on the edge they are past.
                        .children(dots_before.map(|dots| {
                            div()
                                .absolute()
                                .left(px(3.0 * scale))
                                .bottom(px(-4.0 * scale))
                                .child(dots)
                        }))
                        .children(dots_after.map(|dots| {
                            div()
                                .absolute()
                                .right(px(3.0 * scale))
                                .bottom(px(-4.0 * scale))
                                .child(dots)
                        })),
                ),
        ))
    }

    /// One machine's tab.
    #[allow(clippy::too_many_arguments)]
    fn render_native_machine_tab(
        &self,
        snapshot: &NativeSidebarSnapshot,
        machine: &NativeSidebarMachine,
        place: TabPlace,
        width: TabWidth,
        appearance: &SidebarAppearance,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let scale = appearance.scale;
        let id = machine.id.clone();
        let reconnect = id.clone();
        let owner = super::menus::MenuOwner::default();
        let menu_owner = owner.clone();
        let tab_menu = (id != "local").then(|| self.native_machine_tab_menu(&id));
        let selected = snapshot.selected_machine_id == id;
        let dot_group = format!("native-machine-dots-{id}");
        let busy = ghostex_gx_core::machine_state_is_busy(&machine.state);
        let failed = id != "local" && ghostex_gx_core::machine_state_is_failure(&machine.state);
        // CDXC:RemoteMachines 2026-10-16 DECISION: the machine's Ghostex update shows on its tab (an arrow, a spinner while it installs) and as the tab menu's first item (remote_conn/app_update.rs).
        let app_update = (id != "local")
            .then(|| self.remote_app_update_tab_badge(&id))
            .flatten();
        // What the label has to share the tab with: the padding, the icon, a waiting update's glyph.
        let label_room = width.px()
            - (MACHINE_TAB_CHROME
                + if app_update.is_some() {
                    MACHINE_TAB_UPDATE_GLYPH
                } else {
                    0.0
                })
                * scale;
        let cut = super::tooltips::text_width(window, &machine.label, 12.0 * scale) > label_room;
        // CDXC:Sidebar 2026-10-29 DECISION: User: "remove this tooltip in the sidebar" (hovering a machine tab showed only its own label). The tab shows a tooltip only when it says more than the label: the machine's error, or a waiting Ghostex update.
        // CDXC:RemoteMachines 2026-10-31 DECISION: User: a tooltip with the full name when a label is cut. That is still more than the tab says.
        let tooltip = (machine.message.is_some() || app_update.is_some() || cut).then(|| {
            let tooltip = machine
                .message
                .as_ref()
                .map(|message| format!("{}: {message}", machine.label))
                .unwrap_or_else(|| machine.label.clone());
            match app_update.as_ref() {
                Some(update) => format!("{tooltip} · {}", update.tooltip),
                None => tooltip,
            }
        });
        let update_glyph = app_update.as_ref().map(|update| {
            let icon = gpui::svg()
                .path(if update.busy {
                    "titlebar/loader2.svg"
                } else {
                    "titlebar/download.svg"
                })
                .size(px(12.0 * scale))
                .flex_shrink_0()
                .text_color(appearance.foreground);
            if update.busy {
                icon.with_throttled_animation(
                    format!("native-machine-update-{id}"),
                    std::time::Duration::from_millis(900),
                    |icon, progress| {
                        icon.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                            progress,
                        )))
                    },
                )
                .into_any_element()
            } else {
                icon.into_any_element()
            }
        });
        let icon = gpui::svg()
            .path(if id == "local" {
                "titlebar/device-desktop.svg"
            } else if busy {
                "titlebar/loader2.svg"
            } else {
                "titlebar/cloud.svg"
            })
            .size(px(14.0 * scale))
            .text_color(if failed {
                rgb(0xff9494).into()
            } else {
                appearance.chrome_icon
            });
        let glyph = if busy {
            icon.with_throttled_animation(
                format!("native-machine-busy-{id}"),
                std::time::Duration::from_millis(900),
                |icon, progress| {
                    icon.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                        progress,
                    )))
                },
            )
            .into_any_element()
        } else {
            icon.into_any_element()
        };
        let stop_key = format!("machine-tab:{id}");
        h_flex()
            .id(format!("native-sidebar-machine-{id}"))
            .relative()
            .group(dot_group.clone())
            .role(gpui::Role::Tab)
            .aria_label(machine.label.clone())
            .aria_selected(selected)
            .map(|tab| match width {
                TabWidth::Share(_) => tab.flex_1().min_w_0(),
                TabWidth::Fixed(width) => tab.w(px(width)).flex_shrink_0(),
            })
            .h_full()
            .px(px(6.0 * scale))
            .justify_center()
            .gap(px(4.0 * scale))
            .text_size(px(12.0 * scale))
            .text_color(appearance.chrome_label)
            .on_prepaint(super::menus::record_menu_owner(&owner))
            .when(place.divided, |row| {
                row.border_l_1()
                    .border_color(appearance.foreground.opacity(0.08))
            })
            // Inside the control's corner 7 and its 1pt outline.
            .when(place.first, |row| {
                row.rounded_l(px(
                    (super::navigation::SIDEBAR_TOP_ROW_BUTTON_RADIUS - 1.0) * scale
                ))
            })
            .when(place.last, |row| {
                row.rounded_r(px(
                    (super::navigation::SIDEBAR_TOP_ROW_BUTTON_RADIUS - 1.0) * scale
                ))
            })
            .when(selected, |row| {
                row.bg(super::selectors::space_tile_selected_colors(appearance).0)
            })
            .hover(|row| row.bg(appearance.hover))
            .child(
                div()
                    .id(format!("native-machine-connect-{id}"))
                    .flex_shrink_0()
                    .child(glyph)
                    .when(
                        id != "local" && machine.state != "connected" && !busy,
                        |icon| {
                            icon.on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.remote_reconnect_from_sidebar(&reconnect, cx);
                            }))
                        },
                    ),
            )
            .child(div().min_w_0().truncate().child(machine.label.clone()))
            .children(update_glyph)
            /*
            CDXC:Sidebar 2026-09-25 DECISION:
            User: stop showing the status on the machine we're on and show it on the other machine tabs as dots, not numbers, exactly like the Spaces. The selected tab's sessions are already listed below it.
            CDXC:Sidebar 2026-10-24 DECISION:
            User: "when we show those colored circles on remote machines let's make those circles appear below the remote machine tab instead of showing in a way that pushes the icon and name of the remote/local machine". The dots are drawn absolutely at the bottom of the tab, centred, so the icon and name stay centred where they sit without dots and the tab does not grow; they are visual only. Supersedes the dots sitting inline after the name.
            */
            .when(
                !selected
                    && (machine.working_count > 0
                        || machine.attention_count > 0
                        || machine.background_work_count > 0
                        || machine.question_count > 0
                        || machine.approval_count > 0),
                |row| {
                    row.child(
                        h_flex()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(px(-4.0 * scale))
                            .justify_center()
                            .child(super::status::status_dot_stack(
                                machine.working_count,
                                machine.attention_count,
                                machine.background_work_count,
                                machine.question_count,
                                machine.approval_count,
                                scale,
                                super::status::DotBackdrop::new(
                                    appearance,
                                    None,
                                    dot_group.clone(),
                                ),
                            )),
                    )
                },
            )
            .when_some(
                tooltip.filter(|_| {
                    self.native_sidebar.pointer_inside
                        && self.native_sidebar.menu.is_none()
                        && !cx.has_active_drag()
                }),
                |row, tooltip| {
                    row.tooltip_show_delay(appearance.tooltip_delay)
                        .tooltip(move |window, cx| {
                            super::tooltips::sidebar_style_tooltip(tooltip.clone(), window, cx)
                        })
                },
            )
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "selectMachine", "machineId": id}),
                    cx,
                );
            }))
            .when_some(tab_menu, |row, items| {
                row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    cx.stop_propagation();
                    Self::show_native_sidebar_context_menu(
                        &items,
                        menu_owner.get(),
                        event.position,
                        scale,
                        window,
                        cx,
                    );
                })
            })
            .map(|row| {
                self.sidebar_focus_stop(
                    row,
                    stop_key,
                    super::focus_order::SidebarStopPlace::Fixed,
                    appearance,
                    cx,
                )
            })
            .into_any_element()
    }
}

impl GhostexGpuiApp {
    /// A remote machine tab's right-click menu, which the Unified filter dropdown's machine rows open
    /// too. This computer's tab has none.
    pub(crate) fn native_machine_tab_menu(&self, machine_id: &str) -> serde_json::Value {
        let mut items = Vec::new();
        if let Some(label) = self
            .remote_app_update_tab_badge(machine_id)
            .and_then(|update| update.menu_label)
        {
            items.push(json!({"label": label, "icon": "download", "command": {"type": "remoteAppUpdate", "op": "install", "remoteMachineId": machine_id}}));
            items.push(json!({"separator": true}));
        }
        items.push(json!({"label": "Hide Machine", "icon": "eye-off", "command": {"type": "machineAction", "action": "hide", "machineId": machine_id}}));
        // The tab's workspace on this computer (gx-core `machine_workspace_menu`).
        if let Some(workspace_menu) = self.gx_store_machine_workspace_menu(machine_id) {
            items.push(workspace_menu);
        }
        items.push(json!({"separator": true}));
        items.push(json!({"label": "Configure Machines", "icon": "settings", "command": {"type": "machineAction", "action": "configure", "machineId": machine_id}}));
        serde_json::Value::Array(items)
    }
}
