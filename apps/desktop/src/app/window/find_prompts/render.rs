//! Drawing Search by Prompt: the query toolbar, the result list, the selected prompt's pane, the
//! fork picker, the notice line and the `^e` full-prompt view, measured against the React page
//! (find-prompts-view.tsx, find-prompt-row.tsx, find-prompts-overlays.tsx; deleted 2026-10-01, in git history).
//!
//! CDXC:PromptSearch 2026-09-16 DECISION:
//! User: the top-right controls match the Quick Access Sessions tab in size, show their hotkey in the app's regular tooltip for the one control under the pointer, and read as toggles (Days, Fav, View, Fork) or dropdowns (agents, projects) so the active state is obvious.
//! Moved here from the React view with the native port (2026-09-27).
//!
//! CDXC:PromptSearch 2026-09-19 DECISION:
//! User: the matched/total counter sits in a pill at the bottom right of the results, in the style of the floating "Search by Prompt" button over the Previous Sessions list, instead of in the query row where it cut the placeholder short.
//!
//! CDXC:PromptSearch 2026-09-08 DECISION: Hide both result counters while loading so Search by Prompt does not display provisional 0/0 counts.
//!
//! CDXC:PromptSearch 2026-09-16 DECISION:
//! User: while prompts load, Find shows skeleton rows shaped like the rendered result list (day header, agent column, prompt line, time and title line) filling the whole results area, and a paragraph-shaped skeleton in the preview pane, instead of a centered spinner.
use super::model::{
    FIND_PROMPT_AGENTS, FIND_PROMPT_FORK_AGENT_COUNT, FindAction, ViewRow, format_day_header,
    format_last_active_compact,
};
use super::toolbar_steps::{
    ActionsStep, FILTER_AGENTS_WIDTH, FILTER_PROJECTS_WIDTH, ToolbarLayout, ToolbarMetrics,
    toolbar_layout,
};
use super::window::{
    FIND_PLACEHOLDER, FIND_PLACEHOLDER_SHORT, FindMenu, FindNoticeKind, GpuiFindPromptsModalWindow,
};
use crate::app::window::native_modal_kit::{
    hsla, modal_edge_scrollbar, neutral_button_fills, rgba_of,
};
use crate::app::window::quick_access::chrome::quick_access_tooltip;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, BoxShadow, ClickEvent, Context, FontWeight,
    HighlightStyle, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    ParentElement as _, Render, Rgba, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledText, Window, canvas, div, list, point, px, relative, svg,
};
use gpui_component::input::{Input, Textarea};
use gpui_component::{h_flex, v_flex};
use std::time::Duration;

const ICON_CALENDAR_WEEK: &str = "modals/find/calendar-week.svg";
const ICON_STAR: &str = "modals/find/star.svg";
const ICON_STAR_FILLED: &str = "modals/find/star-filled.svg";
const ICON_EYE: &str = "modals/find/eye.svg";
const ICON_COPY: &str = "modals/find/copy.svg";
const ICON_CHECK: &str = "titlebar/check.svg";
const ICON_GIT_FORK: &str = "modals/find/git-fork.svg";
const ICON_CHEVRON_DOWN: &str = "modals/find/chevron-down.svg";

const PROMPT_WIDTHS: [f32; 10] = [0.62, 0.44, 0.78, 0.35, 0.56, 0.70, 0.48, 0.66, 0.40, 0.74];
const TITLE_WIDTHS: [f32; 10] = [0.38, 0.52, 0.30, 0.46, 0.58, 0.34, 0.50, 0.42, 0.60, 0.36];
const PARAGRAPH_WIDTHS: [f32; 8] = [0.92, 0.84, 0.96, 0.70, 0.88, 0.78, 0.94, 0.52];
const SKELETON_ROW_COUNT: usize = 40;
const SKELETON_ROWS_PER_DAY: usize = 7;
/// `h-64`, the bottom pane when the preview is not fullscreen.
const BOTTOM_PANE_HEIGHT: f32 = 256.0;
/// `amber-300`, the active favorite button's hover text.
const FAVORITE_HOVER: u32 = 0xffd230;

fn inset_ring(color: Rgba) -> BoxShadow {
    BoxShadow {
        color: hsla(color),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(1.0),
        inset: true,
    }
}

/// The query field can shrink to this width, no further, so typed text keeps scrolling inside the field instead of under the buttons.
const FIND_QUERY_MIN_WIDTH: f32 = 120.0;

fn hotkey(chord: &str) -> String {
    crate::hotkey_label::terminal_overlay_hotkey_chord_label(chord)
}

impl Render for GpuiFindPromptsModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_preview_input(window, cx);
        let width = f32::from(window.viewport_size().width);
        let metrics = *self
            .toolbar_metrics
            .get_or_insert_with(|| ToolbarMetrics::measure(window, &self.font_family));
        let layout = toolbar_layout(width, &metrics);
        self.sync_search_placeholder(layout.short_placeholder, window, cx);
        if layout.actions != ActionsStep::Menu && self.open_menu == Some(FindMenu::Actions) {
            // The "…" button is gone at this width, so its menu goes with it.
            self.close_menu(window, cx);
        }
        let p = self.p;
        div()
            .id("find-prompts-window")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(hsla(p.background))
            .font_family(self.font_family.clone())
            .text_color(hsla(p.foreground))
            .child(self.render_toolbar(layout, cx))
            .when(!self.fullscreen_preview, |this| {
                this.child(self.render_results(cx))
            })
            .child(self.render_bottom_pane(cx))
            .children(self.render_notice())
            .children(self.render_expanded(cx))
            .children(self.render_menu(cx))
    }
}

impl GpuiFindPromptsModalWindow {
    /// Gives the read-only preview text area the selected prompt and the wrap mode, only when they
    /// changed, so a render does not re-lay out the text.
    fn sync_preview_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .selected_text
            .clone()
            .or_else(|| self.selected_row().map(|row| row.text.clone()))
            .unwrap_or_default();
        if text != self.preview_input_text {
            self.preview_input_text = text.clone();
            self.preview_input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
        if self.wrap_preview != self.preview_input_wrap {
            self.preview_input_wrap = self.wrap_preview;
            let wrap = self.wrap_preview;
            self.preview_input
                .update(cx, |input, cx| input.set_soft_wrap(wrap, window, cx));
        }
    }

    /// Swaps the query placeholder for the short one when the window is narrow, only when that changed.
    fn sync_search_placeholder(
        &mut self,
        short: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if short == self.short_placeholder {
            return;
        }
        self.short_placeholder = short;
        let placeholder = if short {
            FIND_PLACEHOLDER_SHORT
        } else {
            FIND_PLACEHOLDER
        };
        self.search.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx)
        });
    }

    /// The query row: chevron and input on the left, the filter dropdowns and actions on the right.
    /// `layout` says which labels are drawn and whether Fav, View, Copy and Fork sit behind the "…" button (`toolbar_steps.rs`).
    fn render_toolbar(&self, layout: ToolbarLayout, cx: &mut Context<Self>) -> AnyElement {
        let compact = layout.actions == ActionsStep::Icons;
        let narrow = !layout.grouping_label;
        let p = self.p;
        let selected = self.selected_row();
        let has_row = selected.is_some();
        let favorite = selected.is_some_and(|row| row.favorite);
        let mut buttons: Vec<AnyElement> = Vec::with_capacity(7);
        let menu_actions = layout.actions == ActionsStep::Menu;
        buttons.push(self.toolbar_button(
            "find-grouping",
            ICON_CALENDAR_WEEK,
            if narrow { "" } else { "Grouping" },
            self.group_by_day,
            false,
            false,
            format!(
                "{} ({})",
                if self.group_by_day {
                    "Stop grouping results by day"
                } else {
                    "Group results by day"
                },
                hotkey("ctrl+d")
            ),
            FindAction::ToggleDayGrouping,
            cx,
        ));
        buttons.push(self.filter_trigger(FindMenu::Agent, cx));
        buttons.push(self.filter_trigger(FindMenu::Project, cx));
        if menu_actions {
            buttons.push(self.actions_menu_button(cx));
        } else {
            buttons.push(self.toolbar_button(
                "find-favorite",
                if favorite {
                    ICON_STAR_FILLED
                } else {
                    ICON_STAR
                },
                if compact { "" } else { "Fav" },
                favorite,
                favorite,
                !has_row,
                format!(
                    "{} ({})",
                    if favorite {
                        "Remove this prompt from favorites"
                    } else {
                        "Favorite this prompt"
                    },
                    hotkey("ctrl+f")
                ),
                FindAction::ToggleFavorite,
                cx,
            ));
            buttons.push(self.toolbar_button(
                "find-view",
                ICON_EYE,
                if compact { "" } else { "View" },
                self.expanded_prompt,
                false,
                !has_row,
                format!(
                    "{} ({})",
                    if self.expanded_prompt {
                        "Close the full prompt"
                    } else {
                        "View the full prompt"
                    },
                    hotkey("ctrl+e")
                ),
                FindAction::ViewPrompt,
                cx,
            ));
            buttons.push(self.toolbar_button(
                "find-copy",
                if self.copied_visible {
                    ICON_CHECK
                } else {
                    ICON_COPY
                },
                if compact { "" } else { "Copy" },
                false,
                false,
                !has_row,
                format!("Copy this prompt ({})", hotkey("ctrl+y")),
                FindAction::CopyPrompt,
                cx,
            ));
            buttons.push(self.toolbar_button(
                "find-fork",
                ICON_GIT_FORK,
                if compact { "" } else { "Fork" },
                self.fork_open,
                false,
                !has_row,
                format!(
                    "{} ({})",
                    if self.fork_open {
                        "Cancel fork"
                    } else {
                        "Fork this prompt into another agent"
                    },
                    hotkey("ctrl+o")
                ),
                FindAction::ForkPicker,
                cx,
            ));
        }
        h_flex()
            .flex_shrink_0()
            .w_full()
            .px(px(14.0))
            .py(px(8.0))
            .gap(px(10.0))
            .items_center()
            .border_b_1()
            .border_color(hsla(p.hairline))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(15.0))
                    .line_height(px(21.0))
                    .text_color(hsla(p.primary))
                    .child("❯"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(FIND_QUERY_MIN_WIDTH))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(
                        Input::new(&self.search)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .w_full()
                            .h(px(32.0))
                            .px(px(0.0))
                            .py(px(0.0))
                            .text_size(px(15.0))
                            .text_color(hsla(p.foreground)),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap(px(6.0))
                    .items_center()
                    .children(buttons),
            )
            .into_any_element()
    }

    /// A filled toolbar button (`FIND_TOOLBAR_BUTTON_CLASS`, the neutral fill per CDXC:AppModal
    /// 2026-10-14 on `neutral_button_fills`); `active` draws the toggle's pressed state.
    #[allow(clippy::too_many_arguments)]
    fn toolbar_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        active: bool,
        amber: bool,
        disabled: bool,
        tooltip: String,
        action: FindAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.p;
        let text = if amber {
            p.favorite
        } else if active {
            p.foreground
        } else {
            p.muted
        };
        let hover_text = if amber {
            crate::app::window::native_modal_kit::modal_rgba(FAVORITE_HOVER, 1.0)
        } else {
            p.foreground
        };
        let fills = neutral_button_fills(p.light);
        h_flex()
            .id(id)
            .flex_shrink_0()
            .h(px(32.0))
            .px(px(10.0))
            .gap(px(6.0))
            .items_center()
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(if active {
                p.foreground_at(0.35)
            } else {
                rgba_of(p.border, 0.0)
            }))
            .bg(hsla(if active { fills.pressed } else { fills.rest }))
            .text_size(px(13.0))
            .line_height(px(18.2))
            .text_color(hsla(text))
            .when(disabled, |this| this.opacity(0.5))
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .hover(move |this| {
                        let this = this.text_color(hsla(hover_text));
                        if active {
                            this
                        } else {
                            this.bg(hsla(fills.hover))
                        }
                    })
                    .active(move |this| this.bg(hsla(fills.pressed)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.run_action(action, window, cx);
                    }))
            })
            .tooltip(move |window, cx| quick_access_tooltip(tooltip.clone(), window, cx))
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0))
                    .flex_shrink_0()
                    .text_color(hsla(text)),
            )
            .when(!label.is_empty(), |this| this.child(label))
            .into_any_element()
    }

    /// The agent or project dropdown's trigger: fixed width, the picked value truncated.
    fn filter_trigger(&self, menu: FindMenu, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let (id, width, active, label, title, chord, bounds) = match menu {
            FindMenu::Agent => {
                let picked: Vec<&str> = FIND_PROMPT_AGENTS
                    .iter()
                    .zip(self.agents)
                    .filter(|(_, on)| *on)
                    .map(|(agent, _)| *agent)
                    .collect();
                (
                    "find-agents",
                    FILTER_AGENTS_WIDTH,
                    !picked.is_empty(),
                    if picked.is_empty() {
                        "All agents".to_string()
                    } else {
                        picked.join(", ")
                    },
                    "Filter by agent",
                    "ctrl+g",
                    self.agent_trigger.clone(),
                )
            }
            FindMenu::Project => (
                "find-projects",
                FILTER_PROJECTS_WIDTH,
                self.project.is_some(),
                match self.project.as_ref() {
                    None => "All projects".to_string(),
                    Some(path) => self
                        .project_facets
                        .iter()
                        .find(|facet| &facet.path == path)
                        .map(|facet| facet.name.clone())
                        .unwrap_or_else(|| path.clone()),
                },
                "Filter by project",
                "ctrl+j",
                self.project_trigger.clone(),
            ),
            FindMenu::Actions => {
                unreachable!("the actions menu has its own button, `actions_menu_button`")
            }
        };
        let open = self.open_menu == Some(menu);
        let text = if active || open {
            p.foreground
        } else {
            p.muted
        };
        let tooltip = format!("{title} ({})", hotkey(chord));
        h_flex()
            .id(id)
            .relative()
            .flex_shrink_0()
            .w(px(width))
            .h(px(32.0))
            .px(px(10.0))
            .gap(px(6.0))
            .items_center()
            .justify_between()
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(if active {
                p.foreground_at(0.35)
            } else {
                p.border
            }))
            .bg(hsla(if active {
                p.foreground_at(0.09)
            } else if open {
                p.button_hover
            } else {
                p.button
            }))
            .text_size(px(13.0))
            .line_height(px(18.2))
            .text_color(hsla(text))
            .cursor_pointer()
            .hover(move |this| {
                let this = this.text_color(hsla(p.foreground));
                if active {
                    this
                } else {
                    this.bg(hsla(p.button_hover))
                }
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.toggle_menu(menu, window, cx);
            }))
            .tooltip(move |window, cx| quick_access_tooltip(tooltip.clone(), window, cx))
            .child(
                canvas(move |rect, _, _| bounds.set(Some(rect)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
            .child(
                svg()
                    .path(ICON_CHEVRON_DOWN)
                    .size(px(14.0))
                    .flex_shrink_0()
                    .opacity(0.7)
                    .text_color(hsla(text)),
            )
            .into_any_element()
    }

    /// The results, virtualized, with the matched/total pill over their bottom-right corner.
    fn render_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let body = if self.view_rows.is_empty() && self.loading {
            self.render_list_skeleton()
        } else if self.view_rows.is_empty() {
            div()
                .size_full()
                .pt(px(6.0))
                .px(px(10.0))
                .child(
                    div()
                        .w_full()
                        .px(px(8.0))
                        .py(px(24.0))
                        .text_center()
                        .text_size(px(15.0))
                        .line_height(px(21.0))
                        .text_color(hsla(p.muted))
                        .child(if self.total == 0 {
                            "No agent prompt history was found on this machine."
                        } else {
                            "No prompts match this search."
                        }),
                )
                .into_any_element()
        } else {
            list(
                self.list.clone(),
                cx.processor(|this: &mut Self, index: usize, _window, cx| {
                    this.render_view_row(index, cx)
                }),
            )
            .size_full()
            .pt(px(6.0))
            .pb(px(40.0))
            .into_any_element()
        };
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(body)
            .when(!self.view_rows.is_empty(), |this| {
                // After the list's layout, when its rows are measured, bring the selection into view.
                let view = cx.entity().downgrade();
                this.child(
                    canvas(
                        move |_, _window, cx| {
                            let _ = view.update(cx, |this, cx| {
                                if std::mem::take(&mut this.pending_reveal)
                                    && this.reveal_selection()
                                {
                                    cx.notify();
                                }
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_0(),
                )
                .child(modal_edge_scrollbar(&self.list))
            })
            .when(!self.loading, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(14.0))
                        .bottom(px(10.0))
                        .h(px(26.0))
                        .px(px(11.0))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .border_1()
                        .border_color(hsla(p.pill_outline))
                        .bg(hsla(p.pill))
                        .shadow(vec![BoxShadow {
                            color: hsla(p.pill_highlight),
                            offset: point(px(0.0), px(1.0)),
                            blur_radius: px(0.0),
                            spread_radius: px(0.0),
                            inset: true,
                        }])
                        .text_size(px(12.0))
                        .line_height(px(18.0))
                        .text_color(hsla(p.muted))
                        .child(format!("{}/{}", self.matched, self.total)),
                )
            })
            .into_any_element()
    }

    /// A list entry inside the list's 10px side padding (gpui's list sizes items to its full width).
    fn render_view_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .px(px(10.0))
            .child(self.render_view_row_content(index, cx))
            .into_any_element()
    }

    fn render_view_row_content(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        match self.view_rows.get(index).copied() {
            Some(ViewRow::Day(day)) => div()
                .w_full()
                .px(px(8.0))
                .pt(px(14.0))
                .pb(px(6.0))
                .text_size(px(12.0))
                .line_height(px(16.8))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.muted))
                .child(format_day_header(day, self.now))
                .into_any_element(),
            Some(ViewRow::Row(local)) => self.render_row(local, cx),
            None => div().into_any_element(),
        }
    }

    /// One result: the agent and the prompt with its matches, then the time, title and project.
    fn render_row(&self, local: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let Some(row) = self.rows.get(local) else {
            return div().into_any_element();
        };
        let position = self.window_offset + local;
        let selected = position == self.selection;
        let highlight = HighlightStyle {
            color: Some(hsla(p.matched)),
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        };
        let prompt = StyledText::new(row.line.clone()).with_highlights(
            row.line_highlights
                .iter()
                .map(|range| (range.clone(), highlight)),
        );
        let meta_text: SharedString = format!("{} • {}", row.title, row.project_name).into();
        let dot = row.title.len() + 1..row.title.len() + 1 + "•".len();
        let meta = StyledText::new(meta_text).with_highlights([(
            dot,
            HighlightStyle {
                color: Some(hsla(crate::app::window::native_modal_kit::rgba_of(
                    p.muted, 0.6,
                ))),
                ..Default::default()
            },
        )]);
        h_flex()
            .id(("find-row", position))
            .w_full()
            .p(px(8.0))
            .gap(px(8.0))
            .items_start()
            .rounded(px(10.0))
            .cursor_default()
            .when(selected, |this| {
                this.bg(hsla(p.accent_at(0.7)))
                    .shadow(vec![inset_ring(p.border)])
            })
            .when(!selected, |this| {
                this.hover(move |this| this.bg(hsla(p.accent_at(0.3))))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.select_row(position, cx);
                    // The second press of a double click resumes while it still targets this row.
                    if event.click_count == 2 {
                        this.resume_row(local, cx);
                    }
                }),
            )
            .child(
                div()
                    .w(px(12.0))
                    .flex_shrink_0()
                    .pt(px(4.0))
                    .flex()
                    .justify_center()
                    .when(row.favorite, |this| {
                        this.child(
                            svg()
                                .path(ICON_STAR_FILLED)
                                .size(px(14.0))
                                .text_color(hsla(p.favorite)),
                        )
                    }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        h_flex()
                            .min_w_0()
                            .h(px(24.0))
                            .gap(px(8.0))
                            .items_start()
                            .child(
                                div()
                                    .w(px(72.0))
                                    .flex_shrink_0()
                                    .pt(px(4.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(13.0))
                                    .line_height(px(18.2))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(hsla(row.agent_color))
                                    .child(row.agent.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(15.0))
                                    .line_height(px(24.0))
                                    .text_color(hsla(p.foreground))
                                    .child(prompt),
                            ),
                    )
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap(px(8.0))
                            .text_size(px(13.0))
                            .line_height(px(18.2))
                            .text_color(hsla(p.muted))
                            .child(
                                div()
                                    .w(px(72.0))
                                    .flex_shrink_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(format_last_active_compact(row.ts, self.now)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(meta),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn skeleton_bar(&self, height: f32) -> gpui::Div {
        div()
            .flex_shrink_0()
            .h(px(height))
            .rounded(px(4.0))
            .bg(hsla(self.p.foreground_at(0.11)))
    }

    /// Bars pulse together so the list reads as one placeholder.
    fn pulse(&self, id: &'static str, element: gpui::Div) -> AnyElement {
        if self.reduce_motion {
            return element.into_any_element();
        }
        element
            .with_animation(
                id,
                Animation::new(Duration::from_millis(1400)).repeat(),
                |element, delta| {
                    let phase = if delta < 0.5 {
                        delta * 2.0
                    } else {
                        (1.0 - delta) * 2.0
                    };
                    let eased = phase * phase * (3.0 - 2.0 * phase);
                    element.opacity(1.0 - 0.55 * eased)
                },
            )
            .into_any_element()
    }

    fn render_list_skeleton(&self) -> AnyElement {
        let mut children: Vec<AnyElement> = Vec::new();
        for position in 0..SKELETON_ROW_COUNT {
            if self.group_by_day && position % SKELETON_ROWS_PER_DAY == 0 {
                children.push(
                    div()
                        .px(px(8.0))
                        .pt(px(14.0))
                        .pb(px(6.0))
                        .child(self.skeleton_bar(12.0).w(px(176.0)))
                        .into_any_element(),
                );
            }
            children.push(
                h_flex()
                    .p(px(8.0))
                    .gap(px(8.0))
                    .child(div().w(px(12.0)).flex_shrink_0())
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(8.0))
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap(px(10.0))
                                    .items_center()
                                    .child(self.skeleton_bar(14.0).w(px(72.0)))
                                    .child(self.skeleton_bar(14.0).w(relative(
                                        PROMPT_WIDTHS[position % PROMPT_WIDTHS.len()],
                                    ))),
                            )
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap(px(10.0))
                                    .items_center()
                                    .child(self.skeleton_bar(12.0).w(px(72.0)).opacity(0.7))
                                    .child(
                                        self.skeleton_bar(12.0)
                                            .w(relative(
                                                TITLE_WIDTHS[position % TITLE_WIDTHS.len()],
                                            ))
                                            .opacity(0.7),
                                    ),
                            ),
                    )
                    .into_any_element(),
            );
        }
        self.pulse(
            "find-list-skeleton",
            div()
                .size_full()
                .overflow_hidden()
                .pt(px(6.0))
                .px(px(10.0))
                .children(children),
        )
    }

    /// The bottom pane: the fork picker, or the selected prompt with its path line and footer.
    fn render_bottom_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let pane = v_flex()
            .flex_shrink_0()
            .w_full()
            .border_t_1()
            .border_color(hsla(p.hairline))
            .when(self.fullscreen_preview, |this| this.flex_1().min_h_0())
            .when(!self.fullscreen_preview, |this| {
                this.h(px(BOTTOM_PANE_HEIGHT))
            });
        if self.fork_open {
            return pane.child(self.render_fork_overlay(cx)).into_any_element();
        }
        let row = self.selected_row();
        let loading_without_row = self.loading && row.is_none();
        let agents: Vec<&str> = FIND_PROMPT_AGENTS
            .iter()
            .zip(self.agents)
            .filter(|(_, on)| *on)
            .map(|(agent, _)| *agent)
            .collect();
        let path_line = h_flex()
            .flex_shrink_0()
            .w_full()
            .px(px(14.0))
            .pt(px(10.0))
            .pb(px(6.0))
            .gap(px(8.0))
            .items_center()
            .text_size(px(12.0))
            .line_height(px(16.8))
            .text_color(hsla(p.muted))
            .child(if loading_without_row {
                self.pulse("find-path-skeleton", self.skeleton_bar(12.0).w(px(112.0)))
            } else {
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(
                        row.map(|row| row.project.clone())
                            .filter(|project| !project.is_empty())
                            .unwrap_or_else(|| SharedString::new_static("No project")),
                    )
                    .into_any_element()
            })
            .when(!self.loading, |this| {
                this.child(div().flex_shrink_0().child(format!(
                    "{}/{}",
                    if self.matched == 0 {
                        0
                    } else {
                        self.selection + 1
                    },
                    self.matched
                )))
            })
            .when(!agents.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .child(format!("agents: {}", agents.join(","))),
                )
            })
            .when(self.project.is_some(), |this| {
                this.child(div().flex_shrink_0().child("project filter on"))
            });
        let preview_body = if loading_without_row {
            self.pulse(
                "find-preview-skeleton",
                v_flex().gap(px(10.0)).px(px(12.0)).py(px(4.0)).children(
                    PARAGRAPH_WIDTHS
                        .iter()
                        .map(|width| self.skeleton_bar(14.0).w(relative(*width))),
                ),
            )
        } else {
            // `.p_0()` replaces the editor's own inset, so the text lines up with the path line above.
            Textarea::new(&self.preview_input)
                .readonly(true)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .p_0()
                .into_any_element()
        };
        let preview = div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .when(self.preview_focused, |this| {
                this.shadow(vec![inset_ring(
                    crate::app::window::native_modal_kit::rgba_of(p.border, p.border.a * 0.7),
                )])
            })
            .child(
                div()
                    .id("find-preview")
                    .size_full()
                    .px(px(14.0))
                    .text_size(px(14.0))
                    .line_height(px(24.0))
                    .text_color(hsla(p.foreground))
                    .map(|this| {
                        if self.wrap_preview {
                            this.overflow_y_scroll()
                        } else {
                            this.overflow_scroll()
                        }
                    })
                    .track_scroll(&self.preview_scroll)
                    .child(preview_body),
            )
            .child(modal_edge_scrollbar(&self.preview_scroll));
        let footer = h_flex()
            .flex_shrink_0()
            .w_full()
            .px(px(14.0))
            .pt(px(6.0))
            .pb(px(10.0))
            .text_size(px(12.0))
            .line_height(px(16.8))
            .text_color(hsla(p.muted))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(row.map(|row| row.footer.clone()).unwrap_or_default()),
            );
        pane.child(path_line)
            .child(preview)
            .child(footer)
            .into_any_element()
    }

    fn render_fork_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let chips = FIND_PROMPT_AGENTS[..FIND_PROMPT_FORK_AGENT_COUNT]
            .iter()
            .enumerate()
            .map(|(index, agent)| {
                let dot = self.agent_color(index);
                h_flex()
                    .id(("find-fork-agent", index))
                    .gap(px(6.0))
                    .items_center()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(hsla(p.border))
                    .bg(hsla(p.accent_at(0.4)))
                    .hover(move |this| this.bg(hsla(p.accent_at(0.7))))
                    .px(px(10.0))
                    .py(px(6.0))
                    .text_size(px(14.0))
                    .line_height(px(19.6))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, _window, cx| {
                            this.fork_selected(index, cx);
                        }),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{}", index + 1)),
                    )
                    .child(
                        div()
                            .size(px(8.0))
                            .rounded_full()
                            .when_some(dot, |this, dot| this.bg(hsla(dot))),
                    )
                    .child(*agent)
            })
            .collect::<Vec<_>>();
        v_flex()
            .size_full()
            .min_h_0()
            .gap(px(4.0))
            .px(px(12.0))
            .py(px(8.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.8))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.muted))
                    .child("FORK PROMPT INTO"),
            )
            .child(
                div().flex_1().min_h_0().child(
                    h_flex()
                        .flex_wrap()
                        .gap(px(6.0))
                        .py(px(4.0))
                        .children(chips),
                ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.8))
                    .text_color(hsla(p.muted))
                    .child("Press 1-6, or click an agent · Esc cancels"),
            )
            .into_any_element()
    }

    fn render_notice(&self) -> Option<AnyElement> {
        let notice = self.notice.as_ref()?;
        let p = self.p;
        let error = notice.kind == FindNoticeKind::Error;
        let (border, background, color) = if error {
            (
                crate::app::window::native_modal_kit::rgba_of(p.destructive, 0.4),
                crate::app::window::native_modal_kit::rgba_of(p.destructive, 0.1),
                p.foreground,
            )
        } else {
            (p.hairline, p.accent_at(0.3), p.muted)
        };
        let content: AnyElement = match notice.detail.as_ref() {
            Some(detail) => {
                let text: SharedString = format!("{} {}", notice.message, detail).into();
                let start = notice.message.len() + 1;
                let end = text.len();
                StyledText::new(text)
                    .with_highlights([(
                        start..end,
                        HighlightStyle {
                            color: Some(hsla(crate::app::window::native_modal_kit::rgba_of(
                                color,
                                color.a * 0.7,
                            ))),
                            ..Default::default()
                        },
                    )])
                    .into_any_element()
            }
            None => notice.message.clone().into_any_element(),
        };
        Some(
            div()
                .flex_shrink_0()
                .w_full()
                .border_t_1()
                .border_color(hsla(border))
                .bg(hsla(background))
                .px(px(14.0))
                .py(px(8.0))
                .text_size(px(12.0))
                .line_height(px(16.8))
                .text_color(hsla(color))
                .child(content)
                .into_any_element(),
        )
    }

    /// `^e`: the whole prompt over the window.
    fn render_expanded(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.expanded_prompt {
            return None;
        }
        let row = self.selected_row()?;
        let p = self.p;
        let text = self
            .selected_text
            .clone()
            .unwrap_or_else(|| row.text.clone());
        Some(
            v_flex()
                .id("find-expanded")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                // React drew background/95 over a backdrop blur; nothing in a window can blur what it drew, so the full-prompt view sits on the solid menu colour (the window colour, or its lifted solid twin under glass, where the window fill is see-through).
                .bg(hsla(p.popover))
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .w_full()
                        .gap(px(8.0))
                        .items_center()
                        .border_b_1()
                        .border_color(hsla(p.hairline))
                        .px(px(14.0))
                        .py(px(10.0))
                        .text_size(px(13.0))
                        .line_height(px(18.2))
                        .text_color(hsla(p.muted))
                        .child(
                            div()
                                .flex_shrink_0()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(row.agent_color))
                                .child(row.agent.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(row.title.clone()),
                        )
                        .child(
                            div()
                                .id("find-expanded-close")
                                .flex_shrink_0()
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(rgba_of(p.border, 0.0))
                                .bg(hsla(neutral_button_fills(p.light).rest))
                                .px(px(10.0))
                                .py(px(4.0))
                                .cursor_pointer()
                                .hover(move |this| {
                                    this.bg(hsla(neutral_button_fills(p.light).hover))
                                })
                                .active(move |this| {
                                    this.bg(hsla(neutral_button_fills(p.light).pressed))
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                                        this.expanded_prompt = false;
                                        cx.notify();
                                    }),
                                )
                                .child("Close"),
                        ),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .child(
                            div()
                                .id("find-expanded-body")
                                .size_full()
                                .overflow_y_scroll()
                                .track_scroll(&self.expanded_scroll)
                                .px(px(16.0))
                                .py(px(12.0))
                                .text_size(px(15.0))
                                .line_height(px(28.0))
                                .text_color(hsla(p.foreground))
                                .child(text),
                        )
                        .child(modal_edge_scrollbar(&self.expanded_scroll)),
                )
                .into_any_element(),
        )
    }
}
