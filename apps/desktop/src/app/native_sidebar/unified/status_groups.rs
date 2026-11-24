//! Group sessions by tags: a group's set-off header, its cards, and with Group by Project on its
//! projects' headings. The groups are gx-core's (`unified_sidebar/status_groups.rs`); a header is a
//! heading, never a control: no chevron, no fold, no drag.

use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement, Rgba,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::{ElementExt as _, h_flex, v_flex};
use serde_json::Value;

use ghostex_gx_core::StatusGroup;
use ghostex_gx_core::unified_sidebar::UnifiedGroupHead;

use super::cards::{repo_tile, show_project_icons};
use super::model::{UnifiedStatusGroupView, UnifiedStatusProjectView};
use crate::GhostexGpuiApp;
use crate::app::helpers::{chrome_color, gpui_sidebar_command_icon_asset_path};
use crate::app::native_sidebar::appearance::SidebarAppearance;

/// The symbol and colour of a status group in the current appearance (gx-core
/// `StatusGroup::symbol` and `StatusGroup::colors`, which the Settings editor's status rows draw too).
fn status_group_look(group: StatusGroup) -> (&'static str, Rgba) {
    let (dark, light) = group.colors();
    (group.symbol(), chrome_color(dark, light))
}

/// A hand-tag or No tag group's colour, whatever the tag's own: the Idle neutral.
fn neutral() -> Rgba {
    let (dark, light) = StatusGroup::Idle.colors();
    chrome_color(dark, light)
}

/// "1 card", "3 cards".
fn cards_word(count: usize) -> String {
    if count == 1 {
        "1 card".to_string()
    } else {
        format!("{count} cards")
    }
}

impl GhostexGpuiApp {
    /// One group: its header, then its cards or its projects' headings with their cards.
    pub(super) fn render_native_unified_status_group(
        &self,
        group: &UnifiedStatusGroupView,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let (key, label, icon, color): (String, String, gpui::SharedString, Rgba) =
            match &group.head {
                UnifiedGroupHead::Status(status) => {
                    let (icon, color) = status_group_look(*status);
                    (
                        status.id().to_string(),
                        status.label().to_string(),
                        icon.into(),
                        color,
                    )
                }
                UnifiedGroupHead::Tag {
                    id,
                    label,
                    presentation,
                } => (
                    id.clone(),
                    label.clone(),
                    gpui_sidebar_command_icon_asset_path(Some(
                        presentation
                            .as_ref()
                            .map_or("tag", |look| look.icon.as_str()),
                    )),
                    neutral(),
                ),
            };
        let header = h_flex()
            .id(format!("native-unified-status-header-{key}"))
            .role(gpui::Role::Heading)
            .aria_label(format!("{label}, {}", cards_word(group.count)))
            .w_full()
            .min_h(px(40.0 * scale))
            .px(px(12.0 * scale))
            .py(px(8.0 * scale))
            .gap(px(8.0 * scale))
            .bg(color.opacity(0.08))
            .border_t(px(1.0))
            .border_b(px(1.0))
            .border_color(color.opacity(0.26))
            .text_size(px(13.5 * scale))
            .text_color(appearance.foreground)
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(16.0 * scale))
                    .flex_shrink_0()
                    .text_color(color),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_weight(gpui::FontWeight(650.0))
                    .child(label),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(group.count.to_string()),
            );
        v_flex()
            .w_full()
            .pt(px(8.0 * scale))
            .pb(px(4.0 * scale))
            .child(header)
            .children(
                group
                    .cards
                    .iter()
                    .map(|card| self.render_native_unified_card(card, hud, appearance, cx)),
            )
            .children(group.projects.iter().map(|project| {
                self.render_native_unified_status_project(project, &key, hud, appearance, cx)
            }))
            .into_any_element()
    }

    /// A project's heading inside a group: its tile, name, new-session control and count, under the
    /// group's header and weaker than it. It neither folds nor drags (the project order is arranged
    /// while grouping is off); its right-click menu is the project header's.
    ///
    /// CDXC:Sidebar 2026-11-24 DECISION:
    /// User, after the project header's "+" became a chat split button with a menu: a project heading inside a status group takes the same control as the project headers outside the groups (`render_native_unified_project_new_control` in groups.rs), acting on that project, so the two kinds of heading in one list do not differ. It supersedes the heading's own "+". The control shows while the heading is hovered, as the "+" did.
    fn render_native_unified_status_project(
        &self,
        project: &UnifiedStatusProjectView,
        group_key: &str,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let key = format!("{group_key}|{}", project.repo_key);
        let hover_group = format!("native-unified-status-project-{key}");
        let menu_key = project.repo_key.clone();
        let owner = crate::app::native_sidebar::menus::MenuOwner::default();
        let record_owner = crate::app::native_sidebar::menus::record_menu_owner(&owner);
        let heading = h_flex()
            .id(format!("native-unified-status-project-heading-{key}"))
            .group(hover_group.clone())
            .role(gpui::Role::Heading)
            .aria_label(format!(
                "{}, {}",
                project.label,
                cards_word(project.cards.len())
            ))
            .ml(px(11.0 * scale))
            .mr(px(3.0 * scale))
            .h(px(30.0 * scale))
            .px(px(6.0 * scale))
            .gap(px(6.0 * scale))
            .text_size(px(13.0 * scale))
            .text_color(appearance.muted)
            .children(
                project
                    .cards
                    .first()
                    .map(|card| repo_tile(card, appearance, show_project_icons(hud), false)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(project.label.clone()),
            )
            .child(
                self.render_native_unified_project_new_control(
                    &project.repo_key,
                    &project.label,
                    "native-unified-status-project",
                    &key,
                    &owner,
                    appearance,
                    cx,
                )
                .invisible()
                .group_hover(hover_group, |control| control.visible()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(gpui::FontWeight::LIGHT)
                    .child(project.cards.len().to_string()),
            )
            .on_prepaint(move |bounds, window, app| record_owner(bounds, window, app))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let Some(menu) = app.unified_group_menu(&menu_key) else {
                        return;
                    };
                    Self::show_native_sidebar_context_menu(
                        &menu,
                        owner.get(),
                        event.position,
                        scale,
                        window,
                        cx,
                    );
                }),
            );
        v_flex()
            .w_full()
            .child(heading)
            .children(
                project
                    .cards
                    .iter()
                    .map(|card| self.render_native_unified_card(card, hud, appearance, cx)),
            )
            .into_any_element()
    }
}
