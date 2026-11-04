//! A handoff to another project: which of another computer's projects a handed-off session should land in,
//! and what the dialog says when the two checkouts are on different branches.
//!
//! CDXC:TranscriptExport 2026-10-25 DECISION:
//! User (Q543): suggest. The project on the other computer whose git remote matches this session's
//! repository is preselected and listed first, and every other project there is still listed. A
//! folder-name match stands in only when the session's project has no `origin` to compare.
//! SEE-ALSO: apps/desktop/src/app/gx_store/git/export_transcript.rs (the caller),
//! apps/desktop/src/app/window/export_transcript_modal.rs (the Computer and Project selects).

/// One project on the target computer, as the presentation stream describes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandoffProjectCandidate {
    pub project_id: String,
    pub title: String,
    pub path: Option<String>,
    pub origin_url: Option<String>,
    pub is_worktree: bool,
}

/// A row of the dialog's Project select, in display order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffProjectChoice {
    pub project_id: String,
    pub title: String,
    pub path: Option<String>,
    pub same_repo: bool,
    /// The project the session is in: this project, here.
    pub is_own: bool,
}

/// The ranked rows and the one to preselect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandoffProjectRanking {
    pub choices: Vec<HandoffProjectChoice>,
    pub preferred_project_id: Option<String>,
}

/// One computer's rows for a handoff's destination, and the repository it lacks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandoffComputerRanking {
    pub ranking: HandoffProjectRanking,
    /// The repository's name when the session's project has a git remote that no project on this
    /// computer matches: the dialog then offers to clone it there.
    pub missing_repository: Option<String>,
}

/// Ranks a computer's projects for a session. `own_project` is the session's project when this is
/// the session's own computer: it comes first and is what the row starts on, and that computer
/// never lacks the repository, whatever its other projects hold.
///
/// CDXC:TranscriptExport 2026-10-31 DECISION:
/// User: Handoff to an agent "needs to get all the options" of the other card, "allow either handing off in the current project and current machine and all that (default) or handing off to another machine/another project". So this project is a choice like any other, the first one here.
pub fn rank_handoff_computer(
    origin_url: Option<&str>,
    source_path: Option<&str>,
    candidates: &[HandoffProjectCandidate],
    own_project: Option<&str>,
) -> HandoffComputerRanking {
    let mut ranking = rank_handoff_projects(origin_url, source_path, candidates);
    let missing_repository = match (&ranking.preferred_project_id, own_project) {
        (None, None) => origin_url.and_then(repository_display_name),
        _ => None,
    };
    if let Some(index) = own_project.and_then(|own| {
        ranking
            .choices
            .iter()
            .position(|choice| choice.project_id == own)
    }) {
        let mut own = ranking.choices.remove(index);
        own.is_own = true;
        ranking.preferred_project_id = Some(own.project_id.clone());
        ranking.choices.insert(0, own);
    }
    HandoffComputerRanking {
        ranking,
        missing_repository,
    }
}

/// A git remote URL reduced to `host/owner/repo`, lowercased, so the https, ssh and scp forms of
/// one repository compare equal.
pub fn repository_key(url: &str) -> Option<String> {
    let mut rest = url.trim();
    if rest.is_empty() {
        return None;
    }
    if let Some((_, after)) = rest.split_once("://") {
        rest = after;
    } else if let Some((host, path)) = rest.split_once(':') {
        if !host.contains('/') {
            // scp form: `git@github.com:owner/repo.git`.
            let host = host.rsplit('@').next().unwrap_or(host);
            return normalize_host_and_path(host, path);
        }
    }
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority.rsplit('@').next().unwrap_or(authority);
    // `ssh://git@host:22/owner/repo`: the port is not part of the repository.
    let host = host.split(':').next().unwrap_or(host);
    normalize_host_and_path(host, path)
}

fn normalize_host_and_path(host: &str, path: &str) -> Option<String> {
    let host = host.trim().trim_start_matches("www.").to_lowercase();
    let path = path.trim().trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path).trim_matches('/');
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("{host}/{}", path.to_lowercase()))
}

/// The repository's own name (`ghostex` for `git@github.com:maddada/ghostex.git`).
pub fn repository_display_name(url: &str) -> Option<String> {
    let key = repository_key(url)?;
    key.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn folder_name(path: &str) -> Option<String> {
    path.trim()
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_lowercase)
}

/// Orders the target computer's projects for the Project select: the same repository first (a
/// project's main checkout before its worktrees), then the rest in the order given.
pub fn rank_handoff_projects(
    source_origin_url: Option<&str>,
    source_path: Option<&str>,
    candidates: &[HandoffProjectCandidate],
) -> HandoffProjectRanking {
    let source_key = source_origin_url.and_then(repository_key);
    let source_folder = source_path.and_then(folder_name);
    let same_repo = |candidate: &HandoffProjectCandidate| match &source_key {
        Some(key) => candidate
            .origin_url
            .as_deref()
            .and_then(repository_key)
            .is_some_and(|candidate_key| &candidate_key == key),
        None => source_folder.as_ref().is_some_and(|source| {
            candidate.path.as_deref().and_then(folder_name).as_ref() == Some(source)
        }),
    };
    let mut rows: Vec<(u8, usize, HandoffProjectChoice)> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let matched = same_repo(candidate);
            let rank = match (matched, candidate.is_worktree) {
                (true, false) => 0,
                (true, true) => 1,
                (false, _) => 2,
            };
            (
                rank,
                index,
                HandoffProjectChoice {
                    project_id: candidate.project_id.clone(),
                    title: candidate.title.clone(),
                    path: candidate.path.clone(),
                    same_repo: matched,
                    is_own: false,
                },
            )
        })
        .collect();
    rows.sort_by_key(|(rank, index, _)| (*rank, *index));
    let choices: Vec<HandoffProjectChoice> = rows.into_iter().map(|(_, _, row)| row).collect();
    let preferred_project_id = choices
        .first()
        .filter(|choice| choice.same_repo)
        .map(|choice| choice.project_id.clone());
    HandoffProjectRanking {
        choices,
        preferred_project_id,
    }
}

/// The button of a handoff going somewhere else: it names where the session goes (Q862), with the
/// computer when it leaves the session's own.
pub fn handoff_destination_label(
    project_title: &str,
    computer_label: &str,
    is_source: bool,
) -> String {
    if is_source {
        format!("Handoff to {project_title}")
    } else {
        format!("Handoff to {project_title} on {computer_label}")
    }
}

/// A Project select row: the title, its mark, and the folder.
pub fn handoff_project_label(
    title: &str,
    is_own: bool,
    same_repo: bool,
    path: Option<&str>,
) -> String {
    let mut label = title.to_string();
    if is_own {
        label.push_str(" (this project)");
    } else if same_repo {
        label.push_str(" (same repo)");
    }
    if let Some(path) = path.filter(|path| !path.is_empty()) {
        label.push_str(" · ");
        label.push_str(path);
    }
    label
}

/// The first line of the branch notice, or `None` when there is nothing to warn about (same
/// branch, or either side's branch unknown).
///
/// CDXC:TranscriptExport 2026-10-25 DECISION:
/// User (Q545): conversation only, with a notice titled "Please note the difference in branches" whose second line is "We're just handing off the session, not switching the branch on the remote machine". This is its first line: what the difference is.
pub fn branch_difference_line(
    source_branch: Option<&str>,
    target_branch: Option<&str>,
    target_label: &str,
) -> Option<String> {
    let source = source_branch.map(str::trim).filter(|b| !b.is_empty())?;
    let target = target_branch.map(str::trim).filter(|b| !b.is_empty())?;
    (source != target).then(|| {
        format!("This session is on {source}; the project on {target_label} is on {target}.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        id: &str,
        path: &str,
        origin: Option<&str>,
        worktree: bool,
    ) -> HandoffProjectCandidate {
        HandoffProjectCandidate {
            project_id: id.to_string(),
            title: id.to_string(),
            path: Some(path.to_string()),
            origin_url: origin.map(str::to_string),
            is_worktree: worktree,
        }
    }

    #[test]
    fn https_ssh_and_scp_forms_are_one_repository() {
        let key = Some("github.com/maddada/ghostex".to_string());
        assert_eq!(
            repository_key("https://github.com/maddada/Ghostex.git"),
            key
        );
        assert_eq!(repository_key("git@github.com:maddada/ghostex.git"), key);
        assert_eq!(
            repository_key("ssh://git@github.com:22/maddada/ghostex/"),
            key
        );
        assert_eq!(
            repository_key("https://user@www.github.com/maddada/ghostex"),
            key
        );
        assert_eq!(repository_key(""), None);
        assert_eq!(
            repository_display_name("git@github.com:maddada/ghostex.git").as_deref(),
            Some("ghostex")
        );
    }

    #[test]
    fn same_repository_is_first_and_preselected_main_checkout_before_worktree() {
        let ranking = rank_handoff_projects(
            Some("git@github.com:maddada/ghostex.git"),
            Some("/Users/me/dev/Ghostex"),
            &[
                candidate(
                    "site",
                    "~/sites/site",
                    Some("https://github.com/me/site"),
                    false,
                ),
                candidate(
                    "wt",
                    "~/wt/x",
                    Some("https://github.com/maddada/ghostex.git"),
                    true,
                ),
                candidate(
                    "main",
                    "~/dev/Ghostex",
                    Some("https://github.com/maddada/ghostex"),
                    false,
                ),
            ],
        );
        let order: Vec<&str> = ranking
            .choices
            .iter()
            .map(|c| c.project_id.as_str())
            .collect();
        assert_eq!(order, ["main", "wt", "site"]);
        assert_eq!(ranking.preferred_project_id.as_deref(), Some("main"));
    }

    #[test]
    fn no_match_lists_everything_and_preselects_nothing() {
        let ranking = rank_handoff_projects(
            Some("https://github.com/maddada/ghostex"),
            None,
            &[
                candidate("a", "~/a", None, false),
                candidate("b", "~/b", Some("https://gitlab.com/x/y"), false),
            ],
        );
        assert_eq!(ranking.choices.len(), 2);
        assert!(ranking.choices.iter().all(|choice| !choice.same_repo));
        assert_eq!(ranking.preferred_project_id, None);
    }

    #[test]
    fn folder_name_matches_only_without_an_origin() {
        let ranking = rank_handoff_projects(
            None,
            Some("/Users/me/dev/Ghostex/"),
            &[
                candidate("x", "/home/me/x", None, false),
                candidate("g", "/home/me/code/ghostex", None, false),
            ],
        );
        assert_eq!(ranking.preferred_project_id.as_deref(), Some("g"));
    }

    #[test]
    fn branch_notice_only_when_both_branches_known_and_different() {
        assert_eq!(
            branch_difference_line(Some("feature-x"), Some("main"), "studio-mac").as_deref(),
            Some("This session is on feature-x; the project on studio-mac is on main.")
        );
        assert_eq!(
            branch_difference_line(Some("main"), Some("main"), "m"),
            None
        );
        assert_eq!(branch_difference_line(None, Some("main"), "m"), None);
        assert_eq!(branch_difference_line(Some("main"), Some(" "), "m"), None);
    }

    #[test]
    fn the_sessions_own_computer_starts_on_this_project() {
        let origin = Some("git@github.com:o/ghostex.git");
        let candidates = [
            candidate(
                "acme",
                "/dev/acme",
                Some("git@github.com:o/acme.git"),
                false,
            ),
            candidate("docs", "/dev/docs", None, false),
            candidate("own", "/dev/ghostex", origin, false),
        ];
        let own = rank_handoff_computer(origin, Some("/dev/ghostex"), &candidates, Some("own"));
        let ids: Vec<&str> = own
            .ranking
            .choices
            .iter()
            .map(|choice| choice.project_id.as_str())
            .collect();
        assert_eq!(ids, ["own", "acme", "docs"]);
        assert!(own.ranking.choices[0].is_own);
        assert_eq!(own.ranking.preferred_project_id.as_deref(), Some("own"));
        // No offer to clone a repository this computer already has.
        assert_eq!(own.missing_repository, None);
    }

    #[test]
    fn a_second_checkout_on_the_sessions_own_computer_comes_after_this_project() {
        let origin = Some("git@github.com:o/ghostex.git");
        let candidates = [
            candidate("own", "/dev/ghostex", origin, false),
            candidate("other", "/dev/other-ghostex", origin, false),
        ];
        let own = rank_handoff_computer(origin, Some("/dev/ghostex"), &candidates, Some("own"));
        assert_eq!(own.ranking.preferred_project_id.as_deref(), Some("own"));
        assert_eq!(own.ranking.choices[1].project_id, "other");
        assert!(own.ranking.choices[1].same_repo);
    }

    #[test]
    fn another_computer_keeps_every_project_and_can_lack_the_repository() {
        let origin = Some("git@github.com:o/ghostex.git");
        let candidates = [candidate("notes", "/n/notes", None, false)];
        let other = rank_handoff_computer(origin, Some("/dev/ghostex"), &candidates, None);
        assert_eq!(other.ranking.choices.len(), 1);
        assert_eq!(other.missing_repository.as_deref(), Some("ghostex"));
    }
}
