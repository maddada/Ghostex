//! The Rust half of the Git parity harness (tooling/gx-core/git-menu-parity.ts): reads the
//! scenarios the TypeScript half wrote and answers each with gx-core's `git_menu`, in the shape the
//! frozen TypeScript answers, so the harness can diff the two.
//!
//!   cargo run --example git_menu_parity -- <out-dir>      # from packages/gx-core

use ghostex_gx_core::git_menu::{
    agent_workflow_title, confirm_label, finished_title, is_managed_worktree_branch,
    merge_conflict_prompt, normalize_existing_worktree_options, normalize_project_path,
    normalize_relative_git_file_path, normalize_worktree_base_branches, parse_commit_message,
    plan_git_action_after_read, plan_git_action_before_read, plan_git_poll_cycle,
    project_name_from_path, prompt_description, pull_request_agent_prompt,
    resolve_trusted_file_selection, review_modal_draft, short_status_has_changes, started_title,
    sync_with_main_prompt, titlebar_menu, user_visible_git_error, worktree_branch_name,
    worktree_folder_suffix, worktree_list_error_text, worktree_rename_user_visible_error,
    worktree_slug_from_prompt, worktree_user_visible_error, GitAction, GitActionStep,
    GitChangedFile, GitMutation, GitPollTarget, GitReviewAction, GitState, PendingGitReview,
    ReviewDraftInput,
};
use serde_json::{json, Value};

fn step_json(step: Option<GitActionStep>) -> Value {
    match step {
        None => Value::Null,
        Some(GitActionStep::Toast(toast)) => {
            let mut value =
                json!({ "kind": "toast", "level": toast.level.as_str(), "title": toast.title });
            if let Some(description) = toast.description {
                value["description"] = json!(description);
            }
            value
        }
        Some(GitActionStep::PromptWorkflow { title, prompt }) => {
            json!({ "kind": "promptWorkflow", "title": title, "prompt": prompt })
        }
        Some(GitActionStep::Review(action)) => {
            json!({ "kind": "review", "action": action.selector() })
        }
        Some(GitActionStep::OpenExistingPullRequest) => {
            json!({ "kind": "openExistingPullRequest" })
        }
        Some(GitActionStep::PullRequestAgentWorkflow) => {
            json!({ "kind": "pullRequestAgentWorkflow" })
        }
        Some(GitActionStep::Mutation {
            mutation,
            started,
            finished,
        }) => json!({
            "kind": "mutation",
            "mutation": match mutation { GitMutation::Sync => "sync", GitMutation::Push => "push" },
            "started": started,
            "finished": finished,
        }),
    }
}

fn review_action(value: &Value) -> GitReviewAction {
    match value.as_str() {
        Some("push") => GitReviewAction::Push,
        Some("pr") => GitReviewAction::Pr,
        _ => GitReviewAction::Commit,
    }
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn answer(scenario: &Value) -> Value {
    let input = &scenario["input"];
    match text(&scenario["kind"]) {
        "menu" => {
            let state: GitState = serde_json::from_value(input.clone()).expect("state");
            serde_json::to_value(titlebar_menu(&state)).unwrap()
        }
        "plan" => {
            let state: GitState = serde_json::from_value(input["state"].clone()).expect("state");
            let action = GitAction::from_selector(text(&input["action"])).expect("action");
            let before = plan_git_action_before_read(action);
            if before.is_some() {
                return step_json(before);
            }
            step_json(plan_git_action_after_read(
                action,
                &state,
                input["isWorktree"].as_bool().unwrap_or(false),
                input["remote"].as_bool().unwrap_or(false),
            ))
        }
        "reviewDraft" => {
            let state: GitState = serde_json::from_value(input["state"].clone()).expect("state");
            review_modal_draft(ReviewDraftInput {
                action: review_action(&input["action"]),
                agent_id: input["agentId"].as_str(),
                state: &state,
                is_worktree: input["isWorktree"].as_bool().unwrap_or(false),
                remote: input["remote"].as_bool().unwrap_or(false),
                request_id: text(&input["requestId"]),
                worktree_name: input["worktreeName"].as_str(),
            })
        }
        "trusted" => {
            let files: Vec<GitChangedFile> =
                serde_json::from_value(input["files"].clone()).expect("files");
            let review = PendingGitReview {
                action: GitReviewAction::Commit,
                files,
                has_commit: true,
                project_id: String::new(),
                remote: None,
            };
            let paths: Option<Vec<String>> = input
                .get("filePaths")
                .filter(|paths| paths.is_array())
                .map(|paths| serde_json::from_value(paths.clone()).expect("paths"));
            match resolve_trusted_file_selection(&review, paths.as_deref()) {
                Ok(selection) => {
                    json!({ "explicit": selection.explicit, "filePaths": selection.file_paths })
                }
                Err(error) => json!({ "error": error }),
            }
        }
        "labels" => {
            let action = review_action(&input["action"]);
            let has_commit = input["hasCommit"].as_bool().unwrap_or(false);
            json!({
                "confirm": confirm_label(action, has_commit),
                "description": prompt_description(action),
                "started": started_title(action, has_commit),
                "finished": finished_title(action),
            })
        }
        "workflowTitle" => json!(agent_workflow_title(text(input))),
        "syncPrompt" => json!(sync_with_main_prompt()),
        "prPrompt" => {
            let files: Vec<String> =
                serde_json::from_value(input["selectedFiles"].clone()).unwrap_or_default();
            json!(pull_request_agent_prompt(
                input["hasExplicitFileSelection"].as_bool().unwrap_or(false),
                input["hasCommit"].as_bool().unwrap_or(false),
                text(&input["message"]),
                &files,
            ))
        }
        "mergePrompt" => json!(merge_conflict_prompt(
            text(&input["parentName"]),
            text(&input["branch"]),
            text(&input["worktreeName"]),
            text(&input["mergeOutput"]),
        )),
        "commitMessage" => {
            let (subject, body) = parse_commit_message(text(input));
            json!({ "body": body, "subject": subject })
        }
        "relativePath" => json!(normalize_relative_git_file_path(text(input))),
        "gitError" => json!(user_visible_git_error(
            text(&input["message"]),
            text(&input["fallback"])
        )),
        "shortStatus" => json!(short_status_has_changes(text(input))),
        "branchName" => json!(worktree_branch_name(
            input["current"].as_str(),
            input["fallback"].as_str()
        )),
        "folderSuffix" => json!(worktree_folder_suffix(
            text(&input["folder"]),
            text(&input["parent"])
        )),
        "managedBranch" => json!(is_managed_worktree_branch(input.as_str())),
        "renameError" => json!(worktree_rename_user_visible_error(text(input))),
        "worktreeError" => json!(worktree_user_visible_error(text(input))),
        "listError" => json!(worktree_list_error_text(
            text(&input["message"]),
            input["folder"].as_str()
        )),
        "projectPath" => json!(normalize_project_path(input.as_str())),
        "projectName" => json!(project_name_from_path(text(input))),
        "slug" => json!(worktree_slug_from_prompt(text(input))),
        "baseBranches" => normalize_worktree_base_branches(Some(input)),
        "existingWorktrees" => Value::Array(normalize_existing_worktree_options(Some(input))),
        "pollCycle" => {
            let targets = input
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|key| GitPollTarget {
                    key: key.to_string(),
                    scoped_project_id: key.to_string(),
                })
                .collect();
            let cycle = plan_git_poll_cycle(targets);
            json!({
                "cycleMs": cycle.cycle_ms,
                "probes": cycle.probes.iter().map(|(delay, target)| json!([delay, target.key])).collect::<Vec<_>>(),
            })
        }
        other => json!({ "unknownKind": other }),
    }
}

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .expect("usage: git_menu_parity <out-dir>");
    let scenarios: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(format!("{out_dir}/scenarios.json")).expect("scenarios.json"),
    )
    .expect("scenarios");
    let answers: Vec<Value> = scenarios.iter().map(answer).collect();
    std::fs::write(
        format!("{out_dir}/rust.json"),
        serde_json::to_string(&answers).unwrap(),
    )
    .expect("write rust.json");
    eprintln!("{} scenarios answered", answers.len());
}
