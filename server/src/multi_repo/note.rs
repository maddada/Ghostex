//! The lines an agent starting in a folder with several repos is told: which repos are there and
//! that each one commits on its own.

/// The note for an agent whose working folder is `cwd`, `None` for a plain single repo.
pub fn agent_note_for_cwd(cwd: &str) -> Option<String> {
    let set = super::multi_repo_set(cwd)?;
    let mut lines = Vec::new();
    let branch = set
        .session
        .as_ref()
        .map(|session| session.record.branch.clone());
    match &branch {
        Some(branch) => lines.push(format!(
            "This folder holds several git repositories, each its own worktree on the branch {branch}:"
        )),
        None => lines.push("This folder holds several git repositories:".to_string()),
    }
    for repo in &set.repos {
        let place = if repo.rel.is_empty() {
            "this folder".to_string()
        } else {
            format!("./{}", repo.rel)
        };
        lines.push(format!("- {} ({place})", repo.name));
    }
    lines.push(
        "Each repository has its own history: run git inside the repository you change, and commit each repository separately. Ghostex's Changes view shows and commits them all, one commit per repository."
            .to_string(),
    );
    if set.session.is_some() && !set.root_is_repo {
        lines.push(
            "Other files and folders here are links to the project folder, so changing them changes the original that other sessions share."
                .to_string(),
        );
    }
    Some(lines.join("\n"))
}
