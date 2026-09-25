use super::{
    actions::NativeSidebarAction,
    menu_state::{SidebarMenuPanel, SidebarMenuPlacement, SidebarMenuState},
};
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Bounds, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Pixels, Point, StatefulInteractiveElement, Styled, Window, deferred, div, px,
};
use gpui_component::{Root, h_flex, v_flex};
use serde_json::Value;

/// The gap between a trigger button and the menu that drops down from it.
/// How long after the More menu dismisses itself a press on its button still counts as that press.
const MORE_MENU_SAME_PRESS: std::time::Duration = std::time::Duration::from_millis(250);

impl GhostexGpuiApp {
    pub(crate) fn show_native_sidebar_menu(
        items: &Value,
        position: Point<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        Self::open_native_sidebar_menu(
            items,
            position,
            SidebarMenuPlacement::Pointer,
            scale,
            window,
            cx,
        );
    }

    /// CDXC:Sidebar 2026-09-21 DECISION:
    /// User: clicking the sidebar More button again closes that menu. Reading whether the menu is still open cannot toggle: the overlay and the sidebar root both dismiss on the outside mouse down (the button sits above the panel), then the button's click would open a fresh one. The dismissal records the time, and a press on this button within 250ms is the closing half of that press. This supersedes closing only when the menu was still open.
    pub(crate) fn toggle_native_sidebar_more_menu(
        &mut self,
        items: &Value,
        trigger: Bounds<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .native_sidebar
            .menu
            .as_ref()
            .is_some_and(SidebarMenuState::dropped_from_trigger)
        {
            self.native_sidebar.more_menu_dismissed_at = None;
            self.close_native_sidebar_menu(window, cx);
            return;
        }
        if self
            .native_sidebar
            .more_menu_dismissed_at
            .take()
            .is_some_and(|at| at.elapsed() < MORE_MENU_SAME_PRESS)
        {
            return;
        }
        Self::show_native_sidebar_menu_below(items, trigger, scale, window, cx);
    }

    /// CDXC:Sidebar 2026-09-21 DECISION:
    /// User: the sidebar menu drops down from its trigger button rather than opening where the pointer was: its right edge lines up with the button's right edge, and it sits below the button with a gap (4px since 2026-09-25, `HEADER_MENU_TRIGGER_GAP`).
    pub(crate) fn show_native_sidebar_menu_below(
        items: &Value,
        trigger: Bounds<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        Self::open_native_sidebar_menu(
            items,
            Point::new(
                trigger.right(),
                trigger.bottom() + px(crate::app::consts::HEADER_MENU_TRIGGER_GAP * scale),
            ),
            SidebarMenuPlacement::BelowTrigger,
            scale,
            window,
            cx,
        );
    }

    fn open_native_sidebar_menu(
        items: &Value,
        position: Point<Pixels>,
        placement: SidebarMenuPlacement,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let Some(mut items) = items.as_array().filter(|items| !items.is_empty()).cloned() else {
            return;
        };
        let app = window.root::<Root>().flatten().and_then(|root| {
            root.read(cx)
                .view()
                .clone()
                .downcast::<GhostexGpuiApp>()
                .ok()
        });
        // A menu opened from the floating panel belongs to the same app entity; the panel's own
        // window root is what the lookup above cannot see. Every platform floats now, so this is
        // no longer a macOS-only branch.
        let app = app.or_else(|| {
            window
                .root::<Root>()
                .flatten()
                .and_then(|root| {
                    root.read(cx)
                        .view()
                        .clone()
                        .downcast::<crate::app::floating_reveal::window::FloatingRevealWindow>()
                        .ok()
                })
                .and_then(|view| view.read(cx).app.upgrade())
        });
        let Some(app) = app else {
            return;
        };
        let source = window.window_handle();
        cx.defer(move |cx| {
            let _ = source.update(cx, |_, window, cx| {
                app.update(cx, |app, cx| {
                    let previous_focus = window.focused(cx);
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    if items
                        .first()
                        .is_some_and(|item| item["onOpen"]["type"] == "sessionMenu")
                    {
                        app.native_sidebar.next_menu_request += 1;
                        let owner =
                            format!("session-menu:{}", app.native_sidebar.next_menu_request);
                        items[0]["menuOwner"] = Value::String(owner.clone());
                        items[0]["onOpen"]["ownerId"] = Value::String(owner);
                    }
                    let account_panel = items
                        .first()
                        .and_then(|item| item["menuOwner"].as_str())
                        .map(|owner| (owner.to_owned(), 0));
                    let on_open = items.first().and_then(|item| item.get("onOpen")).cloned();
                    app.native_sidebar.menu = Some(SidebarMenuState {
                        window: source,
                        account_panel,
                        panels: vec![SidebarMenuPanel {
                            placement,
                            ..SidebarMenuPanel::new(items, position)
                        }],
                        focus,
                        previous_focus,
                        scale,
                    });
                    if let Some(command) = on_open {
                        app.dispatch_native_sidebar_ui(command, cx);
                    }
                    cx.notify();
                })
            });
        });
    }

    pub(crate) fn dismiss_native_sidebar_menu(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(menu) = self.native_sidebar.menu.take() {
            cx.defer(move |cx| {
                let _ = menu.window.update(cx, |_, window, cx| {
                    if let Some(focus) = menu.previous_focus {
                        focus.focus(window, cx);
                    }
                });
            });
            cx.notify();
        }
    }

    pub(crate) fn close_native_sidebar_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(menu) = self.native_sidebar.menu.take() {
            if let Some(focus) = menu.previous_focus {
                focus.focus(window, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn activate_native_sidebar_menu_secondary(
        &mut self,
        panel_index: usize,
        item_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let command = self
            .native_sidebar
            .menu
            .as_ref()
            .and_then(|menu| menu.panels.get(panel_index))
            .and_then(|panel| panel.items.get(item_index))
            .and_then(|item| item["secondary"].get("command"))
            .cloned();
        if let Some(command) = command {
            self.dispatch_native_sidebar_ui(command, cx);
        }
    }

    /// CDXC:ContextMenus 2026-09-19 DECISION:
    /// User: "please do the same for all context menus that have sub menus in the gpui app. when i
    /// click on the menu item that opens the sub menu again then close the sub menu if it's open.
    /// example in the sidebar's context menu there's a few like park and tag etc". Every sidebar
    /// menu, submenu, and flyout goes through this one activation, so the toggle lives here rather
    /// than in the per-menu builders. `toggle` is set by pointer clicks only: the keyboard's
    /// Enter/Space/Right must keep opening a submenu, and Left keeps closing it, so those callers
    /// pass `false` and a repeated Right leaves the open submenu alone. `child_item` records which
    /// row owns the panel stacked above, because `selected` also moves with the arrow keys and
    /// cannot say whether a submenu is open.
    pub(super) fn activate_native_sidebar_menu_item(
        &mut self,
        panel_index: usize,
        item_index: usize,
        row_bounds: Bounds<Pixels>,
        toggle: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // A row clicked in the frosted host window acts in the window the menu belongs to: its
        // actions open dialogs and move focus there, not in the host.
        if let Some(source) = self.native_sidebar.menu.as_ref().map(|menu| menu.window)
            && source != window.window_handle()
        {
            let app = cx.entity();
            cx.defer(move |cx| {
                let _ = source.update(cx, |_, window, cx| {
                    app.update(cx, |app, cx| {
                        app.activate_native_sidebar_menu_item(
                            panel_index,
                            item_index,
                            row_bounds,
                            toggle,
                            window,
                            cx,
                        );
                    });
                });
            });
            return;
        }
        let Some(menu) = self.native_sidebar.menu.as_mut() else {
            return;
        };
        let child_open = menu.panels.len() > panel_index + 1;
        let Some(panel) = menu.panels.get_mut(panel_index) else {
            return;
        };
        let Some(item) = panel.items.get(item_index).cloned() else {
            return;
        };
        if item["disabled"] == true || item["heading"] == true {
            return;
        }
        // This row already owns the submenu on screen, so a click on it is a request to shut it.
        let owns_open_child = child_open && panel.child_item == Some(item_index);
        if item["back"] == true {
            if let Some(items) = panel.pages.pop() {
                panel.replace_items(items);
            }
        } else if let Some(children) = item["children"].as_array() {
            if item["presentation"] == "page" {
                panel.pages.push(panel.items.clone());
                panel.replace_items(
                    std::iter::once(
                        serde_json::json!({"label": "Back", "icon": "chevron-left", "back": true}),
                    )
                    .chain(std::iter::once(serde_json::json!({"separator": true})))
                    .chain(children.clone())
                    .collect(),
                );
                truncate_panels(menu, panel_index + 1);
            } else if owns_open_child {
                if toggle {
                    close_child_panel(menu, panel_index, item_index);
                }
            } else {
                panel.selected = Some(item_index);
                panel.child_item = Some(item_index);
                truncate_panels(menu, panel_index + 1);
                menu.panels.push(SidebarMenuPanel::new(
                    children.clone(),
                    Point::new(
                        row_bounds.left(),
                        row_bounds.bottom() + px(4.0 * menu.scale),
                    ),
                ));
            }
        } else if let Some(command) = item.get("command") {
            if command["type"] == "sessionAccounts" && command["action"] == "load" {
                if owns_open_child {
                    // The accounts flyout is already up; this click only takes it back down.
                    if toggle {
                        close_child_panel(menu, panel_index, item_index);
                        cx.notify();
                    }
                    return;
                }
                panel.selected = Some(item_index);
                panel.child_item = Some(item_index);
                truncate_panels(menu, panel_index + 1);
                menu.account_panel = command["sessionId"]
                    .as_str()
                    .map(|id| (id.to_owned(), panel_index + 1));
                menu.panels.push(SidebarMenuPanel::new(
                    vec![serde_json::json!({"label": "Loading accounts…", "disabled": true})],
                    Point::new(
                        row_bounds.left(),
                        row_bounds.bottom() + px(4.0 * menu.scale),
                    ),
                ));
            }
            let action = NativeSidebarAction {
                command: command.clone(),
            };
            if item["keepOpen"] != true {
                self.close_native_sidebar_menu(window, cx);
            }
            self.handle_native_sidebar_action(&action, window, cx);
        }
        cx.notify();
    }

    pub(crate) fn render_native_sidebar_menu(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        use crate::app::window::frosted_host::{
            FrostedHostKind, SIDEBAR_MENU_HOST_LEVELS, frosted_hosting_active, show_frosted_host,
        };
        let Some(menu) = self.native_sidebar.menu.as_ref() else {
            hide_sidebar_menu_hosts(0, cx);
            return None;
        };
        let sidebar = self.native_sidebar.bounds;
        let hosted = frosted_hosting_active();
        let (panels, union) = self.native_sidebar_menu_panels(hosted, None, cx)?;
        let layers = div()
            .id("native-sidebar-menu-layers")
            .absolute()
            .left(union.left() - sidebar.left())
            .top(union.top() - sidebar.top())
            .w(union.size.width)
            .h(union.size.height)
            .track_focus(&menu.focus)
            .on_mouse_down_out(
                cx.listener(|app, event: &gpui::MouseDownEvent, window, cx| {
                    let position = window.mouse_position();
                    if app
                        .native_sidebar
                        .more_button_bounds
                        .get()
                        .is_some_and(|bounds| {
                            bounds.contains(&position) || bounds.contains(&event.position)
                        })
                    {
                        return;
                    }
                    if app
                        .native_sidebar
                        .menu
                        .as_ref()
                        .is_some_and(SidebarMenuState::dropped_from_trigger)
                    {
                        app.native_sidebar.more_menu_dismissed_at = Some(web_time::Instant::now());
                    }
                    app.close_native_sidebar_menu(window, cx);
                }),
            )
            .on_key_down(cx.listener(Self::native_sidebar_menu_key_down));
        if hosted {
            // The panels draw in the frosted host above the sidebar; this window keeps only the
            // menu's keyboard focus and its click-outside dismissal. It still owns the menu's
            // frame: as the key window it goes on getting pointer moves under the host, and the
            // rows beneath must not light up there (the same frame its panels occluded unhosted).
            // CDXC:Sidebar 2026-09-25 DECISION: User approved ("Yes, keep it") this occluding region: while a hosted sidebar menu is open, the main window's area under it takes no hover or clicks, so only the menu reacts, as before menus were frosted.
            let layers = layers.occlude();
            // One window per panel, so a submenu stacked over its parent blurs the parent the way
            // a header dropdown blurs whatever is under it.
            let sidebar = self.native_sidebar.bounds;
            let scale = menu.scale;
            let count = menu.panels.len().min(usize::from(SIDEBAR_MENU_HOST_LEVELS));
            for (index, panel) in menu.panels.iter().take(count).enumerate() {
                let app = cx.entity();
                show_frosted_host(
                    FrostedHostKind::SidebarMenu(index as u8),
                    menu.window,
                    panel.bounds(sidebar, scale, index > 0),
                    None,
                    std::rc::Rc::new(move |_, cx| {
                        app.update(cx, |app, cx| {
                            app.native_sidebar_menu_panels(true, Some(index), cx)
                                .map(|(panel, _)| panel.into_any_element())
                                .unwrap_or_else(|| div().into_any_element())
                        })
                    }),
                    Some(cx.entity()),
                    cx,
                );
            }
            hide_sidebar_menu_hosts(count, cx);
            return Some(deferred(layers).with_priority(20).into_any_element());
        }
        hide_sidebar_menu_hosts(0, cx);
        Some(
            deferred(layers.child(panels))
                .with_priority(20)
                .into_any_element(),
        )
    }

    /// The menu's panels, positioned inside the box that holds them all (returned with it, in
    /// window coordinates). `frosted` draws them for a frosted host window, with the thinned fill
    /// over its blur; `only` draws just that panel, at the origin of its own window.
    fn native_sidebar_menu_panels(
        &self,
        frosted: bool,
        only: Option<usize>,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(gpui::Div, Bounds<Pixels>)> {
        let menu = self.native_sidebar.menu.as_ref()?;
        let sidebar = self.native_sidebar.bounds;
        let scale = menu.scale;
        let light = self
            .native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.hud["settings"]
                    .as_object()
                    .is_some_and(sidebar_uses_light_theme)
            });
        let panels = menu.panels.clone();
        let background = titlebar_popup_menu_background();
        let foreground = titlebar_popup_menu_foreground();
        let hover = titlebar_popup_menu_hover_color();
        let panel_bounds: Vec<_> = panels
            .iter()
            .enumerate()
            .map(|(index, panel)| panel.bounds(sidebar, scale, index > 0))
            .collect();
        let left = panel_bounds
            .iter()
            .map(|bounds| bounds.left())
            .min()
            .unwrap_or(sidebar.left());
        let top = panel_bounds
            .iter()
            .map(|bounds| bounds.top())
            .min()
            .unwrap_or(sidebar.top());
        let right = panel_bounds
            .iter()
            .map(|bounds| bounds.right())
            .max()
            .unwrap_or(left);
        let bottom = panel_bounds
            .iter()
            .map(|bounds| bounds.bottom())
            .max()
            .unwrap_or(top);
        let mut layers = div().relative().w(right - left).h(bottom - top);
        let view = cx.entity();
        for (panel_index, panel) in panels.iter().enumerate() {
            if only.is_some_and(|only| only != panel_index) {
                continue;
            }
            let bounds = panel.bounds(sidebar, scale, panel_index > 0);
            let relative = if only.is_some() {
                Point::default()
            } else {
                bounds.origin - Point::new(left, top)
            };
            if panel.is_agent_launcher() {
                layers = layers.child(self.render_agent_launcher_menu_panel(
                    panel_index,
                    panel,
                    bounds,
                    relative,
                    scale,
                    view.clone(),
                    cx,
                ));
                continue;
            }
            let mut content = v_flex()
                .on_children_prepainted(measure_menu_panel(
                    view.clone(),
                    panel_index,
                    px(12.0 * scale + 2.0),
                ))
                .id(format!("native-sidebar-menu-panel-{panel_index}"))
                .absolute()
                .left(relative.x)
                .top(relative.y)
                .w(bounds.size.width)
                .max_h(bounds.size.height)
                .overflow_y_scroll()
                .track_scroll(&panel.scroll)
                .occlude()
                .p(px(6.0 * scale))
                .rounded(px(8.0 * scale))
                .border_1()
                .border_color(titlebar_popup_menu_border_color())
                .bg(if frosted {
                    popup_window_surface(background)
                } else {
                    background
                })
                .text_color(foreground)
                .font_weight(FontWeight::NORMAL)
                .text_size(px(13.0 * scale))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation());
            let mut offset = 6.0;
            for (item_index, item) in panel.items.iter().enumerate() {
                if item["separator"] == true {
                    content = content.child(
                        div()
                            .h(px(13.0 * scale))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .h(px(1.0))
                                    .w_full()
                                    .bg(titlebar_popup_menu_border_color()),
                            ),
                    );
                    offset += 13.0;
                    continue;
                }
                let label = item["label"].as_str().unwrap_or_default().to_owned();
                let icon = item["icon"]
                    .as_str()
                    .map(|icon| gpui_sidebar_command_icon_asset_path(Some(icon)).to_string());
                let icon_color = item["iconColor"]
                    .as_str()
                    .and_then(|color| u32::from_str_radix(color.trim_start_matches('#'), 16).ok())
                    .map(|color| {
                        if !light {
                            return color;
                        }
                        if item["icon"] == "star-filled" {
                            return 0xf6c944;
                        }
                        let channel =
                            |shift| ((color >> shift & 255u32) as f32 * 0.42).round() as u32;
                        channel(16) << 16 | channel(8) << 8 | channel(0)
                    })
                    .map(gpui::rgb)
                    .map(gpui::Hsla::from)
                    .unwrap_or(foreground);
                let image = item["imageDataUrl"].as_str().and_then(|value| {
                    super::images::agent_image(value, item["agentIcon"].as_str(), light)
                });
                let color = item["color"]
                    .as_str()
                    .and_then(|color| u32::from_str_radix(color.trim_start_matches('#'), 16).ok())
                    .map(gpui::rgb);
                let disabled = item["disabled"] == true;
                let heading = item["heading"] == true;
                let selected = panel.selected == Some(item_index);
                let row_bounds = Bounds {
                    origin: Point::new(bounds.left(), bounds.top() + px(offset * scale)),
                    size: gpui::size(bounds.size.width, px(34.0 * scale)),
                };
                content = content.child(
                    h_flex()
                        .id(format!("native-menu-item-{panel_index}-{item_index}"))
                        .w_full()
                        .min_h(px(if heading { 24.0 } else { 34.0 } * scale))
                        .flex_shrink_0()
                        .px(px(10.0 * scale))
                        .gap(px(8.0 * scale))
                        .rounded(px(6.0 * scale))
                        .when(selected, |row| row.bg(hover))
                        .when(!disabled && !heading, |row| row.hover(|row| row.bg(hover)))
                        .when(disabled, |row| row.opacity(0.42))
                        .when(heading, |row| row.text_size(px(11.0 * scale)).opacity(0.6))
                        .when(item["danger"] == true, |row| {
                            row.text_color(chrome_color(0xff7b72, 0xd32f2f))
                        })
                        .when_some(icon, |row, icon| {
                            row.child(
                                gpui::svg()
                                    .path(icon)
                                    .text_color(icon_color)
                                    .size(px(16.0 * scale))
                                    .flex_shrink_0(),
                            )
                        })
                        .when_some(image, |row, image| {
                            row.child(gpui::img(image).size(px(16.0 * scale)).flex_shrink_0())
                        })
                        .when_some(color, |row, color| {
                            row.child(div().size(px(14.0 * scale)).rounded_full().bg(color))
                        })
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    h_flex()
                                        .gap(px(4.0 * scale))
                                        .child(div().min_w_0().whitespace_nowrap().child(label))
                                        .when_some(item["suffix"].as_str(), |row, suffix| {
                                            row.child(
                                                div()
                                                    .text_size(px(10.0 * scale))
                                                    .opacity(0.6)
                                                    .child(suffix.to_owned()),
                                            )
                                        }),
                                )
                                .when_some(
                                    item["detail"].as_str().filter(|detail| !detail.is_empty()),
                                    |column, detail| {
                                        column.child(
                                            div()
                                                .text_size(px(10.0 * scale))
                                                .opacity(0.6)
                                                .child(detail.to_owned()),
                                        )
                                    },
                                ),
                        )
                        .when(item["supportsChat"] == true, |row| {
                            row.child(
                                gpui::svg()
                                    .path("titlebar/message-circle.svg")
                                    .size(px(14.0 * scale))
                                    .text_color(foreground.opacity(0.5)),
                            )
                        })
                        .when_some(item.get("secondary"), |row, secondary| {
                            row.child(
                                h_flex()
                                    .id(format!("native-menu-secondary-{panel_index}-{item_index}"))
                                    .h(px(28.0 * scale))
                                    .px(px(5.0 * scale))
                                    .gap(px(3.0 * scale))
                                    .rounded(px(4.0 * scale))
                                    .hover(|button| button.bg(hover))
                                    .child(
                                        gpui::svg()
                                            .path(gpui_sidebar_command_icon_asset_path(
                                                secondary["icon"].as_str(),
                                            ))
                                            .size(px(14.0 * scale))
                                            .text_color(foreground),
                                    )
                                    .child(
                                        secondary["label"].as_str().unwrap_or_default().to_owned(),
                                    )
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        cx.stop_propagation();
                                        app.activate_native_sidebar_menu_secondary(
                                            panel_index,
                                            item_index,
                                            cx,
                                        );
                                    })),
                            )
                        })
                        .when(item["checked"] == true, |row| {
                            row.child(titlebar_svg_icon(
                                "titlebar/check.svg",
                                14.0 * scale,
                                foreground,
                            ))
                        })
                        .when(
                            item.get("children").is_some()
                                || item["command"]["type"] == "sessionAccounts"
                                    && item["command"]["action"] == "load",
                            |row| {
                                row.child(titlebar_svg_icon(
                                    "titlebar/chevron-right.svg",
                                    14.0 * scale,
                                    foreground,
                                ))
                            },
                        )
                        .on_click(cx.listener(move |app, _, window, cx| {
                            cx.stop_propagation();
                            app.activate_native_sidebar_menu_item(
                                panel_index,
                                item_index,
                                row_bounds,
                                true,
                                window,
                                cx,
                            );
                        })),
                );
                offset += if heading { 24.0 } else { 34.0 };
            }
            layers = layers.child(content);
        }
        Some((
            layers,
            Bounds::from_corners(Point::new(left, top), Point::new(right, bottom)),
        ))
    }

    fn native_sidebar_menu_key_down(
        app: &mut Self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let Some(menu) = app.native_sidebar.menu.as_mut() else {
            return;
        };
        // The agent account page returns to the agent list on Escape or Left, like the React launcher.
        let back = menu
            .panels
            .last()
            .and_then(|panel| panel.items.first())
            .and_then(|item| item.get("command"))
            .filter(|command| command["type"] == "agentAccounts" && command["action"] == "root")
            .cloned();
        match key {
            "escape" => {
                if let Some(command) = back {
                    app.dispatch_native_sidebar_ui(command, cx);
                } else {
                    app.close_native_sidebar_menu(window, cx);
                }
            }
            "left" => {
                if menu.panels.len() > 1 {
                    let len = menu.panels.len() - 1;
                    truncate_panels(menu, len);
                    // The parent row keeps the keyboard cursor but no longer owns a panel,
                    // so Right reopens the submenu instead of finding it already open.
                    if let Some(panel) = menu.panels.last_mut() {
                        panel.child_item = None;
                    }
                } else if let Some(items) = menu.panels[0].pages.pop() {
                    menu.panels[0].replace_items(items);
                } else if let Some(command) = back {
                    app.dispatch_native_sidebar_ui(command, cx);
                }
            }
            "up" | "down" | "home" | "end" => {
                if let Some(panel) = menu.panels.last_mut() {
                    panel.move_selection(key);
                }
            }
            "enter" | "space" | "right" => {
                let panel_index = menu.panels.len() - 1;
                let panel = &menu.panels[panel_index];
                if let Some(index) = panel.selected {
                    if key == "right" && panel.items[index].get("secondary").is_some() {
                        app.activate_native_sidebar_menu_secondary(panel_index, index, cx);
                        window.prevent_default();
                        cx.stop_propagation();
                        return;
                    }
                    let bounds =
                        panel.bounds(app.native_sidebar.bounds, menu.scale, panel_index > 0);
                    let y = panel.items[..index]
                        .iter()
                        .map(|item| {
                            if item["separator"] == true {
                                13.0
                            } else {
                                34.0
                            }
                        })
                        .sum::<f32>();
                    let row_bounds = Bounds {
                        origin: Point::new(
                            bounds.left(),
                            bounds.top() + px((6.0 + y) * menu.scale),
                        ),
                        size: gpui::size(bounds.size.width, px(34.0 * menu.scale)),
                    };
                    app.activate_native_sidebar_menu_item(
                        panel_index,
                        index,
                        row_bounds,
                        false,
                        window,
                        cx,
                    );
                }
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
}

/// Drops the panel that `item_index` opened on top of `panel_index`, and everything stacked above
/// it, then clears the row's expanded highlight so the parent menu no longer looks open.
fn close_child_panel(menu: &mut SidebarMenuState, panel_index: usize, item_index: usize) {
    if let Some(panel) = menu.panels.get_mut(panel_index) {
        panel.child_item = None;
        if panel.selected == Some(item_index) {
            panel.selected = None;
        }
    }
    truncate_panels(menu, panel_index + 1);
}

/// Every panel stack cut goes through here: an answer still on its way to a panel that is gone
/// (closed, popped with Left, or replaced by another row's submenu at the same index) would
/// otherwise land in whatever panel sits at that index now.
fn truncate_panels(menu: &mut SidebarMenuState, len: usize) {
    menu.panels.truncate(len);
    if menu
        .account_panel
        .as_ref()
        .is_some_and(|(_, index)| *index >= len)
    {
        menu.account_panel = None;
    }
}

/// Records the rendered rows' height (plus `chrome`: vertical padding and border) so the panel's next frame fits them exactly.
pub(super) fn measure_menu_panel(
    view: gpui::Entity<GhostexGpuiApp>,
    panel_index: usize,
    chrome: Pixels,
) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut gpui::App) + 'static {
    move |rows, _, cx| {
        let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
            return;
        };
        let height = last.bottom() - first.top() + chrome;
        view.update(cx, |app, cx| {
            let Some(panel) = app
                .native_sidebar
                .menu
                .as_mut()
                .and_then(|menu| menu.panels.get_mut(panel_index))
            else {
                return;
            };
            if panel
                .measured_height
                .is_none_or(|measured| (measured - height).abs() > px(0.5))
            {
                panel.measured_height = Some(height);
                cx.notify();
            }
        });
    }
}

/// Hides the sidebar menu's panel windows from `from` up.
fn hide_sidebar_menu_hosts(from: usize, cx: &mut gpui::App) {
    use crate::app::window::frosted_host::{
        FrostedHostKind, SIDEBAR_MENU_HOST_LEVELS, hide_frosted_host,
    };
    for level in from..usize::from(SIDEBAR_MENU_HOST_LEVELS) {
        hide_frosted_host(FrostedHostKind::SidebarMenu(level as u8), cx);
    }
}
