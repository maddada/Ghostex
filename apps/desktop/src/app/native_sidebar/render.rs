use super::drag::SidebarDropTarget;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::v_flex;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn render_native_sidebar(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if !cx.has_active_drag() {
            self.native_sidebar.drop_command = None;
            self.native_sidebar.drop_memo = None;
            self.native_sidebar.dragging = None;
        }
        // CDXC:Sidebar 2026-09-21 WHY: Rows only carry their tooltip while the pointer is inside, no menu is open and nothing is dragged. The frame that drops it also drops the hover-leave listener that would have closed an open tooltip, so it stayed up with the pointer elsewhere; close it on that transition.
        let row_tooltips_attached = self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        if self.native_sidebar.row_tooltips_attached && !row_tooltips_attached {
            gpui_component::Root::hide_tooltip(window, cx);
        }
        self.native_sidebar.row_tooltips_attached = row_tooltips_attached;
        let Some(snapshot) = self.native_sidebar.snapshot.clone() else {
            return div().size_full().into_any_element();
        };
        self.native_sidebar.focus_stops.begin_frame();
        let entry_handle = self.native_sidebar.focus_stops.entry_handle(cx);
        // The list keeps showing the outgoing Space while its exit fade runs; the selector row above it already shows the new one.
        let content = self
            .native_sidebar
            .space_gesture
            .exiting_snapshot()
            .cloned()
            .unwrap_or_else(|| snapshot.clone());
        let session_groups = super::session_group_tree::SessionGroupTree::new(&content);
        let view = cx.entity().clone();
        let bounds_view = view.clone();
        let wheel_view = view.clone();
        let blocks_view = view.clone();
        let grabbing = self.native_sidebar.space_gesture.is_mouse_dragging();
        let presence_view = view.clone();
        let header_view = view.clone();
        self.native_sidebar.header_hover = Default::default();
        let header_hover = self.native_sidebar.header_hover.clone();
        let (space_offset, space_opacity) = self.native_sidebar.space_gesture.presentation();
        let appearance = SidebarAppearance::from_hud(&snapshot.hud, window);
        // The Unified sidebar draws its own header and list in the classic ones' places; nothing
        // else of the sidebar changes (native_sidebar/unified/).
        //
        // CDXC:Spaces 2026-11-05 DECISION:
        // User: "please implement spaces for the unified sidebar so when that extension is enabled it applies for unified sidebar also not just projecst sidebar". With Spaces on, the Unified header carries the Project sidebar's own Space row (selectors.rs, without the Bots slot), the list takes the same sideways swipe and empty-space drag with the same slide and fade (space_gesture.rs), and the outgoing Space's list stays frozen through the exit fade just as the Project list's does. The scope itself is gx-core's (`unified_space_scope`, `build_unified_sidebar_scoped`).
        let unified = self.native_sidebar.unified.clone();
        // The rows keep the Project sidebar's even spacing (selectors.rs): the filter has its own row.
        let gap = |height: f32| div().h(px(height * appearance.scale)).flex_shrink_0();
        let unified_header = unified.as_ref().map(|unified| {
            v_flex()
                .w_full()
                .flex_shrink_0()
                .child(self.render_native_sidebar_new_session_row(&appearance, window, cx))
                .child(gap(super::selectors::SIDEBAR_TOP_ROW_GAP))
                .child(self.render_native_unified_header(unified, &appearance, window, cx))
                .when(snapshot.spaces_enabled, |header| {
                    header.child(gap(super::selectors::SIDEBAR_TOP_ROW_GAP))
                })
                .children(self.render_native_sidebar_space_row(
                    &snapshot,
                    &appearance,
                    false,
                    false,
                    cx,
                ))
                // CDXC:Chief 2026-11-08 DECISION: User (Q1922): Main Hub shows in the Unified sidebar too, under its header, drawn as in the Project sidebar (native_sidebar/chief.rs); its block lines up with the Unified group headers' box.
                .child(gap(super::selectors::SIDEBAR_TOP_LIST_GAP))
                .children(self.render_native_sidebar_chief(&snapshot, &appearance, true, cx))
                .into_any_element()
        });
        let unified_list = unified.as_ref().map(|unified| {
            let shown = self
                .native_sidebar
                .space_gesture
                .exiting_unified()
                .cloned()
                .unwrap_or_else(|| unified.clone());
            self.render_native_unified_list(&shown, &snapshot.hud, &appearance, cx)
        });
        let classic = unified.is_none();
        let sticky_clip_top = if classic {
            self.native_sticky_project_clip_top(&content, &appearance)
        } else {
            None
        };
        // The project header pinned at the top: focusing it reveals nothing, its pinned copy shows.
        let pinned_header = if classic {
            self.native_sticky_project_group_id(&content, &appearance)
        } else {
            None
        };
        let reveal_scale = appearance.scale;
        // Built once per frame for the Commands row's button and whichever strip can draw: the
        // pinned one and the peek are exclusive (`sidebar_usage_visible`).
        let usage_meters = self.account_usage_meters();
        let usage_meter_count = usage_meters.len();
        let (pinned_usage_meters, peek_usage_meters) = if self.sidebar_usage_visible {
            (usage_meters, Vec::new())
        } else {
            (Vec::new(), usage_meters)
        };
        v_flex()
            .on_children_prepainted(move |bounds, window, cx| {
                if bounds.len() >= 4 {
                    bounds_view.update(cx, |app, _| {
                        app.native_sidebar.bounds = gpui::Bounds {
                            origin: bounds[0].origin,
                            size: gpui::size(
                                bounds[0].size.width,
                                bounds[3].bottom() - bounds[0].top(),
                            ),
                        };
                        #[cfg(all(target_os = "macos", not(ghostex_web)))]
                        super::pointer::track_bounds(app.native_sidebar.bounds, window);
                    });
                }
                // After every part of the sidebar prepainted, the bottom bar's stops included.
                bounds_view.update(cx, |app, cx| {
                    app.reveal_focused_native_sidebar_stop(
                        reveal_scale,
                        pinned_header.as_deref(),
                        window,
                        cx,
                    );
                });
            })
            .id("native-sidebar-root")
            // The arrow keys and Escape while one of the sidebar's controls has focus (keyboard.rs).
            .on_key_down(cx.listener(Self::native_sidebar_keyboard_key_down))
            // CDXC:Accessibility 2026-09-22 DECISION:
            // User: "add full accessibility to the sidebar, the button at the bottom, and the chat view so that we can drive this UI using AI", for "an e2e tester or an engineer to debug issues", without going "overboard with it if it would affect performance". Every row, header, section and button carries a role, a label and its state words; gpui builds the tree only while an assistive client (a screen reader, cua-driver, the web build's DOM mirror) is connected, so an idle desktop pays nothing.
            .role(gpui::Role::Navigation)
            .aria_label("Sidebar")
            .relative()
            .on_drag_move::<super::drag::SidebarDrag>(cx.listener(|app, event, _, cx| {
                app.clear_native_sidebar_drop_outside(event, cx);
            }))
            .on_drop::<super::drag::SidebarDrag>(cx.listener(|app, _, _, cx| {
                app.finish_native_sidebar_drop(cx);
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|app, event: &gpui::MouseDownEvent, window, cx| {
                    if app
                        .native_sidebar
                        .more_button_bounds
                        .get()
                        .is_some_and(|bounds| {
                            bounds.contains(&event.position)
                                || bounds.contains(&window.mouse_position())
                        })
                    {
                        return;
                    }
                    app.dismiss_native_sidebar_menu_by_press(event.position, window, cx);
                }),
            )
            .on_click(cx.listener(|app, event: &gpui::ClickEvent, _, cx| {
                // A second press dragged sideways far enough to switch Space is not a double-click.
                let space_drag = match event {
                    gpui::ClickEvent::Mouse(click) => {
                        super::space_gesture::SpaceGesture::is_drag_click(
                            click.down.position,
                            click.up.position,
                        )
                    }
                    _ => false,
                };
                if event.click_count() == 2
                    && !space_drag
                    && app
                        .native_sidebar
                        .snapshot
                        .as_ref()
                        .is_some_and(|snapshot| {
                            snapshot.hud["createSessionOnSidebarDoubleClick"] == true
                        })
                {
                    app.dispatch_native_sidebar_command(
                        serde_json::json!({"type": "createSession"}),
                        cx,
                    );
                }
            }))
            .size_full()
            .min_h_0()
            .bg(crate::app::helpers::sidebar_chrome_fill(
                appearance.glass,
                180.0,
            ))
            .text_color(appearance.foreground)
            .text_size(px(super::appearance::SIDEBAR_TEXT_SIZE * appearance.scale))
            .font_family(crate::ui_fonts::UI_FONT)
            .font_weight(FontWeight::LIGHT)
            .child(self.render_native_sidebar_navigation(&appearance, cx))
            .child(match unified_header {
                Some(header) => header,
                None => self.render_native_sidebar_selectors(&snapshot, &appearance, window, cx),
            })
            /*
            The list and its bottom fade ramp are one child on purpose: the
            `on_children_prepainted` above measures the sidebar from its fourth
            child, so the ramp must live inside the list's own slot rather than
            become a root child of its own.
            CDXC:Sidebar 2026-09-20 WHY:
            This wrapper has to be a flex column, not a bare `div()`: GPUI's default display is
            block, and under block layout the scroll view's `flex_1().min_h_0()` does nothing, so it
            took its full content height, never clipped and never scrolled, and the session rows
            painted straight over the usage strip and the Commands row below it.
            */
            .child(
                v_flex()
                    .relative()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .child(crate::app::element::clip_above(
                        sticky_clip_top,
                        v_flex()
                            .on_children_prepainted(move |_, window, cx| {
                                view.update(cx, |app, cx| {
                                    app.update_native_sidebar_scroll(window, cx);
                                    app.update_native_space_transition(window, cx);
                                    // The store's own request wins over the publish's.
                                    //
                                    // CDXC:Projects 2026-09-21 WHY:
                                    // `ui.renameRequest` used to be the sidebar page's, carried
                                    // here on every snapshot. Since M5 piece 7d the CREATE is the
                                    // store's (a project dropped onto New Group mints the
                                    // collection in Rust), so the page never learns that collection
                                    // exists and its snapshot carries nothing. Both are read, newest
                                    // first, because the page still creates collections from its own
                                    // menus until that path moves too.
                                    if let Some(request) =
                                        app.gx_store_pending_collection_rename().or_else(|| {
                                            app.native_sidebar.snapshot.as_ref().and_then(
                                                |snapshot| snapshot.rename_request.clone(),
                                            )
                                        })
                                        && app.native_sidebar.handled_rename
                                            != Some(request.request_id)
                                    {
                                        app.native_sidebar.handled_rename =
                                            Some(request.request_id);
                                        cx.defer_in(window, move |app, window, cx| {
                                            app.begin_native_collection_rename(
                                                &request.collection_id,
                                                window,
                                                cx,
                                            )
                                        });
                                    }
                                })
                            })
                            .id("native-sidebar-scroll")
                            .w_full()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.native_sidebar.scroll)
                            .on_scroll(cx.listener(|app, _, _, cx| {
                                app.native_sidebar.scroll_animation = None;
                                app.native_sidebar_scroll_wheel_moved(cx);
                            }))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|app, event: &gpui::MouseDownEvent, _, cx| {
                                    app.begin_native_space_mouse_drag(event, cx)
                                }),
                            )
                            .child(
                                v_flex()
                                    // Every block but the trailing ungroup drop zone, which is empty list space.
                                    .on_children_prepainted(move |bounds, _, cx| {
                                        blocks_view.update(cx, |app, _| {
                                            app.native_sidebar.space_gesture.set_list_blocks(
                                                &bounds[..bounds.len().saturating_sub(1)],
                                            )
                                        })
                                    })
                                    .w_full()
                                    .pl(px(6.0 * appearance.scale))
                                    .pr(px(2.0 * appearance.scale))
                                    .relative()
                                    .left(px(space_offset * appearance.scale))
                                    .opacity(space_opacity)
                                    .children(unified_list)
                                    .when(classic && content.automations_row, |column| {
                                        column.child(self.render_native_sidebar_automations_row(
                                            content.automations_today,
                                            &appearance,
                                            cx,
                                        ))
                                    })
                                    .when(
                                        classic
                                            && content.order.is_empty()
                                            && content.machine_notice.is_none(),
                                        |column| {
                                            column.child(self.render_native_sidebar_empty(
                                                &content,
                                                &appearance,
                                                cx,
                                            ))
                                        },
                                    )
                                    .children(content.order.iter().filter(|_| classic).filter_map(
                                        |item| {
                                            if item.kind == "collection" {
                                                content
                                                    .collections
                                                    .iter()
                                                    .find(|collection| {
                                                        collection.collection_id == item.id
                                                    })
                                                    .map(|collection| {
                                                        self.render_native_collection(
                                                            collection,
                                                            &content,
                                                            &appearance,
                                                            cx,
                                                        )
                                                    })
                                            } else {
                                                content
                                                    .groups
                                                    .iter()
                                                    .find(|group| group.group_id == item.id)
                                                    .filter(|group| !session_groups.nests(group))
                                                    // A GBot is drawn inside Main Hub's group (gbots_rows.rs).
                                                    .filter(|group| group.gbot().is_none())
                                                    .map(|group| {
                                                        // CDXC:Spaces 2026-10-06 WHY: padding, not a margin, so this block's measured bounds (where a press is a row's and not the empty list space that starts a Space drag) cover the project header's box, which reaches left over its chevron; as a margin, a click on the chevron armed the Space drag and showed the grabbing hand.
                                                        div()
                                                            .pl(px(18.0 * appearance.scale))
                                                            .mr(px(5.0 * appearance.scale))
                                                            .mb(px(10.0 * appearance.scale))
                                                            .child(
                                                                self.render_native_sidebar_group(
                                                                    group,
                                                                    session_groups
                                                                        .children(&group.group_id),
                                                                    &content.hud,
                                                                    &appearance,
                                                                    cx,
                                                                ),
                                                            )
                                                            .into_any_element()
                                                    })
                                            }
                                        },
                                    ))
                                    .when(classic && content.bots_mode, |column| {
                                        column.child(
                                            self.render_native_sidebar_new_bot_row(&appearance, cx),
                                        )
                                    })
                                    .children(
                                        content.show_all_projects_row.filter(|_| classic).map(
                                            |row| {
                                                self.render_native_show_all_projects_row(
                                                    row,
                                                    &appearance,
                                                    cx,
                                                )
                                            },
                                        ),
                                    )
                                    // The Unified list's cards drag only onto the panes: there
                                    // the zone would move a session into the classic list's last
                                    // project.
                                    .child(
                                        div()
                                            .id("native-sidebar-ungroup-drop")
                                            .h(px(24.0 * appearance.scale))
                                            .w_full()
                                            .flex_shrink_0()
                                            .when(classic, |zone| {
                                                zone.sidebar_drop_target(
                                                    "ungroup",
                                                    String::new(),
                                                    None,
                                                    cx,
                                                )
                                            }),
                                    ),
                            ),
                    ))
                    .child(self.render_native_sidebar_list_fade(&appearance))
                    .when_some(
                        content.machine_notice.as_ref().filter(|_| classic),
                        |list, notice| {
                            list.child(self.render_native_sidebar_machine_notice(
                                notice,
                                &appearance,
                                cx,
                            ))
                        },
                    ),
            )
            /*
            The bottom (the workspace button's line, the usage strip or its handle) is one footer
            child on purpose, and always there: `on_children_prepainted` above measures the
            sidebar from its fourth child, so parts that come and go must not shift that index.
            */
            .child(self.render_native_sidebar_bottom(
                &appearance,
                pinned_usage_meters,
                usage_meter_count,
                window,
                cx,
            ))
            .child(
                gpui::canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
                    move |bounds, hitbox, window, _| {
                        track_pointer_presence(presence_view, hitbox, window);
                        header_hover.track(header_view, window);
                        let view = wheel_view.clone();
                        window.on_mouse_event(
                            move |event: &gpui::ScrollEvent, phase, window, cx| {
                                if phase == gpui::DispatchPhase::Capture
                                    && bounds.contains(&event.position)
                                {
                                    view.update(cx, |app, cx| {
                                        app.handle_native_space_wheel(event, window, cx)
                                    });
                                }
                            },
                        );
                        // A Space drag follows the pointer anywhere in the window until the button comes up.
                        if grabbing {
                            window.set_window_cursor_style(gpui::CursorStyle::ClosedHand);
                        }
                        let view = wheel_view.clone();
                        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Capture {
                                view.update(cx, |app, cx| {
                                    app.move_native_space_mouse_drag(event, cx)
                                });
                            }
                        });
                        let view = wheel_view.clone();
                        window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Capture
                                && event.button == gpui::MouseButton::Left
                            {
                                view.update(cx, |app, cx| app.end_native_space_mouse_drag(cx));
                            }
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .children(
                classic
                    .then(|| self.render_native_sticky_project(&content, &appearance, window, cx))
                    .flatten(),
            )
            .children(self.render_native_sidebar_usage_peek(
                &appearance,
                peek_usage_meters,
                window,
                cx,
            ))
            .children(self.render_native_sidebar_menu(window, cx))
            // The floating links card, where a build draws it in the page (the desktop's is a window).
            .children(self.render_session_links_page_card(cx))
            // The Main Hub card, where a build draws it in the page (the desktop's is a window).
            .children(self.render_main_hub_page_card(cx))
            // Where the Focus Sidebar hotkey's entry holds the keyboard while it waits for its row:
            // no size, so no click ever focuses it. Last, so the children the bounds above count
            // keep their places.
            .child(div().absolute().size_0().track_focus(&entry_handle))
            .into_any_element()
    }
}

/// CDXC:Sidebar 2026-09-19 WHY:
/// GPUI `on_hover` reports "not hovered" while a mouse button is held, so a click that moved by a pixel marked the pointer as outside the sidebar and cleared the hovered card mid-click; its X vanished and the release landed on the card.
/// Presence follows the hit test alone and still goes false during a drag and when the pointer leaves the window.
fn track_pointer_presence(
    view: gpui::Entity<GhostexGpuiApp>,
    hitbox: gpui::Hitbox,
    window: &mut gpui::Window,
) {
    let set_inside = move |inside: bool, cx: &mut gpui::App| {
        view.update(cx, |app, cx| {
            if app.native_sidebar.pointer_inside == inside {
                return;
            }
            app.native_sidebar.pointer_inside = inside;
            if !inside {
                app.native_sidebar.hovered_collection = None;
                app.native_sidebar.hovered_group = None;
                app.native_sidebar.hovered_session = None;
                app.native_sidebar.hovered_section = None;
            }
            cx.notify();
        })
    };
    let set_outside = set_inside.clone();
    window.on_mouse_event(move |_: &gpui::MouseMoveEvent, phase, window, cx| {
        if phase == gpui::DispatchPhase::Bubble {
            set_inside(!cx.has_active_drag() && hitbox.is_hovered(window), cx);
        }
    });
    window.on_mouse_event(move |_: &gpui::MouseExitEvent, phase, _, cx| {
        if phase == gpui::DispatchPhase::Bubble {
            set_outside(false, cx);
        }
    });
}
