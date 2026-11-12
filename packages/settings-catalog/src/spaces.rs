//! Whether Spaces is on: one switch, `sidebarSpacesEnabled`, in Settings > Sidebar & Sessions >
//! Spaces. Off hides everything about Spaces (the sidebar's Space row, Space menus and drop targets,
//! the Space rows of Settings, the Go to Space hotkeys, Quick Access rows, the Space verbs of
//! `ghostex`) and views behave as if there were one Space; saved Spaces are kept. The desktop, the
//! GPUI web build, gx-core and gxserver all ask here.
//!
//! CDXC:Spaces 2026-11-10 DECISION:
//! User: "move spaces to be one of the options and not an extension. put it in the right place in sidebar in settings". Spaces leaves Settings > Extensions (supersedes the Spaces part of CDXC:Extensions 2026-10-01) and its switch becomes the first row of its own Spaces section on Sidebar & Sessions, above the two Space rows it shows. The switch keeps its key `sidebarSpacesEnabled` and its default (off), so everyone keeps what they had and older apps on the other side of sync or the phone read the same key. Nothing read `spaces` in `builtInFeatures` (the phone and the web build read `sidebarSpacesEnabled` from each machine's presentation), so dropping it from that list changes nothing for them.
//! SEE-ALSO: packages/settings-catalog/src/general/sidebar.rs (`spaces`), apps/desktop/src/app/window/settings_modal/tabs/general/sidebar.rs (`spaces_section`), server/src/sidebar_spaces.rs (`read_sidebar_spaces_enabled`).

use crate::json::J;
use serde_json::{Map, Value};

/// The Spaces switch; `true` means on.
pub const SWITCH_KEY: &str = "sidebarSpacesEnabled";

/// The Active projects switch (`true` means on, the default): the star tile first in the Spaces
/// row, which lists the projects being worked on from every Space.
///
/// CDXC:Spaces 2026-11-12 DECISION:
/// User: "i wanna add a feature called 'active projects' and it should just show the projects that i'm actively working on from all spaces in 1 place so i can jump between them easily without switching spaces ... should be feature can be disabled but enabled by default (can right click on Active Space button in the sidebar to remove it). should be shown on the left of all spaces and have star icon". Its switch is the first row under the Spaces switch in Settings > Sidebar & Sessions > Spaces, shown only while Spaces is on, and the star's right-click menu turns it off.
pub const ACTIVE_PROJECTS_KEY: &str = "sidebarActiveProjects";

/// The rows shown only while Spaces is on, which a Settings search leaves out while it is off.
pub const SETTING_KEYS: &[&str] = &[
    ACTIVE_PROJECTS_KEY,
    "sidebarSpaceSwitchBehavior",
    "sidebarSpaceFollowActiveSession",
];

/// The hotkeys that are listed and fire only while Spaces is on.
pub const HOTKEYS: &[&str] = &[
    "goToSpace1",
    "goToSpace2",
    "goToSpace3",
    "goToSpace4",
    "goToSpace5",
    "goToSpace6",
    "goToSpace7",
    "goToSpace8",
    "goToSpace9",
];

/// Whether Spaces is on, reading saved values through `read` and falling back to the default.
pub fn enabled_with(read: impl Fn(&str) -> Option<bool>) -> bool {
    read(SWITCH_KEY).unwrap_or_else(|| crate::default_value(SWITCH_KEY) == Some(&J::Bool(true)))
}

/// [`enabled_with`] over a settings object (the settings file or the desktop's snapshot).
pub fn enabled(settings: &Map<String, Value>) -> bool {
    enabled_with(|key| settings.get(key).and_then(Value::as_bool))
}

/// [`enabled`] over any settings value; a missing or non-object value reads the default.
pub fn enabled_in_value(settings: Option<&Value>) -> bool {
    enabled_with(|key| {
        settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_bool)
    })
}

/// What a surface says when something needs Spaces while it is off.
pub fn turned_off_message() -> String {
    format!(
        "Spaces is turned off. Turn it on in Settings > Sidebar & Sessions > Spaces.{}",
        crate::built_in_extensions::terminal_hint(SWITCH_KEY, true)
    )
}
