//! The card above the composer while a model change that failed for good holds messages: one
//! card that counts the held messages, names the failed change, keeps the diagnostic behind
//! Details and offers Pick model, shown only
//! while a failed change holds at least one queued message, in every agent's chat that has a
//! model menu.
//!
//! Every test drives the chat core the way a host does (start, composer boot read with a model
//! catalog, gxserver frames) and reads the document the renderers draw.

use ghostex_gx_chat_core::menus::picker::hold_card::{
    model_hold_card, model_selection_failure_reason,
};
use ghostex_gx_chat_core::protocol::ChatStateFrame;
use ghostex_gx_chat_core::{
    ChatContext, ChatCore, ChatFrame, ChatRpcMethod, ChatSnapshotFrame, ComposerBootRead, Effect,
    Event, StartConfig, UserAction,
};
use serde_json::{json, Value};

/// 2026-10-30 12:30 UTC.
const NOW_MS: f64 = 1_793_363_400_000.0;
const PROJECT: &str = "billing-service";
const SESSION: &str = "s-rate-limit";
const REASON: &str = "No API key for openrouter.";
const SENTENCE: &str =
    "The change to openrouter/qwen3.8-max high failed: No API key for openrouter.";

fn catalog() -> Value {
    json!({
        "schemaVersion": 1,
        "updatedAt": "2026-10-30",
        "effortLabels": {},
        "agents": {
            "omp": {
                "name": "OMP",
                "efforts": ["low", "high"],
                "models": [
                    { "value": "anthropic/claude-sonnet-5.5", "label": "Claude Sonnet 5.5", "efforts": ["low", "high"] },
                    { "value": "openrouter/qwen3.8-max", "label": "qwen3.8-max", "efforts": ["low", "high"] },
                ],
            },
            "claude": {
                "name": "Claude Code",
                "efforts": ["low", "high"],
                "fastMode": { "available": true, "command": "/fast", "scope": "model" },
                "models": [{ "value": "sonnet", "label": "Sonnet", "efforts": ["low", "high"] }],
            },
            "codex": {
                "name": "Codex CLI",
                "efforts": ["low", "high"],
                "fastMode": { "available": true, "command": "/fast", "scope": "default" },
                "models": [{ "value": "gpt-6.1-sol", "label": "GPT-6.1-Sol", "efforts": ["low", "high"] }],
            },
        },
    })
}

struct Chat {
    core: ChatCore,
    seq: i64,
}

impl Chat {
    fn open() -> Self {
        Self::open_with(catalog())
    }

    /// A chat whose host's catalog is `catalog`, as the composer boot read hands it over.
    fn open_with(catalog: Value) -> Self {
        let mut chat = Self {
            core: ChatCore::new(),
            seq: 0,
        };
        let start: StartConfig = serde_json::from_value(json!({
            "clientId": "client-1",
            "projectId": PROJECT,
            "sessionId": SESSION,
        }))
        .expect("the start config parses");
        chat.drive(Event::Start(Box::new(start)));
        let boot: ComposerBootRead = serde_json::from_value(json!({
            "sessionKey": format!("{PROJECT}:{SESSION}"),
            "clientId": "client-1",
            "modelCatalog": catalog,
        }))
        .expect("the boot read parses");
        chat.drive(Event::ComposerBootRead(Box::new(boot)));
        chat
    }

    fn drive(&mut self, event: Event) {
        self.core.handle(event, ChatContext::at(NOW_MS));
    }

    /// A `sessionChatSnapshot` for an agent, with `state` spread in as gxserver writes it.
    fn snapshot(&mut self, agent: &str, state: Value) {
        self.seq += 1;
        let mut frame = json!({
            "projectId": PROJECT,
            "sessionId": SESSION,
            "serverId": "server-1",
            "protocolVersion": 1,
            "epoch": 1,
            "seq": self.seq,
            "status": "ready",
            "agent": agent,
            "messages": [],
            "hasMore": false,
            "beforeOffset": 0,
        });
        for (key, value) in state.as_object().cloned().unwrap_or_default() {
            frame[key] = value;
        }
        let frame: ChatSnapshotFrame = serde_json::from_value(frame).expect("the frame parses");
        self.drive(Event::Frame(Box::new(ChatFrame::Snapshot(Box::new(frame)))));
    }

    /// A `sessionChatState`: a queue or model selection change without new rows.
    fn state(&mut self, state: Value) {
        self.seq += 1;
        let mut frame = json!({
            "projectId": PROJECT,
            "sessionId": SESSION,
            "serverId": "server-1",
            "protocolVersion": 1,
            "epoch": 1,
            "seq": self.seq,
            "status": "ready",
        });
        for (key, value) in state.as_object().cloned().unwrap_or_default() {
            frame[key] = value;
        }
        let frame: ChatStateFrame = serde_json::from_value(frame).expect("the frame parses");
        self.drive(Event::Frame(Box::new(ChatFrame::State(Box::new(frame)))));
    }

    fn document(&self) -> Value {
        serde_json::to_value(self.core.document()).expect("the document is JSON")
    }

    fn card(&self) -> Value {
        self.document()
            .get("modelHoldCard")
            .cloned()
            .unwrap_or(Value::Null)
    }
}

/// A row of Ghostex's queue as gxserver sends it: a message sent while the change was pending
/// (`startupSend`), or one queued with Tab.
fn prompt(id: &str, text: &str, startup_send: bool, state: &str) -> Value {
    let mut row = json!({
        "id": id,
        "text": text,
        "startupSend": startup_send,
        "state": state,
        "createdAt": "2026-10-30T12:20:00.000Z",
        "updatedAt": "2026-10-30T12:20:00.000Z",
    });
    if state == "failed" {
        row["errorMessage"] = json!("gxserver restarted while this prompt was being delivered.");
    }
    row
}

/// The session's model selection row as gxserver projects it.
fn selection(model: &str, state: &str, error: Option<&str>) -> Value {
    let mut row = json!({
        "id": "sel-1",
        "model": model,
        "effort": "high",
        "options": {},
        "scope": "default",
        "state": state,
    });
    if let Some(error) = error {
        row["errorMessage"] = json!(error);
    }
    row
}

fn failed() -> Value {
    selection("openrouter/qwen3.8-max", "failed", Some(REASON))
}

fn sso() -> Value {
    prompt(
        "q-1",
        "Now cover the SSO callback too; it builds its own router.",
        true,
        "queued",
    )
}

#[test]
fn one_held_message_gets_one_card_with_the_reason_and_pick_model() {
    let mut chat = Chat::open();
    chat.snapshot(
        "omp",
        json!({ "queue": [sso()], "pendingModelSelection": failed() }),
    );
    assert_eq!(
        chat.card(),
        json!({
            "tone": "warning",
            "title": "1 message is held",
            "body": "The model change failed.",
            "detail": SENTENCE,
            "note": "Pick a model again; held messages send once it applies.",
            "action": { "id": "pickModel", "label": "Pick model" },
            "heldCount": 1,
        })
    );
    // The model menu the button opens is there, and still says why where the model was chosen.
    let document = chat.document();
    assert!(document["modelMenu"].is_object());
    assert_eq!(document["modelSelection"]["selectionError"], json!(REASON));

    for (model, effort, body, detail) in [
        (
            "openrouter/qwen3.8-max",
            "",
            "The model change failed.",
            "The change to openrouter/qwen3.8-max failed: No API key for openrouter.",
        ),
        (
            "",
            "high",
            "The thinking-level change failed.",
            "The change to high failed: No API key for openrouter.",
        ),
        (
            "",
            "",
            "The chat setting change failed.",
            "The chat option change failed: No API key for openrouter.",
        ),
    ] {
        let mut pending = selection(model, "failed", Some(REASON));
        pending["effort"] = json!(effort);
        chat.state(json!({ "pendingModelSelection": pending }));
        assert_eq!(chat.card()["body"], json!(body));
        assert_eq!(chat.card()["detail"], json!(detail));
        assert_eq!(
            chat.document()["modelSelection"]["selectionError"],
            json!(REASON)
        );
    }
}

#[test]
fn several_held_messages_share_one_card_that_counts_only_the_waiting_ones() {
    let mut chat = Chat::open();
    chat.snapshot(
        "omp",
        json!({
            "queue": [
                sso(),
                prompt("q-2", "And add a test that the sixth failed login in a window gets a 429.", true, "queued"),
                prompt("q-3", "Keep the window at 15 minutes.", false, "queued"),
                prompt("q-4", "Run the auth tests.", false, "failed"),
            ],
            "pendingModelSelection": failed(),
        }),
    );
    let card = chat.card();
    assert_eq!(card["title"], json!("3 messages are held"));
    assert_eq!(card["heldCount"], json!(3));
    assert_eq!(card["detail"], json!(SENTENCE));
}

#[test]
fn a_failed_change_with_nothing_held_shows_no_card() {
    let mut chat = Chat::open();
    chat.snapshot(
        "omp",
        json!({ "queue": [], "pendingModelSelection": failed() }),
    );
    assert_eq!(chat.card(), Value::Null);
    // A message that failed to deliver is not waiting behind the change.
    chat.state(json!({
        "queue": [prompt("q-4", "Run the auth tests.", false, "failed")],
        "pendingModelSelection": failed(),
    }));
    assert_eq!(chat.card(), Value::Null);
    // The model menu keeps its Not applied reason either way.
    assert_eq!(
        chat.document()["modelSelection"]["selectionError"],
        json!(REASON)
    );
}

#[test]
fn a_change_still_retrying_shows_no_card_even_with_messages_waiting() {
    let mut chat = Chat::open();
    for state in ["queued", "applying"] {
        chat.snapshot(
            "omp",
            json!({
                "queue": [sso()],
                "pendingModelSelection": selection("openrouter/qwen3.8-max", state, Some("The agent is busy.")),
            }),
        );
        assert_eq!(chat.card(), Value::Null, "a {state} change");
    }
}

#[test]
fn the_card_leaves_when_a_pick_applies_or_the_messages_leave() {
    let mut chat = Chat::open();
    chat.snapshot(
        "omp",
        json!({ "queue": [sso()], "pendingModelSelection": failed() }),
    );
    assert_eq!(chat.card()["heldCount"], json!(1));

    // Picking again replaces the failed change with a queued one: the card goes at once.
    chat.state(json!({
        "queue": [sso()],
        "pendingModelSelection": selection("anthropic/claude-sonnet-5.5", "queued", None),
    }));
    assert_eq!(chat.card(), Value::Null);

    // That pick fails too: the card is back, with the new reason.
    chat.state(json!({
        "queue": [sso()],
        "pendingModelSelection": selection("anthropic/claude-sonnet-5.5", "failed", Some("Unknown model: anthropic/claude-sonnet-5.5")),
    }));
    assert_eq!(
        chat.card()["detail"],
        json!("The change to anthropic/claude-sonnet-5.5 high failed: Unknown model: anthropic/claude-sonnet-5.5")
    );

    // The change applies: gxserver drops the row and delivers the message.
    chat.state(json!({ "queue": [sso()], "pendingModelSelection": null }));
    assert_eq!(chat.card(), Value::Null);

    // A held message the user deleted leaves the queue, and the card goes with the last one.
    chat.state(json!({ "queue": [sso()], "pendingModelSelection": failed() }));
    assert_eq!(chat.card()["heldCount"], json!(1));
    chat.state(json!({ "queue": [], "pendingModelSelection": failed() }));
    assert_eq!(chat.card(), Value::Null);
}

#[test]
fn claude_and_codex_chats_get_the_card_and_keep_not_applied() {
    for (agent, model, reason) in [
        (
            "claude",
            "sonnet",
            "The model is not available for this account.",
        ),
        (
            "codex",
            "gpt-6.1-sol",
            "The agent never confirmed this change after 3 tries, so Ghostex stopped retrying it.",
        ),
    ] {
        let mut chat = Chat::open();
        chat.snapshot(
            agent,
            json!({
                "queue": [sso()],
                "pendingModelSelection": selection(model, "failed", Some(reason)),
            }),
        );
        let document = chat.document();
        assert_eq!(
            document["modelHoldCard"]["title"],
            json!("1 message is held"),
            "{agent}"
        );
        assert_eq!(
            document["modelHoldCard"]["detail"],
            json!(format!("The change to {model} high failed: {reason}")),
            "{agent}"
        );
        // Their model menu and its Not applied reason are what they were.
        assert!(document["modelMenu"].is_object(), "{agent}");
        assert_eq!(
            document["modelSelection"]["selectionError"],
            json!(reason),
            "{agent}"
        );
        // A pick that is still retrying stays quiet for them too.
        chat.state(json!({
            "queue": [sso()],
            "pendingModelSelection": selection(model, "queued", Some(reason)),
        }));
        assert_eq!(chat.card(), Value::Null, "{agent}");
    }
}

#[test]
fn an_older_gxserver_without_the_queue_and_selection_fields_shows_no_card() {
    let mut chat = Chat::open();
    // A gxserver without queue support carries neither field.
    chat.snapshot("omp", json!({}));
    assert!(chat.document().get("modelHoldCard").is_none());
    // One that carries the queue but no model selection row.
    chat.snapshot("omp", json!({ "queue": [sso()] }));
    assert!(chat.document().get("modelHoldCard").is_none());
}

#[test]
fn the_reason_is_the_sentence_gxserver_writes_for_the_sidebar() {
    assert_eq!(
        model_selection_failure_reason("openrouter/qwen3.8-max", "high", REASON),
        SENTENCE
    );
    assert_eq!(
        model_selection_failure_reason("openrouter/qwen3.8-max", "", REASON),
        "The change to openrouter/qwen3.8-max failed: No API key for openrouter."
    );
    assert_eq!(
        model_selection_failure_reason("", "", "That option is not available for this agent."),
        "The chat option change failed: That option is not available for this agent."
    );
}

/// The card's Pick model opens the chat's model menu, which an OMP chat only has once gxserver has
/// read the agent's model list. Until then the failed pick still holds the messages and the
/// sidebar's mark explains it, and the card appears with the list.
#[test]
fn the_card_appears_once_the_agents_model_list_is_available() {
    let mut without_omp = catalog();
    without_omp["agents"]
        .as_object_mut()
        .expect("agents")
        .remove("omp");
    let mut chat = Chat::open_with(without_omp);
    chat.snapshot(
        "omp",
        json!({ "queue": [sso()], "pendingModelSelection": failed() }),
    );
    let document = chat.document();
    assert!(document["modelMenu"].is_null());
    assert!(document.get("modelHoldCard").is_none());
    assert_eq!(document["pendingModelSelection"]["state"], json!("failed"));

    let mut with_omp = catalog();
    with_omp["updatedAt"] = json!("2026-10-31");
    chat.drive(Event::ModelCatalogChanged { catalog: with_omp });
    assert_eq!(chat.card()["title"], json!("1 message is held"));
    assert_eq!(chat.card()["detail"], json!(SENTENCE));
}

/// The card is kept between events while what it says is unchanged, but a state nothing has
/// settled yet still gets the card it describes, never a kept one.
#[test]
fn a_kept_card_never_outlives_what_it_describes() {
    let mut chat = Chat::open();
    chat.snapshot(
        "omp",
        json!({ "queue": [sso()], "pendingModelSelection": failed() }),
    );
    assert_eq!(chat.card()["heldCount"], json!(1));
    let state = chat.core.state_mut();
    *state.session.queue_prompts = Some(vec![
        sso(),
        prompt("q-2", "Keep the window at 15 minutes.", false, "queued"),
    ]);
    let menu = state.pickers.model_menu_context.clone();
    let card = model_hold_card(chat.core.state(), menu.as_ref()).expect("still held");
    assert_eq!(card["title"], json!("2 messages are held"));
    chat.core.republish();
    assert_eq!(chat.card()["heldCount"], json!(2));
}

/// The model the session still runs after the failed change, and the level gxserver reads for it.
const RUNNING: &str = "anthropic/claude-sonnet-5.5";

fn running_at_low() -> Value {
    json!({
        "model": { "value": RUNNING, "label": "Claude Sonnet 5.5", "source": "transcript" },
        "effort": { "value": "low", "label": "low", "source": "transcript" },
        "detectedAt": "2026-10-30T12:25:00.000Z",
    })
}

impl Chat {
    /// A gesture as the renderer sends it. A `selectOption` the core hands back is replayed the
    /// way the desktop host replays it (`gx_chat/effects.rs`). Returns every effect.
    fn act(&mut self, action: Value) -> Vec<Effect> {
        let action: UserAction = serde_json::from_value(action).expect("the action parses");
        let mut effects = self
            .core
            .handle(Event::Action(Box::new(action)), ChatContext::at(NOW_MS));
        let replayed: Vec<UserAction> = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::HostAction { action, params } if action == "selectOption" => {
                    Some(serde_json::from_value((**params).clone()).expect("a replayable action"))
                }
                _ => None,
            })
            .collect();
        for action in replayed {
            effects.extend(
                self.core
                    .handle(Event::Action(Box::new(action)), ChatContext::at(NOW_MS)),
            );
        }
        effects
    }

    /// The model menu row of `model`.
    fn row_key(&self, model: &str) -> Value {
        self.document()["modelMenu"]["rows"]
            .as_array()
            .expect("the model menu lists rows")
            .iter()
            .find(|row| row["value"] == model)
            .expect("the model is listed")["key"]
            .clone()
    }
}

/// The model and level of every pick the chat sent gxserver (`selectSessionChatModel`).
fn sent_picks(effects: &[Effect]) -> Vec<(String, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SendRpc { method, params, .. }
                if *method == ChatRpcMethod::SelectSessionChatModel =>
            {
                Some((
                    params["model"].as_str().unwrap_or_default().to_string(),
                    params["effort"].as_str().unwrap_or_default().to_string(),
                ))
            }
            _ => None,
        })
        .collect()
}

/// A click on a row, and Enter on it after the level was moved back to the one shown.
fn picks_of(chat: &Chat, model: &str, level: &str) -> [Value; 2] {
    let key = chat.row_key(model);
    [
        json!({ "type": "modelMenuPick", "key": key }),
        json!({ "type": "modelMenuPick", "key": key, "effort": level }),
    ]
}

/// The way out the card names: picking the model the session still runs, at the level it reports,
/// is sent to gxserver, which replaces the failed change with it; the card then goes.
#[test]
fn while_messages_are_held_picking_the_running_model_again_is_sent() {
    for gesture in 0..2 {
        let mut chat = Chat::open();
        chat.snapshot(
            "omp",
            json!({
                "queue": [sso()],
                "pendingModelSelection": failed(),
                "selectedOptions": running_at_low(),
            }),
        );
        assert_eq!(chat.card()["heldCount"], json!(1));
        let pick = picks_of(&chat, RUNNING, "low")[gesture].clone();
        let effects = chat.act(pick);
        assert_eq!(
            sent_picks(&effects),
            [(RUNNING.to_string(), "low".to_string())],
            "gesture {gesture}"
        );
        chat.state(json!({ "pendingModelSelection": selection(RUNNING, "queued", None) }));
        assert_eq!(chat.card(), Value::Null, "gesture {gesture}");
    }
}

/// Without a hold, picking the model and level the session already runs still changes nothing.
#[test]
fn without_a_hold_picking_the_running_model_again_changes_nothing() {
    for gesture in 0..2 {
        let mut chat = Chat::open();
        chat.snapshot(
            "omp",
            json!({
                "queue": [],
                "pendingModelSelection": null,
                "selectedOptions": running_at_low(),
            }),
        );
        let pick = picks_of(&chat, RUNNING, "low")[gesture].clone();
        let effects = chat.act(pick);
        assert!(sent_picks(&effects).is_empty(), "gesture {gesture}");
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::WriteStorage { durable: true, .. })),
            "gesture {gesture}: nothing goes into the outbox"
        );
    }
}

/// While messages are held, picking another model is sent as before.
#[test]
fn while_messages_are_held_picking_another_model_is_sent_as_before() {
    let other = "openrouter/qwen3.8-max";
    for (gesture, level) in [(0, "low"), (1, "high")] {
        let mut chat = Chat::open();
        chat.snapshot(
            "omp",
            json!({
                "queue": [sso()],
                "pendingModelSelection": failed(),
                "selectedOptions": running_at_low(),
            }),
        );
        let pick = picks_of(&chat, other, level)[gesture].clone();
        assert_eq!(
            sent_picks(&chat.act(pick)),
            [(other.to_string(), level.to_string())],
            "gesture {gesture}"
        );
    }
}
