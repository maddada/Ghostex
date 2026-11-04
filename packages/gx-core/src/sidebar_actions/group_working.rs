//! The switches of the Sort & Filter page that are settings: Group Working Sessions, Hide Projects
//! Without Sessions, the view switch and, while the Unified sidebar is on, Group by Project and
//! Working Section.
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

/// Each row: its action, the setting it flips and what that setting reads when it was never saved.
const SETTING_ROWS: [(&str, &str, bool); 4] = [
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
    setting_row(command).is_some() || is_switch_sidebar_view(command)
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
    let (action, key, default) = setting_row(command)?;
    let on = saved_settings
        .get(*key)
        .and_then(Value::as_bool)
        .unwrap_or(*default);
    Some(SidebarActionPlan::one(ActionEffect::UpdateSettingsPatch {
        message: json!({
            "type": "updateSettingsPatch",
            "source": format!("sidebar:{action}"),
            "patch": { *key: !on },
        }),
    }))
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
