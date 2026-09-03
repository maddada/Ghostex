use super::*;

/*
CDXC:PromptSearch 2026-08-20:
The Find surface's four RPCs. All of them go through the one warm
`ghostex_find::SearchIndex` in `agent_prompt_search`, which is the same index, favorites
file, and ranking `gx f` uses, so the GUI and the terminal picker can never
disagree about what matched or what is starred.
*/
pub(crate) fn agent_prompt_search_params(
    body: &Value,
) -> Result<Map<String, Value>, crate::agent_prompt_search::PromptSearchError> {
    match read_domain_rpc_params(body) {
        Ok(params) => Ok(params),
        Err(error) => Err(crate::agent_prompt_search::PromptSearchError {
            code: error.code,
            message: error.message,
        }),
    }
}

pub(crate) fn agent_prompt_search_response(
    endpoint_path: String,
    request_id: String,
    outcome: Result<Value, crate::agent_prompt_search::PromptSearchError>,
) -> RoutedResponse {
    match outcome {
        Ok(payload) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, payload),
        ),
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: error.code,
                message: error.message,
            },
        ),
    }
}

pub(crate) fn handle_search_agent_prompts_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = agent_prompt_search_params(body)
        .and_then(|params| crate::agent_prompt_search::search_agent_prompts(&state.paths, &params));
    agent_prompt_search_response(endpoint_path, request_id, outcome)
}

pub(crate) fn handle_read_agent_prompt_text_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = agent_prompt_search_params(body).and_then(|params| {
        crate::agent_prompt_search::read_agent_prompt_text(&state.paths, &params)
    });
    agent_prompt_search_response(endpoint_path, request_id, outcome)
}

pub(crate) fn handle_toggle_agent_prompt_favorite_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = agent_prompt_search_params(body).and_then(|params| {
        crate::agent_prompt_search::toggle_agent_prompt_favorite(&state.paths, &params)
    });
    agent_prompt_search_response(endpoint_path, request_id, outcome)
}

pub(crate) fn handle_resolve_agent_prompt_launch_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = agent_prompt_search_params(body).and_then(|params| {
        let sessions = read_all_sessions_for_prompt_launch(state)?;
        let accept_all_default = read_agent_accept_all_enabled_for_prompt_launch(state);
        crate::agent_prompt_search::resolve_agent_prompt_launch(
            &state.paths,
            &params,
            &sessions,
            accept_all_default,
        )
    });
    agent_prompt_search_response(endpoint_path, request_id, outcome)
}

/// The daemon's Accept All policy, the same value `gx f` reads before handing
/// zehn `--accept-all`. A read failure means the policy is unknown, and an
/// unknown permission policy must not silently become "bypass permissions".
pub(crate) fn read_agent_accept_all_enabled_for_prompt_launch(state: &AppState) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    crate::agents::read_agent_settings(&db)
        .ok()
        .and_then(|settings| {
            settings
                .get("agentAcceptAllEnabled")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

/// Every stored session row, so the launch resolver can decide whether a live
/// Ghostex session already owns the selected agent conversation.
pub(crate) fn read_all_sessions_for_prompt_launch(
    state: &AppState,
) -> Result<Vec<Value>, crate::agent_prompt_search::PromptSearchError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| {
        crate::agent_prompt_search::PromptSearchError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        }
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository
        .list_sessions(None)
        .map_err(|error| crate::agent_prompt_search::PromptSearchError {
            code: error.code,
            message: error.message,
        })
}
