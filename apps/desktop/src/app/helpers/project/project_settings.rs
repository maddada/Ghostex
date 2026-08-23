// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds project settings metadata updates,
// recent-project mutations, and project settings/presentation conversions.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

#[derive(Clone, Debug)]
pub(crate) enum GpuiProjectSettingsMetadataUpdate {
    WorktreeCommand {
        project_id: String,
        command: String,
    },
    BeadsDisplayKey {
        project_id: String,
        display_key: String,
    },
    BeadsDirectory {
        project_id: String,
        directory: String,
    },
    DocsDirectory {
        project_id: String,
        directory: String,
    },
    #[allow(dead_code)] // no live path: sidebar agent/command metadata is projected by gxserver now
    SidebarCommands {
        project_id: String,
        commands: Vec<GpuiStoredSidebarCommand>,
        command_order: Vec<String>,
        deleted_default_command_ids: Vec<String>,
    },
}

impl GpuiProjectSettingsMetadataUpdate {
    pub(crate) fn project_id(&self) -> &str {
        match self {
            Self::WorktreeCommand { project_id, .. }
            | Self::BeadsDisplayKey { project_id, .. }
            | Self::BeadsDirectory { project_id, .. }
            | Self::DocsDirectory { project_id, .. }
            | Self::SidebarCommands { project_id, .. } => project_id,
        }
    }
}

pub(crate) fn gpui_project_settings_projects_from_domain_projects_or_presentation(
    domain_projects: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    gpui_project_settings_projects_from_domain_projects_or_else(domain_projects, || {
        gpui_read_gxserver_presentation_snapshot()
            .ok()
            .and_then(|snapshot| {
                let snapshot = snapshot.as_object()?;
                json_array_field(snapshot, "projects").map(|projects| {
                    gpui_project_settings_projects_from_presentation_projects(projects)
                })
            })
            .unwrap_or_default()
    })
}

pub(crate) fn gpui_project_settings_projects_from_domain_projects_or_else<F>(
    domain_projects: &[serde_json::Value],
    presentation_projects: F,
) -> Vec<serde_json::Value>
where
    F: FnOnce() -> Vec<serde_json::Value>,
{
    /*
    CDXC:GPUIRecentProjects 2026-06-25-19:02:
    Project Settings parks only explicit boolean `isRecentProject: true` domain rows; string, number, false, missing, and malformed values do not become Recent Projects. If all domain rows are explicit recent or otherwise unusable, keep the presentation fallback path available rather than fabricating Settings rows.
    */
    let project_settings_projects =
        gpui_project_settings_projects_from_domain_projects(domain_projects);
    if !project_settings_projects.is_empty() {
        return project_settings_projects;
    }

    presentation_projects()
}

pub(crate) fn gpui_project_settings_projects_from_domain_projects(
    domain_projects: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    domain_projects
        .iter()
        .filter_map(gpui_project_settings_project_from_domain_project)
        .collect::<Vec<_>>()
}

pub(crate) fn gpui_project_settings_projects_from_presentation_projects(
    presentation_projects: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    presentation_projects
        .iter()
        .filter_map(gpui_project_settings_project_from_presentation_project)
        .collect::<Vec<_>>()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiRecentProjectMutation {
    Remove,
    Restore,
}

impl GpuiRecentProjectMutation {
    pub(crate) fn endpoint(self) -> &'static str {
        match self {
            Self::Remove => "/api/removeRecentProject",
            Self::Restore => "/api/restoreRecentProject",
        }
    }
}

pub(crate) fn gpui_recent_projects_result_message(request: &GpuiRecentProjectsRequest) -> serde_json::Value {
    /*
    Recent Projects follows the same transient app-modal response path as
    Previous Sessions. The owning gxserver remains the only persistence
    authority; failed local or remote reads return an empty contract-shaped
    result without exposing transport details or daemon response bodies.
    */
    let recent_projects = match request.machine_id.as_deref() {
        None => gpui_gxserver_recent_projects(Duration::from_secs(10)),
        Some(machine_id) => request
            .remote_target
            .as_ref()
            .map(|target| {
                gpui_recent_projects_from_remote_gxserver(
                    target,
                    machine_id,
                    request.machine_name.as_deref(),
                )
            })
            .unwrap_or_default(),
    };
    let mut message = serde_json::json!({
        "recentProjects": recent_projects,
        "type": "recentProjectsResult",
    });
    if let Some(machine_id) = request.machine_id.as_ref() {
        message["machineId"] = serde_json::json!(machine_id);
    }
    message
}

pub(crate) fn gpui_recent_project_mutation_and_result(
    mutation: GpuiRecentProjectMutation,
    project_id: String,
    request: GpuiRecentProjectsRequest,
) -> (bool, serde_json::Value) {
    let mutated = match request.machine_id.as_deref() {
        None => gpui_gxserver_rpc_result(
            mutation.endpoint(),
            &serde_json::json!({ "projectId": project_id }),
            Duration::from_secs(10),
        )
        .is_ok(),
        Some(_) => request.remote_target.as_ref().is_some_and(|target| {
            gpui_remote_gxserver_rpc_result(
                target,
                mutation.endpoint(),
                &serde_json::json!({ "projectId": project_id }),
                Duration::from_secs(10),
            )
            .is_ok()
        }),
    };
    let result = gpui_recent_projects_result_message(&request);
    (mutated, result)
}

pub(crate) fn gpui_project_settings_project_from_domain_project(
    project: &serde_json::Value,
) -> Option<serde_json::Value> {
    let project = project.as_object()?;
    /*
    CDXC:GPUIRecentProjects 2026-06-25-18:50:
    GPUI app-modal normal project lists must mirror macOS by excluding explicit parked Recent Projects from `/api/listProjects` instead of deriving Settings project rows from those parked rows. Only `isRecentProject: true` is parked; zero-session normal projects and rows without the flag remain settings projects.
    */
    if gpui_gxserver_project_row_is_explicit_recent_project(project) {
        return None;
    }
    let project_id = gpui_trimmed_json_string_field(project, "projectId")?;
    let name = gpui_trimmed_json_string_field(project, "name")?;
    let path = gpui_trimmed_json_string_field(project, "path")?;
    let git_config = project
        .get("gitConfig")
        .and_then(serde_json::Value::as_object);
    let project_board_config = project
        .get("projectBoardConfig")
        .and_then(serde_json::Value::as_object);
    let worktree = project
        .get("worktree")
        .and_then(serde_json::Value::as_object);

    let mut item = serde_json::Map::new();
    item.insert(
        "name".to_string(),
        serde_json::Value::String(name.to_string()),
    );
    item.insert(
        "path".to_string(),
        serde_json::Value::String(path.to_string()),
    );
    item.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "worktreeParentProjectId",
        worktree.and_then(|worktree| gpui_trimmed_json_string_field(worktree, "parentProjectId")),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "worktreeCommand",
        git_config.and_then(|config| gpui_trimmed_json_string_field(config, "worktreeCommand")),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "beadsDisplayKey",
        project_board_config
            .and_then(|config| gpui_trimmed_json_string_field(config, "beadsDisplayKey"))
            .or_else(|| {
                git_config
                    .and_then(|config| gpui_trimmed_json_string_field(config, "beadsDisplayKey"))
            }),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "beadsDirectory",
        project_board_config
            .and_then(|config| gpui_trimmed_json_string_field(config, "beadsDirectory")),
    );
    /*
    CDXC:DocsRootDirectory 2026-08-09:
    The per-project Docs root rides in the same per-project config object the
    Beads directory already uses, so Settings -> Projects keeps one storage seam
    and the feature needs no new domain field, column, or migration.
    */
    gpui_insert_optional_nonempty_string(
        &mut item,
        "docsDirectory",
        project_board_config
            .and_then(|config| gpui_trimmed_json_string_field(config, "docsDirectory")),
    );
    Some(serde_json::Value::Object(item))
}

pub(crate) fn gpui_project_settings_project_from_presentation_project(
    project: &serde_json::Value,
) -> Option<serde_json::Value> {
    let project = project.as_object()?;
    let project_id = gpui_trimmed_json_string_field(project, "projectId")?;
    let name = gpui_trimmed_json_string_field(project, "title")?;
    let path = gpui_trimmed_json_string_field(project, "path")?;
    let mut item = serde_json::Map::new();
    item.insert(
        "name".to_string(),
        serde_json::Value::String(name.to_string()),
    );
    item.insert(
        "path".to_string(),
        serde_json::Value::String(path.to_string()),
    );
    item.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "worktreeParentProjectId",
        project
            .get("worktree")
            .and_then(serde_json::Value::as_object)
            .and_then(|worktree| gpui_trimmed_json_string_field(worktree, "parentProjectId")),
    );
    Some(serde_json::Value::Object(item))
}

pub(crate) fn gpui_update_project_settings_metadata(
    update: GpuiProjectSettingsMetadataUpdate,
) -> Result<(), String> {
    let project_id = gpui_trimmed_nonempty_str(Some(update.project_id()))
        .ok_or_else(|| "Missing gxserver project id.".to_string())?
        .to_string();
    let project = gpui_find_gxserver_project_by_id(&project_id)?;
    let mut params = serde_json::Map::new();
    params.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id),
    );

    match update {
        GpuiProjectSettingsMetadataUpdate::WorktreeCommand { command, .. } => {
            let mut git_config = gpui_clone_json_object_field(&project, "gitConfig");
            git_config.insert(
                "worktreeCommand".to_string(),
                gpui_settings_metadata_string_or_null(&command),
            );
            params.insert(
                "gitConfig".to_string(),
                serde_json::Value::Object(git_config),
            );
        }
        GpuiProjectSettingsMetadataUpdate::BeadsDisplayKey { display_key, .. } => {
            let display_key = gpui_settings_beads_display_key_or_null(&display_key);
            let mut git_config = gpui_clone_json_object_field(&project, "gitConfig");
            git_config.insert("beadsDisplayKey".to_string(), display_key.clone());
            params.insert(
                "gitConfig".to_string(),
                serde_json::Value::Object(git_config),
            );

            let mut project_board_config =
                gpui_clone_json_object_field(&project, "projectBoardConfig");
            project_board_config.insert("beadsDisplayKey".to_string(), display_key);
            params.insert(
                "projectBoardConfig".to_string(),
                serde_json::Value::Object(project_board_config),
            );
        }
        GpuiProjectSettingsMetadataUpdate::BeadsDirectory { directory, .. } => {
            let mut project_board_config =
                gpui_clone_json_object_field(&project, "projectBoardConfig");
            project_board_config.insert(
                "beadsDirectory".to_string(),
                gpui_settings_metadata_string_or_null(&directory),
            );
            params.insert(
                "projectBoardConfig".to_string(),
                serde_json::Value::Object(project_board_config),
            );
        }
        GpuiProjectSettingsMetadataUpdate::DocsDirectory { directory, .. } => {
            let mut project_board_config =
                gpui_clone_json_object_field(&project, "projectBoardConfig");
            project_board_config.insert(
                "docsDirectory".to_string(),
                gpui_settings_metadata_string_or_null(&directory),
            );
            params.insert(
                "projectBoardConfig".to_string(),
                serde_json::Value::Object(project_board_config),
            );
        }
        GpuiProjectSettingsMetadataUpdate::SidebarCommands {
            commands,
            command_order,
            deleted_default_command_ids,
            ..
        } => {
            params.insert(
                "customCommands".to_string(),
                gpui_stored_sidebar_commands_value(&commands),
            );
            params.insert(
                "customCommandOrder".to_string(),
                gpui_string_array_value(&command_order),
            );
            params.insert(
                "deletedDefaultCommandIds".to_string(),
                gpui_string_array_value(&deleted_default_command_ids),
            );
        }
    }

    let result = gpui_gxserver_rpc_result(
        "/api/updateProject",
        &serde_json::Value::Object(params),
        Duration::from_secs(10),
    )?;
    if result
        .get("project")
        .and_then(serde_json::Value::as_object)
        .is_some()
    {
        Ok(())
    } else {
        Err("gxserver returned an invalid project update result.".to_string())
    }
}

pub(crate) fn gpui_active_project_id_from_snapshot(snapshot: Option<&GpuiProjectSnapshot>) -> Option<&str> {
    snapshot
        .and_then(|snapshot| snapshot.active_project_id.as_ref())
        .map(|project_id| project_id.0.as_str())
}

pub(crate) fn gpui_project_snapshot_is_quick_automations_overview(
    snapshot: Option<&GpuiProjectSnapshot>,
) -> bool {
    gpui_active_project_id_from_snapshot(snapshot) == Some(GPUI_QUICK_AUTOMATIONS_PROJECT_ID)
}

pub(crate) fn automate_workarea_runtime_url_from_project_snapshot(
    snapshot: &GpuiProjectSnapshot,
    runtime_settings: &cef::SidebarRuntimeSettingsSnapshot,
) -> Option<ProjectWorkareaRealRuntimeUrl> {
    /*
    CDXC:GPUIAutomateWorkarea 2026-07-04-23:18:
    Automate mirrors macOS `createProjectAutomateEditorUrl`: use the bundled Kanban/tasks CEF page, the explicit project identity params, the automate-mode project editor id, and `surface=automations`. Projectless contexts, missing project path, or missing automateBoardId must stay on the placeholder instead of synthesizing an Automate URL.

    CDXC:GPUIAutomateStable 2026-07-26:
    Project-scoped Automate is no longer an experimental GPUI feature. Mark that first-party workarea explicitly so the shared page does not apply the Show Beta Features content gate or experimental label. Quick Automations Overview keeps its existing experimental startup seed.
    */
    if !snapshot.feature_availability.automate {
        return None;
    }
    let active_project_id = snapshot.active_project_id.as_ref()?.0.clone();
    if active_project_id == GPUI_QUICK_AUTOMATIONS_PROJECT_ID {
        /*
        CDXC:GPUIQuickAutomationsOverview 2026-07-08:
        Mirror macOS `createQuickAutomationsProjectEditorUrl` in `native/sidebar/native-sidebar.tsx`: the quick-automations project is a real Automate overview surface with empty `projectPath`, all-project scope, and the same Show Beta Features seed. Its identity is the project id, so it must not require an in-memory project path or be rejected by the projectless guard.
        */
        let surface_id = snapshot.surface_ids.automate_board_id.as_ref()?.clone();
        let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_KANBAN_URL", "kanban.html").ok()?;
        return ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(
            append_url_query_params_with_percent_encoded_spaces(
                base_url,
                &[
                    (
                        "projectName",
                        GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE.to_string(),
                    ),
                    ("projectPath", String::new()),
                    ("projectId", GPUI_QUICK_AUTOMATIONS_PROJECT_ID.to_string()),
                    ("projectEditorId", surface_id),
                    ("surface", "automations".to_string()),
                    ("scope", "all".to_string()),
                    (
                        "beadsDisplayKey",
                        GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE.to_string(),
                    ),
                    (
                        "showBetaFeatures",
                        if runtime_settings.show_beta_features {
                            "true"
                        } else {
                            "false"
                        }
                        .to_string(),
                    ),
                ],
            ),
        );
    }
    if snapshot.is_quick_projectless {
        return None;
    }
    let project_path = snapshot
        .in_memory_project_path
        .as_ref()?
        .to_string_lossy()
        .to_string();
    let surface_id = snapshot.surface_ids.automate_board_id.as_ref()?.clone();
    let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_KANBAN_URL", "kanban.html").ok()?;
    ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(append_url_query_params(
        base_url,
        &[
            ("projectName", snapshot.display_name.clone()),
            ("projectPath", project_path),
            ("projectId", active_project_id),
            ("projectEditorId", surface_id),
            ("beadsDisplayKey", snapshot.display_name.clone()),
            ("surface", "automations".to_string()),
            ("automationExperimental", "false".to_string()),
            (
                "showBetaFeatures",
                if runtime_settings.show_beta_features {
                    "true"
                } else {
                    "false"
                }
                .to_string(),
            ),
        ],
    ))
}

