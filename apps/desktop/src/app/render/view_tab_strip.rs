//! The view panel's own header: one row of tabs (open views and the Browser's pages, in one order),
//! the `+` that opens another, and the pop-out and expand controls on the trailing side.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::window_drag_region::window_drag_region;
use crate::app::view_strip_order::ViewStripEntry;
use crate::*;

const VIEW_TAB_GROUP: &str = "ghostex-gpui-view-tab";

/// Where a tab sits in the drawn row, and how that changes the way it is drawn.
#[derive(Clone, Copy)]
pub(crate) struct ViewStripTabSlot {
    pub(crate) index: usize,
    pub(crate) pinned: bool,
    pub(crate) shows_drop_marker: bool,
}

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-20 DECISION:
    /// User (screens 03 to 05): a project can have several views open at once, and they live as a
    /// tab strip at the top of the view panel instead of as one mode selected in the work area
    /// header. Each tab is its view's icon and name with a close control, the tabs drag to reorder,
    /// `+` opens another view, and the trailing pair pops the view out or expands it over the
    /// sessions column. This supersedes the 2026-09-11 rule that the view buttons are centred mode
    /// tabs in the header strip: there is no mode switcher there any more.
    /// CDXC:Workarea 2026-09-21 DECISION:
    /// User: the `+` sits right after the last tab of the whole row, the Browser's page tabs
    /// included, with no gap before it and no divider between the view tabs and the page tabs
    /// (this supersedes the same day's placements left of Open Externally and between the two
    /// groups). The Code view shows no Open Externally button at all.
    pub(crate) fn render_view_tab_strip(
        &self,
        active_mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let entries = self.view_strip_entries();
        let scroll_handle = self.view_tab_scroll_handle.clone();
        let drop_index = self.view_strip_drop_index;
        // Exactly one tab in the row is filled: the one the panel is showing. While another view is
        // open the browser has nothing on screen, so none of its tabs claim that fill.
        let showing_browser_tab_id = (self.active_mode == TitlebarMode::Browser)
            .then(|| {
                self.browser_tabs
                    .active_tab_id_for_pane(self.browser_tabs.focused_pane)
            })
            .flatten();
        let mut view_index = 0;
        let tab_elements = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let slot = ViewStripTabSlot {
                    index,
                    pinned: self.view_strip_tab_pinned(entry.key()),
                    shows_drop_marker: drop_index == Some(index),
                };
                match *entry {
                    ViewStripEntry::View(mode) => {
                        let hotkey_index = view_index;
                        view_index += 1;
                        self.render_view_tab(mode, slot, hotkey_index, active_mode, cx)
                    }
                    ViewStripEntry::Browser(tab) => {
                        self.render_view_strip_browser_tab(tab, slot, showing_browser_tab_id, cx)
                    }
                }
            })
            .collect::<Vec<_>>();
        // The strip's own empty space moves the window, as the header beside it does; the tabs and
        // buttons stop their presses from reaching it (see `window_drag_region`).
        window_drag_region(h_flex().id("ghostex-gpui-view-tab-strip"))
            .flex_shrink_0()
            .h(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
            .w_full()
            .items_center()
            .gap(px(WORKAREA_VIEW_TAB_GAP))
            .pl(px(6.0))
            // Matches the header's trailing padding so the panel toggles at the end of this strip
            // sit exactly where they sit in the header while the panel is closed.
            .pr(px(WORKAREA_HEADER_EDGE_PADDING))
            .overflow_hidden()
            .bg(glass_clear(project_editor_shell_background_color()))
            .text_color(titlebar_text_color())
            .font_family("Inter Variable")
            .on_drag_move::<DraggedViewTab>(cx.listener(
                |this, event: &gpui::DragMoveEvent<DraggedViewTab>, _window, cx| {
                    this.clear_view_strip_drop_outside(event.bounds, event.event.position, cx);
                },
            ))
            .on_drag_move::<DraggedBrowserTab>(cx.listener(
                |this, event: &gpui::DragMoveEvent<DraggedBrowserTab>, _window, cx| {
                    this.clear_view_strip_drop_outside(event.bounds, event.event.position, cx);
                },
            ))
            .on_drop(cx.listener(|this, dragged: &DraggedViewTab, _window, cx| {
                this.handle_view_strip_gap_drop(ViewStripTabKey::View(dragged.mode), cx);
            }))
            .on_drop(
                cx.listener(|this, dragged: &DraggedBrowserTab, _window, cx| {
                    this.handle_view_strip_gap_drop(ViewStripTabKey::Browser(dragged.tab_id), cx);
                }),
            )
            .child(
                h_flex()
                    .flex_shrink_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .gap(px(WORKAREA_VIEW_TAB_GAP))
                    .child(
                        h_flex()
                            .id("ghostex-gpui-view-tab-strip-tabs")
                            .flex_shrink_1()
                            .min_w_0()
                            .h_full()
                            .items_center()
                            .gap(px(WORKAREA_VIEW_TAB_GAP))
                            .overflow_x_scroll()
                            .track_scroll(&scroll_handle)
                            .children(tab_elements),
                    )
                    .map(|row| {
                        if entries.is_empty() {
                            row.child(self.render_view_tab_strip_close_panel_button(cx))
                        } else {
                            row.child(self.render_view_tab_add_button(
                                entries.len(),
                                drop_index == Some(entries.len()),
                                cx,
                            ))
                        }
                    }),
            )
            .child(div().flex_1().min_w(px(8.0)).h_full())
            .when(active_mode.is_storybook(), |strip| {
                strip.children(self.render_storybook_controls(cx))
            })
            .when(active_mode != TitlebarMode::Source, |strip| {
                strip.child(self.render_view_tab_strip_pop_out_button(active_mode, cx))
            })
            .child(self.render_view_tab_strip_expand_button(cx))
            // Where the collapsed header puts them (WORKAREA_HEADER_PINNED_GAP).
            .child(
                div()
                    .flex_shrink_0()
                    .mt(px(WORKAREA_HEADER_HEIGHT - WORKAREA_VIEW_TAB_STRIP_HEIGHT))
                    .mr(px(WORKAREA_HEADER_PINNED_GAP))
                    .child(self.render_workarea_panel_toggles(cx)),
            )
            .into_any_element()
    }

    fn render_view_tab(
        &self,
        mode: TitlebarMode,
        slot: ViewStripTabSlot,
        hotkey_index: usize,
        active_mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pinned = slot.pinned;
        let is_active = mode == active_mode;
        let label = mode.tab_label();
        let slug = mode.element_slug();
        let sleeping =
            mode.is_project_editor_mode() && !self.project_editor_shell.is_mode_awake(mode);
        let drag_payload = DraggedViewTab { mode };
        let preview_icon = mode.tab_icon();
        let preview_label = label.clone();
        // CDXC:Hotkeys 2026-09-09 DECISION:
        // User: hovering a titlebar view shows only its shortcut at its current position, including after reordering.
        // The strip is that position now, so the shortcut follows the tab's place in it.
        let shortcut =
            gpui_configured_hotkey_label(&format!("switchTitlebarView{}", hotkey_index + 1));
        // A pinned tab has no label of its own, so its tooltip carries the name as well.
        let tooltip = match (pinned, shortcut) {
            (true, Some(shortcut)) => Some(format!("{label} ({shortcut})")),
            (true, None) => Some(label.clone()),
            (false, shortcut) => shortcut,
        };
        let tab = Self::view_strip_tab_frame(
            format!("ghostex-gpui-view-tab-{slug}"),
            VIEW_TAB_GROUP,
            WORKAREA_VIEW_TAB_WIDTH,
            slot,
        );
        self.with_view_strip_drop_target(tab, slot.index, cx)
            .when(is_active, |this| {
                this.bg(titlebar_active_segment_color())
                    .text_color(titlebar_active_text_color())
            })
            .when(!is_active, |this| {
                this.text_color(titlebar_inactive_text_color())
                    .hover(|this| {
                        this.bg(titlebar_button_hover_color())
                            .text_color(titlebar_active_text_color())
                    })
            })
            // A tab whose page is asleep still says so, so "why is this blank until I click it" has
            // an answer in the strip rather than only in the body.
            .when(sleeping && !is_active, |this| this.opacity(0.72))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.open_view_tab(mode, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_view_tab_context_menu(mode, event.position, window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    // A pinned tab closes from its menu only, never by a stray middle click.
                    if !pinned {
                        this.close_view_tab(mode, window, cx);
                    }
                }),
            )
            .on_drag(drag_payload, move |_dragged, _offset, _window, cx| {
                cx.new(|_| ViewTabDragPreview {
                    icon: preview_icon,
                    label: preview_label.clone(),
                })
            })
            .when_some(tooltip, |this, tooltip| {
                this.managed_tooltip_with_placement(
                    ManagedTooltipPlacement::WiderSide,
                    move |window, cx| {
                        titlebar_tooltip(view_tab_strip_tooltip_text(&tooltip), window, cx)
                    },
                )
            })
            .child(titlebar_svg_icon(
                mode.tab_icon(),
                WORKAREA_VIEW_TAB_ICON_SIZE,
                if is_active {
                    titlebar_active_text_color()
                } else {
                    titlebar_icon_color()
                },
            ))
            .when(!pinned, |this| {
                this.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .child(self.render_view_tab_close_button(mode, is_active, cx))
            })
            .into_any_element()
    }

    /// The box every tab in the strip shares, view or browser page: one fixed width per kind, a
    /// faint outline, and the drop line before it. A pinned tab is its icon alone.
    pub(crate) fn view_strip_tab_frame(
        id: String,
        group: &'static str,
        width: f32,
        slot: ViewStripTabSlot,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .group(group)
            .relative()
            .flex()
            .flex_shrink_0()
            .h(px(WORKAREA_VIEW_TAB_HEIGHT))
            .w(px(if slot.pinned {
                WORKAREA_VIEW_TAB_PINNED_WIDTH
            } else {
                width
            }))
            .items_center()
            .when(slot.pinned, |this| this.justify_center())
            .gap(px(6.0))
            .rounded(px(WORKAREA_VIEW_TAB_RADIUS))
            .border_1()
            .border_color(titlebar_button_border_color())
            .px(px(WORKAREA_VIEW_TAB_HORIZONTAL_PADDING))
            .text_size(px(12.5))
            .line_height(px(WORKAREA_VIEW_TAB_HEIGHT))
            .cursor_default()
            .when(slot.shows_drop_marker, |this| {
                this.child(view_strip_drop_line(slot.index == 0))
            })
    }

    /// Every tab takes every kind of tab: views and browser pages share one order, so a drag of
    /// either lands wherever the pointer is.
    pub(crate) fn with_view_strip_drop_target(
        &self,
        tab: gpui::Stateful<gpui::Div>,
        index: usize,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        tab.on_drag_move::<DraggedViewTab>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<DraggedViewTab>, _window, cx| {
                this.update_view_strip_drop_feedback(event.bounds, event.event.position, index, cx);
            },
        ))
        .on_drag_move::<DraggedBrowserTab>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<DraggedBrowserTab>, _window, cx| {
                this.update_view_strip_drop_feedback(event.bounds, event.event.position, index, cx);
            },
        ))
        .can_drop(|value, _window, _cx| {
            value.downcast_ref::<DraggedViewTab>().is_some()
                || value.downcast_ref::<DraggedBrowserTab>().is_some()
        })
        .on_drop(
            cx.listener(move |this, dragged: &DraggedViewTab, _window, cx| {
                this.handle_view_strip_drop(ViewStripTabKey::View(dragged.mode), index, cx);
            }),
        )
        .on_drop(
            cx.listener(move |this, dragged: &DraggedBrowserTab, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                this.handle_view_strip_drop(ViewStripTabKey::Browser(dragged.tab_id), index, cx);
            }),
        )
    }

    /// The close control. It is always drawn on the active tab, as the mockup does, and appears on
    /// the others while the pointer is over them, so a tab never changes width when it is hovered.
    fn render_view_tab_close_button(
        &self,
        mode: TitlebarMode,
        is_active: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(format!(
                "ghostex-gpui-view-tab-close-{}",
                mode.element_slug()
            ))
            .flex()
            .flex_shrink_0()
            .size(px(WORKAREA_VIEW_TAB_CLOSE_SIZE))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .cursor_default()
            .when(!is_active, |this| {
                this.opacity(0.0)
                    .group_hover(VIEW_TAB_GROUP, |this| this.opacity(1.0))
            })
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_view_tab(mode, window, cx);
                }),
            )
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_X,
                11.0,
                titlebar_icon_color(),
            ))
    }

    fn render_storybook_controls(&self, cx: &mut gpui::Context<Self>) -> Vec<AnyElement> {
        [
            ("annotate", BROWSER_ICON_POINTER, "Annotate with Agentation"),
            ("rebuild", BROWSER_ICON_RELOAD, "Rebuild Storybook"),
        ]
        .into_iter()
        .map(|(action, icon, label)| {
            Self::render_view_tab_strip_icon_button(action, icon, true, false)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        if action == "annotate" {
                            this.annotate_storybook(window, cx);
                        } else {
                            this.reload_titlebar_view(this.active_mode, window, cx);
                        }
                    }),
                )
                .managed_tooltip_with_placement(
                    ManagedTooltipPlacement::WiderSide,
                    move |window, cx| titlebar_tooltip(label, window, cx),
                )
                .into_any_element()
        })
        .collect()
    }

    fn render_view_tab_strip_icon_button(
        id: &'static str,
        icon: &'static str,
        enabled: bool,
        active: bool,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .flex()
            .flex_shrink_0()
            .size(px(WORKAREA_VIEW_TAB_HEIGHT))
            .items_center()
            .justify_center()
            .rounded(px(WORKAREA_VIEW_TAB_RADIUS))
            .cursor_default()
            .when(active, |this| this.bg(titlebar_active_segment_color()))
            .when(enabled, |this| {
                this.hover(|this| this.bg(titlebar_button_hover_color()))
            })
            .child(titlebar_svg_icon(
                icon,
                14.0,
                if enabled {
                    titlebar_icon_color()
                } else {
                    titlebar_disabled_text_color()
                },
            ))
    }

    /// Also the drop target past the last tab, so a tab dragged onto it lands at the end.
    fn render_view_tab_add_button(
        &self,
        tab_count: usize,
        shows_drop_marker: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let button = Self::render_view_tab_strip_icon_button(
            "ghostex-gpui-view-tab-add",
            TITLEBAR_ICON_PLUS,
            true,
            false,
        )
        .when(shows_drop_marker, |this| {
            this.relative().child(view_strip_drop_line(false))
        });
        let trigger_bounds = std::rc::Rc::new(std::cell::Cell::new(None));
        // Anywhere on the `+` is "after the last tab", so both halves mean the same index.
        button
            .on_prepaint({
                let trigger_bounds = trigger_bounds.clone();
                move |bounds, _, _| trigger_bounds.set(Some(bounds))
            })
            .on_drag_move::<DraggedViewTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedViewTab>, _window, cx| {
                    if event.bounds.contains(&event.event.position) {
                        this.set_view_strip_drop_index(Some(tab_count), cx);
                    }
                },
            ))
            .on_drag_move::<DraggedBrowserTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedBrowserTab>, _window, cx| {
                    if event.bounds.contains(&event.event.position) {
                        this.set_view_strip_drop_index(Some(tab_count), cx);
                    }
                },
            ))
            .can_drop(|value, _window, _cx| {
                value.downcast_ref::<DraggedViewTab>().is_some()
                    || value.downcast_ref::<DraggedBrowserTab>().is_some()
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedViewTab, _window, cx| {
                    this.handle_view_strip_drop(ViewStripTabKey::View(dragged.mode), tab_count, cx);
                }),
            )
            .on_drop(
                cx.listener(move |this, dragged: &DraggedBrowserTab, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.handle_view_strip_drop(
                        ViewStripTabKey::Browser(dragged.tab_id),
                        tab_count,
                        cx,
                    );
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if let Some(trigger_bounds) = trigger_bounds.get() {
                        this.show_view_tab_add_menu(trigger_bounds, window, cx);
                    }
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                titlebar_tooltip("Open a view", window, cx)
            })
    }

    /// CDXC:Workarea 2026-09-25 DECISION:
    /// User: with every tab closed the strip shows an X where the `+` was, so the panel closes
    /// right where the last tab's close button was instead of at the panel toggle in the top right
    /// corner. The picker filling the panel already opens a view, so the `+` is not needed there.
    fn render_view_tab_strip_close_panel_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let tooltip = titlebar_tooltip_label("Close side panel", "toggleViewPanel");
        Self::render_view_tab_strip_icon_button(
            "ghostex-gpui-view-tab-close-panel",
            TITLEBAR_ICON_X,
            true,
            false,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                this.close_view_panel(window, cx);
            }),
        )
        .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
            titlebar_tooltip(tooltip.clone(), window, cx)
        })
    }

    fn render_view_tab_strip_pop_out_button(
        &self,
        active_mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let enabled = self.view_pop_out_url(active_mode).is_some();
        Self::render_view_tab_strip_icon_button(
            "ghostex-gpui-view-tab-pop-out",
            TITLEBAR_ICON_EXTERNAL_LINK,
            enabled,
            false,
        )
        .when(enabled, |this| {
            this.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.pop_out_view(active_mode, cx);
                }),
            )
        })
        .managed_tooltip_with_placement(
            ManagedTooltipPlacement::Left,
            move |window, cx| {
                titlebar_tooltip(
                    if enabled {
                        "Open Externally"
                    } else {
                        "This view has no page to pop out yet"
                    },
                    window,
                    cx,
                )
            },
        )
    }

    /// CDXC:Workarea 2026-09-21 DECISION:
    /// User: a second button (first drawn as `+`, then changed to another icon on request) sits right of Expand, joined to it like the header's split buttons. Expand
    /// ("Expand side panel") folds the sessions column away; the second ("Expand side panel fully") also
    /// hides the sidebar if it is visible. Each tooltip shows its configured hotkey.
    fn render_view_tab_strip_expand_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let maximized = self.view_panel_maximized();
        let fully = self.view_panel_fully_expanded();
        // The picker can be expanded like a view (see `view_panel_maximized`).
        let enabled = self.view_panel_open();
        let cell = |id: &'static str, icon: &'static str, active: bool| {
            div()
                .id(id)
                .flex()
                .flex_shrink_0()
                .h_full()
                .w(px(WORKAREA_VIEW_TAB_HEIGHT))
                .items_center()
                .justify_center()
                .cursor_default()
                .when(active, |this| this.bg(titlebar_split_button_open_color()))
                .when(enabled, |this| {
                    this.hover(|this| this.bg(titlebar_split_button_hover_color()))
                })
                .child(titlebar_svg_icon(
                    icon,
                    14.0,
                    if enabled {
                        titlebar_icon_color()
                    } else {
                        titlebar_disabled_text_color()
                    },
                ))
        };
        let tooltip_for = |label: &str, action_id: &str| {
            if enabled {
                titlebar_tooltip_label(label, action_id)
            } else {
                "Open a view to expand it".into()
            }
        };
        let expand_tooltip = tooltip_for("Expand side panel", "expandViewPanel");
        let fully_tooltip = tooltip_for("Expand side panel fully", "expandViewPanelFully");
        let segment_radius = titlebar_split_button_segment_radius(WORKAREA_VIEW_TAB_HEIGHT);
        titlebar_split_button_frame(h_flex(), WORKAREA_VIEW_TAB_HEIGHT)
            .flex_shrink_0()
            .ml(px(2.0))
            .items_center()
            .overflow_hidden()
            .child(
                cell(
                    "ghostex-gpui-view-tab-expand",
                    if maximized {
                        TITLEBAR_ICON_ARROWS_DIAGONAL_MINIMIZE
                    } else {
                        TITLEBAR_ICON_ARROWS_DIAGONAL
                    },
                    maximized && !fully,
                )
                .rounded_l(segment_radius)
                .when(enabled, |this| {
                    this.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.toggle_view_panel_maximized(cx);
                        }),
                    )
                })
                .managed_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    move |window, cx| titlebar_tooltip(expand_tooltip.clone(), window, cx),
                ),
            )
            .child(titlebar_split_button_divider(WORKAREA_VIEW_TAB_HEIGHT))
            .child(
                cell(
                    "ghostex-gpui-view-tab-expand-fully",
                    TITLEBAR_ICON_MAXIMIZE,
                    fully,
                )
                .rounded_r(segment_radius)
                .when(enabled, |this| {
                    this.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.toggle_view_panel_fully_expanded(cx);
                        }),
                    )
                })
                .managed_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    move |window, cx| titlebar_tooltip(fully_tooltip.clone(), window, cx),
                ),
            )
    }
}

/// CDXC:Workarea 2026-09-21 DECISION:
/// User: a dragged tab shows "a line where the tab is going to drop", white in dark mode and
/// #B9D8FA in light mode. It is drawn in the gap before the tab it would land in front of (before
/// the `+` for the end of the row) and replaces the recoloured tab outline the drop used to show.
/// The first tab has no gap before it inside the scroller, so its line sits on its own leading
/// edge instead of being clipped.
fn view_strip_drop_line(first_in_row: bool) -> gpui::Div {
    div()
        .absolute()
        .top(px(2.0))
        .bottom(px(2.0))
        .left(px(if first_in_row {
            0.0
        } else {
            -(WORKAREA_VIEW_TAB_GAP + 1.0)
        }))
        .w(px(2.0))
        .rounded(px(1.0))
        .bg(view_strip_drop_line_color())
}

/// Longest tooltip, in characters, a view or browser tab may show.
const VIEW_TAB_STRIP_TOOLTIP_MAX_CHARS: usize = 60;

/// CDXC:Titlebar 2026-09-21 DECISION:
/// User: tooltips in the view and browser tabs are capped (first at 130 characters, then "cut off
/// much earlier" after a page title still filled the row, so 60), and every tooltip in the view tab
/// strip opens to the right or left of its control rather than below it, because a tooltip below
/// the strip is hidden behind the CEF page under it. A tab's tooltip opens on "whichever area is
/// wider" and must stay inside the strip's row: it uses the 20px titlebar bubble, because the stock
/// bubble with its vertical margins is taller than the strip and spilled under the page.
pub(crate) fn view_tab_strip_tooltip_text(text: &str) -> String {
    if text.chars().count() <= VIEW_TAB_STRIP_TOOLTIP_MAX_CHARS {
        return text.to_string();
    }
    let mut truncated = text
        .chars()
        .take(VIEW_TAB_STRIP_TOOLTIP_MAX_CHARS - 1)
        .collect::<String>();
    truncated.push('…');
    truncated
}
