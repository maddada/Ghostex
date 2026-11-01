use super::*;

/// `/api/previewSessionTitle`: generates a name for a session and returns it without applying it.
///
/// CDXC:SessionTitles 2026-11-01 WHY:
/// The Rename Session dialog puts the generated name into its text box so the user can still edit it before Rename, so this answers with the title instead of running the `/api/generateSessionTitle` job that renames the session and its agent thread. It is a separate path rather than a flag on that endpoint because an older gxserver would ignore the flag and rename. Empty `text` names the session from its recent prompts, as Generate Name always did. The generator is the Title Generation Agent setting, resolved as Catch me up does (`session_chat_catch_up::summary_agent`).
pub(crate) async fn handle_preview_session_title_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    match preview_session_title(state, body).await {
        Ok(title) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "title": title })),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn preview_failure(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

async fn preview_session_title(state: &AppState, body: &Value) -> Result<String, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let text_param = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id, text) = (
        text_param("projectId"),
        text_param("sessionId"),
        text_param("text"),
    );
    if project_id.is_empty() || session_id.is_empty() {
        return Err(preview_failure(
            "invalidParams",
            "previewSessionTitle requires projectId and sessionId.",
        ));
    }
    let (project_path, session) = {
        let db = open_gxserver_database(&state.paths).map_err(|error| {
            preview_failure(
                "internalError",
                format!("SQLite gxserver state error: {error}"),
            )
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = repository
            .get_session(&project_id, &session_id)?
            .ok_or_else(|| preview_failure("notFound", "The session no longer exists."))?;
        let project_path = repository
            .get_project(&project_id)?
            .and_then(|project| read_session_text(&project, "path"))
            .unwrap_or_else(|| state.paths.home_dir.to_string_lossy().to_string());
        (project_path, session)
    };
    let (source_text, source_max_length) = if text.is_empty() {
        let source = session_history_title_source(&session).ok_or_else(|| {
            preview_failure(
                "unsupported",
                "This session has no prompts to name it from yet. Type or paste text to generate a name from.",
            )
        })?;
        (source, GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MAX_LENGTH)
    } else {
        (text, GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH)
    };
    let settings = crate::session_chat_catch_up::read_settings(&state.paths);
    let (agent, command) = crate::session_chat_catch_up::summary_agent(&settings, &session)?;
    // The title job reads its generator from the session's runtime settings.
    let mut generator_session = session.clone();
    if let Some(object) = generator_session.as_object_mut() {
        let runtime = object.entry("runtimeSettings").or_insert_with(|| json!({}));
        if let Some(runtime) = runtime.as_object_mut() {
            runtime.insert("firstPromptTitleGenerationAgent".to_string(), json!(agent));
            runtime.insert(
                "firstPromptTitleGenerationCommand".to_string(),
                json!(command),
            );
        }
    }
    generate_first_prompt_session_title(
        state,
        Some(&project_path),
        &source_text,
        source_max_length,
        &generator_session,
    )
    .await
    .map_err(|message| preview_failure("internalError", message))
}
