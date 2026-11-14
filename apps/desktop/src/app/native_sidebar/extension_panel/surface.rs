//! One panel surface: the sidebar's extension mode (title, tabs, blocks), a side panel view, and
//! the Projects list's sections. Blocks are drawn by their own files; this holds what they share
//! (the row frame, the heading, the hover text).

use ghostex_gx_core::extension_panel::{
    ExtensionBlockView, ExtensionPanelUiState, ExtensionSectionView, ExtensionSurfaceView,
};
use ghostex_gx_protocol::extension_panel::PanelBlockBody;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{h_flex, v_flex};
use std::sync::Arc;

use super::PanelPlace;
use super::icons::panel_icon;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;

/// A row's height before zoom: the session rows' (sessions.rs).
pub(super) const PANEL_ROW_HEIGHT: f32 = 28.0;

/// The box every clickable panel row sits in: the session rows' height, corner and hover wash.
pub(super) fn row_frame(
    id: impl Into<gpui::ElementId>,
    appearance: &SidebarAppearance,
) -> gpui::Stateful<gpui::Div> {
    let scale = appearance.scale;
    h_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .min_h(px(PANEL_ROW_HEIGHT * scale))
        .px(px(6.0 * scale))
        .gap(px(8.0 * scale))
        .rounded(px(5.0 * scale))
        .cursor_default()
        .hover(|row| row.bg(appearance.session_hover))
}

/// Hover text in the sidebar's frosted bubble.
pub(super) fn with_hover_text(
    element: gpui::Stateful<gpui::Div>,
    text: Option<String>,
    appearance: &SidebarAppearance,
) -> gpui::Stateful<gpui::Div> {
    match text.filter(|text| !text.trim().is_empty()) {
        Some(text) => element.managed_discrete_tooltip_with_placement(
            ManagedTooltipPlacement::Right,
            appearance.tooltip_delay,
            move |window, cx| {
                crate::app::native_sidebar::tooltips::sidebar_style_tooltip(
                    text.clone(),
                    window,
                    cx,
                )
            },
        ),
        None => element,
    }
}

/// A block's own heading, in the sidebar's section heading look.
fn block_heading(
    title: &str,
    trailing: Option<String>,
    appearance: &SidebarAppearance,
) -> AnyElement {
    let scale = appearance.scale;
    h_flex()
        .w_full()
        .min_w_0()
        .h(px(24.0 * scale))
        .px(px(6.0 * scale))
        .gap(px(6.0 * scale))
        .text_size(px(11.5 * scale))
        .text_color(appearance.muted)
        .font_weight(FontWeight::MEDIUM)
        .child(div().flex_1().min_w_0().truncate().child(title.to_string()))
        .when_some(trailing, |row, trailing| {
            row.child(div().flex_shrink_0().child(trailing))
        })
        .into_any_element()
}

impl GhostexGpuiApp {
    /// A whole surface: the title row (sidebar mode only), the tabs and the blocks of the tab.
    pub(crate) fn render_native_extension_surface(
        &self,
        view: &ExtensionSurfaceView,
        surface: &'static str,
        with_title: bool,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let place = PanelPlace {
            extension_id: view.extension_id.clone(),
            surface,
            session: view.session.clone(),
            revision: view.revision,
            granted_permissions: Arc::new(view.granted_permissions.clone()),
        };
        let tabs = (view.tabs.len() > 1).then(|| {
            let ui_key = place.ui_key();
            h_flex()
                .w_full()
                .px(px(4.0 * scale))
                .pb(px(6.0 * scale))
                .gap(px(4.0 * scale))
                .children(view.tabs.iter().map(|tab| {
                    let tab_id = tab.id.clone();
                    let ui_key = ui_key.clone();
                    h_flex()
                        .id(SharedString::from(format!("ext-tab-{}-{}", ui_key, tab.id)))
                        // A button, not `Role::Tab`: a Tab without a TabList parent empties the
                        // accessibility tree (CDXC:Accessibility 2026-09-30 in native_modal_kit/controls.rs).
                        .role(gpui::Role::Button)
                        .aria_label(SharedString::from(tab.title.clone()))
                        .aria_selected(tab.selected)
                        .flex_1()
                        .min_w_0()
                        .h(px(26.0 * scale))
                        .px(px(6.0 * scale))
                        .gap(px(5.0 * scale))
                        .justify_center()
                        .rounded(px(5.0 * scale))
                        .text_size(px(12.0 * scale))
                        .text_color(if tab.selected {
                            appearance.foreground
                        } else {
                            appearance.muted
                        })
                        .when(tab.selected, |button| button.bg(appearance.selected))
                        .when(!tab.selected, |button| {
                            button.hover(|button| button.bg(appearance.hover))
                        })
                        .children(panel_icon(
                            tab.icon.as_deref(),
                            &place.extension_id,
                            13.0 * scale,
                            if tab.selected {
                                appearance.foreground
                            } else {
                                appearance.chrome_icon
                            },
                        ))
                        .child(div().min_w_0().truncate().child(tab.title.clone()))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.native_sidebar
                                .extension_panel_ui
                                .selected_tabs
                                .insert(ui_key.clone(), tab_id.clone());
                            cx.notify();
                        }))
                        .into_any_element()
                }))
        });
        v_flex()
            .w_full()
            .min_w_0()
            .when(with_title, |column| {
                column.child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .h(px(30.0 * scale))
                        .px(px(8.0 * scale))
                        .gap(px(8.0 * scale))
                        .children(panel_icon(
                            view.icon.as_deref(),
                            &view.extension_id,
                            15.0 * scale,
                            appearance.chrome_icon,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .child(view.title.clone()),
                        ),
                )
            })
            .children(tabs)
            .when(view.blocks.is_empty(), |column| {
                column.child(
                    div()
                        .px(px(8.0 * scale))
                        .py(px(6.0 * scale))
                        .text_color(appearance.muted)
                        .child("Nothing here yet."),
                )
            })
            .children(
                view.blocks
                    .iter()
                    .map(|block| self.render_native_extension_block(block, &place, appearance, cx)),
            )
            .into_any_element()
    }

    /// The sections an extension adds above or below the projects.
    pub(crate) fn render_native_extension_sections(
        &self,
        sections: &[ExtensionSectionView],
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        sections
            .iter()
            .map(|section| {
                let place = PanelPlace {
                    extension_id: section.extension_id.clone(),
                    surface: "sidebar",
                    session: None,
                    revision: section.revision,
                    granted_permissions: Arc::new(section.granted_permissions.clone()),
                };
                div()
                    .pl(px(12.0 * appearance.scale))
                    .mr(px(5.0 * appearance.scale))
                    .mb(px(10.0 * appearance.scale))
                    .child(self.render_native_extension_block(
                        &section.block,
                        &place,
                        appearance,
                        cx,
                    ))
                    .into_any_element()
            })
            .collect()
    }

    /// One block with its heading.
    pub(super) fn render_native_extension_block(
        &self,
        view: &ExtensionBlockView,
        place: &PanelPlace,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let block = &view.block;
        let heading_trailing = match &block.body {
            PanelBlockBody::Checklist { items, .. } if !items.is_empty() => Some(format!(
                "{}/{}",
                items.iter().filter(|item| item.done).count(),
                items.len()
            )),
            _ => None,
        };
        let fold_key = Self::native_panel_text_key(place, &block.id);
        let ui = &self.native_sidebar.extension_panel_ui;
        // `Some(true)` while the block is folded under its heading.
        let folded = block
            .folded
            .map(|starts_folded| starts_folded != ui.flipped_folds.contains(&fold_key));
        // How many of a list's items show, and how many fold behind "N more".
        let list_len = match &block.body {
            PanelBlockBody::Rows { items } => items.len(),
            PanelBlockBody::Checklist { items, .. } => items.len(),
            _ => 0,
        };
        let lists_open = ui.expanded_lists.contains(&fold_key);
        let shown = block
            .show
            .map(|show| show as usize)
            .filter(|show| !lists_open && *show < list_len)
            .unwrap_or(list_len);
        let hidden_items = block
            .show
            .map(|show| list_len.saturating_sub(show as usize))
            .unwrap_or(0);
        let body = match &block.body {
            PanelBlockBody::Rows { items } => {
                self.render_native_panel_rows(block, &items[..shown], place, appearance, cx)
            }
            PanelBlockBody::Checklist { editable, items } => self.render_native_panel_checklist(
                block,
                &items[..shown],
                editable.unwrap_or(false),
                place,
                appearance,
                cx,
            ),
            PanelBlockBody::Map { root, groups } => self.render_native_panel_map(
                block,
                root.as_ref(),
                groups,
                view.map_as_tree,
                place,
                appearance,
                cx,
            ),
            PanelBlockBody::Progress { value, max, label } => {
                super::progress::render_panel_progress(
                    *value,
                    max.unwrap_or(100.0),
                    label.as_deref(),
                    appearance,
                )
            }
            PanelBlockBody::Text { .. } => {
                self.render_native_panel_text(view, place, appearance, cx)
            }
            PanelBlockBody::Buttons { items } => {
                self.render_native_panel_buttons(block, items, place, appearance, cx)
            }
            PanelBlockBody::Empty {
                subtitle,
                icon,
                action,
            } => self.render_native_panel_empty(
                block,
                subtitle.as_deref(),
                icon.as_deref(),
                action.as_ref(),
                place,
                appearance,
                cx,
            ),
            PanelBlockBody::Web { path, height } => {
                self.render_native_panel_web(block, path, *height, place, appearance)
            }
            PanelBlockBody::Unknown => div().into_any_element(),
        };
        // An empty block's title is its message, drawn by empty.rs.
        let heading = block
            .title
            .as_deref()
            .filter(|_| !matches!(block.body, PanelBlockBody::Empty { .. }))
            .map(|title| match folded {
                Some(is_folded) => {
                    let key = fold_key.clone();
                    let id = SharedString::from(format!("ext-fold-{fold_key}"));
                    self.render_native_panel_fold_heading(
                        id,
                        title,
                        heading_trailing,
                        is_folded,
                        appearance,
                        cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            let flipped = &mut app.native_sidebar.extension_panel_ui.flipped_folds;
                            if !flipped.remove(&key) {
                                flipped.insert(key.clone());
                            }
                            cx.notify();
                        }),
                    )
                }
                None => block_heading(title, heading_trailing, appearance),
            });
        // "N more" under a list that shows only its first items, and "Show less" once open.
        let more = (hidden_items > 0 && folded != Some(true)).then(|| {
            let key = fold_key.clone();
            div()
                .id(SharedString::from(format!("ext-list-more-{fold_key}")))
                .role(gpui::Role::Button)
                .px(px(6.0 * appearance.scale))
                .py(px(3.0 * appearance.scale))
                .text_size(px(12.0 * appearance.scale))
                .text_color(appearance.muted)
                .cursor_pointer()
                .hover(|more| more.text_color(appearance.foreground))
                .child(if lists_open {
                    "Show less".to_string()
                } else {
                    format!("{hidden_items} more")
                })
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    let lists = &mut app.native_sidebar.extension_panel_ui.expanded_lists;
                    if !lists.remove(&key) {
                        lists.insert(key.clone());
                    }
                    cx.notify();
                }))
        });
        v_flex()
            .w_full()
            .min_w_0()
            .pb(px(8.0 * appearance.scale))
            .children(heading)
            .when(folded != Some(true), |column| {
                column.child(body).children(more)
            })
            .into_any_element()
    }

    /// A folding block's heading: a chevron, the title and its count; a click folds or opens it.
    fn render_native_panel_fold_heading(
        &self,
        id: SharedString,
        title: &str,
        trailing: Option<String>,
        folded: bool,
        appearance: &SidebarAppearance,
        on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let scale = appearance.scale;
        h_flex()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(title.to_string()))
            .aria_expanded(!folded)
            .w_full()
            .min_w_0()
            .h(px(24.0 * scale))
            .px(px(6.0 * scale))
            .gap(px(4.0 * scale))
            .rounded(px(5.0 * scale))
            .cursor_pointer()
            .hover(|row| row.bg(appearance.hover))
            .text_size(px(11.5 * scale))
            .text_color(appearance.muted)
            .font_weight(FontWeight::MEDIUM)
            .child(crate::app::helpers::titlebar_svg_icon(
                if folded {
                    "titlebar/chevron-right.svg"
                } else {
                    "titlebar/chevron-down.svg"
                },
                12.0 * scale,
                appearance.chrome_icon,
            ))
            .child(div().flex_1().min_w_0().truncate().child(title.to_string()))
            .when_some(trailing, |row, trailing| {
                row.child(div().flex_shrink_0().child(trailing))
            })
            .on_click(on_click)
            .into_any_element()
    }

    /// Whether the inline field of `kind` (`extensionItem` or `extensionAdd`) is open on this
    /// item or block.
    pub(super) fn native_panel_editor(
        &self,
        kind: &str,
        place: &PanelPlace,
        block_id: &str,
        item_id: Option<&str>,
    ) -> Option<&crate::app::native_sidebar::rename::SidebarNameEditor> {
        let id = super::actions::editor_id(place, block_id, item_id);
        self.native_sidebar
            .name_editor
            .as_ref()
            .filter(|editor| editor.kind == kind && editor.id == id)
    }

    /// The ui-state key of a text block's "Show more".
    pub(super) fn native_panel_text_key(place: &PanelPlace, block_id: &str) -> String {
        format!(
            "{}|{}",
            ExtensionPanelUiState::surface_key(&place.extension_id, place.surface),
            block_id
        )
    }
}
