//! Group by Project: a project's header, its cards and its own shelves. Dragging a header arranges
//! the groups; the rule and the saved order are gx-core's (`unified_sidebar/grouping.rs`).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext as _, DragMoveEvent, Hsla, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::{ElementExt as _, h_flex, v_flex};
use serde_json::Value;

use ghostex_gx_core::unified_sidebar::UnifiedStatus;

use super::cards::{repo_tile, show_project_icons, status_look};
use super::model::{UnifiedCard, UnifiedGroupView};
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::disclosure::unified_key;

/// A project header on the move.
#[derive(Clone)]
pub(super) struct UnifiedGroupDrag {
    repo_key: String,
    label: String,
    scale: f32,
    foreground: Hsla,
    background: Hsla,
}

impl Render for UnifiedGroupDrag {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .h(px(28.0 * self.scale))
            .px(px(10.0 * self.scale))
            .flex()
            .items_center()
            .rounded(px(6.0 * self.scale))
            .bg(self.background)
            .text_color(self.foreground)
            .text_size(px(13.0 * self.scale))
            .child(self.label.clone())
    }
}

/// The most urgent status under a folded header, which its dot shows.
fn most_urgent(group: &UnifiedGroupView) -> Option<UnifiedStatus> {
    let statuses: Vec<UnifiedStatus> = group
        .cards
        .iter()
        .chain(group.shelves.iter().flat_map(|shelf| shelf.cards.iter()))
        .filter_map(|card: &UnifiedCard| card.status)
        .collect();
    [
        UnifiedStatus::Approval,
        UnifiedStatus::Question,
        UnifiedStatus::Failed,
        UnifiedStatus::Done,
        UnifiedStatus::Working,
    ]
    .into_iter()
    .find(|status| statuses.contains(status))
}

impl GhostexGpuiApp {
    /// One group.
    pub(super) fn render_native_unified_group(
        &self,
        group: &UnifiedGroupView,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let key = group.repo_key.clone();
        let folded = self
            .gx_store
            .unified_sidebar
            .saved
            .folded_groups
            .contains(&key);
        let drop_line = self
            .native_sidebar
            .unified_drop_target
            .as_ref()
            .filter(|(target, _)| *target == key && cx.has_active_drag())
            .map(|(_, after)| *after);
        let hover_group = format!("native-unified-group-{key}");
        let toggle_key = key.clone();
        let drag = UnifiedGroupDrag {
            repo_key: key.clone(),
            label: group.label.clone(),
            scale,
            foreground: appearance.foreground,
            background: appearance.selected,
        };
        let new_key = key.clone();
        let menu_key = key.clone();
        // The menu hangs below the header, as a Project sidebar header's does.
        let owner = crate::app::native_sidebar::menus::MenuOwner::default();
        let record_owner = crate::app::native_sidebar::menus::record_menu_owner(&owner);
        let header = h_flex()
            .id(format!("native-unified-group-header-{key}"))
            .group(hover_group.clone())
            .role(gpui::Role::Button)
            .aria_label(format!("{} ({})", group.label, group.count))
            .aria_expanded(!folded)
            .mx(px(3.0 * scale))
            .h(px(30.0 * scale))
            .px(px(6.0 * scale))
            .gap(px(6.0 * scale))
            .rounded(px(5.0 * scale))
            .text_size(px(13.5 * scale))
            .font_weight(gpui::FontWeight::NORMAL)
            .hover(|row| row.bg(appearance.hover))
            .child(
                gpui::svg()
                    .path(if folded {
                        "titlebar/chevron-right.svg"
                    } else {
                        "titlebar/chevron-down.svg"
                    })
                    .size(px(13.0 * scale))
                    .flex_shrink_0()
                    .text_color(appearance.muted),
            )
            .children(
                group
                    .cards
                    .iter()
                    .chain(group.shelves.iter().flat_map(|shelf| shelf.cards.iter()))
                    .next()
                    .map(|card| repo_tile(card, appearance, show_project_icons(hud), false)),
            )
            .child(
                div()
                    .flex_shrink(1.0)
                    .min_w_0()
                    .truncate()
                    .child(group.label.clone()),
            )
            // CDXC:RemoteMachines 2026-11-06 DECISION:
            // User: "also we should show this name on unified sidebar project headers when group by proj is enabled". A project on more than one computer, or only on another one, names each of them after its title in the Project sidebar header's chip, with the computer's short name (gx-core machine_short_names.rs) and its full name in the chip's tooltip.
            .children(group.machines.iter().map(|(name, short)| {
                let tooltip = name.clone();
                let span = crate::app::native_sidebar::tooltips::SidebarTooltipSpan::sidebar(
                    self.sidebar_width,
                    scale,
                );
                div()
                    .id(gpui::SharedString::from(format!(
                        "native-unified-group-machine-{key}-{short}"
                    )))
                    .flex_shrink_0()
                    .child(
                        crate::app::native_sidebar::project_header::machine_name_chip(
                            short, appearance,
                        ),
                    )
                    .when(
                        self.native_sidebar.pointer_inside
                            && self.native_sidebar.menu.is_none()
                            && !cx.has_active_drag(),
                        |chip| {
                            chip.managed_discrete_tooltip_with_placement(
                                span.placement(),
                                appearance.tooltip_delay,
                                move |window, cx| {
                                    crate::app::native_sidebar::tooltips::sidebar_tooltip(
                                        tooltip.clone(),
                                        span,
                                        scale,
                                        window,
                                        cx,
                                    )
                                },
                            )
                        },
                    )
            }))
            .child(div().flex_1())
            .when(folded, |row| {
                row.children(most_urgent(group).map(|status| {
                    let (_, color) = status_look(status, appearance);
                    div()
                        .size(px(7.0 * scale))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(color)
                }))
            })
            .child(
                div()
                    .id(format!("native-unified-group-new-{key}"))
                    .role(gpui::Role::Button)
                    .aria_label(format!("New session in {}", group.label))
                    .size(px(22.0 * scale))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0 * scale))
                    .invisible()
                    .group_hover(hover_group, |button| button.visible())
                    .hover(|button| button.bg(appearance.hover))
                    .child(
                        gpui::svg()
                            .path("titlebar/plus.svg")
                            .size(px(14.0 * scale))
                            .text_color(appearance.chrome_icon),
                    )
                    .on_click(
                        cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            app.unified_group_new_session(&new_key, event, scale, window, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(gpui::FontWeight::LIGHT)
                    .text_color(appearance.muted)
                    .child(group.count.to_string()),
            )
            .on_prepaint(move |bounds, window, app| record_owner(bounds, window, app))
            /*
            CDXC:Sidebar 2026-11-05 DECISION:
            User: "in the unified sidebar i can't right click on the project header when i group by project. pls fix." The header had no right-click handler at all. It now opens the Project sidebar header's menu for that project, from the same builder the filter menu's project row uses (`gx_store_unified_project_menu`), so its rows run the same actions. A repository on several computers opens the menu of its main checkout on this computer when it has one there, else on another (`repo_menu_groups`, CDXC:Sidebar 2026-11-01 in header.rs).
            */
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
            )
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                let host = &mut app.gx_store.unified_sidebar;
                let folded = !host.saved.folded_groups.remove(&toggle_key);
                if folded {
                    host.saved.folded_groups.insert(toggle_key.clone());
                }
                host.persist();
                // It folds as a Project sidebar project does (disclosure.rs, fold_motion.rs).
                let duration = app.native_sidebar_fold_duration();
                app.native_sidebar
                    .disclosures
                    .fold(unified_key(&toggle_key), folded, duration);
                cx.notify();
            }))
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()));
        let move_key = key.clone();
        let drop_key = key.clone();
        v_flex()
            .id(format!("native-unified-group-{key}"))
            .w_full()
            .relative()
            .pb(px(4.0 * scale))
            .on_drag_move::<UnifiedGroupDrag>(cx.listener(
                move |app, event: &DragMoveEvent<UnifiedGroupDrag>, _, cx| {
                    let inside = event.bounds.contains(&event.event.position);
                    let was = app.native_sidebar.unified_drop_target.clone();
                    if inside {
                        let after = event.event.position.y > event.bounds.center().y;
                        app.native_sidebar.unified_drop_target = Some((move_key.clone(), after));
                    } else if was.as_ref().is_some_and(|(target, _)| *target == move_key) {
                        app.native_sidebar.unified_drop_target = None;
                    }
                    if was != app.native_sidebar.unified_drop_target {
                        cx.notify();
                    }
                },
            ))
            .on_drop(cx.listener(move |app, drag: &UnifiedGroupDrag, _, cx| {
                let after = app
                    .native_sidebar
                    .unified_drop_target
                    .take()
                    .is_some_and(|(_, after)| after);
                app.gx_store_unified_move_group(&drag.repo_key, &drop_key, after, cx);
            }))
            .child(header)
            // Drawn while open and while it folds shut, inside the fold that moves it.
            .when(
                self.native_sidebar
                    .disclosures
                    .present(&unified_key(&key), folded),
                |column| {
                    let body = v_flex()
                        .w_full()
                        .children(
                            group.cards.iter().map(|card| {
                                self.render_native_unified_card(card, hud, appearance, cx)
                            }),
                        )
                        .children(group.shelves.iter().map(|shelf| {
                            self.render_native_unified_shelf(shelf, Some(&key), hud, appearance, cx)
                        }));
                    column.child(self.render_native_disclosure(
                        unified_key(&key),
                        body.into_any_element(),
                        cx,
                    ))
                },
            )
            .children(drop_line.map(|after| {
                div()
                    .absolute()
                    .left(px(6.0 * scale))
                    .right(px(6.0 * scale))
                    .h(px(2.0 * scale))
                    .rounded_full()
                    .bg(appearance.foreground.opacity(0.6))
                    .when(after, |line| line.bottom_0())
                    .when(!after, |line| line.top_0())
            }))
            .into_any_element()
    }

    /// A header's right-click menu: the Project sidebar menu of the project its repository's filter
    /// row opens.
    fn unified_group_menu(&mut self, repo_key: &str) -> Option<Value> {
        let group_id = self
            .native_sidebar
            .unified
            .as_ref()?
            .repo_menu_groups
            .get(repo_key)?
            .clone();
        self.gx_store_unified_project_menu(&group_id)
    }

    /// The "+" on a header: the New thread picker on that project, on the machine it starts on.
    ///
    /// CDXC:Sidebar 2026-11-03 DECISION:
    /// User (Q1167, the page's "+" opens the picker with the machine already chosen, unobjected): the machine is picked on the New thread picker, so the "+" no longer opens a menu of machines first. It starts on the last machine used for the project, a connected one when there is one.
    fn unified_group_new_session(
        &mut self,
        repo_key: &str,
        _event: &gpui::ClickEvent,
        _scale: f32,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_new_session_in_repo(repo_key, cx);
    }
}
