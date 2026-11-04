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
    StatefulInteractiveElement, Styled, div, img, px, rgb,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{ElementExt as _, h_flex};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

/// Width of the chevron slot at the header box's left edge: 16px glyph plus 2px, the 18px that used to hang outside the row.
pub(super) const PROJECT_HEADER_CHEVRON_GUTTER: f32 = 18.0;
/// How far the header box reaches left of where its parent places a row: the chevron slot minus the old 3px side margin, so the icon and name stay where they were.
pub(super) const PROJECT_HEADER_CHEVRON_INSET: f32 = PROJECT_HEADER_CHEVRON_GUTTER - 3.0;

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
        let hover_id = id.clone();
        let drag_id = id.clone();
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
        // Set only on a bot's row, which draws a gateway dot instead of git stats and counts.
        let bot_gateway_running = group.bot_gateway_running();
        // CDXC:Icons 2026-10-24 DECISION:
        // User chose "Keep the folder": a user-made session group's header draws a folder instead of its title's first letter, which made "Group 6" and a project named "g…" look alike. The letter tile (CDXC:Icons 2026-09-24 below) stays for projects only.
        let folder_icon = group.owner_group_id.is_some();
        // A collapsed project draws as active when the focused session is in it, its session
        // groups included, which fold away with it.
        let collapsed_active =
            group.collapsed && (group.is_active || group.holds_active_session_group);
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
            refused: Default::default(),
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
        // Borrowed, not copied: the buttons are drawn only while the header is hovered.
        let actions = &group.header_actions;
        // Where this header (or its pinned copy) was painted: its menus hang below it (menus.rs).
        let header_bounds = Rc::new(Cell::new(None::<gpui::Bounds<gpui::Pixels>>));
        // CDXC:Projects 2026-09-18 DECISION:
        // User: the project header row has the same insets, side padding, rounding, and hover fill as a session card.
        h_flex()
            .id(format!("native-sidebar-project-{id}"))
            .role(gpui::Role::TreeItem)
            .aria_label(group.title.clone())
            .aria_expanded(!group.collapsed)
            .relative()
            .h(px(30.0 * scale))
            .ml(px(-PROJECT_HEADER_CHEVRON_INSET * scale))
            .mr(px(3.0 * scale))
            .pl(px((5.0 + PROJECT_HEADER_CHEVRON_GUTTER) * scale))
            .pr(px(6.0 * scale))
            .gap(px(10.0 * scale))
            .rounded(px(5.0 * scale))
            .cursor_default()
            .when(group.is_stale, |row| row.opacity(0.55))
            .hover(|row| row.bg(appearance.session_hover))
            .on_prepaint({
                let header_bounds = header_bounds.clone();
                move |bounds, _, _| header_bounds.set(Some(bounds))
            })
            .child(self.native_sidebar.header_hover.probe(hover_id))
            // CDXC:Projects 2026-10-06 DECISION:
            // User: "Make the chevron to the left of the project part of the button, not outside it". The chevron sits inside the header's hover/selected box at its left edge and is not a hit area of its own: a click on it is the row's click, which expands/collapses the project, and a drag from it moves the project. This supersedes the 2026-09-19 separate chevron button hanging outside the row.
            .child(
                div().absolute().left_0().top(px(7.0 * scale)).child(
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
                        _ if folder_icon => gpui::svg()
                            .path("titlebar/folder.svg")
                            .size(px(16.0 * scale))
                            .flex_shrink_0()
                            .text_color(appearance.muted)
                            .into_any_element(),
                        _ if bot_gateway_running.is_some() => letter_tile(
                            &group.title,
                            super::bots::hermes_color(),
                            rgb(0x111111).into(),
                            scale,
                        ),
                        Some(image) => img(image)
                            .size(px(16.0 * scale))
                            .flex_shrink_0()
                            .into_any_element(),
                        // CDXC:Icons 2026-09-24 DECISION: User: when a project has no favicon, show a square with the first letter of its name instead of the folder icon. Projects only: since 2026-10-24 a session group's header keeps the folder (CDXC:Icons 2026-10-24 above).
                        None => letter_tile(
                            &group.title,
                            appearance.muted.opacity(0.12),
                            appearance.muted,
                            scale,
                        ),
                    })
                },
            )
            .when(collapsed_active, |row| {
                row.bg(appearance.selected)
                    .child(super::decorations::selected_outline(appearance))
            })
            .when(self.native_sidebar_group_drop_target(&id), |row| {
                super::sections::section_drop_highlight(row, scale)
            })
            .child(title)
            .when_some(bot_gateway_running, |row, running| {
                row.child(super::bots::bot_gateway_dot(running, appearance))
            })
            .when(!hovered && bot_gateway_running.is_none(), |row| {
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
                        .children(actions.iter().enumerate().map(|(index, item)| {
                            let label = item["label"].as_str().unwrap_or("").to_owned();
                            let command = item.get("command").cloned();
                            let children = item.get("children").cloned();
                            let header_bounds = header_bounds.clone();
                            let button_owner = super::menus::MenuOwner::default();
                            let trigger_id = format!("project:{id}:{label}");
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
                                .on_prepaint(super::menus::record_menu_owner(&button_owner))
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
                                .when(
                                    self.native_sidebar.pointer_inside
                                        && self.native_sidebar.menu.is_none()
                                        && !cx.has_active_drag(),
                                    |row| {
                                        // CDXC:Sidebar 2026-09-23 DECISION: User: header button tooltips "appear below and aligned to the left not to the right (so they stay within the bounds of the sidebar)". The bubble's right edge lines up with the button and it grows leftward; the session card's buttons (hover_actions.rs) do the same.
                                        // The binding is read here, not in gx-core, so it follows the user's Settings > Hotkeys.
                                        let tooltip = match item["hotkey"].as_str().and_then(
                                            crate::app::hotkeys::gpui_configured_hotkey_label,
                                        ) {
                                            Some(shortcut) => format!("{label} ({shortcut})"),
                                            None => label.clone(),
                                        };
                                        row.managed_discrete_tooltip_with_placement(
                                            ManagedTooltipPlacement::BelowLeft,
                                            appearance.tooltip_delay,
                                            move |window, cx| {
                                                super::tooltips::sidebar_style_tooltip(
                                                    tooltip.clone(),
                                                    window,
                                                    cx,
                                                )
                                            },
                                        )
                                    },
                                )
                                .on_click(cx.listener(
                                    move |app, event: &gpui::ClickEvent, window, cx| {
                                        cx.stop_propagation();
                                        if let Some(children) = &children {
                                            Self::show_native_sidebar_menu_hung(
                                                children,
                                                header_bounds.get(),
                                                button_owner.get(),
                                                Some((
                                                    trigger_id.clone(),
                                                    super::menus::press_position(event),
                                                )),
                                                event.position(),
                                                scale,
                                                window,
                                                cx,
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
                Self::show_native_sidebar_context_menu(
                    &menu,
                    header_bounds.get(),
                    event.position,
                    scale,
                    window,
                    cx,
                );
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

/// The first letter of a row's name on a colored square, in the project icon's slot and size.
pub(in crate::app::native_sidebar) fn letter_tile(
    title: &str,
    background: gpui::Hsla,
    foreground: gpui::Hsla,
    scale: f32,
) -> AnyElement {
    div()
        .size(px(16.0 * scale))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.0 * scale))
        .bg(background)
        .text_color(foreground)
        .text_size(px(10.0 * scale))
        .line_height(px(16.0 * scale))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(
            title
                .trim()
                .chars()
                .next()
                .map(|letter| letter.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_owned()),
        )
        .into_any_element()
}
