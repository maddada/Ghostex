//! Family f's wiring check: drives a [`ChatCore`] through the actions family f owns and asserts
//! what the document says afterwards.
//!
//! It grades the plumbing around the rules, which a table of pure inputs cannot reach: the panel
//! folds and their storage write, the search cursor surviving a re-query, the terminal tail's two
//! independent reads, the subagent viewer's page walk and poll, the Save to Markdown sheet's two
//! Docs calls, and the stint word coming off the context rather than out of the core.
//!
//! ```text
//! cargo run --example extras_check
//! ```

use std::process::ExitCode;

use ghostex_gx_chat_core::{
    ActionKind, ChatContext, ChatCore, Effect, Event, RpcOutcome, UserAction,
};
use serde_json::{json, Map, Value};

fn main() -> ExitCode {
    let mut checks = Checks::default();
    panels(&mut checks);
    search(&mut checks);
    terminal_tail(&mut checks);
    subagent(&mut checks);
    save_markdown(&mut checks);
    working_word(&mut checks);
    loading_stage(&mut checks);

    println!("checks      {} run", checks.run);
    println!("failures    {}", checks.failures.len());
    for failure in &checks.failures {
        println!("  {failure}");
    }
    if checks.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ---------------------------------------------------------------------------

fn panels(checks: &mut Checks) {
    let mut core = Core::new();
    core.state_mut().session.agent = Some("claude".to_string());
    core.state_mut().session.agent_tasks = Some(json!({
        "tasks": [
            { "id": "1", "subject": "Read", "status": "completed" },
            { "id": "2", "subject": "Write", "status": "in_progress", "activeForm": "Writing" },
        ]
    }));
    core.act(ActionKind::ToggleAgentTasks, json!({ "open": false }));
    checks.eq(
        "tasks fold is collapsed",
        core.document()["agentTasksPanel"]["collapsed"].clone(),
        json!(true),
    );
    checks.eq(
        "the collapsed header carries the running task",
        core.document()["agentTasksPanel"]["meta"].clone(),
        json!("1 of 2 done \u{b7} Writing"),
    );
    checks.eq(
        "the fold is written back to storage",
        json!(matches!(
            core.last_effects.first(),
            Some(Effect::WriteStorage { value: Some(value), .. }) if value == "1"
        )),
        json!(true),
    );
    core.act(ActionKind::ToggleAgentTasks, json!({ "open": true }));
    checks.eq(
        "expanded drops the running task from the header",
        core.document()["agentTasksPanel"]["meta"].clone(),
        json!("1 of 2 done"),
    );

    // A fresh plan closes the done pile again.
    core.act(
        ActionKind::ToggleAgentTasksCompleted,
        json!({ "expanded": true }),
    );
    checks.eq(
        "the done pile opens",
        core.document()["agentTasksPanel"]["showCompleted"].clone(),
        json!(true),
    );
    core.state_mut().session.agent_tasks = Some(json!({
        "tasks": [{ "id": "1", "subject": "Only", "status": "pending" }]
    }));
    core.tick();
    checks.eq(
        "a fresh plan closes the done pile",
        core.document()["agentTasksPanel"]["showCompleted"].clone(),
        json!(false),
    );

    core.act(ActionKind::ToggleAgentFleet, json!({ "open": true }));
    checks.eq(
        "an absent roster still publishes null",
        core.document()["agentFleetStrip"].clone(),
        Value::Null,
    );
    core.state_mut().session.agent_fleet = Some(json!({
        "detectedAt": "2026-09-22T11:59:00.000Z",
        "agents": [{ "name": "general-purpose", "task": "Audit", "status": "working", "elapsedSeconds": 30 }],
    }));
    core.tick();
    checks.eq(
        "the fleet override is published as is",
        core.document()["agentFleetStrip"]["openOverride"].clone(),
        json!(true),
    );
    checks.eq(
        "the roster counts what is running",
        core.document()["agentFleetStrip"]["countLabel"].clone(),
        json!("1 running"),
    );
}

fn search(checks: &mut Checks) {
    let mut core = Core::new();
    checks.eq(
        "a closed field publishes null",
        core.document()["transcriptSearch"].clone(),
        Value::Null,
    );
    core.act(ActionKind::SearchOpen, json!({}));
    checks.eq(
        "an open field with no query has no counter",
        core.document()["transcriptSearch"]["label"].clone(),
        json!(""),
    );
    core.act(ActionKind::SearchQuery, json!({ "query": "cat" }));
    let revision = core.document()["transcriptSearch"]["revision"].clone();
    core.act(ActionKind::SearchQuery, json!({ "query": "cat" }));
    checks.eq(
        "the same query does not move the cursor",
        core.document()["transcriptSearch"]["revision"].clone(),
        revision,
    );
    core.act(ActionKind::SearchClose, json!({}));
    checks.eq(
        "closing clears the field",
        core.document()["transcriptSearch"].clone(),
        Value::Null,
    );
}

fn terminal_tail(checks: &mut Checks) {
    let mut core = Core::new();
    checks.eq(
        "nothing is read before the first hover",
        core.document()["terminalTail"]["readiness"].clone(),
        Value::Null,
    );
    core.act(ActionKind::TerminalTailHover, json!({}));
    let id = core.first_rpc().expect("the hover asks for a read");
    core.settle_ok(
        id,
        json!({
            "captured": true,
            "composerState": "notReady",
            "reason": "The agent is mid-turn.",
            "lines": ["\u{2500}".repeat(40), "  hello", "\u{2500}".repeat(40)],
        }),
    );
    checks.eq(
        "a measured verdict tints the button",
        core.document()["terminalTail"]["readiness"].clone(),
        json!("notReady"),
    );
    checks.eq(
        "the hover preview cuts the rules down to the widest real row",
        core.document()["terminalTail"]["preview"].clone(),
        json!(format!(
            "{}\n  hello\n{}",
            "\u{2500}".repeat(24),
            "\u{2500}".repeat(24)
        )),
    );

    // A failed hover keeps the last verdict; it is never "not ready".
    core.act(ActionKind::TerminalTailHover, json!({}));
    let id = core.first_rpc().expect("the second hover asks again");
    core.settle_err(id, "the socket went away");
    checks.eq(
        "a failed hover keeps the last verdict",
        core.document()["terminalTail"]["readiness"].clone(),
        json!("notReady"),
    );

    // The expanded sheet only lives while the refusal that opened it does.
    core.state_mut().core.operation_error_code = Some("composerNotReady".to_string());
    core.act(ActionKind::TerminalTailToggle, json!({}));
    let id = core.first_rpc().expect("expanding re-reads the screen");
    checks.eq(
        "the sheet says it is loading",
        core.document()["terminalTail"]["notice"]["loading"].clone(),
        json!(true),
    );
    core.settle_ok(id, json!({ "captured": false, "lines": [] }));
    checks.eq(
        "an unreadable screen says so",
        core.document()["terminalTail"]["notice"]["empty"].clone(),
        json!("Ghostex could not read this session\u{2019}s terminal screen."),
    );
    core.state_mut().core.operation_error_code = None;
    core.tick();
    checks.eq(
        "the sheet retires with the refusal",
        core.document()["terminalTail"]["notice"]["open"].clone(),
        json!(false),
    );
}

fn subagent(checks: &mut Checks) {
    let mut core = Core::new();
    checks.eq(
        "a closed viewer publishes null",
        core.document()["subagent"].clone(),
        Value::Null,
    );
    core.act(
        ActionKind::OpenSubagent,
        json!({ "selector": "child-1", "name": "Auditor", "task": "Audit the config" }),
    );
    let id = core.first_rpc().expect("opening reads the child");
    checks.eq(
        "the header waits for the model",
        core.document()["subagent"]["title"].clone(),
        json!("Loading\u{2026}"),
    );
    core.settle_ok(
        id,
        json!({
            "subagent": { "id": "child-1", "name": "auditor", "model": "claude-opus-4-5", "effort": "high" },
            "messages": [{ "id": "m1", "byteOffset": 100 }],
            "hasMore": false,
            "beforeOffset": 100,
            "epoch": 1,
            "seq": 1,
        }),
    );
    checks.eq(
        "the header shows the child's own model",
        core.document()["subagent"]["title"].clone(),
        json!("Opus 4.5 High"),
    );
    checks.eq(
        "a read page is no longer loading",
        core.document()["subagent"]["loading"].clone(),
        json!(false),
    );

    // A daemon that answers without a child must never paint the main conversation.
    core.act(ActionKind::SubagentRetry, json!({}));
    let id = core.first_rpc().expect("retry reads again");
    core.settle_ok(
        id,
        json!({ "messages": [], "hasMore": false, "beforeOffset": 0 }),
    );
    checks.eq(
        "an old daemon is refused, not drawn",
        core.document()["subagent"]["error"].clone(),
        json!("This server needs an update to read subagent transcripts."),
    );

    core.act(ActionKind::SubagentClose, json!({}));
    checks.eq(
        "closing hides the modal",
        core.document()["subagent"].clone(),
        Value::Null,
    );
}

fn save_markdown(checks: &mut Checks) {
    let mut core = Core::new();
    core.state_mut().core.title = Some("Fixing the build".to_string());
    checks.eq(
        "a closed sheet publishes null",
        core.document()["saveMarkdown"].clone(),
        Value::Null,
    );
    core.act(
        ActionKind::MarkdownSaveOpen,
        json!({ "markdown": "# Title\n\nbody\n" }),
    );
    let listing = core.first_rpc().expect("opening lists the Docs tree");
    checks.eq(
        "the sheet offers today's folder",
        core.document()["saveMarkdown"]["folder"].clone(),
        json!("2026-09-22"),
    );
    checks.eq(
        "the sheet is loading its listing",
        core.document()["saveMarkdown"]["loading"].clone(),
        json!(true),
    );
    // Submitting before the listing answers is refused, because the numbering is not known yet.
    core.act(ActionKind::MarkdownSaveSubmit, json!({}));
    checks.eq(
        "submit waits for the listing",
        core.document()["saveMarkdown"]["saving"].clone(),
        json!(false),
    );
    core.settle_ok(
        listing,
        json!({
            "requestId": "list-message-markdown-1",
            "entries": [
                { "kind": "file", "path": "docs/2026-09-22/Fixing the build 2.md" },
                { "kind": "file", "path": "docs/2026-09-22/notes.txt" },
            ],
        }),
    );
    checks.eq(
        "the suggestion numbers past what is there",
        core.document()["saveMarkdown"]["fileName"].clone(),
        json!("Fixing the build 3"),
    );

    core.act(
        ActionKind::MarkdownSaveName,
        json!({ "value": "My notes.MD" }),
    );
    checks.eq(
        "typing a name drops the extension and the suggestion",
        core.document()["saveMarkdown"]["fileName"].clone(),
        json!("My notes"),
    );
    core.act(ActionKind::MarkdownSaveSubmit, json!({}));
    let save = core.first_rpc().expect("submit writes the file");
    checks.eq(
        "the sheet says it is saving",
        core.document()["saveMarkdown"]["saving"].clone(),
        json!(true),
    );
    core.settle_ok(
        save,
        json!({ "requestId": "save-message-markdown-2", "file": { "path": "docs/2026-09-22/My notes.md" } }),
    );
    let resolve = core
        .first_rpc()
        .expect("the save resolves an absolute path");
    core.settle_ok(
        resolve,
        json!({ "requestId": "saved-message-path-3", "fullPath": "/projects/app/docs/2026-09-22/My notes.md" }),
    );
    checks.eq(
        "a saved file closes the sheet",
        core.document()["saveMarkdown"].clone(),
        Value::Null,
    );
    checks.eq(
        "the host is told to copy the path",
        json!(matches!(
            core.last_effects.first(),
            Some(Effect::MarkdownSaved { .. })
        )),
        json!(true),
    );

    // A refused name never reaches Docs.
    core.act(ActionKind::MarkdownSaveOpen, json!({ "markdown": "x" }));
    let listing = core.first_rpc().expect("the second sheet lists again");
    core.settle_ok(
        listing,
        json!({ "requestId": "list-message-markdown-4", "entries": [] }),
    );
    core.act(ActionKind::MarkdownSaveName, json!({ "value": "nul.txt" }));
    core.act(ActionKind::MarkdownSaveSubmit, json!({}));
    checks.eq(
        "a reserved name is refused",
        core.document()["saveMarkdown"]["fileNameError"].clone(),
        json!("That file name is reserved by the operating system."),
    );
}

fn working_word(checks: &mut Checks) {
    let mut core = Core::new();
    checks.eq(
        "an idle session shows no word",
        core.document()["workingStrip"]["label"].clone(),
        Value::Null,
    );
    // The first computation is idle, so the initializer's draw is spent and never shown.
    core.state_mut().session.server_working = true;
    core.context.random_units = [0.723_172_509_809_956, 0.0];
    core.tick();
    checks.eq(
        "the stint word comes off the context, not out of the core",
        core.document()["workingStrip"]["label"].clone(),
        json!("Puzzling\u{2026}"),
    );
    // A live activity says what is actually happening, so the word steps aside.
    core.state_mut().session.terminal_activity = Some(json!({
        "kind": "compacting",
        "label": "Compacting",
        "detectedAt": "2026-09-22T11:59:30.000Z",
        "elapsedSeconds": 4,
    }));
    core.tick();
    checks.eq(
        "an activity replaces the word",
        core.document()["workingStrip"]["label"].clone(),
        Value::Null,
    );
    checks.eq(
        "the activity carries its own clock",
        core.document()["workingStrip"]["presentation"]["elapsedLabel"].clone(),
        json!("34s"),
    );
    checks.eq(
        "compaction says what a queued message will do",
        core.document()["workingStrip"]["presentation"]["hint"].clone(),
        json!("Send or queue a message and it will be posted after compaction"),
    );
}

fn loading_stage(checks: &mut Checks) {
    // The published key is gated on `view.kind`, which family a computes; the stage itself is
    // family f's, so this drives the carried value and the document key follows once `view` folds.
    let mut core = Core::new();
    checks.eq(
        "a read that has just started is blank",
        json!(core.core.state().extras.loading_stage),
        json!("blank"),
    );
    core.tick();
    checks.eq(
        "the skeleton shows at once",
        json!(core.core.state().extras.loading_stage),
        json!("indicator"),
    );
    core.context.now_ms += 12_000.0;
    core.tick();
    checks.eq(
        "a long read offers Retry",
        json!(core.core.state().extras.loading_stage),
        json!("retry"),
    );
    core.state_mut().session.server_status = ghostex_gx_chat_core::protocol::ChatStatus::Ready;
    core.tick();
    checks.eq(
        "a finished read drops the stage",
        json!(core.core.state().extras.loading_stage),
        json!("blank"),
    );
    checks.eq(
        "a transcript that is not loading publishes no stage",
        core.document()["loadingStage"].clone(),
        Value::Null,
    );
}

// ---------------------------------------------------------------------------

/// A core plus the clock and the effects its last call produced.
struct Core {
    core: ChatCore,
    context: ChatContext,
    last_effects: Vec<Effect>,
    document: Value,
}

impl Core {
    fn new() -> Self {
        let mut core = Self {
            core: ChatCore::new(),
            // 2026-09-22T12:00:00.000Z, a fixed instant so the clocks below are stable.
            context: ChatContext::at(1_790_078_400_000.0),
            last_effects: Vec::new(),
            document: Value::Null,
        };
        // Nothing publishes before the composer boot read answers, the same way `startController`
        // is only called from `composer('read')`'s `.then(...)`.
        core.drive(Event::ComposerBootRead(Box::default()));
        core
    }

    fn state_mut(&mut self) -> &mut ghostex_gx_chat_core::ChatState {
        self.core.state_mut()
    }

    fn act(&mut self, kind: ActionKind, params: Value) {
        let params: Map<String, Value> = params.as_object().cloned().unwrap_or_default();
        self.drive(Event::Action(Box::new(UserAction { kind, params })));
    }

    fn tick(&mut self) {
        self.drive(Event::Tick);
    }

    fn settle_ok(&mut self, request_id: u64, result: Value) {
        self.drive(Event::RpcSettled {
            request_id,
            outcome: Box::new(RpcOutcome::Ok { result }),
        });
    }

    fn settle_err(&mut self, request_id: u64, message: &str) {
        self.drive(Event::RpcSettled {
            request_id,
            outcome: Box::new(RpcOutcome::Err {
                code: None,
                message: message.to_string(),
                endpoint: None,
            }),
        });
    }

    fn drive(&mut self, event: Event) {
        self.last_effects = self.core.handle(event, self.context.clone());
        self.document = serde_json::to_value(self.core.document()).expect("the document is JSON");
    }

    fn document(&self) -> &Value {
        &self.document
    }

    /// The id of the first gxserver call the last action asked for.
    fn first_rpc(&self) -> Option<u64> {
        self.last_effects.iter().find_map(|effect| match effect {
            Effect::SendRpc { request_id, .. } => Some(*request_id),
            _ => None,
        })
    }
}

/// What passed and what did not.
#[derive(Default)]
struct Checks {
    run: usize,
    failures: Vec<String>,
}

impl Checks {
    fn eq(&mut self, what: &str, actual: Value, expected: Value) {
        self.run += 1;
        if actual != expected {
            self.failures.push(format!(
                "{what}\n    expected {expected}\n    actual   {actual}"
            ));
        }
    }
}
