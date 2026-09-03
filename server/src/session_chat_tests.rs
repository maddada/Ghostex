#[cfg(test)]
mod tests {
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::{json, Value};

    use crate::resume_lookup::home_dir;
    use crate::session_chat::*;

    #[test]
    fn message_serialization_matches_shared_ts_shape() {
        let message = SessionChatMessage {
            id: "m1".to_string(),
            role: SessionChatRole::Assistant,
            blocks: vec![
                text_block("hello"),
                SessionChatBlock::ToolCall {
                    name: "Bash".to_string(),
                    input: json!({"command": "ls"}),
                },
                SessionChatBlock::ToolResult {
                    output: "ok".to_string(),
                    is_error: Some(true),
                },
                SessionChatBlock::ImageRef {
                    path: Some("/tmp/a.png".to_string()),
                    url: None,
                    alt: None,
                },
            ],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
            queued: false,
        };
        let serialized = serde_json::to_value(&message).expect("serialize");
        assert_eq!(
            serialized,
            json!({
                "id": "m1",
                "role": "assistant",
                "blocks": [
                    {"type": "text", "text": "hello"},
                    {"type": "tool-call", "name": "Bash", "input": {"command": "ls"}},
                    {"type": "tool-result", "output": "ok", "isError": true},
                    {"type": "image-ref", "path": "/tmp/a.png"},
                ],
                "timestamp": Value::Null,
                "source": "transcript",
            })
        );
    }

    #[test]
    fn claude_decoder_handles_roles_interrupts_and_injected_turns() {
        let user = decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u1","timestamp":"2026-07-29T01:35:31.020Z","message":{"role":"user","content":"fix the bug"}}"#,
            "fb",
        )
        .expect("user decodes");
        assert_eq!(user.role, SessionChatRole::User);
        assert_eq!(user.id, "u1");
        assert!(user.timestamp.is_some(), "RFC3339 timestamp must parse");

        let tool = decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u2","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"out"}]}}"#,
            "fb",
        )
        .expect("tool result decodes");
        assert_eq!(tool.role, SessionChatRole::Tool);

        let interrupted = decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u3","interruptedMessageId":"m9","message":{"role":"user","content":"x"}}"#,
            "fb",
        )
        .expect("interrupt decodes");
        assert_eq!(interrupted.role, SessionChatRole::System);

        // Injected meta turn keeps only tool results; text-only meta is dropped.
        assert!(decode_claude_transcript_line(
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>x</local-command-caveat>"}}"#,
            "fb",
        )
        .is_none());
        assert!(decode_claude_transcript_line(r#"{"type":"summary"}"#, "fb").is_none());
        assert!(decode_claude_transcript_line("not json", "fb").is_none());
    }

    #[test]
    fn claude_decoder_marks_thinking_only_rows_as_reasoning() {
        let reasoning = decode_claude_transcript_line(
            r#"{"type":"assistant","uuid":"a1","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Inspect the state."}]}}"#,
            "fb",
        )
        .expect("thinking row decodes");
        assert_eq!(reasoning.role, SessionChatRole::Reasoning);

        let mixed = decode_claude_transcript_line(
            r#"{"type":"assistant","uuid":"a2","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Inspect the state."},{"type":"tool_use","name":"Read","input":{}}]}}"#,
            "fb",
        )
        .expect("mixed row decodes");
        assert_eq!(mixed.role, SessionChatRole::Assistant);
    }

    #[test]
    fn grok_session_ids_reject_path_syntax() {
        assert!(is_safe_grok_session_id("session_123-abc"));
        assert!(!is_safe_grok_session_id(""));
        assert!(!is_safe_grok_session_id("../session"));
        assert!(!is_safe_grok_session_id("session\\child"));
        assert!(!is_safe_grok_session_id(&"a".repeat(129)));
    }

    #[test]
    fn claude_lifecycle_tool_use_is_not_terminal() {
        assert!(decode_claude_turn_lifecycle(
            r#"{"type":"assistant","uuid":"a1","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","name":"Bash","input":{}}]}}"#,
            "fb",
        )
        .is_none());
        let completed = decode_claude_turn_lifecycle(
            r#"{"type":"assistant","uuid":"a2","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}}"#,
            "fb",
        )
        .expect("terminal");
        assert_eq!(completed.state, SessionChatTurnLifecycleState::Completed);
        // No stop_reason + prose + no tool_use → completed (historical rows).
        let historical = decode_claude_turn_lifecycle(
            r#"{"type":"assistant","uuid":"a3","message":{"content":[{"type":"text","text":"done"}]}}"#,
            "fb",
        )
        .expect("historical terminal");
        assert_eq!(historical.state, SessionChatTurnLifecycleState::Completed);
        let working = decode_claude_turn_lifecycle(
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"go"}}"#,
            "fb",
        )
        .expect("working");
        assert_eq!(working.state, SessionChatTurnLifecycleState::Working);
        // Harness noise must not restart the spinner.
        assert!(decode_claude_turn_lifecycle(
            r#"{"type":"user","uuid":"u2","message":{"role":"user","content":"<system-reminder>hi</system-reminder>"}}"#,
            "fb",
        )
        .is_none());
    }

    #[test]
    fn codex_decoder_and_lifecycle_cover_event_lane() {
        let user = decode_codex_transcript_line(
            r#"{"timestamp":"2026-07-30T04:33:09.500Z","type":"event_msg","payload":{"type":"user_message","message":"do it"}}"#,
            "fb",
        )
        .expect("user");
        assert_eq!(user.role, SessionChatRole::User);
        let call = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"function_call","id":"fc1","name":"exec_command","arguments":"{\"cmd\":\"ls\"}"}}"#,
            "fb",
        )
        .expect("call");
        assert_eq!(call.role, SessionChatRole::Assistant);
        assert_eq!(call.id, "fc1");
        let output = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"Exit code: 0"}}"#,
            "fb",
        )
        .expect("output");
        assert_eq!(output.role, SessionChatRole::Tool);
        let reasoning = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"reasoning","id":"rs1","summary":[{"type":"summary_text","text":"plan"}]}}"#,
            "fb",
        )
        .expect("reasoning");
        assert_eq!(reasoning.role, SessionChatRole::Reasoning);

        let started = decode_codex_turn_lifecycle(
            r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#,
            "fb",
        )
        .expect("started");
        assert_eq!(started.state, SessionChatTurnLifecycleState::Working);
        let aborted = decode_codex_turn_lifecycle(
            r#"{"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"t1"}}"#,
            "fb",
        )
        .expect("aborted");
        assert_eq!(aborted.state, SessionChatTurnLifecycleState::Interrupted);
    }

    /*
    Record shapes copied from a real 2026-08-07 gpt-5.6 rollout. These builds
    write NO `user_message`/`agent_message` events; the visible turns arrive
    only as `item_completed` items, and dropping them made chat end at the last
    reasoning summary while the terminal showed the full final answer.
    */
    #[test]
    fn codex_decodes_item_completed_message_lane() {
        let user = decode_codex_transcript_line(
            r#"{"timestamp":"2026-08-07T13:59:00.000Z","type":"event_msg","payload":{"type":"item_completed","thread_id":"th1","turn_id":"t1","item":{"type":"UserMessage","id":"019fdc6f-1436-7b10-a55f-23ba56beb5ac","content":[{"type":"skill","name":"ghostex-browser-use","path":"/Users/madda/.agents/skills/ghostex-browser-use/SKILL.md"},{"type":"text","text":"explain questions 2 and 3"}]}}}"#,
            "fb",
        )
        .expect("user item");
        assert_eq!(user.role, SessionChatRole::User);
        assert_eq!(user.id, "019fdc6f-1436-7b10-a55f-23ba56beb5ac");
        // The skill chip decodes to nothing: extra chip-derived text made the
        // turn differ from its optimistic echo, duplicating the sent message.
        assert_eq!(
            user.blocks,
            vec![text_block("explain questions 2 and 3".to_string())]
        );

        // AgentMessage spells its text block `Text`, unlike UserMessage's `text`.
        let assistant = decode_codex_transcript_line(
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage","id":"msg_0a5b","content":[{"type":"Text","text":"Created the Markdown plan."}]}}}"#,
            "fb",
        )
        .expect("assistant item");
        assert_eq!(assistant.role, SessionChatRole::Assistant);
        assert_eq!(assistant.id, "msg_0a5b");
        assert_eq!(message_text(&assistant), "Created the Markdown plan.");

        // Reasoning/CommandExecution/McpToolCall items stay on the
        // response_item lane, which new-style rollouts still write.
        assert!(decode_codex_transcript_line(
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"Reasoning","id":"rs1","summary_text":["**Planning**"],"raw_content":[]}}}"#,
            "fb",
        )
        .is_none());
        assert!(decode_codex_transcript_line(
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"c1"}}}"#,
            "fb",
        )
        .is_none());
    }

    /*
    Record shapes below are copied from real rollouts under the codex sessions
    directory, with the bodies trimmed. `custom_tool_call` +
    `custom_tool_call_output` are as frequent as the function-call lane in
    real files and used to decode to nothing at all.
    */
    #[test]
    fn codex_decodes_custom_tool_and_hosted_tool_lanes() {
        let call = decode_codex_transcript_line(
            r#"{"timestamp":"2026-08-01T02:04:11.100Z","type":"response_item","payload":{"type":"custom_tool_call","id":"ctc_0e7c","call_id":"call_Nx1m","name":"exec","status":"completed","input":"const r = await tools.exec_command({cmd:\"ls\"});"}}"#,
            "fb",
        )
        .expect("custom_tool_call decodes");
        assert_eq!(call.role, SessionChatRole::Assistant);
        assert_eq!(call.id, "ctc_0e7c");
        assert_eq!(
            call.blocks,
            vec![SessionChatBlock::ToolCall {
                name: "exec".to_string(),
                input: json!("const r = await tools.exec_command({cmd:\"ls\"});"),
            }]
        );

        // Output is an array of Responses-API `input_text` blocks.
        let output = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"custom_tool_call_output","id":"ctco_1","call_id":"call_Nx1m","output":[{"type":"input_text","text":"Script completed"},{"type":"input_text","text":"total 4"}]}}"#,
            "fb",
        )
        .expect("custom_tool_call_output decodes");
        assert_eq!(output.role, SessionChatRole::Tool);
        assert_eq!(
            output.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "Script completed\ntotal 4".to_string(),
                is_error: None,
            }]
        );

        // Some builds write the same payload as a plain string.
        let string_output = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call_x","output":"Exit code: 0"}}"#,
            "fb",
        )
        .expect("string output decodes");
        assert_eq!(
            string_output.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "Exit code: 0".to_string(),
                is_error: None,
            }]
        );

        let web_search = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"web_search_call","status":"completed"}}"#,
            "fb",
        )
        .expect("web_search_call decodes");
        assert!(matches!(
            web_search.blocks.as_slice(),
            [SessionChatBlock::ToolCall { name, .. }] if name == "web_search"
        ));

        let tool_search = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"tool_search_call","id":"tsc_1","call_id":"call_q","status":"completed","execution":"client","arguments":{"query":"GitHub issue details"}}}"#,
            "fb",
        )
        .expect("tool_search_call decodes");
        assert_eq!(
            tool_search.blocks,
            vec![SessionChatBlock::ToolCall {
                name: "tool_search".to_string(),
                input: json!({"query": "GitHub issue details"}),
            }]
        );
        let tool_search_output = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"tool_search_output","call_id":"call_q","status":"completed","tools":[{"type":"namespace","name":"mcp__codex_apps__github","description":"…"}]}}"#,
            "fb",
        )
        .expect("tool_search_output decodes");
        assert_eq!(
            tool_search_output.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "mcp__codex_apps__github".to_string(),
                is_error: None,
            }]
        );

        // Inter-agent traffic: readable header text survives, the encrypted
        // envelope is skipped rather than dropping the whole record.
        let agent_message = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"agent_message","author":"/root/worker","recipient":"/root","content":[{"type":"input_text","text":"Message Type: MESSAGE"},{"type":"encrypted_content","encrypted_content":"gAAA"}]}}"#,
            "fb",
        )
        .expect("agent_message decodes");
        assert_eq!(agent_message.role, SessionChatRole::System);
        assert_eq!(
            agent_message.blocks,
            vec![text_block("Message Type: MESSAGE")]
        );
    }

    /*
    Every visible Codex turn is written twice (event lane + response-item lane).
    Measured across 4,881 local rollouts the two are exactly redundant, so only
    the event lane is decoded; decoding both would double every message.
    */
    #[test]
    fn codex_message_response_items_defer_to_the_event_lane() {
        assert!(decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"message","id":"m1","role":"assistant","content":[{"type":"output_text","text":"I'll do it"}]}}"#,
            "fb",
        )
        .is_none());
        assert!(decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"message","id":"m2","role":"user","content":[{"type":"input_text","text":"AGENTS.md instructions"}]}}"#,
            "fb",
        )
        .is_none());
        assert!(decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"message","id":"m3","role":"developer","content":[{"type":"input_text","text":"<permissions instructions>"}]}}"#,
            "fb",
        )
        .is_none());
        // The event lane still carries both sides of the conversation.
        assert_eq!(
            decode_codex_transcript_line(
                r#"{"type":"event_msg","payload":{"type":"agent_message","message":"I'll do it"}}"#,
                "fb",
            )
            .map(|message| message.role),
            Some(SessionChatRole::Assistant)
        );
    }

    #[test]
    fn shared_block_mapper_accepts_responses_api_text_spellings() {
        let blocks = claude_content_blocks(Some(&json!([
            {"type": "input_text", "text": "in"},
            {"type": "output_text", "text": "out"},
            {"type": "summary_text", "text": "sum"},
            {"type": "encrypted_content", "encrypted_content": "gAAA"},
        ])));
        assert_eq!(
            blocks,
            vec![text_block("in"), text_block("out"), text_block("sum")]
        );
    }

    #[test]
    fn base64_images_render_as_a_pasted_image_chip() {
        // A pasted screenshot has no url and no path; returning no block at all
        // made an image-only user turn vanish from chat entirely.
        let pasted = decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}}]}}"#,
            "fb",
        )
        .expect("base64 image decodes");
        assert_eq!(pasted.role, SessionChatRole::User);
        assert_eq!(
            pasted.blocks,
            vec![SessionChatBlock::ImageRef {
                path: None,
                url: None,
                alt: Some("Pasted image".to_string()),
            }]
        );
        // Bytes must never ride the wire.
        let serialized = serde_json::to_string(&pasted).expect("serialize");
        assert!(!serialized.contains("iVBORw0KGgo"));
        // A source with neither bytes nor a locator is still nothing to show.
        assert!(decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u2","message":{"role":"user","content":[{"type":"image","source":{"type":"file"}}]}}"#,
            "fb",
        )
        .is_none());
    }

    #[test]
    fn claude_rows_without_uuid_get_distinct_offset_ids() {
        // Both rows of one API response share message.id; using it as the id
        // fallback made the client's id-dedup drop the second row.
        let first = decode_claude_transcript_line(
            r#"{"type":"assistant","message":{"id":"msg_shared","role":"assistant","content":[{"type":"text","text":"one"}]}}"#,
            "/tmp/t.jsonl:0000000000000000",
        )
        .expect("first row");
        let second = decode_claude_transcript_line(
            r#"{"type":"assistant","message":{"id":"msg_shared","role":"assistant","content":[{"type":"text","text":"two"}]}}"#,
            "/tmp/t.jsonl:0000000000000420",
        )
        .expect("second row");
        assert_ne!(first.id, second.id);
        assert_eq!(first.id, "/tmp/t.jsonl:0000000000000000");
        assert_eq!(second.id, "/tmp/t.jsonl:0000000000000420");
        // A row that HAS a uuid still keys on it.
        assert_eq!(
            decode_claude_transcript_line(
                r#"{"type":"assistant","uuid":"a1","message":{"id":"msg_shared","role":"assistant","content":[{"type":"text","text":"x"}]}}"#,
                "fb",
            )
            .map(|message| message.id),
            Some("a1".to_string())
        );
    }

    #[test]
    fn readers_stamp_byte_offsets_identically_on_every_path() {
        let path = write_temp_transcript(&[
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"one"}}"#,
            r#"{"type":"assistant","uuid":"a1","message":{"role":"assistant","content":[{"type":"text","text":"two"}]}}"#,
        ]);
        let tail = read_session_chat_transcript_tail_file(
            &path,
            300,
            decode_claude_transcript_line,
            true,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("tail read");
        let mut state = SessionChatIncrementalState::new();
        let forward = read_incremental_transcript_messages(
            &path,
            &mut state,
            decode_claude_transcript_line,
            None,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("forward read");
        assert_eq!(tail.messages.len(), 2);
        assert_eq!(tail.messages[0].byte_offset, Some(0));
        assert_eq!(
            tail.messages
                .iter()
                .map(|message| message.byte_offset)
                .collect::<Vec<_>>(),
            forward
                .iter()
                .map(|message| message.byte_offset)
                .collect::<Vec<_>>(),
            "the same line must report the same offset from both readers",
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn transcript_prompt_state_detects_and_retires_question_cards() {
        let ask = |input: Value| SessionChatMessage {
            id: "ask".to_string(),
            role: SessionChatRole::Assistant,
            blocks: vec![SessionChatBlock::ToolCall {
                name: "AskUserQuestion".to_string(),
                input,
            }],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
            queued: false,
        };
        let result = SessionChatMessage {
            id: "res".to_string(),
            role: SessionChatRole::Tool,
            blocks: vec![SessionChatBlock::ToolResult {
                output: "Fast".to_string(),
                is_error: None,
            }],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
            queued: false,
        };
        let input = json!({
            "questions": [{"question": "Which approach?", "options": ["Fast", "Careful"]}],
        });

        // Unanswered trailing question ⇒ card, even with no hook prompt at all.
        let pending = scan_transcript_prompt_state(&[ask(input.clone())]);
        assert!(!pending.answered());
        let derived = resolve_session_chat_prompt(None, &pending).expect("card derives");
        assert!(matches!(
            derived,
            SessionChatInteractivePrompt::Question { ref questions, .. } if questions.len() == 1
        ));

        // Its tool result landing means it was answered (possibly in the
        // terminal), so a stored question card must be retired.
        let answered = scan_transcript_prompt_state(&[ask(input.clone()), result.clone()]);
        assert!(answered.answered());
        assert!(resolve_session_chat_prompt(None, &answered).is_none());
        assert!(resolve_session_chat_prompt(
            Some(SessionChatInteractivePrompt::Question {
                questions: vec![SessionChatQuestion {
                    question: "Which approach?".to_string(),
                    header: None,
                    multi_select: false,
                    allow_custom: None,
                    tool_name: None,
                    recommended: None,
                    options: Vec::new(),
                }],
                tool_use_id: None,
            }),
            &answered,
        )
        .is_none());

        // Hook-derived approvals stay authoritative regardless.
        let approval = SessionChatInteractivePrompt::Approval {
            tool: "Bash".to_string(),
            summary: None,
            tool_use_id: None,
        };
        assert_eq!(
            resolve_session_chat_prompt(Some(approval.clone()), &answered),
            Some(approval.clone()),
        );
        // …and a hook question outranks a transcript-derived one.
        assert_eq!(
            resolve_session_chat_prompt(Some(approval.clone()), &pending),
            Some(approval),
        );
        // Unrelated tool traffic says nothing either way.
        let quiet = scan_transcript_prompt_state(&[result]);
        assert!(!quiet.answered());
        assert!(quiet.pending().is_none());
    }

    #[test]
    fn earlier_answered_question_does_not_retire_a_newer_stored_card() {
        // A *pending* AskUserQuestion has no assistant row in the transcript
        // yet, so the tail's most recent question is the previous, answered
        // one. The hook-stored card for the NEW question must survive.
        let ask = |input: Value| SessionChatMessage {
            id: "ask-old".to_string(),
            role: SessionChatRole::Assistant,
            blocks: vec![SessionChatBlock::ToolCall {
                name: "AskUserQuestion".to_string(),
                input,
            }],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
            queued: false,
        };
        let result = SessionChatMessage {
            id: "res-old".to_string(),
            role: SessionChatRole::Tool,
            blocks: vec![SessionChatBlock::ToolResult {
                output: "Red".to_string(),
                is_error: None,
            }],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
            queued: false,
        };
        let old_question = json!({
            "questions": [{"question": "Which color do you prefer?", "options": ["Red", "Blue"]}],
        });
        let transcript = scan_transcript_prompt_state(&[ask(old_question), result]);
        assert!(transcript.answered());

        let new_stored = SessionChatInteractivePrompt::Question {
            questions: vec![SessionChatQuestion {
                question: "Which animal do you prefer?".to_string(),
                header: Some("Animal".to_string()),
                multi_select: false,
                allow_custom: None,
                tool_name: None,
                recommended: None,
                options: Vec::new(),
            }],
            tool_use_id: None,
        };
        // Different question ⇒ kept (this was the regression: it was retired).
        assert_eq!(
            resolve_session_chat_prompt(Some(new_stored.clone()), &transcript),
            Some(new_stored),
        );
        // Same question re-stored ⇒ still retired (answered in the terminal).
        let same_stored = SessionChatInteractivePrompt::Question {
            questions: vec![SessionChatQuestion {
                question: "Which color do you prefer?".to_string(),
                header: None,
                multi_select: false,
                allow_custom: None,
                tool_name: None,
                recommended: None,
                options: Vec::new(),
            }],
            tool_use_id: None,
        };
        assert!(resolve_session_chat_prompt(Some(same_stored), &transcript).is_none());
    }

    #[test]
    fn embedded_id_scan_requires_a_records_own_session_id() {
        let target = "aaaaaaaa-1111-2222-3333-444444444444";
        let other = "bbbbbbbb-5555-6666-7777-888888888888";
        // A transcript that merely QUOTES the id (orchestrator prompt, hook
        // payload, tool output) must not be adopted.
        let quoting = write_temp_transcript(&[&format!(
            r#"{{"type":"user","sessionId":"{other}","message":{{"role":"user","content":"resume \"session_id\":\"{target}\" please"}}}}"#
        )]);
        assert!(!head_declares_session_id(&quoting, target));
        assert!(head_declares_session_id(&quoting, other));

        // Snake-case spelling counts, sidechain rows do not.
        let sidechain = write_temp_transcript(&[
            &format!(
                r#"{{"type":"user","isSidechain":true,"session_id":"{target}","message":{{"role":"user","content":"sub"}}}}"#
            ),
            r#"{"type":"summary"}"#,
        ]);
        assert!(!head_declares_session_id(&sidechain, target));

        let owned = write_temp_transcript(&[
            &format!(
                r#"{{"type":"user","session_id":"{target}","message":{{"role":"user","content":"go"}}}}"#
            ),
            r#"{"type":"summary"}"#,
            r#"{"type":"other"}"#,
        ]);
        assert!(head_declares_session_id(&owned, target));

        // `agent-*.jsonl` sub-agent transcripts record the PARENT session id in
        // every row and are usually the newest file in the directory.
        let mut sidechain_named = std::env::temp_dir();
        sidechain_named.push(format!("agent-{}.jsonl", std::process::id()));
        fs::write(
            &sidechain_named,
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{target}\",\"message\":{{\"role\":\"user\",\"content\":\"x\"}}}}\n"
            ),
        )
        .expect("write sidechain transcript");
        assert!(!head_declares_session_id(&sidechain_named, target));

        for path in [quoting, sidechain, owned, sidechain_named] {
            let _ = fs::remove_file(path);
        }
    }

    // ---------------------------------------------------------------------
    // Successor-transcript detection fixtures
    // ---------------------------------------------------------------------

    const FIXTURE_STALE_ID: &str = "11111111-1111-4111-8111-111111111111";
    const FIXTURE_SUCCESSOR_ID: &str = "22222222-2222-4222-8222-222222222222";
    const FIXTURE_SECOND_SUCCESSOR_ID: &str = "33333333-3333-4333-8333-333333333333";
    const FIXTURE_DECOY_ID: &str = "44444444-4444-4444-8444-444444444444";

    fn successor_fixture_dir(name: &str) -> PathBuf {
        static NEXT_FIXTURE_DIR: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "gxserver-session-chat-successor-{}-{}-{}",
            std::process::id(),
            name,
            NEXT_FIXTURE_DIR.fetch_add(1, Ordering::SeqCst),
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create fixture dir");
        path
    }

    fn write_fixture_transcript(directory: &Path, file_stem: &str, lines: &[String]) -> PathBuf {
        let path = directory.join(format!("{file_stem}.jsonl"));
        let mut body = String::new();
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
        fs::write(&path, body).expect("write fixture transcript");
        path
    }

    fn user_row(session_id: &str, uuid: &str, timestamp: &str, text: &str) -> String {
        format!(
            r#"{{"type":"user","sessionId":"{session_id}","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"user","content":"{text}"}}}}"#
        )
    }

    fn assistant_row(session_id: &str, uuid: &str, timestamp: &str, text: &str) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"{session_id}","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"assistant","stop_reason":"end_turn","content":[{{"type":"text","text":"{text}"}}]}}}}"#
        )
    }

    /// Claude copies the PREDECESSOR id into the resumed file's snake-case
    /// `session_id` while the camelCase `sessionId` stays the new file's own id.
    fn resume_fork_assistant_row(
        session_id: &str,
        predecessor_session_id: &str,
        uuid: &str,
        timestamp: &str,
    ) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"{session_id}","session_id":"{predecessor_session_id}","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"assistant","stop_reason":"end_turn","content":[{{"type":"text","text":"resumed"}}]}}}}"#
        )
    }

    fn continuation_marker_row(session_id: &str, uuid: &str, timestamp: &str) -> String {
        format!(
            r#"{{"type":"user","sessionId":"{session_id}","isCompactSummary":true,"uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"user","content":[{{"type":"text","text":"This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation."}}]}}}}"#
        )
    }

    fn inherited_snapshot_row(predecessor_session_id: &str) -> String {
        format!(
            r#"{{"type":"file-history-snapshot","messageId":"m1","snapshot":{{"trackedFileBackups":{{"/private/tmp/claude-501/project/{predecessor_session_id}/scratchpad/a.txt":{{"realParentDir":"/private/tmp/claude-501/project/{predecessor_session_id}/scratchpad"}}}}}}}}"#
        )
    }

    fn bare_mode_row(session_id: &str) -> String {
        format!(r#"{{"type":"mode","sessionId":"{session_id}","mode":"default"}}"#)
    }

    fn bare_permission_mode_row(session_id: &str) -> String {
        format!(
            r#"{{"type":"permission-mode","sessionId":"{session_id}","permissionMode":"acceptEdits","timestamp":null}}"#
        )
    }

    /// Stale predecessor: real conversation up to 2026-07-31T21:14, then only
    /// bare mode records — exactly the shape that keeps bumping mtime.
    fn write_stale_fixture(directory: &Path) -> PathBuf {
        write_fixture_transcript(
            directory,
            FIXTURE_STALE_ID,
            &[
                user_row(FIXTURE_STALE_ID, "u1", "2026-07-31T21:00:00.000Z", "start"),
                assistant_row(FIXTURE_STALE_ID, "a1", "2026-07-31T21:14:49.796Z", "done"),
                bare_mode_row(FIXTURE_STALE_ID),
                bare_permission_mode_row(FIXTURE_STALE_ID),
            ],
        )
    }

    fn stale_last_substantive_ms(stale_path: &Path) -> i64 {
        last_substantive_transcript_timestamp_ms(stale_path)
            .expect("stale fixture has a substantive record")
    }

    #[test]
    fn continuation_marker_successor_is_adopted() {
        let directory = successor_fixture_dir("continuation");
        let stale = write_stale_fixture(&directory);
        let successor = write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[
                // Deliberately NO predecessor id field: this exercises the
                // continuation-marker + inherited-snapshot lineage on its own.
                inherited_snapshot_row(FIXTURE_STALE_ID),
                continuation_marker_row(FIXTURE_SUCCESSOR_ID, "c1", "2026-08-01T09:00:00.000Z"),
                assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                    "continuing",
                ),
            ],
        );
        let outcome = find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_last_substantive_ms(&stale),
            &[],
        );
        match outcome {
            SessionChatSuccessorOutcome::Found(found) => {
                assert_eq!(found.agent_session_id, FIXTURE_SUCCESSOR_ID);
                assert_eq!(found.path, successor);
                assert_eq!(
                    found.lineage,
                    SessionChatSuccessorLineage::ContinuationSnapshot
                );
                assert_eq!(found.hops, 1);
            }
            other => panic!("expected the continuation successor, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn resume_fork_copy_successor_is_adopted() {
        let directory = successor_fixture_dir("resume-fork");
        let stale = write_stale_fixture(&directory);
        write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[
                resume_fork_assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    FIXTURE_STALE_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                ),
                user_row(
                    FIXTURE_SUCCESSOR_ID,
                    "u2",
                    "2026-08-01T09:06:00.000Z",
                    "keep going",
                ),
            ],
        );
        match find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_last_substantive_ms(&stale),
            &[],
        ) {
            SessionChatSuccessorOutcome::Found(found) => {
                assert_eq!(found.agent_session_id, FIXTURE_SUCCESSOR_ID);
                assert_eq!(
                    found.lineage,
                    SessionChatSuccessorLineage::PredecessorIdField
                );
            }
            other => panic!("expected the resume-fork successor, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn transcripts_that_merely_mention_the_stale_id_are_not_adopted() {
        let directory = successor_fixture_dir("mention-only");
        let stale = write_stale_fixture(&directory);
        // A busy neighbour session: it declares its own id, carries a
        // continuation marker of its OWN (unrelated) predecessor, and quotes the
        // stale id inside tool input, tool output and a summary record.
        write_fixture_transcript(
            &directory,
            FIXTURE_DECOY_ID,
            &[
                continuation_marker_row(FIXTURE_DECOY_ID, "c9", "2026-08-01T09:00:00.000Z"),
                format!(
                    r#"{{"type":"assistant","sessionId":"{FIXTURE_DECOY_ID}","uuid":"d1","timestamp":"2026-08-01T09:01:00.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"rg {FIXTURE_STALE_ID} ~/.claude/projects"}}}}]}}}}"#
                ),
                format!(
                    r#"{{"type":"user","sessionId":"{FIXTURE_DECOY_ID}","uuid":"d2","timestamp":"2026-08-01T09:02:00.000Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"{FIXTURE_STALE_ID}.jsonl"}}]}}}}"#
                ),
                format!(
                    r#"{{"type":"summary","summary":"work on {FIXTURE_STALE_ID}","leafUuid":"d1"}}"#
                ),
                assistant_row(FIXTURE_DECOY_ID, "d3", "2026-08-01T09:03:00.000Z", "ok"),
            ],
        );
        // A sub-agent transcript with textbook lineage: still never the
        // session's own conversation.
        write_fixture_transcript(
            &directory,
            "agent-abc123",
            &[resume_fork_assistant_row(
                "agent-abc123",
                FIXTURE_STALE_ID,
                "s1",
                "2026-08-01T09:07:00.000Z",
            )],
        );
        // Records that carry ONLY the stale id (no own identity) — the
        // 6d27b5150 guardrail.
        write_fixture_transcript(
            &directory,
            FIXTURE_SECOND_SUCCESSOR_ID,
            &[format!(
                r#"{{"type":"assistant","sessionId":"{FIXTURE_STALE_ID}","uuid":"z1","timestamp":"2026-08-01T09:08:00.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"copy"}}]}}}}"#
            )],
        );
        assert_eq!(
            find_claude_successor_transcript(
                FIXTURE_STALE_ID,
                &stale,
                stale_last_substantive_ms(&stale),
                &[],
            ),
            SessionChatSuccessorOutcome::NotFound
        );
        let _ = fs::remove_dir_all(&directory);
    }

    /*
    CDXC:SessionChatIdentity 2026-08-02:
    End-to-end runtime path, not just the detector: the real follower loop, a
    fake registry behind the state reader / adopt hook, real transcript files.
    The first cut passed every detector test and still never fired in the live
    daemon, so this exercises spawn → drain → staleness → scan → registry write
    → re-snapshot with the production code path and only the timers shrunk.
    Note `working: false` throughout: hooks never report a background-job
    continuation, so adoption must not depend on the working flag.
    */
    #[tokio::test]
    async fn follower_adopts_a_successor_and_delivers_its_tail() {
        use std::sync::Mutex;

        let directory = successor_fixture_dir("follower-loop");
        let stale = write_stale_fixture(&directory);
        let successor = write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[
                resume_fork_assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    FIXTURE_STALE_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                ),
                assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    "a3",
                    "2026-08-01T09:06:00.000Z",
                    "LIVE-SUCCESSOR-TAIL",
                ),
            ],
        );

        // Fake registry: (agentSessionId, agentSessionPath).
        let registry = Arc::new(Mutex::new((
            FIXTURE_STALE_ID.to_string(),
            stale.to_string_lossy().into_owned(),
        )));
        let notices: Arc<Mutex<Vec<SessionChatSuccessorNotice>>> = Arc::new(Mutex::new(Vec::new()));
        let frames: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));

        let state_reader: SessionChatStateReader = {
            let registry = registry.clone();
            Arc::new(move || {
                let (agent_session_id, agent_session_path) =
                    registry.lock().expect("registry").clone();
                SessionChatLiveState {
                    prompt: None,
                    working: false,
                    agent_session_id: Some(agent_session_id),
                    agent_session_path: Some(agent_session_path),
                }
            })
        };
        let hooks = SessionChatSuccessorHooks {
            bound_agent_session_ids: Arc::new(Vec::new),
            pending_fork_child_since_ms: Arc::new(|_| None),
            adopt_identity: {
                let registry = registry.clone();
                Arc::new(move |adoption| {
                    let mut stored = registry.lock().expect("registry");
                    // Compare-and-set, exactly like the domain write.
                    if Some(stored.0.as_str()) != adoption.previous_agent_session_id.as_deref() {
                        return false;
                    }
                    *stored = (adoption.agent_session_id, adoption.agent_session_path);
                    true
                })
            },
            log: {
                let notices = notices.clone();
                Arc::new(move |notice| notices.lock().expect("notices").push(notice))
            },
        };
        let config = SessionChatFollowerConfig {
            project_id: "P1".to_string(),
            session_id: "S1".to_string(),
            agent: Some("claude".to_string()),
            agent_session_id: Some(FIXTURE_STALE_ID.to_string()),
            agent_session_path: Some(stale.to_string_lossy().into_owned()),
            limit: 50,
            protocol_version: 1,
            server_id: "test-server".to_string(),
            state_reader: Some(state_reader),
            options_reader: None,
            options_change_watch: None,
            queue_reader: None,
            successor_hooks: Some(hooks),
            notice_publisher: None,
            tuning: SessionChatFollowerTuning {
                reconcile_interval: Duration::from_millis(20),
                stale_transcript_idle: Duration::from_millis(40),
                successor_scan_interval: Duration::from_millis(60),
                successor_stale_substantive_idle_ms: 1_000,
            },
        };
        let emit: SessionChatFrameEmitter = {
            let frames = frames.clone();
            Arc::new(move |frame| frames.lock().expect("frames").push(frame))
        };
        let task = tokio::spawn(run_session_chat_follower(
            config,
            Arc::new(SessionChatStream::new()),
            Arc::new(tokio::sync::Notify::new()),
            Arc::new(SessionChatFollowerHeartbeat::new()),
            emit,
        ));

        let mut adopted_frame: Option<Value> = None;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(25)).await;
            let seen = frames.lock().expect("frames").clone();
            adopted_frame = seen.into_iter().find(|frame| {
                frame.get("agentSessionId").and_then(Value::as_str) == Some(FIXTURE_SUCCESSOR_ID)
                    && frame
                        .get("messages")
                        .map(|messages| messages.to_string().contains("LIVE-SUCCESSOR-TAIL"))
                        .unwrap_or(false)
            });
            if adopted_frame.is_some() {
                break;
            }
        }
        task.abort();

        let notices = notices.lock().expect("notices").clone();
        assert!(
            adopted_frame.is_some(),
            "the follower never delivered the successor tail; notices: {notices:?}, frames: {:?}",
            frames
                .lock()
                .expect("frames")
                .iter()
                .map(|frame| frame.get("type").cloned().unwrap_or(Value::Null))
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            *registry.lock().expect("registry"),
            (
                FIXTURE_SUCCESSOR_ID.to_string(),
                successor.to_string_lossy().into_owned()
            ),
            "the corrected identity must be written back to the registry",
        );
        assert!(
            notices.iter().any(|notice| matches!(
                notice,
                SessionChatSuccessorNotice::Adopted(adoption)
                    if adoption.agent_session_id == FIXTURE_SUCCESSOR_ID
                        && adoption.previous_agent_session_id.as_deref() == Some(FIXTURE_STALE_ID)
            )),
            "adoption must be logged once: {notices:?}",
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn successor_bound_to_another_registry_session_is_not_adopted() {
        let directory = successor_fixture_dir("already-bound");
        let stale = write_stale_fixture(&directory);
        write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[resume_fork_assistant_row(
                FIXTURE_SUCCESSOR_ID,
                FIXTURE_STALE_ID,
                "a2",
                "2026-08-01T09:05:00.000Z",
            )],
        );
        let stale_ms = stale_last_substantive_ms(&stale);
        assert!(matches!(
            find_claude_successor_transcript(FIXTURE_STALE_ID, &stale, stale_ms, &[]),
            SessionChatSuccessorOutcome::Found(_)
        ));
        assert_eq!(
            find_claude_successor_transcript(
                FIXTURE_STALE_ID,
                &stale,
                stale_ms,
                &[FIXTURE_SUCCESSOR_ID.to_string()],
            ),
            // Reported as owned, NOT as NotFound: the difference is what makes
            // a blocked adoption visible in the log.
            SessionChatSuccessorOutcome::OwnedByAnotherSession {
                candidate_session_ids: vec![FIXTURE_SUCCESSOR_ID.to_string()],
            },
            "a transcript another live session already owns must never be stolen",
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn equally_recent_continuations_adopt_none() {
        let directory = successor_fixture_dir("ambiguous");
        let stale = write_stale_fixture(&directory);
        for stem in [FIXTURE_SUCCESSOR_ID, FIXTURE_SECOND_SUCCESSOR_ID] {
            write_fixture_transcript(
                &directory,
                stem,
                &[resume_fork_assistant_row(
                    stem,
                    FIXTURE_STALE_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                )],
            );
        }
        match find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_last_substantive_ms(&stale),
            &[],
        ) {
            SessionChatSuccessorOutcome::Ambiguous {
                predecessor_session_id,
                candidate_session_ids,
            } => {
                assert_eq!(predecessor_session_id, FIXTURE_STALE_ID);
                assert_eq!(candidate_session_ids.len(), 2);
            }
            other => panic!("expected an ambiguous outcome, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn chained_successors_are_followed_to_the_newest() {
        let directory = successor_fixture_dir("chain");
        let stale = write_stale_fixture(&directory);
        write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[
                resume_fork_assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    FIXTURE_STALE_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                ),
                bare_mode_row(FIXTURE_SUCCESSOR_ID),
            ],
        );
        let newest = write_fixture_transcript(
            &directory,
            FIXTURE_SECOND_SUCCESSOR_ID,
            &[
                inherited_snapshot_row(FIXTURE_SUCCESSOR_ID),
                continuation_marker_row(
                    FIXTURE_SECOND_SUCCESSOR_ID,
                    "c2",
                    "2026-08-01T12:00:00.000Z",
                ),
                assistant_row(
                    FIXTURE_SECOND_SUCCESSOR_ID,
                    "a3",
                    "2026-08-01T12:10:00.000Z",
                    "still going",
                ),
            ],
        );
        match find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_last_substantive_ms(&stale),
            &[],
        ) {
            SessionChatSuccessorOutcome::Found(found) => {
                assert_eq!(found.agent_session_id, FIXTURE_SECOND_SUCCESSOR_ID);
                assert_eq!(found.path, newest);
                assert_eq!(found.hops, 2, "the chain must be followed to its end");
            }
            other => panic!("expected the second successor, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&directory);
    }

    /*
    The dead transcript keeps receiving bare `mode`/`permission-mode` records,
    so its mtime tracks "now" forever. Neither the substantive-staleness clock
    nor the follower's drain may treat that as activity.
    */
    #[test]
    fn bare_mode_appends_do_not_look_like_transcript_activity() {
        let directory = successor_fixture_dir("mtime-bumps");
        let stale = write_stale_fixture(&directory);
        let before = stale_last_substantive_ms(&stale);
        let version_before = read_transcript_file_version(&stale).expect("stat fixture");

        let mut drain_state = FollowerFileState::new();
        let first = follower_drain_once(
            &stale,
            300,
            SessionChatTranscriptAgent::Claude,
            decode_claude_transcript_line,
            Some(decode_claude_turn_lifecycle),
            &mut drain_state,
            true,
        );
        assert!(matches!(first, FollowerDrainOutcome::Snapshot { .. }));

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&stale)
            .expect("append to fixture");
        writeln!(file, "{}", bare_permission_mode_row(FIXTURE_STALE_ID))
            .expect("write bare record");
        drop(file);

        let version_after = read_transcript_file_version(&stale).expect("stat fixture");
        assert!(
            version_after.size > version_before.size,
            "the fixture must actually have grown"
        );
        assert_eq!(
            last_substantive_transcript_timestamp_ms(&stale),
            Some(before),
            "bare records must not move the substantive clock"
        );
        let second = follower_drain_once(
            &stale,
            300,
            SessionChatTranscriptAgent::Claude,
            decode_claude_transcript_line,
            Some(decode_claude_turn_lifecycle),
            &mut drain_state,
            false,
        );
        assert!(
            matches!(second, FollowerDrainOutcome::Idle),
            "a bare mode append must not count as transcript activity"
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn stream_frames_are_published_in_seq_order() {
        use std::sync::Mutex;
        let stream = Arc::new(SessionChatStream::new());
        stream.begin_generation();
        let published: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let stream = stream.clone();
            let published = published.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..50 {
                    stream.emit_sequenced(
                        |seq| json!(seq),
                        |frame| {
                            published
                                .lock()
                                .expect("publish lock")
                                .push(frame.as_i64().expect("seq"));
                        },
                    );
                }
            }));
        }
        for handle in handles {
            handle.join().expect("thread");
        }
        let published = published.lock().expect("publish lock").clone();
        assert_eq!(published.len(), 400);
        assert!(
            published.windows(2).all(|pair| pair[0] < pair[1]),
            "frames must reach the hub in seq order",
        );
    }

    #[test]
    fn grok_decoder_reads_the_session_update_log() {
        // Hook plumbing and mode/plan rows are not conversation.
        assert!(decode_grok_transcript_line(
            r#"{"timestamp":1787360342,"method":"_x.ai/session/update","params":{"sessionId":"s","update":{"sessionUpdate":"hook_execution","event_name":"stop"}}}"#,
            "fb",
        )
        .is_none());
        let user = decode_grok_transcript_line(
            r#"{"timestamp":1787360343,"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"hello there"}}}}"#,
            "fb",
        )
        .expect("user");
        assert_eq!(user.role, SessionChatRole::User);
        assert_eq!(user.blocks, vec![text_block("hello there")]);
        assert_eq!(user.timestamp, Some(1_787_360_343_000));
        let pasted = decode_grok_transcript_line(
            r#"{"timestamp":1787360343,"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"/tmp/ghostex-paste-1.png what is this"}}}}"#,
            "fb",
        )
        .expect("pasted image");
        assert_eq!(
            pasted.blocks,
            vec![
                SessionChatBlock::ImageRef {
                    path: Some("/tmp/ghostex-paste-1.png".to_string()),
                    url: None,
                    alt: None,
                },
                text_block("what is this"),
            ]
        );
        let thought = decode_grok_transcript_line(
            r#"{"timestamp":1787360351,"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"planning"}}}}"#,
            "fb",
        )
        .expect("thought");
        assert_eq!(thought.role, SessionChatRole::Reasoning);
        let call = decode_grok_transcript_line(
            r#"{"timestamp":1787360352,"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"c1","title":"search_replace","rawInput":{"file_path":"/a"},"_meta":{"x.ai/tool":{"name":"search_replace","label":"Edit"}}}}}"#,
            "fb",
        )
        .expect("tool call");
        assert_eq!(
            call.blocks,
            vec![SessionChatBlock::ToolCall {
                name: "Edit".to_string(),
                input: json!({"file_path": "/a"}),
            }],
        );
        // An in-flight update only re-describes the pending call.
        assert!(decode_grok_transcript_line(
            r#"{"timestamp":1787360352,"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"c1","status":"in_progress","title":"Edit `/a`"}}}"#,
            "fb",
        )
        .is_none());
        let result = decode_grok_transcript_line(
            r#"{"timestamp":1787360353,"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"c1","status":"completed","content":[{"type":"content","content":{"type":"text","text":"file body"}}]}}}"#,
            "fb",
        )
        .expect("tool result");
        assert_eq!(
            result.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "file body".to_string(),
                is_error: None,
            }],
        );
        let failed = decode_grok_transcript_line(
            r#"{"timestamp":1787360353,"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"c2","status":"failed","rawOutput":{"Fetch":{"tool_output_for_prompt":"fetch failed"}}}}}"#,
            "fb",
        )
        .expect("failed tool result");
        assert_eq!(
            failed.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "fetch failed".to_string(),
                is_error: Some(true),
            }],
        );
    }

    #[test]
    fn grok_lifecycle_opens_on_the_prompt_and_settles_on_turn_completed() {
        let working = decode_grok_turn_lifecycle(
            r#"{"timestamp":1787360343,"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"go"}}}}"#,
            "fb",
        )
        .expect("working");
        assert_eq!(working.state, SessionChatTurnLifecycleState::Working);
        let completed = decode_grok_turn_lifecycle(
            r#"{"timestamp":1787360387,"method":"_x.ai/session/update","params":{"sessionId":"s","update":{"sessionUpdate":"turn_completed","prompt_id":"p1","stop_reason":"end_turn"}}}"#,
            "fb",
        )
        .expect("completed");
        assert_eq!(completed.state, SessionChatTurnLifecycleState::Completed);
        assert_eq!(completed.turn_id, "p1");
        let cancelled = decode_grok_turn_lifecycle(
            r#"{"timestamp":1787360387,"method":"_x.ai/session/update","params":{"sessionId":"s","update":{"sessionUpdate":"turn_completed","prompt_id":"p2","stop_reason":"cancelled"}}}"#,
            "fb",
        )
        .expect("cancelled");
        assert_eq!(cancelled.state, SessionChatTurnLifecycleState::Interrupted);
        // Tool traffic is not a boundary.
        assert!(decode_grok_turn_lifecycle(
            r#"{"timestamp":1787360352,"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"done"}}}}"#,
            "fb",
        )
        .is_none());
    }

    #[test]
    fn pi_decoder_maps_shared_chat_blocks() {
        let assistant = decode_pi_transcript_line(
            r#"{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-08-03T10:00:00.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"private plan"},{"type":"text","text":"I will inspect it."},{"type":"toolCall","id":"call-1","name":"read","arguments":{"path":"/tmp/example"}}]}}"#,
            "fb",
        )
        .expect("assistant");
        assert_eq!(assistant.role, SessionChatRole::Assistant);
        assert_eq!(assistant.id, "a1");
        assert_eq!(
            assistant.blocks,
            vec![
                text_block("I will inspect it."),
                SessionChatBlock::ToolCall {
                    name: "read".to_string(),
                    input: json!({"path": "/tmp/example"}),
                },
            ]
        );

        let result = decode_pi_transcript_line(
            r#"{"type":"message","id":"t1","parentId":"a1","message":{"role":"toolResult","toolCallId":"call-1","toolName":"read","content":[{"type":"text","text":"contents"}],"isError":false}}"#,
            "fb",
        )
        .expect("tool result");
        assert_eq!(result.role, SessionChatRole::Tool);
        assert_eq!(
            result.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "contents".to_string(),
                is_error: None,
            }]
        );
    }

    #[test]
    fn pi_tail_reader_follows_the_current_branch() {
        let path = write_temp_transcript(&[
            r#"{"type":"session","version":3,"id":"session-1","timestamp":"2026-08-03T10:00:00.000Z","cwd":"/tmp"}"#,
            r#"{"type":"message","id":"u1","parentId":null,"message":{"role":"user","content":[{"type":"text","text":"root"}]}}"#,
            r#"{"type":"message","id":"a-abandoned","parentId":"u1","message":{"role":"assistant","content":[{"type":"text","text":"old branch"}]}}"#,
            r#"{"type":"message","id":"a-current","parentId":"u1","message":{"role":"assistant","content":[{"type":"text","text":"current branch"}]}}"#,
        ]);
        let tail = read_session_chat_tail_page(SessionChatTranscriptAgent::Pi, &path, 300, None)
            .expect("Pi tail");
        let SessionChatTailPage::Page { messages, .. } = tail else {
            panic!("expected Pi transcript page");
        };
        assert_eq!(
            messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["u1", "a-current"]
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn prompt_derivation_matches_canonical_shapes() {
        // AskUserQuestion tool input on a pre-tool event → question card.
        let tool_input = json!({
            "questions": [
                {
                    "question": "Which approach?",
                    "header": "Approach",
                    "multiSelect": false,
                    "options": [
                        {"label": "Fast", "description": "quick"},
                        "Careful",
                    ],
                },
            ],
        });
        let question = derive_session_chat_prompt(
            Some("ask_user_question"),
            Some(&tool_input),
            Some("PreToolUse"),
        )
        .expect("question derives");
        assert_eq!(
            serde_json::to_value(&question).expect("serialize"),
            json!({
                "kind": "question",
                "questions": [
                    {
                        "question": "Which approach?",
                        "header": "Approach",
                        "multiSelect": false,
                        "toolName": "ask_user_question",
                        "options": [
                            {"label": "Fast", "description": "quick"},
                            {"label": "Careful"},
                        ],
                    },
                ],
            })
        );

        // The SAME tool on a post-tool event derives nothing (card would
        // linger after answering otherwise).
        assert!(derive_session_chat_prompt(
            Some("AskUserQuestion"),
            Some(&tool_input),
            Some("PostToolUse"),
        )
        .is_none());
        // Codex 0.145 spelling matches too.
        assert!(derive_session_chat_prompt(
            Some("request_user_input"),
            Some(&tool_input),
            Some("PreToolUse"),
        )
        .is_some());

        // PermissionRequest + tool name → approval with direct-field summary.
        let approval = derive_session_chat_prompt(
            Some("Bash"),
            Some(&json!({"command": "rm -rf /tmp/x", "description": "cleanup"})),
            Some("PermissionRequest"),
        )
        .expect("approval derives");
        assert_eq!(
            serde_json::to_value(&approval).expect("serialize"),
            json!({"kind": "approval", "tool": "Bash", "summary": "rm -rf /tmp/x"})
        );
        // Non-string direct field falls back to the JSON body, capped at 200.
        let long = "x".repeat(300);
        let capped = derive_session_chat_prompt(
            Some("Write"),
            Some(&json!({"file_path": 42, "content": long})),
            Some("PermissionRequest"),
        )
        .expect("capped approval");
        if let SessionChatInteractivePrompt::Approval { summary, .. } = &capped {
            let summary = summary.as_deref().expect("summary present");
            assert_eq!(summary.chars().count(), 201);
            assert!(summary.ends_with('\u{2026}'));
        } else {
            panic!("expected approval");
        }

        // Unrelated tools/events derive nothing.
        assert!(
            derive_session_chat_prompt(Some("Bash"), Some(&tool_input), Some("PreToolUse"))
                .is_none()
        );
        assert!(derive_session_chat_prompt(None, None, Some("PermissionRequest")).is_none());

        // Stored round trip: wire JSON parses back to the same prompt.
        let stored = serde_json::to_string(&question).expect("stringify");
        assert_eq!(parse_stored_session_chat_prompt(&stored), Some(question));

        // Clear rules: post-tool events and Stop/SessionEnd/idle transitions.
        assert!(should_clear_session_chat_prompt(Some("PostToolUse"), None));
        assert!(should_clear_session_chat_prompt(
            Some("post_tool_use_failure"),
            Some("working"),
        ));
        assert!(should_clear_session_chat_prompt(Some("Stop"), Some("idle")));
        assert!(should_clear_session_chat_prompt(Some("SessionEnd"), None));
        assert!(should_clear_session_chat_prompt(
            Some("Notification"),
            Some("idle"),
        ));
        assert!(!should_clear_session_chat_prompt(
            Some("PreToolUse"),
            Some("working"),
        ));
        assert!(!should_clear_session_chat_prompt(
            Some("PermissionRequest"),
            Some("attention"),
        ));
    }

    #[test]
    fn question_parsing_follows_canonical_shape_rules() {
        // Strict multiSelect === true; strings and label objects both parse;
        // malformed options drop; question objects need text OR options.
        let parsed = parse_session_chat_questions(
            None,
            &json!({
                "questions": [
                    {"question": "Q1", "multiSelect": true, "options": ["A", {"label": "B"}, 7]},
                    {"question": "", "options": []},
                    {"question": "Q2", "multiSelect": "yes"},
                ],
            }),
        )
        .expect("questions parse");
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0].multi_select);
        assert_eq!(
            parsed[0]
                .options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "B"]
        );
        assert!(!parsed[1].multi_select);
        assert!(parsed[1].options.is_empty());
        assert!(parse_session_chat_questions(None, &json!({"questions": []})).is_none());
        assert!(parse_session_chat_questions(None, &json!({"notQuestions": true})).is_none());
    }

    fn write_temp_transcript(lines: &[&str]) -> PathBuf {
        // Tests run in parallel: the name must be unique per CALL, not per
        // line count, or two tests share (and then delete) one file.
        static NEXT_TEMP_TRANSCRIPT: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "gxserver-session-chat-test-{}-{}.jsonl",
            std::process::id(),
            NEXT_TEMP_TRANSCRIPT.fetch_add(1, Ordering::SeqCst),
        ));
        let mut file = File::create(&path).expect("create temp transcript");
        for line in lines {
            writeln!(file, "{line}").expect("write line");
        }
        path
    }

    #[test]
    fn tail_reader_limit_and_has_more_semantics() {
        let lines: Vec<String> = (0..10)
            .map(|index| {
                format!(
                    r#"{{"type":"user","uuid":"u{index}","message":{{"role":"user","content":"prompt {index}"}}}}"#
                )
            })
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let path = write_temp_transcript(&line_refs);

        let all = read_session_chat_transcript_tail_file(
            &path,
            300,
            decode_claude_transcript_line,
            true,
            None,
            Some(decode_claude_turn_lifecycle),
            Some(claude_transcript_lineage),
        )
        .expect("tail read");
        assert_eq!(all.messages.len(), 10);
        assert!(!all.has_more);
        assert_eq!(all.before_offset, 0);
        assert_eq!(
            all.lifecycle.as_ref().map(|lifecycle| lifecycle.state),
            Some(SessionChatTurnLifecycleState::Working)
        );

        let limited = read_session_chat_transcript_tail_file(
            &path,
            3,
            decode_claude_transcript_line,
            true,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("limited read");
        assert_eq!(limited.messages.len(), 3);
        assert!(limited.has_more);
        assert_eq!(
            limited.messages.last().map(|message| message.id.as_str()),
            Some("u9")
        );

        // limit 0 ⇒ [] (never "everything").
        let zero = read_session_chat_transcript_tail_file(
            &path,
            0,
            decode_claude_transcript_line,
            true,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("zero read");
        assert!(zero.messages.is_empty());
        assert!(!zero.has_more);

        // Paging by beforeOffset excludes the newer window.
        let older = read_session_chat_transcript_tail_file(
            &path,
            300,
            decode_claude_transcript_line,
            true,
            Some(limited.before_offset),
            None,
            Some(claude_transcript_lineage),
        )
        .expect("older page");
        assert_eq!(older.messages.len(), 7);
        assert_eq!(
            older.messages.last().map(|message| message.id.as_str()),
            Some("u6")
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn incremental_reader_resumes_from_offset() {
        let path = write_temp_transcript(&[
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"one"}}"#,
        ]);
        let mut state = SessionChatIncrementalState::new();
        let first = read_incremental_transcript_messages(
            &path,
            &mut state,
            decode_claude_transcript_line,
            None,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("first pass");
        assert_eq!(first.len(), 1);
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("append");
        writeln!(
            file,
            r#"{{"type":"assistant","uuid":"a1","message":{{"role":"assistant","content":[{{"type":"text","text":"two"}}]}}}}"#
        )
        .expect("append line");
        drop(file);
        let second = read_incremental_transcript_messages(
            &path,
            &mut state,
            decode_claude_transcript_line,
            None,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("second pass");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].id, "a1");
        let _ = fs::remove_file(&path);
    }

    /*
    Real-data sanity checks: decode this machine's actual transcripts when
    they exist and assert non-trivial role coverage. Skipped silently on
    machines without the files.
    */
    #[test]
    fn decodes_real_claude_transcript_when_present() {
        let path = home_dir()
            .join(".claude/projects/-Users-madda-dev--active-Ghostex")
            .join("a3aadc10-7b82-417a-96de-e5ca56ae2e3e.jsonl");
        if !path.is_file() {
            return;
        }
        let result = read_session_chat_transcript_tail_file(
            &path,
            100_000,
            decode_claude_transcript_line,
            true,
            None,
            Some(decode_claude_turn_lifecycle),
            Some(claude_transcript_lineage),
        )
        .expect("read real claude transcript");
        let users = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::User)
            .count();
        let assistants = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Assistant)
            .count();
        let tools = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Tool)
            .count();
        eprintln!(
            "real claude transcript: {} messages ({} user / {} assistant / {} tool), lifecycle: {:?}, malformed {} oversized {}",
            result.messages.len(),
            users,
            assistants,
            tools,
            result.lifecycle.as_ref().map(|lifecycle| lifecycle.state),
            result.malformed_record_count,
            result.oversized_record_count,
        );
        assert!(users > 0, "expected user messages, got {users}");
        assert!(
            assistants > 0,
            "expected assistant messages, got {assistants}"
        );
        assert!(tools > 0, "expected tool messages, got {tools}");
    }

    #[test]
    fn decodes_real_codex_transcript_when_present() {
        let root = home_dir().join(".codex/sessions/2026");
        if !root.is_dir() {
            return;
        }
        let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
        let mut stack = vec![root];
        while let Some(current) = stack.pop() {
            let Ok(entries) = fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
                    continue;
                }
                let modified = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                if newest
                    .as_ref()
                    .map(|(_, newest_modified)| modified > *newest_modified)
                    .unwrap_or(true)
                {
                    newest = Some((path, modified));
                }
            }
        }
        let Some((path, _)) = newest else {
            return;
        };
        let result = read_session_chat_transcript_tail_file(
            &path,
            100_000,
            decode_codex_transcript_line,
            true,
            None,
            Some(decode_codex_turn_lifecycle),
            None,
        )
        .expect("read real codex transcript");
        let users = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::User)
            .count();
        let tools = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Tool)
            .count();
        let assistants = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Assistant)
            .count();
        let reasoning = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Reasoning)
            .count();
        eprintln!(
            "real codex transcript {}: {} messages ({} user / {} assistant / {} reasoning / {} tool), lifecycle: {:?}",
            path.display(),
            result.messages.len(),
            users,
            assistants,
            reasoning,
            tools,
            result.lifecycle.as_ref().map(|lifecycle| lifecycle.state),
        );
        assert!(
            users > 0,
            "expected user messages in {}, got {users}",
            path.display()
        );
        assert!(
            tools > 0,
            "expected tool outputs in {}, got {tools}",
            path.display()
        );
    }

    /*
    Live-files probe for the 2026-08-02 repro: Ghostex session G1ipk still stores
    `agentSessionId = f8ba5a62…` while the pane runs the background-job
    continuation `fb7572ef…`. The directory also holds a SECOND continuation of
    the same predecessor (`d34a3f3c…`, last substantive record 2026-08-01) plus
    ~180 unrelated transcripts, so this asserts the detector picks exactly the
    live one. Skipped on machines without those files.
    */
    #[test]
    fn real_stale_session_resolves_to_its_live_successor_when_present() {
        let stale_session_id = "f8ba5a62-94b4-410f-ba6e-4f462a3e55bc";
        let expected_successor_id = "fb7572ef-2965-4e5e-b21e-bb0e3c455b66";
        let stale_path = home_dir()
            .join(".claude/projects/-Users-madda-dev--active-Ghostex")
            .join(format!("{stale_session_id}.jsonl"));
        let successor_path = stale_path.with_file_name(format!("{expected_successor_id}.jsonl"));
        if !stale_path.is_file() || !successor_path.is_file() {
            return;
        }
        let stale_last_substantive_ms = last_substantive_transcript_timestamp_ms(&stale_path)
            .expect("stale transcript has a substantive record");
        let successor_last_substantive_ms =
            last_substantive_transcript_timestamp_ms(&successor_path)
                .expect("successor transcript has a substantive record");
        eprintln!(
            "stale last substantive {stale_last_substantive_ms}, successor last substantive {successor_last_substantive_ms}"
        );
        // Live-files drift guard (2026-08-07): the "stale" transcript keeps
        // receiving fresh substantive records on this machine, so the
        // staleness premise can expire. Never freeze relationships between
        // live files in tests — skip when the repro state is gone.
        if successor_last_substantive_ms <= stale_last_substantive_ms {
            eprintln!("skipping: stale transcript is no longer stale on this machine");
            return;
        }
        match find_claude_successor_transcript(
            stale_session_id,
            &stale_path,
            stale_last_substantive_ms,
            &[],
        ) {
            SessionChatSuccessorOutcome::Found(successor) => {
                eprintln!(
                    "real successor: {} via {} ({} hop(s))",
                    successor.agent_session_id,
                    successor.lineage.as_str(),
                    successor.hops
                );
                assert_eq!(successor.agent_session_id, expected_successor_id);
                assert_eq!(successor.path, successor_path);
            }
            other => panic!("expected the live successor, got {other:?}"),
        }
        /*
        Bound to another registry session ⇒ never adopted. These are LIVE
        transcripts that keep moving: the second real continuation
        (d34a3f3c…) participates as a fallback candidate only while it is
        still substantively newer than the stale transcript, so the
        drift-dependent shape is asserted conditionally (on 2026-08-02 the
        stale file gained a fresher substantive record and the fallback aged
        out of candidacy).
        */
        let second_continuation_id = "d34a3f3c-3947-4053-8811-af237c1ae288";
        let second_continuation_path =
            stale_path.with_file_name(format!("{second_continuation_id}.jsonl"));
        let second_continuation_ms =
            last_substantive_transcript_timestamp_ms(&second_continuation_path)
                .expect("second continuation has a substantive record");
        if second_continuation_ms > stale_last_substantive_ms {
            assert_eq!(
                find_claude_successor_transcript(
                    stale_session_id,
                    &stale_path,
                    stale_last_substantive_ms,
                    &[expected_successor_id.to_string()],
                ),
                SessionChatSuccessorOutcome::Found(SessionChatSuccessorTranscript {
                    agent_session_id: second_continuation_id.to_string(),
                    path: second_continuation_path.clone(),
                    lineage: SessionChatSuccessorLineage::PredecessorIdField,
                    last_substantive_ms: second_continuation_ms,
                    hops: 1,
                }),
                "excluding the live successor must fall to the other proven continuation, never to an unrelated file",
            );
            /*
            The 2026-08-02 runtime failure in one assertion: BOTH real
            continuations are also carried by long-STOPPED registry rows
            (G7vq1 / G1z1z here). Feeding those in as owners is what produced
            a silent no-op, which is why the ownership list must contain
            active sessions only — and why this case now reports
            OwnedByAnotherSession instead of NotFound.
            */
            assert_eq!(
                find_claude_successor_transcript(
                    stale_session_id,
                    &stale_path,
                    stale_last_substantive_ms,
                    &[
                        expected_successor_id.to_string(),
                        second_continuation_id.to_string(),
                    ],
                ),
                SessionChatSuccessorOutcome::OwnedByAnotherSession {
                    candidate_session_ids: vec![
                        expected_successor_id.to_string(),
                        second_continuation_id.to_string(),
                    ],
                },
            );
        } else {
            // Drifted world: with the fallback aged out, excluding the live
            // successor must leave no adoptable candidate — the exclusion
            // itself is the invariant under test.
            assert_eq!(
                find_claude_successor_transcript(
                    stale_session_id,
                    &stale_path,
                    stale_last_substantive_ms,
                    &[expected_successor_id.to_string()],
                ),
                SessionChatSuccessorOutcome::OwnedByAnotherSession {
                    candidate_session_ids: vec![expected_successor_id.to_string()],
                },
            );
        }
    }
}
