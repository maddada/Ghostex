//! `recentProjects`: this computer's parked projects from `/api/listRecentProjects`, and every
//! remote machine's client-parked ones, newest close first
//! (the app runtime's `createGpuiRecentProjects` and `createGpuiRemoteRecentProjects`, deleted).
//!
//! CDXC:Projects 2026-06-24-12:27:
//! GPUI Recent Projects hydrate from `/api/listRecentProjects`, a gxserver-owned parked-project
//! contract. Keep an empty drawer when the endpoint has no explicit rows; never derive recent
//! projects from labels, inactive sessions, presentation titles, command text, or path guessing.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::keys::{MachineId, ProjectKey};
use crate::presentation_store::PresentationStore;

/// `SIDEBAR_COMMAND_ICON_IDS` (packages/shared/sidebar-command-icons.ts).
const SIDEBAR_COMMAND_ICON_IDS: [&str; 59] = [
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

/// `GPUI_SIDEBAR_THEME_VALUES`, with `plain-dark` read as `dark-2` (`normalizeGpuiSidebarTheme`).
const SIDEBAR_THEMES: [&str; 13] = [
    "dark-1",
    "dark-2",
    "plain-dark",
    "plain-light",
    "dark-green",
    "dark-blue",
    "dark-red",
    "dark-pink",
    "dark-orange",
    "light-blue",
    "light-green",
    "light-pink",
    "light-orange",
];

fn trimmed(value: Option<&Value>) -> &str {
    value.and_then(Value::as_str).map(str::trim).unwrap_or("")
}

/// `normalizeGpuiProjectPath`: trimmed, trailing slashes dropped, `None` when empty.
fn project_path(value: Option<&Value>) -> Option<String> {
    let path = trimmed(value);
    (!path.is_empty()).then(|| path.trim_end_matches('/').to_string())
}

fn icon_data_url(value: Option<&Value>) -> Option<&str> {
    let text = value?.as_str()?;
    (text.starts_with("data:image/png;base64,") || text.starts_with("data:image/svg+xml;base64,"))
        .then_some(text)
}

/// `normalizeSidebarCommandIconColor`: `#rrggbb`, lower-cased.
fn color(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    (text.len() == 7
        && text.starts_with('#')
        && text[1..].bytes().all(|byte| byte.is_ascii_hexdigit()))
    .then(|| text.to_lowercase())
}

/// `normalizeWorkspaceProjectIcon`.
fn project_icon(value: Option<&Value>) -> Option<Value> {
    let icon = value?.as_object()?;
    match icon.get("kind").and_then(Value::as_str) {
        Some("image") => {
            let data_url = icon_data_url(icon.get("dataUrl"))?;
            let mut out = Map::new();
            out.insert("dataUrl".into(), Value::from(data_url));
            out.insert("kind".into(), Value::from("image"));
            Some(Value::Object(out))
        }
        Some("tabler") => {
            let name = icon.get("icon").and_then(Value::as_str)?;
            if !SIDEBAR_COMMAND_ICON_IDS.contains(&name) {
                return None;
            }
            let mut out = Map::new();
            if let Some(color) = color(icon.get("color")) {
                out.insert("color".into(), Value::from(color));
            }
            out.insert("icon".into(), Value::from(name));
            out.insert("kind".into(), Value::from("tabler"));
            Some(Value::Object(out))
        }
        _ => None,
    }
}

/// `normalizeGpuiSidebarTheme(project.theme) ?? resolveSidebarTheme(settings.sidebarTheme, 'dark')`.
fn project_theme(value: Option<&Value>, default_theme: &str) -> String {
    match value.and_then(Value::as_str) {
        Some("plain-dark") => "dark-2".to_string(),
        Some(theme) if SIDEBAR_THEMES.contains(&theme) => theme.to_string(),
        _ => default_theme.to_string(),
    }
}

/// `Number.isFinite(count) ? Math.max(0, Math.floor(count)) : 0`.
fn session_count(value: Option<&Value>) -> i64 {
    value
        .and_then(Value::as_f64)
        .map(|count| count.floor().max(0.0) as i64)
        .unwrap_or(0)
}

/// The appearance and close time every row carries, in the TypeScript's key order.
fn appearance(project: &Value, default_theme: &str, row: &mut Map<String, Value>) {
    if let Some(icon) = project_icon(project.get("icon")) {
        row.insert("icon".into(), icon);
    }
    if let Some(data_url) = icon_data_url(project.get("iconDataUrl")) {
        row.insert("iconDataUrl".into(), Value::from(data_url));
    }
    let closed_at = trimmed(project.get("recentClosedAt"));
    if !closed_at.is_empty() {
        row.insert("recentClosedAt".into(), Value::from(closed_at));
    }
    if let Some(color) = color(project.get("themeColor")) {
        row.insert("themeColor".into(), Value::from(color));
    }
    row.insert(
        "theme".into(),
        Value::from(project_theme(project.get("theme"), default_theme)),
    );
}

/// `Date.parse` of an ISO time, in milliseconds; 0 when it does not parse.
fn closed_at_ms(row: &Value) -> i64 {
    row.get("recentClosedAt")
        .and_then(Value::as_str)
        .and_then(parse_iso_ms)
        .unwrap_or(0)
}

/// `YYYY-MM-DDTHH:MM:SS[.fff](Z|±hh:mm)`, the only shape gxserver writes.
fn parse_iso_ms(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let mut rest = &text[19..];
    let mut millis = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits: String = fraction.chars().take_while(char::is_ascii_digit).collect();
        rest = &fraction[digits.len()..];
        millis = format!("{digits:0<3}")[..3].parse::<i64>().ok()?;
    }
    let offset_minutes = match rest {
        "Z" | "z" => 0,
        offset if offset.len() == 6 && (offset.starts_with('+') || offset.starts_with('-')) => {
            let sign = if offset.starts_with('-') { -1 } else { 1 };
            sign * (offset[1..3].parse::<i64>().ok()? * 60 + offset[4..6].parse::<i64>().ok()?)
        }
        _ => return None,
    };
    let year_shift = if month <= 2 { year - 1 } else { year };
    let era = if year_shift >= 0 {
        year_shift
    } else {
        year_shift - 399
    } / 400;
    let year_of_era = year_shift - era * 400;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(
        ((days * 86_400 + hour * 3600 + minute * 60 + second - offset_minutes * 60) * 1000)
            + millis,
    )
}

/// `createGpuiRecentProjects` and `createGpuiRemoteRecentProjects`, sorted together by close time
/// (a stable sort, so equal times keep the producer's order).
pub(crate) fn recent_projects(
    store: &PresentationStore,
    local_rows: &[Value],
    remote_rows: &[(String, Vec<Value>)],
    remote_machine_names: &BTreeMap<String, String>,
    default_theme: &str,
) -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for project in local_rows {
        let project_id = trimmed(project.get("projectId"));
        let title = trimmed(project.get("title"));
        let Some(path) = project_path(project.get("path")) else {
            continue;
        };
        if project_id.is_empty() || title.is_empty() {
            continue;
        }
        let mut row = Map::new();
        appearance(project, default_theme, &mut row);
        row.insert("path".into(), Value::from(path));
        row.insert("projectId".into(), Value::from(project_id));
        row.insert(
            "sessionCount".into(),
            Value::from(session_count(project.get("sessionCount"))),
        );
        row.insert("title".into(), Value::from(title));
        rows.push(Value::Object(row));
    }
    for (machine_id, projects) in remote_rows {
        let Some(machine_name) = remote_machine_names.get(machine_id) else {
            continue;
        };
        let machine = MachineId::Remote(machine_id.clone());
        // The runtime reconciled against the machine's LIVE presentation only.
        let presentation = store.loaded_live(&machine);
        for project in projects {
            let project_id = trimmed(project.get("projectId"));
            let presented = presentation.and_then(|loaded| loaded.project(project_id));
            if presentation.is_some() && presented.is_none() {
                continue;
            }
            let presented_title = presented.map(|row| row.title.trim()).unwrap_or("");
            let title = if presented_title.is_empty() {
                trimmed(project.get("title"))
            } else {
                presented_title
            };
            // `presentationProject?.path ?? project.path`.
            let presented_path = presented
                .and_then(|row| row.path.as_deref())
                .map(Value::from);
            let path = project_path(presented_path.as_ref().or(project.get("path")));
            let Some(path) = path else {
                continue;
            };
            if project_id.is_empty() || title.is_empty() {
                continue;
            }
            let session_count = match presentation {
                Some(loaded) => loaded
                    .server_sessions()
                    .filter(|session| {
                        session.project_id == project_id
                            && session.visible_in_sidebar_by_default
                            && session.surface.as_str() != "commands"
                    })
                    .count() as i64,
                None => session_count(project.get("sessionCount")),
            };
            let mut row = Map::new();
            appearance(project, default_theme, &mut row);
            row.insert("path".into(), Value::from(path));
            row.insert(
                "projectId".into(),
                Value::from(
                    ProjectKey::remote(machine_id.as_str(), project_id).to_workspace_project_id(),
                ),
            );
            row.insert("remoteMachineId".into(), Value::from(machine_id.as_str()));
            row.insert(
                "remoteMachineName".into(),
                Value::from(if machine_name.is_empty() {
                    "Remote"
                } else {
                    machine_name.as_str()
                }),
            );
            row.insert("sessionCount".into(), Value::from(session_count));
            row.insert("title".into(), Value::from(title));
            rows.push(Value::Object(row));
        }
    }
    rows.sort_by_key(|row| std::cmp::Reverse(closed_at_ms(row)));
    Value::Array(rows)
}
