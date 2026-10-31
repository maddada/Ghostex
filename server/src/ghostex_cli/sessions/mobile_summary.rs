use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::Flags;
use crate::ghostex_cli::output::is_failed_cli_result;
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::selector;

use super::*;

// ---------------------------------------------------------------------------
// fetchSessionList + mobile summary contract
// ---------------------------------------------------------------------------

/// fetchSessionList(flags, options): returns the CLI session objects
/// (toCliSession shape). The bool mirrors the Node CLI's only option,
/// `writeCache` (refresh the session-alias cache in Ghostex state storage).
pub fn fetch_session_list(flags: &Flags, write_cache: bool) -> CliResult<Vec<Value>> {
    let result = fetch_session_list_result(flags, write_cache)?;
    Ok(result
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// Full fetchSessionList result object (`{ ...listSessions result, sessions }`).
pub(super) fn fetch_session_list_result(flags: &Flags, write_cache: bool) -> CliResult<Value> {
    let result = fetch_gxserver_session_list(flags)?;
    /*
     * CDXC:Mobile 2026-06-11-23:52:
     * `ghostex sessions --json` is the React Native Android reconnect/status
     * contract. The inventory must come from gxserver list/snapshot APIs and
     * must not read the retired macOS sidebar persistence file when the daemon
     * is unreachable.
     */
    if is_failed_cli_result(&result) {
        let message = match result.get("error") {
            Some(value) if !value.is_null() => js_display(value),
            _ => "Could not list Ghostex sessions.".to_string(),
        };
        return Err(CliError::Other(message));
    }
    let sessions = result
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if write_cache {
        let mut cache = Map::new();
        cache.insert(
            "createdAt".to_string(),
            json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        );
        insert_present(&mut cache, "revision", result.get("revision"));
        cache.insert("sessions".to_string(), Value::Array(sessions.clone()));
        selector::write_session_alias_cache(&Value::Object(cache))?;
    }
    let mut merged = result.as_object().cloned().unwrap_or_default();
    merged.insert("sessions".to_string(), Value::Array(sessions));
    Ok(Value::Object(merged))
}

/// `settings` is the computer's settings file, which carries the two session-links choices the
/// phone follows; the caller reads it, so this stays a pure reshaping of `result`.
pub(super) fn to_mobile_session_list(result: &Value, settings: &Map<String, Value>) -> Value {
    /*
     * CDXC:Mobile 2026-06-30-04:37:
     * The mobile summary JSON keeps only row/action identity fields so the
     * phone does less network transfer, JSON parsing, and SwiftUI diff work
     * than the full diagnostic `sessions --json` contract.
     *
     * CDXC:Projects 2026-06-30-21:23:
     * Mobile summaries must preserve gxserver's active-project filter even if
     * a future caller passes unfiltered inventory into this compactor.
     */
    let projects: Option<Vec<&Value>> =
        result
            .get("projects")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter(|project| is_active_gxserver_inventory_project(project))
                    .collect()
            });
    let active_project_ids: Option<std::collections::HashSet<String>> =
        projects.as_ref().map(|list| {
            list.iter()
                .filter_map(|project| project.get("projectId"))
                .filter(|value| js_truthy(Some(value)))
                .map(|value| value.to_string())
                .collect()
        });
    let sessions: Vec<&Value> = result
        .get("sessions")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter(|session| match &active_project_ids {
                    None => true,
                    Some(ids) => session
                        .get("projectId")
                        .map(|value| ids.contains(&value.to_string()))
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    /*
     * CDXC:Sessions 2026-07-12-00:00:
     * Mobile rows must render in the same order as the GPUI sidebar: pre-sort
     * by gxserver's presentation snapshot order (`sortOrder`) and forward the
     * named-group overlay (`workspaceGroups`).
     */
    let mut ordered_sessions = sessions;
    ordered_sessions.sort_by(|left, right| {
        mobile_sort_order(left)
            .partial_cmp(&mobile_sort_order(right))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut map = Map::new();
    insert_present(&mut map, "capabilities", result.get("capabilities"));
    insert_present(&mut map, "fallback", result.get("fallback"));
    map.insert(
        "ok".to_string(),
        json!(result.get("ok") != Some(&Value::Bool(false))),
    );
    insert_present(&mut map, "product", result.get("product"));
    // CDXC:SessionLinks 2026-10-24 DECISION: User: "user can pick to open in embedded or external browser in settings (default to embedded)". The phone has no copy of the computer's settings, so the summary carries this one: `embedded` opens a session link in the phone's own browser view, `external` in its default browser.
    // Sent only when it is not the default, which the phone assumes when the key is absent.
    if settings
        .get("sessionLinkOpenTarget")
        .and_then(Value::as_str)
        == Some("external")
    {
        map.insert("sessionLinkOpenTarget".to_string(), json!("external"));
    }
    // CDXC:SessionLinks 2026-10-27 SEE-ALSO: `SessionLinksDisplay` in packages/gx-core/src/sidebar_view/links.rs; the phone's session list draws its rows by the computer's "Session links in the sidebar" (apps/mobile/app/src/chat/native/sessionLinks.ts). Sent only when it is not the default (`badge`, since 2026-10-27).
    if let Some(display) = settings
        .get("sidebarSessionLinks")
        .and_then(Value::as_str)
        .filter(|display| *display != "badge")
    {
        map.insert("sidebarSessionLinks".to_string(), json!(display));
    }
    // CDXC:SessionSleep 2026-10-27 SEE-ALSO: the 2026-09-29 decision in apps/desktop/src/app/native_sidebar/sessions.rs; the phone's session list fades a sleeping row by the computer's Advanced > Dim sleeping sessions (apps/mobile/app/src/components/sessions/SessionRow.tsx). Sent only when on (the default is off).
    if settings.get("dimSleepingSessions").and_then(Value::as_bool) == Some(true) {
        map.insert("dimSleepingSessions".to_string(), json!(true));
    }
    // CDXC:AgentLauncher 2026-10-30 SEE-ALSO: packages/gx-core/src/sidebar_menu/agent_choice.rs; the phone remembers the agent picked on a project's agent button by the computer's Settings > Agents > Remember the agent you pick (apps/mobile/app/src/components/sessions/agentChoice.ts). Sent only when it is Per Space or Everywhere (the default is per project).
    if let Some(scope) = settings
        .get("newSessionAgentScope")
        .and_then(Value::as_str)
        .filter(|scope| matches!(*scope, "space" | "everywhere"))
    {
        map.insert("newSessionAgentScope".to_string(), json!(scope));
    }
    // CDXC:Sidebar 2026-10-30 SEE-ALSO: packages/gx-core/src/unified_sidebar/; the phone's session list follows the computer's Settings > Sidebar & Sessions > Sidebar (apps/mobile/app/src/contract/unified/). Sent only when it is not the default: `sidebarView` when Unified is picked, `unifiedWorkingSection` when its Working section is switched off, `enableSessionParking` when parking is.
    if settings.get("sidebarView").and_then(Value::as_str) == Some("unified") {
        map.insert("sidebarView".to_string(), json!("unified"));
    }
    if settings
        .get("unifiedWorkingSection")
        .and_then(Value::as_bool)
        == Some(false)
    {
        map.insert("unifiedWorkingSection".to_string(), json!(false));
    }
    // Its Parked shelf shows only while session parking is on (the default).
    if settings
        .get("enableSessionParking")
        .and_then(Value::as_bool)
        == Some(false)
    {
        map.insert("enableSessionParking".to_string(), json!(false));
    }
    if let Some(projects) = &projects {
        map.insert(
            "projects".to_string(),
            Value::Array(
                projects
                    .iter()
                    .map(|project| {
                        let mut project_map = Map::new();
                        insert_present(&mut project_map, "name", project.get("name"));
                        insert_present(&mut project_map, "path", project.get("path"));
                        insert_present(&mut project_map, "projectId", project.get("projectId"));
                        // The Unified list names a project by its repository, so one repository on two computers is one project (packages/gx-core/src/unified_sidebar/repo.rs).
                        insert_present(
                            &mut project_map,
                            "gitRemoteOriginUrl",
                            project.get("gitRemoteOriginUrl"),
                        );
                        /*
                         * CDXC:Icons 2026-08-21:
                         * The phone's sessions list ranks project icons exactly
                         * like SidebarV2ProjectIcon does — user image, then the
                         * icon the repository ships, then a typed glyph — so it
                         * needs all three inputs plus the workspace theme color
                         * the branched project rail is drawn from.
                         */
                        let identity_icon = project.get("identityIcon");
                        insert_present(
                            &mut project_map,
                            "icon",
                            identity_icon.and_then(|icon| icon.get("icon")),
                        );
                        insert_present(
                            &mut project_map,
                            "iconDataUrl",
                            identity_icon.and_then(|icon| icon.get("iconDataUrl")),
                        );
                        insert_present(
                            &mut project_map,
                            "themeColor",
                            identity_icon.and_then(|icon| icon.get("themeColor")),
                        );
                        insert_present(
                            &mut project_map,
                            "discoveredIconDataUrl",
                            project.get("discoveredIconDataUrl"),
                        );
                        insert_present(
                            &mut project_map,
                            "worktree",
                            project
                                .get("worktreeJson")
                                .or_else(|| project.get("worktree")),
                        );
                        // Issues Workflow's parts, folded in from the daemon's snapshot (inventory.rs) under the snapshot's own keys: `workMode` (on), `workLinear`, `workGithubOff` and `workTracker`, which pick the phone's Link to rows.
                        if project.get("workMode").and_then(Value::as_bool) == Some(true) {
                            project_map.insert("workMode".to_string(), json!(true));
                            for key in ["workLinear", "workGithubOff"] {
                                if project.get(key).and_then(Value::as_bool) == Some(true) {
                                    project_map.insert(key.to_string(), json!(true));
                                }
                            }
                            insert_present(
                                &mut project_map,
                                "workTracker",
                                project.get("workTracker"),
                            );
                        }
                        // CDXC:Workspaces 2026-10-09 SEE-ALSO: the phone filters by these exactly as gx-core `WindowWorkspace::shows_project` does (apps/mobile/app/src/workspaces/workspaceFilter.ts); sent under the presentation snapshot's own keys and rules.
                        if crate::workspaces::workspaces_feature_enabled() {
                            if let Some(workspace_id) =
                                crate::workspaces::stored_project_workspace_id(project)
                            {
                                project_map.insert("workspaceId".to_string(), json!(workspace_id));
                            }
                            if crate::workspaces::project_in_every_workspace(project) {
                                project_map.insert("everyWorkspace".to_string(), json!(true));
                            }
                        }
                        Value::Object(project_map)
                    })
                    .collect(),
            ),
        );
    }
    let recent_projects = result
        .get("recentProjects")
        .and_then(Value::as_array)
        .map(|projects| {
            projects
                .iter()
                .filter_map(|project| {
                    let project_id = project.get("projectId")?.as_str()?.trim();
                    if project_id.is_empty() {
                        return None;
                    }
                    let mut recent = Map::new();
                    insert_present(&mut recent, "path", project.get("path"));
                    recent.insert("projectId".to_string(), json!(project_id));
                    insert_present(&mut recent, "recentClosedAt", project.get("recentClosedAt"));
                    insert_present(&mut recent, "sessionCount", project.get("sessionCount"));
                    insert_present(&mut recent, "title", project.get("title"));
                    Some(Value::Object(recent))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    map.insert("recentProjects".to_string(), Value::Array(recent_projects));
    insert_present(&mut map, "revision", result.get("revision"));
    map.insert(
        "sessions".to_string(),
        Value::Array(
            ordered_sessions
                .iter()
                .map(|session| to_mobile_session_summary(session))
                .collect(),
        ),
    );
    if let Some(collections) =
        to_mobile_sidebar_project_collections(result.get("sidebarProjectCollections"))
    {
        map.insert("sidebarProjectCollections".to_string(), collections);
    }
    if let Some(spaces) = to_mobile_sidebar_spaces(result.get("sidebarSpaces")) {
        map.insert("sidebarSpaces".to_string(), spaces);
    }
    if let Some(tags) = to_mobile_custom_session_tags(result.get("customSessionTags")) {
        map.insert("customSessionTags".to_string(), tags);
    }
    if let Some(groups) = to_mobile_workspace_groups(result.get("workspaceGroups")) {
        map.insert("workspaceGroups".to_string(), groups);
    }
    // Already normalized by gxserver (`SidebarWorkspacesState`); the phone re-narrows it on parse.
    if crate::workspaces::workspaces_feature_enabled() {
        insert_present(
            &mut map,
            "sidebarWorkspaces",
            result.get("sidebarWorkspaces"),
        );
    }
    Value::Object(map)
}

/// `builtInFeatures`: the ids of this computer's whole-feature built-in extensions that are on,
/// and `builtInFeaturesOff` the ones that are off.
///
/// CDXC:VoiceMode 2026-10-21 WHY: the phone reads no settings of the computer, and Read aloud is a built-in extension of the computer the chat lives on, so the phone learns its switch here, in the one poll it already makes per computer. One generic list serves every future built-in feature; a computer too old to send it means every one is off. The rule is the settings catalog's (`enabled_features_with`), so the phone, the desktop and `ghostex settings` agree.
/// SEE-ALSO: packages/settings-catalog/src/data/official_extensions.rs (`readAloud`), docs/2026-10-20/voice-mode/read-aloud-client-contract.md.
pub(super) fn insert_built_in_features(map: &mut Map<String, Value>) {
    map.insert(
        "builtInFeatures".to_string(),
        json!(crate::ghostex_cli::built_in_extensions::enabled_built_in_features()),
    );
    // CDXC:VoiceMode 2026-10-25 WHY: a feature that is on by default (the voice agent) cannot be read from `builtInFeatures`, where a computer too old to know its switch leaves it out; `builtInFeaturesOff` names it only when this computer has it switched off (`disabled_features_with`).
    map.insert(
        "builtInFeaturesOff".to_string(),
        json!(crate::ghostex_cli::built_in_extensions::disabled_built_in_features()),
    );
}

fn mobile_sort_order(session: &Value) -> f64 {
    const MAX_SAFE_INTEGER: f64 = 9007199254740991.0;
    session
        .get("sortOrder")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(MAX_SAFE_INTEGER)
}

pub(super) fn to_mobile_workspace_groups(workspace_groups: Option<&Value>) -> Option<Value> {
    let object = workspace_groups?.as_object()?;
    let project_order: Vec<Value> = object
        .get("projectOrder")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter(|value| matches!(value, Value::String(text) if !text.is_empty()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mut projects = Map::new();
    if let Some(project_map) = object.get("projects").and_then(Value::as_object) {
        for (project_id, project_groups) in project_map {
            let mut groups: Vec<Value> = Vec::new();
            if let Some(list) = project_groups.get("groups").and_then(Value::as_array) {
                for group in list {
                    let group_id = match group.get("groupId") {
                        Some(Value::String(text)) if !text.is_empty() => text,
                        _ => continue,
                    };
                    let session_ids: Vec<Value> = group
                        .get("sessionIds")
                        .and_then(Value::as_array)
                        .map(|ids| {
                            ids.iter()
                                .filter(|value| {
                                    matches!(value, Value::String(text) if !text.is_empty())
                                })
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    let title = match group.get("title") {
                        Some(Value::String(text)) if !text.is_empty() => text.clone(),
                        _ => group_id.clone(),
                    };
                    groups.push(json!({
                        "groupId": group_id,
                        "sessionIds": session_ids,
                        "title": title,
                    }));
                }
            }
            if !groups.is_empty() {
                projects.insert(project_id.clone(), json!({ "groups": groups }));
            }
        }
    }
    if project_order.is_empty() && projects.is_empty() {
        return None;
    }
    Some(json!({ "projectOrder": project_order, "projects": projects }))
}

pub(super) fn to_mobile_sidebar_project_collections(
    collections_state: Option<&Value>,
) -> Option<Value> {
    /*
     * CDXC:Projects 2026-07-18-00:00:
     * Mobile keeps the server-normalized {order, collections} contract but
     * re-sanitizes rows because fallback caches may carry stale shapes. Empty
     * overlays collapse to an absent key so phones can cheaply skip rendering.
     */
    let object = collections_state?.as_object()?;
    let mut collections = Map::new();
    if let Some(entries) = object.get("collections").and_then(Value::as_object) {
        for (collection_id, collection) in entries {
            if collection_id.is_empty() {
                continue;
            }
            let project_ids: Vec<Value> = collection
                .get("projectIds")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter(|value| matches!(value, Value::String(text) if !text.is_empty()))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            if project_ids.is_empty() {
                continue;
            }
            let title = match collection.get("title") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => collection_id.clone(),
            };
            let color = match collection.get("color") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => "transparent".to_string(),
            };
            collections.insert(
                collection_id.clone(),
                json!({
                    "collectionId": collection_id,
                    "color": color,
                    "projectIds": project_ids,
                    "title": title,
                }),
            );
        }
    }
    if collections.is_empty() {
        return None;
    }
    let mut order: Vec<Value> = Vec::new();
    let mut seen_order_ids = std::collections::HashSet::new();
    if let Some(entries) = object.get("order").and_then(Value::as_array) {
        for entry in entries {
            let Some(id) = entry.as_str() else { continue };
            if collections.contains_key(id) && seen_order_ids.insert(id.to_string()) {
                order.push(Value::String(id.to_string()));
            }
        }
    }
    for collection_id in collections.keys() {
        if seen_order_ids.insert(collection_id.clone()) {
            order.push(Value::String(collection_id.clone()));
        }
    }
    let next_collection_number = object
        .get("nextCollectionNumber")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 1)
        .unwrap_or((collections.len() as i64) + 1);
    Some(json!({
        "collections": collections,
        "nextCollectionNumber": next_collection_number,
        "order": order,
    }))
}

fn to_mobile_sidebar_spaces(spaces_state: Option<&Value>) -> Option<Value> {
    /*
     * CDXC:Spaces 2026-08-27:
     * Mobile keeps the server-normalized {order, spaces} contract but
     * re-sanitizes rows because fallback caches may carry stale shapes. A Space
     * with no members is kept — it is a real, selectable, still-empty filter —
     * so only a document with no Spaces at all collapses to an absent key.
     */
    let object = spaces_state?.as_object()?;
    let mut spaces = Map::new();
    if let Some(entries) = object.get("spaces").and_then(Value::as_object) {
        for (space_id, space) in entries {
            if space_id.is_empty() {
                continue;
            }
            let member_ids = |key: &str| -> Vec<Value> {
                space
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|ids| {
                        ids.iter()
                            .filter(
                                |value| matches!(value, Value::String(text) if !text.is_empty()),
                            )
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let name = match space.get("name") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => space_id.clone(),
            };
            let icon = match space.get("icon") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => "stack".to_string(),
            };
            let color = match space.get("color") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => "#4f5663".to_string(),
            };
            let mut row = json!({
                "color": color,
                "icon": icon,
                "memberCollectionIds": member_ids("memberCollectionIds"),
                "memberProjectIds": member_ids("memberProjectIds"),
                "name": name,
                "spaceId": space_id,
            });
            // The workspace the Space belongs to; absent = the default workspace.
            if let Some(Value::String(workspace_id)) = space
                .get("workspaceId")
                .filter(|_| crate::workspaces::workspaces_feature_enabled())
            {
                if !workspace_id.is_empty() {
                    row["workspaceId"] = json!(workspace_id);
                }
            }
            spaces.insert(space_id.clone(), row);
        }
    }
    if spaces.is_empty() {
        return None;
    }
    let mut order: Vec<Value> = Vec::new();
    let mut seen_order_ids = std::collections::HashSet::new();
    if let Some(entries) = object.get("order").and_then(Value::as_array) {
        for entry in entries {
            let Some(id) = entry.as_str() else { continue };
            if spaces.contains_key(id) && seen_order_ids.insert(id.to_string()) {
                order.push(Value::String(id.to_string()));
            }
        }
    }
    for space_id in spaces.keys() {
        if seen_order_ids.insert(space_id.clone()) {
            order.push(Value::String(space_id.clone()));
        }
    }
    Some(json!({
        "order": order,
        "spaces": spaces,
    }))
}

fn to_mobile_custom_session_tags(tags_state: Option<&Value>) -> Option<Value> {
    /*
     * CDXC:Sessions 2026-09-11 WHY:
     * Mobile keeps the server-normalized {order, tags} contract but re-sanitizes
     * rows because fallback caches may carry stale shapes, exactly like the
     * Spaces compactor above it. An empty catalog collapses to an absent key.
     */
    let object = tags_state?.as_object()?;
    let mut tags = Map::new();
    if let Some(entries) = object.get("tags").and_then(Value::as_object) {
        for (tag_id, tag) in entries {
            if !crate::custom_session_tags::is_custom_session_tag_id(tag_id) {
                continue;
            }
            let name = match tag.get("name") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => "Tag".to_string(),
            };
            let icon = match tag.get("icon") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => "sparkles".to_string(),
            };
            let color = match tag.get("color") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => "#f3cc5f".to_string(),
            };
            tags.insert(
                tag_id.clone(),
                json!({
                    "color": color,
                    "icon": icon,
                    "name": name,
                    "tagId": tag_id,
                }),
            );
        }
    }
    if tags.is_empty() {
        return None;
    }
    let mut order: Vec<Value> = Vec::new();
    let mut seen_order_ids = std::collections::HashSet::new();
    if let Some(entries) = object.get("order").and_then(Value::as_array) {
        for entry in entries {
            let Some(id) = entry.as_str() else { continue };
            if tags.contains_key(id) && seen_order_ids.insert(id.to_string()) {
                order.push(Value::String(id.to_string()));
            }
        }
    }
    for tag_id in tags.keys() {
        if seen_order_ids.insert(tag_id.clone()) {
            order.push(Value::String(tag_id.clone()));
        }
    }
    Some(json!({
        "order": order,
        "tags": tags,
    }))
}

fn to_mobile_session_summary(session: &Value) -> Value {
    let s = |key: &str| session.get(key);
    let mut map = Map::new();
    // CDXC:Drafts 2026-09-15 SEE-ALSO: packages/shared/session-drafts.ts (deleted 2026-10-01) and apps/mobile/app/src/contract/sessionDrafts.ts need the draft marker, composer-text flag and creation clock to move unsent sessions into Drafts after 10 minutes.
    insert_js(&mut map, "createdAt", &[s("createdAt")]);
    insert_js(&mut map, "isDraft", &[s("isDraft")]);
    insert_js(&mut map, "hasComposerDraft", &[s("hasComposerDraft")]);
    insert_js(&mut map, "activity", &[s("activity")]);
    // CDXC:SessionStatus 2026-10-14 SEE-ALSO: the session's OSC 7501 records (server/src/program_status/presentation.rs), for a phone that wants the program's own message.
    insert_non_null(&mut map, "programStatus", s("programStatus"));
    insert_js(&mut map, "agent", &[s("agent"), s("agentId")]);
    insert_js(&mut map, "agentIcon", &[s("agentIcon")]);
    // CDXC:AgentBox 2026-10-01 SEE-ALSO: the box a session runs in (`PresentationAgentbox`); the phone keeps a started box session on its terminal like gx-core's `session_chat_view_unavailable`.
    insert_non_null(&mut map, "agentbox", s("agentbox"));
    insert_js(&mut map, "agentName", &[s("agentName")]);
    insert_js(&mut map, "alias", &[s("alias")]);
    insert_js(&mut map, "displayTitle", &[s("displayTitle"), s("title")]);
    insert_js(&mut map, "groupId", &[s("groupId")]);
    insert_js(&mut map, "isFavorite", &[s("isFavorite")]);
    // CDXC:Sessions 2026-09-11 WHY: the summary forwarded only isFavorite, so the phone could never show a non-Favorite tag; sessionTag is the marker every other surface renders.
    insert_js(&mut map, "sessionTag", &[s("sessionTag")]);
    insert_js(&mut map, "isFocused", &[s("isFocused")]);
    insert_js(&mut map, "isLive", &[s("isLive")]);
    insert_js(&mut map, "isParked", &[s("isParked")]);
    insert_js(&mut map, "isPinned", &[s("isPinned")]);
    insert_js(&mut map, "isSleeping", &[s("isSleeping")]);
    insert_js(&mut map, "kind", &[s("kind")]);
    insert_js(
        &mut map,
        "lastInteractionAt",
        &[s("lastInteractionAt"), s("lastActiveAt"), s("updatedAt")],
    );
    insert_js(&mut map, "nativePaneState", &[s("nativePaneState")]);
    insert_js(&mut map, "projectId", &[s("projectId")]);
    insert_js(&mut map, "projectName", &[s("projectName")]);
    insert_js(&mut map, "projectPath", &[s("projectPath")]);
    insert_js(&mut map, "provider", &[s("provider")]);
    insert_js(
        &mut map,
        "providerSessionName",
        &[s("providerSessionName"), s("sessionPersistenceName")],
    );
    insert_js(
        &mut map,
        "providerSessionState",
        &[s("providerSessionState")],
    );
    insert_js(&mut map, "sessionId", &[s("sessionId")]);
    insert_js(
        &mut map,
        "shouldSubmitStagedFirstPromptTitleCommand",
        &[s("shouldSubmitStagedFirstPromptTitleCommand")],
    );
    /*
     * CDXC:StateSync 2026-07-29-00:00:
     * The settle/snooze lifecycle rides the one poll mobile already makes, so
     * the phone can render the same settled/snoozed shelves as the desktop
     * inbox without a second round trip. Absent keys mean "no lifecycle state".
     */
    insert_js(&mut map, "settledAt", &[s("settledAt")]);
    insert_js(&mut map, "settledOverride", &[s("settledOverride")]);
    insert_js(&mut map, "snoozedAt", &[s("snoozedAt")]);
    insert_js(&mut map, "snoozedUntil", &[s("snoozedUntil")]);
    /*
     * CDXC:Git 2026-07-29-00:00:
     * The card row's git/PR state rides the same poll mobile already makes, so
     * the phone can render branch, +n −n, and the PR badge without a second
     * round trip or a git binary of its own.
     */
    insert_js(&mut map, "gitStatus", &[s("gitStatus")]);
    // CDXC:WorkMode 2026-10-09 WHY: the same second-whitelist trap as the keys around it: the phone's Copy submenu needs the session's PR, Linear and issue links, and `to_cli_session` forwarding `work` is not enough because this compactor drops everything it does not name.
    insert_non_null(&mut map, "work", s("work"));
    // CDXC:SessionLinks 2026-10-24 WHY: the same second-whitelist trap: the phone's chat strip and its links sheet read the session's links from here.
    insert_non_null(&mut map, "links", s("links"));
    // CDXC:ClaudeCloud 2026-10-23 WHY: the same second-whitelist trap: the phone's row needs the Claude cloud record (state, claude.ai link) to mark the row and open the session's page on a tap.
    insert_non_null(&mut map, "claudeCloud", s("claudeCloud"));
    /*
     * CDXC:SessionChat 2026-08-21-b:
     * The phone's session-row queue badge reads these two. This compactor is a
     * SECOND whitelist after `to_cli_session`'s: forwarding them there only is
     * not enough, because everything not named here is dropped again before the
     * summary reaches the phone. `queuedPromptCount` includes `failed` rows and
     * `queuedPromptFailedCount` turns the badge red; both absent means no badge.
     */
    insert_js(&mut map, "queuedPromptCount", &[s("queuedPromptCount")]);
    insert_js(
        &mut map,
        "queuedPromptFailedCount",
        &[s("queuedPromptFailedCount")],
    );
    insert_js(&mut map, "hasComposerDraft", &[s("hasComposerDraft")]);
    // CDXC:SessionTitles 2026-10-12 WHY: the same second-whitelist trap: the phone shakes the row title and prefills Rename from the rename the agent never confirmed.
    insert_js(&mut map, "renameFailedTitle", &[s("renameFailedTitle")]);
    insert_js(&mut map, "renameFailedAt", &[s("renameFailedAt")]);
    /*
     * CDXC:SessionNotes 2026-08-24:
     * Same SECOND-whitelist trap as the queue counts above: forwarding these in
     * `to_cli_session` only is not enough, because everything not named here is
     * dropped again before the summary reaches the phone. `sessionNote` draws
     * the row's note dot and its note text; `agentSessionId` gates the
     * "Session note" long-press item, because a session with no provider
     * conversation has nothing to attach a note to.
     */
    insert_js(&mut map, "sessionNote", &[s("sessionNote")]);
    insert_js(&mut map, "agentSessionId", &[s("agentSessionId")]);
    // CDXC:Coordinators 2026-10-01 WHY: the same second-whitelist trap: the phone draws the coordinator crown in place of the agent logo from this field, and nests each open thread under its coordinator from the link and thread state below.
    insert_js(&mut map, "coordinatorRole", &[s("coordinatorRole")]);
    insert_js(
        &mut map,
        "coordinatorProjectId",
        &[s("coordinatorProjectId")],
    );
    insert_js(
        &mut map,
        "coordinatorSessionId",
        &[s("coordinatorSessionId")],
    );
    insert_js(
        &mut map,
        "coordinatorThreadState",
        &[s("coordinatorThreadState")],
    );
    /*
     * CDXC:DelayedSend 2026-09-03:
     * Same SECOND-whitelist trap once more: `to_cli_session` forwarded the
     * Delayed Send and Close After Done projections for a long time, but this
     * compactor never named them, so a timer armed from the desktop sidebar
     * never reached the phone's session row or its Session Automations dialog.
     * `delayedSendRemainingLabel` paints the yellow clock and the trailing
     * countdown, `delayedSendDeadlineAt` lets the row tick that countdown
     * between polls, the two `sendWhen*Active` flags preselect the dialog's
     * trigger, and `closeAfterDone` paints the pastel-red clock. All absent
     * means "no timer", which is also what a daemon without the projections
     * publishes.
     */
    insert_js(
        &mut map,
        "delayedSendRemainingLabel",
        &[s("delayedSendRemainingLabel")],
    );
    insert_js(
        &mut map,
        "delayedSendDeadlineAt",
        &[s("delayedSendDeadlineAt")],
    );
    insert_js(
        &mut map,
        "sendWhenAgentStopsActive",
        &[s("sendWhenAgentStopsActive")],
    );
    insert_js(
        &mut map,
        "sendWhenAllProjectSessionsStopActive",
        &[s("sendWhenAllProjectSessionsStopActive")],
    );
    insert_js(&mut map, "closeAfterDone", &[s("closeAfterDone")]);
    /*
     * CDXC:SessionStatus 2026-09-25 WHY:
     * Same SECOND-whitelist trap: the phone row draws the desktop row's status, so it needs the question count, the background-work marker, the Close After Done deadline it counts down from, and, while the session is in attention, the attention event the completion flash is keyed by. Each is absent when it has nothing to say (a zero count, no attention), which keeps the summary as small as before for idle rows and is also what an older daemon sends.
     */
    if s("pendingQuestionCount")
        .and_then(Value::as_u64)
        .is_some_and(|count| count > 0)
    {
        insert_js(
            &mut map,
            "pendingQuestionCount",
            &[s("pendingQuestionCount")],
        );
    }
    insert_js(
        &mut map,
        "backgroundWorkDetectedAt",
        &[s("backgroundWorkDetectedAt")],
    );
    insert_js(&mut map, "awaitingApproval", &[s("awaitingApproval")]);
    // The Unified list's working start and failed model change (CDXC:Sidebar 2026-10-30 in cli_session.rs).
    insert_js(&mut map, "workingStartedAt", &[s("workingStartedAt")]);
    insert_js(
        &mut map,
        "modelSelectionFailure",
        &[s("modelSelectionFailure")],
    );
    insert_js(
        &mut map,
        "closeAfterDoneDeadlineAt",
        &[s("closeAfterDoneDeadlineAt")],
    );
    if s("activity").and_then(Value::as_str) == Some("attention") {
        insert_js(&mut map, "attention", &[s("attention")]);
    }
    insert_js(&mut map, "sortOrder", &[s("sortOrder")]);
    insert_js(&mut map, "status", &[s("status")]);
    insert_js(&mut map, "surface", &[s("surface")]);
    insert_js(&mut map, "title", &[s("title")]);
    Value::Object(map)
}
