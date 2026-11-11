//! The Unified sidebar's list: Drafts on top, then the cards and the shelves pushed to the bottom,
//! or each project's group with its own shelves, or what an empty list says.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, IntoElement, ParentElement, StatefulInteractiveElement as _, Styled, div, px,
};
use gpui_component::v_flex;
use serde_json::Value;

use super::model::{UnifiedShelfView, UnifiedSidebarSnapshot};
use crate::GhostexGpuiApp;
use crate::app::helpers::chrome_ink;
use crate::app::native_sidebar::appearance::SidebarAppearance;

impl GhostexGpuiApp {
    pub(in crate::app::native_sidebar) fn render_native_unified_list(
        &mut self,
        unified: &UnifiedSidebarSnapshot,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        self.keep_unified_timers_ticking(unified, cx);
        let scale = appearance.scale;
        let empty = unified.is_empty().then(|| {
            let copy = if !unified.has_projects {
                "No projects yet".to_string()
            } else if !unified.filter.query.trim().is_empty() {
                format!("No sessions match \"{}\"", unified.filter.query.trim())
            } else if unified.total > 0 {
                "No sessions match this filter".to_string()
            } else if let Some(space) = &unified.space {
                // The total counts the active Space's sessions only.
                format!("No sessions in {space}")
            } else {
                "No sessions yet".to_string()
            };
            // CDXC:Onboarding 2026-11-11 WHY: the hands-on check of the new setup ended in this empty
            // list with no way to add a project in sight. With no project at all it offers Add Project,
            // as the Project sidebar's empty state does (native_sidebar/empty.rs).
            let add_project = !unified.has_projects;
            v_flex()
                .w_full()
                .py(px(26.0 * scale))
                .items_center()
                .gap(px(12.0 * scale))
                .text_size(px(13.0 * scale))
                .text_color(appearance.muted)
                .child(copy)
                .when(add_project, |column| {
                    column.child(
                        super::super::empty::empty_action_button(
                            "native-unified-empty-add-project",
                            "Add Project",
                            Some("titlebar/plus.svg"),
                            appearance,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            cx.stop_propagation();
                            app.dispatch_native_sidebar_ui(
                                serde_json::json!({"type": "sidebarAction", "action": "addProject"}),
                                cx,
                            );
                        })),
                    )
                })
        });
        let drafts = (!unified.drafts.is_empty()).then(|| UnifiedShelfView {
            shelf: ghostex_gx_core::unified_sidebar::UnifiedShelf::Drafts,
            cards: unified.drafts.clone(),
        });
        let sessions_below =
            !unified.groups.is_empty() || !unified.list.is_empty() || !unified.shelves.is_empty();
        v_flex()
            .w_full()
            .children(empty)
            .children(drafts.map(|drafts| {
                // CDXC:Sidebar 2026-11-05 DECISION: User: "put a separator line below drafts area in unified sidebar so it's diffrentiated from othrr sessions". A hairline in the sidebar's divider ink (the Links panel's rule, which follows dark and light and glass), inset to the shelf header's text and evenly spaced above and below, stands where the 10px gap was; with nothing below Drafts it stays a plain gap, so no rule dangles.
                v_flex()
                    .w_full()
                    .child(self.render_native_unified_shelf(&drafts, None, hud, appearance, cx))
                    .child(if sessions_below {
                        v_flex()
                            .w_full()
                            .py(px(6.0 * scale))
                            .child(
                                div()
                                    .mx(px(9.0 * scale))
                                    .h(px(1.0 * scale))
                                    .bg(chrome_ink().opacity(0.12)),
                            )
                            .into_any_element()
                    } else {
                        div().h(px(10.0 * scale)).into_any_element()
                    })
            }))
            .children(
                unified
                    .groups
                    .iter()
                    .map(|group| self.render_native_unified_group(group, hud, appearance, cx)),
            )
            .children(
                unified
                    .list
                    .iter()
                    .map(|card| self.render_native_unified_card(card, hud, appearance, cx)),
            )
            .when(!unified.shelves.is_empty(), |column| {
                column.child(div().h(px(10.0 * scale)))
            })
            .children(
                unified.shelves.iter().map(|shelf| {
                    self.render_native_unified_shelf(shelf, None, hud, appearance, cx)
                }),
            )
            .into_any_element()
    }

    /// Redraws once a second while a card counts a working agent's time, and stops when none does.
    fn keep_unified_timers_ticking(
        &mut self,
        unified: &UnifiedSidebarSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        if !unified.has_running_timer() {
            self.native_sidebar.unified_tick = None;
            return;
        }
        if self.native_sidebar.unified_tick.is_some() {
            return;
        }
        self.native_sidebar.unified_tick = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(1))
                .await;
            let _ = view.update(cx, |app, cx| {
                app.native_sidebar.unified_tick = None;
                cx.notify();
            });
        }));
    }
}
