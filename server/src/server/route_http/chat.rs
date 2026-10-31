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
        handle_send_session_chat_message_http,
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
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/readSessionChat" => {
            handle_read_session_chat_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/readVoiceAgent" => {
            crate::voice_agent::handle_read_voice_agent_http(&state, endpoint.path, request_id)
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
            let mut response = handle_send_session_chat_message_http(
                &state,
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
        "/api/handoffSessionChatDraft" => {
            handle_handoff_session_chat_draft_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/replaceSessionChatDraft" => {
            handle_replace_session_chat_draft_http(&state, endpoint.path, request_id, &body_json)
                .await
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
        "/api/exportSessionTranscript" => {
            handle_export_session_transcript_http(&state, endpoint.path, request_id, &body_json)
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
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |_, db, params, _| {
                    crate::agent_message_deliveries::handle_watch_agent_message_delivery(db, params)
                },
            )
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
            })
        }
    })
}
