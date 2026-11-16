//! The `rows` block: rows with an icon or a dot, a title, a subtitle, trailing text and a badge;
//! a `project` or `session` row is Ghostex's own (refs.rs).

use ghostex_gx_protocol::extension_panel::{PanelBlock, PanelRow};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::v_flex;

use super::PanelPlace;
use super::icons::panel_icon;
use super::surface::row_frame;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::status;

/// The colour of a row's `dot`.
pub(super) fn dot_color(dot: &str) -> u32 {
    match dot {
        "green" => 0x4ade80,
        "yellow" => status::WORKING_COLOR,
        "red" => status::FAILED_COLOR,
        "blue" => 0x95d7f6,
        _ => 0x64748b,
    }
}

impl GhostexGpuiApp {
    pub(super) fn render_native_panel_rows(
        &self,
        block: &PanelBlock,
        items: &[PanelRow],
        place: &PanelPlace,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        v_flex()
            .w_full()
            .min_w_0()
            .children(items.iter().enumerate().map(|(index, row)| {
                self.render_native_panel_row(
                    row,
                    format!(
                        "ext-row-{}-{}-{}-{index}",
                        place.ui_key(),
                        block.id,
                        row.id.as_deref().unwrap_or("")
                    ),
                    place,
                    &block.id,
                    appearance,
                    cx,
                )
            }))
            .into_any_element()
    }

    /// One row, shared with the map's items.
    pub(super) fn render_native_panel_row(
        &self,
        row: &PanelRow,
        element_id: String,
        place: &PanelPlace,
        block_id: &str,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if row.project.is_some() || row.session.is_some() {
            return self
                .render_native_panel_ref_row(row, element_id, place, block_id, appearance, cx);
        }
        let scale = appearance.scale;
        let leading = match &row.dot {
            Some(dot) => Some(
                div()
                    .size(px(status::STATUS_DOT_SIZE * scale))
                    .rounded_full()
                    .bg(rgb(dot_color(dot)))
                    .into_any_element(),
            ),
            None => panel_icon(
                row.icon.as_deref(),
                &place.extension_id,
                14.0 * scale,
                appearance.chrome_icon,
            ),
        };
        let title = row.title.clone().unwrap_or_default();
        let click_place = place.clone();
        let click_block = block_id.to_string();
        let click_item = row.id.clone();
        let menu = row.menu.as_ref().map(|menu| {
            Self::native_panel_menu(place, block_id, row.id.as_deref(), Some(menu), false)
        });
        let frame = row_frame(element_id, appearance)
            .py(px(3.0 * scale))
            .when_some(leading, |frame, leading| {
                frame.child(
                    div()
                        .w(px(status::STATUS_SLOT_WIDTH * scale))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(leading),
                )
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().min_w_0().truncate().child(title))
                    .when_some(row.subtitle.clone(), |column, subtitle| {
                        column.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(11.0 * scale))
                                .text_color(appearance.muted)
                                .child(subtitle),
                        )
                    }),
            )
            .when_some(row.trailing.clone(), |frame, trailing| {
                frame.child(
                    div()
                        .flex_shrink_0()
                        .text_color(appearance.muted)
                        .child(trailing),
                )
            })
            .when_some(row.badge.clone(), |frame, badge| {
                frame.child(
                    div()
                        .flex_shrink_0()
                        .px(px(6.0 * scale))
                        .rounded_full()
                        .bg(appearance.hover)
                        .text_size(px(10.5 * scale))
                        .child(badge),
                )
            })
            .when_some(row.action.clone(), |frame, action| {
                frame.on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.run_native_panel_action(
                        &click_place,
                        &click_block,
                        click_item.as_deref(),
                        &action,
                        window,
                        cx,
                    );
                }))
            })
            .when_some(menu, |frame, menu| {
                frame.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |_, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        Self::show_native_sidebar_context_menu(
                            &menu,
                            None,
                            event.position,
                            scale,
                            window,
                            cx,
                        );
                    }),
                )
            });
        frame.into_any_element()
    }
}
