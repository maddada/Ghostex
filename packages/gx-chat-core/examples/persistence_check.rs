//! The cold start: what a chat knows before its first read answers, and what survives a restart.
//!
//! The two round trips were `store.ts`'s and `session-chat-presentation-cache.ts`'s in the
//! TypeScript brain, not `native-host.ts`'s, so the port first missed both: the read at boot and
//! the write after a fold had zero call sites until 2026-09-22, which is why a Ghostex that had
//! just started drew an empty transcript and an empty bottom bar on a session it had been in five
//! seconds earlier.
//!
//! What it walks:
//!
//!  1. A boot asks for the retained record, a frame folds, and the debounce writes it back with the
//!     record key, the stamp and the requested window the TypeScript wrote.
//!  2. A SECOND core, given only those bytes, draws the same first document as the first core.
//!  3. A record older than the seven-day bound, and one whose snapshot is over the two-megabyte
//!     bound, are refused and deleted respectively.
//!  4. The presentation cache is restored at boot and written back when the identity or the status
//!     line's options move.
//!  5. The one document a host draws before anything has answered, which is where `skillsLoading`
//!     is measured.
//!
//! ```text
//! cargo run --example persistence_check
//! ```

use std::process::ExitCode;

use ghostex_gx_chat_core::session::persistence::{storage_key, StoredSnapshot};
use ghostex_gx_chat_core::{
    ChatContext, ChatCore, ChatFrame, ChatSnapshotFrame, Effect, Event, StartConfig,
};
use serde_json::{json, Value};

/// 2026-09-22T12:00:00.000Z, the instant every check here is fixed at.
const NOW_MS: f64 = 1_790_078_400_000.0;
const MACHINE: &str = "machine-1";
const PROJECT: &str = "project-1";
const SESSION: &str = "session-1";

fn main() -> ExitCode {
    let mut checks = Checks::default();
    let record = round_trip(&mut checks);
    restart(&mut checks, record);
    bounds(&mut checks);
    presentation(&mut checks);
    first_paint(&mut checks);

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

/// A boot, a fold, and the record the debounce writes.
fn round_trip(checks: &mut Checks) -> String {
    let mut core = Core::boot();
    checks.eq(
        "the boot asks for the retained record",
        json!(core
            .last_effects
            .iter()
            .any(|effect| matches!(effect, Effect::ReadRetainedSnapshot))),
        json!(true),
    );
    core.drive(Event::RetainedSnapshotLoaded { value: None });
    core.drive(Event::ComposerBootRead(Box::default()));
    core.frame(transcript(&["one", "two"]));

    // `schedulePersistence` debounces by a second, so the fold's own turn writes nothing.
    checks.eq(
        "the fold does not write on its own turn",
        json!(core.written().is_some()),
        json!(false),
    );
    core.advance(1_000.0);
    core.drive(Event::Tick);
    let written = core.written().expect("the debounce wrote the record");
    let value = written.expect("a transcript this size is inside the record bound");
    let record: StoredSnapshot = serde_json::from_str(&value).expect("the record is JSON");
    checks.eq(
        "the record carries the host's own key",
        json!(record.key),
        json!(storage_key(MACHINE, PROJECT, SESSION)),
    );
    checks.eq(
        "the stamp is the fold's clock, as an integer",
        json!(record.saved_at),
        json!(NOW_MS as i64),
    );
    checks.eq(
        "the requested window rides along, so the restored tail is not re-read smaller",
        json!(record.requested_window.is_some()),
        json!(true),
    );
    checks.eq(
        "the record holds the folded transcript",
        json!(record.snapshot.result.messages.len()),
        json!(2),
    );
    // `if (this.persistTimer) return`: one write per debounce, not one per fold.
    core.frame(transcript(&["one", "two", "three"]));
    checks.eq(
        "a second fold inside the debounce does not write twice",
        json!(core.written().is_some()),
        json!(false),
    );
    value
}

/// A cold core, given only the bytes, draws what the warm one drew.
fn restart(checks: &mut Checks, record: String) {
    let mut warm = Core::boot();
    warm.drive(Event::RetainedSnapshotLoaded { value: None });
    warm.drive(Event::ComposerBootRead(Box::default()));
    warm.frame(transcript(&["one", "two"]));
    let expected = warm.document.clone();
    checks.eq(
        "the warm core really did draw the two rows this compares against",
        json!(expected["itemsSplice"]["items"].as_array().map(Vec::len)),
        json!(2),
    );

    let mut cold = Core::boot();
    cold.drive(Event::RetainedSnapshotLoaded {
        value: Some(record.clone()),
    });
    cold.drive(Event::ComposerBootRead(Box::default()));
    checks.eq(
        "the first document after a restart is the one the fold had drawn",
        cold.document["itemsSplice"]["items"].clone(),
        expected["itemsSplice"]["items"].clone(),
    );
    checks.eq(
        "and it is ready rather than loading",
        cold.document["snapshot"]["status"].clone(),
        expected["snapshot"]["status"].clone(),
    );
    checks.eq(
        "the restored window is at least the initial one",
        json!(cold.core.state().messages.limit >= 300),
        json!(true),
    );

    // `if (!this.disposed && !this.snapshot && stored)`: a live fold that beat the read wins.
    let mut raced = Core::boot();
    raced.drive(Event::ComposerBootRead(Box::default()));
    raced.frame(transcript(&["live"]));
    raced.drive(Event::RetainedSnapshotLoaded {
        value: Some(record),
    });
    checks.eq(
        "a stored tail never rolls back a fold that beat it",
        json!(raced.core.state().messages.list.len()),
        json!(1),
    );
}

/// The age bound on the read and the size bound on the write.
fn bounds(checks: &mut Checks) {
    let stale = serde_json::to_string(&StoredSnapshot {
        key: storage_key(MACHINE, PROJECT, SESSION),
        // Eight days, one past the seven-day bound.
        saved_at: (NOW_MS as i64) - 8 * 24 * 60 * 60 * 1_000,
        requested_window: Some(300),
        snapshot: serde_json::from_value(transcript(&["old"])).expect("the fixture folds"),
    })
    .expect("the record serializes");
    let mut core = Core::boot();
    core.drive(Event::RetainedSnapshotLoaded { value: Some(stale) });
    core.drive(Event::ComposerBootRead(Box::default()));
    checks.eq(
        "a record past the seven-day bound is refused",
        json!(core.core.state().messages.list.len()),
        json!(0),
    );

    // `JSON.stringify(snapshot).length * 2 > MAX_RECORD_BYTES` DELETES the record rather than
    // slicing it, because a sliced tail leaves an invalid pagination cursor behind.
    let mut big = Core::boot();
    big.drive(Event::RetainedSnapshotLoaded { value: None });
    big.drive(Event::ComposerBootRead(Box::default()));
    big.frame(transcript(&["x".repeat(1_100_000).as_str()]));
    big.advance(1_000.0);
    big.drive(Event::Tick);
    checks.eq(
        "an oversized snapshot deletes the record instead of writing it",
        json!(big.written()),
        json!(Some(Option::<String>::None)),
    );
}

/// The cache a return to a chat restores its bottom bar from.
fn presentation(checks: &mut Checks) {
    let cached = json!({
        "agent": "claude",
        "agentSessionId": "agent-session-1",
        "sessionAgentId": "launch-1",
        "selectedOptions": { "model": "sonnet" },
        "workingDirectory": "/sample/project",
        "sessionTitle": "Surfaces",
    });
    let mut core = Core::boot_with(Some(cached.clone()));
    core.drive(Event::RetainedSnapshotLoaded { value: None });
    core.drive(Event::ComposerBootRead(Box::default()));
    checks.eq(
        "the agent comes back before anything is read",
        core.document["snapshot"]["agent"].clone(),
        json!("claude"),
    );
    checks.eq(
        "and so does the status line's own options",
        core.document["snapshot"]["selectedOptions"]["model"].clone(),
        json!("sonnet"),
    );
    checks.eq(
        "the working directory reaches family b, which shortens a diff card's path",
        json!(core.core.state().transcript_view.working_directory),
        json!("/sample/project"),
    );
    checks.eq(
        "an unchanged cache is not written back",
        json!(core.presentation().is_some()),
        json!(false),
    );

    // A frame that names a different agent session is an identity change, which is what the
    // cache exists to carry into the next visit.
    let mut frame = transcript(&["one"]);
    frame["agentSessionId"] = json!("agent-session-2");
    core.frame(frame);
    let written = core
        .presentation()
        .expect("the identity change is written back");
    checks.eq(
        "the new identity is written back whole, not as a patch",
        written["agentSessionId"].clone(),
        json!("agent-session-2"),
    );
    checks.eq(
        "the sidebar's own keys are carried through untouched",
        written["sessionTitle"].clone(),
        json!("Surfaces"),
    );
    checks.eq(
        "an identity change drops the options the previous account detected",
        json!(written.get("selectedOptions").is_none()),
        json!(true),
    );
}

/// The document a host draws before any read has answered.
fn first_paint(checks: &mut Checks) {
    let core = ChatCore::new();
    let document = serde_json::to_value(core.document()).expect("the document is JSON");
    // `skillsLoading: current?.loading ?? Boolean(transport.readSkills)` (`skills.ts:62`). Every
    // transport this crate serves has a `readSkills`, so a `$` typed in the window between mount
    // and the first answer draws "Loading skills…" rather than "No skills".
    checks.eq(
        "a chat that has read nothing is still loading its skills",
        document["skillsLoading"].clone(),
        json!(true),
    );
    // Its counterpart is the opposite: `computeSessionChatFiles` reads nothing on mount and hands
    // back a callback the `@` list calls the first time it opens.
    checks.eq(
        "and is not loading its files, which nothing has asked for yet",
        document["filesLoading"].clone(),
        json!(false),
    );
}

// ---------------------------------------------------------------------------

/// One `sessionChatSnapshot` frame carrying these message texts.
fn transcript(texts: &[&str]) -> Value {
    let messages: Vec<Value> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| {
            json!({
                "id": format!("m{index}"),
                "role": if index % 2 == 0 { "user" } else { "assistant" },
                "blocks": [{ "type": "text", "text": text }],
                "timestamp": (NOW_MS as i64) - 60_000,
                "source": "transcript",
                "byteOffset": index as u64 * 100,
            })
        })
        .collect();
    json!({
        "type": "sessionChatSnapshot",
        "projectId": PROJECT,
        "sessionId": SESSION,
        "serverId": "server-1",
        "protocolVersion": 1,
        "epoch": 1,
        "seq": texts.len() as i64,
        "status": "ready",
        "messages": messages,
        "hasMore": false,
        "beforeOffset": 0,
    })
}

struct Core {
    core: ChatCore,
    context: ChatContext,
    last_effects: Vec<Effect>,
    document: Value,
}

impl Core {
    fn boot() -> Self {
        Self::boot_with(None)
    }

    fn boot_with(initial_presentation: Option<Value>) -> Self {
        let mut core = Self {
            core: ChatCore::new(),
            context: ChatContext::at(NOW_MS),
            last_effects: Vec::new(),
            document: Value::Null,
        };
        // Deserialized rather than built field by field, because that is how a host delivers it
        // and because a struct literal here would be the only writer the state audit can see for
        // fields only `serde` ever fills.
        let config: StartConfig = serde_json::from_value(json!({
            "clientId": "client-1",
            "projectId": PROJECT,
            "sessionId": SESSION,
            "initialPresentation": initial_presentation,
            "retainedKey": storage_key(MACHINE, PROJECT, SESSION),
        }))
        .expect("the start config parses");
        core.drive(Event::Start(Box::new(config)));
        core
    }

    fn frame(&mut self, value: Value) {
        let frame: ChatSnapshotFrame = serde_json::from_value(value).expect("the frame parses");
        self.drive(Event::Frame(Box::new(ChatFrame::Snapshot(Box::new(frame)))));
    }

    fn advance(&mut self, by_ms: f64) {
        self.context = ChatContext::at(self.context.now_ms + by_ms);
    }

    fn drive(&mut self, event: Event) {
        self.last_effects = self.core.handle(event, self.context.clone());
        let frame = self.core.frame(0);
        self.document = serde_json::to_value(&frame).expect("the frame is JSON");
    }

    /// The retained write the last turn asked for: the outer option is "was there a write", the
    /// inner one is the record or its deletion.
    fn written(&self) -> Option<Option<String>> {
        self.last_effects.iter().find_map(|effect| match effect {
            Effect::WriteRetainedSnapshot { value } => Some(value.clone()),
            _ => None,
        })
    }

    /// The presentation cache the last turn wrote back.
    fn presentation(&self) -> Option<Value> {
        self.last_effects.iter().find_map(|effect| match effect {
            Effect::UpdatePresentation { state } => Some((**state).clone()),
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
