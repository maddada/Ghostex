//! The Board columns panel (`BoardColumnsDialog`): the six built-in lanes are listed but fixed; the
//! board's own extra statuses can be added, reordered, and deleted once empty.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::model::{
    board_status_label, managed_board_column_names, move_board_column, remove_board_column,
};
use super::palette::KanbanPalette;
use super::state::KanbanColumnsForm;
use super::widgets::{KanbanButtonKind, LANE_RADIUS, kanban_button, section_title, text_field};
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;

impl GhostexGpuiApp {
    pub(crate) fn render_native_kanban_columns_panel(
        &self,
        form: &KanbanColumnsForm,
        p: &KanbanPalette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = &self.native_kanban;
        let managed = managed_board_column_names(&state.column_config);
        let busy = form.busy;
        let row = || {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(34.0))
                .px(px(10.0))
                .rounded(px(8.0))
                .bg(p.card)
                .border_1()
                .border_color(p.border)
        };
        let builtin_rows = state
            .columns
            .iter()
            .filter(|column| !managed.contains(&column.key))
            .map(|column| {
                row()
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(13.0))
                            .text_color(p.foreground.opacity(0.9))
                            .child(column.label.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.faint)
                            .child("Built-in"),
                    )
            })
            .collect::<Vec<_>>();
        let managed_count = managed.len();
        let managed_rows = managed
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let cards = state
                    .tickets
                    .iter()
                    .filter(|ticket| ticket.board_status == *name)
                    .count();
                let icon_button = |suffix: &str, icon: &'static str, disabled: bool| {
                    kanban_button(
                        ElementId::Name(format!("kanban-column-{suffix}-{name}").into()),
                        Some(icon),
                        None,
                        KanbanButtonKind::Ghost,
                        disabled,
                        p,
                    )
                };
                let (up, down, delete) = (name.clone(), name.clone(), name.clone());
                row()
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(13.0))
                            .text_color(p.foreground.opacity(0.9))
                            .child(board_status_label(name, &state.columns)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.faint)
                            .child(if cards == 1 {
                                "1 card".to_string()
                            } else {
                                format!("{cards} cards")
                            }),
                    )
                    .child(
                        icon_button("up", "titlebar/arrow-up.svg", busy || index == 0).on_click(
                            cx.listener(move |this, _, _, cx| {
                                if busy || index == 0 {
                                    return;
                                }
                                let config =
                                    move_board_column(&this.native_kanban.column_config, &up, -1);
                                this.native_kanban_write_column_config(config, cx);
                            }),
                        ),
                    )
                    .child(
                        icon_button(
                            "down",
                            "titlebar/arrow-down.svg",
                            busy || index + 1 == managed_count,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if busy || index + 1 == managed_count {
                                return;
                            }
                            let config =
                                move_board_column(&this.native_kanban.column_config, &down, 1);
                            this.native_kanban_write_column_config(config, cx);
                        })),
                    )
                    .child(
                        icon_button("delete", "titlebar/trash.svg", busy || cards > 0)
                            .when(cards > 0, |this| {
                                this.tooltip(|window, cx| {
                                    crate::app::helpers::titlebar_tooltip(
                                        "Move its cards out before deleting",
                                        window,
                                        cx,
                                    )
                                })
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if busy || cards > 0 {
                                    return;
                                }
                                let config =
                                    remove_board_column(&this.native_kanban.column_config, &delete);
                                this.native_kanban_write_column_config(config, cx);
                            })),
                    )
            })
            .collect::<Vec<_>>();
        let hover = p.control_hover;
        div()
            .id("kanban-columns-panel")
            .w(px(360.0))
            .flex_none()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .rounded(px(LANE_RADIUS))
            .bg(p.panel)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .pt(px(14.0))
                    .pb(px(10.0))
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(div().text_size(px(15.0)).text_color(p.foreground).child("Board columns"))
                            .child(div().text_size(px(12.0)).text_color(p.muted).child(
                                "Extra columns come from this board's Beads status config. The six built-in lanes are fixed.",
                            )),
                    )
                    .child(
                        div()
                            .id("kanban-columns-panel-close")
                            .size(px(26.0))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover))
                            .on_click(cx.listener(|this, _, _, cx| this.native_kanban_close_panel(cx)))
                            .child(titlebar_svg_icon("titlebar/x.svg", 14.0, p.muted)),
                    ),
            )
            .child(
                div()
                    .id("kanban-columns-panel-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .p(px(16.0))
                            .children(builtin_rows)
                            .children(managed_rows)
                            .child(div().h(px(10.0)))
                            .child(section_title("Add column", p))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(div().flex_1().child(text_field(&form.name, p, window, cx)))
                                    .child(
                                        kanban_button(
                                            "kanban-column-add",
                                            None,
                                            Some("Add".into()),
                                            KanbanButtonKind::Secondary,
                                            busy,
                                            p,
                                        )
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.native_kanban_add_column(window, cx);
                                        })),
                                    ),
                            )
                            .when_some(form.error.clone(), |this, error| {
                                this.child(div().text_size(px(12.0)).text_color(p.danger).child(error))
                            }),
                    ),
            )
            .into_any_element()
    }
}
