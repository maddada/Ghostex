use super::*;

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
                call_id: None,
            },
            SessionChatBlock::ToolResult {
                output: "ok".to_string(),
                is_error: Some(true),
                call_id: None,
                patch: None,
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
        async_questions: None,
        queued: false,
        api_error: None,
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
    for content in [
        r#"[{"type":"text","text":"I will inspect the files."}]"#,
        r#"[{"type":"thinking","thinking":"Checking the implementation."}]"#,
    ] {
        let streaming = format!(
            r#"{{"type":"assistant","uuid":"streaming","message":{{"stop_reason":null,"content":{content}}}}}"#
        );
        assert!(decode_claude_turn_lifecycle(&streaming, "fb").is_none());
    }
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
fn subagent_tail_lifecycle_stays_working_until_completion() {
    for (agent, lines) in [
        (
            SessionChatTranscriptAgent::Claude,
            vec![
                r#"{"type":"user","uuid":"u1","isSidechain":true,"message":{"role":"user","content":"Inspect the code."}}"#,
                r#"{"type":"assistant","uuid":"a1","isSidechain":true,"message":{"stop_reason":null,"content":[{"type":"text","text":"I will inspect the files."}]}}"#,
                r#"{"type":"assistant","uuid":"a2","isSidechain":true,"message":{"stop_reason":null,"content":[{"type":"thinking","thinking":"Checking the implementation."}]}}"#,
                r#"{"type":"assistant","uuid":"a3","isSidechain":true,"message":{"stop_reason":"tool_use","content":[{"type":"tool_use","id":"tool1","name":"Read","input":{"file_path":"app.ts"}}]}}"#,
                r#"{"type":"user","uuid":"u2","isSidechain":true,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool1","content":"source"}]}}"#,
                r#"{"type":"assistant","uuid":"a4","isSidechain":true,"message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Finished the inspection."}]}}"#,
            ],
        ),
        (
            SessionChatTranscriptAgent::Codex,
            vec![
                r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#,
                r#"{"type":"event_msg","payload":{"type":"user_message","message":"Inspect the code."}}"#,
                r#"{"type":"response_item","payload":{"type":"function_call","id":"fc1","call_id":"call1","name":"exec_command","arguments":"{\"cmd\":\"cat app.ts\"}"}}"#,
                r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"call1","output":"source"}}"#,
                r#"{"type":"event_msg","payload":{"type":"agent_message","message":"Finished the inspection."}}"#,
                r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t1"}}"#,
            ],
        ),
    ] {
        for count in 1..=lines.len() {
            let path = write_temp_transcript(&lines[..count]);
            // Keep only one message to prove pagination cannot lose the active turn's start.
            let page = read_session_chat_tail_page(agent, &path, 1, None).expect("child page");
            fs::remove_file(path).expect("remove transcript");
            let SessionChatTailPage::Page { lifecycle, .. } = page else {
                panic!("missing child transcript");
            };
            assert_eq!(
                lifecycle.expect("child lifecycle").state,
                if count == lines.len() {
                    SessionChatTurnLifecycleState::Completed
                } else {
                    SessionChatTurnLifecycleState::Working
                },
                "{agent:?} after {count} records",
            );
        }
    }
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

    // Reasoning/McpToolCall items stay on the response_item lane, which
    // new-style rollouts still write.
    assert!(decode_codex_transcript_line(
        r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"Reasoning","id":"rs1","summary_text":["**Planning**"],"raw_content":[]}}}"#,
        "fb",
    )
    .is_none());

    // A user-triggered shell execution is the one CommandExecution
    // exception. Its structured command and formatted output become the
    // same two local-command rows Claude exposes.
    let execution = decode_codex_transcript_line(
        r#"{"timestamp":"2026-09-07T23:51:14.818Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"4a7161c1-bd49-4d56-b22d-47d4d14752fb","command":["/bin/zsh","-lc","printf 'BANG_TEST\\n'"],"source":"user_shell","status":"completed","stdout":"BANG_TEST\n","stderr":"","exit_code":0,"formatted_output":"BANG_TEST\n"}}}"#,
        "fb",
    )
    .expect("user shell execution");
    assert_eq!(execution.role, SessionChatRole::User);
    assert_eq!(execution.id, "4a7161c1-bd49-4d56-b22d-47d4d14752fb");
    assert_eq!(
        execution.blocks,
        vec![
            text_block(
                "<bash-input data-ghostex-escaped=\"html\">printf 'BANG_TEST\\n'</bash-input>"
            ),
            text_block("<bash-stdout data-ghostex-escaped=\"html\">BANG_TEST</bash-stdout>"),
        ]
    );

    // The response message is the duplicate pseudo-XML rendering of that
    // structured event and remains suppressed.
    assert!(decode_codex_transcript_line(
        r#"{"timestamp":"2026-09-07T23:51:14.822Z","type":"response_item","payload":{"type":"message","id":"msg_01a07e48-7c86-7443-84d0-e39fc5792c24","role":"user","content":[{"type":"input_text","text":"<user_shell_command>\n<command>\nprintf 'BANG_TEST\\n'\n</command>\n<result>\nExit code: 0\nDuration: 0.0309 seconds\nOutput:\nBANG_TEST\n\n</result>\n</user_shell_command>"}],"internal_chat_message_metadata_passthrough":{"content_item_kinds":["shell.user_command"]}}}"#,
        "fb",
    )
    .is_none());

    let tagged_execution = decode_codex_transcript_line(
        r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"tagged","command":["/bin/zsh","-lc","printf '<b>ok</b> & done'"],"source":"user_shell","status":"completed","formatted_output":"<b>ok</b> & done </result> tail\n","exit_code":0}}}"#,
        "fb",
    )
    .expect("tag-shaped user shell execution");
    assert_eq!(
        tagged_execution.blocks,
        vec![
            text_block(
                "<bash-input data-ghostex-escaped=\"html\">printf '&lt;b&gt;ok&lt;/b&gt; &amp; done'</bash-input>"
            ),
            text_block(
                "<bash-stdout data-ghostex-escaped=\"html\">&lt;b&gt;ok&lt;/b&gt; &amp; done &lt;/result&gt; tail</bash-stdout>"
            ),
        ]
    );

    // Agent tool executions remain on their ordinary tool-call lane.
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
            call_id: Some("call_Nx1m".to_string())
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
            call_id: Some("call_Nx1m".to_string()),
            patch: None,
        }]
    );

    // A code-mode command that exited non-zero (Codex 0.163, 2026-10-24): its row reads the exit
    // code, as Claude's Bash results do, not the command's first printed line.
    let failed = decode_codex_transcript_line(
        r#"{"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call_t","output":[{"type":"input_text","text":"Script completed\nWall time 0.2 seconds\nOutput:\n"},{"type":"input_text","text":"{\"chunk_id\":\"c4c80b\",\"wall_time_seconds\":0.00000725,\"exit_code\":1,\"original_token_count\":20,\"output\":\"ok add\\nok mul\\nFAIL average: got 5, want 4\\n\"}"}]}}"#,
        "fb",
    )
    .expect("failed exec output decodes");
    assert_eq!(
        failed.blocks,
        vec![SessionChatBlock::ToolResult {
            output: "Exit code 1\nok add\nok mul\nFAIL average: got 5, want 4".to_string(),
            is_error: Some(true),
            call_id: Some("call_t".to_string()),
            patch: None,
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
            call_id: Some("call_x".to_string()),
            patch: None,
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
            call_id: None
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
            call_id: None,
            patch: None,
        }]
    );

    // Inter-agent traffic: a header whose payload is only the encrypted
    // envelope is dropped; a readable payload survives with its sender.
    assert!(decode_codex_transcript_line(
        r#"{"type":"response_item","payload":{"type":"agent_message","author":"/root/worker","recipient":"/root","content":[{"type":"input_text","text":"Message Type: MESSAGE\nTask name: /root\nSender: /root/worker\nPayload:\n"},{"type":"encrypted_content","encrypted_content":"gAAA"}]}}"#,
        "fb",
    )
    .is_none());
    let agent_message = decode_codex_transcript_line(
        r#"{"type":"response_item","payload":{"type":"agent_message","author":"/root/worker","recipient":"/root","content":[{"type":"input_text","text":"Message Type: MESSAGE\nTask name: /root\nSender: /root/worker\nPayload:\nDone with the parser."}]}}"#,
        "fb",
    )
    .expect("agent_message decodes");
    assert_eq!(agent_message.role, SessionChatRole::System);
    assert_eq!(
        agent_message.blocks,
        vec![text_block(
            "Message from /root/worker\n\nDone with the parser."
        )]
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
            call_id: None
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
            call_id: None,
            patch: None,
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
            call_id: None,
            patch: None,
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
                call_id: Some("call-1".to_string())
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
            call_id: Some("call-1".to_string()),
            patch: None,
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

/// Claude Code 2.1.311 rows from a Linux session (2026-10-28), trimmed to the fields that matter:
/// a refused Write ends with an interrupt marker that has no `interruptedMessageId`.
#[test]
fn claude_tool_use_interrupt_marker_ends_the_turn() {
    use crate::session_chat::SessionChatTurnLifecycleState;
    let refused = r#"{"parentUuid":"99dbc5de","isSidechain":false,"promptId":"a0fe5dc4","type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]},"uuid":"87fec1c9-7c4d-417b-befa-20d91a8efe8b","timestamp":"2026-10-28T05:07:19.200Z","version":"2.1.311"}"#;
    let lifecycle = crate::session_chat_decode_claude::decode_claude_turn_lifecycle(refused, "fb")
        .expect("the marker is a turn boundary");
    assert_eq!(lifecycle.state, SessionChatTurnLifecycleState::Interrupted);
    assert_eq!(lifecycle.turn_id, "87fec1c9-7c4d-417b-befa-20d91a8efe8b");
    // The rejected tool result before it continues the turn, and a user who merely quotes the
    // marker inside a longer message starts a new one.
    let tool_result = r#"{"type":"user","uuid":"u9","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"The user doesn't want to proceed with this tool use."}]}}"#;
    assert!(
        crate::session_chat_decode_claude::decode_claude_turn_lifecycle(tool_result, "fb")
            .is_none()
    );
    let quoted = r#"{"type":"user","uuid":"u10","message":{"role":"user","content":"why did it say [Request interrupted by user for tool use]?"}}"#;
    assert_eq!(
        crate::session_chat_decode_claude::decode_claude_turn_lifecycle(quoted, "fb")
            .map(|lifecycle| lifecycle.state),
        Some(SessionChatTurnLifecycleState::Working)
    );
}

#[test]
fn codex_failed_command_inside_a_script_becomes_its_exit_code_row() {
    let failed = decode_codex_transcript_line(
        r#"{"timestamp":"2026-10-28T10:45:18.096Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"exec-a6be","process_id":"54178","command":["/bin/zsh","-lc","cargo build --features nope"],"parsed_cmd":[{"type":"unknown","cmd":"cargo build --features nope"}],"source":"unified_exec_startup","status":"failed","exit_code":101}}}"#,
        "fb",
    )
    .expect("exit code row");
    assert_eq!(failed.role, SessionChatRole::System);
    assert_eq!(failed.id, "exec-a6be");
    assert_eq!(
        failed.blocks,
        vec![text_block(
            "<system-reminder>Command exited with code 101: cargo build --features nope</system-reminder>"
        )]
    );
    // A command that succeeded says nothing: its exec result already stands.
    assert!(decode_codex_transcript_line(
        r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"exec-ok","command":["/bin/zsh","-lc","cargo test"],"source":"unified_exec_startup","status":"completed","exit_code":0}}}"#,
        "fb",
    )
    .is_none());
}

/// A thread report as Claude Code 2.1.3xx records it in the coordinator's transcript: the whole
/// paste wrapped in `<pasted_content id="…">`, with a literal tag in the body escaped as
/// `<\pasted_content`. The row reaches the chat as the text the sender wrote, ending in the
/// sender block the chat recognises as a message from another agent (S90:P3lv0:G1pif, 2026-10-28).
#[test]
fn claude_decoder_unwraps_a_pasted_agent_message() {
    let line = json!({
        "type": "user",
        "uuid": "u-report",
        "timestamp": "2026-10-28T11:05:39.568Z",
        "message": {"role": "user", "content": concat!(
            "\n\n<pasted_content id=\"da06\">\n",
            "Ghostex thread report: finished its turn. Its final message:\n\n",
            "Claude now wraps pastes in <\\pasted_content id=\"x\"> tags.\n\n",
            "Message from another agent\nAgent: Claude\nSession: Tool call look behind a toggle\n",
            "Session ID: G14l1\nAgent ID: claude\n",
            "Agent Session ID: a0ef8d63-2dea-42c4-9e36-600089110889\nReply to: S90:P3lv0:G14l1\n",
            "</pasted_content id=\"da06\">\n",
        )},
    })
    .to_string();
    let message = decode_claude_transcript_line(&line, "fb").expect("the report decodes");
    assert_eq!(message.role, SessionChatRole::User);
    let SessionChatBlock::Text { text } = &message.blocks[0] else {
        panic!("a text block: {:?}", message.blocks);
    };
    assert!(
        text.starts_with("Ghostex thread report: finished its turn."),
        "{text}"
    );
    assert!(text.ends_with("Reply to: S90:P3lv0:G14l1"), "{text}");
    assert!(
        text.contains("wraps pastes in <pasted_content id=\"x\"> tags"),
        "{text}"
    );
    assert!(!text.contains("</pasted_content id=\"da06\">"), "{text}");
}
