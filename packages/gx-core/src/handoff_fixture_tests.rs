//! The handoff rules' shared fixture: cases with what these functions make of them, kept in the
//! phone app so its port is tested against the same answers.
//!
//! CDXC:TranscriptExport 2026-10-31 WHY:
//! User: the phone gets the same Handoff / Export and the same lead-in. The phone's sheet runs in TypeScript in front of a computer it reaches over SSH, and gx-core runs on neither the phone nor its WebAssembly chat core, so `apps/mobile/app/src/contract/handoff/handoff.ts` is a port of `session_handoff.rs` and `handoff_target.rs`. So that the two cannot drift, this test runs every case of `handoffFixture.json` through these functions and fails when the file's expected answers differ; `UPDATE_HANDOFF_FIXTURE=1 cargo test handoff_fixture` rewrites them. The phone's `handoff.test.ts` runs the same cases through its port. A checkout without the phone app skips the comparison.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::handoff_prefs::{
    handoff_history_estimate, handoff_history_stop_index, handoff_include_summary,
    handoff_prefs_json, parse_handoff_prefs, remembered_handoff_destination, HandoffDestination,
};
use crate::handoff_target::{
    branch_difference_line, handoff_destination_label, handoff_project_label,
    rank_handoff_computer, repository_display_name, repository_key, HandoffProjectCandidate,
};
use crate::session_handoff::{handoff_draft, handoff_lead_in};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/app/src/contract/handoff/handoffFixture.json")
}

fn text<'a>(case: &'a Value, key: &str) -> &'a str {
    case[key].as_str().unwrap_or_default()
}

fn answer(case: &Value) -> Value {
    match text(case, "kind") {
        "draft" => {
            let session = text(case, "sessionTitle");
            let project = text(case, "projectTitle");
            json!({
                "leadIn": handoff_lead_in(session, project),
                "draft": handoff_draft(text(case, "path"), session, project),
            })
        }
        "repository" => {
            let url = text(case, "url");
            json!({
                "key": repository_key(url),
                "name": repository_display_name(url),
            })
        }
        "computer" => {
            let candidates: Vec<HandoffProjectCandidate> = case["candidates"]
                .as_array()
                .expect("candidates")
                .iter()
                .map(|candidate| HandoffProjectCandidate {
                    project_id: text(candidate, "projectId").to_string(),
                    title: text(candidate, "title").to_string(),
                    path: candidate["path"].as_str().map(str::to_string),
                    origin_url: candidate["originUrl"].as_str().map(str::to_string),
                    is_worktree: candidate["isWorktree"].as_bool() == Some(true),
                })
                .collect();
            let ranked = rank_handoff_computer(
                case["originUrl"].as_str(),
                case["sourcePath"].as_str(),
                &candidates,
                case["ownProject"].as_str(),
            );
            json!({
                "choices": ranked.ranking.choices.iter().map(|choice| json!({
                    "projectId": choice.project_id,
                    "sameRepo": choice.same_repo,
                    "isOwn": choice.is_own,
                    "label": handoff_project_label(
                        &choice.title,
                        choice.is_own,
                        choice.same_repo,
                        choice.path.as_deref(),
                    ),
                })).collect::<Vec<_>>(),
                "preferredProjectId": ranked.ranking.preferred_project_id,
                "missingRepository": ranked.missing_repository,
            })
        }
        "branch" => json!({
            "line": branch_difference_line(
                case["source"].as_str(),
                case["target"].as_str(),
                text(case, "label"),
            ),
        }),
        "button" => json!({
            "label": handoff_destination_label(
                text(case, "projectTitle"),
                text(case, "computerLabel"),
                case["isSource"].as_bool() == Some(true),
            ),
        }),
        // What a saved value reads as, written back in this shape.
        "prefs" => handoff_prefs_json(&parse_handoff_prefs(&case["saved"])),
        "destination" => {
            let of = |value: &Value| HandoffDestination {
                machine_id: text(value, "machineId").to_string(),
                project_id: text(value, "projectId").to_string(),
            };
            let offered = case["offered"].as_array().expect("offered");
            let remembered = case["remembered"]
                .is_object()
                .then(|| of(&case["remembered"]));
            let destination = remembered_handoff_destination(
                remembered.as_ref(),
                &of(&case["own"]),
                |destination| offered.iter().any(|entry| of(entry) == *destination),
            );
            json!({
                "destination": destination.map(|destination| json!({
                    "machineId": destination.machine_id,
                    "projectId": destination.project_id,
                })),
            })
        }
        // The History heading and hint for a saved choice and the computer's count.
        "history" => {
            let history = parse_handoff_prefs(&case["saved"]).history;
            let sizes: Vec<usize> = case["lastTurnsBytes"]
                .as_array()
                .map(|sizes| {
                    sizes
                        .iter()
                        .filter_map(Value::as_u64)
                        .map(|size| size as usize)
                        .collect()
                })
                .unwrap_or_default();
            json!({
                "lastTurns": history.last_turns(),
                "stopIndex": handoff_history_stop_index(history.turns),
                "summary": handoff_include_summary(&history, text(case, "include")),
                "estimate": handoff_history_estimate(
                    &history,
                    case["turns"].as_u64().unwrap_or_default() as usize,
                    case["bytes"].as_u64().unwrap_or_default() as usize,
                    &sizes,
                ),
            })
        }
        other => panic!("unknown case kind {other}"),
    }
}

#[test]
fn handoff_fixture_holds_these_functions_answers() {
    let path = fixture_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let mut fixture: Value = serde_json::from_str(&raw).expect("the fixture parses");
    let update = std::env::var_os("UPDATE_HANDOFF_FIXTURE").is_some();
    for case in fixture["cases"].as_array_mut().expect("cases") {
        let expected = answer(case);
        if update {
            case["expected"] = expected;
        } else {
            assert_eq!(case["expected"], expected, "{}", text(case, "name"));
        }
    }
    if update {
        let written = serde_json::to_string_pretty(&fixture).expect("the fixture serializes");
        std::fs::write(&path, format!("{written}\n")).expect("the fixture is written");
    }
}
