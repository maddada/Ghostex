//! Session chat routes: reading the chat, sending, attachments and images, prompts, interrupts, rewind, model selection, drafts and the queue, the terminal tail, and transcript export.

use serde_json::Value;

use crate::{
    domain::{read_project_id, read_session_id},
    session_chat_files::{
        handle_read_session_chat_files_http, handle_read_session_chat_image_http,
        handle_save_session_chat_attachment_http, handle_save_session_chat_image_http,
    },
    session_chat_queue_runtime::handle_session_chat_queue_http,
    session_chat_read::handle_read_session_chat_http,
    session_chat_send::{
        handle_answer_session_chat_prompt_http, handle_handoff_session_chat_draft_http,
        handle_interrupt_session_chat_http, handle_replace_session_chat_draft_http,
        handle_send_session_chat_message_from,
    },
    session_chat_skills::handle_read_session_chat_skills_http,
    session_transcript_export::handle_export_session_transcript_http,
    session_transcript_size::handle_read_session_transcript_sizes_http,
};

use super::*;

pub(super) async fn route_chat_http(
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
        "/api/readSessionChat" => {
            // A phone that asked gets its long tool outputs cut (session_chat_tool_outputs.rs).
            let cut = body_json
                .get("params")
                .and_then(|params| params.get("cutToolOutputs"))
                == Some(&Value::Bool(true));
            let ids = |name: &str| {
                body_json
                    .get("params")
                    .and_then(|params| params.get(name))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string()
            };
            let (project_id, session_id) = (ids("projectId"), ids("sessionId"));
            let routed =
                handle_read_session_chat_http(&state, endpoint.path, request_id, &body_json).await;
            if cut {
                crate::session_chat_tool_outputs::cut_read_answer(routed, &project_id, &session_id)
                    .await
            } else {
                routed
            }
        }
        "/api/readSessionChatToolOutput" => {
            crate::session_chat_tool_outputs::handle_read_tool_output_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/setSessionChatView" => crate::session_chat_view::handle_set_session_chat_view_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
        ),
        "/api/readVoiceAgent" => {
            crate::voice_agent::handle_read_voice_agent_http(&state, endpoint.path, request_id)
                .await
        }
        "/api/voiceAgentClip" => {
            crate::voice_agent::handle_voice_agent_clip_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/voiceAgentText" => {
            crate::voice_agent::handle_voice_agent_text_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/forgetVoiceAgentMemory" => {
            crate::voice_agent::handle_forget_voice_agent_memory_http(
                &state,
                endpoint.path,
                request_id,
            )
            .await
        }
        "/api/setVoiceAgentSettings" => {
            crate::voice_agent::handle_set_voice_agent_settings_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/saveDictationRecording"
        | "/api/transcribeDictation"
        | "/api/dismissDictation"
        | "/api/listDictations"
        | "/api/readDictationAudio"
        | "/api/deleteDictation" => {
            crate::voice_agent::handle_dictation_recordings_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/readDictateSettings"
        | "/api/setDictateSettings"
        | "/api/readDictateEngine"
        | "/api/deleteOldSpeechModels"
        | "/api/skipDictationPolish" => {
            crate::voice_agent::handle_dictate_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/setVoiceAgentKey" => {
            crate::voice_agent::handle_set_voice_agent_key_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/catchUpSession" => {
            crate::session_chat_catch_up::handle_catch_up_session_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/readSessionDetails" => {
            crate::session_details::handle_read_session_details_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/stopSessionBackgroundTask" => {
            crate::session_details::handle_stop_session_background_task_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/readSessionFilesLinks" => {
            crate::session_files_links::handle_read_session_files_links_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/readSessionChatSkills" => {
            handle_read_session_chat_skills_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/readSessionChatFiles" => {
            handle_read_session_chat_files_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/sendSessionChatMessage" => {
            let mut response = handle_send_session_chat_message_from(
                &state,
                &caller,
                endpoint.path,
                request_id.clone(),
                &body_json,
            )
            .await;
            crate::session_chat_send_diagnostics::record_response(
                &state,
                &request_id,
                &body_json,
                &mut response,
            )
            .await;
            response
        }
        "/api/saveSessionChatImage" => {
            handle_save_session_chat_image_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/saveSessionChatAttachment" => {
            handle_save_session_chat_attachment_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/readSessionChatImage" => {
            let (fallback_path, fallback_id) = (endpoint.path.clone(), request_id.clone());
            tokio::task::spawn_blocking(move || {
                handle_read_session_chat_image_http(endpoint.path, request_id, &body_json)
            })
            .await
            .unwrap_or_else(|_| {
                domain_error_response(
                    fallback_path,
                    fallback_id,
                    crate::domain::DomainStateError {
                        code: "internalError",
                        message: "Could not read the image.".to_string(),
                    },
                )
            })
        }
        "/api/answerSessionChatPrompt" => {
            let mut response = crate::session_chat_send_requests::send_once(
                &state,
                endpoint.path,
                request_id.clone(),
                &body_json,
                |state, endpoint_path, request_id, body| async move {
                    handle_answer_session_chat_prompt_http(&state, endpoint_path, request_id, &body)
                        .await
                },
            )
            .await;
            crate::session_chat_send_diagnostics::record_response(
                &state,
                &request_id,
                &body_json,
                &mut response,
            )
            .await;
            response
        }
        "/api/interruptSessionChat" => {
            handle_interrupt_session_chat_http(&state, endpoint.path, request_id, &body_json).await
        }
        /*
        CDXC:SessionChat 2026-09-02:
        Held open for the whole drive rather than queued-and-acknowledged: the
        answer a caller needs is whether Claude Code ACCEPTED the rewind, and
        that is only knowable once the driver has watched the dialog close.
        */
        "/api/rewindSessionChat" => {
            crate::session_chat_rewind::handle_rewind_session_chat_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/selectSessionChatModel" => {
            crate::session_chat_codex_picker::handle_select_session_chat_model_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/handoffSessionChatDraft" | "/api/replaceSessionChatDraft" => {
            let mut response = if endpoint.path == "/api/handoffSessionChatDraft" {
                handle_handoff_session_chat_draft_http(
                    &state,
                    endpoint.path,
                    request_id.clone(),
                    &body_json,
                )
                .await
            } else {
                handle_replace_session_chat_draft_http(
                    &state,
                    endpoint.path,
                    request_id.clone(),
                    &body_json,
                )
                .await
            };
            crate::session_chat_send_diagnostics::record_response(
                &state,
                &request_id,
                &body_json,
                &mut response,
            )
            .await;
            response
        }
        "/api/takeBackSessionChatQueuedMessage" => {
            crate::session_chat_send::handle_take_back_session_chat_queued_message_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/claimSessionChatLaunchDraft" => handle_claim_session_chat_launch_draft_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
        ),
        "/api/readSessionChatQueue"
        | "/api/queueSessionChatPrompt"
        | "/api/updateSessionChatQueuedPrompt"
        | "/api/removeSessionChatQueuedPrompt"
        | "/api/reorderSessionChatQueue"
        | "/api/sendSessionChatQueuedPrompt"
        | "/api/setSessionChatDraft"
        | "/api/acknowledgeSessionChatDraftHandoff" => {
            // A queued prompt is a send too: a repeated `sendRequestId` must not queue it twice.
            let mut response = if endpoint.path == "/api/queueSessionChatPrompt" {
                let queue_state = state.clone();
                crate::session_chat_send_requests::send_once(
                    &state,
                    endpoint.path,
                    request_id.clone(),
                    &body_json,
                    move |_, endpoint_path, request_id, body| async move {
                        handle_session_chat_queue_http(
                            &queue_state,
                            endpoint_path,
                            request_id,
                            &body,
                        )
                        .await
                    },
                )
                .await
            } else {
                handle_session_chat_queue_http(
                    &state,
                    endpoint.path,
                    request_id.clone(),
                    &body_json,
                )
                .await
            };
            crate::session_chat_send_diagnostics::record_response(
                &state,
                &request_id,
                &body_json,
                &mut response,
            )
            .await;
            response
        }
        /*
        CDXC:SessionChat 2026-08-26:
        The evidence read behind a `composerNotReady` refusal. Domain-shaped
        because everything it needs is one session row plus one screen capture,
        and the capture is a direct socket read measured in single-digit
        milliseconds — the same one every screen-state reader already takes.
        */
        // On a blocking thread: a half-drawn Claude frame is looked at again and may be repaired
        // before the answer (`repair_unreadable_frame`), which waits about a second.
        "/api/readSessionTerminalTail" => {
            let (fallback_path, fallback_id) = (endpoint.path.clone(), request_id.clone());
            tokio::task::spawn_blocking(move || {
                handle_domain_http(
                    &state,
                    endpoint.path,
                    request_id,
                    &body_json,
                    |repository, _db, params, _| {
                        crate::session_chat_composer::read_session_terminal_tail(
                            repository,
                            &read_project_id(params)?,
                            &read_session_id(params)?,
                            params.get("agentId").and_then(Value::as_str),
                        )
                    },
                )
            })
            .await
            .unwrap_or_else(|_| {
                domain_error_response(
                    fallback_path,
                    fallback_id,
                    crate::domain::DomainStateError {
                        code: "internalError",
                        message: "Could not read the terminal screen.".to_string(),
                    },
                )
            })
        }
        // Copy Report (session_chat_report.rs).
        "/api/readSessionChatReport" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _db, params, _| {
                crate::session_chat_report::read_session_chat_report(
                    &state,
                    repository,
                    &read_project_id(params)?,
                    &read_session_id(params)?,
                )
            },
        ),
        // One form, page or chart of the session's resource index, read back by its offsets
        // (session_chat_resources.rs).
        "/api/readSessionChatResource" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                crate::session_chat_resources::read_resource(
                    repository,
                    db,
                    &read_project_id(params)?,
                    &read_session_id(params)?,
                    params.get("id").and_then(Value::as_str).unwrap_or_default(),
                )
            },
        ),
        // The question tags the session's chat asked and the next free one
        // (session_chat_open_questions.rs `read_question_tags`).
        "/api/readSessionChatQuestionTags" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                crate::session_chat_open_questions::read_question_tags(
                    &state,
                    db,
                    repository,
                    &read_project_id(params)?,
                    &read_session_id(params)?,
                )
            },
        ),
        "/api/exportSessionTranscript" => {
            handle_export_session_transcript_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/countSessionTranscriptTurns" => {
            crate::session_transcript_export::handle_count_session_transcript_turns_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/readSessionTranscriptSizes" => {
            handle_read_session_transcript_sizes_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        crate::visual_pages::READ_VISUAL_PAGE_THUMBNAIL_ENDPOINT => {
            crate::visual_pages::handle_read_visual_page_thumbnail_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
        }
        // It may read the page from another computer over its tunnel (exported.rs `export_url`).
        crate::visual_pages::EXPORT_VISUAL_PAGE_ENDPOINT => {
            let state = state.clone();
            let path = endpoint.path.clone();
            let body = body_json.clone();
            let id = request_id.clone();
            match tokio::task::spawn_blocking(move || {
                crate::visual_pages::handle_export_visual_page_http(&state, path, id, &body)
            })
            .await
            {
                Ok(routed) => routed,
                Err(_) => crate::server::domain_error_response(
                    endpoint.path,
                    request_id,
                    crate::domain::DomainStateError::bad_request("The page could not be exported."),
                ),
            }
        }
        crate::visual_pages::IMPORT_VISUAL_PAGE_ANSWERS_ENDPOINT => {
            crate::visual_pages::handle_import_visual_page_answers_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        crate::visual_pages::READ_VISUAL_PAGE_SOURCE_ENDPOINT => {
            crate::visual_pages::handle_read_visual_page_source_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
        }
        crate::visual_pages::REFRESH_VISUAL_PAGE_ENDPOINT => {
            crate::visual_pages::handle_refresh_visual_page_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
        }
        crate::visual_pages::STORE_VISUAL_PAGE_THUMBNAIL_ENDPOINT => {
            crate::visual_pages::handle_store_visual_page_thumbnail_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
        }
        crate::agent_message_deliveries::WATCH_AGENT_MESSAGE_DELIVERY_ENDPOINT => {
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |_, db, params, _| {
                    crate::agent_message_deliveries::handle_watch_agent_message_delivery(db, params)
                },
            );
            // The sender's chat learns of the send before the agent's tool result reaches it.
            let sender = read_domain_rpc_params(&body_json).ok().and_then(|params| {
                Some((
                    params.get("senderProjectId")?.as_str()?.to_string(),
                    params.get("senderSessionId")?.as_str()?.to_string(),
                ))
            });
            if let Some((project_id, session_id)) = sender {
                super::super::coordinator_runtime::republish_session_chat(
                    &state,
                    &project_id,
                    &session_id,
                );
            }
            response
        }
        crate::visual_pages::PUBLISH_VISUAL_PAGE_ENDPOINT => {
            crate::visual_pages::handle_publish_visual_page_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
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
