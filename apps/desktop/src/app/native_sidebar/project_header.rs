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
        let scale = appearance.scale;
        // The home project's header is Main Hub's row (chief.rs): its name, flower, status
        // and click are Chief's; its chevron, drag, drop and hover buttons stay the project's.
        let home = self.native_sidebar.snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .home_headers
                .iter()
                .find(|home| home.group_id == group.group_id)
                .cloned()
        });
        let menu = match &home {
            Some(home) => {
                let mut items = self
                    .native_sidebar_chief_menu(Some(&home.machine_id))
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                items.push(json!({ "separator": true }));
                items.extend(group.menu.as_array().cloned().unwrap_or_default());
                std::sync::Arc::new(Value::Array(items))
            }
            None => group.menu.clone(),
        };
        let home_title = home
            .as_ref()
            .map(|_| super::chief_row_view::CHIEF_LABEL.to_string());
        let home_chief = home.as_ref().and_then(|home| home.chief.clone());
        let home_focused = home_chief.as_ref().is_some_and(|chief| chief.is_focused);
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
        let bot_status = group.bot_status();
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
        // The chips draw short names (CDXC:RemoteMachines 2026-11-06 in gx-core
        // machine_short_names.rs); the title's tooltip names the computers in full.
        let tooltip = if group.machine_names.is_empty() {
            group.title_tooltip.clone()
        } else {
            let computers = group
                .machine_names
                .iter()
                .map(|(name, short)| format!("{short}: {name}"))
                .collect::<Vec<_>>()
                .join("\n");
            Some(match &group.title_tooltip {
                Some(title) => format!("{title}\n\n{computers}"),
                None => format!("{}\n\n{computers}", group.title),
            })
        };
        let title = if let Some(editor) = self
            .native_sidebar
            .name_editor
            .as_ref()
            .filter(|editor| home.is_none() && editor.kind == "group" && editor.id == id)
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
                .gap(px(5.0 * scale))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(home_title.clone().unwrap_or_else(|| group.title.clone())),
                )
                .children(
                    group
                        .machine_names
                        .iter()
                        .filter(|_| home.is_none())
                        .map(|(_, short)| machine_name_chip(short, appearance)),
                )
                .when_some(tooltip.filter(|_| home.is_none()), |title, tooltip| {
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
            .aria_label(home_title.clone().unwrap_or_else(|| group.title.clone()))
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
            // Resting on Main Hub's header opens its card beside the sidebar (chief.rs).
            .when(home.is_some(), |row| {
                let header_bounds = header_bounds.clone();
                let card_key = id.clone();
                row.on_hover(cx.listener(move |app, hovered: &bool, _, cx| {
                    let top = header_bounds.get().map(|bounds| bounds.top().as_f32());
                    app.hover_main_hub_row(card_key.clone(), top, *hovered, cx);
                }))
                .on_any_mouse_down(cx.listener(|app, _, _, cx| app.dismiss_main_hub_card(cx)))
            })
            .child(self.native_sidebar.header_hover.probe(hover_id))
            // Where the pick-an-agent menu of this computer's Chief hangs (chief.rs).
            .when(
                home.as_ref()
                    .is_some_and(|home| home.machine_id == ghostex_gx_core::LOCAL_MACHINE_ID),
                |row| {
                    row.on_prepaint(super::menus::record_menu_owner(
                        &self.native_sidebar.chief_row_bounds,
                    ))
                },
            )
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
                        _ if home.is_some() => gpui::svg()
                            .path(super::chief_row_view::FLOWER_ICON)
                            .size(px(16.0 * scale))
                            .flex_shrink_0()
                            .text_color(appearance.muted)
                            .into_any_element(),
                        _ if folder_icon => gpui::svg()
                            .path("titlebar/folder.svg")
                            .size(px(16.0 * scale))
                            .flex_shrink_0()
                            .text_color(appearance.muted)
                            .into_any_element(),
                        _ if bot_status.is_some() => letter_tile(
                            &group.title,
                            super::bots::bot_tile_color(group.is_gbot()),
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
            .when(collapsed_active || home_focused, |row| {
                row.bg(appearance.selected)
                    .child(super::decorations::selected_outline(appearance))
            })
            .when(self.native_sidebar_group_drop_target(&id), |row| {
                super::sections::section_drop_highlight(row, scale)
            })
            .child(title)
            .when_some(bot_status, |row, status| {
                row.child(super::bots::bot_status_dot(status, appearance))
            })
            .when(!hovered && bot_status.is_none() && home.is_none(), |row| {
                row.children(super::project_status::project_status(
                    group, hud, appearance,
                ))
            })
            // A home header shows its Chief's dot, or "Pick an agent" before the first open.
            .when(!hovered && home.is_some(), |row| match &home_chief {
                Some(chief) => row.children(super::chief::chief_status(chief, scale)),
                None => row.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * scale))
                        .text_color(appearance.muted)
                        .child("Pick an agent"),
                ),
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
            .map(|row| match &home {
                // The chevron folds the home's sessions; anywhere else opens Chief's chat.
                Some(home) => {
                    let machine_id = home.machine_id.clone();
                    let fold_id = id.clone();
                    row.child(
                        div()
                            .id(format!("native-sidebar-home-fold-{id}"))
                            .role(gpui::Role::Button)
                            .aria_label(if group.collapsed {
                                "Show the home folder's sessions"
                            } else {
                                "Fold the home folder's sessions"
                            })
                            .absolute()
                            .left_0()
                            .top_0()
                            .h_full()
                            .w(px(PROJECT_HEADER_CHEVRON_GUTTER * scale))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.dispatch_native_sidebar_ui(
                                    json!({"type": "toggleGroup", "groupId": fold_id}),
                                    cx,
                                );
                            })),
                    )
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.handle_native_sidebar_action(
                            &super::actions::NativeSidebarAction {
                                command: json!({ "type": "chief", "op": "open", "machineId": machine_id }),
                            },
                            window,
                            cx,
                        );
                    }))
                }
                None => row.on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.dispatch_native_sidebar_ui(
                        json!({"type": "toggleGroup", "groupId": id}),
                        cx,
                    );
                })),
            })
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

/// One machine a merged project is on, after its name (gx-core `sidebar_view/merged.rs`): a light
/// outlined chip in the muted text colour, never a filled one, holding the machine's short name.
/// The Unified sidebar's project headers draw the same chip (unified/groups.rs).
pub(crate) fn machine_name_chip(name: &str, appearance: &SidebarAppearance) -> AnyElement {
    let scale = appearance.scale;
    div()
        .flex_shrink_0()
        .max_w(px(72.0 * scale))
        .px(px(5.0 * scale))
        .rounded(px(99.0))
        .border_1()
        .border_color(appearance.muted.opacity(0.35))
        .text_size(px(10.0 * scale))
        .text_color(appearance.muted)
        .font_weight(gpui::FontWeight::NORMAL)
        .truncate()
        .child(name.to_owned())
        .into_any_element()
}
