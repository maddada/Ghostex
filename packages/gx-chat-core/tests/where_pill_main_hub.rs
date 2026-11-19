//! CDXC:Chief 2026-11-19 (extras/where_pill.rs): a Main Hub session's where pill and card read
//! "Main Hub" with its icon and have no Branch row when its folder is not a git repository; any
//! other session keeps "Not a git repository". The answers are gxserver's `/api/readSessionWhere`
//! as a throwaway test server gave them.

use ghostex_gx_chat_core::extras::where_pill;
use ghostex_gx_chat_core::state::ChatState;
use serde_json::{json, Value};

fn pill(answer: Value) -> Value {
    let mut state = ChatState::default();
    state.core.controller_started = true;
    state.extras.where_pill.data = Some(answer);
    where_pill::document(&state)
}

fn answer(project: &str, main_hub: bool, folder: &str) -> Value {
    json!({
        "project": project,
        "mainHub": main_hub,
        "folder": folder,
        "folderLabel": folder,
        "location": {"kind": "local", "computer": "Studio"},
        "worktree": null,
        "git": null,
        "agent": "claude",
        "account": null,
        "accountEmail": null,
        "terminal": "zmx",
    })
}

fn row_ids(pill: &Value) -> Vec<String> {
    pill["card"]["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["id"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn a_main_hub_session_reads_main_hub_without_a_branch_row() {
    let pill = pill(answer("Main Hub", true, "/Users/me/.config/ghostex"));
    println!("{}", serde_json::to_string_pretty(&pill).unwrap());
    assert_eq!(pill["project"], "Main Hub");
    assert_eq!(pill["mainHub"], true);
    assert_eq!(pill["card"]["title"], "Main Hub");
    assert_eq!(pill["card"]["icon"], "mainHub");
    assert_eq!(pill["branch"], Value::Null);
    assert_eq!(row_ids(&pill), vec!["folder", "agent", "terminal"]);
}

#[test]
fn any_other_session_without_git_keeps_not_a_git_repository() {
    let pill = pill(answer("plain", false, "/Users/me/plain"));
    assert_eq!(pill["card"]["title"], "plain");
    assert_eq!(pill["card"]["icon"], "folder");
    assert_eq!(
        row_ids(&pill),
        vec!["branch", "folder", "agent", "terminal"]
    );
    assert_eq!(pill["card"]["rows"][0]["value"], "Not a git repository");
}
