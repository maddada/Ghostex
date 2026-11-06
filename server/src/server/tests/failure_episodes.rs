//! Failed rows once per error episode: each row of `cases` runs one sequence on a fresh temporary
//! state database through the paths that change a session's account recovery state (the hook
//! route, the account route's `sessionPolicy` save and cached `titlebar` answer, the recovery
//! pass's `save`, `collect` and `plan_session` reading the detection cache, the composer's
//! `user_action`), with SQLite refusing chosen writes where a row needs it. After each `Expect` it
//! checks the session's failed rows, the episode marker in the stored recovery state, and whether
//! the account answer, read by the shared chat core's account panel, shows a recovery.

use super::*;
use crate::accounts::api_errors::FAILURE_NOTIFIED_AT;
use crate::accounts::continuation::SwitchSource;
use crate::accounts::model::{Policy, Provider, Snapshot};
use crate::accounts::{recovery, store};
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
};

/// The episode marker on the stored recovery state.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Marker {
    /// No `failureNotifiedAt`, or no recovery state at all.
    Absent,
    /// `failureNotifiedAt: null`: a row came due and could not be stored.
    Owed,
    /// `failureNotifiedAt` holds the time the row was stored.
    Stored,
}

#[derive(Clone, Copy)]
enum Fault {
    /// SQLite refuses every failed row.
    FailedRow,
    /// SQLite refuses a session write that stores a marker time.
    Marker,
}

enum Step {
    /// A recovery state built fresh, as the pass builds a new wait, through `recovery::save`.
    Save(Value),
    /// The stored recovery state with these fields changed, through `recovery::save`.
    Update(Value),
    /// One agent hook event through `/api/ingestAgentHookEvent`.
    Hook(Value),
    /// `/api/agentAccounts` `sessionPolicy`: Continue automatically off for this session.
    PolicyOff,
    /// Both providers' defaults switched off in the account registry.
    DefaultsOff,
    /// What the agent's screen shows, as the detection cache holds it.
    Screen(Option<&'static str>),
    /// One recovery pass over every session that is not stopped.
    Collect,
    /// The recovery pass's plan for this session.
    Plan,
    /// The user sends from the composer.
    Composer,
    /// Stop retrying.
    StopRetrying,
    /// The session's lifecycle state, as a wake or sleep leaves it.
    Lifecycle(&'static str),
    /// `endpoint::select` onto the default login: an automatic switch (`true`), or the account the
    /// user picks (`false`, the source the account route passes).
    Switch(bool),
    /// `Switch` from the remembered read.
    SwitchSnapshot(bool),
    /// Moves the session's typed continuation this many seconds into the past.
    AgeTyped(u64),
    /// Whether a typed continuation still waits for its prompt hook.
    ExpectTyped(bool),
    /// Ends the session's background tasks (the continuation an automatic switch starts).
    StopBackground,
    /// Ghostex types this continuation for the stored recovery claim (`note_continuation`, as
    /// `recovery::deliver` and the switch continuation do right before sending).
    Typed(&'static str),
    /// Remembers the session as stored now, the read an episode writer starts from.
    Snapshot,
    /// `recovery::save` from the remembered read.
    SaveSnapshot(Value),
    /// `api_errors::announce_episode` from the remembered read.
    AnnounceSnapshot,
    /// The account route's settings save (Continue automatically off) from the remembered read.
    SettingsOffSnapshot,
    /// The async-question publisher's own field write, on another connection.
    PublishQuestion,
    /// Whether the session holds an open question card.
    ExpectCard(bool),
    /// The pending question count the question dot and auto-sleep read.
    ExpectQuestions(usize),
    Refuse(Fault),
    Allow(Fault),
    /// The user reads the feed (a later row then cannot replace an unread one).
    Read,
    /// Failed rows of this session, the marker, and whether a recovery is shown.
    Expect(usize, Marker, bool),
}

struct Case {
    name: &'static str,
    agent: &'static str,
    runtime: Value,
    steps: Vec<Step>,
}

fn connection_error(status: &str, attempt: u64) -> Value {
    json!({
        "status": status, "trigger": "streamError", "attempt": attempt,
        "reason": "Claude Code hit a temporary service error",
        "errorText": "API Error: Connection refused (ECONNREFUSED)",
    })
}

/// The pass's writes around each quick try: claimed, sent, and the failure seen after it.
fn failed_tries(attempts: std::ops::RangeInclusive<u64>) -> Vec<Step> {
    attempts
        .flat_map(|attempt| {
            ["retrying", "resumed", "waiting"]
                .map(|status| Step::Save(connection_error(status, attempt)))
        })
        .collect()
}

fn hook_failure(error: &str) -> Step {
    Step::Hook(json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": error }))
}

/// A prompt hook as the hook runtime forwards it: its text only as `promptDigest`.
fn prompt_hook(text: &str) -> Step {
    Step::Hook(json!({
        "agentName": "claude", "eventName": "UserPromptSubmit",
        "promptDigest": crate::accounts::api_errors::prompt_digest(text),
    }))
}

/// The user's own prompt typed into the agent's terminal.
fn user_prompt() -> Step {
    prompt_hook("A separate run")
}

/// What recovery types as its continuation in these rows.
const CONTINUATION_TEXT: &str = "Continue the unfinished task.";

/// A recovery try (or Retry now) prepared: its claim stands for the continuation Ghostex will type.
fn continuation(claim: &str) -> Value {
    json!({
        "status": "retrying", "trigger": "streamError", "attempt": 6, "claim": claim,
        "reason": "Claude Code hit a temporary service error",
    })
}

fn question_card() -> Step {
    Step::Hook(json!({
        "agentName": "claude", "eventName": "PreToolUse", "toolName": "AskUserQuestion",
        "toolUseId": "newer-card",
        "toolInput": { "questions": [{ "question": "Which one?", "options": [{ "label": "A" }, { "label": "B" }] }] },
    }))
}

fn retries_off() -> Value {
    json!({ "enabled": true, "atLimit": "wait", "priority": "soonestReset", "retryErrors": false })
}

/// A session on a saved account with a conversation to resume, so a switch onto the default login
/// changes its account without a fresh launch.
fn switched_session() -> Value {
    json!({ "accountId": "episode-account-old", "agentSessionId": "episode-claude-session" })
}

fn minutes_ago(minutes: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339()
}

fn cases() -> Vec<Case> {
    let waiting_error = || json!({ "accountRecovery": connection_error("waiting", 2) });
    vec![
        Case {
            name: "a hook's failed run keeps a marker the account answer does not show",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Expect(1, Marker::Stored, false),
            ],
        },
        Case {
            name: "a marker stays bookkeeping while the agent makes progress",
            agent: "codex",
            runtime: json!({
                "agentActivity": { "activity": "working", "workingSource": "explicit" },
                "accountRecovery": { FAILURE_NOTIFIED_AT: minutes_ago(5), "healthySince": minutes_ago(2) },
            }),
            steps: vec![
                Step::Screen(None),
                Step::Plan,
                Step::Expect(0, Marker::Stored, false),
            ],
        },
        Case {
            name: "wait metadata at attempt zero keeps the episode",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("unknown"),
                Step::Save(connection_error("waiting", 0)),
                Step::Update(json!({ "waitingForNetwork": true })),
                Step::Expect(1, Marker::Stored, true),
                Step::Read,
                Step::Save(connection_error("waiting", 5)),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "an unchanged settings save keeps a settings close's marker",
            agent: "claude",
            runtime: waiting_error(),
            steps: vec![
                Step::PolicyOff,
                Step::Expect(1, Marker::Stored, false),
                Step::PolicyOff,
                Step::Expect(1, Marker::Stored, false),
            ],
        },
        Case {
            name: "a late hook after a settings close is the same episode",
            agent: "claude",
            runtime: waiting_error(),
            steps: vec![
                Step::PolicyOff,
                Step::Expect(1, Marker::Stored, false),
                Step::Read,
                hook_failure("server_error"),
                Step::Expect(1, Marker::Stored, false),
            ],
        },
        Case {
            name: "a prompt the user submits in the terminal starts a new episode",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                user_prompt(),
                Step::Expect(1, Marker::Absent, false),
                hook_failure("authentication_failed"),
                Step::Expect(2, Marker::Stored, false),
            ],
        },
        Case {
            name: "the prompt of recovery's own continuation keeps the episode",
            agent: "claude",
            runtime: json!({ "accountRecovery": {
                "status": "retrying", "trigger": "streamError", "attempt": 6, "claim": "continuation",
                FAILURE_NOTIFIED_AT: minutes_ago(3),
            }}),
            steps: vec![
                Step::Typed(CONTINUATION_TEXT),
                prompt_hook(CONTINUATION_TEXT),
                Step::Expect(0, Marker::Stored, true),
            ],
        },
        Case {
            name: "the continuation's prompt and then the user's prompt end the episode",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Save(continuation("continuation-1")),
                Step::Typed(CONTINUATION_TEXT),
                prompt_hook(CONTINUATION_TEXT),
                Step::Expect(1, Marker::Stored, true),
                Step::Update(json!({ "status": "resumed" })),
                user_prompt(),
                Step::Expect(1, Marker::Absent, true),
                hook_failure("authentication_failed"),
                Step::Expect(2, Marker::Stored, true),
            ],
        },
        Case {
            name: "a continuation's prompt arriving after its send was recorded keeps the episode",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Save(continuation("continuation-1")),
                Step::Typed(CONTINUATION_TEXT),
                Step::Update(json!({ "status": "resumed" })),
                prompt_hook(CONTINUATION_TEXT),
                Step::Expect(1, Marker::Stored, true),
                hook_failure("authentication_failed"),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "each new continuation's prompt is its own again",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Save(continuation("continuation-1")),
                Step::Typed(CONTINUATION_TEXT),
                prompt_hook(CONTINUATION_TEXT),
                Step::Update(json!({ "status": "resumed" })),
                Step::Update(json!({ "status": "retrying", "claim": "continuation-2" })),
                Step::Typed(CONTINUATION_TEXT),
                prompt_hook(CONTINUATION_TEXT),
                Step::Expect(1, Marker::Stored, true),
                hook_failure("authentication_failed"),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "a user prompt while a continuation is prepared but not typed starts a new episode",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Save(continuation("continuation-1")),
                user_prompt(),
                Step::Expect(1, Marker::Absent, true),
                hook_failure("authentication_failed"),
                Step::Expect(2, Marker::Stored, true),
                Step::Read,
                Step::Typed(CONTINUATION_TEXT),
                prompt_hook(CONTINUATION_TEXT),
                Step::Expect(2, Marker::Stored, true),
            ],
        },
        Case {
            name: "a user prompt while a continuation is typed starts a new episode",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Save(continuation("continuation-1")),
                Step::Typed(CONTINUATION_TEXT),
                user_prompt(),
                Step::Expect(1, Marker::Absent, true),
                prompt_hook(CONTINUATION_TEXT),
                hook_failure("authentication_failed"),
                Step::Expect(2, Marker::Stored, true),
            ],
        },
        Case {
            name: "a prompt hook without text after a continuation was typed is that continuation",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Save(continuation("continuation-1")),
                Step::Typed(CONTINUATION_TEXT),
                Step::Hook(json!({ "agentName": "claude", "eventName": "UserPromptSubmit" })),
                Step::Expect(1, Marker::Stored, true),
                Step::Hook(json!({ "agentName": "claude", "eventName": "UserPromptSubmit" })),
                Step::Expect(1, Marker::Absent, true),
            ],
        },
        Case {
            name: "a new episode past the quick tries is announced at the next failed try",
            agent: "claude",
            runtime: json!({}),
            steps: [
                vec![Step::Save(connection_error("waiting", 0))],
                failed_tries(1..=5),
                vec![
                    Step::Expect(1, Marker::Stored, true),
                    Step::Read,
                    user_prompt(),
                    Step::Expect(1, Marker::Absent, true),
                    hook_failure("server_error"),
                    Step::Expect(1, Marker::Absent, true),
                ],
                failed_tries(6..=6),
                vec![Step::Expect(2, Marker::Stored, true)],
            ]
            .into_iter()
            .flatten()
            .collect(),
        },
        Case {
            name: "a prompt typed while recovery waits starts a new episode without announcing the wait again",
            agent: "claude",
            runtime: json!({ "accountRecovery": {
                "status": "waiting", "trigger": "streamError", "attempt": 7,
                FAILURE_NOTIFIED_AT: minutes_ago(3),
            }}),
            steps: vec![
                user_prompt(),
                Step::Expect(0, Marker::Absent, true),
                Step::Update(json!({ "waitingForNetwork": true })),
                Step::Expect(0, Marker::Absent, true),
                hook_failure("authentication_failed"),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "a prompt typed after recovery was switched off starts a new episode",
            agent: "claude",
            runtime: json!({ "accountRecovery": {
                "status": "waiting", "trigger": "streamError", "attempt": 7,
                FAILURE_NOTIFIED_AT: minutes_ago(3),
            }}),
            steps: vec![
                Step::DefaultsOff,
                user_prompt(),
                Step::Collect,
                Step::Expect(0, Marker::Absent, false),
                hook_failure("authentication_failed"),
                Step::Expect(1, Marker::Stored, false),
            ],
        },
        Case {
            name: "a hook's newer question card survives the episode's row",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                Step::Snapshot,
                question_card(),
                Step::ExpectCard(true),
                Step::AnnounceSnapshot,
                Step::Expect(1, Marker::Stored, false),
                Step::ExpectCard(true),
            ],
        },
        Case {
            name: "a user prompt with exactly the continuation's text counts as the continuation",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Save(continuation("continuation-1")),
                Step::Typed("."),
                prompt_hook("."),
                Step::Expect(1, Marker::Stored, true),
                hook_failure("authentication_failed"),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "a typed continuation is forgotten when recovery takes another claim",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                Step::Save(continuation("continuation-1")),
                Step::Typed(CONTINUATION_TEXT),
                Step::ExpectTyped(true),
                Step::Update(json!({ "claim": "continuation-2" })),
                Step::ExpectTyped(false),
            ],
        },
        Case {
            name: "a typed continuation is forgotten when Stop retrying removes recovery",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                Step::Save(continuation("continuation-1")),
                Step::Typed(CONTINUATION_TEXT),
                Step::StopRetrying,
                Step::ExpectTyped(false),
            ],
        },
        Case {
            name: "a typed continuation is forgotten when the session is stopped",
            agent: "codex",
            runtime: json!({}),
            steps: vec![
                Step::Save(continuation("continuation-1")),
                Step::Typed(CONTINUATION_TEXT),
                Step::Lifecycle("stopped"),
                Step::DefaultsOff,
                Step::Collect,
                Step::ExpectTyped(false),
            ],
        },
        Case {
            name: "a continuation's hook after its window counts as a user prompt",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Save(continuation("continuation-1")),
                Step::Typed(CONTINUATION_TEXT),
                Step::AgeTyped(121),
                prompt_hook(CONTINUATION_TEXT),
                Step::Expect(1, Marker::Absent, true),
                Step::ExpectTyped(false),
            ],
        },
        Case {
            name: "an account switch from an earlier read keeps a newer card and published question",
            agent: "claude",
            runtime: switched_session(),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Lifecycle("sleeping"),
                Step::Snapshot,
                question_card(),
                Step::PublishQuestion,
                Step::SwitchSnapshot(false),
                Step::Expect(1, Marker::Absent, false),
                Step::Lifecycle("running"),
                Step::ExpectCard(true),
                Step::ExpectQuestions(1),
            ],
        },
        Case {
            name: "a prompt typed while an unannounced wait is no longer retried retires the wait",
            agent: "claude",
            runtime: json!({ "accountRecovery": connection_error("waiting", 0) }),
            steps: vec![
                Step::DefaultsOff,
                user_prompt(),
                Step::Collect,
                Step::Expect(0, Marker::Absent, false),
            ],
        },
        Case {
            name: "a recovery save that announces nothing keeps newer runtime members",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Save(continuation("continuation-1")),
                Step::Snapshot,
                question_card(),
                Step::PublishQuestion,
                Step::SaveSnapshot(continuation("continuation-2")),
                Step::Expect(1, Marker::Stored, true),
                Step::ExpectCard(true),
                Step::ExpectQuestions(1),
            ],
        },
        Case {
            name: "a newer published question survives the episode's row",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                Step::Snapshot,
                Step::PublishQuestion,
                Step::ExpectQuestions(1),
                Step::AnnounceSnapshot,
                Step::Expect(1, Marker::Stored, false),
                Step::ExpectQuestions(1),
            ],
        },
        Case {
            name: "a newer question card survives a recovery save that announces",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                Step::Save(connection_error("waiting", 4)),
                Step::Snapshot,
                question_card(),
                Step::SaveSnapshot(connection_error("waiting", 5)),
                Step::Expect(1, Marker::Stored, true),
                Step::ExpectCard(true),
            ],
        },
        Case {
            name: "a newer published question survives a settings save that closes the episode",
            agent: "claude",
            runtime: waiting_error(),
            steps: vec![
                Step::Snapshot,
                Step::PublishQuestion,
                Step::SettingsOffSnapshot,
                Step::Expect(1, Marker::Stored, false),
                Step::ExpectQuestions(1),
            ],
        },
        Case {
            name: "a prompt typed after recovery gave up starts a new episode",
            agent: "claude",
            runtime: json!({ "accountRecovery": {
                "status": "needsAttention", "trigger": "streamError", "attempt": 3,
                FAILURE_NOTIFIED_AT: minutes_ago(3),
            }}),
            steps: vec![
                user_prompt(),
                Step::Expect(0, Marker::Absent, true),
                hook_failure("authentication_failed"),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "a composer send starts a new episode",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Composer,
                Step::Expect(1, Marker::Absent, false),
                hook_failure("authentication_failed"),
                Step::Expect(2, Marker::Stored, false),
            ],
        },
        Case {
            name: "an automatic account switch and its continuation keep the episode",
            agent: "claude",
            runtime: switched_session(),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Lifecycle("sleeping"),
                Step::Switch(true),
                Step::Expect(1, Marker::Stored, true),
                Step::StopBackground,
                Step::Lifecycle("running"),
                Step::Typed("."),
                prompt_hook("."),
                hook_failure("authentication_failed"),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "an account the user picks starts a new episode",
            agent: "claude",
            runtime: switched_session(),
            steps: vec![
                hook_failure("authentication_failed"),
                Step::Read,
                Step::Lifecycle("sleeping"),
                Step::Switch(false),
                Step::Expect(1, Marker::Absent, false),
                Step::Lifecycle("running"),
                hook_failure("authentication_failed"),
                Step::Expect(2, Marker::Stored, false),
            ],
        },
        Case {
            name: "a refused row while recovery gives up is stored on the episode's next save",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                Step::Save(connection_error("waiting", 2)),
                Step::Refuse(Fault::FailedRow),
                Step::Update(json!({ "status": "needsAttention" })),
                Step::Expect(0, Marker::Owed, true),
                Step::Allow(Fault::FailedRow),
                Step::Update(json!({ "updatedAt": minutes_ago(0) })),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "a refused row while recovery gives up is stored on the next pass",
            agent: "codex",
            runtime: json!({}),
            steps: vec![
                Step::Save(connection_error("waiting", 2)),
                Step::Refuse(Fault::FailedRow),
                Step::Update(json!({ "status": "needsAttention" })),
                Step::Expect(0, Marker::Owed, true),
                Step::Allow(Fault::FailedRow),
                Step::DefaultsOff,
                Step::Collect,
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "a refused row at a settings close is stored by the next settings save",
            agent: "claude",
            runtime: waiting_error(),
            steps: vec![
                Step::Refuse(Fault::FailedRow),
                Step::PolicyOff,
                Step::Expect(0, Marker::Owed, true),
                Step::Allow(Fault::FailedRow),
                Step::PolicyOff,
                Step::Expect(1, Marker::Stored, false),
            ],
        },
        Case {
            name: "a refused row at a settings close is stored on the next pass",
            agent: "codex",
            runtime: waiting_error(),
            steps: vec![
                Step::Refuse(Fault::FailedRow),
                Step::PolicyOff,
                Step::Expect(0, Marker::Owed, true),
                Step::Allow(Fault::FailedRow),
                Step::DefaultsOff,
                Step::Collect,
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "the failed row and its marker are stored together or not at all",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                Step::Save(connection_error("waiting", 4)),
                Step::Refuse(Fault::Marker),
                Step::Save(connection_error("waiting", 5)),
                Step::Expect(0, Marker::Owed, true),
                Step::Allow(Fault::Marker),
                Step::Read,
                Step::Save(connection_error("waiting", 5)),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "the collection's fast path does not skip a close recovery owes",
            agent: "codex",
            runtime: waiting_error(),
            steps: vec![
                Step::DefaultsOff,
                Step::Collect,
                Step::Expect(1, Marker::Stored, true),
                Step::Collect,
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "an earlier healthy record does not hide an error recovery will not retry",
            agent: "codex",
            runtime: json!({
                "agentActivity": { "activity": "idle" },
                "accountPolicyOverride": retries_off(),
                "accountRecovery": {
                    "status": "resumed", "trigger": "streamError", "attempt": 0,
                    "reason": "The agent is making progress again.",
                },
            }),
            steps: vec![
                Step::Screen(Some("streamError")),
                Step::Plan,
                Step::Expect(1, Marker::Stored, true),
                Step::Plan,
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "an error recovery never started because retries are off is announced once",
            agent: "codex",
            runtime: json!({
                "agentActivity": { "activity": "idle" },
                "accountPolicyOverride": retries_off(),
            }),
            steps: vec![
                Step::Screen(Some("streamError")),
                Step::Plan,
                Step::Expect(1, Marker::Stored, false),
                Step::Plan,
                Step::Expect(1, Marker::Stored, false),
            ],
        },
        Case {
            name: "the fifth failed quick try announces the episode once",
            agent: "claude",
            runtime: json!({}),
            steps: [
                vec![Step::Save(connection_error("waiting", 0))],
                failed_tries(1..=4),
                vec![
                    Step::Expect(0, Marker::Absent, true),
                    Step::Save(connection_error("retrying", 5)),
                    Step::Save(connection_error("resumed", 5)),
                    Step::Expect(0, Marker::Absent, true),
                    Step::Save(connection_error("waiting", 5)),
                    Step::Expect(1, Marker::Stored, true),
                    Step::Read,
                ],
                failed_tries(6..=7),
                vec![
                    Step::Update(json!({ "status": "needsAttention" })),
                    Step::Expect(1, Marker::Stored, true),
                ],
            ]
            .into_iter()
            .flatten()
            .collect(),
        },
        Case {
            name: "a new failure after an answered try starts a new episode",
            agent: "claude",
            runtime: json!({}),
            steps: [
                vec![Step::Save(connection_error("waiting", 0))],
                failed_tries(1..=5),
                vec![
                    Step::Save(connection_error("resumed", 6)),
                    Step::Read,
                    Step::Save(connection_error("waiting", 0)),
                    Step::Expect(1, Marker::Absent, true),
                ],
                failed_tries(1..=5),
                vec![Step::Expect(2, Marker::Stored, true)],
            ]
            .into_iter()
            .flatten()
            .collect(),
        },
        Case {
            name: "a minute of progress ends the episode",
            agent: "claude",
            runtime: json!({}),
            steps: [
                vec![Step::Save(connection_error("waiting", 0))],
                failed_tries(1..=5),
                vec![
                    Step::Update(json!({
                        "status": "resumed", "attempt": 0, "healthySince": minutes_ago(2),
                        "reason": "The agent is making progress again.",
                    })),
                    Step::Expect(1, Marker::Absent, true),
                ],
            ]
            .into_iter()
            .flatten()
            .collect(),
        },
        Case {
            name: "a hook first and recovery giving up later share one row",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                hook_failure("oauth_org_not_allowed"),
                Step::Read,
                Step::Save(json!({
                    "status": "needsAttention", "trigger": "accountDisabled", "attempt": 0,
                    "reason": "This account's organization disabled Claude subscription access.",
                })),
                Step::Expect(1, Marker::Stored, true),
            ],
        },
        Case {
            name: "recovery first and a hook later share one row",
            agent: "claude",
            runtime: json!({}),
            steps: [
                vec![Step::Save(connection_error("waiting", 0))],
                failed_tries(1..=5),
                vec![
                    Step::Read,
                    hook_failure("unknown"),
                    Step::Expect(1, Marker::Stored, true),
                ],
            ]
            .into_iter()
            .flatten()
            .collect(),
        },
        Case {
            name: "a limit wait that gives up never notifies",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                Step::Save(json!({ "status": "waiting", "trigger": "usageLimit", "attempt": 7 })),
                Step::Update(json!({ "status": "needsAttention" })),
                Step::Expect(0, Marker::Absent, true),
            ],
        },
        Case {
            name: "an unidentified exit that gives up never notifies",
            agent: "claude",
            runtime: json!({}),
            steps: vec![
                Step::Save(json!({ "status": "waiting", "trigger": "agentExited", "attempt": 1 })),
                Step::Update(json!({
                    "status": "needsAttention", "reason": "No saved conversation is available to resume.",
                })),
                Step::Expect(0, Marker::Absent, true),
            ],
        },
        Case {
            name: "Stop retrying never notifies",
            agent: "claude",
            runtime: json!({}),
            steps: [
                vec![Step::Save(connection_error("waiting", 0))],
                failed_tries(1..=4),
                vec![
                    Step::StopRetrying,
                    Step::Expect(0, Marker::Absent, false),
                    Step::PolicyOff,
                    Step::Expect(0, Marker::Absent, false),
                ],
            ]
            .into_iter()
            .flatten()
            .collect(),
        },
        Case {
            name: "a usage-limit hook never notifies",
            agent: "claude",
            runtime: json!({ "accountRecoverySuppressed": true }),
            steps: vec![
                hook_failure("rate_limit"),
                Step::Expect(0, Marker::Absent, false),
            ],
        },
    ]
}

struct Run {
    state: Arc<AppState>,
    project: String,
    session: String,
    agent: &'static str,
    snapshot: std::sync::Mutex<Option<Value>>,
    _temp: tempfile::TempDir,
}

impl Run {
    fn new(case: &Case) -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
        let db = open_gxserver_database(&state.paths).expect("db");
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project = repository
            .create_project(
                json!({ "name": "Episodes", "path": temp.path() })
                    .as_object()
                    .unwrap(),
            )
            .expect("project")["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        let mut runtime = case.runtime.as_object().cloned().unwrap_or_default();
        runtime
            .entry("agentActivity".to_string())
            .or_insert(json!({ "activity": "idle" }));
        runtime.insert("agentName".to_string(), json!(case.agent));
        let session = repository
            .create_session(
                json!({
                    "projectId": project, "kind": "agent", "agentId": case.agent,
                    "title": case.name, "lifecycleState": "running", "runtimeSettings": runtime,
                })
                .as_object()
                .unwrap(),
                false,
            )
            .expect("session")["sessionId"]
            .as_str()
            .expect("sessionId")
            .to_string();
        Self {
            state,
            project,
            session,
            agent: case.agent,
            snapshot: std::sync::Mutex::new(None),
            _temp: temp,
        }
    }

    fn db(&self) -> crate::storage::GxserverDatabase {
        open_gxserver_database(&self.state.paths).expect("db")
    }

    fn stored(&self) -> Value {
        let db = self.db();
        DomainRepository::new(&db, self.state.metadata.server_id.as_str())
            .get_session(&self.project, &self.session)
            .expect("read")
            .expect("session")
    }

    async fn route(&self, path: &str, params: Value) -> Value {
        let mut params = params.as_object().cloned().expect("params");
        params.insert("projectId".to_string(), json!(self.project));
        params.insert("sessionId".to_string(), json!(self.session));
        let routed = route_http(
            self.state.clone(),
            rpc_request(path, &self.state.auth_token, json!({ "params": params })),
            "request-failure-episode".to_string(),
        )
        .await;
        response_json(routed.response).await
    }

    /// Runs one step; an error a step returns is part of what the expectations measure.
    async fn step(&self, step: &Step) -> Result<(), String> {
        let db = self.db();
        let repository = DomainRepository::new(&db, self.state.metadata.server_id.as_str());
        match step {
            Step::Save(recovery) => {
                recovery::save(&self.state, &repository, &self.stored(), recovery.clone())
                    .map_err(|error| error.message)
            }
            Step::Update(fields) => {
                let session = self.stored();
                let mut recovery = session["runtimeSettings"]["accountRecovery"].clone();
                for (key, value) in fields.as_object().expect("fields") {
                    recovery[key] = value.clone();
                }
                recovery::save(&self.state, &repository, &session, recovery)
                    .map_err(|error| error.message)
            }
            Step::Hook(params) => {
                self.route("/api/ingestAgentHookEvent", params.clone()).await;
                Ok(())
            }
            Step::PolicyOff => {
                let policy = json!({ "enabled": false, "atLimit": "wait", "priority": "soonestReset", "retryErrors": false });
                let answer = self
                    .route(
                        "/api/agentAccounts",
                        json!({ "operation": "sessionPolicy", "policy": policy }),
                    )
                    .await;
                answer["error"].as_object().map_or(Ok(()), |error| Err(format!("{error:?}")))
            }
            Step::DefaultsOff => {
                let mut registry = store::read(&db).map_err(|error| error.message)?;
                for provider in [Provider::Claude, Provider::Codex] {
                    let mut off = Policy::default();
                    off.enabled = false;
                    registry.defaults.insert(provider, off);
                }
                store::write(&db, &registry).map_err(|error| error.message)
            }
            Step::Screen(kind) => {
                let notice = kind.map(|kind| {
                    SessionChatTerminalNotice::new(
                        kind,
                        SessionChatTerminalNoticeSeverity::Warning,
                        SessionChatTerminalNoticeSource::Screen,
                        "Stream error",
                    )
                });
                self.state.session_chat_option_cache.lock().unwrap().insert(
                    crate::server::session_observer_key(&self.project, &self.session),
                    crate::session_chat_options::SessionChatOptionCacheEntry {
                        fetched_at: std::time::Instant::now(),
                        projected_compacting: None,
                        projected_fleet: None,
                        projected_monitor: None,
                        model_grace_started: None,
                        retired_notice: None,
                        value: crate::session_chat_options::SessionChatTerminalDetection {
                            notice,
                            captured: true,
                            attempted: true,
                            ..Default::default()
                        },
                    },
                );
                Ok(())
            }
            Step::Collect => recovery::collect(&self.state, false, None)
                .map(|_| ())
                .map_err(|error| error.message),
            Step::Plan => {
                let registry = store::read(&db).map_err(|error| error.message)?;
                let detector = crate::session_chat_options::SessionChatOptionDetector::new(&self.state);
                recovery::plan_session(
                    &self.state,
                    &db,
                    &repository,
                    &registry,
                    &Snapshot::default(),
                    &detector,
                    &self.stored(),
                )
                .map(|_| ())
                .map_err(|error| error.message)
            }
            Step::Composer | Step::StopRetrying => recovery::user_action(
                &self.state,
                &self.project,
                &self.session,
                matches!(step, Step::StopRetrying),
            )
            .map_err(|error| error.message),
            Step::Refuse(Fault::FailedRow) => db
                .execute_batch(
                    "CREATE TRIGGER refuse_failed_row BEFORE INSERT ON notification_feed WHEN NEW.kind = 'failed' BEGIN SELECT RAISE(ABORT, 'failed row refused'); END;",
                )
                .map_err(|error| error.to_string()),
            Step::Refuse(Fault::Marker) => db
                .execute_batch(&format!(
                    "CREATE TRIGGER refuse_marker BEFORE UPDATE ON sessions WHEN json_type(NEW.runtimeSettingsJson, '$.accountRecovery.{FAILURE_NOTIFIED_AT}') = 'text' BEGIN SELECT RAISE(ABORT, 'marker refused'); END;"
                ))
                .map_err(|error| error.to_string()),
            Step::Allow(fault) => db
                .execute_batch(match fault {
                    Fault::FailedRow => "DROP TRIGGER refuse_failed_row;",
                    Fault::Marker => "DROP TRIGGER refuse_marker;",
                })
                .map_err(|error| error.to_string()),
            Step::Read => crate::notification_feed::store::mark_all_notifications_read(&db)
                .map(|_| ())
                .map_err(|error| error.message),
            Step::Lifecycle(lifecycle) => repository
                .update_session(
                    json!({
                        "projectId": self.project, "sessionId": self.session,
                        "lifecycleState": lifecycle,
                    })
                    .as_object()
                    .expect("params"),
                )
                .map(|_| ())
                .map_err(|error| error.message),
            Step::Switch(automatic) | Step::SwitchSnapshot(automatic) => {
                let session = if matches!(step, Step::SwitchSnapshot(_)) {
                    self.remembered()
                } else {
                    self.stored()
                };
                let registry = store::read(&db).map_err(|error| error.message)?;
                let project = repository
                    .get_project(&self.project)
                    .map_err(|error| error.message)?
                    .expect("project");
                crate::accounts::endpoint::select(
                    &self.state,
                    &repository,
                    &registry,
                    &Snapshot::default(),
                    &project,
                    &session,
                    None,
                    if *automatic {
                        SwitchSource::Automatic
                    } else {
                        SwitchSource::Manual
                    },
                )
                .map_err(|error| error.message)
            }
            Step::StopBackground => {
                let _ = self.state.shutdown_tx.send(());
                Ok(())
            }
            Step::Typed(text) => {
                let claim = self.stored()["runtimeSettings"]["accountRecovery"]["claim"]
                    .as_str()
                    .map(str::to_string)
                    .ok_or("no claim to type for")?;
                crate::accounts::api_errors::note_continuation(
                    &self.project,
                    &self.session,
                    &claim,
                    text,
                );
                Ok(())
            }
            Step::Snapshot => {
                *self.snapshot.lock().unwrap() = Some(self.stored());
                Ok(())
            }
            Step::SaveSnapshot(recovery) => {
                recovery::save(&self.state, &repository, &self.remembered(), recovery.clone())
                    .map_err(|error| error.message)
            }
            Step::AnnounceSnapshot => {
                let _gate = self.state.accounts.mutation_gate();
                crate::accounts::api_errors::announce_episode(
                    &self.state,
                    &repository,
                    &self.remembered(),
                )
                .map_err(|error| error.message)
            }
            Step::SettingsOffSnapshot => {
                let _gate = self.state.accounts.mutation_gate();
                let session = self.remembered();
                let mut runtime = session["runtimeSettings"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                runtime.insert(
                    "accountPolicyOverride".into(),
                    json!({ "enabled": false, "atLimit": "wait", "priority": "soonestReset", "retryErrors": false }),
                );
                runtime.remove("accountRecoverySuppressed");
                crate::accounts::api_errors::save_settings(
                    &self.state,
                    &repository,
                    &session,
                    runtime,
                )
                .map_err(|error| error.message)
            }
            Step::PublishQuestion => {
                let other = self.db();
                other
                    .execute(
                        "UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatAsyncQuestionIds', json(?3)) WHERE projectId = ?1 AND sessionId = ?2",
                        rusqlite::params![self.project, self.session, "[\"newer-question\"]"],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
            Step::AgeTyped(seconds) => {
                crate::accounts::api_errors::age_continuation(
                    &self.project,
                    &self.session,
                    std::time::Duration::from_secs(*seconds),
                );
                Ok(())
            }
            Step::Expect(..)
            | Step::ExpectCard(_)
            | Step::ExpectQuestions(_)
            | Step::ExpectTyped(_) => Ok(()),
        }
    }

    fn remembered(&self) -> Value {
        self.snapshot
            .lock()
            .unwrap()
            .clone()
            .expect("a Snapshot step first")
    }

    /// The question card and count checks; `None` when they hold.
    fn check(&self, step: &Step) -> Option<String> {
        let stored = self.stored();
        match step {
            Step::ExpectCard(card) => {
                let held = crate::session_approval::session_awaits_answer(&stored);
                (held != *card).then(|| format!("question card expected {card}, got {held}"))
            }
            Step::ExpectQuestions(count) => {
                let pending = crate::session_chat_async_questions::pending_question_count(&stored);
                (pending != *count)
                    .then(|| format!("pending questions expected {count}, got {pending}"))
            }
            Step::ExpectTyped(pending) => {
                let held =
                    crate::accounts::api_errors::continuation_pending(&self.project, &self.session);
                (held != *pending)
                    .then(|| format!("typed continuation expected {pending}, got {held}"))
            }
            _ => None,
        }
    }

    /// What `Expect` compares: failed rows, marker, and whether a recovery is shown.
    async fn observed(&self) -> (usize, Marker, bool) {
        let rows = crate::notification_feed::store::read_notification_feed_state(&self.db())
            .expect("feed")["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter(|row| row["kind"] == "failed" && row["sessionId"] == self.session.as_str())
            .count();
        let marker =
            match self.stored()["runtimeSettings"]["accountRecovery"].get(FAILURE_NOTIFIED_AT) {
                None => Marker::Absent,
                Some(Value::Null) => Marker::Owed,
                Some(_) => Marker::Stored,
            };
        // The account panel draws a session only for a provider with a saved account, which the
        // pass would also read, so one is saved for this answer alone.
        let db = self.db();
        let registry = store::read(&db).expect("registry");
        let mut with_account = registry.clone();
        with_account.accounts.push(
            serde_json::from_value(json!({
                "id": "episode-account", "provider": self.agent, "selector": "1",
                "identity": "episode-account", "name": "Episode account", "color": "neutral",
                "eligible": false, "sharedHistory": true, "showInTitlebar": true,
            }))
            .expect("account"),
        );
        store::write(&db, &with_account).expect("write registry");
        let answer = self
            .route(
                "/api/agentAccounts",
                json!({ "operation": "titlebar", "cachedOnly": true }),
            )
            .await;
        store::write(&db, &registry).expect("restore registry");
        let data = ghostex_gx_chat_core::menus::accounts_data::AccountsState::from_value(
            &answer["result"],
        );
        let panel = serde_json::to_value(
            ghostex_gx_chat_core::menus::native_accounts::native_account_panel(
                Some(&data),
                None,
                false,
                false,
                &ghostex_gx_chat_core::ChatContext::default(),
            ),
        )
        .expect("panel");
        let answered = !answer["result"]["session"]["recovery"].is_null();
        let drawn = !panel["session"]["recovery"].is_null();
        assert_eq!(
            answered, drawn,
            "the chat core draws what the answer says: {panel}"
        );
        (rows, marker, drawn)
    }
}

#[tokio::test]
async fn failed_rows_follow_the_error_episode() {
    let mut mismatches = Vec::new();
    for case in cases() {
        let run = Run::new(&case);
        let mut trace = Vec::new();
        for (index, step) in case.steps.iter().enumerate() {
            if let Err(error) = run.step(step).await {
                trace.push(format!("step {index} returned {error}"));
            }
            if let Some(problem) = run.check(step) {
                mismatches.push(format!("{}: step {index} {problem}", case.name));
            }
            if let Step::Expect(rows, marker, shown) = step {
                let observed = run.observed().await;
                if observed != (*rows, *marker, *shown) {
                    mismatches.push(format!(
                        "{}: step {index} expected (rows, marker, shown) {:?}, got {:?}; {}; recovery {}",
                        case.name,
                        (rows, marker, shown),
                        observed,
                        trace.join("; "),
                        run.stored()["runtimeSettings"]["accountRecovery"],
                    ));
                }
            }
        }
    }
    assert!(mismatches.is_empty(), "\n{}", mismatches.join("\n"));
}
