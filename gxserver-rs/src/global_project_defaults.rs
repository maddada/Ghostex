/*
CDXC:GlobalProjectDefaults 2026-08-02:
Global Defaults back the three per-project fields on the Projects settings page.
A project keeps winning whenever its own stored value is non-empty; only an empty
project value consults the global. Every global therefore defaults to empty, which
reproduces the exact pre-feature resolution for every project.

The read and the decision are split the same way `session_lifecycle` splits its
settings gate: `read_global_project_defaults` owns the one filesystem read, and
`resolve_with_global_default` is a pure function so the cascade is testable
without a settings file on disk.
*/

use serde_json::Value;

/// Global Defaults as stored in the shared sidebar settings file. Empty strings
/// mean "not configured", never "configured as blank".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlobalProjectDefaults {
    pub beads_directory: String,
    pub beads_display_key: String,
    pub worktree_command: String,
}

impl GlobalProjectDefaults {
    fn from_settings(settings: Option<&Value>) -> Self {
        let read = |key: &str| -> String {
            settings
                .and_then(|settings| settings.get(key))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        Self {
            beads_directory: read("globalBeadsDirectory"),
            beads_display_key: read("globalBeadsDisplayKey"),
            worktree_command: read("globalWorktreeCommand"),
        }
    }
}

/// Resolve one project field against its Global Default. Returns `None` when
/// neither side is configured, which every caller must keep treating exactly as
/// it treated an unset project field before this feature existed.
pub fn resolve_with_global_default(
    project_value: Option<&str>,
    global_value: &str,
) -> Option<String> {
    let project_value = project_value.unwrap_or_default().trim();
    if !project_value.is_empty() {
        return Some(project_value.to_string());
    }
    let global_value = global_value.trim();
    if global_value.is_empty() {
        return None;
    }
    Some(global_value.to_string())
}

/// The single read of Global Defaults out of the shared sidebar settings file.
/// A missing or unparseable file yields all-empty defaults, so a broken settings
/// file can never silently redirect a project at some other global value.
pub fn read_global_project_defaults() -> GlobalProjectDefaults {
    let paths = crate::paths::get_gxserver_paths(None);
    let settings_path = paths
        .home_dir
        .join(".ghostex")
        .join("state")
        .join("native-sidebar-settings.json");
    let settings = std::fs::read_to_string(settings_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    GlobalProjectDefaults::from_settings(settings.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn project_value_always_wins_over_the_global_default() {
        assert_eq!(
            resolve_with_global_default(Some("/project/board"), "/global/board"),
            Some("/project/board".to_string())
        );
    }

    #[test]
    fn empty_project_value_falls_back_to_the_global_default() {
        assert_eq!(
            resolve_with_global_default(Some("   "), "/global/board"),
            Some("/global/board".to_string())
        );
        assert_eq!(
            resolve_with_global_default(None, "/global/board"),
            Some("/global/board".to_string())
        );
    }

    #[test]
    fn unset_global_default_preserves_the_pre_feature_none() {
        assert_eq!(resolve_with_global_default(None, ""), None);
        assert_eq!(resolve_with_global_default(Some(""), "   "), None);
    }

    #[test]
    fn missing_settings_file_reads_as_nothing_configured() {
        assert_eq!(
            GlobalProjectDefaults::from_settings(None),
            GlobalProjectDefaults::default()
        );
    }

    #[test]
    fn settings_values_are_trimmed_and_read_by_key() {
        let settings = json!({
            "globalBeadsDirectory": " /global/board ",
            "globalBeadsDisplayKey": "AGE",
            "globalWorktreeCommand": "bun install",
        });
        let defaults = GlobalProjectDefaults::from_settings(Some(&settings));
        assert_eq!(defaults.beads_directory, "/global/board");
        assert_eq!(defaults.beads_display_key, "AGE");
        assert_eq!(defaults.worktree_command, "bun install");
    }

    #[test]
    fn non_string_settings_values_read_as_nothing_configured() {
        let settings = json!({ "globalBeadsDirectory": 12, "globalWorktreeCommand": null });
        let defaults = GlobalProjectDefaults::from_settings(Some(&settings));
        assert_eq!(defaults, GlobalProjectDefaults::default());
    }
}
