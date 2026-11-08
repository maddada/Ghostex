//! What a session link points at: a GitHub PR, a GitHub issue, a Linear issue or a ticket of the
//! workspace's ticket command, named by a key that stays the same whichever way the link was
//! written (`#318` from the session's repo, a full URL, `ghx-142`).

use crate::work_mode::{work_source, CommandSourceId, WorkSourceId};

/// The kinds a session can be linked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum SessionLinkKind {
    PullRequest,
    Issue,
    Linear,
    /// A ticket command's ticket; its wire kind and key prefix are the command's id.
    Command(CommandSourceId),
}

impl SessionLinkKind {
    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::PullRequest => "pullRequest",
            Self::Issue => "issue",
            Self::Linear => "linear",
            Self::Command(source) => source.as_str(),
        }
    }

    fn key_prefix(self) -> &'static str {
        match self {
            Self::PullRequest => "pr",
            Self::Issue => "issue",
            Self::Linear => "linear",
            Self::Command(source) => source.as_str(),
        }
    }
}

/// One link target.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SessionLinkRef {
    /// `owner/repo` is lowercase, so two spellings of one repo are one link.
    Github {
        kind: SessionLinkKind,
        repo: String,
        number: u64,
    },
    /// `GHX-142`, uppercase.
    Linear { identifier: String },
    /// `ACM-12` of the ticket command that pinned `source`, as the command reads it.
    ///
    /// CDXC:SessionLinks 2026-11-03 WHY:
    /// A command ticket joins a session's links only through its branch or `/api/setSessionWorkLinks` (`workLinks.tickets`), which refuses an id holding a value the workspace stores; it is never stored among `sessionLinks.items` (`/api/restoreSessionLink` adds no command ticket back by hand) and its last title is kept in the command's own titles table, which goes with its workspace, never in `sessionLinks.read`.
    Command { source: CommandSourceId, id: String },
}

impl SessionLinkRef {
    /// Whether the link may join a session, or be suggested for it, without a person adding it:
    /// a GitHub PR or issue only when it is in one of the session's own repos (`repos`, its
    /// checkout's `origin` and `upstream`, lowercase); a Linear ticket always, as before (the pass
    /// keeps only IDs of the Linear teams the project's key sees).
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "completely unrelated pr from different repo being suggested to be linked to session in my own repo... doesn't make any sense pls fix" (a merged PR of joeblau/voice suggested for a Ghostex session whose chat had read it). Suggestions, PRs and issues the agent opened, and issues a linked PR closes come only from the project's own repos; a link to any repo can still be added by hand, and links a person added or pinned are never dropped.
    pub(crate) fn in_repos(&self, repos: &[String]) -> bool {
        match self {
            Self::Github { repo, .. } => repos.iter().any(|own| own == repo),
            Self::Linear { .. } | Self::Command { .. } => true,
        }
    }

    pub(crate) fn kind(&self) -> SessionLinkKind {
        match self {
            Self::Github { kind, .. } => *kind,
            Self::Linear { .. } => SessionLinkKind::Linear,
            Self::Command { source, .. } => SessionLinkKind::Command(*source),
        }
    }

    /// `pr:owner/repo#318`, `issue:owner/repo#305`, `linear:GHX-142` or `fixture:ACM-12` (a
    /// ticket command's id first): what storage, the removed list and clients name a link by.
    pub(crate) fn key(&self) -> String {
        match self {
            Self::Github { kind, repo, number } => {
                format!("{}:{repo}#{number}", kind.key_prefix())
            }
            Self::Linear { identifier } => format!("linear:{identifier}"),
            Self::Command { source, id } => format!("{}:{id}", source.as_str()),
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        let (prefix, rest) = key.trim().split_once(':')?;
        match prefix {
            "linear" => crate::work_mode::normalize_linear_identifier(rest)
                .map(|identifier| Self::Linear { identifier }),
            "pr" | "issue" => {
                let (repo, number) = rest.rsplit_once('#')?;
                let repo = normalize_repo(repo)?;
                let number = number.parse().ok().filter(|number: &u64| *number > 0)?;
                Some(Self::Github {
                    kind: if prefix == "pr" {
                        SessionLinkKind::PullRequest
                    } else {
                        SessionLinkKind::Issue
                    },
                    repo,
                    number,
                })
            }
            prefix => {
                let source = CommandSourceId::known(prefix)?;
                let id = work_source(WorkSourceId::Command(source)).parse_ref(rest)?;
                Some(Self::Command { source, id })
            }
        }
    }

    /// The GitHub page of a PR or issue. A Linear issue's URL comes from Linear itself.
    pub(crate) fn github_url(&self) -> Option<String> {
        match self {
            Self::Github { kind, repo, number } => Some(format!(
                "https://github.com/{repo}/{}/{number}",
                if *kind == SessionLinkKind::PullRequest {
                    "pull"
                } else {
                    "issues"
                }
            )),
            Self::Linear { .. } | Self::Command { .. } => None,
        }
    }

    /// `#318` in the session's own repo, `repo#318` in another one, `GHX-142` for Linear.
    pub(crate) fn label(&self, session_repo: Option<&str>) -> String {
        match self {
            Self::Github { repo, number, .. } => {
                if session_repo.is_some_and(|own| own.eq_ignore_ascii_case(repo)) {
                    format!("#{number}")
                } else {
                    let name = repo.rsplit('/').next().unwrap_or(repo);
                    format!("{name}#{number}")
                }
            }
            Self::Linear { identifier } => identifier.clone(),
            Self::Command { id, .. } => id.clone(),
        }
    }
}

/// `Owner/Repo` → `owner/repo`; anything that is not two path segments → `None`.
pub(crate) fn normalize_repo(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches(".git");
    let (owner, name) = text.split_once('/')?;
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (valid(owner) && valid(name)).then(|| format!("{owner}/{name}").to_ascii_lowercase())
}

/// A GitHub PR or issue URL (`https://github.com/o/r/pull/12`, `…/issues/5`, with or without
/// a trailing `/files`, `#issuecomment-…` or query).
pub(crate) fn parse_github_url(text: &str) -> Option<SessionLinkRef> {
    let rest = text
        .trim()
        .strip_prefix("https://")
        .or_else(|| text.trim().strip_prefix("http://"))?;
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let rest = rest.strip_prefix("github.com/")?;
    let mut parts = rest.split(['/', '#', '?']);
    let owner = parts.next()?;
    let name = parts.next()?;
    let kind = match parts.next()? {
        "pull" => SessionLinkKind::PullRequest,
        "issues" => SessionLinkKind::Issue,
        _ => return None,
    };
    let number = parts
        .next()?
        .parse()
        .ok()
        .filter(|number: &u64| *number > 0)?;
    Some(SessionLinkRef::Github {
        kind,
        repo: normalize_repo(&format!("{owner}/{name}"))?,
        number,
    })
}

/// A Linear issue URL (`https://linear.app/<workspace>/issue/GHX-142/some-title`).
pub(crate) fn parse_linear_url(text: &str) -> Option<SessionLinkRef> {
    let (_, identifier) = super::linear_url_parts(text)?;
    Some(SessionLinkRef::Linear { identifier })
}

/// What a person or an agent typed to add a link: a GitHub or Linear URL, `#318` or `318` (a PR
/// or issue of the session's repo; `kind_hint` says which, a PR when absent), `owner/repo#318`,
/// or a Linear ID.
pub(crate) fn parse_link_input(
    text: &str,
    session_repo: Option<&str>,
    kind_hint: Option<SessionLinkKind>,
) -> Option<SessionLinkRef> {
    let text = text.trim();
    if let Some(link) = parse_github_url(text).or_else(|| parse_linear_url(text)) {
        return Some(link);
    }
    let github_kind = match kind_hint {
        Some(SessionLinkKind::Issue) => SessionLinkKind::Issue,
        _ => SessionLinkKind::PullRequest,
    };
    if let Some((repo, number)) = text.split_once('#') {
        if !repo.is_empty() {
            let number = number
                .trim()
                .parse()
                .ok()
                .filter(|number: &u64| *number > 0)?;
            return Some(SessionLinkRef::Github {
                kind: github_kind,
                repo: normalize_repo(repo)?,
                number,
            });
        }
    }
    if let Ok(number) = text.trim_start_matches('#').parse::<u64>() {
        if number == 0 || kind_hint == Some(SessionLinkKind::Linear) {
            return None;
        }
        return Some(SessionLinkRef::Github {
            kind: github_kind,
            repo: session_repo?.to_ascii_lowercase(),
            number,
        });
    }
    crate::work_mode::normalize_linear_identifier(text)
        .map(|identifier| SessionLinkRef::Linear { identifier })
}
