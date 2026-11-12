//! The board's toolbar row: search, Filters, Columns, card details, then Refresh and `+ Ticket`
//! (only Filters, Columns and card details under the Beads header).

use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::Input;
use gpui_component::{Sizable as _, Size as ComponentSize};

use super::palette::KanbanPalette;
use super::state::{KanbanLoadState, KanbanPanel};
use super::widgets::{
    CONTROL_HEIGHT, CONTROL_RADIUS, KanbanButtonKind, anchor_probe, kanban_button,
};
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;

impl GhostexGpuiApp {
    fn render_native_kanban_search(
        &self,
        p: &KanbanPalette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let search = self.native_kanban.search.clone()?;
        let focused = search.read(cx).focus_handle(cx).is_focused(window);
        let has_query = !self.native_kanban.search_query.is_empty();
        let hover = p.control_hover;
        let suffix = if has_query {
            let clear_target = search.clone();
            div()
                .id("kanban-search-clear")
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(move |this, _, window, cx| {
                    clear_target.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                        input.focus(window, cx);
                    });
                    this.native_kanban.search_query.clear();
                    this.native_kanban.invalidate_derived();
                    this.native_kanban_notify(cx);
                }))
                .child(titlebar_svg_icon("titlebar/x.svg", 14.0, p.muted))
                .into_any_element()
        } else {
            titlebar_svg_icon("titlebar/search.svg", 14.0, p.muted).into_any_element()
        };
        Some(
            div()
                .w(px(256.0))
                .h(px(CONTROL_HEIGHT))
                .flex()
                .flex_none()
                .items_center()
                .pl(px(10.0))
                .pr(px(6.0))
                .rounded(px(CONTROL_RADIUS))
                .border_1()
                .border_color(if focused { p.border_strong } else { p.border })
                .bg(p.control)
                .child(
                    Input::new(&search)
                        .with_size(ComponentSize::Small)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .flex_1()
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(13.0))
                        .text_color(p.foreground)
                        .suffix(suffix),
                )
                .into_any_element(),
        )
    }

    /// `embedded_status` is set for the Board under the Beads header (`render_native_kanban_embedded`):
    /// the row then has no search, Refresh or `+ Ticket` (the header has them) and shows the
    /// board's status line on its right instead, where the title row would have shown it.
    pub(crate) fn render_native_kanban_toolbar(
        &self,
        p: &KanbanPalette,
        filter_count: usize,
        embedded_status: Option<Option<&'static str>>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let embedded = embedded_status.is_some();
        let state = &self.native_kanban;
        let loading = state.load_state == Some(KanbanLoadState::Loading);
        let columns_open = matches!(state.panel, Some(KanbanPanel::Columns(_)));
        let accent = p.accent;
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.0))
            .when(!embedded, |this| {
                this.children(self.render_native_kanban_search(p, window, cx))
            })
            .child(
                kanban_button(
                    "kanban-filters",
                    Some("titlebar/filter.svg"),
                    Some("Filters".into()),
                    KanbanButtonKind::Secondary,
                    false,
                    p,
                )
                .relative()
                .when(filter_count > 0, |this| {
                    this.child(
                        div()
                            .min_w(px(16.0))
                            .h(px(16.0))
                            .px(px(4.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(accent.opacity(0.22))
                            .text_color(accent)
                            .text_size(px(11.0))
                            .child(filter_count.to_string()),
                    )
                })
                .child(anchor_probe(self.native_kanban.anchors.filters.clone()))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_native_kanban_filters_menu(window, cx);
                })),
            )
            .child(
                kanban_button(
                    "kanban-columns",
                    Some("titlebar/layout-columns.svg"),
                    None,
                    KanbanButtonKind::Secondary,
                    false,
                    p,
                )
                .when(columns_open, |this| this.border_color(p.border_strong))
                .tooltip(|window, cx| crate::app::helpers::titlebar_tooltip("Columns", window, cx))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if columns_open {
                        this.native_kanban_close_panel(cx);
                    } else {
                        this.native_kanban_open_columns(window, cx);
                    }
                })),
            )
            .child(
                kanban_button(
                    "kanban-card-view",
                    Some("titlebar/adjustments-horizontal.svg"),
                    None,
                    KanbanButtonKind::Secondary,
                    false,
                    p,
                )
                .relative()
                .tooltip(|window, cx| {
                    crate::app::helpers::titlebar_tooltip("Card details", window, cx)
                })
                .child(anchor_probe(self.native_kanban.anchors.card_view.clone()))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_native_kanban_card_view_menu(window, cx);
                })),
            )
            .child(div().flex_1())
            .when_some(embedded_status.flatten(), |this, status| {
                this.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.0))
                        .text_color(p.muted)
                        .child(status),
                )
            })
            .when(!embedded, |this| {
                this.child(
                    kanban_button(
                        "kanban-refresh",
                        Some("titlebar/refresh.svg"),
                        None,
                        KanbanButtonKind::Ghost,
                        loading,
                        p,
                    )
                    .tooltip(|window, cx| {
                        crate::app::helpers::titlebar_tooltip("Refresh project", window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if loading {
                            return;
                        }
                        this.native_kanban_refresh_now(cx);
                    })),
                )
                .child(
                    kanban_button(
                        "kanban-new-ticket",
                        Some("titlebar/plus.svg"),
                        Some("Ticket".into()),
                        KanbanButtonKind::Secondary,
                        false,
                        p,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.native_kanban_open_new_ticket("todo", window, cx);
                    })),
                )
            })
            .into_any_element()
    }
}
