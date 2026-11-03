//! Stopping a session and starting a coordinator by voice (`control.rs`): which stops are sure and
//! which ask first, the confirm card's frames end to end in a real `Conversation`, and the
//! coordinator a voice start asks gxserver for.

use std::time::Duration;

use serde_json::{json, Value};

use super::control::{self, OpenConfirm, StopTarget};
use super::conversation::Input;
use super::focus::FocusScope;
use super::focus_tests::Call;
use super::sessions::{SessionInfo, VoiceState};
use super::tools::{self, ToolMemory, ToolOutcome};

fn session(project: &str, id: &str, title: &str) -> SessionInfo {
    SessionInfo {
        project_id: project.to_string(),
        session_id: id.to_string(),
        title: title.to_string(),
        project: "Ghostex".to_string(),
        agent: "Claude".to_string(),
        state: VoiceState::Working,
        waiting: None,
        last_active_at: String::new(),
        turn_event: None,
        has_chat: true,
        thread_of: None,
        threads: None,
        computer: None,
    }
}

fn sessions() -> Vec<SessionInfo> {
    vec![
        session("P1", "S1", "Discord triage"),
        session("P1", "S2", "Sidebar triage fix"),
        session("P1", "S3", "Release notes"),
    ]
}

fn titles(target: &StopTarget) -> (bool, Vec<String>) {
    match target {
        StopTarget::Sure(session) => (true, vec![session.title.clone()]),
        StopTarget::Unsure(candidates) => (
            false,
            candidates
                .iter()
                .map(|session| session.title.clone())
                .collect(),
        ),
    }
}

fn target(memory: &ToolMemory, query: &str) -> (bool, Vec<String>) {
    titles(&control::stop_target(memory, &sessions(), query).expect("a match"))
}

#[test]
fn a_plainly_named_session_is_stopped_without_asking() {
    let memory = ToolMemory::new(false);
    assert_eq!(
        target(&memory, "Discord triage"),
        (true, vec!["Discord triage".into()])
    );
    assert_eq!(
        target(&memory, "the discord one"),
        (true, vec!["Discord triage".into()])
    );
    assert_eq!(
        target(&memory, "P1:S3"),
        (true, vec!["Release notes".into()])
    );
    assert_eq!(
        target(&memory, "the release session"),
        (true, vec!["Release notes".into()])
    );
}

#[test]
fn a_loose_or_shared_name_asks_first() {
    let memory = ToolMemory::new(false);
    // "triage" fits two titles alike.
    let (sure, candidates) = target(&memory, "the triage one");
    assert!(!sure);
    assert_eq!(candidates, vec!["Discord triage", "Sidebar triage fix"]);
    // Only "discord" matches; "bot" matches nothing, so the match is a guess.
    assert_eq!(
        target(&memory, "the discord bot"),
        (false, vec!["Discord triage".into()])
    );
    assert!(control::stop_target(&memory, &sessions(), "billing").is_err());
}

#[test]
fn a_name_resolved_earlier_in_the_call_stays_sure() {
    let memory = ToolMemory::new(false);
    let all = sessions();
    memory.remember("the bot one", &all[0]);
    assert_eq!(
        target(&memory, "the bot one"),
        (true, vec!["Discord triage".into()])
    );
}

#[test]
fn an_unsure_stop_puts_a_card_up_and_a_sure_one_does_not() {
    let all = sessions();
    let one = control::unsure_result(&all[..1]);
    assert_eq!(one["stopped"], false);
    let frame = control::confirm_frame("c7", &one).expect("a card");
    assert_eq!(frame["type"], "confirm");
    assert_eq!(frame["id"], "c7");
    assert_eq!(frame["title"], "Stop Discord triage?");
    assert_eq!(
        frame["choices"],
        json!([{"session": "P1:S1", "title": "Discord triage", "detail": "Ghostex · working"}])
    );
    assert_eq!(frame["confirmLabel"], "Stop");

    let two = control::unsure_result(&all[..2]);
    let frame = control::confirm_frame("c8", &two).expect("a card");
    assert_eq!(frame["title"], "Stop which session?");
    let open = OpenConfirm::from_frame(&frame).expect("open card");
    assert_eq!(open.title_of("P1:S2"), Some("Sidebar triage fix"));
    assert_eq!(open.title_of("P1:S3"), None);

    let stopped = json!({"ok": true, "stopped": true, "session": all[0].brief()});
    assert!(control::confirm_frame("c9", &stopped).is_none());
    assert_eq!(control::stopped_ref(&stopped), Some("P1:S1"));
    assert_eq!(control::stopped_ref(&one), None);
}

fn launchers() -> Vec<Value> {
    vec![
        json!({"agentId": "gemini", "name": "Gemini", "icon": "gemini", "command": "gemini"}),
        json!({"agentId": "codex", "name": "Codex", "icon": "codex", "command": "codex"}),
        json!({"agentId": "claude", "name": "Claude", "icon": "claude", "command": "claude"}),
    ]
}

#[test]
fn a_voice_coordinator_runs_on_an_agent_that_can_coordinate() {
    let rows = launchers();
    assert_eq!(
        control::coordinator_agent(&rows, "Codex", None).as_deref(),
        Ok("codex")
    );
    let refused = control::coordinator_agent(&rows, "gemini", None).unwrap_err();
    assert!(refused.contains("can't run on"), "{refused}");
    assert!(control::coordinator_agent(&rows, "nothing", None).is_err());
    // The user's default agent when it can coordinate, else Claude first.
    assert_eq!(
        control::coordinator_agent(&rows, "", Some("codex")).as_deref(),
        Ok("codex")
    );
    assert_eq!(
        control::coordinator_agent(&rows, "", Some("gemini")).as_deref(),
        Ok("claude")
    );
    assert_eq!(
        control::coordinator_agent(&rows, "", None).as_deref(),
        Ok("claude")
    );
}

#[test]
fn a_voice_coordinator_is_created_like_ghostex_coordinator_create() {
    let rows = launchers();
    let params =
        crate::coordinators::coordinator_create_params(crate::coordinators::CoordinatorLaunch {
            agent_rows: &rows,
            project_id: "P1",
            agent_id: "claude",
            title: None,
            goal: String::new(),
            task: "Work through the Discord requests".to_string(),
            model: None,
            effort: None,
        });
    assert_eq!(
        params["coordinator"]["firstRequest"],
        "Work through the Discord requests"
    );
    assert_eq!(params["title"], "Coordinator");
    assert_eq!(params["runtimeSettings"]["titleSource"], "placeholder");
    assert_eq!(params["agentModel"], "opus[1m]");
    assert_eq!(params["agentEffort"], "high");
    assert_eq!(
        params["launchSettings"],
        json!({"agentCommand": "claude", "icon": "claude"})
    );
}

#[test]
fn the_tools_are_offered_and_a_call_about_one_session_can_only_stop_it() {
    let names = |tools: Value| -> Vec<String> {
        tools
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect()
    };
    let all = names(tools::definitions(None));
    assert!(all.contains(&"stop_session".to_string()));
    assert!(all.contains(&"start_coordinator".to_string()));
    let scope = FocusScope::from_sessions(&sessions(), "P1:S1").expect("scope");
    let focused = names(tools::definitions(Some(&scope)));
    assert!(focused.contains(&"stop_session".to_string()));
    assert!(!focused.contains(&"start_coordinator".to_string()));
    assert_eq!(
        tools::label("stop_session", &json!({"session": "the discord one"}), None),
        "Stopping the discord one"
    );
    assert_eq!(
        tools::label("start_coordinator", &json!({"project": "Ghostex"}), None),
        "Starting a coordinator in Ghostex"
    );
}

/// Sends the conversation a finished `stop_session` call, as if the model had called it.
fn stop_done(call: &Call, call_id: &str, output: Value) {
    call.tx
        .send(Input::ToolDone {
            call_id: call_id.to_string(),
            name: "stop_session".to_string(),
            label: "Stopping the triage one".to_string(),
            outcome: ToolOutcome { output, ok: true },
        })
        .expect("tool done");
}

#[tokio::test]
async fn the_card_goes_up_on_an_unsure_stop_and_cancel_takes_it_down() {
    let mut call = Call::start(None, false).await;
    call.client_frame("ready").await;
    stop_done(&call, "c1", control::unsure_result(&sessions()[..2]));
    let tool = call.client_frame("tool").await;
    assert_eq!(tool["label"], control::UNSURE_LABEL);
    let card = call.client_frame("confirm").await;
    assert_eq!(card["id"], "c1");
    assert_eq!(card["choices"].as_array().map(Vec::len), Some(2));

    call.tx
        .send(Input::Confirm {
            id: "c1".into(),
            session: None,
        })
        .unwrap();
    let done = call.client_frame("confirmDone").await;
    assert_eq!(
        done,
        json!({"type": "confirmDone", "id": "c1", "outcome": "cancelled"})
    );
    let told = call.model_message("[Ghostex card]").await;
    assert!(told.contains("Cancel"), "{told}");
    call.end();
}

#[tokio::test]
async fn stop_on_the_card_runs_the_stop_and_the_model_hears_the_result() {
    let mut call = Call::start(None, false).await;
    call.client_frame("ready").await;
    stop_done(&call, "c1", control::unsure_result(&sessions()[..1]));
    call.client_frame("confirm").await;
    // A session the card does not offer is refused and the card stays.
    call.tx
        .send(Input::Confirm {
            id: "c1".into(),
            session: Some("P1:S3".into()),
        })
        .unwrap();
    let refused = call.client_frame("error").await;
    assert_eq!(refused["code"], "badRequest");

    call.tx
        .send(Input::Confirm {
            id: "c1".into(),
            session: Some("P1:S1".into()),
        })
        .unwrap();
    let done = call.client_frame("confirmDone").await;
    assert_eq!(done["outcome"], "confirmed");
    let started = call.client_frame("tool").await;
    assert_eq!(started["phase"], "started");
    assert_eq!(started["label"], "Stopping Discord triage");
    // The test gxserver has no sessions, so the stop itself fails; its result still reaches the
    // client's row and the model.
    let finished = call.client_frame("tool").await;
    assert_eq!(finished["phase"], "finished");
    assert_eq!(finished["callId"], started["callId"]);
    let told = call.model_message("pressed Stop on the card").await;
    assert!(told.contains("Discord triage"), "{told}");
    call.end();
}

#[tokio::test]
async fn saying_yes_stops_it_and_the_card_comes_down() {
    let mut call = Call::start(None, false).await;
    call.client_frame("ready").await;
    stop_done(&call, "c1", control::unsure_result(&sessions()[..2]));
    call.client_frame("confirm").await;
    // The model calls stop_session again with the ref the user confirmed.
    stop_done(
        &call,
        "c2",
        json!({"ok": true, "stopped": true, "session": sessions()[1].brief()}),
    );
    let done = call.client_frame("confirmDone").await;
    assert_eq!(
        done,
        json!({"type": "confirmDone", "id": "c1", "outcome": "stopped"})
    );
    // A card no longer on screen ignores a late press.
    call.tx
        .send(Input::Confirm {
            id: "c1".into(),
            session: Some("P1:S1".into()),
        })
        .unwrap();
    assert!(call
        .client_frames("confirmDone", Duration::from_millis(300))
        .await
        .is_empty());
    call.end();
}

/// Opt-in live check against this computer's running gxserver: runs the real `stop_session` tool,
/// as the model would call it, with the words in `GX_VOICE_LIVE_STOP`.
/// `GX_VOICE_LIVE_STOP="…" cargo test --lib voice_agent::control_tests::live_ -- --ignored --nocapture`
#[tokio::test]
#[ignore = "drives the running gxserver; set GX_VOICE_LIVE_STOP"]
async fn live_stop_session() {
    let words = std::env::var("GX_VOICE_LIVE_STOP").expect("GX_VOICE_LIVE_STOP");
    let temp = tempfile::tempdir().expect("temp");
    let state = super::focus_tests::live_state(&temp);
    // As a client on another computer: the tool never switches the user's app to the session.
    let memory = ToolMemory::new(true);
    let outcome = tools::run(&state, &memory, "stop_session", &json!({"session": words})).await;
    println!("stop_session ok={} output={}", outcome.ok, outcome.output);
    if let Some(card) = control::confirm_frame("live", &outcome.output) {
        println!("confirm card: {card}");
    }
}

/// Opt-in live check: runs the real `start_coordinator` tool with `GX_VOICE_LIVE_PROJECT` and
/// `GX_VOICE_LIVE_MESSAGE` (and `GX_VOICE_LIVE_TITLE`, when set).
#[tokio::test]
#[ignore = "creates a coordinator in the running gxserver; set GX_VOICE_LIVE_PROJECT"]
async fn live_start_coordinator() {
    let var = |key: &str| std::env::var(key).unwrap_or_default();
    let temp = tempfile::tempdir().expect("temp");
    let state = super::focus_tests::live_state(&temp);
    // As a client on another computer: the tool never switches the user's app to the session.
    let memory = ToolMemory::new(true);
    let args = json!({
        "project": var("GX_VOICE_LIVE_PROJECT"),
        "message": var("GX_VOICE_LIVE_MESSAGE"),
        "title": var("GX_VOICE_LIVE_TITLE"),
    });
    let outcome = tools::run(&state, &memory, "start_coordinator", &args).await;
    println!(
        "start_coordinator ok={} output={}",
        outcome.ok, outcome.output
    );
}
