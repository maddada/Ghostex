//! The Unified sidebar's shared fixture: cases in the phone's terms, with what this builder makes
//! of them, kept in the phone app so its port of the builder is tested against the same answers.
//!
//! CDXC:Sidebar 2026-10-30 WHY:
//! The phone draws the Unified list from its own per-machine summaries in TypeScript
//! (apps/mobile/app/src/contract/unified/), because neither gxserver nor the phone's WebAssembly
//! chat core can run this builder in front of its session list. So that the two cannot drift,
//! this test runs every case of `unifiedFixture.json` through `build_unified_sidebar` and fails
//! when the file's expected answers differ; `UPDATE_UNIFIED_FIXTURE=1 cargo test -p
//! ghostex-gx-core unified_fixture` rewrites them. The phone's `unifiedFixture.test.ts` runs the
//! same cases through its port. A checkout without the phone app skips the comparison.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::keys::{MachineId, SessionKey};
use crate::sidebar_view::view::SpaceProject;
use crate::sidebar_view::{
    GroupCore, GroupSummary, GroupView, ProjectContextView, SessionRow, SessionTiming, SessionView,
    SidebarView, Space, SpacesState, WorktreeView,
};

use super::*;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/app/src/contract/unified/unifiedFixture.json")
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

fn flag(value: &Value, key: &str) -> bool {
    value[key].as_bool() == Some(true)
}

fn millis(value: &Value, key: &str) -> Option<i64> {
    value[key].as_i64()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn machine_of(id: &str) -> MachineId {
    if id == "local" {
        MachineId::Local
    } else {
        MachineId::Remote(id.to_string())
    }
}

/// A session of the fixture as a gx-core row; a remote machine's rows carry its routed id.
fn session_row(machine: &MachineId, session: &Value) -> SessionRow {
    let project = text(session, "projectId");
    let id = text(session, "sessionId");
    SessionRow {
        sidebar_session_id: match machine.remote_id() {
            None => id.to_string(),
            Some(remote) => format!("{remote}:{project}:{id}"),
        },
        key: Some(SessionKey {
            machine: machine.clone(),
            project_id: project.to_string(),
            session_id: id.to_string(),
        }),
        activity: session["activity"].as_str().unwrap_or("idle").to_string(),
        awaiting_approval: flag(session, "awaitingApproval"),
        pending_question_count: session["pendingQuestionCount"].as_u64().unwrap_or(0),
        model_selection_failed: flag(session, "modelSelectionFailed"),
        program_status_failed: flag(session, "programStatusFailed"),
        has_background_work: flag(session, "hasBackgroundWork"),
        is_pinned: flag(session, "isPinned"),
        is_parked: flag(session, "isParked"),
        is_draft: flag(session, "isDraft"),
        has_composer_draft: flag(session, "hasComposerDraft"),
        is_browser: flag(session, "isBrowser"),
        timing: SessionTiming {
            created_ms: millis(session, "createdMs"),
            last_interaction_ms: millis(session, "lastInteractionMs"),
            working_started_ms: millis(session, "workingStartedMs"),
            snoozed_until_ms: millis(session, "snoozedUntilMs"),
            ..SessionTiming::default()
        },
        ..SessionRow::default()
    }
}

fn project_group(machine: &MachineId, project: &Value, sessions: &[&Value]) -> GroupView {
    let project_id = text(project, "projectId");
    let worktree = project["worktreeParentPath"]
        .as_str()
        .map(|parent| WorktreeView {
            parent_project_path: parent.to_string(),
            ..WorktreeView::default()
        });
    GroupView {
        core: Arc::new(GroupCore {
            group_id: format!("project:{project_id}"),
            storage_id: project_id.to_string(),
            title: text(project, "title").to_string(),
            title_tooltip: None,
            is_active: false,
            project_context: Some(ProjectContextView {
                project_id: project_id.to_string(),
                path: text(project, "path").to_string(),
                git_remote_origin_url: project["gitRemoteOriginUrl"].as_str().map(str::to_string),
                worktree,
                ..ProjectContextView::default()
            }),
            summary: GroupSummary::default(),
            collapsed: false,
            hover_actions_expanded: false,
            sections: Vec::new(),
            sessions: sessions
                .iter()
                .map(|session| SessionView {
                    row: Arc::new(session_row(machine, session)),
                    is_focused: false,
                    is_visible: false,
                    is_multi_selected: false,
                    nesting: Default::default(),
                })
                .collect(),
            remote_machine: None,
            is_stale: false,
        }),
        collection_color: None,
        collection_id: None,
        space_project: Some(SpaceProject {
            project_id: project_id.to_string(),
            parent_project_id: project["parentProjectId"].as_str().map(str::to_string),
            collection_id: project["collectionId"].as_str().map(str::to_string),
        }),
        holds_active_session_group: false,
    }
}

fn spaces_of(value: &Value) -> Option<SpacesState> {
    let spaces = value.as_array()?;
    let mut state = SpacesState::default();
    for space in spaces {
        let id = text(space, "spaceId").to_string();
        state.order.push(id.clone());
        state.spaces.insert(
            id.clone(),
            Space {
                space_id: id,
                name: text(space, "name").to_string(),
                member_project_ids: strings(&space["memberProjectIds"]),
                member_collection_ids: strings(&space["memberCollectionIds"]),
                ..Space::default()
            },
        );
    }
    Some(state)
}

/// What the fixture records of one built row.
fn row_answer(row: &UnifiedRow) -> Value {
    json!({
        "status": row.status.map(UnifiedStatus::label),
        "repoKey": row.repo.key,
        "repoLabel": row.repo.label,
        "machineLabel": row.machine_label,
        "spaceKey": row.space_key,
        "cameBackMs": row.came_back_ms,
    })
}

fn row_id(row: &UnifiedRow) -> String {
    let machine = row.machine.remote_id().unwrap_or("local");
    let session = row
        .session
        .row
        .key
        .as_ref()
        .map(|key| key.session_id.as_str())
        .unwrap_or_default();
    format!("{machine}|{session}")
}

fn options_answer(options: &[UnifiedFilterOption]) -> Value {
    Value::Array(
        options
            .iter()
            .map(
                |option| json!({"key": option.key, "label": option.label, "detail": option.detail}),
            )
            .collect(),
    )
}

/// Runs one case's input through the builder.
fn expected_of(case: &Value) -> Value {
    let machines_in = case["machines"].as_array().cloned().unwrap_or_default();
    let mut views: Vec<(MachineId, String, SidebarView, Option<SpacesState>)> = Vec::new();
    for machine in &machines_in {
        let id = machine_of(text(machine, "machine"));
        let sessions = machine["sessions"].as_array().cloned().unwrap_or_default();
        let groups = machine["projects"]
            .as_array()
            .map(|projects| {
                projects
                    .iter()
                    .map(|project| {
                        let mine: Vec<&Value> = sessions
                            .iter()
                            .filter(|session| {
                                text(session, "projectId") == text(project, "projectId")
                            })
                            .collect();
                        project_group(&id, project, &mine)
                    })
                    .collect()
            })
            .unwrap_or_default();
        views.push((
            id,
            text(machine, "label").to_string(),
            SidebarView {
                groups,
                ..SidebarView::default()
            },
            spaces_of(&machine["spaces"]),
        ));
    }
    let inputs: Vec<UnifiedMachineInput<'_>> = views
        .iter()
        .map(|(machine, label, view, spaces)| UnifiedMachineInput {
            machine: machine.clone(),
            label: label.clone(),
            view,
            spaces: spaces.clone(),
        })
        .collect();
    let settings = UnifiedSettings {
        working_section: case["settings"]["workingSection"].as_bool() != Some(false),
        enable_session_parking: flag(&case["settings"], "parking"),
    };
    let set =
        |key: &str| -> BTreeSet<String> { strings(&case["filter"][key]).into_iter().collect() };
    let filter = UnifiedFilter {
        repos: set("repos"),
        machines: set("machines"),
        spaces: set("spaces"),
    };
    let now = case["nowMs"].as_u64().unwrap_or(0);
    let view = build_unified_sidebar(&inputs, settings, &filter, now);
    let mut rows = BTreeMap::new();
    for row in view
        .drafts
        .iter()
        .chain(&view.pinned)
        .chain(&view.active)
        .chain(&view.working)
        .chain(&view.snoozed)
        .chain(&view.parked)
    {
        rows.insert(row_id(row), row_answer(row));
    }
    let ids =
        |rows: &[UnifiedRow]| Value::Array(rows.iter().map(|row| json!(row_id(row))).collect());
    json!({
        "drafts": ids(&view.drafts),
        "pinned": ids(&view.pinned),
        "active": ids(&view.active),
        "working": ids(&view.working),
        "snoozed": ids(&view.snoozed),
        "parked": ids(&view.parked),
        "total": view.total,
        "rows": rows,
        "options": {
            "repos": options_answer(&view.options.repos),
            "machines": options_answer(&view.options.machines),
            "spaces": options_answer(&view.options.spaces),
        },
    })
}

fn session(id: &str, project: &str, extra: Value) -> Value {
    let mut base = json!({"sessionId": id, "projectId": project, "activity": "idle"});
    if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
        base.extend(extra.clone());
    }
    base
}

/// The cases, in the phone's terms. The expected answers are the builder's.
fn cases() -> Vec<Value> {
    let now = 10_000_000_i64;
    vec![
        json!({
            "name": "one repository on two machines is one project, named as this computer spells it",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "machines": [
                {
                    "machine": "local", "label": "Local",
                    "projects": [
                        {"projectId": "p1", "title": "Ghostex app", "path": "/Users/me/dev/Ghostex", "gitRemoteOriginUrl": "git@github.com:Owner/Ghostex.git"},
                        {"projectId": "p2", "title": "Ghostex feature", "path": "/Users/me/dev/Ghostex-feature", "worktreeParentPath": "/Users/me/dev/Ghostex"},
                        {"projectId": "p3", "title": "Notes", "path": "/Users/me/notes/"},
                    ],
                    "sessions": [
                        session("a", "p1", json!({"createdMs": 100, "lastInteractionMs": 5_000})),
                        session("b", "p2", json!({"createdMs": 200, "lastInteractionMs": 4_000})),
                        session("c", "p3", json!({"createdMs": 300, "lastInteractionMs": 6_000})),
                    ],
                },
                {
                    "machine": "hp", "label": "hp-laptop",
                    "projects": [
                        {"projectId": "p9", "title": "gx", "path": "/home/me/src/gx", "gitRemoteOriginUrl": "https://user@GitHub.com/owner/ghostex.git/"},
                        {"projectId": "p8", "title": "Other ghostex", "path": "/home/me/fork", "gitRemoteOriginUrl": "ssh://git@gitlab.com:22/fork/Ghostex.git"},
                        {"projectId": "p7", "title": "notes", "path": "C:\\code\\Notes"},
                    ],
                    "sessions": [
                        session("d", "p9", json!({"createdMs": 400, "lastInteractionMs": 7_000})),
                        session("e", "p8", json!({"createdMs": 500, "lastInteractionMs": 3_000})),
                        session("f", "p7", json!({"createdMs": 600, "lastInteractionMs": 2_000})),
                        session("g", "p9", json!({"createdMs": 700, "isBrowser": true})),
                    ],
                },
            ],
        }),
        json!({
            "name": "statuses, shelves and the order things came back in",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "machines": [{
                "machine": "local", "label": "Local",
                "projects": [{"projectId": "p", "title": "p", "path": "/p", "gitRemoteOriginUrl": "https://github.com/o/p"}],
                "sessions": [
                    session("approval", "p", json!({"activity": "working", "awaitingApproval": true, "createdMs": 10, "workingStartedMs": 9_000, "lastInteractionMs": 100})),
                    session("question", "p", json!({"activity": "working", "pendingQuestionCount": 2, "createdMs": 10, "workingStartedMs": 8_000})),
                    session("model", "p", json!({"modelSelectionFailed": true, "createdMs": 10, "lastInteractionMs": 7_500})),
                    session("program", "p", json!({"activity": "attention", "programStatusFailed": true, "createdMs": 10, "lastInteractionMs": 7_400})),
                    session("working", "p", json!({"activity": "working", "createdMs": 10, "workingStartedMs": 9_500, "lastInteractionMs": 9_900})),
                    session("monitoring", "p", json!({"hasBackgroundWork": true, "createdMs": 10, "lastInteractionMs": 9_800})),
                    session("done", "p", json!({"activity": "attention", "hasBackgroundWork": true, "createdMs": 10, "lastInteractionMs": 6_000})),
                    session("idle-old", "p", json!({"createdMs": 10, "lastInteractionMs": 1_000})),
                    session("created-late", "p", json!({"createdMs": 6_500, "lastInteractionMs": 50})),
                    session("tie-b", "p", json!({"createdMs": 10, "lastInteractionMs": 2_000})),
                    session("tie-a", "p", json!({"createdMs": 10, "lastInteractionMs": 2_000})),
                    session("pinned", "p", json!({"isPinned": true, "createdMs": 10, "lastInteractionMs": 500})),
                    session("pinned-draft", "p", json!({"isPinned": true, "isDraft": true, "createdMs": 10})),
                    session("draft-old", "p", json!({"isDraft": true, "createdMs": 20})),
                    session("draft-new", "p", json!({"isDraft": true, "hasComposerDraft": true, "createdMs": 30})),
                    session("snoozed-late", "p", json!({"snoozedUntilMs": now + 90_000, "createdMs": 10})),
                    session("snoozed-soon", "p", json!({"snoozedUntilMs": now + 60_000, "createdMs": 10})),
                    session("woke", "p", json!({"snoozedUntilMs": now - 1, "createdMs": 10, "lastInteractionMs": 1_500})),
                    session("parked-old", "p", json!({"isParked": true, "createdMs": 10, "lastInteractionMs": 1_000})),
                    session("parked-new", "p", json!({"isParked": true, "createdMs": 10, "lastInteractionMs": 3_000})),
                    session("working-2", "p", json!({"activity": "working", "createdMs": 10, "workingStartedMs": 9_600})),
                ],
            }],
        }),
        json!({
            "name": "the Working section off and parking off leave those rows in the list",
            "nowMs": now,
            "settings": {"workingSection": false, "parking": false},
            "filter": {},
            "machines": [{
                "machine": "local", "label": "Local",
                "projects": [{"projectId": "p", "title": "p", "path": "/p"}],
                "sessions": [
                    session("working", "p", json!({"activity": "working", "createdMs": 10, "workingStartedMs": 5_000})),
                    session("parked", "p", json!({"isParked": true, "createdMs": 10, "lastInteractionMs": 4_000})),
                    session("idle", "p", json!({"createdMs": 10, "lastInteractionMs": 3_000})),
                ],
            }],
        }),
        json!({
            "name": "Spaces, and the filter narrowing by repository, machine and Space together",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {"repos": ["github.com/o/web"], "machines": ["local", "hp"], "spaces": ["local|work", "hp|other"]},
            "machines": [
                {
                    "machine": "local", "label": "Local",
                    "spaces": [
                        {"spaceId": "work", "name": "Work", "memberProjectIds": ["web"], "memberCollectionIds": ["c1"]},
                        {"spaceId": "home", "name": "Home", "memberProjectIds": ["web", "api"]},
                    ],
                    "projects": [
                        {"projectId": "web", "title": "web", "path": "/web", "gitRemoteOriginUrl": "https://github.com/o/web"},
                        {"projectId": "web-wt", "title": "web wt", "path": "/web-wt", "gitRemoteOriginUrl": "https://github.com/o/web", "parentProjectId": "web"},
                        {"projectId": "api", "title": "api", "path": "/api", "gitRemoteOriginUrl": "https://github.com/o/api", "collectionId": "c1"},
                        {"projectId": "loose", "title": "loose", "path": "/loose", "gitRemoteOriginUrl": "https://github.com/o/web"},
                    ],
                    "sessions": [
                        session("w1", "web", json!({"createdMs": 10, "lastInteractionMs": 100})),
                        session("w2", "web-wt", json!({"createdMs": 10, "lastInteractionMs": 200})),
                        session("a1", "api", json!({"createdMs": 10, "lastInteractionMs": 300})),
                        session("l1", "loose", json!({"createdMs": 10, "lastInteractionMs": 400})),
                    ],
                },
                {
                    "machine": "hp", "label": "hp-laptop",
                    "spaces": [{"spaceId": "box", "name": "Box", "memberProjectIds": ["x"]}],
                    "projects": [{"projectId": "rweb", "title": "web", "path": "/r/web", "gitRemoteOriginUrl": "git@github.com:o/web.git"}],
                    "sessions": [session("r1", "rweb", json!({"createdMs": 10, "lastInteractionMs": 500}))],
                },
                {
                    "machine": "pi", "label": "pi",
                    "projects": [{"projectId": "pweb", "title": "web", "path": "/pi/web", "gitRemoteOriginUrl": "https://github.com/o/web"}],
                    "sessions": [session("p1", "pweb", json!({"createdMs": 10, "lastInteractionMs": 600}))],
                },
            ],
        }),
    ]
}

#[test]
fn unified_fixture_matches_the_builder() {
    let path = fixture_path();
    let Some(dir) = path
        .parent()
        .filter(|dir| dir.parent().is_some_and(|parent| parent.exists()))
    else {
        eprintln!("the phone app is not checked out; skipping the Unified fixture");
        return;
    };
    let cases: Vec<Value> = cases()
        .into_iter()
        .map(|mut case| {
            let expected = expected_of(&case);
            case["expected"] = expected;
            case
        })
        .collect();
    let fixture = serde_json::to_string_pretty(&json!({
        "about": "Generated by packages/gx-core/src/unified_sidebar/fixture_tests.rs from gx-core's Unified sidebar builder; do not edit by hand. Rewrite with UPDATE_UNIFIED_FIXTURE=1 cargo test -p ghostex-gx-core unified_fixture.",
        "cases": cases,
    }))
    .expect("the fixture serializes")
        + "\n";
    if std::env::var_os("UPDATE_UNIFIED_FIXTURE").is_some() {
        std::fs::create_dir_all(dir).expect("the fixture folder");
        std::fs::write(&path, &fixture).expect("the fixture writes");
        return;
    }
    let saved = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        saved == fixture,
        "{} is out of date with the Unified sidebar builder; rewrite it with UPDATE_UNIFIED_FIXTURE=1 cargo test -p ghostex-gx-core unified_fixture, then make the phone's port pass it",
        path.display()
    );
}
