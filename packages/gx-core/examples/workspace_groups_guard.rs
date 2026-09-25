//! The interleaving gate for the workspace-groups pending-push guard.
//!
//! The failure this exists to catch is OSCILLATION, and oscillation is not visible in a final
//! state: a local move lands, a stale echo undoes it, the push lands, the daemon's next echo redoes
//! it, and the user watches a row jump back and then forward while the sequence ends exactly where
//! it should. So this probe records what is held after EVERY event, not after the last one, and the
//! TypeScript half is driven through the same script so the two are compared step by step.
//!
//! The scripts are enumerated rather than chosen. Every well-formed sequence of four events over
//! the alphabet below is written out, which puts an echo before the push, between the push starting
//! and its answer, and after it, without anyone having to think of those three cases by name. A
//! sequence is well-formed when a push answer follows a push that started, and when a push starts
//! only while one is booked, because firing a timer nobody booked is not something the app can do.
//!
//! The second half of this probe is the PRUNE, which is a delete path over the user's own data and
//! therefore gets its own cases rather than riding on the guard's. `pruneWorkspaceGroupAssignments`
//! used to run on every sidebar build and write whatever the old runtime held; it is Rust's now, and
//! the case that matters most is the one that must NOT happen: a project the presentation does not
//! list keeps every member, because a machine whose rows have not arrived lists nothing and pruning
//! against nothing would delete every group the user made on it.
//!
//!   bun tooling/gx-core/action-parity.ts workspace-groups <out-dir>   # the TypeScript half
//!   cargo run --release --example workspace_groups_guard -- <out-dir>

use std::collections::{BTreeMap, BTreeSet};

use ghostex_gx_core::{
    document_reconcile_wanted, AdoptOutcome, ProjectWorkspaceGroups, WorkspaceGroupsDocument,
    WorkspaceGroupsEffect, WorkspaceGroupsSync, WorkspaceSubgroup,
};
use serde_json::{json, Value};

/// The events a script is made of.
const EVENTS: [&str; 9] = [
    "editA",
    "editB",
    "echoNone",
    "echoEmpty",
    "echoA",
    "echoB",
    "pushStart",
    "pushOk",
    // The failure leg matters as much as the success one: a push that cannot reach the server
    // keeps the flag UP and retries for ever, which is what stops a stale echo overwriting a
    // document just because the network is down.
    "pushFail",
];

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_default();
    if out_dir.is_empty() {
        eprintln!("usage: workspace_groups_guard <out-dir>");
        std::process::exit(2);
    }
    let documents = documents();
    let scripts = scripts();
    let mut cases = Vec::new();
    for (start_name, start) in [("empty", 0usize), ("a", 1), ("b", 2)] {
        for script in &scripts {
            cases.push(run(
                start_name,
                documents[start].clone(),
                script,
                &documents,
            ));
        }
    }
    let steps: usize = cases
        .iter()
        .map(|case| case["steps"].as_array().map(Vec::len).unwrap_or(0))
        .sum();
    let prune_cases = prune_cases();
    let launch_cases = launch_cases();
    let path = std::path::Path::new(&out_dir).join("rust-groups.json");
    std::fs::write(
        &path,
        serde_json::to_string(&json!({
            "documents": documents.iter().map(WorkspaceGroupsDocument::to_json).collect::<Vec<_>>(),
            "cases": cases,
            "pruneCases": prune_cases,
            "launchCases": launch_cases,
        }))
        .expect("serialize"),
    )
    .expect("write");
    println!(
        "workspace groups guard: {} cases, {steps} steps, {} prune cases, {} launch cases, written to {}",
        cases.len(),
        prune_cases.len(),
        launch_cases.len(),
        path.display()
    );
}

/// THE SEQUENCE EVERY LAUNCH PERFORMS, which the interleaving enumeration above never does.
///
/// Those 5,643 scripts are made of edits, echoes and pushes, and every one of them starts from a
/// document that is simply `restore`d. A real launch is different in a way that turned out to
/// matter: the HOST reads the stored key, puts that document into the store's side state ITSELF,
/// and only then does the daemon's first snapshot land. The reducer therefore compares the daemon's
/// copy against what the host just wrote rather than against nothing, and on an ordinary launch,
/// where the two agree, it reports no change and the host asks the guard nothing at all. A whole
/// run went by with the guard never once consulted, and the enumeration could not see it because
/// "the host seeded the store" is not one of its events.
///
/// So this probe drives the real decision: both orders of (the stored key landing, the snapshot
/// landing), every pair of stored and server documents, and the rule
/// `document_reconcile_wanted` deciding whether the guard is asked. What it counts is `asked`:
/// with the rule reduced to the change flag alone, the equal pairs are never asked and that
/// counter collapses, which is the shape of the defect rather than a difference in the answer.
fn launch_cases() -> Vec<Value> {
    let documents = documents();
    let names = ["empty", "a", "b"];
    let mut cases = Vec::new();
    for (stored_index, stored_name) in names.iter().enumerate() {
        for (server_index, server_name) in names.iter().enumerate() {
            for stored_first in [true, false] {
                let stored = documents[stored_index].clone();
                let server = documents[server_index].clone();
                let mut sync = WorkspaceGroupsSync::default();
                // What the store's side state holds, which the host writes as well as the daemon.
                let mut side: Option<WorkspaceGroupsDocument> = None;
                let mut asked = 0usize;
                let mut outcome = None;
                let mut effects = Vec::new();
                let mut judge = |sync: &mut WorkspaceGroupsSync,
                                 side: &mut Option<WorkspaceGroupsDocument>,
                                 asked: &mut usize,
                                 outcome: &mut Option<AdoptOutcome>,
                                 effects: &mut Vec<WorkspaceGroupsEffect>,
                                 reloaded: bool| {
                    // `note_workspace_groups_change`: the flag is "the daemon's copy differs from
                    // what is held", and what is held is whatever the host last put there.
                    let changed = side.as_ref() != Some(&server);
                    *side = Some(server.clone());
                    if !document_reconcile_wanted(changed, reloaded) {
                        return;
                    }
                    *asked += 1;
                    let value = server.to_json();
                    let (answer, next) = sync.adopt(Some(&value));
                    *outcome = Some(answer);
                    *effects = next;
                };
                if stored_first {
                    // The host's read lands first: it restores the guard AND seeds the side state.
                    sync.restore(stored.clone());
                    side = Some(stored.clone());
                    judge(
                        &mut sync,
                        &mut side,
                        &mut asked,
                        &mut outcome,
                        &mut effects,
                        true,
                    );
                } else {
                    // The snapshot lands first, so the guard has not been restored yet. The host
                    // defers, the read lands, and the deferral is recovered.
                    judge(
                        &mut sync,
                        &mut side,
                        &mut asked,
                        &mut outcome,
                        &mut effects,
                        true,
                    );
                    sync.restore(stored.clone());
                    judge(
                        &mut sync,
                        &mut side,
                        &mut asked,
                        &mut outcome,
                        &mut effects,
                        true,
                    );
                }
                cases.push(json!({
                    "name": format!("{stored_name}/{server_name}/{}", match stored_first {
                        true => "storedFirst",
                        false => "snapshotFirst",
                    }),
                    "asked": asked,
                    "outcome": outcome.map(outcome_name),
                    "document": sync.document().to_json(),
                    "pending": sync.is_pending(),
                    "schedules": effects
                        .iter()
                        .filter(|effect| matches!(effect, WorkspaceGroupsEffect::SchedulePush { .. }))
                        .count(),
                }));
            }
        }
    }
    cases
}

/// The prune, case by case. Each entry carries the document, the projects the presentation lists
/// with the sessions it holds for each, and what `prune_projects` answered: the document, and
/// whether it answered at all, which is what decides a storage write and a push.
///
/// The cases are written by hand rather than taken from a recording because a recording has no
/// user-made groups in it at all, and because the two answers worth being sure about are the two a
/// recording could never contain: a project the presentation does not list, and a project it lists
/// with nothing in it.
fn prune_cases() -> Vec<Value> {
    let group = |group_id: &str, ids: &[&str]| WorkspaceSubgroup {
        group_id: group_id.to_string(),
        session_ids: ids.iter().map(|id| id.to_string()).collect(),
        title: format!("Group {group_id}"),
    };
    let project = |groups: Vec<WorkspaceSubgroup>| ProjectWorkspaceGroups {
        groups,
        next_group_number: 4,
    };
    let document = |entries: Vec<(&str, ProjectWorkspaceGroups)>| {
        let mut projects = BTreeMap::new();
        let mut order = Vec::new();
        for (project_id, groups) in entries {
            order.push(project_id.to_string());
            projects.insert(project_id.to_string(), groups);
        }
        WorkspaceGroupsDocument {
            project_order: order,
            projects,
        }
    };
    let listed = |entries: Vec<(&str, Vec<&str>)>| -> Vec<(String, BTreeSet<String>)> {
        entries
            .into_iter()
            .map(|(project_id, ids)| {
                (
                    project_id.to_string(),
                    ids.iter().map(|id| id.to_string()).collect(),
                )
            })
            .collect()
    };

    let two_groups = || {
        project(vec![
            group("group-2", &["S1", "S2"]),
            group("group-3", &["S3"]),
        ])
    };
    let cases: Vec<(
        &str,
        WorkspaceGroupsDocument,
        Vec<(String, BTreeSet<String>)>,
    )> = vec![
        (
            "nothing-dropped",
            document(vec![("P1", two_groups())]),
            listed(vec![("P1", vec!["S1", "S2", "S3", "S4"])]),
        ),
        (
            "one-dropped",
            document(vec![("P1", two_groups())]),
            listed(vec![("P1", vec!["S1", "S3"])]),
        ),
        (
            // A group whose every member is gone stays, empty. The sidebar still draws its heading
            // and the user can still drop a row into it, which is why the entry is not removed.
            "a-group-emptied",
            document(vec![("P1", two_groups())]),
            listed(vec![("P1", vec!["S3"])]),
        ),
        (
            // The presentation lists the project and has no sessions for it at all, which is what a
            // project whose last session was closed looks like.
            "listed-with-nothing",
            document(vec![("P1", two_groups())]),
            listed(vec![("P1", vec![])]),
        ),
        (
            // THE ONE THAT MUST NOT PRUNE. P2 is in the document and the presentation does not list
            // it, which is every project of a machine that has not connected.
            "unlisted-project-untouched",
            document(vec![("P1", two_groups()), ("P2", two_groups())]),
            listed(vec![("P1", vec!["S1", "S2", "S3"])]),
        ),
        (
            "unlisted-project-untouched-while-another-prunes",
            document(vec![("P1", two_groups()), ("P2", two_groups())]),
            listed(vec![("P1", vec!["S1"])]),
        ),
        (
            // Two projects losing a member each is ONE edit, not two writes and two pushes.
            "two-projects-one-pass",
            document(vec![("P1", two_groups()), ("P2", two_groups())]),
            listed(vec![("P1", vec!["S1", "S3"]), ("P2", vec!["S2", "S3"])]),
        ),
        (
            // A project the presentation lists that the document has no entry for: the identity
            // return that must not count as an edit.
            "listed-project-with-no-entry",
            document(vec![("P1", two_groups())]),
            listed(vec![("P1", vec!["S1", "S2", "S3"]), ("P9", vec!["S8"])]),
        ),
        (
            // A remote project, whose document key carries the machine. The prune reads the key as
            // an opaque string, and this is the case that says so.
            "remote-project",
            document(vec![("remote:m1:project:P1", two_groups())]),
            listed(vec![("remote:m1:project:P1", vec!["S2"])]),
        ),
        (
            "empty-document",
            WorkspaceGroupsDocument::default(),
            listed(vec![("P1", vec!["S1"])]),
        ),
    ];

    cases
        .into_iter()
        .map(|(name, document, projects)| {
            let pruned = document.prune_projects(
                projects
                    .iter()
                    .map(|(project_id, ids)| (project_id.as_str(), ids)),
            );
            json!({
                "name": name,
                "document": document.to_json(),
                "projects": projects
                    .iter()
                    .map(|(project_id, ids)| json!({
                        "projectId": project_id,
                        "sessionIds": ids.iter().collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
                // `edited` is the whole reason this is compared rather than assumed: it is what
                // decides a client-storage write and a push, and the TypeScript expresses it as
                // "the same object came back".
                "edited": pruned.is_some(),
                "result": pruned.unwrap_or(document).to_json(),
            })
        })
        .collect()
}

/// Three documents: empty, and two that differ in the one place a move changes, which is the order
/// of the session ids inside a group.
fn documents() -> Vec<WorkspaceGroupsDocument> {
    let group = |ids: &[&str]| WorkspaceSubgroup {
        group_id: "group-2".to_string(),
        session_ids: ids.iter().map(|id| id.to_string()).collect(),
        title: "Group 2".to_string(),
    };
    let doc = |ids: &[&str]| {
        let mut projects = BTreeMap::new();
        projects.insert(
            "P1".to_string(),
            ProjectWorkspaceGroups {
                groups: vec![group(ids)],
                next_group_number: 3,
            },
        );
        WorkspaceGroupsDocument {
            project_order: vec!["P1".to_string()],
            projects,
        }
    };
    vec![
        WorkspaceGroupsDocument::default(),
        doc(&["S1", "S2", "S3"]),
        doc(&["S2", "S1", "S3"]),
    ]
}

/// Every well-formed sequence of four events.
fn scripts() -> Vec<Vec<&'static str>> {
    let mut scripts = Vec::new();
    for a in EVENTS {
        for b in EVENTS {
            for c in EVENTS {
                for d in EVENTS {
                    let script = vec![a, b, c, d];
                    if well_formed(&script) {
                        scripts.push(script);
                    }
                }
            }
        }
    }
    scripts
}

/// A push answer needs a push in flight, and a push starts only while one is booked. Both are
/// properties of the SCRIPT and not of either implementation, so they are decided here once and
/// both sides are driven through the same list.
fn well_formed(script: &[&str]) -> bool {
    let mut in_flight = false;
    let mut booked = false;
    for event in script {
        match *event {
            "editA" | "editB" => booked = true,
            "pushStart" => {
                if !booked || in_flight {
                    return false;
                }
                booked = false;
                in_flight = true;
            }
            "pushOk" | "pushFail" => {
                if !in_flight {
                    return false;
                }
                in_flight = false;
            }
            // An echo that schedules a push books one, which a later pushStart may then fire.
            "echoEmpty" => booked = true,
            _ => {}
        }
    }
    true
}

/// One script, run against the guard, recording what is held after every event.
fn run(
    start_name: &str,
    start: WorkspaceGroupsDocument,
    script: &[&str],
    documents: &[WorkspaceGroupsDocument],
) -> Value {
    let mut sync = WorkspaceGroupsSync::default();
    sync.restore(start);
    let mut in_flight: Option<u64> = None;
    let mut steps = Vec::new();
    for event in script {
        let (outcome, effects) = match *event {
            "editA" => (None, sync.edit(documents[1].clone())),
            "editB" => (None, sync.edit(documents[2].clone())),
            "echoNone" => {
                let (outcome, effects) = sync.adopt(None);
                (Some(outcome), effects)
            }
            "echoEmpty" => {
                let (outcome, effects) = sync.adopt(Some(&json!({})));
                (Some(outcome), effects)
            }
            "echoA" => {
                let value = documents[1].to_json();
                let (outcome, effects) = sync.adopt(Some(&value));
                (Some(outcome), effects)
            }
            "echoB" => {
                let value = documents[2].to_json();
                let (outcome, effects) = sync.adopt(Some(&value));
                (Some(outcome), effects)
            }
            "pushStart" => {
                let (_document, revision) = sync.push_started();
                in_flight = Some(revision);
                (None, Vec::new())
            }
            "pushOk" | "pushFail" => {
                let revision = in_flight.take().unwrap_or_default();
                (None, sync.push_finished(revision, *event == "pushOk"))
            }
            _ => (None, Vec::new()),
        };
        steps.push(json!({
            "event": event,
            // What the user is looking at after this event, which is the whole point: a sequence
            // that ends right having gone wrong in the middle is the bug.
            "document": sync.document().to_json(),
            "pending": sync.is_pending(),
            "outcome": outcome.map(outcome_name),
            "writes": effects
                .iter()
                .filter(|effect| matches!(effect, WorkspaceGroupsEffect::WriteStorage { .. }))
                .count(),
            "schedules": effects
                .iter()
                .filter_map(|effect| match effect {
                    WorkspaceGroupsEffect::SchedulePush { delay_ms } => Some(*delay_ms),
                    _ => None,
                })
                .collect::<Vec<_>>(),
        }));
    }
    json!({ "start": start_name, "script": script, "steps": steps })
}

fn outcome_name(outcome: AdoptOutcome) -> &'static str {
    match outcome {
        // `echoNone` is the one script event that produces this, and it is its own outcome since
        // 2026-09-21: folding it into `ignoredPending` made the guard's own counter count the
        // guard never being asked.
        AdoptOutcome::NoEcho => "noEcho",
        // Structurally unreachable for THIS document and named rather than hidden under a
        // wildcard: `parseGpuiWorkspaceSessionGroupsState` answers an empty document for anything
        // that is not an object, so nothing the daemon can send refuses to parse here. A run that
        // ever reports it means that rule changed, which is what a named arm says and a `_` does
        // not.
        AdoptOutcome::Unparsable => "unparsable",
        AdoptOutcome::IgnoredPending => "ignoredPending",
        AdoptOutcome::IgnoredEqual => "ignoredEqual",
        AdoptOutcome::ScheduledPush => "scheduledPush",
        AdoptOutcome::Adopted => "adopted",
    }
}
