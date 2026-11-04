//! The Sessions tab's Closed list as the window sees it: the page gxserver answers, the project
//! filter, and restoring rows one after another without closing Quick Access.

use serde_json::{json, Value};

use super::controller::{QuickAccessContext, QuickAccessController, QuickAccessEffect};
use super::data::{
    QuickAccessCollection, QuickAccessData, QuickAccessHiddenItems, QuickAccessRecoveredDraft,
    QuickAccessStorage,
};
use super::text::FixedClock;
use super::wire::{QuickAccessSnapshot, QuickAccessUpdate};

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

/// A closed, restorable Ghostex session as `/api/listPreviousSessions` pages it.
fn closed(history_id: &str, project_id: &str, title: &str, minutes_ago: i64) -> Value {
    let closed_at = chrono_iso(NOW_MS - minutes_ago * 60_000);
    json!({
        "historyId": history_id,
        "sessionId": history_id,
        "projectId": project_id,
        "projectName": if project_id == "P3lv0" { "Ghostex" } else { "Other" },
        "displayTitle": title,
        "closedAt": closed_at,
        "lastInteractionAt": closed_at,
        "isRestorable": true,
        "agentIcon": "claude",
    })
}

fn chrono_iso(ms: i64) -> String {
    let seconds = ms.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
        of_day / 3_600,
        of_day % 3_600 / 60,
        of_day % 60
    )
}

struct Palette {
    controller: QuickAccessController,
    data: QuickAccessData,
    storage: NoStorage,
    clock: FixedClock,
}

impl Palette {
    /// Quick Access opened on Sessions with the Closed scope and the Ghostex project, answered
    /// with `page`.
    fn closed_in_ghostex(page: Vec<Value>) -> (Self, Value) {
        let mut palette = Self {
            controller: QuickAccessController::new(NOW_MS),
            data: QuickAccessData::default(),
            storage: NoStorage,
            clock: FixedClock {
                now_ms: NOW_MS,
                utc_offset_ms: 0,
            },
        };
        let effects = palette.command(json!({
            "type": "open",
            "tab": "recentSessions",
            "scope": "closed",
            "projectId": "P3lv0",
        }));
        let token = effects
            .iter()
            .find_map(|effect| match effect {
                QuickAccessEffect::ScheduleSessionsRequest { token, .. } => Some(*token),
                _ => None,
            })
            .expect("opening Sessions asks for a page");
        let request = palette
            .with_context(|controller, context| controller.sessions_timer_fired(token, context))
            .into_iter()
            .find_map(|effect| match effect {
                QuickAccessEffect::Post(message)
                    if message["type"] == "requestPreviousSessions" =>
                {
                    Some(message)
                }
                _ => None,
            })
            .expect("the page request");
        let answer = json!({
            "type": "previousSessionsResult",
            "requestId": request["requestId"],
            "previousSessions": page,
            "projects": [{ "projectId": "P3lv0", "name": "Ghostex" }],
        });
        palette.with_context(|controller, context| controller.receive(&answer, context));
        (palette, request)
    }

    fn with_context<T>(
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

    fn command(&mut self, command: Value) -> Vec<QuickAccessEffect> {
        self.with_context(|controller, context| controller.command(&command, context))
    }

    fn snapshot(&mut self) -> QuickAccessSnapshot {
        self.with_context(|controller, context| controller.flush_publish(context))
            .into_iter()
            .find_map(|effect| match effect {
                QuickAccessEffect::Update(QuickAccessUpdate::Snapshot(snapshot)) => Some(*snapshot),
                _ => None,
            })
            .expect("a snapshot")
    }

    fn row_keys(&mut self) -> Vec<String> {
        self.snapshot()
            .groups
            .iter()
            .flat_map(|group| &group.rows)
            .map(|row| row.key().to_string())
            .collect()
    }
}

fn restores(effects: &[QuickAccessEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            QuickAccessEffect::Post(message) if message["type"] == "restorePreviousSession" => {
                message["historyId"].as_str().map(str::to_string)
            }
            _ => None,
        })
        .collect()
}

fn closes(effects: &[QuickAccessEffect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, QuickAccessEffect::Update(QuickAccessUpdate::Close)))
}

#[test]
fn the_closed_scope_lists_the_projects_closed_sessions_newest_first() {
    let (mut palette, request) = Palette::closed_in_ghostex(vec![
        closed("H1", "P3lv0", "Fix the sidebar", 5),
        closed("H2", "P3lv0", "Coordinator thread", 30),
        closed("H3", "P9", "Elsewhere", 1),
        closed("H4", "P3lv0", "Resolved last week", 60 * 24 * 9),
    ]);

    assert_eq!(request["projectId"], "P3lv0");
    assert_eq!(
        palette.row_keys(),
        ["closed:H1", "closed:H2", "closed:H4"],
        "only Ghostex's closed sessions, newest closed first, older than a day included"
    );
}

#[test]
fn restoring_closed_rows_keeps_quick_access_open_and_each_row_restores_once() {
    let (mut palette, _) = Palette::closed_in_ghostex(vec![
        closed("H1", "P3lv0", "Fix the sidebar", 5),
        closed("H2", "P3lv0", "Coordinator thread", 30),
        closed("H3", "P3lv0", "Review the palette", 90),
    ]);

    // A click (and Enter, which sends the same command for the selected row).
    let effects = palette.command(json!({ "type": "activate", "key": "closed:H1" }));
    assert_eq!(restores(&effects), ["H1"]);
    assert!(!closes(&effects), "restoring must not close Quick Access");
    assert!(palette.controller.is_open());
    let snapshot = palette.snapshot();
    assert_eq!(
        palette.row_keys(),
        ["closed:H2", "closed:H3"],
        "the restored row leaves the list"
    );
    assert_eq!(
        snapshot.selected_key, "closed:H2",
        "Enter moves on to the next row"
    );

    // A second click on the restored row (a stale frame) restores nothing.
    let again = palette.command(json!({ "type": "activate", "key": "closed:H1" }));
    assert!(restores(&again).is_empty());

    // Enter on the highlighted row restores the next one; the last row hands the highlight back.
    let selected = palette.snapshot().selected_key;
    palette.command(json!({ "type": "activate", "key": selected }));
    let effects = palette.command(json!({ "type": "activate", "key": "closed:H3" }));
    assert_eq!(restores(&effects), ["H3"]);
    assert!(palette.row_keys().is_empty());
    assert!(palette.controller.is_open());

    // Escape still closes it.
    assert!(closes(&palette.command(json!({ "type": "close" }))));
}

#[test]
fn a_restored_row_takes_its_hidden_older_copies_with_it() {
    // One project and title is one row: the newest closed stands for the older copies.
    let (mut palette, _) = Palette::closed_in_ghostex(vec![
        closed("H1", "P3lv0", "Coordinator thread", 5),
        closed("H0", "P3lv0", "Coordinator thread", 600),
        closed("H2", "P3lv0", "Fix the sidebar", 30),
    ]);
    assert_eq!(palette.row_keys(), ["closed:H1", "closed:H2"]);

    palette.command(json!({ "type": "activate", "key": "closed:H1" }));

    assert_eq!(
        palette.row_keys(),
        ["closed:H2"],
        "the older copy does not surface as if the click had failed"
    );
}

#[test]
fn a_restored_row_stays_gone_when_the_next_page_still_lists_it() {
    let (mut palette, _) = Palette::closed_in_ghostex(vec![
        closed("H1", "P3lv0", "Fix the sidebar", 5),
        closed("H2", "P3lv0", "Coordinator thread", 30),
    ]);
    palette.command(json!({ "type": "activate", "key": "closed:H1" }));

    // A filter change pages again before gxserver has recorded the restore.
    let effects = palette.command(json!({ "type": "query", "query": "" }));
    let token = effects
        .iter()
        .find_map(|effect| match effect {
            QuickAccessEffect::ScheduleSessionsRequest { token, .. } => Some(*token),
            _ => None,
        })
        .expect("a new page request");
    let request = palette
        .with_context(|controller, context| controller.sessions_timer_fired(token, context))
        .into_iter()
        .find_map(|effect| match effect {
            QuickAccessEffect::Post(message) => Some(message),
            _ => None,
        })
        .expect("the page request");
    let answer = json!({
        "type": "previousSessionsResult",
        "requestId": request["requestId"],
        "previousSessions": [
            closed("H1", "P3lv0", "Fix the sidebar", 5),
            closed("H2", "P3lv0", "Coordinator thread", 30),
        ],
    });
    palette.with_context(|controller, context| controller.receive(&answer, context));

    assert_eq!(palette.row_keys(), ["closed:H2"]);
}
