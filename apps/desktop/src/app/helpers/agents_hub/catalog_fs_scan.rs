// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the Agents Hub file-content/save/open
// message helpers plus the catalog filesystem scan: allowed roots/files,
// profile discovery, directory walking, and path/language/id helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_agents_hub_file_content_message(
    file_path: String,
    request_id: String,
) -> serde_json::Value {
    let normalized_file_path = file_path.trim().to_string();
    if normalized_file_path.is_empty() {
        return gpui_agents_hub_file_content_error(
            file_path,
            request_id,
            "Choose an Agents Hub file first.",
        );
    }
    let resolved = match gpui_agents_hub_validate_catalog_file_path(&normalized_file_path) {
        Ok(path) => path,
        Err(message) => {
            return gpui_agents_hub_file_content_error(normalized_file_path, request_id, &message);
        }
    };
    match fs::read_to_string(&resolved) {
        Ok(content) => serde_json::json!({
            "content": content,
            "filePath": gpui_path_string(&resolved),
            "requestId": request_id,
            "type": "agentsHubFileContent",
        }),
        Err(_) => gpui_agents_hub_file_content_error(
            gpui_path_string(&resolved),
            request_id,
            "Unable to load file contents.",
        ),
    }
}

pub(crate) fn gpui_agents_hub_file_content_error(
    file_path: String,
    request_id: String,
    message: &str,
) -> serde_json::Value {
    serde_json::json!({
        "errorMessage": message,
        "filePath": file_path,
        "requestId": request_id,
        "type": "agentsHubFileContent",
    })
}

pub(crate) fn gpui_save_agents_hub_file(
    file_path: String,
    content: String,
) -> Result<serde_json::Value, String> {
    let resolved = gpui_agents_hub_validate_catalog_file_path(&file_path)?;
    let Some(directory) = resolved.parent() else {
        return Err("Unable to save the selected Agents Hub file.".to_string());
    };
    let temp_path = directory.join(format!(
        ".ghostex-agents-hub-save.{}.tmp",
        system_time_epoch_millis_string(SystemTime::now())
    ));
    if fs::write(&temp_path, content.as_bytes()).is_err() {
        let _ = fs::remove_file(&temp_path);
        return Err("Unable to save the selected Agents Hub file.".to_string());
    }
    if fs::rename(&temp_path, &resolved).is_err() {
        let _ = fs::remove_file(&temp_path);
        return Err("Unable to save the selected Agents Hub file.".to_string());
    }
    Ok(gpui_agents_hub_catalog_message())
}

pub(crate) fn gpui_agents_hub_open_path_in_finder(path: String) -> Result<(), String> {
    /*
    CDXC:GPUIAgentsHubBridge 2026-06-24-12:26:
    Agents Hub opener commands accept only paths proven by the current catalog builder: catalog files, their containing directories, group roots, and profile roots. React-supplied arbitrary paths, URLs, shell commands, query strings, environment values, and file contents are never forwarded to the OS opener.
    */
    let normalized = path.trim();
    if normalized.is_empty() {
        return Err("Choose an Agents Hub path first.".to_string());
    }
    let resolved = fs::canonicalize(normalized)
        .map_err(|_| "The selected Agents Hub path is unavailable.".to_string())?;
    let metadata = fs::metadata(&resolved)
        .map_err(|_| "The selected Agents Hub path is unavailable.".to_string())?;
    if metadata.is_file() {
        let file = gpui_agents_hub_validate_catalog_file_path(normalized)?;
        let parent = file
            .parent()
            .ok_or_else(|| "The selected Agents Hub file has no containing folder.".to_string())?;
        return gpui_open_path(parent);
    }
    let catalog = gpui_agents_hub_catalog_build();
    if metadata.is_dir() && catalog.open_paths.contains(&resolved) {
        gpui_open_path(&resolved)
    } else {
        Err("The selected Agents Hub path is not available from the current catalog.".to_string())
    }
}

pub(crate) fn gpui_agents_hub_source_open_target(file_path: String) -> Result<PendingSourceFileOpen, String> {
    /*
    The app-modal renderer supplies only a catalog file candidate. Resolve it
    again against the current Rust-owned catalog and root Source at the file's
    containing directory; no external command or saved editor preference is
    involved.
    */
    let resolved = gpui_agents_hub_validate_catalog_file_path(&file_path)?;
    let Some(project_path) = resolved.parent().map(Path::to_path_buf) else {
        return Err("The selected Agents Hub file has no containing folder.".to_string());
    };
    Ok(PendingSourceFileOpen {
        file_path: resolved,
        project_path,
    })
}

pub(crate) fn gpui_agents_hub_validate_catalog_file_path(file_path: &str) -> Result<PathBuf, String> {
    let normalized = file_path.trim();
    if normalized.is_empty() {
        return Err("Choose an Agents Hub file first.".to_string());
    }
    let resolved = fs::canonicalize(normalized)
        .map_err(|_| "The selected Agents Hub file is unavailable.".to_string())?;
    let catalog = gpui_agents_hub_catalog_build();
    if catalog.file_paths.contains(&resolved) {
        Ok(resolved)
    } else {
        Err("The selected Agents Hub file is not available from the current catalog.".to_string())
    }
}

pub(crate) fn gpui_agents_hub_allowed_roots(home: &Path) -> Vec<PathBuf> {
    [
        home.join(".agents"),
        home.join("agents"),
        home.join(".claude"),
        home.join(".claude-profiles"),
        home.join(".codebuddy"),
        home.join(".config").join("agents"),
        home.join(".codex"),
        home.join(".codex-profiles"),
        home.join(".copilot"),
        home.join(".cursor"),
        home.join(".factory"),
        home.join(".gemini"),
        home.join(".grok").join("hooks"),
        home.join(".hermes"),
        home.join(".kiro"),
        home.join(".config").join("opencode"),
        home.join(".pi").join("agent"),
        home.join(".qoder"),
        home.join(".rovodev"),
    ]
    .into_iter()
    .filter_map(|path| fs::canonicalize(path).ok())
    .collect()
}

pub(crate) fn gpui_agents_hub_allowed_files(home: &Path) -> Vec<PathBuf> {
    [home.join(".claude.json")]
        .into_iter()
        .filter_map(|path| fs::canonicalize(path).ok())
        .collect()
}

pub(crate) fn gpui_agents_hub_profile(
    agent_icon: &'static str,
    label: impl Into<String>,
    profile_path: PathBuf,
    file_path: PathBuf,
    target_path: Option<PathBuf>,
) -> GpuiAgentsHubProfileItem {
    GpuiAgentsHubProfileItem {
        agent_icon,
        file_path,
        label: label.into(),
        profile_path,
        target_path,
    }
}

pub(crate) fn gpui_agents_hub_profile_json(profile: GpuiAgentsHubProfileItem) -> serde_json::Value {
    let mut value = serde_json::json!({
        "agentIcon": profile.agent_icon,
        "filePath": gpui_path_string(&profile.file_path),
        "label": profile.label,
        "profilePath": gpui_path_string(&profile.profile_path),
    });
    if let Some(target_path) = profile.target_path {
        value["targetPath"] = serde_json::Value::String(gpui_path_string(&target_path));
    }
    value
}

pub(crate) fn gpui_agents_hub_profiles_for(
    candidate_path: &Path,
    home: &Path,
    profiles: &[GpuiAgentsHubProfileItem],
    linked_profiles: &[GpuiAgentsHubProfileItem],
    open_code: &GpuiAgentsHubProfileItem,
    pi_agent: &GpuiAgentsHubProfileItem,
) -> Vec<GpuiAgentsHubProfileItem> {
    let shared_agents_root = home.join(".agents");
    let shared_skills_root = home.join("agents");
    if gpui_path_is_relative_to(candidate_path, &shared_agents_root)
        || gpui_path_is_relative_to(candidate_path, &shared_skills_root)
    {
        let mut result = linked_profiles.to_vec();
        if gpui_path_is_relative_to(candidate_path, &home.join("agents").join("hooks")) {
            result.push(pi_agent.clone());
        }
        return result;
    }
    for profile in profiles {
        if gpui_path_is_relative_to(candidate_path, &profile.profile_path) {
            return vec![profile.clone()];
        }
    }
    if gpui_path_is_relative_to(candidate_path, &open_code.profile_path) {
        return vec![open_code.clone()];
    }
    if gpui_path_is_relative_to(candidate_path, &pi_agent.profile_path) {
        return vec![pi_agent.clone()];
    }
    Vec::new()
}

pub(crate) fn gpui_agents_hub_existing_files(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter(|path| gpui_is_file(path))
        .collect()
}

pub(crate) fn gpui_agents_hub_list_directories(root: &Path) -> Vec<PathBuf> {
    let mut directories = fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            fs::metadata(path)
                .map(|metadata| metadata.is_dir())
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    directories.sort();
    directories
}

pub(crate) fn gpui_agents_hub_walk_files<F>(root: &Path, max_depth: usize, predicate: F) -> Vec<PathBuf>
where
    F: Fn(&Path) -> bool,
{
    let mut files = Vec::new();
    gpui_agents_hub_walk_files_inner(root, 0, max_depth, &predicate, &mut files);
    files.sort();
    files
}

pub(crate) fn gpui_agents_hub_walk_files_inner<F>(
    current: &Path,
    depth: usize,
    max_depth: usize,
    predicate: &F,
    files: &mut Vec<PathBuf>,
) where
    F: Fn(&Path) -> bool,
{
    let mut entries = fs::read_dir(current)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in &entries {
        let path = entry.path();
        if fs::metadata(&path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
            && predicate(&path)
        {
            files.push(path);
        }
    }
    if depth >= max_depth {
        return;
    }
    for entry in entries {
        let path = entry.path();
        let name = gpui_file_name_string(&path);
        if gpui_agents_hub_ignored_directory_name(&name) {
            continue;
        }
        if fs::metadata(&path)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            gpui_agents_hub_walk_files_inner(&path, depth + 1, max_depth, predicate, files);
        }
    }
}

pub(crate) fn gpui_agents_hub_ignored_directory_name(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "dist"
            | "build"
            | "out"
            | "coverage"
            | ".cache"
            | "cache"
            | "__pycache__"
            | "sessions"
            | "projects"
            | "todos"
            | "telemetry"
            | "usage-data"
            | "ambient-suggestions"
            | "memories_2026-04-24"
            | "logs"
            | "tmp"
            | ".tmp"
    )
}

pub(crate) fn gpui_agents_hub_is_skill_file(candidate: &Path) -> bool {
    gpui_file_name_string(candidate) == "SKILL.md"
        || matches!(
            gpui_agents_hub_extension(candidate).as_str(),
            ".json" | ".yaml" | ".yml" | ".sh" | ".py" | ".js" | ".ts"
        )
}

pub(crate) fn gpui_agents_hub_is_text_file(candidate: &Path) -> bool {
    matches!(
        gpui_agents_hub_extension(candidate).as_str(),
        ".md"
            | ".json"
            | ".jsonl"
            | ".toml"
            | ".yaml"
            | ".yml"
            | ".sh"
            | ".ts"
            | ".js"
            | ".mjs"
            | ".py"
            | ".txt"
    )
}

pub(crate) fn gpui_agents_hub_plugin_root(plugin_file: &Path) -> Option<PathBuf> {
    let components = plugin_file.components().collect::<Vec<_>>();
    let file_name = gpui_file_name_string(plugin_file);
    if file_name == "SKILL.md" {
        for (index, component) in components.iter().enumerate() {
            if component.as_os_str() == std::ffi::OsStr::new("skills")
                && index + 1 < components.len()
            {
                return Some(gpui_path_from_components(&components[..=index + 1]));
            }
        }
    }
    for (index, component) in components.iter().enumerate() {
        if component.as_os_str() == std::ffi::OsStr::new(".codex-plugin")
            || component.as_os_str() == std::ffi::OsStr::new(".claude-plugin")
        {
            return Some(gpui_path_from_components(&components[..index]));
        }
    }
    None
}

pub(crate) fn gpui_path_from_components(components: &[std::path::Component<'_>]) -> PathBuf {
    let mut path = PathBuf::new();
    for component in components {
        path.push(component.as_os_str());
    }
    path
}

pub(crate) fn gpui_agents_hub_language_for(file_path: &Path) -> &'static str {
    let name = gpui_file_name_string(file_path);
    let extension = gpui_agents_hub_extension(file_path);
    if name.ends_with(".yaml") || name.ends_with(".yml") {
        "yaml"
    } else {
        match extension.as_str() {
            ".json" | ".jsonl" => "json",
            ".toml" => "toml",
            ".sh" => "shell",
            ".py" => "python",
            ".ts" | ".tsx" => "typescript",
            ".js" | ".mjs" | ".cjs" => "javascript",
            ".md" => "markdown",
            _ => "plaintext",
        }
    }
}

pub(crate) fn gpui_agents_hub_file_id(path: &Path) -> String {
    let mut id = String::new();
    let mut previous_dash = false;
    for character in gpui_path_string(path).to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character);
            previous_dash = false;
        } else if !previous_dash && !id.is_empty() {
            id.push('-');
            previous_dash = true;
        }
        if id.len() >= 180 {
            break;
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    id
}

pub(crate) fn gpui_agents_hub_extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_lowercase()))
        .unwrap_or_default()
}

pub(crate) fn gpui_file_name_string(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn gpui_path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn gpui_relative_path_name(path: &Path, root: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(gpui_path_string)
        .filter(|value| !value.is_empty())
}

pub(crate) fn gpui_path_is_relative_to(candidate: &Path, root: &Path) -> bool {
    candidate == root || candidate.strip_prefix(root).is_ok()
}

