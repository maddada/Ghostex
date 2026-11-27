//! The switches of the Sort & Filter page that are settings: Group Working Sessions, Hide Projects
//! Without Sessions, the view switch and, while the Unified sidebar is on, Group by Project, Group
//! sessions by tags and Working Section.
//!
//! CDXC:Sidebar 2026-10-05 WHY:
//! The rows write the setting, not the sidebar's own state like Show Hidden and the tag filters,
//! because Settings > Sidebar shows the same switch and the two must always agree. The save goes
//! through the same end function as Hide Machine (`machine_disable.rs`), so the new value reaches
//! the sidebar, the open Settings window and every other surface through the ordinary settings
//! fan-out instead of a second path.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/ordering.rs (`is_grouped_working`, the rule),
//! packages/settings-catalog/src/general/sidebar.rs (the Settings row),
//! apps/desktop/src/app/gx_store/sidebar_state_actions.rs (the host).

use serde_json::{json, Map, Value};

use super::plan::{ActionEffect, SidebarActionPlan};
use super::resolve::text_field;
use crate::sidebar_view::GROUP_WORKING_SESSIONS_SETTING_KEY;

/// The row that switches between the Project and Unified sidebars.
pub const SWITCH_SIDEBAR_VIEW_ACTION: &str = "switchSidebarView";

/// The `sidebarAction` the Group Working Sessions row carries.
pub const GROUP_WORKING_SESSIONS_ACTION: &str = "groupWorkingSessions";

/// `source` of that row's patch.
pub const GROUP_WORKING_SESSIONS_SETTINGS_SOURCE: &str = "sidebar:groupWorkingSessions";

/// `sidebarHideProjectsWithoutSessions`.
pub const HIDE_PROJECTS_WITHOUT_SESSIONS_ACTION: &str = "hideProjectsWithoutSessions";
/// `unifiedGroupByProject`.
pub const UNIFIED_GROUP_BY_PROJECT_ACTION: &str = "unifiedGroupByProject";
/// `unifiedWorkingSection`.
pub const UNIFIED_WORKING_SECTION_ACTION: &str = "unifiedWorkingSection";
/// `groupSessionsByTags`.
pub const GROUP_SESSIONS_BY_TAGS_ACTION: &str = "groupSessionsByTags";
/// `pinnedSessionOrder: manual` (CDXC:Sidebar 2026-11-07 in sidebar_menu/navigation.rs).
pub const PINNED_ORDER_MANUAL_ACTION: &str = "pinnedOrderManual";
/// `pinnedSessionOrder: recent`.
pub const PINNED_ORDER_RECENT_ACTION: &str = "pinnedOrderRecent";

/// `unifiedSessionOrder: recent` (CDXC:Sidebar 2026-11-17 in unified_sidebar/build.rs).
pub const SESSION_ORDER_RECENT_ACTION: &str = "unifiedSessionOrderRecent";
/// `unifiedSessionOrder: alphabetical`.
pub const SESSION_ORDER_ALPHABETICAL_ACTION: &str = "unifiedSessionOrderAlphabetical";
/// `unifiedSessionOrder: created`.
pub const SESSION_ORDER_CREATED_ACTION: &str = "unifiedSessionOrderCreated";

/// The Session order row's value, for one of its three rows.
fn session_order_value(command: &Value) -> Option<&'static str> {
    if text_field(command, "type") != Some("sidebarAction") {
        return None;
    }
    match text_field(command, "action")? {
        SESSION_ORDER_RECENT_ACTION => Some("recent"),
        SESSION_ORDER_ALPHABETICAL_ACTION => Some("alphabetical"),
        SESSION_ORDER_CREATED_ACTION => Some("created"),
        _ => None,
    }
}

/// The Pinned order row's value, for one of its two rows.
fn pinned_order_value(command: &Value) -> Option<&'static str> {
    if text_field(command, "type") != Some("sidebarAction") {
        return None;
    }
    match text_field(command, "action")? {
        PINNED_ORDER_MANUAL_ACTION => Some("manual"),
        PINNED_ORDER_RECENT_ACTION => Some("recent"),
        _ => None,
    }
}

/// Each row: its action, the setting it flips and what that setting reads when it was never saved.
const SETTING_ROWS: [(&str, &str, bool); 5] = [
    (
        GROUP_WORKING_SESSIONS_ACTION,
        GROUP_WORKING_SESSIONS_SETTING_KEY,
        false,
    ),
    (
        HIDE_PROJECTS_WITHOUT_SESSIONS_ACTION,
        "sidebarHideProjectsWithoutSessions",
        false,
    ),
    (
        UNIFIED_GROUP_BY_PROJECT_ACTION,
        "unifiedGroupByProject",
        false,
    ),
    (GROUP_SESSIONS_BY_TAGS_ACTION, "groupSessionsByTags", false),
    (
        UNIFIED_WORKING_SECTION_ACTION,
        "unifiedWorkingSection",
        true,
    ),
];

fn setting_row(command: &Value) -> Option<&'static (&'static str, &'static str, bool)> {
    if text_field(command, "type") != Some("sidebarAction") {
        return None;
    }
    let action = text_field(command, "action")?;
    SETTING_ROWS.iter().find(|row| row.0 == action)
}

fn is_switch_sidebar_view(command: &Value) -> bool {
    text_field(command, "type") == Some("sidebarAction")
        && text_field(command, "action") == Some(SWITCH_SIDEBAR_VIEW_ACTION)
}

/// Whether this renderer command is one of the rows.
pub fn owns_group_working_sessions_command(command: &Value) -> bool {
    setting_row(command).is_some()
        || is_switch_sidebar_view(command)
        || pinned_order_value(command).is_some()
        || session_order_value(command).is_some()
}

/// The patch that flips the row's setting, given the settings as saved.
pub fn plan_group_working_sessions(
    command: &Value,
    saved_settings: &Map<String, Value>,
) -> Option<SidebarActionPlan> {
    // The one row whose setting is a choice: `sidebarView`, which Settings > Sidebar picks too.
    if is_switch_sidebar_view(command) {
        let unified = saved_settings.get("sidebarView").and_then(Value::as_str) == Some("unified");
        return Some(SidebarActionPlan::one(ActionEffect::UpdateSettingsPatch {
            message: json!({
                "type": "updateSettingsPatch",
                "source": format!("sidebar:{SWITCH_SIDEBAR_VIEW_ACTION}"),
                "patch": { "sidebarView": if unified { "project" } else { "unified" } },
            }),
        }));
    }
    // Pinned order is a choice too, the one Settings > Sidebar & Sessions > Advanced picks.
    if let Some(value) = pinned_order_value(command) {
        return Some(SidebarActionPlan::one(ActionEffect::UpdateSettingsPatch {
            message: json!({
                "type": "updateSettingsPatch",
                "source": "sidebar:pinnedSessionOrder",
                "patch": { crate::sidebar_view::PINNED_SESSION_ORDER_SETTING_KEY: value },
            }),
        }));
    }
    // So is the Unified sidebar's Session order, the one Settings > Sidebar picks.
    if let Some(value) = session_order_value(command) {
        return Some(SidebarActionPlan::one(ActionEffect::UpdateSettingsPatch {
            message: json!({
                "type": "updateSettingsPatch",
                "source": "sidebar:unifiedSessionOrder",
                "patch": { crate::sidebar_view::UNIFIED_SESSION_ORDER_SETTING_KEY: value },
            }),
        }));
    }
    let (action, key, default) = setting_row(command)?;
    let on = saved_settings
        .get(*key)
        .and_then(Value::as_bool)
        .unwrap_or(*default);
    // The list goes in first, so it is saved no later than the switch that reads its places.
    let mut patch = Map::new();
    if *key == "groupSessionsByTags" {
        patch.extend(group_sessions_by_tags_list(!on, saved_settings));
    }
    patch.insert((*key).to_string(), Value::Bool(!on));
    Some(SidebarActionPlan::one(ActionEffect::UpdateSettingsPatch {
        message: json!({
            "type": "updateSettingsPatch",
            "source": format!("sidebar:{action}"),
            "patch": patch,
        }),
    }))
}

/// What turning Group sessions by tags on saves besides the switch, in the same patch and before
/// it: the Session Tags list with the status entries it lacks on top, when this computer saved an
/// arranged list that predates them. Nothing when turning it off, when the list already has them,
/// when it was never saved, or when it is the default list apart from the status entries (never
/// changed here): writing the default then would replace a list another computer arranged, so only
/// the switch is saved, as on the phone.
///
/// CDXC:Sidebar 2026-11-14 WHY: the status entries' places are read from the stored list, and sync keeps them through an older app's write only when the computer held them (sidebar_view/status_tags.rs `keep_status_entries`), so turning grouping on stores them first; a list nobody changed here is left to sync, which brings the arranged one.
pub fn group_sessions_by_tags_list(
    on: bool,
    saved_settings: &Map<String, Value>,
) -> Option<(String, Value)> {
    let list = saved_settings.get("sidebarSessionTagListItems")?;
    let default =
        ghostex_settings_catalog::default_value("sidebarSessionTagListItems").map(|default| {
            use ghostex_settings_catalog::ToJson as _;
            default.to_json().to_value()
        });
    if default.is_some_and(|default| {
        crate::sidebar_view::without_status_entries(&default)
            == crate::sidebar_view::without_status_entries(list)
    }) {
        return None;
    }
    let list = on.then(|| crate::sidebar_view::with_status_entries(list))??;
    Some(("sidebarSessionTagListItems".to_string(), list))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(action: &str, saved: Value) -> Value {
        let command = json!({"type": "sidebarAction", "action": action});
        let plan = plan_group_working_sessions(&command, saved.as_object().unwrap()).unwrap();
        match &plan.effects[0] {
            ActionEffect::UpdateSettingsPatch { message } => message["patch"].clone(),
            other => panic!("unexpected effect {other:?}"),
        }
    }

    #[test]
    fn each_row_flips_its_own_setting_from_its_own_default() {
        assert_eq!(
            patch(GROUP_WORKING_SESSIONS_ACTION, json!({})),
            json!({"groupWorkingSessions": true})
        );
        assert_eq!(
            patch(UNIFIED_GROUP_BY_PROJECT_ACTION, json!({})),
            json!({"unifiedGroupByProject": true})
        );
        assert_eq!(
            patch(HIDE_PROJECTS_WITHOUT_SESSIONS_ACTION, json!({})),
            json!({"sidebarHideProjectsWithoutSessions": true})
        );
        // The Working section is on until someone turns it off.
        assert_eq!(
            patch(UNIFIED_WORKING_SECTION_ACTION, json!({})),
            json!({"unifiedWorkingSection": false})
        );
        assert_eq!(
            patch(
                UNIFIED_WORKING_SECTION_ACTION,
                json!({"unifiedWorkingSection": false})
            ),
            json!({"unifiedWorkingSection": true})
        );
    }

    #[test]
    fn the_pinned_order_rows_pick_their_own_value_whatever_is_saved() {
        // One setting with Settings > Sidebar & Sessions > Advanced's Pinned order.
        for saved in [json!({}), json!({"pinnedSessionOrder": "recent"})] {
            assert_eq!(
                patch(PINNED_ORDER_MANUAL_ACTION, saved.clone()),
                json!({"pinnedSessionOrder": "manual"})
            );
        }
        for saved in [json!({}), json!({"pinnedSessionOrder": "recent"})] {
            assert_eq!(
                patch(PINNED_ORDER_RECENT_ACTION, saved),
                json!({"pinnedSessionOrder": "recent"})
            );
        }
    }

    #[test]
    fn the_view_switch_goes_to_the_other_view_from_either_one() {
        assert_eq!(
            patch(SWITCH_SIDEBAR_VIEW_ACTION, json!({})),
            json!({"sidebarView": "unified"})
        );
        assert_eq!(
            patch(
                SWITCH_SIDEBAR_VIEW_ACTION,
                json!({"sidebarView": "project"})
            ),
            json!({"sidebarView": "unified"})
        );
        assert_eq!(
            patch(
                SWITCH_SIDEBAR_VIEW_ACTION,
                json!({"sidebarView": "unified"})
            ),
            json!({"sidebarView": "project"})
        );
    }

    #[test]
    fn other_commands_are_not_owned() {
        let command = json!({"type": "sidebarAction", "action": "showHidden"});
        assert!(!owns_group_working_sessions_command(&command));
    }
}
