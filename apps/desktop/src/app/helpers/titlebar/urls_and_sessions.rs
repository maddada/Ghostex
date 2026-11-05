use std::collections::HashMap;

use anyhow::Result;
use raw_window_handle::RawWindowHandle;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn cef_parent_native_view(window: &Window) -> Result<*mut std::ffi::c_void> {
    /*
    CDXC:CefRuntime 2026-07-04:
    Windowed CEF parents its child views on the GPUI window's native handle:
    the root NSView on macOS, the top-level HWND on Windows, and the X11
    window id on Linux (gpui's X11 backend hands out an Xcb handle; the Xlib
    arm covers the same id space for completeness). The pointer stays opaque
    past this point; only the cef platform adapters interpret it.
    */
    let handle = raw_window_handle::HasWindowHandle::window_handle(window)
        .map_err(|error| anyhow::anyhow!("failed to read GPUI raw window handle: {error:?}"))?;
    let native_view = match handle.as_raw() {
        RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr(),
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as *mut std::ffi::c_void,
        RawWindowHandle::Xcb(handle) => handle.window.get() as usize as *mut std::ffi::c_void,
        RawWindowHandle::Xlib(handle) => handle.window as usize as *mut std::ffi::c_void,
        other => {
            anyhow::bail!("windowed CEF requires an AppKit, Win32, or X11 parent, got {other:?}")
        }
    };
    #[cfg(target_os = "linux")]
    crate::cef::note_gpui_window_native_parent(window.window_handle().window_id(), native_view);
    Ok(native_view)
}

pub(crate) fn normalize_address(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        return Some(trimmed.to_string());
    }
    if trimmed == "localhost"
        || trimmed.starts_with("localhost:")
        || trimmed.starts_with("127.0.0.1")
    {
        return Some(format!("http://{trimmed}"));
    }
    if trimmed.contains('.') && !trimmed.contains(' ') {
        return Some(format!("https://{trimmed}"));
    }
    /*
    CDXC:Browser 2026-06-14-17:42:
    The GPUI address field should resolve committed non-empty text the same way as the macOS browser toolbar: explicit schemes are kept, local hosts use http, domain-like text uses https, and free text becomes an in-pane Google search. Empty commits are not normalized; the toolbar restores the current tab URL and returns focus to page content.
    */
    Some(format!(
        "https://www.google.com/search?q={}",
        encode_search_query(trimmed)
    ))
}

pub(crate) fn gpui_open_external_http_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 {
        return Err("External URL is invalid.".to_string());
    }
    let parsed = gpui::http_client::Url::parse(trimmed)
        .map_err(|_| "External URL is invalid.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.host_str().is_none()
    {
        return Err("External URL is invalid.".to_string());
    }
    gpui_spawn_os_open(std::ffi::OsStr::new(trimmed))
}

pub(crate) fn gpui_app_modal_unsupported_settings_command_noop(command_type: &str) -> bool {
    /*
    CDXC:StatusPet 2026-06-24-11:36:
    GPUI Settings status/action requests that affect visible loading state must send an explicit contract-shaped response instead of disappearing here. Keep this matcher only for non-loading actions whose production GPUI bridge is still absent, and do not claim success for installers, Launch Services, Ghostty config, preferences panes, or sound previews.

    CDXC:Settings 2026-06-24-11:59:
    Worker 7 removed the remaining non-privileged Settings action commands from this matcher. New GPUI Settings commands should either perform a bounded action, refresh a visible status, or return an explicit unsupported status/toast instead of being added here by default.
    */
    let _ = command_type;
    false
}

pub(crate) fn gpui_open_url(url: &'static str) -> Result<(), String> {
    /*
    CDXC:Settings 2026-06-24-11:59:
    Settings URL actions in GPUI are bounded to hardcoded product URLs from Rust. Do not accept React-provided URLs, shell commands, environment values, or user paths for docs/System Settings opens.
    */
    gpui_spawn_os_open(std::ffi::OsStr::new(url))
}

/// Terminal link clicks (Ghostty OPEN_URL actions) carry runtime-provided
/// text and only fire on an explicit cmd+click, so this mirrors the macOS
/// host's open-url handling: text with a URL scheme opens with its default
/// handler, and anything else is treated as a file path (`~` expanded) so
/// the ghostty link regex's absolute/relative path matches open too. A
/// terminal link to another app's scheme reaches it only after the reader
/// chose Open (`open_other_app_link_after_confirming`).
pub(crate) fn gpui_open_terminal_action_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Terminal link is empty.".to_string());
    }
    if trimmed.len() > 2048 {
        return Err("Terminal link is too long.".to_string());
    }
    let open_value = gpui_terminal_markdown_image_reference_path(trimmed).unwrap_or(trimmed);
    if gpui_terminal_link_has_scheme(open_value) {
        return gpui_spawn_os_open(std::ffi::OsStr::new(open_value));
    }
    gpui_spawn_os_open(gpui_expand_terminal_link_path(open_value).as_os_str())
}

pub(crate) fn gpui_terminal_link_is_web_url(link: &str) -> bool {
    link.get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || link
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
}

/// A web page or an email link: what a link from terminal output or a document may open without
/// asking. Any other scheme hands the link to another app, which asks first
/// (`open_other_app_link_after_confirming`).
pub(crate) fn gpui_link_is_web_or_email(link: &str) -> bool {
    gpui_terminal_link_is_web_url(link)
        || link
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("mailto:"))
}

pub(crate) fn gpui_daemon_session_items_from_presentation_snapshot(
    snapshot: &serde_json::Value,
    active_project_id: Option<&str>,
) -> Vec<serde_json::Value> {
    let Some(snapshot) = snapshot.as_object() else {
        return Vec::new();
    };
    let projects_by_id = json_array_field(snapshot, "projects")
        .into_iter()
        .flatten()
        .filter_map(|project| {
            let project = project.as_object()?;
            let project_id = json_string_field(project, "projectId")?;
            Some((project_id.to_string(), project.clone()))
        })
        .collect::<HashMap<_, _>>();
    json_array_field(snapshot, "sessions")
        .into_iter()
        .flatten()
        .filter_map(|session| {
            gpui_presentation_session_to_daemon_session_item(
                session,
                &projects_by_id,
                active_project_id,
            )
        })
        .collect()
}

pub(crate) fn gpui_presentation_session_to_daemon_session_item(
    session: &serde_json::Value,
    projects_by_id: &HashMap<String, serde_json::Map<String, serde_json::Value>>,
    active_project_id: Option<&str>,
) -> Option<serde_json::Value> {
    let session = session.as_object()?;
    let kind = json_string_field(session, "kind")?;
    let surface = json_string_field(session, "surface")?;
    if surface != "workspace" || !matches!(kind, "terminal" | "agent") {
        return None;
    }
    let project_id = json_string_field(session, "projectId")?;
    let session_id = json_string_field(session, "sessionId")?;
    let project = projects_by_id.get(project_id);
    let cwd = json_string_field(session, "cwd")
        .or_else(|| project.and_then(|project| json_string_field(project, "path")))
        .unwrap_or("");
    let title = json_string_field(session, "displayTitle")
        .or_else(|| json_string_field(session, "primaryTitle"))
        .or_else(|| json_string_field(session, "title"));
    let started_at = json_string_field(session, "createdAt")?;
    let shell = json_string_field(session, "sessionPersistenceProvider").unwrap_or("");
    let lifecycle_state = json_string_field(session, "lifecycleState").unwrap_or("unknown");
    let provider_state = json_string_field(session, "providerSessionState").unwrap_or("unknown");
    let status = gpui_daemon_session_status_from_gxserver(lifecycle_state, provider_state);
    let mut item = serde_json::Map::new();
    gpui_insert_optional_string(
        &mut item,
        "agentName",
        json_string_field(session, "agentName").or_else(|| json_string_field(session, "agentId")),
    );
    item.insert(
        "agentStatus".to_string(),
        serde_json::Value::String(
            gpui_daemon_agent_status(json_string_field(session, "activity")).to_string(),
        ),
    );
    /*
    CDXC:Sessions 2026-06-24-12:00:
    gxserver presentation does not currently expose terminal dimensions. Keep cols/rows as explicit zero values in GPUI Running Sessions until a real dimensions contract exists instead of inventing 80x24 or reading terminal/private process state.
    */
    item.insert("cols".to_string(), serde_json::Value::Number(0.into()));
    item.insert(
        "cwd".to_string(),
        serde_json::Value::String(cwd.to_string()),
    );
    item.insert(
        "isCurrentWorkspace".to_string(),
        serde_json::Value::Bool(active_project_id == Some(project_id)),
    );
    item.insert(
        "ownership".to_string(),
        serde_json::Value::String("gxserver".to_string()),
    );
    item.insert(
        "restoreState".to_string(),
        serde_json::Value::String("live".to_string()),
    );
    item.insert("rows".to_string(), serde_json::Value::Number(0.into()));
    item.insert(
        "sessionId".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    item.insert(
        "shell".to_string(),
        serde_json::Value::String(shell.to_string()),
    );
    item.insert(
        "startedAt".to_string(),
        serde_json::Value::String(started_at.to_string()),
    );
    item.insert(
        "status".to_string(),
        serde_json::Value::String(status.to_string()),
    );
    gpui_insert_optional_string(&mut item, "title", title);
    item.insert(
        "workspaceId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    Some(serde_json::Value::Object(item))
}
