//! The words the Git flows show and the prompts their agent workflows start with, ported verbatim
//! from the old runtime (`helpers/git.ts`, `constants.ts`).

use super::state::GitReviewAction;

/// The prompt agent used when neither the dialog nor the settings name one.
pub const DEFAULT_PROMPT_AGENT_ID: &str = "codex";

pub const GIT_MULTIPLE_COMMITS_PROMPT: &str =
    "Please review my current changes and commit them as multiple focused commits.

Commit-splitting rules:
- Group changes by related feature, fix, or topic.
- Do not combine unrelated work in the same commit.
- Use file-based splitting only; do not split individual hunks.
- Make each commit easy to revert or cherry-pick later.
- Use clear, concise commit messages.";

pub const REMOTE_MERGE_CONFLICT_PROMPT: &str = "A direct merge into main has conflicts in this remote project. Inspect the repository state, resolve the conflicts, and commit the merge when it is correct.";

const GIT_RELEASE_STEPS_PROMPT: &str = "1. Push any local commits to remote.
2. Review the commits since the last released version.
3. Update CHANGELOG.md to mention the new changes.
4. Publish the next minor version to the usual places we publish this app.";

/// `GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT`.
pub fn multicommit_release_prompt() -> String {
    format!(
        "{GIT_MULTIPLE_COMMITS_PROMPT}\n\nAfter all focused commits are created:\n{GIT_RELEASE_STEPS_PROMPT}"
    )
}

/// `GPUI_GIT_RELEASE_ONLY_PROMPT`.
pub fn release_only_prompt() -> String {
    format!(
        "Please release this app using the usual release workflow.\n\n{GIT_RELEASE_STEPS_PROMPT}"
    )
}

/// `resolveGpuiSidebarGitConfirmLabel`.
pub fn confirm_label(action: GitReviewAction, has_commit: bool) -> &'static str {
    match action {
        GitReviewAction::Commit => "Commit",
        GitReviewAction::Push if has_commit => "Commit & Push",
        GitReviewAction::Push => "Push",
        GitReviewAction::Pr if has_commit => "Commit, Push & PR",
        GitReviewAction::Pr => "Push & Create PR",
    }
}

/// `resolveGpuiSidebarGitPromptDescription`.
pub fn prompt_description(action: GitReviewAction) -> &'static str {
    match action {
        GitReviewAction::Commit => "Review and commit changes.",
        GitReviewAction::Push => "Push the current branch.",
        GitReviewAction::Pr => "Create or open a pull request.",
    }
}

/// `resolveGpuiSidebarGitStartedTitle`.
pub fn started_title(action: GitReviewAction, has_commit: bool) -> &'static str {
    match action {
        GitReviewAction::Pr if has_commit => "Committing, pushing, and creating PR",
        GitReviewAction::Pr => "Pushing and creating PR",
        GitReviewAction::Push if has_commit => "Committing and pushing",
        GitReviewAction::Push => "Pushing",
        GitReviewAction::Commit => "Committing",
    }
}

/// `resolveGpuiSidebarGitFinishedTitle`.
pub fn finished_title(action: GitReviewAction) -> &'static str {
    match action {
        GitReviewAction::Pr => "Pull request ready",
        GitReviewAction::Push => "Push complete",
        GitReviewAction::Commit => "Commit complete",
    }
}

/// `formatGpuiGitAgentWorkflowTitle`.
pub fn agent_workflow_title(title: &str) -> String {
    let title = title.trim();
    if title.starts_with("Git:") {
        title.to_string()
    } else {
        format!("Git: {title}")
    }
}

/// `buildGpuiGitSyncWithMainPrompt`.
pub fn sync_with_main_prompt() -> String {
    [
        "Please sync the latest main branch changes into this worktree so it can be merged back to main afterward.",
        "",
        "Use the current repository and branch in this terminal. Inspect Git state directly before changing anything.",
        "",
        "Requirements:",
        "- Fetch the latest remote refs before syncing.",
        "- Bring main into this worktree branch using the safest normal project workflow for this repository, such as merge or rebase only if that is clearly the repo convention.",
        "- Preserve work from both main and this worktree. If conflicts happen, resolve them without dropping code, behavior, or UX from either side.",
        "- After resolving conflicts, run the relevant checks you can run locally.",
        "- Leave the worktree branch ready for the user to merge back into main.",
        "- Stop and explain clearly if the repository state is unsafe or if a decision is needed.",
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
}

/// `buildGpuiGitPullRequestAgentPrompt`.
pub fn pull_request_agent_prompt(
    has_explicit_file_selection: bool,
    has_commit: bool,
    message: &str,
    selected_files: &[String],
) -> String {
    let selected: Vec<&String> = selected_files
        .iter()
        .filter(|path| !path.trim().is_empty())
        .collect();
    let commit_step = if has_commit {
        if has_explicit_file_selection {
            "- Stage and commit only the selected files listed below. Do not stage excluded files."
        } else {
            "- Stage and commit all new/modified files."
        }
    } else {
        "- There were no working tree changes when the modal opened, so skip committing unless you find new user changes."
    };
    let message_step = if message.is_empty() {
        "- Write a concise commit message that matches the staged diff."
    } else {
        "- Use the requested commit message below unless it is clearly invalid for the actual diff."
    };
    let files = if has_explicit_file_selection && !selected.is_empty() {
        let mut lines = vec!["Selected files:".to_string()];
        lines.extend(selected.iter().map(|path| format!("- {path}")));
        lines.join("\n")
    } else {
        "Selected files: all new/modified files.".to_string()
    };
    let requested = if message.is_empty() {
        String::new()
    } else {
        format!("\nRequested commit message:\n{message}")
    };
    [
        "Please complete the Git pull request flow in this terminal.".to_string(),
        String::new(),
        "Use the current repository checkout in this terminal. Inspect branch, remote, and PR state directly before changing anything.".to_string(),
        String::new(),
        "Do these steps visibly:".to_string(),
        commit_step.to_string(),
        message_step.to_string(),
        "- If you encounter conflicts, rebases, merge state, or divergent local/remote changes, make sure not to lose changes from either side.".to_string(),
        "- Push the current branch to origin, setting upstream if needed.".to_string(),
        "- Create a GitHub pull request with `gh pr create --fill`, or open/show the existing PR if one already exists.".to_string(),
        "- Stop and explain clearly if a command fails, authentication is missing, or a merge/rebase/conflict situation needs the user's decision.".to_string(),
        String::new(),
        files,
        requested,
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
}

/// `buildGpuiMergeConflictPrompt`.
pub fn merge_conflict_prompt(
    parent_name: &str,
    branch: &str,
    worktree_name: &str,
    merge_output: &str,
) -> String {
    let output = merge_output.trim();
    let output_block = if output.is_empty() {
        String::new()
    } else {
        format!("\nMerge output:\n{output}")
    };
    [
        "Please handle the current Git merge conflicts on the main branch.".to_string(),
        String::new(),
        format!("Target project: {parent_name}"),
        "Target branch: main".to_string(),
        format!("Merged worktree branch: {branch}"),
        format!("Worktree: {worktree_name}"),
        String::new(),
        "Resolve the conflicts without losing any code, behavior, or UX from either side.".to_string(),
        "Inspect the conflict markers, preserve the important intent from main and the worktree branch, run the relevant checks you can run locally, stage the resolved files, and leave the final state ready for review.".to_string(),
        output_block,
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
}

/// `gpuiUserVisibleGitErrorMessage`.
///
/// CDXC:Git 2026-07-11-05:08:
/// The gxserver client already converts daemon failures into bounded, user-facing messages.
/// Preserve those messages at the Git mutation boundary so stale reviews, unavailable agents, and
/// generation failures do not collapse into an unactionable generic toast.
pub fn user_visible_git_error(message: &str, fallback: &str) -> String {
    let mut cleaned = String::new();
    let mut in_space = false;
    for ch in message.chars() {
        let control = (ch as u32) <= 0x1f || (0x7f..=0x9f).contains(&(ch as u32));
        if control || ch.is_whitespace() {
            if !in_space {
                cleaned.push(' ');
                in_space = true;
            }
        } else {
            cleaned.push(ch);
            in_space = false;
        }
    }
    let trimmed: String = cleaned.trim().chars().take(500).collect();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}
