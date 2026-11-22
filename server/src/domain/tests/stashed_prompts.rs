use super::*;
use rusqlite::params;
use serde_json::{json, Map, Value};

fn stash_params(content: &str, project_id: Option<&str>) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("content".to_string(), json!(content));
    if let Some(project_id) = project_id {
        params.insert("projectId".to_string(), json!(project_id));
    }
    params.insert("sessionId".to_string(), json!("G1abc"));
    params.insert("cwd".to_string(), json!("/tmp/example"));
    params
}

fn listed_stash_contents(repository: &DomainRepository, project_id: Option<&str>) -> Vec<String> {
    let mut params = Map::new();
    if let Some(project_id) = project_id {
        params.insert("projectId".to_string(), json!(project_id));
    }
    repository
        .list_stashed_prompts(&params)
        .expect("list stashed prompts")
        .get("prompts")
        .and_then(Value::as_array)
        .expect("prompts array")
        .iter()
        .map(|prompt| value_str(prompt, "content").to_string())
        .collect()
}

#[test]
fn stashed_prompts_save_dedupes_same_project_content() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");

    let first = repository
        .save_stashed_prompt(&stash_params("fix the login bug", Some("P1aaa")))
        .expect("first save");
    let second = repository
        .save_stashed_prompt(&stash_params("fix the login bug", Some("P1aaa")))
        .expect("duplicate save");
    assert_eq!(
        value_str(first.get("prompt").expect("prompt"), "promptId"),
        value_str(second.get("prompt").expect("prompt"), "promptId"),
    );
    assert_eq!(listed_stash_contents(&repository, None).len(), 1);

    // Same content in another project stays a separate stash.
    repository
        .save_stashed_prompt(&stash_params("fix the login bug", Some("P2bbb")))
        .expect("other-project save");
    assert_eq!(listed_stash_contents(&repository, None).len(), 2);

    let error = repository
        .save_stashed_prompt(&stash_params("   \n  ", Some("P1aaa")))
        .expect_err("blank content rejected");
    assert_eq!(error.code, "badRequest");
}

#[test]
fn stashed_prompts_edit_updates_existing_row_in_place() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");

    let saved = repository
        .save_stashed_prompt(&stash_params("original prompt", Some("P1aaa")))
        .expect("save prompt");
    let saved_prompt = saved.get("prompt").expect("saved prompt");
    let prompt_id = value_str(saved_prompt, "promptId").to_string();

    let mut edit_params = stash_params("edited prompt", Some("P2bbb"));
    edit_params.insert("promptId".to_string(), json!(prompt_id));
    let edited = repository
        .save_stashed_prompt(&edit_params)
        .expect("edit prompt");
    let edited_prompt = edited.get("prompt").expect("edited prompt");

    assert_eq!(value_str(edited_prompt, "promptId"), prompt_id);
    assert_eq!(value_str(edited_prompt, "content"), "edited prompt");
    assert_eq!(value_str(edited_prompt, "projectId"), "P1aaa");
    assert_eq!(
        listed_stash_contents(&repository, None),
        vec!["edited prompt"]
    );
}

#[test]
fn stashed_prompts_list_scopes_to_project_and_delete_removes() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");

    repository
        .save_stashed_prompt(&stash_params("prompt in project A", Some("P1aaa")))
        .expect("save A");
    repository
        .save_stashed_prompt(&stash_params("prompt in project B", Some("P2bbb")))
        .expect("save B");
    repository
        .save_stashed_prompt(&stash_params("projectless prompt", None))
        .expect("save projectless");

    assert_eq!(
        listed_stash_contents(&repository, Some("P1aaa")),
        vec!["prompt in project A".to_string()]
    );
    let all = listed_stash_contents(&repository, None);
    assert_eq!(all.len(), 3);
    // Newest first.
    assert_eq!(all[0], "projectless prompt");

    let saved = repository
        .save_stashed_prompt(&stash_params("prompt in project B", Some("P2bbb")))
        .expect("re-save B");
    let prompt_id = value_str(saved.get("prompt").expect("prompt"), "promptId").to_string();
    let mut delete_params = Map::new();
    delete_params.insert("promptId".to_string(), json!(prompt_id));
    let deleted = repository
        .delete_stashed_prompt(&delete_params)
        .expect("delete");
    assert_eq!(deleted.get("deleted"), Some(&json!(true)));
    assert_eq!(listed_stash_contents(&repository, None).len(), 2);
    let deleted_again = repository
        .delete_stashed_prompt(&delete_params)
        .expect("delete again");
    assert_eq!(deleted_again.get("deleted"), Some(&json!(false)));
}

#[test]
fn stashed_prompts_project_scope_includes_legacy_worktree_family() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");

    // Register a parent project and a legacy worktree-as-project child by
    // writing the rows directly; detect_registered_git_worktree_metadata
    // needs a real git checkout, which this scoping test does not.
    let parent = repository
        .create_project(
            json!({ "name": "Main", "path": "/tmp/stash-main" })
                .as_object()
                .expect("parent params"),
        )
        .expect("parent project");
    let parent_id = value_str(&parent, "projectId").to_string();
    let child = repository
        .create_project(
            json!({ "name": "Main Worktree", "path": "/tmp/stash-worktree" })
                .as_object()
                .expect("child params"),
        )
        .expect("child project");
    let child_id = value_str(&child, "projectId").to_string();
    db.execute(
        "UPDATE projects SET worktreeJson = ?1 WHERE projectId = ?2",
        params![
            json!({ "parentProjectId": parent_id }).to_string(),
            child_id
        ],
    )
    .expect("mark worktree project");

    repository
        .save_stashed_prompt(&stash_params("parent prompt", Some(&parent_id)))
        .expect("save parent");
    repository
        .save_stashed_prompt(&stash_params("worktree prompt", Some(&child_id)))
        .expect("save worktree");
    repository
        .save_stashed_prompt(&stash_params("unrelated prompt", Some("P9zzz")))
        .expect("save unrelated");

    let mut from_parent = listed_stash_contents(&repository, Some(&parent_id));
    let mut from_child = listed_stash_contents(&repository, Some(&child_id));
    from_parent.sort();
    from_child.sort();
    let expected = vec!["parent prompt".to_string(), "worktree prompt".to_string()];
    assert_eq!(from_parent, expected);
    assert_eq!(from_child, expected);
}
