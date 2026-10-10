//! The Work page's lists (`/api/listWorkItems`): the open Linear issues of the viewer and of the
//! teams the work-mode projects use, and each repo's open GitHub PRs and issues through `gh`.
//! Fetched only by `refresh_work_feeds`, which the route runs on a blocking worker with a time
//! limit; the list itself is built from these caches, so it never waits on the network.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::session_git_status::{
    gh_cli_is_available, parse_gh_pull_request_state, run_gh_command, run_git_probe_command,
    PullRequestState,
};

use super::*;

/// The list is a dashboard people glance at; a minute and a half keeps it fresh without spending
/// Linear's rate limit or a `gh` call per repo on every open.
const WORK_FEED_TTL: Duration = Duration::from_secs(90);
const VIEWER_LOGIN_TTL: Duration = Duration::from_secs(60 * 60);
/// Rows one feed returns. The page is a list of ongoing work, not an archive.
const LINEAR_LIST_LIMIT: usize = 100;
const GITHUB_LIST_LIMIT: &str = "50";
/// How long one repo's `gh pr list` / `gh issue list` may take.
///
/// CDXC:WorkMode 2026-10-10 WHY: `statusCheckRollup` on 50 open PRs of a repo with a large CI
/// matrix (ShortPoint's) takes 5 to 9 s, so the 10 s limit of the git-status probes
/// (`run_gh_command`) cut it off under load and the Work page showed no PRs and no issues at all,
/// with that failure cached for the whole feed TTL. The route never waits on this fetch (it stops
/// after `WORK_LIST_WAIT` and the page shows "refreshing"), so a longer limit costs nothing.
const GITHUB_LIST_TIMEOUT: Duration = Duration::from_secs(45);
/// A PR body is only scanned for "Fixes #218", so a long one is cut.
const PULL_REQUEST_BODY_CHARS: usize = 4000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkPerson {
    pub(crate) name: String,
    pub(crate) login: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) is_me: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinearListIssue {
    pub(crate) identifier: String,
    pub(crate) title: String,
    pub(crate) url: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) branch_name: Option<String>,
    pub(crate) state_name: Option<String>,
    pub(crate) state_type: Option<String>,
    pub(crate) assignee: Option<WorkPerson>,
    pub(crate) team_key: Option<String>,
    pub(crate) project_name: Option<String>,
    pub(crate) project_url: Option<String>,
    pub(crate) cycle: Option<String>,
    pub(crate) labels: Vec<String>,
    pub(crate) pull_request_urls: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinearFeed {
    pub(crate) issues: Vec<LinearListIssue>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GithubListPullRequest {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: Option<String>,
    pub(crate) state: PullRequestState,
    pub(crate) author: Option<String>,
    pub(crate) assignees: Vec<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) head_branch: Option<String>,
    pub(crate) checks: Option<&'static str>,
    pub(crate) review_decision: Option<String>,
    pub(crate) body: String,
    /// The first GitHub Project it is in (title, Status), when `gh` may read projects.
    pub(crate) project: Option<GithubListProject>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GithubListProject {
    pub(crate) title: String,
    pub(crate) status: Option<String>,
    /// The project's page. `gh`'s `projectItems` carry no URL, so `fetch_github_feed` fills it
    /// from the repo owner's project list (`project_urls_by_title`).
    pub(crate) url: Option<String>,
}

/// `projectItems` of `gh issue list` / `gh pr list` → the first project.
fn first_project_item(value: &Value) -> Option<GithubListProject> {
    value
        .get("projectItems")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|item| {
            let title = item.get("title").and_then(Value::as_str)?.trim();
            (!title.is_empty()).then(|| GithubListProject {
                title: title.to_string(),
                status: item
                    .pointer("/status/name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string),
                url: None,
            })
        })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GithubListIssue {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) assignees: Vec<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) labels: Vec<String>,
    pub(crate) project: Option<GithubListProject>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GithubFeed {
    /// `owner/repo`, lowercased, from the checkout's `origin`.
    pub(crate) repo: Option<String>,
    pub(crate) pull_requests: Vec<GithubListPullRequest>,
    pub(crate) issues: Vec<GithubListIssue>,
}

struct Cached<T> {
    value: T,
    fetched_at: Instant,
}

impl<T> Cached<T> {
    fn fresh(&self) -> bool {
        self.fetched_at.elapsed() < WORK_FEED_TTL
    }
}

#[derive(Default)]
struct FeedCache {
    /// Keyed by the key's fingerprint and the team keys asked for.
    linear: HashMap<(u64, String), Cached<Result<LinearFeed, String>>>,
    /// Keyed by the checkout `gh` ran in.
    github: HashMap<String, Cached<Result<GithubFeed, String>>>,
    viewer_login: Option<Cached<Option<String>>>,
}

fn cache() -> &'static Mutex<FeedCache> {
    static CACHE: OnceLock<Mutex<FeedCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(FeedCache::default()))
}

/// One refresh at a time: a second page opening while the first is still fetching waits for it
/// and then finds the caches fresh instead of asking Linear and `gh` again.
fn refresh_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// What one Linear key is asked for: the viewer's issues plus these teams' open ones.
#[derive(Clone, Debug)]
pub(crate) struct LinearFeedRequest {
    pub(crate) api_key: String,
    pub(crate) team_keys: BTreeSet<String>,
}

impl LinearFeedRequest {
    fn cache_key(&self) -> (u64, String) {
        (
            linear_key_fingerprint(&self.api_key),
            self.team_keys.iter().cloned().collect::<Vec<_>>().join(","),
        )
    }
}

pub(crate) fn cached_linear_feed(
    request: &LinearFeedRequest,
) -> Option<Result<LinearFeed, String>> {
    let cache = cache().lock().ok()?;
    cache
        .linear
        .get(&request.cache_key())
        .map(|cached| cached.value.clone())
}

pub(crate) fn cached_github_feed(cwd: &str) -> Option<Result<GithubFeed, String>> {
    let cache = cache().lock().ok()?;
    cache.github.get(cwd).map(|cached| cached.value.clone())
}

/// The `gh` account's login, for "Assigned to me" on PRs and GitHub issues.
pub(crate) fn cached_github_viewer_login() -> Option<String> {
    let cache = cache().lock().ok()?;
    cache.viewer_login.as_ref()?.value.clone()
}

/// Whether any of these feeds is missing or stale.
pub(crate) fn work_feeds_stale(linear: &[LinearFeedRequest], github_cwds: &[String]) -> bool {
    let Ok(cache) = cache().lock() else {
        return false;
    };
    linear.iter().any(|request| {
        cache
            .linear
            .get(&request.cache_key())
            .is_none_or(|cached| !cached.fresh())
    }) || github_cwds
        .iter()
        .any(|cwd| cache.github.get(cwd).is_none_or(|cached| !cached.fresh()))
}

/// Fetches every stale feed, all at once (one thread per Linear key and per repo). Blocking; the
/// route runs it on a blocking worker and stops waiting after its own time limit, while this
/// carries on and fills the caches for the next read.
pub(crate) fn refresh_work_feeds(
    linear: &[LinearFeedRequest],
    github_cwds: &[String],
    force: bool,
) {
    let Ok(_guard) = refresh_lock().lock() else {
        return;
    };
    let stale_linear: Vec<&LinearFeedRequest> = {
        let Ok(cache) = cache().lock() else {
            return;
        };
        linear
            .iter()
            .filter(|request| {
                force
                    || cache
                        .linear
                        .get(&request.cache_key())
                        .is_none_or(|cached| !cached.fresh())
            })
            .collect()
    };
    let gh = !github_cwds.is_empty() && gh_cli_is_available();
    let stale_github: Vec<&String> = if gh {
        let Ok(cache) = cache().lock() else {
            return;
        };
        github_cwds
            .iter()
            .filter(|cwd| {
                force
                    || cache
                        .github
                        .get(cwd.as_str())
                        .is_none_or(|cached| !cached.fresh())
            })
            .collect()
    } else {
        Vec::new()
    };
    // Whether the repo lists may ask for project items (`projectItems`).
    if gh {
        refresh_github_projects_access(force);
    }
    let viewer_stale = gh
        && cache().lock().ok().is_some_and(|cache| {
            cache
                .viewer_login
                .as_ref()
                .is_none_or(|cached| cached.fetched_at.elapsed() >= VIEWER_LOGIN_TTL)
        });

    std::thread::scope(|scope| {
        for request in stale_linear {
            scope.spawn(move || {
                let value = fetch_linear_feed(request);
                if let Ok(mut cache) = cache().lock() {
                    cache.linear.insert(
                        request.cache_key(),
                        Cached {
                            value,
                            fetched_at: Instant::now(),
                        },
                    );
                }
            });
        }
        for cwd in stale_github {
            scope.spawn(move || {
                let value = fetch_github_feed(cwd);
                if let Ok(mut cache) = cache().lock() {
                    cache.github.insert(
                        cwd.clone(),
                        Cached {
                            value,
                            fetched_at: Instant::now(),
                        },
                    );
                }
            });
        }
        if viewer_stale {
            scope.spawn(|| {
                let login = run_gh_command(None, &["api", "user", "--jq", ".login"])
                    .map(|login| login.trim().to_string())
                    .filter(|login| !login.is_empty());
                if let Ok(mut cache) = cache().lock() {
                    cache.viewer_login = Some(Cached {
                        value: login,
                        fetched_at: Instant::now(),
                    });
                }
            });
        }
    });
}

const LINEAR_LIST_FIELDS: &str = "identifier title url updatedAt branchName state { name type } assignee { name displayName avatarUrl isMe } team { key } project { name url } cycle { name number } labels(first: 10) { nodes { name } } attachments(first: 10) { nodes { url } }";

fn fetch_linear_feed(request: &LinearFeedRequest) -> Result<LinearFeed, String> {
    let team_keys: Vec<&String> = request.team_keys.iter().collect();
    let query = format!(
        "query WorkList($teamKeys: [String!], $first: Int) {{ issues(first: $first, orderBy: updatedAt, filter: {{ state: {{ type: {{ nin: [\"completed\", \"canceled\"] }} }}, or: [{{ assignee: {{ isMe: {{ eq: true }} }} }}, {{ team: {{ key: {{ in: $teamKeys }} }} }}] }}) {{ nodes {{ {LINEAR_LIST_FIELDS} }} }} }}"
    );
    let body = linear_graphql(
        &request.api_key,
        &query,
        json!({ "teamKeys": team_keys, "first": LINEAR_LIST_LIMIT }),
    )?;
    let issues = body
        .pointer("/data/issues/nodes")
        .and_then(Value::as_array)
        .map(|nodes| nodes.iter().filter_map(parse_linear_list_issue).collect())
        .unwrap_or_default();
    Ok(LinearFeed { issues })
}

pub(crate) fn parse_linear_list_issue(issue: &Value) -> Option<LinearListIssue> {
    let text = |pointer: &str| {
        issue
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let identifier = text("/identifier")?;
    let assignee = issue
        .get("assignee")
        .filter(|assignee| assignee.is_object())
        .map(|assignee| WorkPerson {
            name: assignee
                .get("displayName")
                .or_else(|| assignee.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            login: None,
            avatar_url: assignee
                .get("avatarUrl")
                .and_then(Value::as_str)
                .map(str::to_string),
            is_me: assignee
                .get("isMe")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    let cycle = issue
        .get("cycle")
        .filter(|cycle| cycle.is_object())
        .map(|cycle| {
            cycle
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "Cycle {}",
                        cycle.get("number").and_then(Value::as_u64).unwrap_or(0)
                    )
                })
        });
    Some(LinearListIssue {
        title: text("/title").unwrap_or_else(|| identifier.clone()),
        url: text("/url"),
        updated_at: text("/updatedAt"),
        branch_name: text("/branchName"),
        state_name: text("/state/name"),
        state_type: text("/state/type"),
        assignee,
        team_key: text("/team/key"),
        project_name: text("/project/name"),
        project_url: text("/project/url"),
        cycle,
        labels: names(issue.pointer("/labels/nodes"), "name"),
        pull_request_urls: names(issue.pointer("/attachments/nodes"), "url")
            .into_iter()
            .filter(|url| is_github_pull_request_url(url))
            .collect(),
        identifier,
    })
}

pub(crate) fn is_github_pull_request_url(url: &str) -> bool {
    url.contains("github.com/") && url.contains("/pull/")
}

fn names(nodes: Option<&Value>, key: &str) -> Vec<String> {
    nodes
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| node.get(key).and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn logins(people: Option<&Value>) -> Vec<String> {
    names(people, "login")
}

/// `owner/repo` of a checkout's GitHub `origin`, lowercased.
pub(crate) fn work_repo_of(cwd: &str) -> Option<String> {
    let url = run_git_probe_command(cwd, &["remote", "get-url", "origin"])?;
    let url = url.trim();
    let rest = url
        .split_once("github.com/")
        .or_else(|| url.split_once("github.com:"))?
        .1;
    let mut parts = rest.trim_end_matches('/').splitn(3, '/');
    let owner = parts.next().filter(|part| !part.is_empty())?;
    let repo = parts.next()?.trim_end_matches(".git");
    (!repo.is_empty()).then(|| format!("{owner}/{repo}").to_ascii_lowercase())
}

/// `owner/repo` (lowercased) and the number of a `https://github.com/o/r/pull/12` URL.
pub(crate) fn pull_request_url_parts(url: &str) -> Option<(String, u64)> {
    let rest = url.split_once("github.com/")?.1;
    let mut parts = rest.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if parts.next()? != "pull" {
        return None;
    }
    let number = parts.next()?.split(['#', '?']).next()?.parse().ok()?;
    Some((format!("{owner}/{repo}").to_ascii_lowercase(), number))
}

fn fetch_github_feed(cwd: &str) -> Result<GithubFeed, String> {
    let repo = work_repo_of(cwd);
    // `projectItems` makes `gh` fail outright without the `read:project` scope, so it is asked
    // for only once `gh` is known to have it.
    let projects = cached_github_projects_access() == GithubProjectsAccess::Granted;
    let pull_request_fields = if projects {
        "number,title,url,state,isDraft,author,assignees,updatedAt,headRefName,statusCheckRollup,reviewDecision,body,projectItems"
    } else {
        "number,title,url,state,isDraft,author,assignees,updatedAt,headRefName,statusCheckRollup,reviewDecision,body"
    };
    let issue_fields = if projects {
        "number,title,url,author,assignees,updatedAt,labels,projectItems"
    } else {
        "number,title,url,author,assignees,updatedAt,labels"
    };
    let pull_requests = run_gh_full(
        Some(cwd),
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            GITHUB_LIST_LIMIT,
            "--json",
            pull_request_fields,
        ],
        GITHUB_LIST_TIMEOUT,
    )
    .ok_or_else(|| "gh took too long to list this repo's pull requests.".to_string())
    .and_then(|run| {
        if run.success {
            Ok(run.stdout)
        } else {
            Err(format!(
                "gh could not list this repo's pull requests: {}",
                run.error_text()
            ))
        }
    })?;
    let mut pull_requests: Vec<GithubListPullRequest> =
        serde_json::from_str::<Value>(pull_requests.trim())
            .ok()
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(parse_github_list_pull_request)
            .collect();
    let mut issues: Vec<GithubListIssue> = run_gh_full(
        Some(cwd),
        &[
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            GITHUB_LIST_LIMIT,
            "--json",
            issue_fields,
        ],
        GITHUB_LIST_TIMEOUT,
    )
    .filter(|run| run.success)
    .and_then(|run| serde_json::from_str::<Value>(run.stdout.trim()).ok())
    .and_then(|value| value.as_array().cloned())
    .unwrap_or_default()
    .iter()
    .filter_map(|issue| {
        Some(GithubListIssue {
            number: issue.get("number").and_then(Value::as_u64)?,
            title: issue
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            url: issue.get("url").and_then(Value::as_str).map(str::to_string),
            author: issue
                .pointer("/author/login")
                .and_then(Value::as_str)
                .map(str::to_string),
            assignees: logins(issue.get("assignees")),
            updated_at: issue
                .get("updatedAt")
                .and_then(Value::as_str)
                .map(str::to_string),
            labels: names(issue.get("labels"), "name"),
            project: first_project_item(issue),
        })
    })
    .collect();
    let has_projects = pull_requests.iter().any(|pr| pr.project.is_some())
        || issues.iter().any(|issue| issue.project.is_some());
    if let Some(owner) = repo
        .as_deref()
        .and_then(|repo| repo.split_once('/'))
        .map(|(owner, _)| owner)
        .filter(|_| has_projects)
    {
        let urls = project_urls_by_title(owner);
        let projects = pull_requests
            .iter_mut()
            .filter_map(|pr| pr.project.as_mut())
            .chain(issues.iter_mut().filter_map(|issue| issue.project.as_mut()));
        for project in projects {
            project.url = urls.get(&project.title).cloned();
        }
    }
    Ok(GithubFeed {
        repo,
        pull_requests,
        issues,
    })
}

/// The open GitHub Projects of `owner` by title, for the Work page's project links. A project
/// owned by someone else (a user's project holding an org's issue) stays without a link.
fn project_urls_by_title(owner: &str) -> HashMap<String, String> {
    list_github_projects(owner)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|project| Some((project.title?, project.url?)))
        .collect()
}

fn parse_github_list_pull_request(value: &Value) -> Option<GithubListPullRequest> {
    let number = value.get("number").and_then(Value::as_u64)?;
    let is_draft = value
        .get("isDraft")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let state = parse_gh_pull_request_state(value.get("state").and_then(Value::as_str)?, is_draft)?;
    let mut body = value
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Some((cut, _)) = body.char_indices().nth(PULL_REQUEST_BODY_CHARS) {
        body.truncate(cut);
    }
    Some(GithubListPullRequest {
        number,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        url: value.get("url").and_then(Value::as_str).map(str::to_string),
        state,
        author: value
            .pointer("/author/login")
            .and_then(Value::as_str)
            .map(str::to_string),
        assignees: logins(value.get("assignees")),
        updated_at: value
            .get("updatedAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        head_branch: value
            .get("headRefName")
            .and_then(Value::as_str)
            .map(str::to_string),
        checks: value.get("statusCheckRollup").and_then(checks_state),
        review_decision: value
            .get("reviewDecision")
            .and_then(Value::as_str)
            .filter(|decision| !decision.is_empty())
            .map(str::to_string),
        body,
        project: first_project_item(value),
    })
}

/// GitHub issue numbers a PR body closes ("Fixes #218", "closes #12, resolves #13").
pub(crate) fn closing_issue_numbers(body: &str) -> Vec<u64> {
    let lower = body.to_ascii_lowercase();
    let mut found = Vec::new();
    for keyword in [
        "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
    ] {
        let mut rest = lower.as_str();
        while let Some(index) = rest.find(keyword) {
            let before_ok = index == 0 || !rest.as_bytes()[index - 1].is_ascii_alphanumeric();
            let after = &rest[index + keyword.len()..];
            rest = after;
            if !before_ok {
                continue;
            }
            let after = after.trim_start_matches([':', ' ']);
            let Some(digits) = after.strip_prefix('#') else {
                continue;
            };
            let end = digits
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(digits.len());
            if let Ok(number) = digits[..end].parse::<u64>() {
                if number > 0 && !found.contains(&number) {
                    found.push(number);
                }
            }
        }
    }
    found
}
