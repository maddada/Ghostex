use super::*;

// ---------------------------------------------------------------------------
// Output file
// ---------------------------------------------------------------------------

/// `<exports dir>/<title-slug>-<sessionId8>-<yyyyMMdd-HHmmss>.md`, suffixed on
/// collision the same way saved chat attachments are.
pub(super) fn unique_export_path(
    exports_dir: &Path,
    title: &str,
    session_id: &str,
) -> Result<PathBuf, SessionTranscriptExportError> {
    fs::create_dir_all(exports_dir).map_err(|_| {
        SessionTranscriptExportError::ExportsDirectoryUnwritable {
            path: exports_dir.to_path_buf(),
        }
    })?;
    let slug = export_slug(title);
    let session_prefix = export_session_prefix(session_id);
    let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let base = if session_prefix.is_empty() {
        format!("{slug}-{stamp}")
    } else {
        format!("{slug}-{session_prefix}-{stamp}")
    };
    let first = exports_dir.join(format!("{base}.md"));
    if !first.exists() {
        return Ok(first);
    }
    for index in 2..UNIQUE_EXPORT_PATH_ATTEMPTS {
        let candidate = exports_dir.join(format!("{base}-{index}.md"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(exports_dir.join(format!("{base}-{}.md", std::process::id())))
}

fn export_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        return "session".to_string();
    }
    trimmed
        .chars()
        .take(EXPORT_FILE_NAME_SLUG_MAX_CHARS)
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn export_session_prefix(session_id: &str) -> String {
    session_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(EXPORT_SESSION_ID_PREFIX_CHARS)
        .collect::<String>()
        .to_ascii_lowercase()
}

/*
CDXC:TranscriptExport 2026-08-20:
exportSessionTranscript renders a session's agent transcript into a markdown
file under `<app data dir>/exports` and answers with its absolute path on THIS
machine. The transcript only exists where the agent runs, so every client
(gpui, web, mobile, the CLI) calls this over its per-machine RPC and a remote
session's export lands on the remote machine next to the transcript it came
from.

Failures stay structured and never degrade into a partial file: an unsupported
agent, a session that has not reported an agent session id yet, and a
transcript with nothing to render each surface their own error code, because a
half transcript handed to the next agent is worse than a clear refusal.
*/
pub(crate) async fn handle_export_session_transcript_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if project_id.is_empty() || session_id.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "exportSessionTranscript requires projectId and sessionId.".to_string(),
            },
        );
    }

    /*
    CDXC:TranscriptExport 2026-08-24:
    The export dialog's include-toggles arrive as three optional booleans.
    Absent params keep the historical default selection (commands and patches
    in, reasoning out), so older clients and the CLI export exactly what they
    always did.
    */
    let include_commands = params
        .get("includeCommands")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let include_patches = params
        .get("includePatches")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let include_reasoning = params
        .get("includeReasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    /*
    CDXC:TranscriptExport 2026-10-25 WHY:
    A handoff can start the follow-up session on ANOTHER computer, where this machine's
    path means nothing. `includeMarkdown` answers the file's text as well, so the client can
    carry it to the target machine (`saveSessionChatAttachment` there) without a
    machine-to-machine path. Absent, the answer is unchanged.
    */
    let include_markdown = params
        .get("includeMarkdown")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let resolved = (|| {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = repository
            .get_session(&project_id, &session_id)?
            .ok_or_else(|| DomainStateError {
                code: "notFound",
                message: "The session no longer exists.".to_string(),
            })?;
        let coordinator = crate::coordinators::read_coordinator(&db, &project_id, &session_id)
            .ok()
            .flatten();
        Ok::<_, DomainStateError>((
            session_chat_agent_for_session(&session),
            read_runtime_text(&session, "agentSessionId"),
            read_runtime_text(&session, "agentSessionPath"),
            read_session_text(&session, "title"),
            coordinator,
        ))
    })();
    let (agent, agent_session_id, agent_session_path, title, coordinator) = match resolved {
        Ok(resolved) => resolved,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };

    // Parsing a long transcript and writing the export are both blocking file
    // work, exactly like the readSessionChat tail read.
    let response_agent = agent.clone();
    let exports_dir = state.paths.app_data_dir.join("exports");
    let export_session_id = session_id.clone();
    let selection =
        crate::session_transcript_export::SessionTranscriptExportSelection::from_include_toggles(
            include_commands,
            include_patches,
            include_reasoning,
        );
    let exported = match tokio::task::spawn_blocking(move || {
        crate::session_transcript_export::export_session_transcript(
            &crate::session_transcript_export::SessionTranscriptExportRequest {
                agent: agent.as_deref(),
                agent_session_id: agent_session_id.as_deref(),
                agent_session_path: agent_session_path.as_deref(),
                session_id: &export_session_id,
                session_title: title.as_deref(),
                exports_dir: &exports_dir,
                selection: Some(&selection),
            },
        )
    })
    .await
    {
        Ok(exported) => exported,
        Err(_) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: "The transcript export did not finish.".to_string(),
                },
            )
        }
    };
    let outcome = match exported {
        Ok(outcome) => outcome,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: error.code(),
                    message: error.to_string(),
                },
            )
        }
    };

    let mut result = Map::new();
    result.insert("path".to_string(), json!(outcome.path.to_string_lossy()));
    result.insert("bytes".to_string(), json!(outcome.bytes));
    result.insert(
        "sourcePath".to_string(),
        json!(outcome.source_path.to_string_lossy()),
    );
    result.insert(
        "renderedEntries".to_string(),
        json!(outcome.rendered_entries),
    );
    result.insert("parsedEntries".to_string(), json!(outcome.parsed_entries));
    if let Some(agent) = response_agent.as_deref() {
        result.insert("agent".to_string(), json!(agent));
    }
    if let Some(coordinator) = coordinator {
        result.insert(
            "coordinator".to_string(),
            json!({
                "goal": coordinator.goal,
                "instructions": coordinator.instructions,
            }),
        );
    }
    if include_markdown {
        let path = outcome.path.clone();
        match tokio::task::spawn_blocking(move || fs::read_to_string(path)).await {
            Ok(Ok(markdown)) => {
                result.insert("markdown".to_string(), json!(markdown));
            }
            _ => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "internalError",
                        message: "The exported transcript could not be read back.".to_string(),
                    },
                )
            }
        }
    }
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, Value::Object(result)),
    )
}
