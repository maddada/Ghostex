//! The agent and project filter dropdowns, drawn in the window like the React popovers
//! (`SearchableDropdownContent`, 260px, right-aligned 6px under the trigger).
//!
//! CDXC:PromptSearch 2026-09-16 DECISION:
//! User: the agent and project filters at the top right of Find are dropdowns like the Quick Access Sessions tab (searchable project list, checkmarks on the active choices), not hint keys that open a pane at the bottom. The hotkeys still open these menus.
//! Moved here from find-prompts-filters.tsx with the native port (2026-09-27).
//!
//! CDXC:PromptSearch 2026-09-16 DECISION:
//! User: the agent and project dropdowns keep a fixed width and truncate the picked value, so the toolbar does not resize when the selection changes.
//!
//! The "…" button that holds Fav, View, Copy and Fork in a narrow window (`toolbar_steps.rs`) opens the same kind of panel, one row per action with its hotkey.
use super::model::FIND_PROMPT_AGENTS;
use super::window::{FindMenu, GpuiFindPromptsModalWindow};
use crate::app::window::native_modal_kit::{hsla, neutral_button_fills};
use crate::app::window::quick_access::chrome::quick_access_tooltip;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, BoxShadow, ClickEvent, Context, InteractiveElement as _, IntoElement,
    MouseDownEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    anchored, canvas, deferred, div, point, px, svg,
};
use gpui_component::input::Input;
use gpui_component::{h_flex, v_flex};

const MENU_WIDTH: f32 = 260.0;
const MENU_LIST_MAX_HEIGHT: f32 = 288.0;
const ICON_CHECK: &str = "titlebar/check.svg";
const ICON_SEARCH: &str = "modals/kit/search.svg";

struct MenuRow {
    label: SharedString,
    dot: Option<gpui::Rgba>,
    checked: bool,
    tooltip: Option<String>,
    /// The action's hotkey, drawn at the right.
    hint: Option<String>,
    /// A row that does nothing for now (no prompt is selected).
    disabled: bool,
}

fn hotkey(chord: &str) -> String {
    crate::hotkey_label::terminal_overlay_hotkey_chord_label(chord)
}

impl GpuiFindPromptsModalWindow {
    /// The four toolbar actions as menu rows, in the buttons' order; the hotkeys are the tooltips'.
    fn action_menu_rows(&self) -> Vec<(usize, MenuRow)> {
        let favorite = self.selected_row().is_some_and(|row| row.favorite);
        let has_row = self.selected_row().is_some();
        let row = |label: &'static str, chord: &str| MenuRow {
            label: SharedString::new_static(label),
            dot: None,
            checked: false,
            tooltip: None,
            hint: Some(hotkey(chord)),
            disabled: !has_row,
        };
        vec![
            row(
                if favorite {
                    "Remove from favorites"
                } else {
                    "Favorite this prompt"
                },
                "ctrl+f",
            ),
            row(
                if self.expanded_prompt {
                    "Close the full prompt"
                } else {
                    "View the full prompt"
                },
                "ctrl+e",
            ),
            row("Copy this prompt", "ctrl+y"),
            row(
                if self.fork_open {
                    "Cancel fork"
                } else {
                    "Fork into another agent"
                },
                "ctrl+o",
            ),
        ]
        .into_iter()
        .enumerate()
        .collect()
    }

    /// The "…" button that stands in for Fav, View, Copy and Fork in a narrow window.
    pub(super) fn actions_menu_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let open = self.open_menu == Some(FindMenu::Actions);
        let fills = neutral_button_fills(p.light);
        let bounds = self.actions_trigger.clone();
        let text = if open { p.foreground } else { p.muted };
        h_flex()
            .id("find-actions-menu")
            .relative()
            .flex_shrink_0()
            .size(px(32.0))
            .items_center()
            .justify_center()
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(crate::app::window::native_modal_kit::rgba_of(
                p.border, 0.0,
            )))
            .bg(hsla(if open { fills.pressed } else { fills.rest }))
            .text_color(hsla(text))
            .cursor_pointer()
            .hover(move |this| this.text_color(hsla(p.foreground)).bg(hsla(fills.hover)))
            .active(move |this| this.bg(hsla(fills.pressed)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.toggle_menu(FindMenu::Actions, window, cx);
            }))
            .tooltip(move |window, cx| {
                quick_access_tooltip(
                    "Favorite, view, copy or fork this prompt".to_string(),
                    window,
                    cx,
                )
            })
            .child(
                canvas(move |rect, _, _| bounds.set(Some(rect)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                svg()
                    .path(crate::app::consts::TITLEBAR_ICON_DOTS)
                    .size(px(14.0))
                    .flex_shrink_0()
                    .text_color(hsla(text)),
            )
            .into_any_element()
    }

    pub(super) fn render_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.open_menu?;
        let trigger = match menu {
            FindMenu::Agent => self.agent_trigger.get(),
            FindMenu::Project => self.project_trigger.get(),
            FindMenu::Actions => self.actions_trigger.get(),
        }?;
        let p = self.p;
        let rows: Vec<(usize, MenuRow)> = match menu {
            FindMenu::Agent => std::iter::once(MenuRow {
                label: SharedString::new_static("All agents"),
                dot: None,
                checked: !self.agents.iter().any(|on| *on),
                tooltip: None,
                hint: None,
                disabled: false,
            })
            .chain(
                FIND_PROMPT_AGENTS
                    .iter()
                    .enumerate()
                    .map(|(index, agent)| MenuRow {
                        label: SharedString::new_static(agent),
                        dot: Some(self.agent_color(index).unwrap_or(p.muted)),
                        checked: self.agents[index],
                        tooltip: None,
                        hint: None,
                        disabled: false,
                    }),
            )
            .enumerate()
            .collect(),
            FindMenu::Project => self
                .project_menu_rows()
                .into_iter()
                .enumerate()
                .map(|(position, row)| {
                    let row = match row.and_then(|index| self.project_facets.get(index)) {
                        None => MenuRow {
                            label: SharedString::new_static("All projects"),
                            dot: None,
                            checked: self.project.is_none(),
                            tooltip: None,
                            hint: None,
                            disabled: false,
                        },
                        Some(facet) => MenuRow {
                            label: facet.name.clone().into(),
                            dot: None,
                            checked: self.project.as_deref() == Some(facet.path.as_str()),
                            tooltip: Some(facet.path.clone()),
                            hint: None,
                            disabled: false,
                        },
                    };
                    (position, row)
                })
                .collect(),
            FindMenu::Actions => self.action_menu_rows(),
        };
        let highlight = self.menu_highlight.min(rows.len().saturating_sub(1));
        let items = rows
            .into_iter()
            .map(|(index, row)| {
                let highlighted = index == highlight;
                h_flex()
                    .id(("find-menu-row", index))
                    .w_full()
                    .flex_shrink_0()
                    .h(px(32.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .gap(px(8.0))
                    .items_center()
                    .rounded(px(6.0))
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.foreground))
                    .cursor_pointer()
                    .when(row.disabled, |this| this.opacity(0.5))
                    .when(highlighted && !row.disabled, |this| {
                        this.bg(hsla(p.foreground_at(0.08)))
                    })
                    .on_mouse_move(cx.listener(move |this, _, _window, cx| {
                        if this.menu_highlight != index {
                            this.menu_highlight = index;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.choose_menu_row(index, window, cx);
                    }))
                    .when_some(row.tooltip, |this, tooltip| {
                        this.tooltip(move |window, cx| {
                            quick_access_tooltip(tooltip.clone(), window, cx)
                        })
                    })
                    .children(row.dot.map(|dot| {
                        div()
                            .flex_shrink_0()
                            .size(px(8.0))
                            .rounded_full()
                            .bg(hsla(dot))
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(row.label),
                    )
                    .when_some(row.hint, |this, hint| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(12.0))
                                .text_color(hsla(p.muted))
                                .child(hint),
                        )
                    })
                    .when(row.checked, |this| {
                        this.child(
                            svg()
                                .path(ICON_CHECK)
                                .size(px(16.0))
                                .flex_shrink_0()
                                .text_color(hsla(p.foreground)),
                        )
                    })
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let empty = items.is_empty();
        let search = (menu == FindMenu::Project).then(|| {
            div()
                .w_full()
                .p(px(8.0))
                .border_b_1()
                .border_color(hsla(p.popover_border))
                .child(
                    h_flex()
                        .w_full()
                        .h(px(32.0))
                        .pl(px(10.0))
                        .pr(px(10.0))
                        .gap(px(6.0))
                        .items_center()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(hsla(p.field_border))
                        .bg(hsla(p.field))
                        .shadow(vec![BoxShadow {
                            color: hsla(p.field_ring),
                            offset: point(px(0.0), px(0.0)),
                            blur_radius: px(0.0),
                            spread_radius: px(3.0),
                            inset: false,
                        }])
                        .child(
                            div().flex_1().min_w_0().child(
                                Input::new(&self.menu_search)
                                    .appearance(false)
                                    .bordered(false)
                                    .focus_bordered(false)
                                    .w_full()
                                    .px(px(0.0))
                                    .py(px(0.0))
                                    .text_size(px(13.0))
                                    .text_color(hsla(p.foreground)),
                            ),
                        )
                        .child(
                            svg()
                                .path(ICON_SEARCH)
                                .size(px(16.0))
                                .flex_shrink_0()
                                .text_color(hsla(p.muted)),
                        ),
                )
        });
        let panel = v_flex()
            .id("find-filter-menu")
            .w(px(MENU_WIDTH))
            .occlude()
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(p.popover_border))
            .bg(hsla(p.popover))
            .shadow(vec![BoxShadow {
                color: hsla(crate::app::window::native_modal_kit::modal_rgba(
                    0x000000,
                    if p.light { 0.12 } else { 0.35 },
                )),
                offset: point(px(0.0), px(12.0)),
                blur_radius: px(28.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .font_family(self.font_family.clone())
            .on_mouse_down_out(
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    // The trigger's own click toggles the menu.
                    if trigger.contains(&event.position) {
                        return;
                    }
                    this.close_menu(window, cx);
                }),
            )
            .children(search)
            .child(
                v_flex()
                    .id("find-filter-menu-list")
                    .w_full()
                    .max_h(px(MENU_LIST_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .p(px(4.0))
                    .children(items)
                    .when(empty, |this| {
                        this.child(
                            div()
                                .w_full()
                                .py(px(24.0))
                                .text_center()
                                .text_size(px(14.0))
                                .line_height(px(20.0))
                                .text_color(hsla(p.foreground))
                                .child("No projects found."),
                        )
                    }),
            );
        let position = point(trigger.right() - px(MENU_WIDTH), trigger.bottom() + px(6.0));
        Some(
            deferred(
                anchored()
                    .position(position)
                    .snap_to_window_with_margin(px(8.0))
                    .child(panel),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
