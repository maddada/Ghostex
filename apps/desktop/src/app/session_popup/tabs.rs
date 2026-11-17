//! A popup's tabs: the tab bar over a pane, its right-click menu, and moving a tab within its
//! popup, to another popup or to a popup of its own (by dragging it out of the window or from the
//! menu). The bar is the view panel's tab strip (render/view_tab_strip.rs): the same tab frame,
//! colours, drop line and drag preview.

use ghostex_gx_core::SessionKey;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement as _,
    MouseButton, MouseDownEvent, MouseUpEvent, ParentElement as _, Pixels, Point,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::h_flex;

use super::{SessionPopupId, window_global_bounds};
use crate::GhostexGpuiApp;
use crate::app::consts::*;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::view_tab_strip::{ViewStripTabSlot, view_strip_drop_line};

const POPUP_TAB_GROUP: &str = "session-popup-tab";
/// The icon a tab whose agent has none of its own shows.
const TERMINAL_TAB_ICON: &str = "titlebar/terminal-2.svg";

/// A popup's tab being dragged.
#[derive(Clone)]
pub(crate) struct SessionPopupTabDrag {
    pub(crate) popup: SessionPopupId,
    pub(crate) key: SessionKey,
}

/// Where a tab moved out of its popup goes.
pub(crate) enum SessionPopupTarget {
    /// Another popup, as its last tab.
    Popup(SessionPopupId),
    /// A popup of its own at this frame (GPUI's global space).
    New(Bounds<Pixels>),
}

impl GhostexGpuiApp {
    /// The tab bar of popup `id`'s pane `pane`, led by the name of the project its session belongs
    /// to when `project` is given (a split popup's panes).
    pub(crate) fn render_session_popup_tab_strip(
        &self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        project: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(popup) = self.session_popups.get(id) else {
            return div().into_any_element();
        };
        let Some(leaf) = popup.layout.find_leaf(pane) else {
            return div().into_any_element();
        };
        let active = leaf.tab_group.active_session_id();
        let focused = popup.layout.focused_pane == pane;
        let drop_index = popup
            .strip_drop
            .filter(|(drop_pane, _)| *drop_pane == pane)
            .map(|(_, index)| index);
        let tabs = leaf
            .tab_group
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| {
                let key = popup.keys.get(&tab.session_id)?.clone();
                let slot = ViewStripTabSlot {
                    index,
                    pinned: false,
                    shows_drop_marker: drop_index == Some(index),
                };
                Some(self.render_session_popup_tab(
                    id,
                    pane,
                    key,
                    slot,
                    active == Some(tab.session_id) && focused,
                    active == Some(tab.session_id),
                    cx,
                ))
            })
            .collect::<Vec<_>>();
        let end = tabs.len();
        h_flex()
            .id(gpui::SharedString::from(format!(
                "session-popup-tab-strip-{}",
                pane.0
            )))
            .flex_shrink_0()
            .h(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
            .w_full()
            .items_center()
            .gap(px(WORKAREA_VIEW_TAB_GAP))
            .px(px(6.0))
            .overflow_hidden()
            .bg(glass_clear(project_editor_shell_background_color()))
            .text_color(titlebar_text_color())
            .font_family(crate::ui_fonts::UI_FONT)
            // Leaving the bar takes its drop line away.
            .on_drag_move::<SessionPopupTabDrag>(cx.listener(
                move |app, event: &gpui::DragMoveEvent<SessionPopupTabDrag>, _, cx| {
                    if !event.bounds.contains(&event.event.position) {
                        app.session_popup_clear_strip_drop(id, pane, cx);
                    }
                },
            ))
            // A drop on the bar's own padding takes the tab at the end.
            .on_drop(
                cx.listener(move |app, dragged: &SessionPopupTabDrag, _, cx| {
                    app.session_popup_strip_drop(id, pane, end, dragged, cx);
                }),
            )
            .when_some(project, |strip, project| {
                strip.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(140.0))
                        .px(px(6.0))
                        .truncate()
                        .text_size(px(11.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(titlebar_inactive_text_color())
                        .child(project),
                )
            })
            .child(
                h_flex()
                    .id(gpui::SharedString::from(format!(
                        "session-popup-tabs-{}",
                        pane.0
                    )))
                    .relative()
                    .flex_shrink_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .gap(px(WORKAREA_VIEW_TAB_GAP))
                    .overflow_x_scroll()
                    .children(tabs),
            )
            // The bar's empty space past the last tab takes a tab at the end.
            .child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "session-popup-tab-strip-end-{}",
                        pane.0
                    )))
                    .relative()
                    .flex_1()
                    .min_w(px(8.0))
                    .h_full()
                    .when(drop_index == Some(end), |gap| {
                        gap.child(view_strip_drop_line(end == 0))
                    })
                    .on_drag_move::<SessionPopupTabDrag>(cx.listener(
                        move |app, event: &gpui::DragMoveEvent<SessionPopupTabDrag>, _, cx| {
                            if event.bounds.contains(&event.event.position) {
                                app.session_popup_set_strip_drop(id, pane, end, cx);
                            }
                        },
                    ))
                    .on_drop(
                        cx.listener(move |app, dragged: &SessionPopupTabDrag, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            app.session_popup_strip_drop(id, pane, end, dragged, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_session_popup_tab(
        &self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        key: SessionKey,
        slot: ViewStripTabSlot,
        is_active: bool,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row_id = key.to_sidebar_session_id();
        let title = self.session_popup_title(&key);
        let agent_icon = self
            .session_popup_shell_session(&key)
            .and_then(|id| self.agents_workspace.session(id))
            .and_then(|session| session.agent_icon);
        let icon_path = agent_icon
            .and_then(workspace_tab_agent_icon_path)
            .unwrap_or(TERMINAL_TAB_ICON);
        let icon_color = if agent_icon == Some("claude") {
            gpui::rgb(0xd97757).into()
        } else if is_active {
            titlebar_active_text_color()
        } else {
            titlebar_icon_color()
        };
        let index = slot.index;
        let select_key = key.clone();
        let menu_key = key.clone();
        let close_row = row_id.clone();
        let middle_row = row_id.clone();
        let drag = SessionPopupTabDrag { popup: id, key };
        let preview_label = title.clone();
        let drag_app = cx.entity().downgrade();
        GhostexGpuiApp::view_strip_tab_frame(
            format!("session-popup-tab-{row_id}"),
            POPUP_TAB_GROUP,
            WORKAREA_VIEW_TAB_WIDTH,
            slot,
        )
        .when(is_selected, |tab| {
            tab.bg(titlebar_active_segment_color())
                .text_color(titlebar_active_text_color())
        })
        .when(!is_selected, |tab| {
            tab.text_color(titlebar_inactive_text_color()).hover(|tab| {
                tab.bg(titlebar_button_hover_color())
                    .text_color(titlebar_active_text_color())
            })
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app, _: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                app.session_popup_select(id, &select_key, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                app.show_session_popup_tab_menu(id, menu_key.clone(), event.position, window, cx);
            }),
        )
        .on_mouse_up(
            MouseButton::Middle,
            cx.listener(move |app, _: &MouseUpEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                app.close_session_popup(&middle_row, cx);
            }),
        )
        .on_drag(drag, move |drag, _, _, cx| {
            let started = drag.clone();
            let _ = drag_app.update(cx, |app, _| app.session_popups.drag = Some(started));
            cx.new(|_| ViewTabDragPreview {
                icon: icon_path,
                label: preview_label.clone(),
            })
        })
        .on_drag_move::<SessionPopupTabDrag>(cx.listener(
            move |app, event: &gpui::DragMoveEvent<SessionPopupTabDrag>, _, cx| {
                if event.bounds.contains(&event.event.position) {
                    let at =
                        workspace_tab_insertion_index(event.bounds, event.event.position, index);
                    app.session_popup_set_strip_drop(id, pane, at, cx);
                }
            },
        ))
        .on_drop(
            cx.listener(move |app, dragged: &SessionPopupTabDrag, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                let at = app
                    .session_popups
                    .get(id)
                    .and_then(|popup| popup.strip_drop)
                    .filter(|(drop_pane, _)| *drop_pane == pane)
                    .map_or(index, |(_, at)| at);
                app.session_popup_strip_drop(id, pane, at, dragged, cx);
            }),
        )
        .child(titlebar_svg_icon(
            icon_path,
            WORKAREA_VIEW_TAB_ICON_SIZE,
            icon_color,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(title),
        )
        .child(
            // The close control, drawn on the selected tab and on the others while hovered, as
            // the view strip's is; it brings the session back to its pane.
            div()
                .id(gpui::SharedString::from(format!(
                    "session-popup-tab-close-{close_row}"
                )))
                .flex()
                .flex_shrink_0()
                .size(px(WORKAREA_VIEW_TAB_CLOSE_SIZE))
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .cursor_default()
                .when(!is_selected, |close| {
                    close
                        .opacity(0.0)
                        .group_hover(POPUP_TAB_GROUP, |close| close.opacity(1.0))
                })
                .hover(|close| close.bg(titlebar_button_hover_color()))
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new("Bring Back").build(window, cx)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        app.close_session_popup(&close_row, cx);
                    }),
                )
                .child(titlebar_svg_icon(
                    TITLEBAR_ICON_X,
                    11.0,
                    titlebar_icon_color(),
                )),
        )
        .into_any_element()
    }

    /// The tab's right-click menu: Move to New Popup, Move to Popup › (the other popups), and
    /// Bring Back.
    fn show_session_popup_tab_menu(
        &mut self,
        id: SessionPopupId,
        key: SessionKey,
        position: Point<Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.session_popups.get(id) else {
            return;
        };
        let alone = popup.tab_count() <= 1;
        let others = self
            .session_popups
            .iter()
            .filter(|other| other.id != id)
            .map(|other| {
                let title = other
                    .active_key()
                    .map(|key| self.session_popup_title(&key))
                    .unwrap_or_default();
                let count = other.tab_count();
                let label = if count > 1 {
                    format!("{title} and {} more", count - 1)
                } else {
                    title
                };
                (other.id, label)
            })
            .collect::<Vec<_>>();
        let source_frame = window_global_bounds(window, cx);
        let app = cx.entity().downgrade();
        let mut menu = GpuiContextMenu::new();
        if !alone {
            let app = app.clone();
            let key = key.clone();
            menu = menu.menu_with_callback("Move to New Popup", false, move |_, cx| {
                let frame = offset_frame(source_frame);
                let _ = app.update(cx, |app, cx| {
                    app.session_popup_move_tab(key.clone(), SessionPopupTarget::New(frame), cx)
                });
            });
        }
        let mut targets = GpuiContextMenu::new();
        for (target, label) in others {
            let app = app.clone();
            let key = key.clone();
            targets = targets.menu_with_callback(label, false, move |_, cx| {
                let _ = app.update(cx, |app, cx| {
                    app.session_popup_move_tab(key.clone(), SessionPopupTarget::Popup(target), cx)
                });
            });
        }
        let row_id = key.to_sidebar_session_id();
        menu.submenu_menu("Move to Popup", targets)
            .separator()
            .menu_with_callback("Bring Back", false, move |_, cx| {
                let _ = app.update(cx, |app, cx| app.bring_back_session_popup(&row_id, cx));
            })
            .show(position, window, cx);
    }

    /// A tab dragged over pane `pane`'s bar of popup `id` would land before its tab `index`.
    pub(crate) fn session_popup_set_strip_drop(
        &mut self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        if popup.strip_drop != Some((pane, index)) || popup.drop.is_some() {
            popup.strip_drop = Some((pane, index));
            popup.drop = None;
            popup.revision += 1;
            cx.notify();
        }
    }

    /// The dragged tab left pane `pane`'s bar of popup `id`.
    pub(crate) fn session_popup_clear_strip_drop(
        &mut self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        if popup
            .strip_drop
            .is_some_and(|(drop_pane, _)| drop_pane == pane)
        {
            popup.strip_drop = None;
            popup.revision += 1;
            cx.notify();
        }
    }

    /// A tab dropped on pane `pane`'s bar of popup `id`, before its tab `index` (its end when
    /// `index` is past the last): it joins that pane there and is selected. A pane's last tab
    /// dropped on another pane's bar closes that pane, so the split goes (panes.rs).
    pub(crate) fn session_popup_strip_drop(
        &mut self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        index: usize,
        dragged: &SessionPopupTabDrag,
        cx: &mut Context<Self>,
    ) {
        self.session_popups.drag = None;
        if dragged.popup != id {
            self.session_popup_move_tab(dragged.key.clone(), SessionPopupTarget::Popup(id), cx);
            return;
        }
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        popup.strip_drop = None;
        popup.drop = None;
        popup.revision += 1;
        let Some(tab) = popup.tab_for(&dragged.key) else {
            cx.notify();
            return;
        };
        if let Some(source) = popup.layout.pane_id_for_session(tab)
            && source != pane
        {
            popup.layout.group_tab_into_pane(source, pane, tab);
        }
        if let Some(leaf) = popup.layout.find_leaf_mut(pane)
            && let Some(from) = leaf
                .tab_group
                .tabs
                .iter()
                .position(|known| known.session_id == tab)
        {
            let moved = leaf.tab_group.tabs.remove(from);
            let to = if index > from { index - 1 } else { index };
            let to = to.min(leaf.tab_group.tabs.len());
            leaf.tab_group.tabs.insert(to, moved);
            leaf.tab_group.active_tab = tab;
        }
        popup.layout.set_focused_pane(pane);
        self.session_popup_save_open_list();
        self.session_popup_settle(cx);
    }

    /// The mouse went up in popup `id` (a tab drag ends there): the drag feedback goes, and a tab
    /// released at `outside` (a point outside the window, in GPUI's global space) moves to the
    /// popup under that point, or to a popup of its own there.
    pub(crate) fn session_popup_tab_released(
        &mut self,
        id: SessionPopupId,
        outside: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let drag = self.session_popups.drag.take();
        if let Some(popup) = self.session_popups.get_mut(id)
            && (popup.drop.is_some() || popup.strip_drop.is_some())
        {
            popup.drop = None;
            popup.strip_drop = None;
            popup.revision += 1;
            cx.notify();
        }
        let (Some(drag), Some(point)) = (drag, outside) else {
            return;
        };
        if drag.popup != id {
            return;
        }
        let Some(source) = self.session_popups.get(id).map(|popup| popup.window) else {
            return;
        };
        let others = self
            .session_popups
            .iter()
            .filter(|popup| popup.id != id)
            .map(|popup| (popup.id, popup.window))
            .collect::<Vec<_>>();
        let app = cx.entity().downgrade();
        // Deferred: the release arrives inside the source window's update, and the other windows
        // are read here.
        cx.defer(move |cx| {
            let target = others.into_iter().find_map(|(other, handle)| {
                handle
                    .update(cx, |_, window, cx| window_global_bounds(window, cx))
                    .ok()
                    .filter(|bounds| bounds.contains(&point))
                    .map(|_| other)
            });
            let size = source
                .update(cx, |_, window, _| window.bounds().size)
                .unwrap_or_else(|_| gpui::size(px(super::SMALL_WIDTH), px(super::SMALL_HEIGHT)));
            let target = target.map(SessionPopupTarget::Popup).unwrap_or_else(|| {
                // The new window's tab bar comes to rest under the pointer.
                SessionPopupTarget::New(Bounds::new(point - gpui::point(px(80.0), px(52.0)), size))
            });
            let _ = app.update(cx, |app, cx| {
                app.session_popup_move_tab(drag.key, target, cx)
            });
        });
    }

    /// Moves `key` out of its popup to `target`. A popup left with nothing closes; a popup's only
    /// tab never leaves for a popup of its own (it has one).
    pub(crate) fn session_popup_move_tab(
        &mut self,
        key: SessionKey,
        target: SessionPopupTarget,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.session_popups.containing(&key) else {
            return;
        };
        let source_id = source.id;
        match target {
            SessionPopupTarget::Popup(id) if id == source_id => return,
            SessionPopupTarget::Popup(id) if self.session_popups.get(id).is_none() => return,
            SessionPopupTarget::New(_) if source.tab_count() <= 1 => return,
            _ => {}
        }
        if let Some(source) = self.session_popups.get_mut(source_id) {
            source.remove(&key);
            if source.keys.is_empty() {
                self.close_session_popup_window(source_id, cx);
            }
        }
        match target {
            SessionPopupTarget::Popup(id) => {
                if let Some(popup) = self.session_popups.get_mut(id) {
                    popup.add(key);
                }
                self.session_popup_save_open_list();
                self.session_popup_settle(cx);
                self.session_popup_activate(id, cx);
            }
            SessionPopupTarget::New(frame) => {
                self.session_popup_save_open_list();
                self.open_session_popup_window(vec![key], Some(frame), true, cx);
            }
        }
    }
}

/// A new popup's frame beside the one it came from, so both stay in sight.
fn offset_frame(frame: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(frame.origin + gpui::point(px(32.0), px(32.0)), frame.size)
}
