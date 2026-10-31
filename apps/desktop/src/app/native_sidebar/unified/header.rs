//! The Unified sidebar's header: the filter bar, the filter, Add project and New session.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{ElementExt as _, h_flex};
use serde_json::{Value, json};

use super::model::UnifiedSidebarSnapshot;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;

/// The header's side padding, its three buttons and the gap before each, before the zoom.
const HEADER_PADDING: f32 = 8.0;
const HEADER_BUTTON: f32 = 26.0;
const HEADER_GAP: f32 = 2.0;
const HEADER_BUTTONS: f32 = 3.0;

/// The filter menu: projects (by repository), machines and Spaces, each a pick that narrows the
/// list, ticked while it does. Typing narrows its rows (`SidebarMenuSearch`), and a project's row
/// carries its Remove button (`activate_native_sidebar_menu_secondary`).
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User (Q788): the header's filter narrows by projects, machines and Spaces in one menu, which stands in for the machine tabs and Space tiles the Unified sidebar does not draw.
///
/// CDXC:Sidebar 2026-10-31 WHY:
/// Only a project's row gets Remove, as the user asked. A Space has its own Delete behind a dialog, and deleting one moves its projects to Other rather than taking anything away, so a two-click button beside it would be a different action wearing the same icon; the Other Space and a machine can't be removed at all.
pub(super) fn filter_menu_items(unified: &UnifiedSidebarSnapshot) -> Value {
    let filter = &unified.filter;
    let mut items = vec![json!({
        "label": "All sessions",
        "checked": filter.keeps_everything(),
        "command": {"type": "unifiedFilter", "kind": "clear"},
        "searchHidden": true,
        "search": {
            "placeholder": "Search or filter",
            "submit": {
                "label": "Search sessions for",
                "icon": "search",
                "command": {"type": "unifiedFilter", "kind": "text"},
            },
        },
    })];
    let mut section = |title: &str,
                       kind: &str,
                       options: &[ghostex_gx_core::unified_sidebar::UnifiedFilterOption],
                       picked: &std::collections::BTreeSet<String>,
                       least: usize| {
        if options.len() < least && picked.is_empty() {
            return;
        }
        items.push(json!({"separator": true}));
        items.push(json!({"label": title, "heading": true}));
        for option in options {
            let mut item = json!({
                "label": option.label,
                "checked": picked.contains(&option.key),
                "command": {"type": "unifiedFilter", "kind": kind, "key": option.key},
            });
            // On its own line: the rows are tall enough, and the machines' names no longer get cut.
            if let Some(detail) = &option.detail {
                item["detail"] = Value::String(detail.clone());
            }
            if !option.project_group_ids.is_empty() {
                item["secondary"] = json!({
                    "icon": "x",
                    "label": "",
                    "confirm": true,
                    "ariaLabel": format!("Remove {}", option.label),
                    "command": {"type": "unifiedRemoveProject", "key": option.key},
                });
            }
            items.push(item);
        }
    };
    // The two short lists first, so they are not lost below a long list of projects.
    section(
        "Machines",
        "machine",
        &unified.options.machines,
        &filter.machines,
        2,
    );
    section(
        "Spaces",
        "space",
        &unified.options.spaces,
        &filter.spaces,
        2,
    );
    // A single project is no filter, but its row is the way to remove it.
    section("Projects", "repo", &unified.options.repos, &filter.repos, 1);
    Value::Array(items)
}

/// How much of the window the filter menu may take.
const FILTER_MENU_WINDOW_SHARE: f32 = 0.75;

/// The filter menu's tallest height, before the zoom (`SidebarMenuOpener::capped` applies it).
fn filter_menu_max_height(window_height: f32, scale: f32) -> f32 {
    window_height * FILTER_MENU_WINDOW_SHARE / scale
}

/// What the filter button says: everything, or the first pick and how many more.
fn filter_label(unified: &UnifiedSidebarSnapshot) -> String {
    let filter = &unified.filter;
    let picked: Vec<&str> = unified
        .options
        .repos
        .iter()
        .filter(|option| filter.repos.contains(&option.key))
        .chain(
            unified
                .options
                .machines
                .iter()
                .filter(|option| filter.machines.contains(&option.key)),
        )
        .chain(
            unified
                .options
                .spaces
                .iter()
                .filter(|option| filter.spaces.contains(&option.key)),
        )
        .map(|option| option.label.as_str())
        .collect();
    match picked.as_slice() {
        [] => "All sessions".to_string(),
        [one] => (*one).to_string(),
        [first, rest @ ..] => format!("{first} +{}", rest.len()),
    }
}

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User (2026-10-30): "please make the sidebar filter in unified sidebar mode not take all the height like it does now". The filter menu is a dropdown: hung under the filter button with its left edge on the button's, shrinking to fit a short list, and its rows scroll inside it with the bar showing only while they scroll. Pressing the button again closes it. Its rows have since grown to a session card's height, which leaves about four at the app's usual dropdown height, so its tallest is three quarters of the window (`filter_menu_max_height`): still not all of it, and never past the room left below it. This supersedes `TITLEBAR_POPUP_MENU_MAX_HEIGHT` for this menu.
    pub(super) fn show_unified_filter_menu(
        menu: &Value,
        owner: Option<gpui::Bounds<gpui::Pixels>>,
        event: &gpui::ClickEvent,
        scale: f32,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        use crate::app::native_sidebar::menus::{SidebarMenuOpener, press_position};
        let Some(owner) = owner else {
            Self::show_native_sidebar_menu(menu, event.position(), scale, window, cx);
            return;
        };
        Self::show_native_sidebar_menu_from(
            menu,
            SidebarMenuOpener::below(owner)
                .toggled_by("native-unified-filter", press_position(event))
                .capped(filter_menu_max_height(
                    f32::from(window.viewport_size().height),
                    scale,
                )),
            scale,
            window,
            cx,
        );
    }

    /// The filter bar (filter_bar.rs), then the filter's icon, marked while anything narrows the
    /// list, Add project and New session.
    pub(in crate::app::native_sidebar) fn render_native_unified_header(
        &mut self,
        unified: &UnifiedSidebarSnapshot,
        appearance: &SidebarAppearance,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let icon_button = |id: &'static str, label: String, icon: &'static str| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .size(px(26.0 * scale))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0 * scale))
                .hover(|button| button.bg(appearance.hover))
                .child(
                    gpui::svg()
                        .path(icon)
                        .size(px(15.0 * scale))
                        .text_color(appearance.chrome_icon),
                )
        };
        let filtered = !unified.filter.keeps_everything();
        let menu = std::rc::Rc::new(filter_menu_items(unified));
        let owner = crate::app::native_sidebar::menus::MenuOwner::default();
        let tooltip_label = filter_label(unified);
        let tooltip_delay = appearance.tooltip_delay;
        let sidebar_width = match f32::from(self.native_sidebar.bounds.size.width) {
            0.0 => self.sidebar_width,
            width => width,
        };
        let room = (sidebar_width
            - (HEADER_PADDING * 2.0 + (HEADER_BUTTON + HEADER_GAP) * HEADER_BUTTONS) * scale)
            .max(0.0);
        let bar = self.render_native_unified_filter_bar(
            unified,
            appearance,
            menu.clone(),
            owner.clone(),
            room,
            window,
            cx,
        );
        h_flex()
            .w_full()
            .flex_shrink_0()
            .h(px(36.0 * scale))
            .px(px(HEADER_PADDING * scale))
            .gap(px(HEADER_GAP * scale))
            .child(bar)
            .child(
                icon_button(
                    "native-unified-filter",
                    format!("Filter sessions: {tooltip_label}"),
                    "titlebar/filter.svg",
                )
                .on_prepaint(crate::app::native_sidebar::menus::record_menu_owner(&owner))
                .when(filtered, |button| button.bg(appearance.selected))
                .when(self.native_sidebar.pointer_inside, |button| {
                    button.managed_discrete_tooltip_with_placement(
                        ManagedTooltipPlacement::BelowLeft,
                        tooltip_delay,
                        move |window, cx| {
                            crate::app::native_sidebar::tooltips::sidebar_style_tooltip(
                                tooltip_label.clone(),
                                window,
                                cx,
                            )
                        },
                    )
                })
                .on_click(cx.listener(
                    move |_, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        Self::show_unified_filter_menu(
                            &menu,
                            owner.get(),
                            event,
                            scale,
                            window,
                            cx,
                        );
                    },
                )),
            )
            .child(
                icon_button(
                    "native-unified-add-project",
                    "Add project".to_string(),
                    "titlebar/folder-plus.svg",
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.dispatch_native_sidebar_ui(
                        json!({"type": "sidebarAction", "action": "addProject"}),
                        cx,
                    );
                })),
            )
            .child(
                icon_button(
                    "native-unified-new-session",
                    "New session".to_string(),
                    "titlebar/edit.svg",
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.open_unified_new_session(cx);
                })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostex_gx_core::unified_sidebar::{UnifiedFilterOption, UnifiedFilterOptions};

    fn option(key: &str, groups: &[&str]) -> UnifiedFilterOption {
        UnifiedFilterOption {
            key: key.to_string(),
            label: key.to_string(),
            detail: None,
            project_group_ids: groups.iter().map(ToString::to_string).collect(),
        }
    }

    fn snapshot(repos: Vec<UnifiedFilterOption>) -> UnifiedSidebarSnapshot {
        UnifiedSidebarSnapshot {
            options: UnifiedFilterOptions {
                repos,
                machines: vec![option("local", &[]), option("windows", &[])],
                spaces: vec![option("space-a", &[]), option("space-b", &[])],
            },
            ..UnifiedSidebarSnapshot::default()
        }
    }

    fn row<'a>(items: &'a Value, label: &str) -> &'a Value {
        items
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["label"] == label)
            .unwrap()
    }

    #[test]
    fn the_menu_asks_for_a_search_and_all_sessions_leaves_while_typing() {
        let items = filter_menu_items(&snapshot(vec![option("ghostex", &["project:a"])]));
        assert!(items[0]["search"]["placeholder"].is_string());
        assert_eq!(items[0]["searchHidden"], true);
        assert_eq!(items[0]["search"]["submit"]["command"]["kind"], "text");
    }

    #[test]
    fn all_sessions_is_only_ticked_when_nothing_narrows_the_list() {
        let mut unified = snapshot(Vec::new());
        assert_eq!(filter_menu_items(&unified)[0]["checked"], true);
        unified.filter.query = "auth".to_string();
        assert_eq!(filter_menu_items(&unified)[0]["checked"], false);
    }

    #[test]
    fn only_a_row_with_a_project_can_be_removed() {
        let items = filter_menu_items(&snapshot(vec![
            option("ghostex", &["project:a", "machine:windows:project:b"]),
            option("a-group", &[]),
        ]));
        let remove = &row(&items, "ghostex")["secondary"];
        assert_eq!(remove["confirm"], true);
        assert_eq!(remove["ariaLabel"], "Remove ghostex");
        assert_eq!(remove["command"]["type"], "unifiedRemoveProject");
        assert_eq!(remove["command"]["key"], "ghostex");
        assert!(row(&items, "a-group").get("secondary").is_none());
        for no_button in ["space-a", "windows"] {
            assert!(row(&items, no_button).get("secondary").is_none());
        }
    }

    #[test]
    fn the_menu_may_take_three_quarters_of_the_window_at_any_zoom() {
        assert_eq!(filter_menu_max_height(900.0, 1.0), 675.0);
        // Before the zoom, so the opener's scaling lands on the same share of the window.
        assert_eq!(filter_menu_max_height(900.0, 1.5) * 1.5, 675.0);
    }

    #[test]
    fn a_rows_machines_go_on_its_second_line() {
        let mut repo = option("ghostex", &["project:a"]);
        repo.detail = Some("Local, Windows".to_string());
        let items = filter_menu_items(&snapshot(vec![repo]));
        assert_eq!(row(&items, "ghostex")["detail"], "Local, Windows");
        assert!(row(&items, "ghostex").get("suffix").is_none());
    }

    #[test]
    fn a_lone_project_keeps_its_row_so_it_can_be_removed() {
        let items = filter_menu_items(&snapshot(vec![option("ghostex", &["project:a"])]));
        assert!(row(&items, "ghostex").get("secondary").is_some());
    }
}
