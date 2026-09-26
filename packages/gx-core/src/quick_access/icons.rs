//! Project icons as Quick Access draws them (packages/shared/workspace-project-appearance.ts).

use serde_json::Value;

use super::wire::QuickAccessIcon;
use crate::sidebar_view::text::js_trim;

/// `SIDEBAR_COMMAND_ICON_IDS`: the glyphs a project icon may name.
const SIDEBAR_COMMAND_ICON_IDS: &[&str] = &[
    "playerPlay",
    "api",
    "archive",
    "bell",
    "bolt",
    "book",
    "brain",
    "braces",
    "brandDocker",
    "brandGithub",
    "brandPython",
    "brandReact",
    "brandVscode",
    "bug",
    "chartBar",
    "cloud",
    "checklist",
    "clock",
    "code",
    "command",
    "cpu",
    "database",
    "deviceDesktop",
    "deviceLaptop",
    "download",
    "fileCode",
    "fileDiff",
    "fileSearch",
    "fileText",
    "flask",
    "folder",
    "folderOpen",
    "gitBranch",
    "gitCommit",
    "gitMerge",
    "gitPullRequest",
    "key",
    "layoutDashboard",
    "link",
    "lock",
    "messageCircle",
    "package",
    "pencilCode",
    "refresh",
    "robot",
    "route",
    "rocket",
    "search",
    "server",
    "settings",
    "shieldSearch",
    "sparkles",
    "stack",
    "terminal",
    "testPipe",
    "tool",
    "upload",
    "wand",
    "world",
];

/// `normalizeWorkspaceProjectIcon`.
pub(crate) enum ProjectIcon {
    Image(String),
    Tabler { icon: String, color: Option<String> },
}

pub(crate) fn normalize_project_icon(value: &Value) -> Option<ProjectIcon> {
    let object = value.as_object()?;
    match object.get("kind").and_then(Value::as_str) {
        Some("image") => normalize_icon_data_url(object.get("dataUrl")).map(ProjectIcon::Image),
        Some("tabler") => {
            let icon = object.get("icon").and_then(Value::as_str)?;
            if !SIDEBAR_COMMAND_ICON_IDS.contains(&icon) {
                return None;
            }
            Some(ProjectIcon::Tabler {
                icon: icon.to_string(),
                color: normalize_icon_color(object.get("color")),
            })
        }
        _ => None,
    }
}

/// `normalizeSidebarCommandIconColor`.
fn normalize_icon_color(value: Option<&Value>) -> Option<String> {
    let trimmed = js_trim(value?.as_str()?);
    let is_hex = trimmed.len() == 7
        && trimmed.starts_with('#')
        && trimmed[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    is_hex.then(|| trimmed.to_lowercase())
}

/// `normalizeWorkspaceProjectIconDataUrl`: a PNG or SVG base64 data URL, as Ghostex's own picker
/// writes one.
pub(crate) fn normalize_icon_data_url(value: Option<&Value>) -> Option<String> {
    let value = value?.as_str()?;
    (value.starts_with("data:image/png;base64,") || value.starts_with("data:image/svg+xml;base64,"))
        .then(|| value.to_string())
}

/// `normalizeDiscoveredProjectIconDataUrl`: any image type the discovery probe reads, as a whole
/// base64 data URL.
pub(crate) fn normalize_discovered_icon_data_url(value: Option<&Value>) -> Option<String> {
    let trimmed = js_trim(value?.as_str()?);
    let rest = trimmed.strip_prefix("data:image/")?;
    let (kind, payload) = rest.split_once(";base64,")?;
    if !matches!(
        kind,
        "png" | "svg+xml" | "x-icon" | "vnd.microsoft.icon" | "jpeg" | "webp" | "gif"
    ) {
        return None;
    }
    let body = payload.trim_end_matches('=');
    if body.is_empty()
        || !body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
    {
        return None;
    }
    Some(trimmed.to_string())
}

/// `resolveWorkspaceProjectIconDataUrl({ icon, iconDataUrl })`.
pub(crate) fn resolve_icon_data_url(icon: &Value, icon_data_url: Option<&Value>) -> Option<String> {
    if let Some(ProjectIcon::Image(url)) = normalize_project_icon(icon) {
        return Some(url);
    }
    normalize_icon_data_url(icon_data_url)
}

/// A recent project's glyph (`projectIcon` in projects.ts).
pub(crate) fn recent_project_icon(project: &Value) -> QuickAccessIcon {
    if let Some(image) = QuickAccessIcon::image(
        resolve_icon_data_url(&project["icon"], project.get("iconDataUrl")).as_deref(),
    ) {
        return image;
    }
    // `project.icon?.kind === 'tabler'` reads the raw icon, not the normalized one.
    if project["icon"]["kind"] == "tabler" {
        return QuickAccessIcon::asset(
            project["icon"]["icon"].as_str().unwrap_or(""),
            project["icon"]["color"].as_str(),
        );
    }
    QuickAccessIcon::asset("folder", None)
}

/// A saved prompt's project glyph (`StashedPromptProjectIcon`).
pub(crate) fn prompt_project_icon(prompt: &Value) -> QuickAccessIcon {
    let icon = normalize_project_icon(&prompt["projectIcon"]);
    let explicit = match &icon {
        Some(ProjectIcon::Image(url)) => Some(url.clone()),
        _ => normalize_icon_data_url(prompt.get("projectIconDataUrl")),
    };
    if let Some(image) = QuickAccessIcon::image(explicit.as_deref()) {
        return image;
    }
    if let Some(image) = QuickAccessIcon::image(
        normalize_discovered_icon_data_url(prompt.get("projectDiscoveredIconDataUrl")).as_deref(),
    ) {
        return image;
    }
    if let Some(ProjectIcon::Tabler { icon, color }) = icon {
        return QuickAccessIcon::asset(&icon, color.as_deref());
    }
    QuickAccessIcon::asset("folder", None)
}
