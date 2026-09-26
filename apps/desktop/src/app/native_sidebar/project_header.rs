use super::drag::SidebarDropTarget;
use super::drag_source::SidebarDragSource;
use super::{appearance::SidebarAppearance, drag::SidebarDrag, model::NativeSidebarGroup};
use crate::{
    GhostexGpuiApp,
    app::{consts::*, helpers::*},
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, img, px,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{ElementExt as _, h_flex};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

impl GhostexGpuiApp {
    /// CDXC:Projects 2026-09-17 DECISION:
    /// User: expanded projects show the same right-facing chevron rotated down, shifted 2px left.
    pub(crate) fn render_native_project_header(
        &self,
        group: &NativeSidebarGroup,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let id = group.group_id.clone();
        let drop_position = self
            .native_sidebar_drop_position("targetGroupId", &id)
            .or_else(|| self.native_sidebar_drop_position("targetId", &id));
        let hover_id = id.clone();
        let drag_id = id.clone();
        let chevron_id = id.clone();
        let menu = group.menu.clone();
        let scale = appearance.scale;
        let hovered = self.native_sidebar.hovered_group.as_ref() == Some(&id);
        let tooltip_span = self
            .native_sidebar
            .group_bounds
            .get(&id)
            .map(|header| super::tooltips::SidebarTooltipSpan {
                left: header.left().as_f32(),
                right: header.right().as_f32(),
            })
            .unwrap_or_else(|| {
                super::tooltips::SidebarTooltipSpan::sidebar(self.sidebar_width, scale)
            });
        let icon_image = group
            .project_context
            .as_ref()
            .and_then(|project| {
                project
                    .get("iconDataUrl")
                    .or_else(|| project.get("discoveredIconDataUrl"))
            })
            .and_then(Value::as_str)
            .and_then(super::images::sidebar_image);
        let dragged = SidebarDrag {
            kind: "group",
            preview: super::drag::SidebarDragPreview::Row(super::row_drag::RowDragPreview {
                identity: super::row_drag::RowDragIdentity::Project {
                    image: icon_image.clone(),
                    show_icon: hud["settings"]["showProjectIcons"].as_bool() != Some(false),
                },
                appearance: appearance.clone(),
                width: px(0.0),
                pointer_x: px(0.0),
            }),
            id: id.clone(),
            title: group.title.clone(),
            scale,
        };
        let tooltip = group.title_tooltip.clone();
        let title = if let Some(editor) = self
            .native_sidebar
            .name_editor
            .as_ref()
            .filter(|editor| editor.kind == "group" && editor.id == id)
        {
            div()
                .flex_1()
                .min_w_0()
                .on_action(
                    cx.listener(|app, _: &gpui_component::input::Escape, _, cx| {
                        cx.stop_propagation();
                        app.finish_native_sidebar_rename(false, cx);
                    }),
                )
                .child(gpui_component::input::Input::new(&editor.input).h(px(24.0 * scale)))
                .into_any_element()
        } else {
            div()
                .id(format!("native-project-title-{id}"))
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .font_weight(gpui::FontWeight::LIGHT)
                .child(div().min_w_0().truncate().child(group.title.clone()))
                .when_some(tooltip, |title, tooltip| {
                    title.when(
                        self.native_sidebar.pointer_inside
                            && self.native_sidebar.menu.is_none()
                            && !cx.has_active_drag(),
                        |row| {
                            row.managed_discrete_tooltip_with_placement(
                                tooltip_span.placement(),
                                appearance.tooltip_delay,
                                move |window, cx| {
                                    super::tooltips::sidebar_tooltip(
                                        tooltip.clone(),
                                        tooltip_span,
                                        scale,
                                        window,
                                        cx,
                                    )
                                },
                            )
                        },
                    )
                })
                .into_any_element()
        };
        let mut actions = (*group.header_actions).clone();
        if group.show_list_toggle {
            actions.insert(0, json!({ "label": if group.expanded { "Compact" } else { "Full" }, "icon": if group.expanded { "chevron-up" } else { "chevron-down" }, "command": { "type": "toggleList", "groupId": group.storage_id } }));
        }
        // CDXC:Projects 2026-09-18 DECISION:
        // User: the project header row has the same insets, side padding, rounding, and hover fill as a session card.
        h_flex()
            .id(format!("native-sidebar-project-{id}"))
            .role(gpui::Role::TreeItem)
            .aria_label(group.title.clone())
            .aria_expanded(!group.collapsed)
            .relative()
            .h(px(30.0 * scale))
            .mx(px(3.0 * scale))
            .pl(px(5.0 * scale))
            .pr(px(6.0 * scale))
            .gap(px(10.0 * scale))
            .rounded(px(5.0 * scale))
            .cursor_default()
            .when(group.is_stale, |row| row.opacity(0.55))
            .hover(|row| row.bg(appearance.session_hover))
            .child(self.native_sidebar.header_hover.probe(hover_id))
            // CDXC:Projects 2026-09-19 DECISION:
            // User: clicking the chevron left of the project header expands/collapses the project, same as clicking the header.
            .child(
                div()
                    .id(format!("native-project-chevron-{id}"))
                    .role(gpui::Role::Button)
                    .aria_label(if group.collapsed {
                        "Expand project"
                    } else {
                        "Collapse project"
                    })
                    .absolute()
                    .left(px(-18.0 * scale))
                    .top(px(7.0 * scale))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.dispatch_native_sidebar_ui(
                            json!({"type": "toggleGroup", "groupId": chevron_id}),
                            cx,
                        );
                    }))
                    .child(
                        gpui::svg()
                            .path(COMMAND_ICON_CHEVRON_RIGHT)
                            .size(px(16.0 * scale))
                            .text_color(appearance.muted)
                            .with_transformation(gpui::Transformation::rotate(gpui::percentage(
                                if group.collapsed { 0.0 } else { 0.25 },
                            ))),
                    ),
            )
            .when(
                hud["settings"]["showProjectIcons"].as_bool() != Some(false),
                |row| {
                    row.child(match icon_image {
                        Some(image) => img(image)
                            .size(px(16.0 * scale))
                            .flex_shrink_0()
                            .into_any_element(),
                        // CDXC:Icons 2026-09-24 DECISION: User: when a project has no favicon, show a square with the first letter of its name instead of the folder icon.
                        None => div()
                            .size(px(16.0 * scale))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(3.0 * scale))
                            .bg(appearance.muted.opacity(0.12))
                            .text_color(appearance.muted)
                            .text_size(px(10.0 * scale))
                            .line_height(px(16.0 * scale))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(
                                group
                                    .title
                                    .trim()
                                    .chars()
                                    .next()
                                    .map(|letter| letter.to_uppercase().to_string())
                                    .unwrap_or_else(|| "?".to_owned()),
                            )
                            .into_any_element(),
                    })
                },
            )
            .when(group.collapsed && group.is_active, |row| {
                row.bg(appearance.selected)
                    .child(super::decorations::selected_outline(appearance))
            })
            .when_some(drop_position, |row, position| {
                row.child(super::drag::drop_line(position, scale))
            })
            .child(title)
            .when(!hovered, |row| {
                row.children(super::project_status::project_status(
                    group, hud, appearance,
                ))
            })
            .when(hovered, |row| {
                row.child(
                    h_flex()
                        .id(format!("native-project-actions-{id}"))
                        .h_full()
                        .gap(px(2.0 * scale))
                        .flex_shrink_0()
                        // Same as the session card's buttons (hover_actions.rs): reaching them closes the title's tooltip.
                        .on_hover(|hovered, window, cx| {
                            if *hovered {
                                gpui_component::Root::hide_tooltip(window, cx);
                            }
                        })
                        .children(actions.into_iter().enumerate().map(|(index, item)| {
                            let label = item["label"].as_str().unwrap_or("").to_owned();
                            let command = item.get("command").cloned();
                            let children = item.get("children").cloned();
                            let trigger = Rc::new(Cell::new(None::<gpui::Bounds<gpui::Pixels>>));
                            // CDXC:AgentLauncher 2026-09-18 DECISION:
                            // User: no gap between the last-used agent button and the Select agent chevron; the two halves join into one split button like the React header (24px action, 17px chevron, only the outer corners rounded).
                            let split = item["split"].as_str();
                            let image = item["imageDataUrl"].as_str().and_then(|value| {
                                super::images::agent_image(
                                    value,
                                    item["agentIcon"].as_str(),
                                    appearance.light,
                                )
                            });
                            let glyph = match image {
                                Some(image) => img(image).size(px(14.0 * scale)).into_any_element(),
                                None => gpui::svg()
                                    .path(gpui_sidebar_command_icon_asset_path(
                                        item["icon"].as_str(),
                                    ))
                                    .size(px(14.0 * scale))
                                    .text_color(appearance.muted)
                                    .into_any_element(),
                            };
                            div()
                                .id(format!("native-project-action-{id}-{index}"))
                                .role(gpui::Role::Button)
                                .aria_label(label.clone())
                                .h(px(22.0 * scale))
                                .w(px(match split {
                                    Some("start") => 24.0,
                                    Some("end") => 17.0,
                                    _ => 22.0,
                                } * scale))
                                .flex()
                                .items_center()
                                .justify_center()
                                .map(|button| match split {
                                    Some("start") => button.rounded_l(px(4.0 * scale)),
                                    Some("end") => {
                                        button.rounded_r(px(4.0 * scale)).ml(px(-2.0 * scale))
                                    }
                                    _ => button.rounded(px(4.0 * scale)),
                                })
                                .cursor_pointer()
                                .hover(|button| button.bg(appearance.hover))
                                .child(glyph)
                                .when(children.is_some(), |button| {
                                    let trigger = trigger.clone();
                                    button
                                        .on_prepaint(move |bounds, _, _| trigger.set(Some(bounds)))
                                })
                                .when(
                                    self.native_sidebar.pointer_inside
                                        && self.native_sidebar.menu.is_none()
                                        && !cx.has_active_drag(),
                                    |row| {
                                        // CDXC:Sidebar 2026-09-23 DECISION: User: header button tooltips "appear below and aligned to the left not to the right (so they stay within the bounds of the sidebar)". The bubble's right edge lines up with the button and it grows leftward; the session card's buttons (hover_actions.rs) do the same.
                                        row.managed_discrete_tooltip_with_placement(
                                            ManagedTooltipPlacement::BelowLeft,
                                            appearance.tooltip_delay,
                                            move |window, cx| {
                                                titlebar_tooltip(label.clone(), window, cx)
                                            },
                                        )
                                    },
                                )
                                .on_click(cx.listener(
                                    move |app, event: &gpui::ClickEvent, window, cx| {
                                        cx.stop_propagation();
                                        if let Some(children) = &children {
                                            // Project header menus open `HEADER_MENU_TRIGGER_GAP` below the button, right-aligned to it.
                                            let anchor = trigger
                                                .get()
                                                .map(|bounds| {
                                                    gpui::Point::new(
                                                        bounds.right(),
                                                        bounds.bottom()
                                                            + px(crate::app::consts::HEADER_MENU_TRIGGER_GAP * scale),
                                                    )
                                                })
                                                .unwrap_or_else(|| event.position());
                                            Self::show_native_sidebar_menu(
                                                children, anchor, scale, window, cx,
                                            );
                                        } else if let Some(command) = &command {
                                            app.dispatch_native_sidebar_ui(command.clone(), cx);
                                        }
                                    },
                                ))
                        })),
                )
            })
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                Self::show_native_sidebar_menu(&menu, event.position, scale, window, cx);
            })
            .sidebar_drag_source(dragged, cx)
            .sidebar_drop_target("group", drag_id, None, cx)
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(json!({"type": "toggleGroup", "groupId": id}), cx);
            }))
            .into_any_element()
    }
}
