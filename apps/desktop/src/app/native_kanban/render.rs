//! The native Kanban view: title, toolbar, notice, the lanes, and the side panel.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, Context, DragMoveEvent, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, StatefulInteractiveElement as _, StyleRefinement, Styled as _, Window, div,
    px,
};
use gpui_component::scroll::Scrollbar;

use super::board_scroll::kanban_scrollbar;
use super::card::KanbanCardDrag;
use super::lane::{LANE_GAP, lane_row_min_width, lanes_in_view};
use super::palette::KanbanPalette;
use super::state::KanbanPanel;
use crate::GhostexGpuiApp;
use crate::app::model::TitlebarMode;

impl GhostexGpuiApp {
    /// CDXC:ProjectBoard 2026-09-30 DECISION:
    /// The user chose to delete the React Kanban/Automate page on 2026-09-30 because the web app it was kept for is gone, so this native GPUI board is the only board. It takes part in window glass: it paints no page fill under glass, and its lanes and cards are light washes of the chat's ink. This supersedes the 2026-09-23 decision that kept the React page as the spec for this port (the deleted apps/desktop/views/project-board/, in git history).
    ///
    /// Runs in the app's render: the title, toolbar, notice and side panel are drawn here and the
    /// lanes by the cached view (`view.rs`). `None` when the current context has no Kanban
    /// (Quick/projectless or the feature is off), which keeps the static placeholder.
    pub(crate) fn render_native_kanban(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let project = self.native_kanban_project()?;
        let view = self.native_kanban_view_entity(cx);
        let project_changed = self.native_kanban_sync_project(&project, window, cx);
        let signature = (
            crate::app::helpers::window_glass_active_in(window),
            crate::app::helpers::CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed),
        );
        let name_changed = self.native_kanban.display_name != project.display_name;
        if name_changed {
            self.native_kanban.display_name = project.display_name.clone();
            self.native_kanban_sync_window_title(cx);
        }
        let appearance_changed = self.native_kanban.appearance_signature != Some(signature);
        if appearance_changed {
            self.native_kanban.appearance_signature = Some(signature);
            self.native_kanban.palette = None;
        }
        if project_changed || name_changed || appearance_changed {
            // A notify from inside this draw would only reach the next frame.
            window.render_view_this_frame(view.entity_id());
        }
        let lanes = AnyView::from(view).cached(StyleRefinement::default().size_full());
        Some(
            div()
                .id("native-kanban-host")
                .size_full()
                .min_w_0()
                .min_h_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        if this.native_kanban_in_own_window(window) {
                            // The board is that window's only surface: no shell focus to route.
                            if let Some(focus) = this.native_kanban.focus.clone() {
                                window.focus(&focus, cx);
                            }
                            return;
                        }
                        this.focus_project_editor_surface(TitlebarMode::Kanban, window, cx);
                    }),
                )
                .child(self.render_native_kanban_board(lanes.into_any_element(), window, cx))
                .into_any_element(),
        )
    }

    /// The board's palette, resolved once per appearance change.
    fn native_kanban_palette(&mut self, window: &Window) -> KanbanPalette {
        self.native_kanban
            .palette
            .get_or_insert_with(|| KanbanPalette::current(window))
            .clone()
    }

    /// CDXC:ProjectBoard 2026-09-23 WHY:
    /// Only the lanes are inside the cached view. A gpui-component text field notifies its state on every paint, and the next draw marks every view above that state dirty; with the search box or the side panel's fields inside the cache, any app redraw (terminal output, status indicators) rebuilt and laid out every card, which pinned the main thread while Kanban was on screen. Keep text fields and other child views out of the cached lanes.
    pub(crate) fn render_native_kanban_lanes(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.native_kanban_palette(window);
        let lane_count = self.native_kanban.columns.len();
        let scroll = self.native_kanban.lanes_scroll.clone();
        let in_view = lanes_in_view(
            lane_count,
            -f32::from(scroll.offset().x),
            f32::from(window.viewport_size().width),
        );
        let lanes = self.native_kanban_lane_elements(in_view, &p, cx);
        // GPUI measures a scroll container's content from its direct children only, so the row
        // must be as wide as the lanes need; a row stretched to the viewport let the lanes spill
        // past it with nothing to scroll to.
        let row_min_width = lane_row_min_width(lane_count);
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id("native-kanban-lanes")
                    .size_full()
                    .overflow_x_scroll()
                    // A vertical wheel over a lane scrolls its cards, not the board sideways.
                    .restrict_scroll_to_axis()
                    .track_scroll(&scroll)
                    // A card dragged to the board's edge scrolls it towards the lanes off screen.
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<KanbanCardDrag>, _, cx| {
                            this.native_kanban_drag_autoscroll(
                                event.event.position,
                                event.bounds,
                                cx,
                            );
                        },
                    ))
                    .child(
                        div()
                            .flex()
                            .h_full()
                            .w_full()
                            .min_w(px(row_min_width))
                            .gap(px(LANE_GAP))
                            .children(lanes),
                    ),
            )
            .child(kanban_scrollbar(Scrollbar::horizontal(&scroll), &p))
            .into_any_element()
    }

    /// The board around the cached `lanes`: title, toolbar, notice and side panel.
    fn render_native_kanban_board(
        &mut self,
        lanes: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(focus) = self.native_kanban.focus.clone() else {
            return div().size_full().into_any_element();
        };
        let p = self.native_kanban_palette(window);
        let (nothing_matches, filter_count) = {
            let derived = self.native_kanban.derived();
            (derived.nothing_matches, derived.filter_count)
        };
        // Before `state` borrows the board: the notice may ask gxserver about Install Beads.
        let notice = self.render_native_kanban_notice(&p, cx);
        let state = &self.native_kanban;
        // The first load draws skeleton cards in the lanes instead of a status line.
        let status = (nothing_matches && !state.loading_first())
            .then_some("No tickets match the search or filters.");
        let panel = match self.native_kanban.panel.as_ref() {
            Some(KanbanPanel::Ticket(form)) => {
                Some(self.render_native_kanban_ticket_panel(form, &p, window, cx))
            }
            Some(KanbanPanel::Columns(form)) => {
                Some(self.render_native_kanban_columns_panel(form, &p, window, cx))
            }
            None => None,
        };
        let toolbar = self.render_native_kanban_toolbar(&p, filter_count, window, cx);
        let display_name = state.display_name.clone();
        div()
            .id("native-kanban")
            .track_focus(&focus)
            .on_action(cx.listener(Self::handle_native_kanban_action))
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .px(px(16.0))
            .pt(px(14.0))
            .pb(px(14.0))
            .overflow_hidden()
            .font_family(p.font.clone())
            .text_size(px(13.0))
            .text_color(p.foreground)
            .when(!p.glass, |this| this.bg(p.page))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_end()
                    .justify_between()
                    .gap(px(12.0))
                    // The title keeps its width and the status beside it truncates: with both
                    // allowed to shrink, the title collapsed and wrapped one letter per line.
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_shrink_0()
                            .max_w(gpui::relative(0.6))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.muted)
                                    .child("Project"),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(15.0))
                                    .text_color(p.foreground)
                                    .child(display_name),
                            ),
                    )
                    .when_some(status, |this, status| {
                        this.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_right()
                                .text_size(px(12.0))
                                .text_color(p.muted)
                                .child(status),
                        )
                    }),
            )
            .child(toolbar)
            .children(notice)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap(px(12.0))
                    .child(div().flex_1().min_w_0().h_full().child(lanes))
                    .children(panel),
            )
            .into_any_element()
    }
}
