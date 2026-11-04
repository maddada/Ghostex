//! Sidebar-state routes: the launcher/action HUD and its Settings mutations, navigation history, the notification feed, session groups, project collections, Spaces, custom session tags and app user data.

use serde_json::{json, Map, Value};

use crate::{
    constants::GXSERVER_PROTOCOL_VERSION,
    custom_session_tags::{
        clear_session_tags_missing_from_catalog, read_custom_session_tags,
        update_custom_session_tags,
    },
    domain::DomainStateError,
    navigation_history::{navigate_history, read_navigation_history, record_navigation_visit},
    notification_feed::{
        create_notification_endpoint, read_notification_feed_endpoint,
        update_notification_feed_endpoint, NOTIFICATION_FEED_CREATE_ENDPOINT,
        NOTIFICATION_FEED_READ_ENDPOINT, NOTIFICATION_FEED_UPDATE_ENDPOINT,
    },
    presentation::increment_presentation_revision,
    sidebar_hud::{
        create_sidebar_hud_settings_mutation, read_sidebar_agent_roster, read_sidebar_hud,
        read_sidebar_hud_global_commands, GlobalSidebarCommandUpdate,
    },
    sidebar_project_collections::{
        assign_project_to_sidebar_collection, read_sidebar_project_collections,
        update_sidebar_project_collections,
    },
    sidebar_spaces::{read_sidebar_spaces, update_sidebar_spaces},
    workspace_groups::{read_workspace_session_groups, update_workspace_session_groups},
};

use super::*;

pub(super) async fn route_sidebar_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/readSidebarHud" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| {
                /*
                CDXC:AgentLauncher 2026-06-24-20:34:
                GPUI Settings and SidebarApp read normalized launcher/action HUD rows through gxserver so app-modal Rust does not hand-mirror the shared TypeScript projection. The response is derived only from project domain metadata and carries no paths, project names, prompts, tokens, stdout/stderr, daemon bodies, or renderer payload authority.
                */
                // The HUD reads launcher and action settings only; the session history column is
                // left out of every project read on these routes (the mutations patch named fields).
                let projects = repository.list_projects_without_session_history()?;
                let active_project_id = params
                    .get("activeProjectId")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let mut hud = read_sidebar_hud(&projects, active_project_id);
                /*
                CDXC:AgentLauncher 2026-07-12-00:00:
                React Native Android renders agent-launcher and quick-action buttons for
                every visible project at once, so the mobile CLI transport asks
                for per-project command rows in one round trip instead of one
                readSidebarHud call per project each poll.
                */
                apply_commands_by_project_if_requested(&mut hud, &projects, params);
                /*
                CDXC:AgentLauncher 2026-08-01-16:00:
                Global Actions live in their own daemon table rather than in
                project metadata, so they are attached here instead of inside
                read_sidebar_hud, which stays a pure projection of project rows.
                Served unconditionally rather than behind an opt-in flag: the
                list is one small array with no per-project fan-out, and every
                surface that renders the tab strip needs it on first paint.
                */
                if let Some(hud) = hud.as_object_mut() {
                    hud.insert(
                        "globalCommands".to_string(),
                        read_sidebar_hud_global_commands(
                            &repository.list_global_sidebar_commands()?,
                        ),
                    );
                    /*
                    CDXC:AgentLauncher 2026-10-06 WHY:
                    Settings › Agents asks for every agent, on and off, with when each was last used. That needs a sessions-table read, so it is opt-in and the launcher's frequent HUD reads stay a pure projection of project rows.
                    */
                    if params.get("includeAgentRoster").and_then(Value::as_bool) == Some(true) {
                        hud.insert(
                            "agentRoster".to_string(),
                            read_sidebar_agent_roster(
                                &projects,
                                &repository.agent_launcher_last_used()?,
                            ),
                        );
                    }
                }
                Ok(hud)
            },
        ),
        "/api/mutateSidebarHudSettings" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                /*
                CDXC:AgentLauncher 2026-06-24-20:54:
                Settings mutation RPCs write through the production project repository after gxserver normalizes the narrow agent/action intent. Return refreshed HUD rows and updated project rows so GPUI clients do not reparse raw metadata or log command text, URLs, project names, paths, prompts, tokens, stdout/stderr, daemon bodies, or renderer payload contents.
                */
                let projects = repository.list_projects_without_session_history()?;
                let mutation = create_sidebar_hud_settings_mutation(&projects, params)?;
                let hud_active_project_id = mutation.hud_active_project_id;
                let mut item_ids = mutation.item_ids;
                /*
                CDXC:AgentLauncher 2026-08-01-16:00:
                A reorder response must echo the order the daemon actually
                stored — the sidebar treats itemIds as the confirmation for its
                optimistic reorder and falls back to an empty list without it.
                The stored order can differ from the ids the client sent, since
                the repository keeps unlisted actions instead of dropping them.
                */
                let global_command_order_requested = matches!(
                    mutation.global_command_update,
                    Some(GlobalSidebarCommandUpdate::Order { .. })
                );
                let global_command_written = mutation.global_command_update.is_some();
                let mut updated_projects = Vec::new();
                for update in mutation.updates {
                    let project = repository.update_project(&update.params)?;
                    schedule_presentation_project_delta(
                        &state,
                        db,
                        repository,
                        &update.project_id,
                        "projectUpdated",
                    )?;
                    updated_projects.push(project);
                }
                /*
                CDXC:AgentLauncher 2026-08-01-16:00:
                A Global Action write touches no project row, so it schedules no
                projectUpdated presentation delta.

                CDXC:AgentLauncher 2026-08-07:
                It announces itself with its own event instead. Only the caller
                sees the refreshed HUD this response carries; every other live
                surface learns about HUD changes from a broadcast, and none of
                them polls the HUD on a timer. The GPUI sidebar refetches it
                when a projectUpdated delta arrives, which is why a project
                Action edit reaches the row at once and a Global Action edit did
                not — the row kept the stale list until some unrelated project
                delta happened to fire. The event carries no list, because
                /api/readSidebarHud stays the single projection of it, and it
                bumps the presentation revision so snapshot pollers converge as
                well, exactly like the sidebar-collection writes below.
                */
                match mutation.global_command_update {
                    Some(GlobalSidebarCommandUpdate::Save {
                        command_id,
                        definition,
                    }) => repository.save_global_sidebar_command(&command_id, &definition)?,
                    Some(GlobalSidebarCommandUpdate::Delete { command_id }) => {
                        repository.delete_global_sidebar_command(&command_id)?
                    }
                    Some(GlobalSidebarCommandUpdate::Order { command_ids }) => {
                        repository.order_global_sidebar_commands(&command_ids)?
                    }
                    None => {}
                }
                if global_command_written {
                    let _event_sequence = lock_presentation_event_sequence(&state)?;
                    let revision = increment_presentation_revision(db)?;
                    state.event_hub.broadcast(json!({
                        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                        "revision": revision,
                        "serverId": state.metadata.server_id.clone(),
                        "type": "globalSidebarCommandsChanged",
                    }));
                }
                let projects = repository.list_projects_without_session_history()?;
                let mut hud = read_sidebar_hud(&projects, hud_active_project_id.as_deref());
                let global_commands =
                    read_sidebar_hud_global_commands(&repository.list_global_sidebar_commands()?);
                if global_command_order_requested {
                    item_ids = Some(
                        global_commands
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_object)
                            .filter_map(|command| command.get("commandId"))
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect(),
                    );
                }
                if let Some(hud) = hud.as_object_mut() {
                    hud.insert("globalCommands".to_string(), global_commands);
                }
                /*
                CDXC:Projects 2026-08-01:
                Clients that render per-project quick actions (GPUI sidebar rows)
                replace their whole HUD snapshot with this response, so the
                mutation mirrors readSidebarHud's opt-in commandsByProject block.
                Without it a Settings save would drop the per-project rows until
                the next full HUD poll.
                */
                apply_commands_by_project_if_requested(&mut hud, &projects, params);
                let mut result = Map::new();
                result.insert("hud".to_string(), hud);
                if let Some(item_ids) = item_ids {
                    result.insert(
                        "itemIds".to_string(),
                        Value::Array(item_ids.into_iter().map(Value::String).collect()),
                    );
                }
                result.insert("projects".to_string(), Value::Array(updated_projects));
                Ok(Value::Object(result))
            },
        ),
        /*
        CDXC:Navigation 2026-08-19:
        Titlebar Back/Forward is one daemon-owned trail of previously active
        sessions and projects, shared by the gpui desktop titlebar and the web
        titlebar — see `navigation_history`. These three calls carry only
        opaque routing ids plus the display titles the sidebar already renders,
        so they sit with the other sidebar-state endpoints and need no
        repository or database access.
        */
        "/api/readNavigationHistory" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, _, params, _| read_navigation_history(params),
        ),
        "/api/recordNavigationVisit" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, _, params, _| record_navigation_visit(params),
        ),
        "/api/navigateHistory" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, _, params, _| navigate_history(params),
        ),
        NOTIFICATION_FEED_READ_ENDPOINT => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, _, _| read_notification_feed_endpoint(db),
        ),
        NOTIFICATION_FEED_UPDATE_ENDPOINT => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, _| update_notification_feed_endpoint(&state, db, params),
        ),
        NOTIFICATION_FEED_CREATE_ENDPOINT => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                create_notification_endpoint(&state, repository, db, params)
            },
        ),
        "/api/updateWorkspaceSessionGroups" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, _| {
                /*
                CDXC:Sessions 2026-07-12-00:00:
                GPUI write-through-syncs its whole normalized named-group overlay
                after each local edit. Bump the presentation revision and broadcast
                a dedicated event so snapshot pollers (mobile via CLI) and live
                sidebar clients converge without re-sending session rows.
                */
                let _event_sequence = lock_presentation_event_sequence(&state)?;
                let groups = update_workspace_session_groups(db, params)?;
                let revision = increment_presentation_revision(db)?;
                state.event_hub.broadcast(json!({
                    "groups": groups.clone(),
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "revision": revision,
                    "serverId": state.metadata.server_id.clone(),
                    "type": "workspaceGroupsChanged",
                }));
                Ok(json!({ "groups": groups }))
            },
        ),
        "/api/readWorkspaceSessionGroups" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, _, _| {
                read_workspace_session_groups(db).map(|groups| json!({ "groups": groups }))
            },
        ),
        "/api/readSidebarProjectCollections" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, _, _| {
                read_sidebar_project_collections(db)
                    .map(|collections| json!({ "sidebarProjectCollections": collections }))
            },
        ),
        "/api/updateSidebarProjectCollections" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, _| {
                /*
                CDXC:Projects 2026-07-18-00:00:
                Editors write-through-sync the whole normalized project-collection
                overlay after each local edit. Bump the presentation revision and
                broadcast a dedicated event so snapshot pollers (mobile via CLI)
                and live sidebar clients converge without re-sending project rows.
                */
                let _event_sequence = lock_presentation_event_sequence(&state)?;
                let previous_collections = read_sidebar_project_collections(db)?;
                let collections = update_sidebar_project_collections(db, params)?;
                let revision = increment_presentation_revision(db)?;
                state.event_hub.broadcast(json!({
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "revision": revision,
                    "serverId": state.metadata.server_id.clone(),
                    "sidebarProjectCollections": collections.clone(),
                    "type": "sidebarProjectCollectionsChanged",
                }));
                broadcast_pruned_sidebar_spaces(&state, db, &previous_collections, &collections)?;
                Ok(json!({ "sidebarProjectCollections": collections }))
            },
        ),
        "/api/assignProjectToSidebarCollection" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project_id = resolve_sidebar_collection_project_id(repository, params)?;
                let collection_title = params
                    .get("collectionTitle")
                    .or_else(|| params.get("group"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .ok_or_else(|| {
                        DomainStateError::bad_request(
                            "group-project requires a non-empty sidebar group title.",
                        )
                    })?;
                let _event_sequence = lock_presentation_event_sequence(&state)?;
                let previous_collections = read_sidebar_project_collections(db)?;
                let collections =
                    assign_project_to_sidebar_collection(db, &project_id, collection_title)?;
                let revision = increment_presentation_revision(db)?;
                state.event_hub.broadcast(json!({
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "revision": revision,
                    "serverId": state.metadata.server_id.clone(),
                    "sidebarProjectCollections": collections.clone(),
                    "type": "sidebarProjectCollectionsChanged",
                }));
                broadcast_pruned_sidebar_spaces(&state, db, &previous_collections, &collections)?;
                Ok(json!({
                    "projectId": project_id,
                    "sidebarProjectCollections": collections,
                }))
            },
        ),
        "/api/readSidebarSpaces" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, _, _| read_sidebar_spaces(db).map(|spaces| json!({ "sidebarSpaces": spaces })),
        ),
        "/api/updateSidebarSpaces" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, _| {
                /*
                CDXC:Spaces 2026-08-27:
                Space editors write-through-sync the whole normalized Space
                document after each local edit, exactly like the project
                collections beside them. Bump the presentation revision and
                broadcast a dedicated event so snapshot pollers (mobile via CLI)
                and live sidebar clients converge without re-sending project rows.
                */
                let _event_sequence = lock_presentation_event_sequence(&state)?;
                let spaces = update_sidebar_spaces(db, params)?;
                let revision = increment_presentation_revision(db)?;
                state.event_hub.broadcast(json!({
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "revision": revision,
                    "serverId": state.metadata.server_id.clone(),
                    "sidebarSpaces": spaces.clone(),
                    "sidebarSpacesEnabled":
                        crate::sidebar_spaces::read_sidebar_spaces_enabled(&state.paths),
                    "type": "sidebarSpacesChanged",
                }));
                Ok(json!({ "sidebarSpaces": spaces }))
            },
        ),
        "/api/readCustomSessionTags" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, _, _| {
                read_custom_session_tags(db).map(|tags| json!({ "customSessionTags": tags }))
            },
        ),
        "/api/updateCustomSessionTags" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                /*
                CDXC:Sessions 2026-09-11 WHY:
                Tag editors write-through-sync the whole normalized catalog
                after each local edit, exactly like Spaces. A tag deleted from
                the catalog is cleared from every session that carried it
                inside the same sequenced write, so no client ever sees a
                session pointing at an id the daemon no longer knows; each
                cleared session then gets its ordinary presentation delta so
                sidebars drop the marker without a full snapshot reload.
                */
                let (tags, cleared) = {
                    let _event_sequence = lock_presentation_event_sequence(&state)?;
                    let tags = update_custom_session_tags(db, params)?;
                    let cleared = clear_session_tags_missing_from_catalog(db, &tags)?;
                    let revision = increment_presentation_revision(db)?;
                    state.event_hub.broadcast(json!({
                        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                        "revision": revision,
                        "serverId": state.metadata.server_id.clone(),
                        "customSessionTags": tags.clone(),
                        "type": "customSessionTagsChanged",
                    }));
                    (tags, cleared)
                };
                for (project_id, session_id) in &cleared {
                    schedule_presentation_session_delta(
                        &state, db, repository, project_id, session_id,
                    )?;
                }
                Ok(json!({ "customSessionTags": tags }))
            },
        ),
        "/api/readAppUserData" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, _, _| repository.read_app_user_data(),
        ),
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
            })
        }
    })
}
