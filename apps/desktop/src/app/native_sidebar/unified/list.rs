//! The Unified sidebar's list: Drafts on top, then the cards and the shelves pushed to the bottom,
//! or each project's group with its own shelves, or what an empty list says.

use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px};
use gpui_component::v_flex;
use serde_json::Value;

use super::model::{UnifiedShelfView, UnifiedSidebarSnapshot};
use crate::GhostexGpuiApp;
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
            } else {
                "No sessions yet".to_string()
            };
            div()
                .w_full()
                .py(px(26.0 * scale))
                .flex()
                .justify_center()
                .text_size(px(13.0 * scale))
                .text_color(appearance.muted)
                .child(copy)
        });
        let drafts = (!unified.drafts.is_empty()).then(|| UnifiedShelfView {
            shelf: ghostex_gx_core::unified_sidebar::UnifiedShelf::Drafts,
            cards: unified.drafts.clone(),
        });
        v_flex()
            .w_full()
            .children(empty)
            .children(drafts.map(|drafts| {
                v_flex()
                    .w_full()
                    .child(self.render_native_unified_shelf(&drafts, None, hud, appearance, cx))
                    .child(div().h(px(10.0 * scale)))
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
