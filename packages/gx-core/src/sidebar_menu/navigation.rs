//! The sidebar's own "more" menu and the Sort & Filter page inside it.
//!
//! Ported from the TypeScript sidebar page (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/navigation.ts`; see git history).

use crate::app_lifecycle::APP_LIFECYCLE_ACTIONS;
use crate::sidebar_actions::{
    GROUP_WORKING_SESSIONS_ACTION, HIDE_PROJECTS_WITHOUT_SESSIONS_ACTION,
    PINNED_ORDER_MANUAL_ACTION, PINNED_ORDER_RECENT_ACTION, SWITCH_SIDEBAR_VIEW_ACTION,
    UNIFIED_GROUP_BY_PROJECT_ACTION, UNIFIED_WORKING_SECTION_ACTION,
};
use crate::sidebar_view::tags::{
    normalize_tag_list_items, tag_list_item_filter, tag_list_item_label, tag_presentation,
    TagCatalog, TagListItemKind,
};
use crate::sidebar_view::{SidebarSettings, SidebarUiState, LOCAL_MACHINE_ID};

use super::commands::{message, MenuCommand};
use super::host::MenuHost;
use super::item::MenuItem;

/// The Unified sidebar's Collapse All row, which folds its project groups.
pub const UNIFIED_TOGGLE_GROUPS_ACTION: &str = "toggleUnifiedGroups";

const DISCORD_URL: &str = "https://discord.gg/df7b3G92CS";

/// `KEEP_AWAKE_DURATION_OPTIONS`.
const KEEP_AWAKE_OPTIONS: [(&str, i64); 3] =
    [("Until turned off", 0), ("2 hours", 120), ("5 hours", 300)];

/// Everything the more menu reads.
pub struct MoreMenuInput<'a> {
    pub ui: &'a SidebarUiState,
    pub settings: &'a SidebarSettings,
    /// What a tag id resolves to for its label and its icon: every machine's catalog.
    pub catalog: &'a TagCatalog,
    /// Which filter rows exist at all, and which of them are enabled: this computer's catalog.
    pub filter_catalog: &'a TagCatalog,
    pub host: &'a MenuHost,
    /// The project group ids the list draws.
    pub drawn_project_group_ids: &'a [String],
    /// The list draws a Cloud Box session, so the Cloud Box filter is offered.
    pub cloud_box_rows: bool,
}

/// `createNativeNavigation(...).moreMenu`.
pub fn more_menu(input: &MoreMenuInput<'_>) -> Vec<MenuItem> {
    let ui = input.ui;
    let is_local = ui.selected_machine_id == LOCAL_MACHINE_ID;
    let intent = |label: &str, icon: &str, action: &str| {
        MenuItem::row(label, icon, MenuCommand::sidebar_action(action))
    };

    let unified = input.settings.unified_sidebar;
    let mut sort: Vec<MenuItem> = Vec::new();
    /*
    CDXC:Sidebar 2026-10-31 DECISION:
    User (issue #359): Collapse All and the Sort & Filter rows that do nothing mustn't act on a hidden list while Unified is on. The Unified sidebar builds its own list across every machine, so here Show Hidden stays and shows on every machine tab (User, Q852: Unified respects Hide), Group by Project and Working Section take Group Working Sessions' place because that is what shapes this list, and the tag rows leave, since tags don't filter it (User, Q853, "hint and heading", left tags out of Unified). A row in this menu always changes what is on screen.
    */
    /*
    CDXC:Sidebar 2026-10-31 DECISION:
    User: "let's please add an item under sort and filter to switch between unified and project views". The row opens the page in both views and says where it goes, writing `sidebarView`, the setting Settings > Sidebar picks. The whole sidebar changes under it, so unlike the page's other rows it closes the menu.
    */
    sort.push(intent(
        if unified {
            "Switch to Project View"
        } else {
            "Switch to Unified View"
        },
        "switch-horizontal",
        SWITCH_SIDEBAR_VIEW_ACTION,
    ));
    sort.push(MenuItem::separator());
    if is_local || unified {
        sort.push(intent("Show Hidden", "eye", "showHidden").with_checked(ui.show_hidden));
    }
    /*
    CDXC:Sidebar 2026-11-07 DECISION:
    User, of Settings > Sidebar & Sessions > Advanced > Pinned order: "works. pls add this setting to sort and filter menu". Both sidebars' Sort & Filter pages list it under a Pinned order heading as two ticked choices, Manual (drag to rearrange) and Most recently updated, written to `pinnedSessionOrder`, the setting the Advanced row picks (one setting, two places; CDXC:Sidebar 2026-11-07 DECISION in sidebar_view/inputs.rs). They go at the end of the page's switches, before the tag rows.
    */
    let pinned_order = |sort: &mut Vec<MenuItem>| {
        let recent =
            input.settings.pinned_session_order == crate::sidebar_view::PinnedSessionOrder::Recent;
        sort.push(MenuItem::separator());
        sort.push(MenuItem::heading("Pinned order"));
        sort.push(
            intent(
                "Manual (drag to rearrange)",
                "pin",
                PINNED_ORDER_MANUAL_ACTION,
            )
            .with_checked(!recent),
        );
        sort.push(
            intent("Most recently updated", "clock", PINNED_ORDER_RECENT_ACTION)
                .with_checked(recent),
        );
    };
    if unified {
        sort.push(
            intent(
                "Group by Project",
                "folder",
                UNIFIED_GROUP_BY_PROJECT_ACTION,
            )
            .with_checked(input.settings.unified_group_by_project),
        );
        sort.push(
            intent("Moving Section", "stack", UNIFIED_WORKING_SECTION_ACTION)
                .with_checked(input.settings.unified_working_section),
        );
        pinned_order(&mut sort);
    } else {
        // The Settings > Sidebar switch writes the same setting (CDXC:Sidebar 2026-10-05 in
        // sidebar_view/ordering.rs `is_grouped_working`).
        sort.push(
            intent(
                "Group Working Sessions",
                "stack",
                GROUP_WORKING_SESSIONS_ACTION,
            )
            .with_checked(input.settings.group_working_sessions),
        );
        sort.push(
            intent(
                "Hide Projects Without Sessions",
                "folder",
                HIDE_PROJECTS_WITHOUT_SESSIONS_ACTION,
            )
            .with_checked(input.settings.hide_projects_without_sessions),
        );
        pinned_order(&mut sort);
        sort.push(MenuItem::separator());
        // The Cloud filter of Q1015 (CDXC:CloudBoxes in cloud_boxes.rs): offered on this
        // computer's tab while a box session is listed, or while it is ticked.
        let cloud_ticked = ui
            .selected_tag_filters
            .iter()
            .any(|tag| tag == crate::cloud_boxes::CLOUD_BOX_FILTER);
        if is_local && (input.cloud_box_rows || cloud_ticked) {
            sort.push(
                MenuItem::row(
                    "Cloud Box Sessions",
                    "cloud",
                    MenuCommand::toggle_tag_filter(crate::cloud_boxes::CLOUD_BOX_FILTER),
                )
                .with_checked(cloud_ticked),
            );
            sort.push(MenuItem::separator());
        }
        // CDXC:Sidebar 2026-10-31 DECISION: User (Q853): a heading says what the tag rows do. The reporter ticked one expecting it to tag something.
        let tag_rows_start = sort.len();
        for item in
            normalize_tag_list_items(&input.settings.tag_list_items, Some(input.filter_catalog))
        {
            if !item.visible {
                continue;
            }
            if item.kind == TagListItemKind::Separator {
                if item.enabled {
                    sort.push(MenuItem::separator());
                }
                continue;
            }
            let Some(tag) = tag_list_item_filter(&item).map(str::to_string) else {
                continue;
            };
            sort.push(
                MenuItem {
                    label: Some(tag_list_item_label(&item, input.catalog)),
                    command: Some(MenuCommand::toggle_tag_filter(&tag)),
                    ..MenuItem::default()
                }
                .with_tag_presentation(tag_presentation(Some(&tag), input.catalog).as_ref())
                .with_checked(ui.selected_tag_filters.contains(&tag))
                .with_disabled(!item.enabled),
            );
        }
        if sort[tag_rows_start..].iter().any(|item| !item.separator) {
            sort.insert(
                tag_rows_start,
                MenuItem::heading("Show only sessions tagged"),
            );
        }
    }

    /*
    CDXC:Sidebar 2026-09-21 DECISION:
    User: "Remove the manual sorting and last active sorting. I don't care about them right now."
    The two rows did nothing on desktop (the runtime had no handler for the sort mode). Without
    them the page can start or end on a separator, or be empty on a remote tab with no tag rows,
    so those separators are trimmed and an empty page is not drawn.
    The frozen TypeScript twin of this page (`tooling/gx-core/sidebar-page-frozen/navigation.ts`) is deleted; this is the only builder.
    */
    while sort.first().is_some_and(|item| item.separator) {
        sort.remove(0);
    }
    while sort.last().is_some_and(|item| item.separator) {
        sort.pop();
    }
    let mut more: Vec<MenuItem> = Vec::new();
    if is_local || input.host.machine_connected {
        more.push(intent("Add Project", "plus", "addProject"));
    }
    if !sort.is_empty() {
        more.push(
            MenuItem::submenu(
                "Sort & Filter",
                "filter",
                sort.into_iter()
                    .enumerate()
                    .map(|(index, item)| {
                        // The first row is the view switch, which closes the menu.
                        let stays_open = index > 0 && item.command.is_some();
                        MenuItem {
                            keep_open: stays_open,
                            ..item
                        }
                    })
                    .collect(),
            )
            .with_page(),
        );
    }
    if unified {
        // CDXC:Sidebar 2026-10-31 DECISION: User (issue #359): Collapse All mustn't act on a hidden list while Unified is on. It folds the project groups you can see, so it is only offered while Group by Project is on, and the flat list has nothing to fold. The host swaps the label for Expand All at the moment the menu opens, when every group is folded, because folding does not rebuild this menu.
        if input.settings.unified_group_by_project {
            more.push(intent(
                "Collapse All",
                "arrows-diagonal",
                UNIFIED_TOGGLE_GROUPS_ACTION,
            ));
        }
    } else if !input.drawn_project_group_ids.is_empty() {
        let any_expanded = input
            .drawn_project_group_ids
            .iter()
            .any(|group_id| !ui.collapse.collapsed_groups.contains(group_id));
        more.push(intent(
            if any_expanded {
                "Collapse All"
            } else {
                "Expand Previous"
            },
            "arrows-diagonal",
            "toggleProjects",
        ));
    }
    if !is_local {
        more.push(intent("Edit Machine", "pencil", "editMachine"));
    }
    more.push(MenuItem::separator());
    more.push(intent("Sessions", "history", "sessions"));
    more.push(intent("Import Sessions", "download", "importSessions"));
    more.push(MenuItem::row(
        "Search by Prompt",
        "file-search",
        MenuCommand::command(message::search_previous_sessions_by_text()),
    ));
    more.push(MenuItem::separator());
    more.push(intent("Agents Hub", "users-group", "agentsHub"));
    more.push(MenuItem::row(
        "All Automations",
        "clock",
        MenuCommand::command(message::open_automations_page()),
    ));
    more.push(MenuItem::separator());
    more.push(intent("Mobile & Remote", "devices", "remoteSetup"));
    if input.settings.show_beta_features && !input.settings.hide_keep_awake_titlebar_control {
        let active = input.host.keep_awake_minutes;
        let mut children: Vec<MenuItem> = KEEP_AWAKE_OPTIONS
            .iter()
            .map(|(label, minutes)| {
                MenuItem::row(
                    label,
                    "coffee",
                    MenuCommand::command(message::start_keep_awake(*minutes)),
                )
                .with_checked(active == Some(*minutes))
            })
            .collect();
        if active.is_some() {
            children.push(MenuItem::row(
                "Don't Keep Awake",
                "square-minus",
                MenuCommand::command(message::stop_keep_awake()),
            ));
        }
        children.push(MenuItem::separator());
        children.push(intent("Power Settings", "settings", "powerSettings"));
        more.push(MenuItem::submenu(
            "Keep Awake",
            if active.is_some() { "coffee" } else { "moon" },
            children,
        ));
    }
    /*
    CDXC:Sidebar 2026-09-30 DECISION:
    User: "add the buttons that we have only in mac os menu bar under Ghostex to the dropdown in the sidebar (for example check for updates, quit ghostex, quit bg services, restart". The menu ends with those app rows, below Join Discord, wherever the host is the desktop app; Windows and Linux have no menu bar to find them in. This supersedes the 2026-09-21 rule that the menu ends at Join Discord. The rest of that rule stands: the menu has no Settings or Hotkeys entries, because Settings is the gear beside the Commands row and Hotkeys is a page inside Settings. About and the Hide rows stay in the macOS menu bar only.
    The frozen TypeScript twin of this menu (`tooling/gx-core/sidebar-page-frozen/navigation.ts`) is deleted; this is the only builder.
    SEE-ALSO: packages/gx-core/src/app_lifecycle.rs (the rows, shared with Quick Access's Commands tab).
    */
    more.push(MenuItem::row(
        "Join Discord",
        "users-group",
        MenuCommand::command(message::open_external_url(DISCORD_URL)),
    ));
    /*
    CDXC:Feedback 2026-10-16 DECISION:
    User: "please move the feedback button to be inside the more options dropdown in the sidebar under join discord" (2026-10-14) and "please also rename the option to Send Bug / Feedback" (2026-10-16). Send Bug / Feedback is the row directly under Join Discord, with the chat-bubble icon, and opens the Send Bug / Feedback pop-up (apps/desktop/src/app/window/feedback_modal/). This supersedes the 2026-10-11 rule that put a chat-bubble button in the sidebar's bottom bar right of the workspace switcher.
    */
    more.push(intent("Send Bug / Feedback", "message-circle", "feedback"));
    if input.host.app_lifecycle {
        more.push(MenuItem::separator());
        for action in APP_LIFECYCLE_ACTIONS {
            more.push(MenuItem::row(
                action.title,
                action.icon,
                MenuCommand::command(message::app_lifecycle(action.id)),
            ));
        }
    }
    more
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar_view::tags::TagCatalog;

    fn page(settings: &SidebarSettings, host: &MenuHost) -> (Vec<String>, Vec<String>) {
        let ui = SidebarUiState::default();
        let catalog = TagCatalog::default();
        let drawn = vec!["project:a".to_string()];
        let more = more_menu(&MoreMenuInput {
            ui: &ui,
            settings,
            catalog: &catalog,
            filter_catalog: &catalog,
            host,
            drawn_project_group_ids: &drawn,
            cloud_box_rows: false,
        });
        let top = more.iter().filter_map(|item| item.label.clone()).collect();
        let sort = more
            .iter()
            .find(|item| item.label.as_deref() == Some("Sort & Filter"))
            .and_then(|item| item.children.clone())
            .unwrap_or_default()
            .iter()
            .filter_map(|item| item.label.clone())
            .collect();
        (top, sort)
    }

    #[test]
    fn the_project_sidebars_page_has_the_hide_switch_and_a_heading_over_its_tags() {
        let (top, sort) = page(&SidebarSettings::default(), &MenuHost::default());
        assert!(top.iter().any(|label| label == "Collapse All"));
        assert_eq!(sort[0], "Switch to Unified View");
        assert_eq!(sort[1], "Show Hidden");
        assert_eq!(sort[2], "Group Working Sessions");
        assert_eq!(sort[3], "Hide Projects Without Sessions");
        // Pinned order, one setting with Settings > Advanced (CDXC:Sidebar 2026-11-07 above).
        assert_eq!(sort[4], "Pinned order");
        assert_eq!(sort[5], "Manual (drag to rearrange)");
        assert_eq!(sort[6], "Most recently updated");
        assert_eq!(sort[7], "Show only sessions tagged");
        assert!(sort.len() > 8, "the tag rows follow the heading: {sort:?}");
    }

    #[test]
    fn the_unified_page_only_offers_what_changes_its_list() {
        let settings = SidebarSettings {
            unified_sidebar: true,
            ..SidebarSettings::default()
        };
        let (top, sort) = page(&settings, &MenuHost::default());
        // No tags and no Group Working Sessions, whose list is hidden here.
        assert_eq!(
            sort,
            [
                "Switch to Project View",
                "Show Hidden",
                "Group by Project",
                "Moving Section",
                "Pinned order",
                "Manual (drag to rearrange)",
                "Most recently updated"
            ]
        );
        // Flat: nothing to fold, though the hidden Project list has a group.
        assert!(!top.iter().any(|label| label == "Collapse All"));
        assert!(!top.iter().any(|label| label == "Expand Previous"));
    }

    #[test]
    fn only_the_view_switch_closes_the_page() {
        let settings = SidebarSettings::default();
        let ui = SidebarUiState::default();
        let catalog = TagCatalog::default();
        let more = more_menu(&MoreMenuInput {
            ui: &ui,
            settings: &settings,
            catalog: &catalog,
            filter_catalog: &catalog,
            host: &MenuHost::default(),
            drawn_project_group_ids: &[],
            cloud_box_rows: false,
        });
        let rows = more
            .iter()
            .find(|item| item.label.as_deref() == Some("Sort & Filter"))
            .and_then(|item| item.children.clone())
            .unwrap();
        assert!(!rows[0].keep_open);
        assert!(rows
            .iter()
            .skip(1)
            .filter(|row| row.command.is_some())
            .all(|row| row.keep_open));
    }

    #[test]
    fn collapse_all_folds_the_unified_groups_while_they_are_on() {
        let settings = SidebarSettings {
            unified_sidebar: true,
            unified_group_by_project: true,
            ..SidebarSettings::default()
        };
        let (top, _) = page(&settings, &MenuHost::default());
        assert!(top.iter().any(|label| label == "Collapse All"));
    }
}
