use super::*;

#[test]
fn transcript_prompt_state_detects_and_retires_question_cards() {
    let ask = |input: Value| SessionChatMessage {
        id: "ask".to_string(),
        role: SessionChatRole::Assistant,
        blocks: vec![SessionChatBlock::ToolCall {
            name: "AskUserQuestion".to_string(),
            input,
            call_id: None,
        }],
        timestamp: None,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
        custom_message: None,
    };
    let result = SessionChatMessage {
        id: "res".to_string(),
        role: SessionChatRole::Tool,
        blocks: vec![SessionChatBlock::ToolResult {
            output: "Fast".to_string(),
            is_error: None,
            call_id: None,
            patch: None,
        }],
        timestamp: None,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
        custom_message: None,
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
                preview_layout: false,
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

    let codex_call = decode_codex_transcript_line(
        &json!({
            "type": "response_item",
            "payload": {
                "type": "function_call",
                "namespace": "functions",
                "name": "request_user_input",
                "call_id": "regular-question",
                "arguments": input.to_string(),
            },
        })
        .to_string(),
        "codex-question",
    )
    .expect("regular Codex call decodes");
    assert!(codex_call.async_questions.is_none());
    let mut codex_state = scan_transcript_prompt_state(&[codex_call]);
    let codex_prompt = resolve_session_chat_prompt(None, &codex_state)
        .expect("string arguments produce a regular question card without hooks");
    assert!(matches!(
        codex_prompt,
        SessionChatInteractivePrompt::Question { ref questions, .. }
            if questions[0].question == "Which approach?"
                && questions[0].tool_name.as_deref() == Some("request_user_input")
                && questions[0].options.len() == 2
    ));
    let codex_result = decode_codex_transcript_line(
        r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"regular-question","output":"{\"answers\":{\"approach\":{\"answers\":[\"Fast\"]}}}"}}"#,
        "codex-answer",
    ).expect("regular Codex result decodes");
    codex_state.advance(&[codex_result]);
    assert!(resolve_session_chat_prompt(Some(codex_prompt), &codex_state).is_none());
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
            call_id: None,
        }],
        timestamp: None,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
        custom_message: None,
    };
    let result = SessionChatMessage {
        id: "res-old".to_string(),
        role: SessionChatRole::Tool,
        blocks: vec![SessionChatBlock::ToolResult {
            output: "Red".to_string(),
            is_error: None,
            call_id: None,
            patch: None,
        }],
        timestamp: None,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
        custom_message: None,
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
            preview_layout: false,
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
            preview_layout: false,
            options: Vec::new(),
        }],
        tool_use_id: None,
    };
    assert!(resolve_session_chat_prompt(Some(same_stored), &transcript).is_none());
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
        derive_session_chat_prompt(Some("Bash"), Some(&tool_input), Some("PreToolUse")).is_none()
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
    assert!(parse_session_chat_questions(Some("request_user_input"), &json!("{broken")).is_none());
    assert!(parse_session_chat_questions(Some("request_user_input"), &json!("null")).is_none());
}
