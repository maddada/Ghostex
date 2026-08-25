use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path};
use std::time::Duration;

use gpui::{Image, ImageFormat};

use crate::GhostexGpuiApp;
use crate::app::helpers::{gpui_gxserver_domain_projects_result, gpui_gxserver_rpc_result};

use super::{
    GpuiExtensionPermission, GpuiExtensionPlacement, GpuiExtensionPopupSize,
    GpuiExtensionProjectMetadata, GpuiExtensionsSnapshot, GpuiInstalledExtension,
};

pub(crate) fn read_gpui_extensions_snapshot() -> Result<
    (
        GpuiExtensionsSnapshot,
        HashMap<String, GpuiExtensionProjectMetadata>,
        HashMap<String, serde_json::Value>,
    ),
    String,
> {
    let result = gpui_gxserver_rpc_result(
        "/api/listExtensions",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    let extensions = result
        .get("extensions")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "gxserver returned invalid extension metadata.".to_string())?;
    let installed = extensions
        .iter()
        .filter_map(parse_installed_extension)
        .map(|extension| (extension.id.clone(), extension))
        .collect();
    let projects: HashMap<_, _> = gpui_gxserver_domain_projects_result(Duration::from_secs(5))?
        .iter()
        .filter_map(parse_project_metadata)
        .map(|project| (project.project_id.clone(), project))
        .collect();
    let presentation = gpui_gxserver_rpc_result(
        "/api/readPresentationSnapshot",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    let presentation_sessions = presentation
        .get("snapshot")
        .and_then(|snapshot| snapshot.get("sessions"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|session| copy_details_from_presentation_session(session, &projects))
        .collect();
    Ok((
        GpuiExtensionsSnapshot { installed },
        projects,
        presentation_sessions,
    ))
}

impl GhostexGpuiApp {
    pub(crate) fn refresh_extensions_in_background(&mut self, cx: &mut gpui::Context<Self>) {
        if self.extensions_refresh_in_flight {
            return;
        }
        self.extensions_refresh_in_flight = true;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async { read_gpui_extensions_snapshot() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.extensions_refresh_in_flight = false;
                if let Ok((snapshot, projects, session_details)) = result {
                    this.extensions_snapshot = snapshot;
                    this.extension_projects = projects;
                    this.extension_session_details = session_details;
                    this.broadcast_extension_context_changes(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

fn parse_installed_extension(value: &serde_json::Value) -> Option<GpuiInstalledExtension> {
    let object = value.as_object()?;
    let id = text(object.get("id"))?.to_string();
    let manifest = object.get("manifest")?.as_object()?;
    let state = object.get("state")?.as_object()?;
    let title = text(manifest.get("title"))?.to_string();
    let icon_image = extension_icon_image(&id, text(manifest.get("icon"))?)?;
    let declared_permissions = parse_permissions(manifest.get("permissions"));
    let granted_permissions = parse_permissions(state.get("grantedPermissions"));
    let placements = manifest
        .get("placements")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter_map(GpuiExtensionPlacement::from_str)
        .collect();
    let placement = state
        .get("placement")
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiExtensionPlacement::from_str);
    let preferences = value_map(state.get("preferences"));
    let storage = value_map(state.get("storage"));
    let runtime_url = object
        .get("runtime")
        .and_then(serde_json::Value::as_object)
        .and_then(|runtime| runtime.get("url"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let popup_size = manifest
        .get("popup")
        .and_then(serde_json::Value::as_object)
        .and_then(|size| {
            Some(GpuiExtensionPopupSize {
                width: size.get("width")?.as_u64()?.min(420) as f32,
                height: size.get("height")?.as_u64()?.min(640) as f32,
            })
        });
    let badge_lines = object
        .get("badge")
        .and_then(serde_json::Value::as_object)
        .and_then(|badge| badge.get("lines"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect();
    Some(GpuiInstalledExtension {
        id,
        title,
        icon_image,
        declared_permissions,
        granted_permissions,
        placements,
        placement,
        popup_size,
        preferences,
        storage,
        runtime_url,
        badge_lines,
        enabled: state.get("enabled").and_then(serde_json::Value::as_bool) == Some(true),
        pinned: state.get("pinned").and_then(serde_json::Value::as_bool) == Some(true),
        terminal_pane: manifest.get("kind").and_then(serde_json::Value::as_str)
            == Some("terminal-pane"),
    })
}

fn extension_icon_image(id: &str, icon: &str) -> Option<std::sync::Arc<Image>> {
    let icon = Path::new(icon);
    if icon.is_absolute()
        || icon
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return None;
    }
    let payload_dir = crate::shared_settings::ghostex_storage_paths()
        .extensions_dir()
        .join("installed")
        .join(id);
    let bytes = std::fs::read(payload_dir.join(icon)).ok()?;
    if bytes.is_empty() || bytes.len() > 256 * 1024 {
        return None;
    }
    let bytes = normalize_extension_titlebar_svg(bytes)?;
    Some(std::sync::Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        bytes,
    )))
}

fn normalize_extension_titlebar_svg(bytes: Vec<u8>) -> Option<Vec<u8>> {
    const TITLEBAR_ICON_COLOR: &str = "#b9b9b9";

    let mut svg = String::from_utf8(bytes)
        .ok()?
        .replace("currentColor", TITLEBAR_ICON_COLOR);
    let svg_start = svg.find("<svg")?;
    let tag_end = svg[svg_start..].find('>')? + svg_start;
    if !svg_tag_has_attribute(&svg[svg_start..tag_end], "fill") {
        svg.insert_str(tag_end, " fill=\"#b9b9b9\"");
    }
    Some(svg.into_bytes())
}

fn svg_tag_has_attribute(tag: &str, name: &str) -> bool {
    tag.match_indices(name).any(|(start, _)| {
        let before = tag[..start].chars().next_back();
        let after = tag[start + name.len()..].trim_start();
        before.is_none_or(|character| character.is_ascii_whitespace() || character == '<')
            && after.starts_with('=')
    })
}

fn parse_permissions(value: Option<&serde_json::Value>) -> HashSet<GpuiExtensionPermission> {
    value
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter_map(GpuiExtensionPermission::from_str)
        .collect()
}

fn value_map(value: Option<&serde_json::Value>) -> BTreeMap<String, serde_json::Value> {
    value
        .and_then(serde_json::Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_project_metadata(value: &serde_json::Value) -> Option<GpuiExtensionProjectMetadata> {
    let object = value.as_object()?;
    let project_id = text(object.get("projectId"))?.to_string();
    let worktree = object
        .get("worktree")
        .and_then(serde_json::Value::as_object);
    let remote_machine_name = object
        .get("remoteMachineContext")
        .and_then(serde_json::Value::as_object)
        .and_then(|remote| text(remote.get("machineName")))
        .or_else(|| text(object.get("remoteMachine")))
        .or_else(|| text(object.get("machineName")))
        .map(str::to_string);
    Some(GpuiExtensionProjectMetadata {
        project_id,
        name: text(object.get("title"))
            .or_else(|| text(object.get("name")))
            .unwrap_or("")
            .to_string(),
        path: text(object.get("path")).map(str::to_string),
        remote_machine_name,
        is_worktree: worktree.is_some(),
        worktree_branch: worktree
            .and_then(|value| text(value.get("branch")))
            .map(str::to_string),
        worktree_name: worktree
            .and_then(|value| text(value.get("name")))
            .map(str::to_string),
        parent_project_name: worktree
            .and_then(|value| text(value.get("parentProjectName")))
            .map(str::to_string),
    })
}

fn text(value: Option<&serde_json::Value>) -> Option<&str> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn copy_details_from_presentation_session(
    value: &serde_json::Value,
    projects: &HashMap<String, GpuiExtensionProjectMetadata>,
) -> Option<(String, serde_json::Value)> {
    let session = value.as_object()?;
    let session_id = text(session.get("sessionId"))?.to_string();
    let project = text(session.get("projectId")).and_then(|id| projects.get(id));
    let title = text(session.get("displayTitle"))
        .or_else(|| text(session.get("primaryTitle")))
        .or_else(|| text(session.get("alias")))
        .unwrap_or("Session");
    let alias = text(session.get("alias")).filter(|alias| *alias != title);
    let terminal_title = text(session.get("terminalTitle")).filter(|terminal| *terminal != title);
    let persistence_provider = text(session.get("sessionPersistenceProvider"));
    let persistence_name = text(session.get("sessionPersistenceName"));
    let persistence = match (persistence_provider, persistence_name) {
        (Some(provider), Some(name)) => Some(format!("{provider} ({name})")),
        (Some(provider), None) => Some(provider.to_string()),
        (None, Some(name)) => Some(name.to_string()),
        (None, None) => None,
    };
    let remote_machine = text(session.get("remoteMachine"))
        .or_else(|| text(session.get("remoteMachineName")))
        .or_else(|| project.and_then(|project| project.remote_machine_name.as_deref()));
    Some((
        session_id.clone(),
        serde_json::json!({
            "title": title,
            "alias": alias,
            "sessionId": session_id,
            "routingId": text(session.get("sessionRoutingId")),
            "kind": text(session.get("sessionKind")).or_else(|| text(session.get("kind"))),
            "status": text(session.get("lifecycleState")).or_else(|| text(session.get("status"))),
            "activity": text(session.get("activityLabel")).or_else(|| text(session.get("activity"))),
            "agent": text(session.get("agentIcon")),
            "agentSessionId": text(session.get("agentSessionId")),
            "terminalTitle": terminal_title,
            "detail": text(session.get("detail")),
            "persistence": persistence,
            "remoteMachine": remote_machine,
            "project": project.map(|project| project.name.as_str()),
            "projectPath": project.and_then(|project| project.path.as_deref()),
            "worktree": project.and_then(|project| project.worktree_name.as_deref()),
            "worktreeBranch": project.and_then(|project| project.worktree_branch.as_deref()),
            "parentProject": project.and_then(|project| project.parent_project_name.as_deref()),
            "lastActive": text(session.get("lastInteractionAt")),
        }),
    ))
}
