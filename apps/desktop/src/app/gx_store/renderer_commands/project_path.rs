//! A renderer command's project folder (`projectPath`, `path`), matched to the registered project
//! the way the filesystem matches folders before gx-core looks it up by exact path.
//!
//! CDXC:Projects 2026-10-31 WHY:
//! An agent's folder comes from `getcwd`, which on macOS reports the on-disk capitals, while a project keeps the capitals it was added with, so `ghostex browser open` (and `switch-project --path`, and the browser tools' project) answered "No matching project" for a project added as `~/Dev/App` when the folder is `~/dev/app`. gx-core stays free of file I/O and keeps its exact match; this host step swaps such a folder for the project's registered path when the two are one folder on disk (`ghostex_paths::same_folder`), which never happens on a case-sensitive volume.
//! SEE-ALSO: packages/paths/src/same_folder.rs, packages/gx-core/src/renderer_commands/targets.rs (`local_project_at_path`).

use ghostex_gx_core::{Core, MachineId};
use serde_json::Value;

/// The payload keys that name a local project by its folder.
const PROJECT_FOLDER_KEYS: [&str; 2] = ["projectPath", "path"];

/// `payload` with a project folder that names a registered project's folder in other letters
/// replaced by that project's registered path; unchanged otherwise (no such key, an exact match,
/// or no project there).
pub(super) fn with_registered_project_folder(core: &Core, payload: &Value) -> Value {
    let Some(machine) = core.presentation().machine(&MachineId::Local) else {
        return payload.clone();
    };
    let registered = machine
        .domain_projects()
        .filter_map(|(_, project)| project.get("path")?.as_str())
        .map(str::to_string)
        .collect::<Vec<_>>();
    with_registered_folder(payload, &registered)
}

fn with_registered_folder(payload: &Value, registered: &[String]) -> Value {
    let mut payload = payload.clone();
    for key in PROJECT_FOLDER_KEYS {
        let Some(folder) = payload.get(key).and_then(Value::as_str) else {
            continue;
        };
        let Some(path) = ghostex_paths::same_folder::find_same_folder(
            folder,
            registered.iter().map(|path| (path.clone(), path.clone())),
        ) else {
            continue;
        };
        payload[key] = Value::String(path);
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_folder_in_other_capitals_becomes_the_registered_path_where_the_volume_ignores_case() {
        let root =
            std::env::temp_dir().join(format!("ghostex-project-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Dev").join("App")).unwrap();
        let registered = root.join("Dev").join("App").to_string_lossy().to_string();
        let agent_folder = root.join("dev").join("app").to_string_lossy().to_string();
        let ignores_case = root.join("dev").exists();
        let payload = with_registered_folder(
            &json!({ "projectPath": agent_folder, "url": "https://example.com" }),
            std::slice::from_ref(&registered),
        );
        let expected = if ignores_case {
            &registered
        } else {
            &agent_folder
        };
        assert_eq!(payload["projectPath"], json!(expected));
        assert_eq!(payload["url"], json!("https://example.com"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unregistered_folder_is_left_alone() {
        let payload = with_registered_folder(
            &json!({ "path": "/no/such/folder" }),
            &["/no/such/Folder".to_string()],
        );
        assert_eq!(payload["path"], json!("/no/such/folder"));
    }
}
