// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds sidebar HUD hydration, gxserver app user-data reads, and
// scratch-pad/pinned-prompt persistence helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_sidebar_hud_from_gxserver(
    timeout: Duration,
    active_project_id: Option<&str>,
) -> Result<GpuiSidebarHudButtons, String> {
    /*
    CDXC:SidebarHudContract 2026-06-24-20:34:
    GPUI Settings/app-modal and titlebar reads must use gxserver's normalized sidebar HUD contract instead of recreating custom agent/action projection in host Rust. If the endpoint is unavailable, callers leave those read rows empty rather than falling back to a second custom metadata normalizer.
    */
    let mut params = serde_json::Map::new();
    if let Some(active_project_id) =
        active_project_id.and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
    {
        params.insert(
            "activeProjectId".to_string(),
            serde_json::Value::String(active_project_id.to_string()),
        );
    }
    let result = gpui_gxserver_rpc_result(
        "/api/readSidebarHud",
        &serde_json::Value::Object(params),
        timeout,
    )?;
    Ok(GpuiSidebarHudButtons {
        agents: gpui_sidebar_hud_array_field(&result, "agents")?,
        commands: gpui_sidebar_hud_array_field(&result, "commands")?,
        /*
        A gxserver older than this app omits globalCommands entirely, so treat a
        missing list as empty rather than failing the whole HUD read and blanking
        Actions that do exist.
        */
        global_commands: gpui_sidebar_hud_array_field(&result, "globalCommands")
            .unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
    })
}

#[allow(dead_code)] // no caller: gxserver project settings are persisted through the sidebar runtime bridge instead
pub(crate) fn gpui_persist_sidebar_agents_to_gxserver_projects(
    domain_projects: &[serde_json::Value],
    agents: &[GpuiStoredSidebarAgent],
    agent_order: &[String],
) -> Result<(), String> {
    let mut updated_any = false;
    for project in domain_projects
        .iter()
        .filter_map(serde_json::Value::as_object)
    {
        let Some(project_id) = gpui_trimmed_json_string_field(project, "projectId") else {
            continue;
        };
        let mut params = serde_json::Map::new();
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
        params.insert(
            "customAgents".to_string(),
            gpui_stored_sidebar_agents_value(agents),
        );
        params.insert(
            "customAgentOrder".to_string(),
            gpui_string_array_value(agent_order),
        );
        let result = gpui_gxserver_rpc_result(
            "/api/updateProject",
            &serde_json::Value::Object(params),
            Duration::from_secs(10),
        )?;
        if result
            .get("project")
            .and_then(serde_json::Value::as_object)
            .is_none()
        {
            return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string());
        }
        updated_any = true;
    }
    if updated_any {
        Ok(())
    } else {
        Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())
    }
}

pub(crate) fn gpui_read_gxserver_app_user_data(timeout: Duration) -> GpuiAppModalProductState {
    /*
    CDXC:GxserverAppUserData 2026-06-24-13:30:
    GPUI app-modal hydrate reads Scratch Pad and Pinned Prompts from gxserver's
    shared app-user-data snapshot instead of the old GPUI product-state file.
    Parse only the shared React contract fields and silently drop malformed
    prompt rows without logging note or prompt content.
    */
    gpui_gxserver_rpc_result("/api/readAppUserData", &serde_json::json!({}), timeout)
        .ok()
        .and_then(|value| gpui_app_modal_product_state_from_value(&value))
        .unwrap_or_default()
}

pub(crate) fn gpui_save_gxserver_scratch_pad(content: &str) -> Result<(), String> {
    gpui_gxserver_rpc_result(
        "/api/saveScratchPad",
        &serde_json::json!({ "content": content }),
        Duration::from_secs(5),
    )
    .map(|_| ())
}

pub(crate) fn gpui_save_gxserver_pinned_prompt(
    content: &str,
    title: &str,
    prompt_id: Option<&str>,
) -> Result<(), String> {
    let mut params = serde_json::Map::new();
    params.insert(
        "content".to_string(),
        serde_json::Value::String(content.to_string()),
    );
    params.insert(
        "title".to_string(),
        serde_json::Value::String(title.to_string()),
    );
    if let Some(prompt_id) = prompt_id {
        params.insert(
            "promptId".to_string(),
            serde_json::Value::String(prompt_id.to_string()),
        );
    }
    gpui_gxserver_rpc_result(
        "/api/savePinnedPrompt",
        &serde_json::Value::Object(params),
        Duration::from_secs(5),
    )
    .map(|_| ())
}

pub(crate) fn gpui_app_modal_product_state_from_value(
    value: &serde_json::Value,
) -> Option<GpuiAppModalProductState> {
    let object = value.as_object()?;
    Some(GpuiAppModalProductState {
        pinned_prompts: gpui_pinned_prompts_from_value(object.get("pinnedPrompts")),
        scratch_pad_content: object
            .get("scratchPadContent")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

pub(crate) fn gpui_pinned_prompts_from_value(value: Option<&serde_json::Value>) -> Vec<GpuiPinnedPrompt> {
    let Some(items) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    gpui_normalize_pinned_prompts(
        items
            .iter()
            .filter_map(gpui_pinned_prompt_from_value)
            .collect::<Vec<_>>(),
    )
}

pub(crate) fn gpui_pinned_prompt_from_value(value: &serde_json::Value) -> Option<GpuiPinnedPrompt> {
    let object = value.as_object()?;
    let prompt_id = object.get("promptId")?.as_str()?.trim().to_string();
    let content = object.get("content")?.as_str()?.to_string();
    let created_at = object.get("createdAt")?.as_str()?.to_string();
    let title = object
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let updated_at = object.get("updatedAt")?.as_str()?.to_string();
    if prompt_id.is_empty() || content.is_empty() || created_at.is_empty() || updated_at.is_empty()
    {
        return None;
    }
    Some(GpuiPinnedPrompt {
        title: gpui_normalize_pinned_prompt_title(title, &content),
        content,
        created_at,
        prompt_id,
        updated_at,
    })
}

pub(crate) fn gpui_normalize_pinned_prompts(mut prompts: Vec<GpuiPinnedPrompt>) -> Vec<GpuiPinnedPrompt> {
    prompts.retain(|prompt| {
        !prompt.prompt_id.is_empty()
            && !prompt.content.is_empty()
            && !prompt.created_at.is_empty()
            && !prompt.updated_at.is_empty()
    });
    prompts.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    prompts
}

pub(crate) fn gpui_pinned_prompt_value(prompt: &GpuiPinnedPrompt) -> serde_json::Value {
    serde_json::json!({
        "content": prompt.content.clone(),
        "createdAt": prompt.created_at.clone(),
        "promptId": prompt.prompt_id.clone(),
        "title": prompt.title.clone(),
        "updatedAt": prompt.updated_at.clone(),
    })
}

pub(crate) fn gpui_normalize_pinned_prompt_title(title: &str, content: &str) -> String {
    let trimmed_title = title.trim();
    if !trimmed_title.is_empty() {
        return trimmed_title.to_string();
    }
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(80).collect::<String>())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "Untitled Prompt".to_string())
}

