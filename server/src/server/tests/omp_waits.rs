//! OMP's tool approvals and `ask` questions through `/api/ingestAgentHookEvent`, as Ghostex's OMP
//! extension forwards them: a PermissionRequest for the approval dialog OMP shows, the dialog's
//! answer as `post_approval_response`, and the `ask` call's pre- and post-tool events. They take
//! Claude Code's path (stored card, attention, needs-input row, `awaitingApproval`,
//! `awaitingAnswer`), here in OMP 18.13.3's order from live run 9, with OMP's OSC 7501 records read
//! through the watch line's parser and the presentation projection the phone's summary forwards.

use super::*;
use crate::program_status::ProgramStatusRecord;
use crate::session_chat::SessionChatInteractivePrompt;

const ASK_CALL: &str = "toolu_ask_1";
const QUESTION: &str = "Which colour?";

struct Fixture {
    state: Arc<AppState>,
    project: Value,
    _temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let db = open_gxserver_database(&state.paths).expect("db");
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project = repository
            .create_project(
                json!({ "name": "Questions", "path": temp.path() })
                    .as_object()
                    .expect("params"),
            )
            .expect("project");
        Self {
            state,
            project,
            _temp: temp,
        }
    }

    fn project_id(&self) -> &str {
        self.project["projectId"].as_str().expect("projectId")
    }

    fn session(&self, agent: &str) -> String {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        let repository = DomainRepository::new(&db, self.state.metadata.server_id.as_str());
        repository
            .create_session(
                json!({
                    "projectId": self.project_id(), "kind": "agent", "agentId": agent,
                    "title": "Pick a colour", "lifecycleState": "running",
                    "runtimeSettings": { "agentName": agent, "agentActivity": { "activity": "working" } },
                })
                .as_object()
                .expect("params"),
                false,
            )
            .expect("session")["sessionId"]
            .as_str()
            .expect("sessionId")
            .to_string()
    }

    fn omp_session(&self) -> String {
        self.session("omp")
    }

    /// A session whose terminal Ghostex watches (its zmx exists and a title observer runs), as a
    /// live session is, so its OSC 7501 records stay while the session is published. The observer
    /// is a stand-in task, so no zmx process starts.
    fn watched_session(&self, agent: &str) -> String {
        let session = self.session(agent);
        let zmx_name = self.stored_session(&session)["zmxName"]
            .as_str()
            .expect("zmxName")
            .to_string();
        let db = open_gxserver_database(&self.state.paths).expect("db");
        db.execute(
            r#"UPDATE sessions SET providerStateJson = json_set(providerStateJson, '$.lifecycleState', 'exists')
            WHERE projectId = ?1 AND sessionId = ?2"#,
            rusqlite::params![self.project_id(), session],
        )
        .expect("provider exists");
        self.state
            .zmx_title_observers
            .lock()
            .expect("observers")
            .insert(
                crate::server::session_observer_key(self.project_id(), &session),
                crate::server::ZmxTitleObserverTask {
                    handle: tokio::spawn(std::future::pending::<()>()),
                    zmx_name,
                },
            );
        session
    }

    /// A watched OMP session running a prompt, with OMP reporting `working`. The watch saw OMP start
    /// (a snapshot without records), as it does for an OMP started under it.
    async fn running_omp_turn(&self) -> String {
        let session = self.watched_session("omp");
        self.records(&session, Vec::new());
        self.hook(&session, json!({ "eventName": "SessionStart" }))
            .await;
        self.hook(
            &session,
            json!({ "eventName": "UserPromptSubmit", "prompt": "Go" }),
        )
        .await;
        self.report(&session, 1, "working", None, None);
        session
    }

    /// The params the notify hook posts for an agent's hook (agent_hooks/notify_runtime.rs);
    /// `agentName` defaults to `omp`. Each hook is its own process, so two never share a
    /// millisecond.
    async fn hook(&self, session_id: &str, params: Value) {
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let mut params = params.as_object().cloned().expect("params");
        params
            .entry("agentName".to_string())
            .or_insert(json!("omp"));
        if let Some(event) = params.get("eventName").cloned() {
            params.insert("rawEventName".to_string(), event);
        }
        params.insert("projectId".to_string(), json!(self.project_id()));
        params.insert("sessionId".to_string(), json!(session_id));
        let routed = route_http(
            self.state.clone(),
            rpc_request(
                "/api/ingestAgentHookEvent",
                &self.state.auth_token,
                json!({ "params": params }),
            ),
            "request-omp-wait-hook".to_string(),
        )
        .await;
        assert_eq!(routed.response.status(), StatusCode::OK);
    }

    /// One `zmx watch-title` line with OMP's root record as zmx sends it (`seq` grows with every
    /// report, also one that repeats the record unchanged), through the watch line's own parser.
    fn report(
        &self,
        session_id: &str,
        seq: u64,
        state: &str,
        kind: Option<&str>,
        msg: Option<&str>,
    ) {
        self.records(
            session_id,
            vec![status_record("", seq, state, "omp", kind, msg)],
        );
    }

    fn records(&self, session_id: &str, records: Vec<ProgramStatusRecord>) {
        let line = json!({ "programStatus": { "reported": true, "records": records } });
        let parsed = crate::program_status::parse_program_status_line(&line.to_string())
            .expect("a program-status watch line");
        crate::program_status::ingest_program_status_line(
            &self.state,
            self.project_id(),
            session_id,
            parsed,
        );
    }

    /// What the desktop sends when the user looks at the waiting session (as answering from its
    /// card does).
    async fn acknowledge(&self, session_id: &str) {
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let routed = route_http(
            self.state.clone(),
            rpc_request(
                "/api/updateAgentActivity",
                &self.state.auth_token,
                json!({ "params": {
                    "projectId": self.project_id(), "sessionId": session_id, "event": "acknowledge",
                } }),
            ),
            "request-acknowledge".to_string(),
        )
        .await;
        assert_eq!(routed.response.status(), StatusCode::OK);
    }

    /// A running child (the fleet reader's marker) or none.
    fn set_fleet(&self, session_id: &str, running: bool) {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        let sql = if running {
            r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatFleetDetectedAt', '2026-11-03T14:37:00.000Z')
            WHERE projectId = ?1 AND sessionId = ?2"#
        } else {
            r#"UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.sessionChatFleetDetectedAt')
            WHERE projectId = ?1 AND sessionId = ?2"#
        };
        db.execute(sql, rusqlite::params![self.project_id(), session_id])
            .expect("fleet marker");
    }

    fn stored_session(&self, session_id: &str) -> Value {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        DomainRepository::new(&db, self.state.metadata.server_id.as_str())
            .get_session(self.project_id(), session_id)
            .expect("read session")
            .expect("session")
    }

    fn stored_prompt(&self, session_id: &str) -> Option<SessionChatInteractivePrompt> {
        crate::agents::session_chat_prompt_setting(&self.stored_session(session_id))
            .as_deref()
            .and_then(crate::session_chat::parse_stored_session_chat_prompt)
    }

    /// A field of the presentation session, as the presentation snapshot projects it.
    fn flag(&self, session_id: &str, name: &str) -> Option<Value> {
        let generated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        crate::presentation::project_presentation_session(
            &self.project,
            "group",
            &self.stored_session(session_id),
            &generated_at,
        )
        .get(name)
        .cloned()
    }

    fn awaiting_answer(&self, session_id: &str) -> Option<Value> {
        self.flag(session_id, "awaitingAnswer")
    }

    fn awaiting_approval(&self, session_id: &str) -> Option<Value> {
        self.flag(session_id, "awaitingApproval")
    }

    /// The session's feed rows of `kind`, oldest first: (attention event, read, body).
    fn rows(&self, session_id: &str, kind: &str) -> Vec<(Option<String>, bool, String)> {
        let db = open_gxserver_database(&self.state.paths).expect("db");
        let mut statement = db
            .prepare(
                "SELECT attentionEventId, readAt IS NOT NULL, body FROM notification_feed
                WHERE sessionId = ?1 AND kind = ?2 ORDER BY createdAt, rowid",
            )
            .expect("feed query");
        statement
            .query_map(rusqlite::params![session_id, kind], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .expect("feed rows")
            .map(|row| row.expect("feed row"))
            .collect()
    }

    fn activity(&self, session_id: &str) -> Value {
        self.stored_session(session_id)["runtimeSettings"]["agentActivity"]["activity"].clone()
    }

    fn attention_event(&self, session_id: &str) -> Option<String> {
        self.stored_session(session_id)["runtimeSettings"]["agentActivity"]["attentionEventId"]
            .as_str()
            .map(str::to_string)
    }

    fn program_state(&self, session_id: &str) -> Value {
        self.flag(session_id, "programStatus")
            .expect("programStatus")["state"]
            .clone()
    }

    /// Working as every client reads it: stored and presented working with its start time.
    fn assert_working(&self, session_id: &str, label: &str) {
        assert_eq!(self.activity(session_id), json!("working"), "{label}");
        assert_eq!(
            self.flag(session_id, "activity"),
            Some(json!("working")),
            "{label}"
        );
        assert!(
            self.flag(session_id, "workingStartedAt")
                .is_some_and(|started| started.is_string()),
            "{label}"
        );
    }

    /// A new wait was announced: its own attention event with an unread needs-input row saying
    /// `body`, shown as attention with its entry time. Returns the event.
    fn assert_new_wait(
        &self,
        session_id: &str,
        seen: &[String],
        body: &str,
        label: &str,
    ) -> String {
        let event = self.attention_event(session_id).expect("attention event");
        assert!(!seen.contains(&event), "{label}: the wait before it again");
        assert_eq!(self.activity(session_id), json!("attention"), "{label}");
        let attention = self.flag(session_id, "attention").expect("attention");
        assert_eq!(attention["eventId"], json!(event), "{label}");
        assert!(attention["enteredAt"].is_string(), "{label}");
        assert_eq!(
            self.rows(session_id, "needsInput").last(),
            Some(&(Some(event.clone()), false, body.to_string())),
            "{label}"
        );
        event
    }

    fn forget(&self, session_id: &str) {
        crate::program_status::forget_program_status(self.project_id(), session_id);
    }
}

fn status_record(
    id: &str,
    seq: u64,
    state: &str,
    app: &str,
    kind: Option<&str>,
    msg: Option<&str>,
) -> ProgramStatusRecord {
    ProgramStatusRecord {
        id: id.to_string(),
        state: state.to_string(),
        kind: kind.map(str::to_string),
        app: Some(app.to_string()),
        msg: msg.map(str::to_string),
        seq,
        updated_at_ms: chrono::Utc::now().timestamp_millis(),
        ..Default::default()
    }
}

/// What Ghostex's OMP extension sends when OMP starts its `ask` tool.
fn ask_started() -> Value {
    ask_call(ASK_CALL)
}

fn ask_call(call: &str) -> Value {
    json!({
        "eventName": "PreToolUse",
        "toolName": "ask",
        "toolUseId": call,
        "toolInput": { "questions": [{
            "question": QUESTION,
            "options": [{ "label": "Blue" }, { "label": "Yellow", "description": "Warmer" }],
            "recommended": 1,
        }] },
    })
}

fn ask_ended(call: &str) -> Value {
    json!({ "eventName": "PostToolUse", "toolName": "ask", "toolUseId": call })
}

/// The extension's events for one `bash` call: its start and end, the PermissionRequest for its
/// dialog once OMP shows it (with the command from OMP's `tool_call` event) and the dialog's answer.
fn bash_start(call: &str) -> Value {
    json!({ "eventName": "PreToolUse", "toolName": "bash", "toolUseId": call })
}

fn bash_end(call: &str) -> Value {
    json!({ "eventName": "PostToolUse", "toolName": "bash", "toolUseId": call })
}

fn dialog_shown(call: &str, command: &str) -> Value {
    json!({
        "eventName": "PermissionRequest", "toolName": "bash", "toolUseId": call,
        "toolInput": { "command": command },
    })
}

fn dialog_answered(call: &str) -> Value {
    json!({ "eventName": "post_approval_response", "toolUseId": call })
}

/// OMP 18.13.3's approval prompt text for a `bash` call.
fn allow(command: &str) -> String {
    format!("Allow tool: bash Command: {command}")
}

fn approval_body(command: &str) -> String {
    format!("Waiting for your approval: bash: {command}")
}

#[tokio::test]
async fn an_omp_ask_awaits_an_answer_until_the_asking_call_ends() {
    let fixture = Fixture::new();
    let session = fixture.omp_session();
    assert_eq!(fixture.awaiting_answer(&session), None);

    fixture.hook(&session, ask_started()).await;
    let Some(SessionChatInteractivePrompt::Question {
        questions,
        tool_use_id,
    }) = fixture.stored_prompt(&session)
    else {
        panic!(
            "no question card stored: {:?}",
            fixture.stored_prompt(&session)
        );
    };
    assert_eq!(tool_use_id.as_deref(), Some(ASK_CALL));
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0].question, QUESTION);
    assert_eq!(questions[0].tool_name.as_deref(), Some("ask"));
    assert_eq!(questions[0].recommended, Some(1));
    assert!(!questions[0].multi_select);
    let labels: Vec<&str> = questions[0]
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect();
    assert_eq!(labels, ["Blue", "Yellow"]);
    assert_eq!(
        questions[0].options[1].description.as_deref(),
        Some("Warmer")
    );
    assert_eq!(fixture.awaiting_answer(&session), Some(json!(true)));

    // Another tool of the same run starting and finishing leaves the question open.
    fixture.hook(&session, bash_start("toolu_read")).await;
    fixture.hook(&session, bash_end("toolu_read")).await;
    assert_eq!(fixture.awaiting_answer(&session), Some(json!(true)));

    // The answer ends the call.
    fixture.hook(&session, ask_ended(ASK_CALL)).await;
    assert_eq!(fixture.stored_prompt(&session), None);
    assert_eq!(fixture.awaiting_answer(&session), None);
}

#[tokio::test]
async fn an_interrupted_omp_turn_ends_its_open_question() {
    let fixture = Fixture::new();
    let session = fixture.omp_session();
    fixture.hook(&session, ask_started()).await;
    assert_eq!(fixture.awaiting_answer(&session), Some(json!(true)));

    fixture
        .hook(&session, json!({ "eventName": "Interrupt" }))
        .await;
    assert_eq!(fixture.stored_prompt(&session), None);
    assert_eq!(fixture.awaiting_answer(&session), None);
}

/// An OMP extension installed before the forwarding names the tool only, as before.
#[tokio::test]
async fn an_omp_ask_without_its_questions_stores_no_card() {
    let fixture = Fixture::new();
    let session = fixture.omp_session();
    fixture
        .hook(
            &session,
            json!({ "eventName": "PreToolUse", "toolName": "ask" }),
        )
        .await;
    assert_eq!(fixture.stored_prompt(&session), None);
    assert_eq!(fixture.awaiting_answer(&session), None);
}

/// Step 2 of the live test plan in OMP 18.13.3's order: the `ask` tool starts (OMP reports
/// `blocked`/`question` and the extension's pre-tool event arrives before or after that report),
/// OMP repeats its record while the question waits and through its custom-answer prompt, and the
/// answer ends the tool (OMP reports `working`, the post-tool event before or after it). One wait:
/// one row, attention until the user looks, the question card until the answer, as Claude's
/// AskUserQuestion; no question dot (`pendingQuestionCount` counts question forms only).
#[tokio::test]
async fn step_2_an_ask_answered_with_own_text_is_one_wait() {
    for (start_first, end_first) in [(false, false), (true, true), (false, true), (true, false)] {
        let label = format!("start event first: {start_first}, end event first: {end_first}");
        let fixture = Fixture::new();
        let session = fixture.running_omp_turn().await;
        if start_first {
            fixture.hook(&session, ask_started()).await;
        }
        fixture.report(&session, 2, "blocked", Some("question"), Some(QUESTION));
        if !start_first {
            fixture.hook(&session, ask_started()).await;
        }
        fixture.assert_new_wait(&session, &[], "Waiting for your input", &label);
        assert_eq!(
            fixture.awaiting_answer(&session),
            Some(json!(true)),
            "{label}"
        );
        assert_eq!(
            fixture.flag(&session, "pendingQuestionCount"),
            Some(json!(0)),
            "{label}"
        );
        assert_ne!(fixture.program_state(&session), json!("blocked"), "{label}");

        fixture.report(&session, 3, "blocked", Some("question"), Some(QUESTION));
        fixture.acknowledge(&session).await;
        for seq in [4, 5, 6] {
            fixture.report(&session, seq, "blocked", Some("question"), Some(QUESTION));
        }
        let rows = fixture.rows(&session, "needsInput");
        assert_eq!(rows.len(), 1, "no second wait ({label})");
        assert!(rows[0].1, "read once looked at ({label})");
        assert_eq!(fixture.activity(&session), json!("idle"), "{label}");
        assert_eq!(fixture.flag(&session, "attention"), None, "{label}");
        assert_eq!(
            fixture.awaiting_answer(&session),
            Some(json!(true)),
            "{label}"
        );

        if end_first {
            fixture.hook(&session, ask_ended(ASK_CALL)).await;
        }
        fixture.report(&session, 7, "working", None, None);
        if !end_first {
            fixture.hook(&session, ask_ended(ASK_CALL)).await;
        }
        assert_eq!(fixture.awaiting_answer(&session), None, "{label}");
        assert_eq!(fixture.rows(&session, "needsInput").len(), 1, "{label}");
        fixture.assert_working(&session, &label);
        fixture.forget(&session);
    }
}

/// Step 2b: an `ask` answered by picking an option: one wait, which the answer ends.
#[tokio::test]
async fn step_2b_an_ask_answered_by_an_option_is_one_wait() {
    let fixture = Fixture::new();
    let session = fixture.running_omp_turn().await;
    fixture.report(
        &session,
        2,
        "blocked",
        Some("question"),
        Some("Which number?"),
    );
    fixture.hook(&session, ask_call("toolu_ask_2b")).await;
    fixture.assert_new_wait(&session, &[], "Waiting for your input", "asked");
    assert_eq!(fixture.awaiting_answer(&session), Some(json!(true)));
    fixture.report(&session, 3, "working", None, None);
    fixture.hook(&session, ask_ended("toolu_ask_2b")).await;
    assert_eq!(fixture.rows(&session, "needsInput").len(), 1);
    assert_eq!(fixture.awaiting_answer(&session), None);
    fixture.assert_working(&session, "answered");
    fixture.forget(&session);
}

/// Steps 5 and 7 as OMP runs one approved call per assistant message (step 7: two foreground
/// `sleep 45`). The tool starts (OMP reports `blocked`/`permission` and keeps it, repeating it,
/// until the tool ends), OMP shows the dialog (PermissionRequest), the user looks and answers
/// (`post_approval_response`, before or after a repeat), the tool runs and ends. Each dialog is one
/// wait with the card's text in its row; after the look the session reads idle with the red
/// approval flag, as Claude's does; the answer retires the card and the session reads working
/// while the approved tool runs; the turn's end is the one finished row.
#[tokio::test]
async fn steps_5_and_7_each_approval_is_a_claude_permission_request() {
    let plans: [(&str, &[&str]); 2] = [
        ("step 7", &["sleep 45", "sleep 45"]),
        (
            "step 5",
            &["echo ghostex-approval-test", "echo second-approval"],
        ),
    ];
    for (step, commands) in plans {
        for answer_first in [false, true] {
            let fixture = Fixture::new();
            let session = fixture.running_omp_turn().await;
            let mut seq = 1;
            let mut seen = Vec::new();
            for (index, command) in commands.iter().enumerate() {
                let label = format!("{step}, call {index}, answer before repeat: {answer_first}");
                let call = format!("toolu_{index}");
                let prompt = allow(command);
                seq += 1;
                fixture.report(&session, seq, "blocked", Some("permission"), Some(&prompt));
                fixture.hook(&session, bash_start(&call)).await;
                fixture.hook(&session, dialog_shown(&call, command)).await;
                seen.push(fixture.assert_new_wait(
                    &session,
                    &seen,
                    &approval_body(command),
                    &label,
                ));
                assert_eq!(
                    fixture.stored_prompt(&session),
                    Some(SessionChatInteractivePrompt::Approval {
                        tool: "bash".to_string(),
                        summary: Some(command.to_string()),
                        tool_use_id: Some(call.clone()),
                    }),
                    "{label}"
                );
                assert_eq!(
                    fixture.awaiting_approval(&session),
                    Some(json!(true)),
                    "{label}"
                );
                assert_ne!(fixture.program_state(&session), json!("blocked"), "{label}");

                fixture.acknowledge(&session).await;
                seq += 1;
                fixture.report(&session, seq, "blocked", Some("permission"), Some(&prompt));
                assert_eq!(fixture.activity(&session), json!("idle"), "{label}");
                assert_eq!(fixture.flag(&session, "attention"), None, "{label}");
                assert_eq!(
                    fixture.awaiting_approval(&session),
                    Some(json!(true)),
                    "{label}"
                );

                if answer_first {
                    fixture.hook(&session, dialog_answered(&call)).await;
                }
                seq += 1;
                fixture.report(&session, seq, "blocked", Some("permission"), Some(&prompt));
                if !answer_first {
                    fixture.hook(&session, dialog_answered(&call)).await;
                }
                assert_eq!(fixture.stored_prompt(&session), None, "{label}");
                assert_eq!(fixture.awaiting_approval(&session), None, "{label}");
                fixture.assert_working(&session, &label);
                let started = fixture.flag(&session, "workingStartedAt");
                seq += 1;
                fixture.report(&session, seq, "blocked", Some("permission"), Some(&prompt));
                fixture.assert_working(&session, &label);
                assert_eq!(
                    fixture.flag(&session, "workingStartedAt"),
                    started,
                    "{label}"
                );
                seq += 1;
                fixture.report(&session, seq, "working", None, None);
                fixture.hook(&session, bash_end(&call)).await;
                fixture.assert_working(&session, &label);
                assert_eq!(
                    fixture.rows(&session, "needsInput").len(),
                    index + 1,
                    "{label}"
                );
            }
            fixture.report(&session, seq + 1, "done", None, None);
            fixture.hook(&session, json!({ "eventName": "Stop" })).await;
            assert_eq!(fixture.rows(&session, "finished").len(), 1, "{step}");
            fixture.forget(&session);
        }
    }
}

/// Step 5 in the order live run 9 recorded it: Haiku sent three `bash` calls in one assistant
/// message (14:25:29.66Z); OMP started them together, requested the three approvals at once and
/// showed one dialog at a time. The extension forwards the dialog OMP shows: card 1 approved (its
/// `sleep 3` ends at 14:25:46 while card 2 shows), card 2 denied (14:26:11), card 3 denied with a
/// message (14:26:37). Every dialog is its own wait with its own row and notification (run 9 F5:
/// only the first was). The first call's end while card 2 shows reads working, as a Claude
/// subagent's tool hook does; card 2 stays stored and its row is already out.
#[tokio::test]
async fn step_5_three_approvals_in_one_message_are_three_waits() {
    let fixture = Fixture::new();
    let session = fixture.running_omp_turn().await;
    let commands = [
        ("toolu_a", "sleep 3 && echo ghostex-approval-test"),
        ("toolu_b", "echo second-approval"),
        ("toolu_c", "echo third-approval"),
    ];
    fixture.report(
        &session,
        2,
        "blocked",
        Some("permission"),
        Some(&allow(commands[0].1)),
    );
    for (call, _) in commands {
        fixture.hook(&session, bash_start(call)).await;
    }
    let mut seen = Vec::new();
    fixture
        .hook(&session, dialog_shown(commands[0].0, commands[0].1))
        .await;
    seen.push(fixture.assert_new_wait(&session, &seen, &approval_body(commands[0].1), "card 1"));
    fixture.acknowledge(&session).await;
    fixture.hook(&session, dialog_answered(commands[0].0)).await;
    fixture.assert_working(&session, "card 1 approved");

    fixture
        .hook(&session, dialog_shown(commands[1].0, commands[1].1))
        .await;
    seen.push(fixture.assert_new_wait(&session, &seen, &approval_body(commands[1].1), "card 2"));
    assert_eq!(fixture.awaiting_approval(&session), Some(json!(true)));
    fixture.report(
        &session,
        3,
        "blocked",
        Some("permission"),
        Some(&allow(commands[1].1)),
    );
    fixture.hook(&session, bash_end(commands[0].0)).await;
    assert_eq!(fixture.activity(&session), json!("working"));
    assert_eq!(fixture.awaiting_approval(&session), None);
    assert_eq!(
        fixture
            .stored_prompt(&session)
            .as_ref()
            .and_then(SessionChatInteractivePrompt::tool_use_id),
        Some(commands[1].0)
    );
    fixture.hook(&session, dialog_answered(commands[1].0)).await;
    fixture.hook(&session, bash_end(commands[1].0)).await;

    fixture
        .hook(&session, dialog_shown(commands[2].0, commands[2].1))
        .await;
    seen.push(fixture.assert_new_wait(&session, &seen, &approval_body(commands[2].1), "card 3"));
    fixture.acknowledge(&session).await;
    fixture.hook(&session, dialog_answered(commands[2].0)).await;
    fixture.hook(&session, bash_end(commands[2].0)).await;
    fixture.report(&session, 4, "working", None, None);
    assert_eq!(fixture.awaiting_approval(&session), None);
    assert_eq!(fixture.stored_prompt(&session), None);
    fixture.assert_working(&session, "all answered");
    let events: Vec<Option<String>> = fixture
        .rows(&session, "needsInput")
        .into_iter()
        .map(|(event, _, _)| event)
        .collect();
    assert_eq!(events, seen.into_iter().map(Some).collect::<Vec<_>>());
    fixture.forget(&session);
}

/// The same approval read the same way for Claude Code and OMP (10.40.0's Claude behaviour): an
/// open request is attention with the red flag; after the look it reads idle with the flag; while
/// a subagent runs it reads working without the flag; when the child stops the flag is back.
#[tokio::test]
async fn an_omp_approval_reads_like_a_claude_one() {
    let mut readings = Vec::new();
    for agent in ["claude", "omp"] {
        let fixture = Fixture::new();
        let session = fixture.session(agent);
        let (tool, call) = if agent == "claude" {
            ("Bash", None)
        } else {
            ("bash", Some("toolu_1"))
        };
        let mut start = json!({ "eventName": "PreToolUse", "agentName": agent, "toolName": tool });
        let mut request = json!({
            "eventName": "PermissionRequest", "agentName": agent, "toolName": tool,
            "toolInput": { "command": "sleep 45" },
        });
        if let Some(call) = call {
            start["toolUseId"] = json!(call);
            request["toolUseId"] = json!(call);
        }
        fixture.hook(&session, start).await;
        fixture.hook(&session, request).await;
        let read = |fixture: &Fixture| {
            (
                fixture.flag(&session, "activity"),
                fixture.flag(&session, "attention").is_some(),
                fixture.awaiting_approval(&session),
            )
        };
        let open = read(&fixture);
        let body = fixture.rows(&session, "needsInput")[0].2.clone();
        fixture.acknowledge(&session).await;
        let looked_at = read(&fixture);
        fixture.set_fleet(&session, true);
        let child_runs = read(&fixture);
        fixture.set_fleet(&session, false);
        let child_stopped = read(&fixture);
        readings.push((open, looked_at, child_runs, child_stopped, body));
    }
    let (claude, omp) = (&readings[0], &readings[1]);
    assert_eq!(
        claude.0,
        (Some(json!("attention")), true, Some(json!(true)))
    );
    assert_eq!(claude.1, (Some(json!("idle")), false, Some(json!(true))));
    assert_eq!(claude.2, (Some(json!("working")), false, None));
    assert_eq!(claude.3, claude.1);
    assert_eq!(
        (&omp.0, &omp.1, &omp.2, &omp.3),
        (&claude.0, &claude.1, &claude.2, &claude.3)
    );
    assert_eq!(claude.4, "Waiting for your approval: Bash: sleep 45");
    assert_eq!(omp.4, approval_body("sleep 45"));
}

/// OMP's own `blocked` record raises nothing (its hooks carry the waits); another program's root
/// record and an OMP worker's record keep raising theirs, and OMP's `done` still finishes a turn.
#[tokio::test]
async fn omp_blocked_reports_raise_nothing_and_other_reports_keep_their_waits() {
    let fixture = Fixture::new();
    let session = fixture.running_omp_turn().await;
    fixture.report(
        &session,
        2,
        "blocked",
        Some("permission"),
        Some(&allow("rm -rf build")),
    );
    fixture.report(
        &session,
        3,
        "blocked",
        Some("permission"),
        Some(&allow("rm -rf build")),
    );
    assert_eq!(fixture.rows(&session, "needsInput"), []);
    assert_eq!(fixture.activity(&session), json!("working"));
    assert_ne!(fixture.program_state(&session), json!("blocked"));
    fixture.report(&session, 4, "done", None, None);
    assert_eq!(fixture.activity(&session), json!("attention"));
    fixture.forget(&session);

    for (label, record) in [
        (
            "another program's root",
            status_record("", 2, "blocked", "codex", Some("permission"), None),
        ),
        (
            "an OMP worker",
            status_record("worker-1", 2, "blocked", "omp", Some("permission"), None),
        ),
    ] {
        let session = fixture.watched_session("codex");
        fixture.records(&session, vec![record]);
        assert_eq!(fixture.activity(&session), json!("attention"), "{label}");
        assert_eq!(fixture.rows(&session, "needsInput").len(), 1, "{label}");
        assert_eq!(fixture.program_state(&session), json!("blocked"), "{label}");
        fixture.forget(&session);
    }
}
