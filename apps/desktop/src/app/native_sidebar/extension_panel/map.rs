//! The `map` block: a root and its groups of items. Wide (a side panel), the groups are cards in
//! two columns under the root; narrow (the sidebar, gx-core `MAP_TREE_BELOW_WIDTH`), an indented
//! tree: the root, each group, and its items.

use ghostex_gx_protocol::extension_panel::{MapGroup, MapRoot, PanelBlock};
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, FontWeight, IntoElement, ParentElement, Styled, div, px, relative};
use gpui_component::{h_flex, v_flex};

use super::PanelPlace;
use super::icons::panel_icon;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;

/// A tree level's indent before zoom: a session row's under its project.
const TREE_INDENT: f32 = 14.0;

impl GhostexGpuiApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_native_panel_map(
        &self,
        block: &PanelBlock,
        root: Option<&MapRoot>,
        groups: &[MapGroup],
        as_tree: bool,
        place: &PanelPlace,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let root_row = root.map(|root| {
            h_flex()
                .w_full()
                .min_w_0()
                .h(px(28.0 * scale))
                .px(px(6.0 * scale))
                .gap(px(8.0 * scale))
                .children(panel_icon(
                    root.icon.as_deref(),
                    &place.extension_id,
                    15.0 * scale,
                    appearance.chrome_icon,
                ))
                // The title keeps its words; the subtitle gives way on a narrow sidebar.
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w(relative(0.7))
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .child(root.title.clone()),
                )
                .when_some(root.subtitle.clone(), |row, subtitle| {
                    row.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(appearance.muted)
                            .child(subtitle),
                    )
                })
                .into_any_element()
        });
        let indent = if root.is_some() {
            TREE_INDENT * scale
        } else {
            0.0
        };
        let groups: Vec<AnyElement> = groups
            .iter()
            .enumerate()
            .map(|(group_index, group)| {
                let heading = h_flex()
                    .w_full()
                    .min_w_0()
                    .h(px(26.0 * scale))
                    .px(px(6.0 * scale))
                    .gap(px(8.0 * scale))
                    .text_color(appearance.muted)
                    .children(panel_icon(
                        group.icon.as_deref(),
                        &place.extension_id,
                        14.0 * scale,
                        appearance.chrome_icon,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(group.title.clone()),
                    )
                    .child(div().flex_shrink_0().text_size(px(11.0 * scale)).child(
                        group.subtitle.clone().unwrap_or_else(|| {
                            let noun = if group.items.iter().all(|item| item.project.is_some()) {
                                "project"
                            } else {
                                "item"
                            };
                            match group.items.len() {
                                1 => format!("1 {noun}"),
                                count => format!("{count} {noun}s"),
                            }
                        }),
                    ));
                let items = group.items.iter().enumerate().map(|(index, row)| {
                    self.render_native_panel_row(
                        row,
                        format!(
                            "ext-map-{}-{}-{group_index}-{index}",
                            place.ui_key(),
                            block.id
                        ),
                        place,
                        &block.id,
                        appearance,
                        cx,
                    )
                });
                if as_tree {
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .pl(px(indent))
                        .child(heading)
                        .child(
                            v_flex()
                                .w_full()
                                .min_w_0()
                                .pl(px(TREE_INDENT * scale))
                                .children(items),
                        )
                        .into_any_element()
                } else {
                    v_flex()
                        .min_w_0()
                        .p(px(4.0 * scale))
                        .rounded(px(8.0 * scale))
                        .border_1()
                        .border_color(appearance.foreground.opacity(0.08))
                        .child(heading)
                        .children(items)
                        .into_any_element()
                }
            })
            .collect();
        let body = if as_tree {
            v_flex()
                .w_full()
                .min_w_0()
                .children(groups)
                .into_any_element()
        } else {
            // Two cards a line; a lone last card keeps a card's width.
            let mut lines = Vec::new();
            let mut groups = groups.into_iter();
            while let Some(first) = groups.next() {
                let second = groups.next();
                lines.push(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_start()
                        .gap(px(8.0 * scale))
                        .child(div().flex_1().min_w_0().child(first))
                        .child(div().flex_1().min_w_0().children(second))
                        .into_any_element(),
                );
            }
            v_flex()
                .w_full()
                .min_w_0()
                .gap(px(8.0 * scale))
                .pt(px(4.0 * scale))
                .children(lines)
                .into_any_element()
        };
        v_flex()
            .w_full()
            .min_w_0()
            .children(root_row)
            .child(body)
            .into_any_element()
    }
}
