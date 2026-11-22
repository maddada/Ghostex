use super::*;

/// Rounds of the provider's composer wait a staged draft takes before it is typed anyway: two
/// minutes, at 10 seconds a round.
const GXSERVER_FIRST_USER_INPUT_DRAFT_COMPOSER_WAIT_ROUNDS: u32 = 12;

#[derive(Clone)]
pub(crate) struct FirstUserInputDraftTarget {
    draft: String,
    project_id: String,
    session_id: String,
}

/*
CDXC:Drafts 2026-08-20:
A session created with `runtimeSettings.firstUserInputDraft` gets that text
typed into its agent CLI composer once, and never sent. The claim happens
synchronously while the provider start is still being answered — the marker
flips from `pending` to `typing` before the readiness delay — so a second
provider start (wake, re-attach, restart, a concurrent attach) finds a
non-pending marker and types nothing.
*/
pub(crate) fn claim_first_user_input_draft(
    repository: &DomainRepository<'_>,
    endpoint_path: &str,
    result: &Value,
    project_id: &str,
    session_id: &str,
) -> Option<FirstUserInputDraftTarget> {
    if endpoint_path != "/api/startSessionProvider"
        || result.get("started").and_then(Value::as_bool) != Some(true)
    {
        return None;
    }
    let session = repository.get_session(project_id, session_id).ok()??;
    // An orchestrator always opens in Chat and starts by running `/ghostex-coordinator` in its
    // terminal, so its staged handoff draft stays pending for `/api/claimSessionChatLaunchDraft`
    // instead of being typed into the terminal on top of the skill send.
    if crate::coordinators::coordinator_role_is_skill(&session) {
        return None;
    }
    if read_runtime_text(&session, FIRST_USER_INPUT_DRAFT_STATUS_KEY).as_deref() != Some("pending")
    {
        return None;
    }
    let draft = read_first_user_input_draft(&session)?;
    update_first_user_input_draft_status(repository, project_id, session_id, "typing").ok()?;
    Some(FirstUserInputDraftTarget {
        draft,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    })
}

pub(crate) fn update_first_user_input_draft_status(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    status: &str,
) -> Result<(), DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(());
    };
    let mut runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.insert(FIRST_USER_INPUT_DRAFT_STATUS_KEY.to_string(), json!(status));
    runtime_settings.insert(
        FIRST_USER_INPUT_DRAFT_UPDATED_AT_KEY.to_string(),
        json!(now_iso()),
    );
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update)?;
    Ok(())
}

pub(crate) fn schedule_first_user_input_draft(state: AppState, target: FirstUserInputDraftTarget) {
    /*
    CDXC:Drafts 2026-08-20:
    Same readiness window the fork's initial rename uses: the provider start
    owns a freshly launched CLI whose composer is not accepting input yet. The
    draft then goes through zmx's text path with `submit: false`, so no Enter
    and no trailing carriage return is ever produced — the composer keeps the
    text staged for the user to write around.

    CDXC:SessionChat 2026-08-26: the readiness window is now a real
    composer wait rather than a blind four-second sleep. Staging a draft into a
    trust dialog types the user's text at it and then discards it on the next
    repaint, with nothing anywhere to recover it from.
    */
    tokio::spawn(async move {
        let server_id = state.metadata.server_id.as_str();
        // A box session's agent needs its box first; the draft never goes into boot output or
        // agentbox's sign-in prompt (agentbox/input_ready.rs).
        if crate::agentbox::is_agentbox_session_by_ids(
            &state.paths,
            server_id,
            &target.project_id,
            &target.session_id,
        ) {
            if !crate::agentbox::wait_for_box_agent_input(
                &state.paths,
                server_id,
                &target.project_id,
                &target.session_id,
            )
            .await
            {
                if let Ok(db) = open_gxserver_database(&state.paths) {
                    let repository = DomainRepository::new(&db, server_id);
                    let _ = update_first_user_input_draft_status(
                        &repository,
                        &target.project_id,
                        &target.session_id,
                        "failed",
                    );
                }
                schedule_delta_for_ids(&state, &target.project_id, &target.session_id);
                return;
            }
        } else {
            /*
            CDXC:TranscriptExport 2026-10-31 WHY:
            A prompt the CLI opens at startup (Codex's "Update available", a trust question) keeps its composer away for as long as the user leaves it there, and a draft typed at it is gone: a handoff's message box came up empty in testing. An agent whose composer can be read gets more rounds of the wait, so dismissing the prompt lets the draft through; a composer that never appears still gets the draft in the end, as before.
            */
            for _ in 0..GXSERVER_FIRST_USER_INPUT_DRAFT_COMPOSER_WAIT_ROUNDS {
                let wait = crate::session_chat_composer::wait_for_session_chat_composer_by_ids(
                    &state.paths,
                    server_id,
                    &target.project_id,
                    &target.session_id,
                    crate::session_chat_composer::SessionChatComposerWaitPolicy {
                        settle_ms: 0,
                        timeout_ms: GXSERVER_PROVIDER_COMPOSER_WAIT_TIMEOUT_MS,
                        unknown_hold_ms: GXSERVER_FIRST_USER_INPUT_DRAFT_READY_DELAY_MS,
                    },
                )
                .await;
                if !matches!(
                    wait,
                    crate::session_chat_composer::SessionChatComposerWait::NotReady(_)
                ) {
                    break;
                }
            }
        }
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        /*
        CDXC:Drafts 2026-09-02:
        The marker was claimed (`typing`) before this wait, and a Chat launch
        may have taken the draft for the chat composer in the meantime
        (`claim_first_user_input_draft_for_chat`). Re-read it here so a draft
        that already lives in the chat composer is never also typed into the
        parked terminal, where it would sit unseen and later be stashed as a
        stray Saved Prompt by the chat send.
        */
        let still_typing = repository
            .get_session(&target.project_id, &target.session_id)
            .ok()
            .flatten()
            .is_some_and(|session| {
                read_runtime_text(&session, FIRST_USER_INPUT_DRAFT_STATUS_KEY).as_deref()
                    == Some("typing")
            });
        if !still_typing {
            return;
        }
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(target.project_id.clone()));
        params.insert("sessionId".to_string(), json!(target.session_id.clone()));
        params.insert(
            "diagnosticInputSource".to_string(),
            json!("first-user-input-draft"),
        );
        params.insert("submit".to_string(), Value::Bool(false));
        params.insert("text".to_string(), Value::String(target.draft));
        let status = if dispatch_zmx_session_interaction_endpoint(
            &repository,
            "/api/sendSessionMessage",
            &params,
        )
        .is_ok()
        {
            "applied"
        } else {
            "failed"
        };
        let _ = update_first_user_input_draft_status(
            &repository,
            &target.project_id,
            &target.session_id,
            status,
        );
        schedule_delta_for_ids(&state, &target.project_id, &target.session_id);
    });
}

/*
CDXC:Drafts 2026-09-02:
The Chat-side half of the first-input draft. A session that opens straight in
Chat (Default Agent View = Chat, or a Handoff / Export conversation for a Chat
user) must find the staged text in the CHAT composer, not typed into the
terminal that Chat parks behind it. The terminal typing waits for the CLI
composer to become ready, so at the moment Chat activates the marker is still
`pending` or `typing` and the text has not reached any composer yet. Claiming
it here flips the marker to `transferred`, which the typing task honours by
doing nothing, and hands the verbatim text back for the chat composer.

Only a marker in one of those two states is claimable: `applied` means the
terminal already holds the text and the ordinary terminal → chat capture is
the right tool, and anything else means there is nothing to hand over.
*/
pub(crate) fn claim_first_user_input_draft_for_chat(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Option<String>, DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(None);
    };
    if !matches!(
        read_runtime_text(&session, FIRST_USER_INPUT_DRAFT_STATUS_KEY).as_deref(),
        Some("pending" | "typing")
    ) {
        return Ok(None);
    }
    let Some(draft) = read_first_user_input_draft(&session) else {
        return Ok(None);
    };
    update_first_user_input_draft_status(repository, project_id, session_id, "transferred")?;
    Ok(Some(draft))
}

/// `/api/claimSessionChatLaunchDraft`: the launch-time claim above as an
/// endpoint of its own. It is deliberately NOT a flag on
/// `/api/handoffSessionChatDraft`: that endpoint's fallback is the
/// prompt-editor handshake against the live CLI, which must never run against
/// a CLI that is still booting, and a daemon predating this endpoint would
/// silently ignore the flag and do exactly that. An unknown endpoint fails
/// closed instead.
pub(crate) fn handle_claim_session_chat_launch_draft_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match crate::domain::read_domain_rpc_params(body) {
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
                message: "claimSessionChatLaunchDraft requires projectId and sessionId."
                    .to_string(),
            },
        );
    }
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("SQLite gxserver state error: {error}"),
                },
            );
        }
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let content = match claim_first_user_input_draft_for_chat(&repository, &project_id, &session_id)
    {
        Ok(content) => content.unwrap_or_default(),
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    if !content.is_empty() {
        schedule_delta_for_ids(state, &project_id, &session_id);
    }
    routed_json(
        Some(endpoint_path),
        axum::http::StatusCode::OK,
        crate::protocol::rpc_success(
            request_id,
            json!({ "content": content, "transferred": !content.is_empty() }),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::FIRST_USER_INPUT_DRAFT_KEY;
    use crate::paths::get_gxserver_paths;
    use crate::storage::{initialize_gxserver_storage, open_gxserver_database};

    fn test_db() -> (tempfile::TempDir, crate::storage::GxserverDatabase) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        (temp, db)
    }

    #[test]
    fn skill_coordinator_keeps_handoff_draft_pending_for_chat() {
        let (_temp, db) = test_db();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({
                    "name": "Ghostex",
                    "path": std::env::temp_dir(),
                })
                .as_object()
                .expect("project params"),
            )
            .expect("project created");
        let project_id = project["projectId"].as_str().expect("projectId");
        let draft_text =
            "You are continuing from \"Coordinator\" in Ghostex.\n\n[Coordinator Handoff](/tmp/export.md)\n\n";
        let session = repository
            .create_session(
                json!({
                    "agentId": "claude",
                    "kind": "agent",
                    "lifecycleState": "running",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "coordinatorRole": "skill",
                        FIRST_USER_INPUT_DRAFT_KEY: draft_text,
                        FIRST_USER_INPUT_DRAFT_STATUS_KEY: "pending",
                    },
                    "surface": "workspace",
                    "title": "Coordinator",
                })
                .as_object()
                .expect("session params"),
                true,
            )
            .expect("session created");
        let session_id = session["sessionId"].as_str().expect("sessionId");

        assert!(claim_first_user_input_draft(
            &repository,
            "/api/startSessionProvider",
            &json!({ "started": true }),
            project_id,
            session_id,
        )
        .is_none());
        let reloaded = repository
            .get_session(project_id, session_id)
            .expect("get session")
            .expect("session exists");
        assert_eq!(
            read_runtime_text(&reloaded, FIRST_USER_INPUT_DRAFT_STATUS_KEY).as_deref(),
            Some("pending")
        );

        let claimed = claim_first_user_input_draft_for_chat(&repository, project_id, session_id)
            .expect("claim for chat");
        assert_eq!(claimed.as_deref(), Some(draft_text));
        let after_chat_claim = repository
            .get_session(project_id, session_id)
            .expect("get session")
            .expect("session exists");
        assert_eq!(
            read_runtime_text(&after_chat_claim, FIRST_USER_INPUT_DRAFT_STATUS_KEY).as_deref(),
            Some("transferred")
        );
    }

    #[test]
    fn plain_agent_claims_first_user_input_draft_on_provider_start() {
        let (_temp, db) = test_db();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({
                    "name": "Ghostex",
                    "path": std::env::temp_dir(),
                })
                .as_object()
                .expect("project params"),
            )
            .expect("project created");
        let project_id = project["projectId"].as_str().expect("projectId");
        let draft_text = "You are continuing from \"Session\" in Ghostex.\n\n[Session Handoff](/tmp/export.md)\n\n";
        let session = repository
            .create_session(
                json!({
                    "agentId": "claude",
                    "kind": "agent",
                    "lifecycleState": "running",
                    "projectId": project_id,
                    "runtimeSettings": {
                        FIRST_USER_INPUT_DRAFT_KEY: draft_text,
                        FIRST_USER_INPUT_DRAFT_STATUS_KEY: "pending",
                    },
                    "surface": "workspace",
                    "title": "Claude",
                })
                .as_object()
                .expect("session params"),
                true,
            )
            .expect("session created");
        let session_id = session["sessionId"].as_str().expect("sessionId");

        let claimed = claim_first_user_input_draft(
            &repository,
            "/api/startSessionProvider",
            &json!({ "started": true }),
            project_id,
            session_id,
        )
        .expect("claimed on start");
        assert_eq!(claimed.draft, draft_text);
        let reloaded = repository
            .get_session(project_id, session_id)
            .expect("get session")
            .expect("session exists");
        assert_eq!(
            read_runtime_text(&reloaded, FIRST_USER_INPUT_DRAFT_STATUS_KEY).as_deref(),
            Some("typing")
        );
    }
}
