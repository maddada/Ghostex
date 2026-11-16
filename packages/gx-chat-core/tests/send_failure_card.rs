//! A failed send's card (server/src/session_chat_send_failure.rs) as the chat core reads it: it
//! stops the spinner, its row gives way to it, and closing it tells gxserver. And a mode the session
//! cannot use greys its menu row with how to turn it on (server/src/session_chat_mode_availability.rs).

use ghostex_gx_chat_core::questions::actions::handle as handle_action;
use ghostex_gx_chat_core::questions::document::document as question_document;
use ghostex_gx_chat_core::questions::gates::notice_visible;
use ghostex_gx_chat_core::session::working::is_working;

use ghostex_gx_chat_core::{ActionKind, ChatContext, ChatRpcMethod, ChatState, Effect, UserAction};
use serde_json::{json, Value};

fn card(prompt_id: &str) -> Value {
    json!({
        "kind": "deliveryFailed",
        "severity": "error",
        "title": "Your message was not sent",
        "detail": "The agent stayed in the middle of a turn for five minutes without taking new input, so the message was not sent.",
        "source": "watchdog",
        "detectedAt": "2026-10-31T18:02:00.000Z",
        "sendFailure": { "promptId": prompt_id },
        "actions": [
            { "id": "retrySend", "label": "Retry", "kind": "restartAgent" },
            { "id": "switchToTerminal", "label": "Open terminal", "kind": "switchToTerminal" },
            { "id": "fixSend", "label": "Fix it", "kind": "restartAgent" },
        ],
    })
}

fn context() -> ChatContext {
    ChatContext {
        now_ms: 1_793_468_000_000.0,
        ..Default::default()
    }
}

fn state_with_card() -> ChatState {
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state.session.terminal_notice = Some(card("Q1"));
    state
}

fn card_prompt_id(state: &ChatState) -> Option<String> {
    let mut document = Default::default();
    question_document(state, &context(), &mut document);
    document.send_failure_card_prompt_id
}

#[test]
fn the_row_gives_way_to_its_card_only_while_the_card_shows() {
    let state = state_with_card();
    assert!(notice_visible(&state));
    assert_eq!(card_prompt_id(&state).as_deref(), Some("Q1"));
}

#[test]
fn every_button_rides_the_answer_every_client_already_sends() {
    let state = state_with_card();
    let mut document = Default::default();
    question_document(&state, &context(), &mut document);
    let notice = match &document.terminal_notice {
        ghostex_gx_protocol::Tri::Value(value) => value.clone(),
        _ => panic!("the card is on the document"),
    };
    let answers: Vec<Value> = notice["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action["answer"].clone())
        .collect();
    assert_eq!(
        answers[0],
        json!({ "kind": "restartAgent", "actionId": "retrySend" })
    );
    assert_eq!(
        answers[2],
        json!({ "kind": "restartAgent", "actionId": "fixSend" })
    );
}

#[test]
fn closing_the_card_tells_gxserver_and_a_row_keeps_its_own_buttons_after() {
    let mut state = state_with_card();
    let effects = handle_action(
        &mut state,
        &UserAction::new(ActionKind::DismissNotice),
        &context(),
    );
    let answer = effects.iter().find_map(|effect| match effect {
        Effect::SendRpc { method, params, .. }
            if *method == ChatRpcMethod::AnswerSessionChatPrompt =>
        {
            Some(params.clone())
        }
        _ => None,
    });
    let answer = answer.expect("the dismissal reaches gxserver");
    assert_eq!(answer["actionId"], "dismissSendFailure");
    assert!(!notice_visible(&state));
    assert_eq!(card_prompt_id(&state), None);
}

#[test]
fn another_notice_closed_stays_on_this_client() {
    let mut state = state_with_card();
    let mut notice = card("Q1");
    notice.as_object_mut().unwrap().remove("sendFailure");
    state.session.terminal_notice = Some(notice);
    let effects = handle_action(
        &mut state,
        &UserAction::new(ActionKind::DismissNotice),
        &context(),
    );
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::SendRpc { .. })));
}

#[test]
fn a_failed_row_is_not_a_send_on_its_way() {
    // The composer's echo adopts the failed row, which stops the local spinner: only an echo with
    // no row reads as working.
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state
        .pending
        .sends
        .push(ghostex_gx_chat_core::state::PendingSend {
            id: "a".into(),
            queued_prompt_id: Some("Q1".into()),
            startup_delivery: Some(ghostex_gx_protocol::StartupDelivery {
                prompt_id: "Q1".into(),
                state: "failed".into(),
                error_message: Some("why".into()),
                wait_reason: None,
            }),
            ..Default::default()
        });
    assert!(!is_working(&state));
}

mod mode_menu {
    use ghostex_gx_chat_core::menus::{
        catalog::parse_agent_model_catalog,
        option_catalog::session_option_catalog,
        option_menus::{native_option_menus, OptionMenuParams},
        option_values::{apply_detected_options, DetectedOptions, OptionState},
    };
    use ghostex_gx_protocol::UnavailableMode;
    use serde_json::{json, Value};

    /// The Mode menu's rows, with Claude in `current` and `unavailable` greyed.
    fn rows(current: &str, unavailable: Vec<UnavailableMode>) -> Vec<Value> {
        let model_catalog = parse_agent_model_catalog(&json!({"schemaVersion":1,"updatedAt":"2026-10-31","effortLabels":{},"agents":{"claude":{"name":"Claude","efforts":["low","high"],"models":[{"value":"fable","label":"Fable","efforts":["low","high"]}]}}})).unwrap();
        let catalog = session_option_catalog(&model_catalog, Some("claude")).unwrap();
        let descriptors = catalog.options_for_model("fable");
        let detected = DetectedOptions::from_value(&json!({
            "model": {"value": "fable", "label": "Fable", "source": "terminal"},
            "mode": {"value": current, "label": current, "source": "terminal"},
            "detectedAt": "2026-10-31T10:00:00.000Z",
        }))
        .unwrap();
        let state = apply_detected_options(&catalog, &OptionState::new(), Some(&detected));
        let menus = native_option_menus(
            Some(&catalog),
            &state,
            &descriptors,
            &OptionMenuParams {
                unavailable_modes: unavailable,
                ..Default::default()
            },
        );
        serde_json::to_value(&menus.mode)
            .unwrap()
            .as_array()
            .unwrap()
            .clone()
    }

    fn row<'a>(rows: &'a [Value], value: &str) -> &'a Value {
        rows.iter()
            .find(|row| row["id"] == format!("mode:{value}"))
            .unwrap()
    }

    fn bypass_greyed() -> Vec<UnavailableMode> {
        vec![UnavailableMode {
            value: "bypass".into(),
            why: "This session wasn't started with the skip-permissions flag.".into(),
            restart: None,
        }]
    }

    #[test]
    fn a_mode_the_session_cannot_use_is_greyed_with_how_to_turn_it_on() {
        let rows = rows("manual", bypass_greyed());
        let bypass = row(&rows, "bypass");
        assert_eq!(bypass["disabled"], true);
        assert_eq!(
            bypass["description"],
            "This session wasn't started with the skip-permissions flag."
        );
        assert_eq!(row(&rows, "plan")["disabled"], false);
    }

    #[test]
    fn the_mode_it_is_in_is_never_greyed() {
        let rows = rows("bypass", bypass_greyed());
        let bypass = row(&rows, "bypass");
        assert_eq!(bypass["checked"], true);
        assert_eq!(bypass["disabled"], false);
    }

    #[test]
    fn nothing_known_greys_nothing() {
        let rows = rows("manual", Vec::new());
        assert_eq!(row(&rows, "bypass")["disabled"], false);
    }
}

/// The cards of e3d35d537 share this code: a send that left the queue and that the transcript never
/// confirmed says nothing about the agent, so it turns no spinner and raises no failed-send card.
#[test]
fn an_unconfirmed_card_is_neither_working_nor_a_failed_send() {
    let mut state = ChatState::default();
    state.session.agent = Some("claude".into());
    state
        .pending
        .sends
        .push(ghostex_gx_chat_core::state::PendingSend {
            id: "a".into(),
            queued_prompt_id: Some("Q1".into()),
            startup_delivery: Some(ghostex_gx_protocol::StartupDelivery {
                prompt_id: "Q1".into(),
                state: "unconfirmed".into(),
                error_message: None,
                wait_reason: None,
            }),
            ..Default::default()
        });
    assert!(!is_working(&state));
    assert_eq!(card_prompt_id(&state), None);
}

#[test]
fn retained_question_and_approval_cards_wait_for_live_confirmation() {
    use ghostex_gx_chat_core::{ChatCore, ChatFrame, Event, RpcOutcome, StartConfig};

    fn drive(core: &mut ChatCore, event: Event) {
        let mut pending = core.handle(event, context());
        while let Some(effect) = pending.pop() {
            let keys = match effect {
                Effect::ReadStorage { key } => vec![key],
                Effect::ReadStorageBatch { keys } => keys,
                _ => continue,
            };
            for key in keys {
                pending.extend(core.handle(Event::StorageLoaded { key, value: None }, context()));
            }
        }
    }

    fn seed(core: &mut ChatCore, result: Value) {
        let request_id = core.state().messages.reads[0].request_id;
        drive(
            core,
            Event::RpcSettled {
                request_id,
                outcome: Box::new(RpcOutcome::Ok { result }),
            },
        );
    }

    // The terminal card the screen showed for the same wait comes back with the fold and must wait
    // for a live read just like the prompt.
    let notice = |detected_at: &str| {
        json!({
            "kind":"claudeInputBlocked", "severity":"warning", "title":"Continue?",
            "source":"screen", "detectedAt":detected_at,
        })
    };
    let saved_notice = notice("2026-10-31T18:01:00.000Z");
    let live_notice = notice("2026-10-31T18:03:00.000Z");
    for prompt in [
        json!({"kind":"question","toolUseId":"ask-1","questions":[{"question":"Continue?","options":[{"label":"Yes"}]}]}),
        json!({"kind":"approval","toolUseId":"ask-1","tool":"edit","summary":"Change a file"}),
    ] {
        let retained = json!({
            "projectId":"project-1", "sessionId":"session-1", "serverId":"server-1",
            "protocolVersion":1, "epoch":1, "seq":1, "status":"ready", "prompt":prompt,
            "terminalNotice":saved_notice,
            "messages":[{"id":"m1","role":"user","source":"transcript","blocks":[{"type":"text","text":"Hello"}]}],
            "hasMore":false, "beforeOffset":0,
        });
        let record = json!({
            "key":"retained-1", "savedAt":context().now_ms as i64,
            "requestedWindow":300, "snapshot":retained,
        })
        .to_string();
        for source in ["stored-before-boot", "stored-after-boot", "initial"] {
            for confirmation in ["snapshot", "replaced", "state", "seed"] {
                for still_open in [false, true] {
                    let mut core = ChatCore::new();
                    drive(
                        &mut core,
                        Event::Start(Box::new(StartConfig {
                            project_id: "project-1".into(),
                            session_id: "session-1".into(),
                            initial_snapshot: (source == "initial").then(|| retained.clone()),
                            ..Default::default()
                        })),
                    );
                    if source == "stored-before-boot" {
                        drive(
                            &mut core,
                            Event::RetainedSnapshotLoaded {
                                value: Some(record.clone()),
                            },
                        );
                    }
                    drive(&mut core, Event::ComposerBootRead(Box::default()));
                    if source == "stored-after-boot" {
                        drive(
                            &mut core,
                            Event::RetainedSnapshotLoaded {
                                value: Some(record.clone()),
                            },
                        );
                    }
                    assert_eq!(core.state().messages.list.len(), 1, "{source}");
                    assert!(!core.document().question_card.visible, "{source}: {prompt}");
                    assert_eq!(core.state().session.terminal_notice, None, "{source}");
                    assert!(!notice_visible(core.state()), "{source}: {prompt}");
                    assert_eq!(
                        core.state()
                            .messages
                            .snapshot
                            .as_ref()
                            .unwrap()
                            .result
                            .state
                            .prompt
                            .as_ref(),
                        Some(&prompt),
                        "retention keeps the original fold"
                    );
                    assert_eq!(
                        core.state()
                            .messages
                            .snapshot
                            .as_ref()
                            .unwrap()
                            .result
                            .state
                            .terminal_notice
                            .as_ref(),
                        Some(&saved_notice),
                        "retention keeps the original fold"
                    );

                    // Appends confirm transcript rows, not whether a blocking prompt is still open.
                    drive(
                        &mut core,
                        Event::Frame(Box::new(ChatFrame::Appended(Box::new(
                            serde_json::from_value(json!({
                                "projectId":"project-1", "sessionId":"session-1",
                                "serverId":"server-1", "protocolVersion":1, "epoch":1, "seq":2, "messages":[],
                            })).unwrap(),
                        )))),
                    );
                    assert!(!core.document().question_card.visible);
                    assert!(!notice_visible(core.state()));

                    let mut live = retained.clone();
                    live["seq"] = json!(3);
                    live["prompt"] = if still_open {
                        prompt.clone()
                    } else {
                        Value::Null
                    };
                    live["terminalNotice"] = if still_open {
                        live_notice.clone()
                    } else {
                        Value::Null
                    };
                    match confirmation {
                        "seed" => seed(&mut core, live),
                        "state" => drive(
                            &mut core,
                            Event::Frame(Box::new(ChatFrame::State(Box::new(
                                serde_json::from_value(live).unwrap(),
                            )))),
                        ),
                        _ => {
                            let frame = Box::new(serde_json::from_value(live).unwrap());
                            let frame = if confirmation == "snapshot" {
                                ChatFrame::Snapshot(frame)
                            } else {
                                ChatFrame::Replaced(frame)
                            };
                            drive(&mut core, Event::Frame(Box::new(frame)));
                        }
                    }
                    let card = &core.document().question_card;
                    assert_eq!(
                        card.visible, still_open,
                        "{source}/{confirmation}: {prompt}"
                    );
                    assert!(!card.loading, "{source}/{confirmation}: {prompt}");
                    assert_eq!(
                        core.state().session.terminal_notice.as_ref(),
                        still_open.then_some(&live_notice),
                        "{source}/{confirmation}: {prompt}"
                    );
                    assert_eq!(
                        notice_visible(core.state()),
                        still_open,
                        "{source}/{confirmation}: {prompt}"
                    );
                }
            }
        }

        // A late storage answer must not replace a prompt a seed read already confirmed.
        for still_open in [false, true] {
            let mut core = ChatCore::new();
            drive(&mut core, Event::Start(Box::default()));
            drive(&mut core, Event::ComposerBootRead(Box::default()));
            let mut live = retained.clone();
            live["prompt"] = if still_open {
                let mut current = prompt.clone();
                current["toolUseId"] = json!("ask-2");
                current
            } else {
                Value::Null
            };
            live["terminalNotice"] = if still_open {
                live_notice.clone()
            } else {
                Value::Null
            };
            seed(&mut core, live.clone());
            drive(
                &mut core,
                Event::RetainedSnapshotLoaded {
                    value: Some(record.clone()),
                },
            );
            assert_eq!(core.document().question_card.visible, still_open);
            assert_eq!(
                core.state().session.prompt.as_ref(),
                live["prompt"].as_object().map(|_| &live["prompt"])
            );
            assert_eq!(
                core.state().session.terminal_notice.as_ref(),
                still_open.then_some(&live_notice)
            );
            assert_eq!(notice_visible(core.state()), still_open);
        }
    }
}
