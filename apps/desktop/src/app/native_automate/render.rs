//! The Automate page frame: the header (eyebrow, project title, the Automations / Runs / Triage
//! tabs, refresh and + Automation), the notice line, and the list | detail split per tab.

use super::style::{
    AutomatePalette, ICON_ALERT, ICON_BELL, ICON_CLOCK, ICON_PLUS, ICON_REFRESH, ROW_RADIUS,
    empty_state, icon_button, secondary_button, truncated,
};
use super::view::{AutomateTab, NativeAutomateView};
use crate::app::helpers::{
    CHROME_LIGHT_APPEARANCE, glass_clear, window_glass_active_in, workspace_background_color,
};
use crate::app::view_skeletons::{ViewSkeletonKind, render_view_skeleton};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, Window, div, px, relative, rgb,
};
use gpui_component::{h_flex, v_flex};

impl NativeAutomateView {
    fn render_header(&self, p: &AutomatePalette, cx: &mut Context<Self>) -> AnyElement {
        let all = self.is_all_projects();
        let scope = self.scope.as_ref();
        let title = scope
            .map(|scope| scope.project_name.clone())
            .unwrap_or_default();
        let show_controls = scope.is_some_and(|scope| !scope.coming_soon);
        // CDXC:Automations 2026-09-17 DECISION:
        // User: rename Automations Overview to All Automations, drop the Experimental label, and use Overview as the all-project page eyebrow. The project Automate surface still uses Automations so the page title is not repeated. This supersedes the 2026-06-30 eyebrow copy.
        let eyebrow = if all { "Overview" } else { "Automations" };
        let tabs = h_flex().gap(px(4.0)).children(AutomateTab::ALL.map(|tab| {
            let active = self.tab == tab;
            let hover = p.foreground.opacity(0.8);
            div()
                .id(("automate-tab", tab as usize))
                .h(px(32.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .text_size(px(14.0))
                .cursor_pointer()
                .when(active, |this| this.bg(p.selected).text_color(p.foreground))
                .when(!active, |this| {
                    this.text_color(p.muted)
                        .hover(move |this| this.text_color(hover))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.set_tab(tab, cx);
                }))
                .child(tab.label())
        }));
        h_flex()
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap(px(16.0))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_size(px(12.0)).text_color(p.muted).child(eyebrow))
                    .child(
                        truncated(title)
                            .text_size(px(15.0))
                            .text_color(p.foreground),
                    ),
            )
            .when(show_controls, |this| this.child(tabs))
            .child(h_flex().flex_1().min_w_0().justify_end().gap(px(6.0)).when(
                show_controls,
                |this| {
                    this.child(icon_button(
                        p,
                        "automate-refresh",
                        ICON_REFRESH,
                        "Refresh automations",
                        self.loading,
                        |this: &mut Self, _window, cx| this.load(cx),
                        cx,
                    ))
                    .when(self.state.is_some(), |this| {
                        this.child(secondary_button(
                            p,
                            "automate-new",
                            Some(ICON_PLUS),
                            "Automation",
                            |this: &mut Self, _window, cx| this.open_create_dialog(cx),
                            cx,
                        ))
                    })
                },
            ))
            .into_any_element()
    }

    fn render_notice(&self, p: &AutomatePalette) -> Option<AnyElement> {
        let message = self.error_message.as_ref()?;
        Some(
            h_flex()
                .w_full()
                .flex_shrink_0()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(8.0))
                .items_center()
                .rounded(px(ROW_RADIUS))
                .border_1()
                .border_color(p.danger.opacity(0.35))
                .bg(p.danger.opacity(0.08))
                .text_size(px(13.0))
                .text_color(p.danger)
                .child(super::style::icon(ICON_ALERT, 15.0, p.danger))
                .child(div().min_w_0().child(message.clone()))
                .into_any_element(),
        )
    }

    /// The list on the left and the detail on the right (`minmax(280px, 0.9fr)` |
    /// `minmax(320px, 1.1fr)`), split by a hairline.
    fn split(p: &AutomatePalette, list: AnyElement, detail: AnyElement) -> AnyElement {
        h_flex()
            .size_full()
            .min_h_0()
            .items_stretch()
            .child(
                div()
                    .w(relative(0.45))
                    .min_w(px(280.0))
                    .h_full()
                    .min_h_0()
                    .border_r_1()
                    .border_color(p.border)
                    .child(list),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(320.0))
                    .h_full()
                    .min_h_0()
                    .child(detail),
            )
            .into_any_element()
    }

    fn render_automations_tab(&self, p: &AutomatePalette, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = self.state.as_ref() else {
            return div().into_any_element();
        };
        if state.automations.is_empty() {
            // One centred empty state, not an empty list beside a "No automation selected" detail.
            let action = secondary_button(
                p,
                "automate-empty-create",
                None,
                "Create automation",
                |this: &mut Self, _window, cx| this.open_create_dialog(cx),
                cx,
            )
            .into_any_element();
            return empty_state(
                p,
                ICON_CLOCK,
                "No automations yet",
                "Schedule agents with a timer, a specific date, or a repeating cadence.",
                Some(action),
            );
        }
        let list = self.render_automation_list(p, state, cx);
        let detail = self.render_automation_detail(p, state, cx);
        Self::split(p, list, detail)
    }

    fn render_runs_tab(
        &self,
        p: &AutomatePalette,
        triage: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(state) = self.state.as_ref() else {
            return div().into_any_element();
        };
        let runs: Vec<_> = if triage {
            state.triage_runs()
        } else {
            state.visible_runs().collect()
        };
        if runs.is_empty() {
            // Like the Automations tab: one centred empty state instead of the split view.
            let (title, description) = if triage {
                (
                    "No automation results need triage",
                    "When an automation reports findings or needs attention, the result appears here for review.",
                )
            } else {
                (
                    "No automation runs yet",
                    "Runs appear here after automations execute on their schedule or when you run them manually.",
                )
            };
            return empty_state(p, ICON_BELL, title, description, None);
        }
        let selected = self
            .selected_run_id
            .as_deref()
            .and_then(|id| runs.iter().find(|run| run.id == id))
            .or_else(|| runs.first())
            .copied();
        let list = self.render_run_list(p, state, &runs, selected.map(|run| run.id.as_str()), cx);
        let detail = self.render_run_detail(p, state, selected, cx);
        Self::split(p, list, detail)
    }

    fn render_body(&self, p: &AutomatePalette, cx: &mut Context<Self>) -> AnyElement {
        if let Some(scope) = self.scope.as_ref()
            && scope.coming_soon
        {
            return empty_state(
                p,
                ICON_CLOCK,
                "All Automations is coming very soon",
                "Turn on All Automations in Settings > Extensions > Experimental to preview it before launch.",
                None,
            );
        }
        if self.state.is_none()
            && let Some(error) = self.load_error.clone()
        {
            let retry = secondary_button(
                p,
                "automate-retry",
                Some(ICON_REFRESH),
                "Try again",
                |this: &mut Self, _window, cx| this.load(cx),
                cx,
            )
            .into_any_element();
            return empty_state(
                p,
                ICON_ALERT,
                "Could not load automations",
                error,
                Some(retry),
            );
        }
        match self.tab {
            AutomateTab::Automations => self.render_automations_tab(p, cx),
            AutomateTab::Runs => self.render_runs_tab(p, false, cx),
            AutomateTab::Triage => self.render_runs_tab(p, true, cx),
        }
    }
}

/// CDXC:Automations 2026-09-30 DECISION:
/// The user chose to delete the React Kanban/Automate page on 2026-09-30 because the web app it was kept for is gone, so this native GPUI page is the only Automate view; it sits on the window glass like the chat. This supersedes the 2026-09-23 decision that kept the React page (the deleted apps/desktop/views/project-board/automations.tsx, in git history) as the behaviour spec.
impl Render for NativeAutomateView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.is_initial_loading() {
            let light = CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
            return render_view_skeleton(
                ViewSkeletonKind::Automate,
                "view-skeleton-automate",
                glass_clear(if light {
                    rgb(0xffffff).into()
                } else {
                    workspace_background_color()
                }),
            );
        }
        let p = AutomatePalette::resolve(window_glass_active_in(window));
        let header = self.render_header(&p, cx);
        let notice = self.render_notice(&p);
        let body = self.render_body(&p, cx);
        v_flex()
            .id("ghostex-gpui-native-automate")
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(p.page)
            .font_family(p.font.clone())
            .text_color(p.foreground)
            .p(px(20.0))
            .gap(px(14.0))
            .child(header)
            .children(notice)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .border_t_1()
                    .border_color(p.border)
                    .pt(px(4.0))
                    .child(body),
            )
            .into_any_element()
    }
}
