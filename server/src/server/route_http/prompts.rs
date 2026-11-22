//! Prompt routes: delayed sends, pinned and saved prompts with their tags, session agent notes, and prompt-history search.

use serde_json::Value;

use super::*;

pub(super) async fn route_prompts_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
        caller,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/scheduleDelayedSend" | "/api/cancelDelayedSend" | "/api/postponeDelayedSend" => {
            handle_delayed_send_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/savePinnedPrompt" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.save_pinned_prompt(params),
        ),
        "/api/saveStashedPrompt" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let result = repository.save_stashed_prompt(params)?;
                if let Some((project_id, session_id)) = result
                    .get("prompt")
                    .and_then(Value::as_object)
                    .and_then(|prompt| {
                        prompt
                            .get("projectId")
                            .and_then(Value::as_str)
                            .zip(prompt.get("sessionId").and_then(Value::as_str))
                    })
                {
                    schedule_presentation_session_delta(
                        &state, db, repository, project_id, session_id,
                    )?;
                }
                Ok(result)
            },
        ),
        "/api/listStashedPrompts" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.list_stashed_prompts(params),
        ),
        "/api/deleteStashedPrompt" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let result = repository.delete_stashed_prompt(params)?;
                if let Some((project_id, session_id)) = result
                    .get("projectId")
                    .and_then(Value::as_str)
                    .zip(result.get("sessionId").and_then(Value::as_str))
                {
                    schedule_presentation_session_delta(
                        &state, db, repository, project_id, session_id,
                    )?;
                }
                Ok(result)
            },
        ),
        /*
        CDXC:SavedPrompts 2026-08-23:
        The Saved Prompts tag catalogue is daemon-owned like the prompts
        themselves, so every client filters the same rail instead of keeping a
        private list that drifts per machine.
        */
        "/api/saveStashedPromptTag" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.save_stashed_prompt_tag(params),
        ),
        "/api/deleteStashedPromptTag" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.delete_stashed_prompt_tag(params),
        ),
        "/api/setStashedPromptTags" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.set_stashed_prompt_tags(params),
        ),
        /*
        CDXC:SessionNotes 2026-08-24:
        A saved note changes what the sidebar row renders (the note dot and the
        hover tooltip), so the save schedules the same presentation delta
        `/api/updateSession` does instead of asking every client to refetch.
        The read is a pure lookup and schedules nothing.
        */
        "/api/saveSessionAgentNote" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let result = repository.save_session_agent_note(params)?;
                /*
                Delta targets come from the repository result (the canonical
                session-row ids), not the raw request params: a coercible
                non-string id in the body would otherwise schedule a bogus
                ("", "") delta that broadcasts `sessionRemoved` to every
                client. Same id source as the /api/createSession arm.
                */
                let project_id = result
                    .get("projectId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let session_id = result
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !project_id.is_empty() && !session_id.is_empty() {
                    schedule_presentation_session_delta(
                        &state,
                        db,
                        repository,
                        &project_id,
                        &session_id,
                    )?;
                }
                Ok(result)
            },
        ),
        "/api/readSessionAgentNote" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| repository.read_session_agent_note(params),
        ),
        "/api/searchAgentPrompts" => {
            handle_search_agent_prompts_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/readAgentPromptText" => {
            handle_read_agent_prompt_text_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/toggleAgentPromptFavorite" => {
            handle_toggle_agent_prompt_favorite_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/resolveAgentPromptLaunch" => {
            handle_resolve_agent_prompt_launch_http(&state, endpoint.path, request_id, &body_json)
        }
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
                caller,
            })
        }
    })
}
