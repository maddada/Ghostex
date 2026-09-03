use serde_json::{json, Map, Value};

use super::project_setup::count_project_file_lines;
use super::values::{
    display_unknown_value, normalize_git_commit_message, normalize_git_ref,
    normalize_git_remote_name, normalize_relative_file_path, optional_relative_file_path,
    optional_relative_file_paths, required_relative_file_paths, run_process_command, typed_result,
    CommandSummary, ProcessCommand, TypedOperationContext, TypedOperationError,
};
use super::worktree::parse_git_branch_list;

pub(crate) async fn run_git_action(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = normalize_git_action(params.get("action"))?;
    if action == "countFileLines" {
        let file_paths = optional_relative_file_paths(params.get("filePaths"))?;
        let line_count = count_project_file_lines(&context.cwd, &file_paths)?;
        return Ok(json!({
            "action": action,
            "exitCode": 0,
            "stderr": "",
            "stdout": line_count.to_string(),
        }));
    }
    let command = build_git_command(&action, params, &context.cwd)?;
    let output = run_process_command(&command, context).await?;
    let mut result = typed_result(&action, &command, output);
    if action == "listBranches" && result.get("exitCode").and_then(Value::as_i64) == Some(0) {
        let stdout = result
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        result.as_object_mut().expect("result object").insert(
            "branches".to_string(),
            Value::Array(parse_git_branch_list(&stdout)),
        );
    }
    Ok(result)
}

pub(crate) fn build_git_command(
    action: &str,
    params: &Map<String, Value>,
    cwd: &str,
) -> Result<ProcessCommand, TypedOperationError> {
    let command = match action {
        "addAll" => {
            let files = optional_relative_file_paths(params.get("filePaths"))?;
            let mut args = vec!["add".to_string(), "-A".to_string()];
            let result_command = if files.is_empty() {
                None
            } else {
                args.push("--".to_string());
                args.extend(files.clone());
                Some(CommandSummary {
                    args: vec![
                        "add".to_string(),
                        "-A".to_string(),
                        "--".to_string(),
                        format!("<{} files>", files.len()),
                    ],
                    cwd: cwd.to_string(),
                    executable: "git".to_string(),
                })
            };
            let command = ProcessCommand::new("git", args, cwd).with_result_command(result_command);
            if files.is_empty() {
                command
            } else {
                command.with_env("GIT_LITERAL_PATHSPECS", "1")
            }
        }
        "branch" => ProcessCommand::new("git", vec!["branch", "--show-current"], cwd),
        "checkout" => ProcessCommand::new(
            "git",
            vec![
                "checkout".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        "checkoutNewBranch" => ProcessCommand::new(
            "git",
            vec![
                "checkout".to_string(),
                "-b".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        "deleteLocalBranch" => {
            let branch = normalize_git_ref(params.get("branch"), "branch")?;
            ProcessCommand::new(
                "git",
                vec![
                    "branch".to_string(),
                    "-d".to_string(),
                    "--".to_string(),
                    branch,
                ],
                cwd,
            )
            .with_result_args(vec!["branch", "-d", "--", "<branch>"])
        }
        /*
        CDXC:Worktrees 2026-07-29-00:00:
        Rolling back a half-created worktree session, and force-removing a
        worktree the user chose to discard, both have to delete a branch that
        `git branch -d` refuses because it is unmerged — which is exactly the
        state a discarded attempt is in. Kept as its own allowlisted action so
        the safe delete stays the default everywhere else.
        */
        "deleteLocalBranchForce" => {
            let branch = normalize_git_ref(params.get("branch"), "branch")?;
            ProcessCommand::new(
                "git",
                vec![
                    "branch".to_string(),
                    "-D".to_string(),
                    "--".to_string(),
                    branch,
                ],
                cwd,
            )
            .with_result_args(vec!["branch", "-D", "--", "<branch>"])
        }
        "deleteRemoteBranch" => {
            let remote = normalize_git_remote_name(params.get("remoteName"))?;
            let branch = normalize_git_ref(params.get("branch"), "branch")?;
            ProcessCommand::new(
                "git",
                vec!["push".to_string(), remote, "--delete".to_string(), branch],
                cwd,
            )
            .with_result_args(vec!["push", "<remote>", "--delete", "<branch>"])
        }
        "diff" => {
            let mut args = vec!["diff".to_string(), "--".to_string()];
            args.extend(optional_relative_file_path(params.get("filePath"))?);
            ProcessCommand::new("git", args, cwd)
        }
        "diffCached" => ProcessCommand::new("git", vec!["diff", "--cached"], cwd),
        /*
        CDXC:Git 2026-06-24-16:11:
        GPUI blank commit-message generation needs staged diff text for exactly
        the review-approved file set. Keep this as an allowlisted cached-diff
        action with path validation and redacted file-count command metadata
        instead of exposing a free-form git command or logging file paths.
        */
        "diffCachedFiles" => {
            let files = required_relative_file_paths(params.get("filePaths"))?;
            let mut args = vec![
                "diff".to_string(),
                "--cached".to_string(),
                "--no-ext-diff".to_string(),
                "--".to_string(),
            ];
            args.extend(files.clone());
            ProcessCommand::new("git", args, cwd)
                .with_result_command(Some(CommandSummary {
                    args: vec![
                        "diff".to_string(),
                        "--cached".to_string(),
                        "--no-ext-diff".to_string(),
                        "--".to_string(),
                        format!("<{} files>", files.len()),
                    ],
                    cwd: cwd.to_string(),
                    executable: "git".to_string(),
                }))
                .with_env("GIT_LITERAL_PATHSPECS", "1")
        }
        "diffCachedNoExt" => ProcessCommand::new(
            "git",
            vec![
                "diff".to_string(),
                "--cached".to_string(),
                "--no-ext-diff".to_string(),
                "--".to_string(),
                normalize_relative_file_path(params.get("filePath"))?,
            ],
            cwd,
        ),
        "diffCachedStat" => ProcessCommand::new("git", vec!["diff", "--cached", "--stat"], cwd),
        "diffCachedStatFiles" => {
            let files = required_relative_file_paths(params.get("filePaths"))?;
            let mut args = vec![
                "diff".to_string(),
                "--cached".to_string(),
                "--stat".to_string(),
                "--".to_string(),
            ];
            args.extend(files.clone());
            ProcessCommand::new("git", args, cwd)
                .with_result_command(Some(CommandSummary {
                    args: vec![
                        "diff".to_string(),
                        "--cached".to_string(),
                        "--stat".to_string(),
                        "--".to_string(),
                        format!("<{} files>", files.len()),
                    ],
                    cwd: cwd.to_string(),
                    executable: "git".to_string(),
                }))
                .with_env("GIT_LITERAL_PATHSPECS", "1")
        }
        "diffNoExt" => ProcessCommand::new(
            "git",
            vec![
                "diff".to_string(),
                "--no-ext-diff".to_string(),
                "--".to_string(),
                normalize_relative_file_path(params.get("filePath"))?,
            ],
            cwd,
        ),
        "diffNoIndexAgainstNull" => ProcessCommand::new(
            "git",
            vec![
                "diff".to_string(),
                "--no-index".to_string(),
                "--no-ext-diff".to_string(),
                "--".to_string(),
                "/dev/null".to_string(),
                normalize_relative_file_path(params.get("filePath"))?,
            ],
            cwd,
        ),
        "diffNumstat" => ProcessCommand::new("git", vec!["diff", "--numstat", "HEAD"], cwd),
        "getOriginRemoteUrl" => {
            ProcessCommand::new("git", vec!["remote", "get-url", "origin"], cwd)
        }
        "isInsideWorkTree" => {
            ProcessCommand::new("git", vec!["rev-parse", "--is-inside-work-tree"], cwd)
        }
        "isUntrackedFile" => ProcessCommand::new(
            "git",
            vec![
                "ls-files".to_string(),
                "--others".to_string(),
                "--exclude-standard".to_string(),
                "--".to_string(),
                normalize_relative_file_path(params.get("filePath"))?,
            ],
            cwd,
        ),
        "list" => ProcessCommand::new(
            "git",
            vec![
                "ls-files",
                "--cached",
                "--modified",
                "--others",
                "--exclude-standard",
            ],
            cwd,
        ),
        "listBranches" => {
            /*
            CDXC:Worktrees 2026-06-24-11:32:
            Add Worktree needs an explicit base-branch picker. Keep branch
            discovery inside the gxserver typed Git boundary, include local and
            remote-tracking refs, and parse structured metadata server-side so
            UI clients do not shell out or parse raw Git output.
            */
            ProcessCommand::new(
                "git",
                vec![
                    "for-each-ref",
                    "--format=%(refname:short)%09%(refname)%09%(HEAD)",
                    "refs/heads",
                    "refs/remotes",
                ],
                cwd,
            )
        }
        "listRemotes" => ProcessCommand::new("git", vec!["remote"], cwd),
        "listUntracked" => ProcessCommand::new(
            "git",
            vec!["ls-files", "--others", "--exclude-standard", "-z"],
            cwd,
        ),
        "status" => ProcessCommand::new("git", vec!["status", "--short", "--branch"], cwd),
        "statusPorcelain" => ProcessCommand::new("git", vec!["status", "--porcelain"], cwd)
            .with_preserved_stdout_whitespace(),
        "statusPorcelainZ" => ProcessCommand::new("git", vec!["status", "--porcelain", "-z"], cwd)
            .with_preserved_stdout_whitespace(),
        "upstreamCounts" => ProcessCommand::new(
            "git",
            vec!["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
            cwd,
        ),
        "merge" => ProcessCommand::new(
            "git",
            vec![
                "merge".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        "commit" => {
            let message = normalize_git_commit_message(
                params.get("messageSubject"),
                params.get("messageBody"),
            )?;
            let mut args = vec!["commit".to_string()];
            if params.get("noVerify").and_then(Value::as_bool) == Some(true) {
                args.push("--no-verify".to_string());
            }
            args.extend(["-F".to_string(), "-".to_string()]);
            let result_args = args
                .iter()
                .map(|arg| {
                    if arg == "-" {
                        "<stdin>".to_string()
                    } else {
                        arg.clone()
                    }
                })
                .collect();
            ProcessCommand::new("git", args, cwd)
                .with_result_command(Some(CommandSummary {
                    args: result_args,
                    cwd: cwd.to_string(),
                    executable: "git".to_string(),
                }))
                .with_stdin(message)
        }
        "pullFastForward" => {
            /*
            CDXC:Git 2026-06-19-14:38:
            The titlebar remote-sync workflow must update the current branch only through Git's fast-forward pull contract. Rust keeps the typed operation to `git pull --ff-only` so merge, rebase, dirty-worktree, and divergent-history failures remain visible to callers instead of being hidden by fallback behavior.
            */
            ProcessCommand::new("git", vec!["pull", "--ff-only"], cwd)
        }
        "push" => ProcessCommand::new("git", vec!["push"], cwd),
        /*
        CDXC:Git 2026-06-24-17:47:
        Remote GPUI push parity must not send renderer-observed branch names as mutation authority. Push the current HEAD to origin with upstream tracking so gxserver/Git derive the branch from the checked-out repository state.
        */
        "pushSetUpstreamCurrent" => ProcessCommand::new(
            "git",
            vec![
                "push".to_string(),
                "-u".to_string(),
                "origin".to_string(),
                "HEAD".to_string(),
            ],
            cwd,
        ),
        "pushSetUpstream" => ProcessCommand::new(
            "git",
            vec![
                "push".to_string(),
                "-u".to_string(),
                "origin".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        "remoteBranchExists" => {
            let remote = normalize_git_remote_name(params.get("remoteName"))?;
            let branch = normalize_git_ref(params.get("branch"), "branch")?;
            ProcessCommand::new(
                "git",
                vec![
                    "ls-remote".to_string(),
                    "--exit-code".to_string(),
                    "--heads".to_string(),
                    remote,
                    branch,
                ],
                cwd,
            )
            .with_result_args(vec![
                "ls-remote",
                "--exit-code",
                "--heads",
                "<remote>",
                "<branch>",
            ])
        }
        "verifyRef" => ProcessCommand::new(
            "git",
            vec![
                "rev-parse".to_string(),
                "--verify".to_string(),
                normalize_git_ref(params.get("ref"), "ref")?,
            ],
            cwd,
        ),
        _ => {
            return Err(TypedOperationError::bad_request(format!(
                "Unsupported Git action: {action}"
            )))
        }
    };
    Ok(command)
}

pub(crate) fn normalize_git_action(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let action = input.and_then(Value::as_str).unwrap_or("undefined");
    match action {
        "addAll"
        | "branch"
        | "commit"
        | "countFileLines"
        | "checkout"
        | "checkoutNewBranch"
        | "deleteLocalBranch"
        | "deleteLocalBranchForce"
        | "deleteRemoteBranch"
        | "diff"
        | "diffCached"
        | "diffCachedFiles"
        | "diffCachedStatFiles"
        | "diffCachedNoExt"
        | "diffCachedStat"
        | "diffNoExt"
        | "diffNoIndexAgainstNull"
        | "diffNumstat"
        | "getOriginRemoteUrl"
        | "isInsideWorkTree"
        | "isUntrackedFile"
        | "list"
        | "listBranches"
        | "listRemotes"
        | "listUntracked"
        | "merge"
        | "pullFastForward"
        | "push"
        | "pushSetUpstreamCurrent"
        | "pushSetUpstream"
        | "remoteBranchExists"
        | "status"
        | "statusPorcelain"
        | "statusPorcelainZ"
        | "upstreamCounts"
        | "verifyRef" => Ok(action.to_string()),
        _ => Err(TypedOperationError::bad_request(format!(
            "Unsupported Git action: {}",
            display_unknown_value(input)
        ))),
    }
}
