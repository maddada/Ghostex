//! The Unified header's filter bar: a chip for each pick and for the session text, and a count for
//! the picks that don't fit.
//!
//! CDXC:Sidebar 2026-10-31 DECISION:
//! User (Q880, "chips"): the row that held the Search sessions box is a bar that always shows what is filtered, so you can see which project is picked. Each pick is a chip with its own X, then a count that opens the menu; with nothing picked it reads All sessions. Session text search moved into the filter dropdown's field: a "Search sessions for" row comes first while typing, and Enter on it makes the text a chip like the rest, narrowing the list from then on. The chip's text isn't saved across restarts (`UnifiedSavedState`). This supersedes Q854's search field in the header.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::h_flex;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use serde_json::{Value, json};

use super::model::UnifiedSidebarSnapshot;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::menus::MenuOwner;

/// The gap between chips, before the zoom.
const CHIP_GAP: f32 = 4.0;
const CHIP_HEIGHT: f32 = 24.0;
/// The widest a chip's whole width gets, before the zoom.
const CHIP_MAX_WIDTH: f32 = 112.0;
const CHIP_TEXT_SIZE: f32 = 12.0;
/// A chip's left padding and its X with the right padding, before the zoom.
const CHIP_LEFT: f32 = 5.0;
const CHIP_X: f32 = 3.0 + 16.0 + 2.0;
/// A project chip's tile and the gap after it, before the zoom.
const CHIP_TILE: f32 = 16.0 + 4.0;
/// The count's padding on both sides together, before the zoom.
const COUNT_PADDING: f32 = 10.0;

/// One chip of the bar.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FilterChip {
    pub(super) label: String,
    /// The repository whose tile it draws.
    pub(super) repo_key: Option<String>,
    /// The session text search, which the bar keeps in sight whatever else has to give way.
    pub(super) text: bool,
    /// What its X runs.
    pub(super) remove: Value,
}

/// The bar's chips: projects, machines and Spaces, as the filter button's tooltip names them, then
/// the text. A remote machine's Space says which machine, since two machines can both have one
/// called Other.
pub(super) fn filter_chips(unified: &UnifiedSidebarSnapshot) -> Vec<FilterChip> {
    let filter = &unified.filter;
    let toggle = |kind: &str, key: &str| json!({"type": "unifiedFilter", "kind": kind, "key": key});
    let mut chips: Vec<FilterChip> = Vec::new();
    for option in &unified.options.repos {
        if filter.repos.contains(&option.key) {
            chips.push(FilterChip {
                label: option.label.clone(),
                repo_key: Some(option.key.clone()),
                text: false,
                remove: toggle("repo", &option.key),
            });
        }
    }
    for option in &unified.options.machines {
        if filter.machines.contains(&option.key) {
            chips.push(FilterChip {
                label: option.label.clone(),
                repo_key: None,
                text: false,
                remove: toggle("machine", &option.key),
            });
        }
    }
    for option in &unified.options.spaces {
        if filter.spaces.contains(&option.key) {
            chips.push(FilterChip {
                label: match &option.detail {
                    Some(machine) => format!("{} · {machine}", option.label),
                    None => option.label.clone(),
                },
                repo_key: None,
                text: false,
                remove: toggle("space", &option.key),
            });
        }
    }
    let query = filter.query.trim();
    if !query.is_empty() {
        chips.push(FilterChip {
            label: format!("\u{201c}{query}\u{201d}"),
            repo_key: None,
            text: true,
            remove: json!({"type": "unifiedFilter", "kind": "text", "text": ""}),
        });
    }
    chips
}

/// Which chips show, in order, in `room`: all of them when they fit; else as many picks as fit in
/// order, beside the text chip and the count of the rest (`count_width`). At least one chip always
/// shows, the first it can, and it gives up width to fit (the chip the caller lets shrink).
pub(super) fn fit_filter_chips(
    widths: &[f32],
    text: Option<usize>,
    room: f32,
    gap: f32,
    count_width: f32,
) -> Vec<usize> {
    let all: Vec<usize> = (0..widths.len()).collect();
    let total: f32 = widths.iter().sum::<f32>() + gap * (widths.len().saturating_sub(1) as f32);
    if total <= room {
        return all;
    }
    let mut used = count_width;
    if let Some(text) = text {
        used += gap + widths[text];
    }
    let mut shown: Vec<usize> = Vec::new();
    for (index, width) in widths.iter().enumerate() {
        if Some(index) == text {
            continue;
        }
        if used + gap + width > room {
            break;
        }
        used += gap + width;
        shown.push(index);
    }
    match text {
        Some(text) => shown.push(text),
        None if shown.is_empty() => shown.extend(all.first()),
        None => {}
    }
    shown
}

impl GhostexGpuiApp {
    /// The bar: everything between the sidebar's edge and the header's three buttons, `room` wide.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_native_unified_filter_bar(
        &self,
        unified: &UnifiedSidebarSnapshot,
        appearance: &SidebarAppearance,
        menu: Rc<Value>,
        owner: MenuOwner,
        room: f32,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let gap = CHIP_GAP * scale;
        let text_size = CHIP_TEXT_SIZE * scale;
        let chips = filter_chips(unified);
        let text_system = window.text_system().clone();
        let measure = |text: &str| {
            crate::app::native_sidebar::link_chips::ui_text_width(&text_system, text, text_size)
        };
        let widths: Vec<f32> = chips
            .iter()
            .map(|chip| {
                let tile = if chip.repo_key.is_some() {
                    CHIP_TILE
                } else {
                    0.0
                };
                ((CHIP_LEFT + tile + CHIP_X) * scale + measure(&chip.label))
                    .min(CHIP_MAX_WIDTH * scale)
            })
            .collect();
        let hidden_at_most = chips.len();
        let count_width = measure(&format!("+{hidden_at_most}")) + COUNT_PADDING * scale;
        let shown = fit_filter_chips(
            &widths,
            chips.iter().position(|chip| chip.text),
            room,
            gap,
            count_width,
        );
        let hidden: Vec<String> = chips
            .iter()
            .enumerate()
            .filter(|(index, _)| !shown.contains(index))
            .map(|(_, chip)| chip.label.clone())
            .collect();
        let show_icons = self
            .native_sidebar
            .snapshot
            .as_ref()
            .is_none_or(|snapshot| super::cards::show_project_icons(&snapshot.hud));
        let tooltip_delay = appearance.tooltip_delay;
        let pointer_inside = self.native_sidebar.pointer_inside;
        let open_on_click = move |id: gpui::ElementId, menu: Rc<Value>, owner: MenuOwner| {
            div().id(id).role(gpui::Role::Button).on_click(
                move |event: &gpui::ClickEvent, window: &mut gpui::Window, cx: &mut gpui::App| {
                    cx.stop_propagation();
                    Self::show_unified_filter_menu(&menu, owner.get(), event, scale, window, cx);
                },
            )
        };
        let chip_elements = shown.iter().enumerate().map(|(position, &index)| {
            let chip = &chips[index];
            let remove = chip.remove.clone();
            let label = chip.label.clone();
            let tooltip = chip.label.clone();
            let tile = chip.repo_key.as_ref().map(|key| {
                match unified.repo_icons.get(key).cloned().filter(|_| show_icons) {
                    Some(icon) => gpui::img(icon)
                        .size(px(16.0 * scale))
                        .flex_shrink_0()
                        .into_any_element(),
                    None => crate::app::native_sidebar::project_header::letter_tile(
                        &chip.label,
                        appearance.muted.opacity(0.12),
                        appearance.muted,
                        scale,
                    ),
                }
            });
            open_on_click(
                ("native-unified-filter-chip", index).into(),
                menu.clone(),
                owner.clone(),
            )
            .aria_label(format!("Filter: {label}"))
            .h(px(CHIP_HEIGHT * scale))
            .max_w(px(CHIP_MAX_WIDTH * scale))
            .min_w_0()
            .map(|chip| {
                // The first one gives up width when what shows is still too wide.
                if position == 0 {
                    chip.flex_shrink(1.0)
                } else {
                    chip.flex_shrink_0()
                }
            })
            .flex()
            .items_center()
            .gap(px(4.0 * scale))
            .pl(px(CHIP_LEFT * scale))
            .pr(px(2.0 * scale))
            .rounded(px(5.0 * scale))
            .bg(appearance.selected)
            .text_size(px(text_size))
            .text_color(appearance.foreground)
            .children(tile)
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label.clone()),
            )
            .child(
                div()
                    .id(("native-unified-filter-chip-remove", index))
                    .role(gpui::Role::Button)
                    .aria_label(format!("Remove filter: {label}"))
                    .size(px(16.0 * scale))
                    .flex_shrink_0()
                    .ml(px(-scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0 * scale))
                    .hover(|button| button.bg(appearance.hover))
                    .child(
                        gpui::svg()
                            .path("titlebar/x.svg")
                            .size(px(11.0 * scale))
                            .text_color(appearance.muted),
                    )
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.dispatch_native_sidebar_ui(remove.clone(), cx);
                    })),
            )
            .when(pointer_inside, |chip| {
                chip.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::BelowLeft,
                    tooltip_delay,
                    move |window, cx| {
                        crate::app::native_sidebar::tooltips::sidebar_style_tooltip(
                            tooltip.clone(),
                            window,
                            cx,
                        )
                    },
                )
            })
            .into_any_element()
        });
        let count = (!hidden.is_empty()).then(|| {
            let tooltip = hidden.join(", ");
            open_on_click(
                "native-unified-filter-count".into(),
                menu.clone(),
                owner.clone(),
            )
            .aria_label(format!("{} more: {tooltip}", hidden.len()))
            .h(px(CHIP_HEIGHT * scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(COUNT_PADDING / 2.0 * scale))
            .rounded(px(5.0 * scale))
            .bg(appearance.selected)
            .hover(|button| button.bg(appearance.hover))
            .text_size(px(text_size))
            .text_color(appearance.foreground)
            .child(format!("+{}", hidden.len()))
            .when(pointer_inside, |count| {
                count.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::BelowLeft,
                    tooltip_delay,
                    move |window, cx| {
                        crate::app::native_sidebar::tooltips::sidebar_style_tooltip(
                            tooltip.clone(),
                            window,
                            cx,
                        )
                    },
                )
            })
        });
        let all_sessions = chips.is_empty().then(|| {
            open_on_click(
                "native-unified-filter-all".into(),
                menu.clone(),
                owner.clone(),
            )
            .aria_label("Filter sessions: All sessions")
            .h(px(CHIP_HEIGHT * scale))
            .min_w_0()
            .flex()
            .items_center()
            .px(px(6.0 * scale))
            .rounded(px(5.0 * scale))
            .hover(|button| button.bg(appearance.hover))
            .text_size(px(13.0 * scale))
            .text_color(appearance.muted)
            .overflow_hidden()
            .whitespace_nowrap()
            .child("All sessions")
        });
        h_flex()
            .id("native-unified-filter-bar")
            .min_w_0()
            .flex_1()
            .gap(px(gap))
            .overflow_hidden()
            .children(all_sessions)
            .children(chip_elements.collect::<Vec<_>>())
            .children(count)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostex_gx_core::unified_sidebar::{
        UnifiedFilter, UnifiedFilterOption, UnifiedFilterOptions,
    };

    fn option(key: &str, label: &str, detail: Option<&str>) -> UnifiedFilterOption {
        UnifiedFilterOption {
            key: key.to_string(),
            label: label.to_string(),
            detail: detail.map(str::to_string),
            ..UnifiedFilterOption::default()
        }
    }

    fn snapshot(filter: UnifiedFilter) -> UnifiedSidebarSnapshot {
        UnifiedSidebarSnapshot {
            options: UnifiedFilterOptions {
                repos: vec![option("g", "Ghostex", Some("Local, Windows"))],
                machines: vec![option("local", "Local", None), option("w", "Windows", None)],
                spaces: vec![
                    option("s1", "Other", None),
                    option("s2", "Other", Some("Windows")),
                ],
            },
            filter,
            ..UnifiedSidebarSnapshot::default()
        }
    }

    #[test]
    fn nothing_picked_is_no_chips() {
        assert!(filter_chips(&snapshot(UnifiedFilter::default())).is_empty());
    }

    #[test]
    fn picks_come_in_order_and_the_text_comes_last() {
        let chips = filter_chips(&snapshot(UnifiedFilter {
            repos: ["g".to_string()].into(),
            machines: ["w".to_string()].into(),
            spaces: ["s2".to_string()].into(),
            query: " auth ".to_string(),
        }));
        let labels: Vec<&str> = chips.iter().map(|chip| chip.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Ghostex",
                "Windows",
                "Other · Windows",
                "\u{201c}auth\u{201d}"
            ]
        );
        assert_eq!(chips[0].repo_key.as_deref(), Some("g"));
        assert!(chips[3].text);
        // Each X takes off its own pick, and the text's X empties the text.
        assert_eq!(chips[1].remove["kind"], "machine");
        assert_eq!(chips[1].remove["key"], "w");
        assert_eq!(chips[3].remove["kind"], "text");
        assert_eq!(chips[3].remove["text"], "");
    }

    #[test]
    fn a_query_of_spaces_is_no_chip() {
        let chips = filter_chips(&snapshot(UnifiedFilter {
            query: "   ".to_string(),
            ..UnifiedFilter::default()
        }));
        assert!(chips.is_empty());
    }

    // Measures from the page: 200px at a 300px sidebar, 135px at the narrowest.
    #[test]
    fn everything_shows_when_it_fits() {
        assert_eq!(
            fit_filter_chips(&[90.0, 76.0, 20.0], None, 200.0, 4.0, 24.0),
            [0, 1, 2]
        );
    }

    #[test]
    fn what_does_not_fit_goes_in_the_count() {
        assert_eq!(
            fit_filter_chips(&[90.0, 76.0, 76.0], None, 200.0, 4.0, 24.0),
            [0, 1]
        );
        assert_eq!(
            fit_filter_chips(&[90.0, 76.0, 76.0], None, 135.0, 4.0, 24.0),
            [0]
        );
    }

    #[test]
    fn later_chips_never_jump_ahead_of_one_that_does_not_fit() {
        assert_eq!(
            fit_filter_chips(&[90.0, 112.0, 30.0], None, 150.0, 4.0, 24.0),
            [0]
        );
    }

    #[test]
    fn a_first_chip_too_wide_for_the_room_still_shows() {
        assert_eq!(
            fit_filter_chips(&[112.0, 112.0], None, 100.0, 4.0, 24.0),
            [0]
        );
    }

    #[test]
    fn the_text_chip_stays_in_sight_when_picks_give_way() {
        // The text at the end of the picks that fit beside it.
        assert_eq!(
            fit_filter_chips(&[90.0, 76.0, 60.0], Some(2), 200.0, 4.0, 24.0),
            [0, 2]
        );
        // Too narrow for a pick beside it: the pick is in the count.
        assert_eq!(
            fit_filter_chips(&[90.0, 60.0], Some(1), 135.0, 4.0, 24.0),
            [1]
        );
    }
}
