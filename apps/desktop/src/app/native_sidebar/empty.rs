use super::{appearance::SidebarAppearance, model::NativeSidebarSnapshot};
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, relative, rgb,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

/// The bordered pill the empty list's action draws.
fn empty_action_button(
    id: &'static str,
    label: impl Into<gpui::SharedString>,
    icon: Option<&'static str>,
    appearance: &SidebarAppearance,
) -> gpui::Stateful<gpui::Div> {
    let scale = appearance.scale;
    h_flex()
        .id(id)
        .h(px(30.0 * scale))
        .pl(px(10.0 * scale))
        .pr(px(12.0 * scale))
        .gap(px(6.0 * scale))
        .rounded(px(8.0 * scale))
        .border_1()
        .border_color(appearance.foreground.opacity(0.22))
        .text_color(appearance.foreground.opacity(0.8))
        .text_size(px(12.5 * scale))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|row| row.bg(appearance.hover))
        .when_some(icon, |row, icon| {
            row.child(titlebar_svg_icon(icon, 14.0 * scale, appearance.foreground))
        })
        .child(label.into())
}

impl GhostexGpuiApp {
    /// The row that brings the projects with no sessions back, and takes them away again.
    ///
    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User (Q851, "show all row"): with Hide Projects Without Sessions on, one row at the end of the Project sidebar turns the full list back on, in the user's own order, until it is pressed again. It only shows while there are such projects. The reporter asked for this: "An option to hide projects with no active sessions ... plus a Show all projects toggle". The switch is off by default and lives in Settings and Sort & Filter.
    pub(crate) fn render_native_show_all_projects_row(
        &self,
        row: ghostex_gx_core::ShowAllProjectsRow,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let label = if row.showing {
            "Hide projects without sessions".to_string()
        } else {
            format!("Show all projects ({} hidden)", row.empty_projects)
        };
        h_flex()
            .id("native-sidebar-show-all-projects")
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .ml(px(18.0 * scale))
            .mr(px(5.0 * scale))
            .mt(px(4.0 * scale))
            .h(px(28.0 * scale))
            .px(px(6.0 * scale))
            .gap(px(6.0 * scale))
            .rounded(px(5.0 * scale))
            .text_size(px(13.0 * scale))
            .text_color(appearance.muted)
            .cursor_pointer()
            .hover(|row| row.bg(appearance.hover))
            .child(titlebar_svg_icon(
                if row.showing {
                    "titlebar/eye-off.svg"
                } else {
                    "titlebar/eye.svg"
                },
                14.0 * scale,
                appearance.muted,
            ))
            .child(div().min_w_0().truncate().child(label))
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "sidebarAction", "action": "showAllProjects"}),
                    cx,
                );
            }))
            .into_any_element()
    }

    pub(crate) fn render_native_sidebar_empty(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let state = &snapshot.empty_state;
        if state["loading"] == true {
            return v_flex()
                .px(px(18.0 * scale))
                .py(px(8.0 * scale))
                .gap(px(8.0 * scale))
                .children([72, 56, 64, 48, 68, 52, 60].map(|width| {
                    h_flex()
                        .h(px(28.0 * scale))
                        .gap(px(10.0 * scale))
                        .child(
                            div()
                                .size(px(16.0 * scale))
                                .rounded(px(4.0 * scale))
                                .bg(appearance.foreground),
                        )
                        .child(
                            div()
                                .w(relative(width as f32 / 100.0))
                                .h(px(12.0 * scale))
                                .rounded(px(4.0 * scale))
                                .bg(appearance.foreground),
                        )
                        .with_throttled_animation(
                            format!("sidebar-loading-{width}"),
                            std::time::Duration::from_millis(1400),
                            |row, progress| {
                                row.opacity(0.1 + 0.1 * (std::f32::consts::PI * progress).sin())
                            },
                        )
                }))
                .into_any_element();
        }
        // `error` is the wait for this computer's Ghostex service: the core's copy says it is
        // connecting, the stores retry on their own, and the button (Try now) retries at once.
        let error = state["error"] == true;
        let add = state["canAddProject"] == true;
        let action = if error { "loadSessions" } else { "addProject" };
        let empty_menu = json!([{ "label": "Add Project", "icon": "plus", "command": {"type": "sidebarAction", "action": "addProject"} }]);
        let detail = state["detail"].as_str().unwrap_or_default().to_owned();
        let clear_tags: Vec<String> = state["clearTagFilters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tag| tag.as_str().map(str::to_owned))
            .collect();
        let clearing = !clear_tags.is_empty();
        let add = add && !clearing;
        let action_label = if error {
            state["actionLabel"]
                .as_str()
                .unwrap_or("Try now")
                .to_owned()
        } else {
            "Add Project".to_owned()
        };
        v_flex()
            .id("native-sidebar-empty")
            .items_start()
            .flex_1()
            .w_full()
            .mt(px(8.0 * scale))
            .pl(px(18.0 * scale))
            .pr(px(18.0 * scale))
            .text_color(appearance.muted)
            .font_weight(FontWeight::MEDIUM)
            .child(state["copy"].as_str().unwrap_or_default().to_owned())
            .when(!detail.is_empty(), |column| {
                column.child(
                    div()
                        .mt(px(4.0 * scale))
                        .text_size(px(12.5 * scale))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(appearance.muted.opacity(0.8))
                        .child(detail),
                )
            })
            .when(clearing, |column| {
                column.child(
                    empty_action_button(
                        "native-sidebar-empty-clear",
                        "Clear filter",
                        None,
                        appearance,
                    )
                    .mt(px(12.0 * scale))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        for tag in &clear_tags {
                            app.dispatch_native_sidebar_ui(
                                json!({"type": "toggleTagFilter", "tag": tag}),
                                cx,
                            );
                        }
                    })),
                )
            })
            .when(error || add, |column| {
                column.child(
                    empty_action_button(
                        "native-sidebar-empty-action",
                        action_label,
                        (!error).then_some("titlebar/plus.svg"),
                        appearance,
                    )
                    .mt(px(12.0 * scale))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.dispatch_native_sidebar_ui(
                            json!({"type": "sidebarAction", "action": action}),
                            cx,
                        );
                    })),
                )
            })
            .when(add, |column| {
                column.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    cx.stop_propagation();
                    Self::show_native_sidebar_menu(&empty_menu, event.position, scale, window, cx);
                })
            })
            .into_any_element()
    }
}
