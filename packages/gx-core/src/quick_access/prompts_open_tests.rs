//! The Saved Prompts tab opened for one session: its first frames are already narrowed to that
//! session, never the whole library (CDXC:SavedPrompts 2026-11-02 DECISION in `prompts.rs`).

use serde_json::{json, Value};

use super::controller::{QuickAccessContext, QuickAccessController, QuickAccessEffect};
use super::data::{
    QuickAccessCollection, QuickAccessData, QuickAccessHiddenItems, QuickAccessRecoveredDraft,
    QuickAccessStorage,
};
use super::text::FixedClock;
use super::wire::{QuickAccessSnapshot, QuickAccessToolbar, QuickAccessUpdate};

const NOW_MS: i64 = 1_793_300_000_000;

struct NoStorage;

impl QuickAccessStorage for NoStorage {
    fn hidden_items(&mut self) -> QuickAccessHiddenItems {
        QuickAccessHiddenItems::default()
    }
    fn project_collections(&mut self) -> Vec<QuickAccessCollection> {
        Vec::new()
    }
    fn recovered_drafts(&mut self) -> Vec<QuickAccessRecoveredDraft> {
        Vec::new()
    }
    fn sent_messages(&mut self) -> Vec<Value> {
        Vec::new()
    }
    fn delete_sent_message(&mut self, _prompt_id: &str) {}
    fn dismiss_draft_recovery(&mut self, _recovery_id: &str) {}
    fn delete_stored_draft(&mut self, _session_key: &str) {}
    fn import_draft_recovery(&mut self, _recovery_drafts: &Value) {}
    fn reconcile_drafts_from_server(&mut self, _drafts: &Value) {}
    fn record_delivered_drafts(&mut self, _delivered_drafts: &Value) {}
}

fn prompt(id: &str, session: &str) -> Value {
    json!({
        "promptId": id,
        "content": format!("prompt {id}"),
        "projectId": "P1",
        "sessionId": session,
        "createdAt": "2026-11-02T10:00:00.000Z",
        "updatedAt": "2026-11-02T10:00:00.000Z",
        "tagIds": ["stashed"],
    })
}

struct Window {
    controller: QuickAccessController,
    data: QuickAccessData,
    storage: NoStorage,
    clock: FixedClock,
}

impl Window {
    fn new() -> Self {
        Self {
            controller: QuickAccessController::new(NOW_MS),
            data: QuickAccessData::default(),
            storage: NoStorage,
            clock: FixedClock {
                now_ms: NOW_MS,
                utc_offset_ms: 0,
            },
        }
    }

    fn context<T>(
        &mut self,
        run: impl FnOnce(&mut QuickAccessController, &mut QuickAccessContext<'_>) -> T,
    ) -> T {
        let mut context = QuickAccessContext {
            data: &self.data,
            storage: &mut self.storage,
            clock: &self.clock,
        };
        run(&mut self.controller, &mut context)
    }

    /// Opens Saved Prompts for session G1 of project P1 (what the chat's Stashed pill sends) and
    /// answers the request for the library with `library`; the snapshot drawn before the answer.
    fn open_for_session(&mut self, library: Vec<Value>) -> QuickAccessSnapshot {
        let open = json!({
            "type": "open",
            "tab": "savedPrompts",
            "promptProjectId": "P1",
            "promptSessionId": "G1",
        });
        let effects = self.context(|controller, context| controller.command(&open, context));
        let request = effects
            .into_iter()
            .find_map(|effect| match effect {
                QuickAccessEffect::Post(message) if message["type"] == "requestStashedPrompts" => {
                    Some(message)
                }
                _ => None,
            })
            .expect("the library request");
        let first = self.snapshot();
        let answer = json!({
            "type": "stashedPromptsResult",
            "requestId": request["requestId"],
            "prompts": library,
            "tags": [],
        });
        self.context(|controller, context| controller.receive(&answer, context));
        first
    }

    fn snapshot(&mut self) -> QuickAccessSnapshot {
        self.context(|controller, context| controller.flush_publish(context))
            .into_iter()
            .find_map(|effect| match effect {
                QuickAccessEffect::Update(QuickAccessUpdate::Snapshot(snapshot)) => Some(*snapshot),
                _ => None,
            })
            .expect("a snapshot")
    }

    fn close(&mut self) {
        self.context(|controller, context| controller.command(&json!({"type": "closed"}), context));
    }
}

fn rows(snapshot: &QuickAccessSnapshot) -> usize {
    snapshot.groups.iter().map(|group| group.rows.len()).sum()
}

fn scope_label(snapshot: &QuickAccessSnapshot) -> String {
    match &snapshot.toolbar {
        QuickAccessToolbar::Prompts { projects, .. } => format!("{projects:?}"),
        _ => String::new(),
    }
}

fn library() -> Vec<Value> {
    vec![
        prompt("a", "G1"),
        prompt("b", "G2"),
        prompt("c", "G1"),
        prompt("d", "G3"),
    ]
}

#[test]
fn the_first_open_lists_nothing_until_the_session_filter_can_apply() {
    let mut window = Window::new();
    let first = window.open_for_session(library());
    assert!(first.loading, "the library has not been read yet");
    assert_eq!(rows(&first), 0, "no row before the filter");
    assert!(scope_label(&first).contains("This session"));

    let settled = window.snapshot();
    assert!(!settled.loading);
    assert_eq!(rows(&settled), 2, "only this session's two prompts");
    assert!(scope_label(&settled).contains("This session"));
}

#[test]
fn a_reopen_with_a_kept_library_is_filtered_on_its_first_frame() {
    let mut window = Window::new();
    window.open_for_session(library());
    window.snapshot();
    window.close();

    let first = window.open_for_session(library());
    assert_eq!(rows(&first), 2, "the kept library, already narrowed");
    assert!(!first.loading);
    assert!(scope_label(&first).contains("This session"));
}

#[test]
fn a_kept_library_older_than_the_stash_is_held_back_not_shown_whole() {
    let mut window = Window::new();
    // Last open: this session had nothing stashed yet, so the library kept has none of its rows.
    window.open_for_session(vec![prompt("b", "G2"), prompt("d", "G3")]);
    window.close();

    // Now the user has stashed one and opens the list from the pill.
    let first = window.open_for_session(vec![prompt("b", "G2"), prompt("a", "G1")]);
    assert_eq!(rows(&first), 0, "the stale whole library is not drawn");
    assert!(first.loading);
    let settled = window.snapshot();
    assert_eq!(rows(&settled), 1);
}

#[test]
fn a_session_with_nothing_saved_falls_back_to_every_prompt() {
    let mut window = Window::new();
    let first = window.open_for_session(vec![prompt("b", "G2"), prompt("d", "G3")]);
    assert_eq!(rows(&first), 0);
    let settled = window.snapshot();
    assert_eq!(rows(&settled), 2);
    assert!(scope_label(&settled).contains("All projects"));
}
