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
    GroupCore, GroupSummary, GroupView, ProjectContextView, SessionMenuFacts, SessionRow,
    SessionTiming, SessionView, SidebarView, Space, SpacesState, WorktreeView,
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
        display_title: text(session, "title").to_string(),
        menu_facts: SessionMenuFacts {
            branch: session["branch"].as_str().map(str::to_string),
            ..SessionMenuFacts::default()
        },
        activity: session["activity"].as_str().unwrap_or("idle").to_string(),
        awaiting_approval: flag(session, "awaitingApproval"),
        awaiting_answer: flag(session, "awaitingAnswer"),
        pending_question_count: session["pendingQuestionCount"].as_u64().unwrap_or(0),
        model_selection_failed: flag(session, "modelSelectionFailed"),
        program_status_failed: flag(session, "programStatusFailed"),
        has_background_work: flag(session, "hasBackgroundWork"),
        is_pinned: flag(session, "isPinned"),
        is_parked: flag(session, "isParked"),
        is_draft: flag(session, "isDraft"),
        has_composer_draft: flag(session, "hasComposerDraft"),
        is_browser: flag(session, "isBrowser"),
        is_coordinator: flag(session, "isCoordinator"),
        thread_state: session["coordinatorThreadState"]
            .as_str()
            .map(str::to_string),
        stuck: flag(session, "stuck"),
        // The phone knows a thread's coordinator on the thread's own computer only.
        coordinator_parent: session["coordinatorSessionId"]
            .as_str()
            .map(|parent| SessionKey {
                machine: machine.clone(),
                project_id: text(session, "coordinatorProjectId").to_string(),
                session_id: parent.to_string(),
            }),
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
                    is_focused: flag(session, "isFocused"),
                    is_visible: false,
                    is_multi_selected: false,
                    nesting: Default::default(),
                })
                .collect(),
            remote_machine: None,
            is_stale: false,
            machine_names: Vec::new(),
            action_target: None,
            folded_worktrees: Vec::new(),
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

/// What the fixture records of a row with threads under it.
fn crew_answer(crew: &UnifiedCrew) -> Value {
    let folded = crew.folded_status;
    let dots: Vec<&str> = [
        (folded.working, "working"),
        (folded.attention, "attention"),
        (folded.background_work, "backgroundWork"),
        (folded.question, "question"),
        (folded.approval, "approval"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    json!({
        "key": crew.key,
        "open": crew.open,
        "unfolded": crew.unfolded,
        "total": crew.total,
        "listed": crew.listed,
        "older": crew.older,
        "olderShown": crew.older_shown,
        "heldOpen": crew.held_open,
        "olderRow": crew.older_row,
        "badgeCount": crew.badge.count,
        "badgeTone": format!("{:?}", crew.badge.tone).to_lowercase(),
        "folded": dots,
        "containsActive": crew.contains_active,
    })
}

/// The copies of a coordinator's threads on other computers that its computer borrows, as the
/// store makes them (`sidebar_view/model.rs` `remote_thread_rows`): a coordinator names them in
/// `coordinatorRemoteThreads` by the thread computer's `serverId`, and the copy is that computer's
/// own row, linked to the coordinator, drawn in the coordinator's group.
fn borrow_remote_threads(
    machines_in: &[Value],
    views: &mut [(MachineId, String, SidebarView, Option<SpacesState>)],
) {
    let mut borrowed: Vec<(usize, String, SessionRow)> = Vec::new();
    for (index, machine) in machines_in.iter().enumerate() {
        let coordinator_machine = machine_of(text(machine, "machine"));
        for coordinator in machine["sessions"].as_array().into_iter().flatten() {
            for thread in coordinator["coordinatorRemoteThreads"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let Some(remote) = machines_in
                    .iter()
                    .find(|other| other["serverId"] == thread["serverId"])
                else {
                    continue;
                };
                let Some(own) = remote["sessions"].as_array().and_then(|sessions| {
                    sessions.iter().find(|session| {
                        session["sessionId"] == thread["sessionId"]
                            && session["projectId"] == thread["projectId"]
                    })
                }) else {
                    continue;
                };
                let mut row = session_row(&machine_of(text(remote, "machine")), own);
                row.coordinator_parent = Some(SessionKey {
                    machine: coordinator_machine.clone(),
                    project_id: text(coordinator, "projectId").to_string(),
                    session_id: text(coordinator, "sessionId").to_string(),
                });
                row.thread_state = thread["threadState"].as_str().map(str::to_string);
                borrowed.push((
                    index,
                    format!("project:{}", text(coordinator, "projectId")),
                    row,
                ));
            }
        }
    }
    for (index, group_id, row) in borrowed {
        let Some(group) = views[index]
            .2
            .groups
            .iter_mut()
            .find(|group| group.core.group_id == group_id)
        else {
            continue;
        };
        Arc::make_mut(&mut group.core).sessions.push(SessionView {
            row: Arc::new(row),
            is_focused: false,
            is_visible: false,
            is_multi_selected: false,
            nesting: Default::default(),
        });
    }
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
    borrow_remote_threads(&machines_in, &mut views);
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
        pinned_session_order: crate::sidebar_view::PinnedSessionOrder::Manual,
    };
    let set =
        |key: &str| -> BTreeSet<String> { strings(&case["filter"][key]).into_iter().collect() };
    let filter = UnifiedFilter {
        repos: set("repos"),
        machines: set("machines"),
        spaces: set("spaces"),
        query: text(&case["filter"], "query").to_string(),
    };
    let now = case["nowMs"].as_u64().unwrap_or(0);
    let folds = UnifiedFolds {
        unfolded: strings(&case["folds"]["unfolded"]).into_iter().collect(),
        older_shown: strings(&case["folds"]["olderShown"]).into_iter().collect(),
        reveal: case["folds"]["reveal"].as_str().map(str::to_string),
    };
    let view = build_unified_sidebar_folded(&inputs, settings, &filter, None, &folds, now);
    let mut crews = BTreeMap::new();
    let mut lines = BTreeMap::new();
    for card in view
        .drafts
        .iter()
        .chain(&view.pinned)
        .chain(&view.active)
        .chain(&view.working)
        .chain(&view.snoozed)
        .chain(&view.parked)
    {
        for row in card.rows_in_tree() {
            if let Some(crew) = &row.crew {
                crews.insert(row_id(row), crew_answer(crew));
            }
        }
        let drawn: Vec<String> = card
            .drawn_lines()
            .into_iter()
            .map(|line| match line {
                UnifiedTreeLine::Thread { row, depth } => format!("{}@{depth}", row_id(row)),
                UnifiedTreeLine::Older { coordinator, depth } => {
                    format!("older:{}@{depth}", row_id(coordinator))
                }
            })
            .collect();
        if !drawn.is_empty() {
            lines.insert(row_id(card), drawn);
        }
    }
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
    let order = strings(&case["order"]);
    let groups: Vec<Value> = group_unified_sidebar(&view, &order)
        .iter()
        .map(|group| {
            json!({
                "repoKey": group.repo_key,
                "label": group.label,
                "pinned": ids(&group.pinned),
                "active": ids(&group.active),
                "working": ids(&group.working),
                "snoozed": ids(&group.snoozed),
                "parked": ids(&group.parked),
            })
        })
        .collect();
    let moves: Vec<Value> = case["moves"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|step| {
            json!(move_unified_project(
                &order,
                &view.options.repos,
                text(step, "moved"),
                text(step, "target"),
                flag(step, "after"),
            ))
        })
        .collect();
    let choice_inputs: Vec<UnifiedChoiceMachineInput> = machines_in
        .iter()
        .map(|machine| UnifiedChoiceMachineInput {
            machine: machine_of(text(machine, "machine")),
            label: text(machine, "label").to_string(),
            connected: machine["connected"].as_bool() != Some(false),
            projects: machine["projects"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|project| ProjectChoiceInput {
                    project_id: text(project, "projectId").to_string(),
                    title: text(project, "title").to_string(),
                    path: text(project, "path").to_string(),
                    git_remote_origin_url: project["gitRemoteOriginUrl"]
                        .as_str()
                        .map(str::to_string),
                    worktree_parent_path: project["worktreeParentPath"]
                        .as_str()
                        .map(str::to_string),
                    has_sessions: machine["sessions"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or_default()
                        .iter()
                        .any(|session| {
                            session["projectId"] == project["projectId"]
                                && session["isBrowser"].as_bool() != Some(true)
                        }),
                    last_activity_ms: machine["sessions"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or_default()
                        .iter()
                        .filter(|session| {
                            session["projectId"] == project["projectId"]
                                && session["isBrowser"].as_bool() != Some(true)
                        })
                        .filter_map(|session| session["lastInteractionMs"].as_i64())
                        .max(),
                })
                .collect(),
        })
        .collect();
    let last_machines: BTreeMap<String, String> = case["lastMachines"]
        .as_object()
        .map(|map| {
            map.iter()
                .filter_map(|(repo, machine)| Some((repo.clone(), machine.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let current_machine = case["currentMachine"].as_str().unwrap_or("local");
    let recent_repos: Vec<String> = case["recentRepos"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|repo| repo.as_str().map(str::to_string))
        .collect();
    let all_choices = unified_project_choices(
        &choice_inputs,
        &last_machines,
        &recent_repos,
        current_machine,
    );
    let choices: Vec<Value> = filter_project_choices(&all_choices, text(case, "choiceQuery"))
        .iter()
        .map(|choice| {
            json!({
                "repoKey": choice.repo_key,
                "label": choice.label,
                "selected": choice.selected,
                "hasSessions": choice.has_sessions,
                "recent": choice.recent,
                "detail": choice.detail,
                "machines": choice.machines.iter().map(|chip| json!({
                    "machineId": chip.machine_id,
                    "label": chip.label,
                    "connected": chip.connected,
                    "projectId": chip.project_id,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let active_machine = case["activeMachine"].as_str().unwrap_or("local");
    let active_repo = case["activeProjectId"].as_str().and_then(|project_id| {
        let machine = machines_in
            .iter()
            .find(|machine| text(machine, "machine") == active_machine)?;
        let project = machine["projects"]
            .as_array()?
            .iter()
            .find(|project| text(project, "projectId") == project_id)?;
        Some(
            repo_identity(
                project["gitRemoteOriginUrl"].as_str(),
                text(project, "path"),
                project["worktreeParentPath"].as_str(),
                text(project, "title"),
            )
            .key,
        )
    });
    let active = case["activeProjectId"]
        .as_str()
        .zip(active_repo.as_deref())
        .map(|(project_id, repo_key)| ActiveProject {
            repo_key,
            machine_id: active_machine,
            project_id,
        });
    let target = unified_new_session_target(&filter, &all_choices, active).map(|target| {
        json!({
            "repoKey": target.repo_key,
            "machineId": target.machine_id,
            "projectId": target.project_id,
        })
    });
    let widened = case.get("createdIn").map(|created| {
        let widened = widen_filter_for_new_session(
            &filter,
            text(created, "repoKey"),
            text(created, "machineId"),
            created["spaceKey"].as_str(),
        );
        json!({
            "repos": widened.repos,
            "machines": widened.machines,
            "spaces": widened.spaces,
        })
    });
    json!({
        "groups": groups,
        "moves": moves,
        "choices": choices,
        "newSessionTarget": target,
        "widenedFilter": widened,
        "drafts": ids(&view.drafts),
        "pinned": ids(&view.pinned),
        "active": ids(&view.active),
        "working": ids(&view.working),
        "snoozed": ids(&view.snoozed),
        "parked": ids(&view.parked),
        "total": view.total,
        "rows": rows,
        "crews": crews,
        "lines": lines,
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

/// A coordinator with threads at two depths, one waiting on you, one working, one older than two
/// hours, one on another computer, plus a worker whose coordinator is gone and a session started
/// by hand, for the tree cases.
fn tree_machines() -> Value {
    json!([
        {
            "machine": "local", "label": "Local", "serverId": "S1",
            "projects": [{"projectId": "p", "title": "p", "path": "/p"}],
            "sessions": [
                session("lead", "p", json!({"isCoordinator": true, "createdMs": 10, "lastInteractionMs": 9_000_000, "activity": "working", "workingStartedMs": 8_900_000, "coordinatorRemoteThreads": [{"serverId": "S2", "projectId": "q", "sessionId": "away", "threadState": "working"}]})),
                session("worker", "p", json!({"title": "worker", "activity": "working", "createdMs": 10, "workingStartedMs": 9_600_000, "coordinatorProjectId": "p", "coordinatorSessionId": "lead", "coordinatorThreadState": "working"})),
                session("sub-lead", "p", json!({"isCoordinator": true, "createdMs": 10, "lastInteractionMs": 9_500_000, "coordinatorProjectId": "p", "coordinatorSessionId": "lead"})),
                session("deep", "p", json!({"title": "deepest thread", "activity": "attention", "createdMs": 10, "lastInteractionMs": 9_700_000, "coordinatorProjectId": "p", "coordinatorSessionId": "sub-lead", "coordinatorThreadState": "waiting"})),
                session("old", "p", json!({"createdMs": 10, "lastInteractionMs": 1_000_000, "coordinatorProjectId": "p", "coordinatorSessionId": "lead"})),
                // A second older thread, which stays tucked when "old" is revealed.
                session("stale", "p", json!({"createdMs": 10, "lastInteractionMs": 900_000, "coordinatorProjectId": "p", "coordinatorSessionId": "lead"})),
                session("orphan", "p", json!({"createdMs": 10, "lastInteractionMs": 4_000, "coordinatorProjectId": "p", "coordinatorSessionId": "closed"})),
                session("mine", "p", json!({"createdMs": 10, "lastInteractionMs": 3_000})),
            ],
        },
        {
            "machine": "studio", "label": "Studio", "serverId": "S2",
            "projects": [
                {"projectId": "p", "title": "p", "path": "/p"},
                {"projectId": "q", "title": "q", "path": "/q"},
            ],
            "sessions": [
                // A link names a coordinator on the worker's own computer, so this one's is not here.
                session("elsewhere", "p", json!({"createdMs": 10, "lastInteractionMs": 2_000, "coordinatorProjectId": "p", "coordinatorSessionId": "lead"})),
                // Linked only by its coordinator's `coordinatorRemoteThreads`.
                session("away", "q", json!({"activity": "working", "createdMs": 10, "workingStartedMs": 9_800_000})),
            ],
        },
    ])
}

/// The cases, in the phone's terms. The expected answers are the builder's.
fn cases() -> Vec<Value> {
    let now = 10_000_000_i64;
    let scale_machines = json!([
        {
            "machine": "local", "label": "Local",
            "projects": [
                {"projectId": "zeb", "title": "zebra", "path": "/zebra", "gitRemoteOriginUrl": "https://github.com/o/zebra"},
                {"projectId": "ac-wt", "title": "acme wt", "path": "/acme-wt", "gitRemoteOriginUrl": "https://github.com/o/acme", "worktreeParentPath": "/acme"},
                {"projectId": "ac", "title": "acme", "path": "/acme", "gitRemoteOriginUrl": "https://github.com/o/acme"},
                {"projectId": "bil", "title": "billing", "path": "/billing", "gitRemoteOriginUrl": "https://github.com/o/billing"},
                {"projectId": "quiet", "title": "quiet", "path": "/quiet", "gitRemoteOriginUrl": "https://github.com/o/quiet"},
            ],
            "sessions": [
                session("z1", "zeb", json!({"title": "Tune zebra", "createdMs": 1, "lastInteractionMs": 900})),
                session("a1", "ac", json!({"title": "Fix login", "branch": "feature/Login", "createdMs": 1, "lastInteractionMs": 100})),
                session("a-pin", "ac", json!({"title": "Pinned one", "isPinned": true, "createdMs": 1, "lastInteractionMs": 50})),
                session("a-work", "ac-wt", json!({"title": "Busy", "activity": "working", "createdMs": 1, "workingStartedMs": 500, "lastInteractionMs": 500})),
                session("a-park", "ac", json!({"title": "Later", "isParked": true, "createdMs": 1, "lastInteractionMs": 400})),
                session("a-draft", "ac", json!({"title": "New chat", "isDraft": true, "createdMs": 7, "lastInteractionMs": 600})),
                session("b1", "bil", json!({"title": "Invoices", "createdMs": 1, "lastInteractionMs": 300})),
            ],
        },
        {
            "machine": "studio", "label": "Studio", "connected": false,
            "projects": [
                {"projectId": "rac", "title": "acme", "path": "/r/acme", "gitRemoteOriginUrl": "git@github.com:o/acme.git"},
                {"projectId": "only-wt", "title": "tools wt", "path": "/r/tools-wt", "gitRemoteOriginUrl": "git@github.com:o/tools.git", "worktreeParentPath": "/r/tools"},
            ],
            "sessions": [
                session("r1", "rac", json!({"title": "Remote fix", "snoozedUntilMs": now + 1_000, "createdMs": 1, "lastInteractionMs": 700})),
            ],
        },
    ]);
    let scale = |name: &str, extra: Value| {
        let mut case = json!({
            "name": name,
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "machines": scale_machines,
        });
        if let (Some(case), Some(extra)) = (case.as_object_mut(), extra.as_object()) {
            case.extend(extra.clone());
        }
        case
    };
    vec![
        scale("groups hold every machine's sessions with their own shelves, A to Z until dragged", json!({
            "activeProjectId": "zeb",
        })),
        scale("the saved order comes first, a repository it has not met lands after it, and a drag writes the whole order", json!({
            "order": ["github.com/o/zebra", "gone/repo", "github.com/o/billing"],
            "moves": [
                {"moved": "github.com/o/acme", "target": "github.com/o/zebra"},
                {"moved": "github.com/o/zebra", "target": "github.com/o/billing", "after": true},
                {"moved": "github.com/o/billing", "target": "github.com/o/billing"},
            ],
        })),
        scale("the search field matches a title, a project, a branch and a machine", json!({
            "filter": {"query": "LOGIN"},
        })),
        scale("every word of a search has to match, across project and machine", json!({
            "filter": {"query": "studio acme"},
        })),
        scale("a search and a filter pick narrow together", json!({
            "filter": {"query": "fix", "repos": ["github.com/o/acme"], "machines": ["studio"]},
        })),
        scale("machine chips start on the last used machine, and New session aims at the filtered repository", json!({
            "filter": {"repos": ["github.com/o/acme"]},
            "lastMachines": {"github.com/o/acme": "studio", "github.com/o/zebra": "gone"},
            "activeProjectId": "zeb",
        })),
        scale("machine chips start on the current machine, and New session stays in a project of the filtered repository", json!({
            "filter": {"repos": ["github.com/o/acme"]},
            "currentMachine": "studio",
            "activeProjectId": "ac",
        })),
        scale("the project search matches names and machines", json!({
            "choiceQuery": "stu",
            "filter": {"repos": ["github.com/o/tools"]},
        })),
        scale("the project list puts recent projects first and keeps projects with no session", json!({
            "recentRepos": ["github.com/o/quiet", "github.com/o/billing", "gone/repo"],
        })),
        scale("a machine that is not connected is never where a project starts while another has it", json!({
            "lastMachines": {"github.com/o/acme": "studio"},
            "activeProjectId": "bil",
        })),
        scale("the picker starts on the open project, on its own machine", json!({
            "activeProjectId": "ac-wt",
        })),
        scale("a new session outside the filter joins its repository and machine picks", json!({
            "filter": {"repos": ["github.com/o/zebra"], "machines": ["local"]},
            "createdIn": {"repoKey": "github.com/o/billing", "machineId": "studio"},
        })),
        scale("a new session leaves a kind with no pick empty", json!({
            "filter": {"machines": ["studio"]},
            "createdIn": {"repoKey": "github.com/o/billing", "machineId": "local"},
        })),
        json!({
            "name": "an origin spelled with an SSH host alias, or not known yet, joins its repository's row, and a computer's name in the search picks it",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "choiceQuery": "ghostex win",
            "machines": [
                {
                    "machine": "local", "label": "MacBook",
                    "projects": [
                        {"projectId": "m1", "title": "Ghostex", "path": "/Users/me/dev/Ghostex", "gitRemoteOriginUrl": "https://github.com/maddada/Ghostex.git"},
                        {"projectId": "m2", "title": "ghostex", "path": "/Users/me/old/ghostex"},
                    ],
                    "sessions": [session("s1", "m1", json!({"createdMs": 1, "lastInteractionMs": 50}))],
                },
                {
                    "machine": "win", "label": "Windows",
                    "projects": [
                        {"projectId": "w1", "title": "Ghostex", "path": "C:\\dev\\Ghostex", "gitRemoteOriginUrl": "git@github-work:maddada/Ghostex.git"},
                    ],
                    "sessions": [session("s2", "w1", json!({"createdMs": 1, "lastInteractionMs": 40}))],
                },
                {
                    "machine": "box", "label": "Studio",
                    "projects": [
                        {"projectId": "b1", "title": "Ghostex", "path": "/home/me/Ghostex"},
                    ],
                    "sessions": [],
                },
            ],
        }),
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
        json!({
            "name": "a coordinator's threads nest under its card, folded by default; a worker whose coordinator is gone, and a session started by hand, are cards",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "machines": tree_machines(),
        }),
        json!({
            "name": "an unfolded coordinator lists its busy and recent threads, busy first, with the rest behind its older row; a sub-coordinator stays folded",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "folds": {"unfolded": ["local|p|lead"]},
            "machines": tree_machines(),
        }),
        json!({
            "name": "a revealed older thread is listed with its coordinators unfolded, its siblings' older threads stay tucked",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "folds": {"unfolded": ["local|p|lead"], "reveal": "old"},
            "machines": tree_machines(),
        }),
        json!({
            "name": "a coordinator's older threads and a sub-coordinator's threads listed",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "folds": {"unfolded": ["local|p|lead", "local|p|sub-lead"], "olderShown": ["local|p|lead"]},
            "machines": tree_machines(),
        }),
        json!({
            "name": "a search for a thread keeps its coordinator's card and shows the way to it while folded",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {"query": "deepest"},
            "machines": tree_machines(),
        }),
        json!({
            "name": "a search matching a folded coordinator and one of its threads opens it to that thread",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {"query": "api"},
            "machines": [{
                "machine": "local", "label": "Local", "serverId": "S1",
                "projects": [{"projectId": "p", "title": "p", "path": "/p"}],
                "sessions": [
                    session("api-lead", "p", json!({"title": "API coordinator", "isCoordinator": true, "createdMs": 10, "lastInteractionMs": 9_000_000})),
                    session("api-tests", "p", json!({"title": "API tests", "createdMs": 10, "lastInteractionMs": 9_100_000, "coordinatorProjectId": "p", "coordinatorSessionId": "api-lead"})),
                    session("docs", "p", json!({"title": "Docs", "createdMs": 10, "lastInteractionMs": 9_200_000, "coordinatorProjectId": "p", "coordinatorSessionId": "api-lead"})),
                ],
            }],
        }),
        json!({
            "name": "a chain of unfolded coordinators is drawn no deeper than the Project sidebar's tree",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "folds": {"unfolded": ["local|p|c1", "local|p|c2", "local|p|c3", "local|p|c4"]},
            "machines": [{
                "machine": "local", "label": "Local", "serverId": "S1",
                "projects": [{"projectId": "p", "title": "p", "path": "/p"}],
                "sessions": [
                    session("c1", "p", json!({"isCoordinator": true, "createdMs": 10, "lastInteractionMs": 9_000_000})),
                    session("c2", "p", json!({"isCoordinator": true, "createdMs": 10, "lastInteractionMs": 9_000_000, "coordinatorProjectId": "p", "coordinatorSessionId": "c1"})),
                    session("c3", "p", json!({"isCoordinator": true, "createdMs": 10, "lastInteractionMs": 9_000_000, "coordinatorProjectId": "p", "coordinatorSessionId": "c2"})),
                    session("c4", "p", json!({"isCoordinator": true, "createdMs": 10, "lastInteractionMs": 9_000_000, "coordinatorProjectId": "p", "coordinatorSessionId": "c3"})),
                    session("leaf", "p", json!({"createdMs": 10, "lastInteractionMs": 9_000_000, "coordinatorProjectId": "p", "coordinatorSessionId": "c4"})),
                ],
            }],
        }),
        json!({
            "name": "a computer filter keeps a coordinator for its thread on that computer",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {"machines": ["studio"]},
            "machines": tree_machines(),
        }),
        json!({
            "name": "parked and snoozed threads are cards; a failed hidden thread is red; a hidden focused thread marks its coordinator; a loop stays cards",
            "nowMs": now,
            "settings": {"workingSection": true, "parking": true},
            "filter": {},
            "machines": [{
                "machine": "local", "label": "Local", "serverId": "S1",
                "projects": [{"projectId": "p", "title": "p", "path": "/p"}],
                "sessions": [
                    session("boss", "p", json!({"isCoordinator": true, "createdMs": 10, "lastInteractionMs": 9_000_000})),
                    session("parked", "p", json!({"isParked": true, "createdMs": 10, "lastInteractionMs": 9_100_000, "coordinatorProjectId": "p", "coordinatorSessionId": "boss"})),
                    session("napping", "p", json!({"snoozedUntilMs": 20_000_000, "createdMs": 10, "lastInteractionMs": 9_200_000, "coordinatorProjectId": "p", "coordinatorSessionId": "boss"})),
                    session("broke", "p", json!({"modelSelectionFailed": true, "createdMs": 10, "lastInteractionMs": 9_300_000, "coordinatorProjectId": "p", "coordinatorSessionId": "boss"})),
                    session("looking", "p", json!({"isFocused": true, "createdMs": 10, "lastInteractionMs": 9_400_000, "coordinatorProjectId": "p", "coordinatorSessionId": "boss"})),
                    session("left", "p", json!({"isCoordinator": true, "createdMs": 10, "lastInteractionMs": 8_000_000, "coordinatorProjectId": "p", "coordinatorSessionId": "right"})),
                    session("right", "p", json!({"isCoordinator": true, "createdMs": 10, "lastInteractionMs": 8_100_000, "coordinatorProjectId": "p", "coordinatorSessionId": "left"})),
                ],
            }],
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
