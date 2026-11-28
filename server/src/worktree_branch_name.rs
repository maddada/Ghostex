//! The name of a new chat's worktree: the first ticket, issue or PR its first message names, then
//! words, by the one rule in `ghostex_gx_protocol::worktree_name`.
//!
//! The references are read the way session links read them (`session_links::reference`: GitHub
//! and Linear URLs, `owner/repo#123`, `#123`, Linear IDs), and a title comes only from what Work
//! mode's background pass already cached. Sending never waits on a lookup: an unknown title just
//! leaves the message's own words after the key.
//!
//! SEE-ALSO: server/src/server/worktree_ops/draft_worktree.rs (the tray's endpoint),
//! packages/gx-protocol/src/worktree_name.rs (the rule and its tests).

use ghostex_gx_protocol::worktree_name::{worktree_name_for_message, WorktreeNameReference};

use crate::session_links::{normalize_repo, parse_github_url, parse_linear_url, SessionLinkRef};

/// What the message's reference looks up its title in.
pub(crate) struct ReferenceScope<'a> {
    /// The project whose Linear key the background pass recorded.
    pub(crate) project_id: &'a str,
    /// The checkout `gh` reads from, for `#123` and cached PRs and issues.
    pub(crate) cwd: &'a str,
}

/// One reference found in a message, with the text that named it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FoundReference {
    pub(crate) link: SessionLinkRef,
    pub(crate) matched: String,
}

/// Characters around a word that are punctuation, not part of a reference (`(SPX-12)`, `#4,`).
fn trim_word(word: &str) -> &str {
    word.trim_matches(|c: char| {
        matches!(
            c,
            '(' | ')'
                | '['
                | ']'
                | '<'
                | '>'
                | '{'
                | '}'
                | '"'
                | '\''
                | '`'
                | ','
                | '.'
                | ';'
                | ':'
                | '!'
                | '?'
                | '*'
                | '_'
        )
    })
}

/// A Linear ID word: `SPX-123`. Lowercase spellings count only for a team key the project's Linear
/// key is known to see, so `utf-8` and `gpt-5` never name a ticket.
fn linear_identifier(word: &str, team_keys: &[String]) -> Option<String> {
    let (key, number) = word.split_once('-')?;
    let shaped = (1..=10).contains(&key.len())
        && key.starts_with(|c: char| c.is_ascii_alphabetic())
        && key.chars().all(|c| c.is_ascii_alphanumeric())
        && (1..=7).contains(&number.len())
        && number.chars().all(|c| c.is_ascii_digit());
    if !shaped {
        return None;
    }
    let known_team = team_keys.iter().any(|team| team.eq_ignore_ascii_case(key));
    let uppercase = key.chars().all(|c| !c.is_ascii_lowercase());
    // With known team keys only theirs count; without any, an uppercase key does.
    let counts = if team_keys.is_empty() {
        uppercase
    } else {
        known_team
    };
    counts.then(|| format!("{}-{number}", key.to_ascii_uppercase()))
}

/// The first reference in `message`, in reading order. `#123` needs the checkout's repo
/// (`session_repo`); a bare number never counts.
pub(crate) fn first_reference_in_message(
    message: &str,
    session_repo: Option<&str>,
    team_keys: &[String],
) -> Option<FoundReference> {
    for raw in message.split_whitespace() {
        let word = trim_word(raw);
        if word.is_empty() {
            continue;
        }
        let link = parse_github_url(word)
            .or_else(|| parse_linear_url(word))
            .or_else(|| {
                let (repo, number) = word.split_once('#')?;
                let number = number.parse::<u64>().ok().filter(|number| *number > 0)?;
                let repo = if repo.is_empty() {
                    session_repo?.to_ascii_lowercase()
                } else {
                    normalize_repo(repo)?
                };
                Some(SessionLinkRef::Github {
                    // A `#123` names a PR or an issue; both lead the name with the number.
                    kind: crate::session_links::SessionLinkKind::PullRequest,
                    repo,
                    number,
                })
            })
            .or_else(|| {
                linear_identifier(word, team_keys)
                    .map(|identifier| SessionLinkRef::Linear { identifier })
            });
        if let Some(link) = link {
            return Some(FoundReference {
                link,
                matched: word.to_string(),
            });
        }
    }
    None
}

/// What leads the name: `spx-123` for Linear, the number for GitHub.
fn reference_key(link: &SessionLinkRef) -> String {
    match link {
        SessionLinkRef::Github { number, .. } => number.to_string(),
        SessionLinkRef::Linear { identifier } => identifier.to_ascii_lowercase(),
    }
}

/// The title Work mode's caches already hold for `link`, without asking anyone.
fn cached_title(link: &SessionLinkRef, scope: &ReferenceScope) -> Option<String> {
    let title = match link {
        SessionLinkRef::Linear { identifier } => {
            let fingerprint = crate::work_mode::project_linear_key(scope.project_id)?;
            crate::work_mode::cached_linear_issue(fingerprint, identifier)?.title
        }
        SessionLinkRef::Github { number, .. } => {
            let url = link.github_url().unwrap_or_default();
            crate::work_mode::cached_work_pull_request(scope.cwd, &number.to_string())
                .and_then(|pull_request| pull_request.title)
                .or_else(|| {
                    crate::work_mode::cached_work_pull_request(scope.cwd, &url)
                        .and_then(|pull_request| pull_request.title)
                })
                .or_else(|| {
                    crate::work_mode::cached_work_github_issue(scope.cwd, *number)
                        .and_then(|issue| issue.title)
                })
                .or_else(|| {
                    crate::work_mode::cached_work_github_issue_url(
                        &url.replace("/pull/", "/issues/"),
                    )
                    .and_then(|issue| issue.title)
                })
        }
    };
    title.filter(|title| !title.trim().is_empty())
}

/// The Linear team keys the project's key is known to see, from the background pass's cache.
fn cached_team_keys(project_id: &str) -> Vec<String> {
    crate::work_mode::project_linear_key(project_id)
        .and_then(crate::work_mode::cached_linear_team_keys)
        .unwrap_or_default()
}

/// The name hint for a new chat's worktree, from its first message.
pub(crate) fn worktree_name_hint(message: &str, scope: &ReferenceScope) -> String {
    let session_repo = crate::work_mode::cached_work_repo(scope.cwd);
    let team_keys = cached_team_keys(scope.project_id);
    let found = first_reference_in_message(message, session_repo.as_deref(), &team_keys);
    let Some(found) = found else {
        return worktree_name_for_message(message, None);
    };
    let key = reference_key(&found.link);
    let title = cached_title(&found.link, scope);
    worktree_name_for_message(
        message,
        Some(&WorktreeNameReference {
            key: &key,
            matched: &found.matched,
            title: title.as_deref(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(message: &str, repo: Option<&str>, team_keys: &[&str]) -> String {
        let team_keys: Vec<String> = team_keys.iter().map(|key| key.to_string()).collect();
        let found = first_reference_in_message(message, repo, &team_keys);
        match found {
            None => worktree_name_for_message(message, None),
            Some(found) => worktree_name_for_message(
                message,
                Some(&WorktreeNameReference {
                    key: &reference_key(&found.link),
                    matched: &found.matched,
                    title: None,
                }),
            ),
        }
    }

    #[test]
    fn linear_keys_lead_the_name() {
        assert_eq!(
            name("SPX-123 fix copy link", None, &[]),
            "spx-123-fix-copy-link"
        );
        assert_eq!(
            name("Look at (SPX-123): copy link", None, &[]),
            "spx-123-look-at-copy-link"
        );
        // Lowercase counts only for a known team.
        assert_eq!(name("spx-123 fix it", None, &["SPX"]), "spx-123-fix-it");
        assert_eq!(name("fix utf-8 paste", None, &[]), "fix-utf-8-paste");
        assert_eq!(
            name("bump GPT-5 default", None, &["SPX"]),
            "bump-gpt-5-default"
        );
    }

    #[test]
    fn linear_urls_lead_the_name() {
        assert_eq!(
            name(
                "Do https://linear.app/acme/issue/SPX-77/copy-link-breaks today",
                None,
                &[]
            ),
            "spx-77-do-today"
        );
    }

    #[test]
    fn github_forms_lead_with_the_number() {
        assert_eq!(
            name("Fix #218 arabic plan cards", Some("acme/web"), &[]),
            "218-fix-arabic-plan-cards"
        );
        // `#218` needs the checkout's repo; without one it is just words.
        assert_eq!(name("Fix #218 now", None, &[]), "fix-218-now");
        assert_eq!(
            name("Port acme/api#9 to web", Some("acme/web"), &[]),
            "9-port-to-web"
        );
        assert_eq!(
            name(
                "Review https://github.com/acme/web/pull/42/files please",
                None,
                &[]
            ),
            "42-review-please"
        );
        assert_eq!(
            name("Close https://github.com/acme/web/issues/7.", None, &[]),
            "7-close"
        );
        // A bare number is not a reference.
        assert_eq!(name("Fix 218 bugs", Some("acme/web"), &[]), "fix-218-bugs");
    }

    #[test]
    fn the_first_reference_wins() {
        assert_eq!(
            name("SPX-1 then #4 then SPX-2", Some("acme/web"), &[]),
            "spx-1-then-4-then-spx-2"
        );
    }
}
