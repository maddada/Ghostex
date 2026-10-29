//! What the "Link to" picker suggests: the session repo's open PRs and GitHub issues (through
//! `gh`), the viewer's Linear issues and the team's Linear projects (through Linear's API). Blocking;
//! the route runs it on a blocking worker.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: right-click a work-mode session → Link to → Pull request, Linear issue, Linear project or GitHub issue opens a picker whose suggestions come from the session's repo first (its open PRs and issues, and Linear issues whose PRs are in this repo), then everything else as you type. Linear issues are a list (several small issues shipped in one PR), the other kinds one each.
//!
//! SEE-ALSO: `merge_work_links` in links.rs (what a pick writes, through `/api/setSessionWorkLinks`),
//! apps/desktop/src/app/window/work_link_picker_modal.rs (the picker), packages/gx-core/src/sidebar_menu/link_menu.rs
//! (the submenu that opens it).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;
use crate::session_git_status::{gh_cli_is_available, run_gh_command, run_git_probe_command};

use super::*;

/// Typing back and forth over the same query reuses the answer instead of asking again.
const CANDIDATES_TTL: Duration = Duration::from_secs(20);
const MAX_CACHED_QUERIES: usize = 64;
const GH_LIST_LIMIT: &str = "30";
const LINEAR_ISSUE_LIMIT: usize = 30;
/// Linked issues the search did not return are fetched by ID so they can still be unticked.
const MAX_LINKED_LOOKUPS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WorkLinkKind {
    PullRequest,
    LinearIssue,
    LinearProject,
    GithubIssue,
    GithubProject,
}

impl WorkLinkKind {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "pullRequest" => Some(Self::PullRequest),
            "linearIssue" => Some(Self::LinearIssue),
            "linearProject" => Some(Self::LinearProject),
            "githubIssue" => Some(Self::GithubIssue),
            "githubProject" => Some(Self::GithubProject),
            _ => None,
        }
    }

    fn wire(self) -> &'static str {
        match self {
            Self::PullRequest => "pullRequest",
            Self::LinearIssue => "linearIssue",
            Self::LinearProject => "linearProject",
            Self::GithubIssue => "githubIssue",
            Self::GithubProject => "githubProject",
        }
    }
}

/// One suggestion. `value` is what `/api/setSessionWorkLinks` takes for the kind.
#[derive(Clone, Debug, PartialEq)]
struct Candidate {
    value: String,
    label: String,
    title: String,
    detail: Option<String>,
    url: Option<String>,
    own_repo: bool,
}

impl Candidate {
    fn to_json(&self, linked: &[String]) -> Value {
        let mut object = Map::new();
        object.insert("value".to_string(), json!(self.value));
        object.insert("label".to_string(), json!(self.label));
        object.insert("title".to_string(), json!(self.title));
        if let Some(detail) = self.detail.as_deref().filter(|text| !text.is_empty()) {
            object.insert("detail".to_string(), json!(detail));
        }
        if let Some(url) = &self.url {
            object.insert("url".to_string(), json!(url));
        }
        if self.own_repo {
            object.insert("ownRepo".to_string(), json!(true));
        }
        if linked.iter().any(|value| value == &self.value) {
            object.insert("linked".to_string(), json!(true));
        }
        Value::Object(object)
    }
}

type CacheKey = (WorkLinkKind, String, String);

fn cache() -> &'static Mutex<HashMap<CacheKey, (Instant, Vec<Candidate>)>> {
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, (Instant, Vec<Candidate>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The picker's answer: `{ kind, multiSelect, linked, candidates, notice? }`. `linked` is what the
/// session links to now (hand-set or automatic), as candidate values.
pub(crate) fn list_work_link_candidates(
    paths: &GxserverPaths,
    project: &Value,
    session: &Value,
    kind: WorkLinkKind,
    query: &str,
) -> Value {
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let targets = work_targets(project, session);
    let cwd = targets.cwd.clone();
    let query = query.trim();
    let linked = linked_values(kind, &targets);

    let (candidates, notice) = match kind {
        WorkLinkKind::PullRequest | WorkLinkKind::GithubIssue => match cwd.as_deref() {
            None => (
                Vec::new(),
                Some("This session has no folder to ask GitHub about.".to_string()),
            ),
            Some(_) if !gh_cli_is_available() => (
                Vec::new(),
                Some("Install the GitHub CLI (gh) and sign in to see suggestions.".to_string()),
            ),
            Some(cwd) => (
                cached(kind, cwd, query, || {
                    if kind == WorkLinkKind::PullRequest {
                        list_pull_requests(cwd, query, targets.branch.as_deref())
                    } else {
                        list_github_issues(cwd, query)
                    }
                }),
                None,
            ),
        },
        WorkLinkKind::GithubProject => match cwd.as_deref() {
            _ if !gh_cli_is_available() => (
                Vec::new(),
                Some("Install the GitHub CLI (gh) and sign in to see suggestions.".to_string()),
            ),
            cwd => {
                let owner = cwd
                    .and_then(github_repo_of)
                    .and_then(|repo| repo.split_once('/').map(|(owner, _)| owner.to_string()));
                // Listed once per owner (cached); typing filters that list here.
                let mut failure = None;
                let all = cached(kind, owner.as_deref().unwrap_or("@me"), "", || {
                    list_project_candidates(owner.as_deref())
                        .map_err(|error| failure = Some(error))
                        .ok()
                });
                let query = query.to_lowercase();
                let found: Vec<Candidate> = all
                    .into_iter()
                    .filter(|candidate| {
                        query.is_empty()
                            || candidate.label.to_lowercase().contains(&query)
                            || candidate.value.to_lowercase().contains(&query)
                    })
                    .collect();
                (found, failure)
            }
        },
        WorkLinkKind::LinearIssue | WorkLinkKind::LinearProject => {
            match linear_api_key(
                paths,
                Some(project_id),
                crate::workspaces::stored_project_workspace_id(project),
            ) {
                None => (
                    Vec::new(),
                    Some(
                        "Add a Linear API key (ghostex issues linear-key) to see suggestions."
                            .to_string(),
                    ),
                ),
                Some(key) => {
                    let scope = format!("{:x}", linear_key_fingerprint(&key));
                    let repo = cwd.as_deref().and_then(github_repo_of);
                    let found = cached(kind, &scope, query, || {
                        if kind == WorkLinkKind::LinearIssue {
                            list_linear_issues(&key, query, repo.as_deref())
                        } else {
                            list_linear_projects(&key, query)
                        }
                    });
                    let found = if kind == WorkLinkKind::LinearIssue {
                        with_linked_linear_issues(&key, found, &linked)
                    } else {
                        found
                    };
                    (found, None)
                }
            }
        }
    };

    let mut output = Map::new();
    output.insert("kind".to_string(), json!(kind.wire()));
    output.insert(
        "multiSelect".to_string(),
        json!(kind == WorkLinkKind::LinearIssue),
    );
    output.insert("linked".to_string(), json!(linked));
    output.insert(
        "candidates".to_string(),
        Value::Array(
            candidates
                .iter()
                .map(|candidate| candidate.to_json(&linked))
                .collect(),
        ),
    );
    if let Some(notice) = notice {
        output.insert("notice".to_string(), json!(notice));
    }
    Value::Object(output)
}

fn linked_values(kind: WorkLinkKind, targets: &WorkTargets) -> Vec<String> {
    match kind {
        WorkLinkKind::PullRequest => targets
            .pull_request
            .as_deref()
            .and_then(pull_request_number)
            .map(|number| vec![number.to_string()])
            .unwrap_or_default(),
        WorkLinkKind::LinearIssue => targets.ids_of(WorkSourceId::Linear),
        WorkLinkKind::GithubIssue => targets.ids_of(WorkSourceId::Github),
        WorkLinkKind::GithubProject => session_github_project(targets)
            .map(|project| vec![project.reference()])
            .unwrap_or_default(),
        WorkLinkKind::LinearProject => match &targets.linear_project {
            Some(Some(name)) => vec![name.clone()],
            Some(None) => Vec::new(),
            // Automatic: the project Linear reports for the linked issues.
            None => targets
                .linear_key
                .and_then(|key| {
                    targets
                        .ids_of(WorkSourceId::Linear)
                        .iter()
                        .find_map(|identifier| {
                            cached_linear_issue(key, identifier)
                                .and_then(|issue| issue.project_name)
                        })
                })
                .into_iter()
                .collect(),
        },
    }
}

/// `123` or `https://github.com/o/r/pull/123` → 123.
fn pull_request_number(selector: &str) -> Option<u64> {
    let selector = selector.trim().trim_end_matches('/');
    selector
        .parse()
        .ok()
        .or_else(|| selector.rsplit_once("/pull/")?.1.parse().ok())
}

fn cached(
    kind: WorkLinkKind,
    scope: &str,
    query: &str,
    fetch: impl FnOnce() -> Option<Vec<Candidate>>,
) -> Vec<Candidate> {
    let key = (kind, scope.to_string(), query.to_lowercase());
    if let Some((fetched_at, candidates)) = cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
    {
        if fetched_at.elapsed() < CANDIDATES_TTL {
            return candidates;
        }
    }
    // A failed call is not cached, so the next keystroke tries again.
    let Some(candidates) = fetch() else {
        return Vec::new();
    };
    if let Ok(mut cache) = cache().lock() {
        if cache.len() >= MAX_CACHED_QUERIES {
            cache.retain(|_, (fetched_at, _)| fetched_at.elapsed() < CANDIDATES_TTL);
        }
        if cache.len() < MAX_CACHED_QUERIES {
            cache.insert(key, (Instant::now(), candidates.clone()));
        }
    }
    candidates
}

// --- GitHub ------------------------------------------------------------------------------------

fn gh_list(cwd: &str, noun: &str, fields: &str, query: &str) -> Option<Value> {
    let mut args = vec![
        noun,
        "list",
        "--state",
        "open",
        "--json",
        fields,
        "--limit",
        GH_LIST_LIMIT,
    ];
    if !query.is_empty() {
        args.push("--search");
        args.push(query);
    }
    let output = run_gh_command(Some(cwd), &args)?;
    serde_json::from_str(output.trim()).ok()
}

fn list_pull_requests(cwd: &str, query: &str, branch: Option<&str>) -> Option<Vec<Candidate>> {
    let value = gh_list(cwd, "pr", "number,title,headRefName,author,url", query)?;
    Some(parse_pull_request_list(&value, branch))
}

/// `gh pr list --json number,title,headRefName,author,url`, the session branch's own PR first.
fn parse_pull_request_list(value: &Value, branch: Option<&str>) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            let number = item.get("number").and_then(Value::as_u64)?;
            let head = item.get("headRefName").and_then(Value::as_str);
            let author = item.pointer("/author/login").and_then(Value::as_str);
            Some(Candidate {
                value: number.to_string(),
                label: format!("#{number}"),
                title: text_of(item, "title"),
                detail: join_detail([author, head]),
                url: item.get("url").and_then(Value::as_str).map(str::to_string),
                own_repo: branch.is_some() && head == branch,
            })
        })
        .collect();
    candidates.sort_by_key(|candidate| !candidate.own_repo);
    candidates
}

fn list_github_issues(cwd: &str, query: &str) -> Option<Vec<Candidate>> {
    let value = gh_list(cwd, "issue", "number,title,url,assignees", query)?;
    Some(parse_github_issue_list(&value))
}

/// `gh issue list --json number,title,url,assignees`.
fn parse_github_issue_list(value: &Value) -> Vec<Candidate> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            let number = item.get("number").and_then(Value::as_u64)?;
            let assignees: Vec<&str> = item
                .get("assignees")
                .and_then(Value::as_array)
                .map(|people| {
                    people
                        .iter()
                        .filter_map(|person| person.get("login").and_then(Value::as_str))
                        .collect()
                })
                .unwrap_or_default();
            Some(Candidate {
                value: number.to_string(),
                label: format!("#{number}"),
                title: text_of(item, "title"),
                detail: (!assignees.is_empty()).then(|| assignees.join(", ")),
                url: item.get("url").and_then(Value::as_str).map(str::to_string),
                // Every issue `gh` lists here is from the session's own repo.
                own_repo: true,
            })
        })
        .collect()
}

/// `owner/repo` of the checkout's GitHub `origin`, lowercased.
fn github_repo_of(cwd: &str) -> Option<String> {
    let url = run_git_probe_command(cwd, &["remote", "get-url", "origin"])?;
    github_repo_from_remote(url.trim())
}

/// `https://github.com/o/r(.git)`, `git@github.com:o/r(.git)` and `ssh://git@github.com/o/r` →
/// `o/r`.
fn github_repo_from_remote(url: &str) -> Option<String> {
    let rest = url
        .split_once("github.com/")
        .or_else(|| url.split_once("github.com:"))?
        .1;
    let mut parts = rest.trim_end_matches('/').splitn(3, '/');
    let owner = parts.next().filter(|part| !part.is_empty())?;
    let repo = parts.next()?.trim_end_matches(".git");
    (!repo.is_empty()).then(|| format!("{owner}/{repo}").to_ascii_lowercase())
}

/// The GitHub Projects of the repo's owner, then the person's own (`gh project list`). `Err` is the
/// picker's notice (the missing `read:project` scope).
fn list_project_candidates(owner: Option<&str>) -> Result<Vec<Candidate>, String> {
    let mut candidates: Vec<Candidate> = Vec::new();
    let owners = owner.into_iter().chain(std::iter::once("@me"));
    for (index, owner) in owners.enumerate() {
        let projects = match list_github_projects(owner) {
            Ok(projects) => projects,
            // The person's own list is a bonus; the repo owner's failure is the answer.
            Err(error) if index == 0 || candidates.is_empty() => return Err(error),
            Err(_) => continue,
        };
        for project in projects {
            let value = project.reference();
            let title = project.title.clone().unwrap_or_default();
            if candidates.iter().any(|candidate| candidate.value == value) {
                continue;
            }
            candidates.push(Candidate {
                label: if title.is_empty() {
                    value.clone()
                } else {
                    title
                },
                detail: Some(value.clone()),
                value,
                title: String::new(),
                url: project.url.clone(),
                own_repo: index == 0 && owner != "@me",
            });
        }
    }
    Ok(candidates)
}

// --- Linear ------------------------------------------------------------------------------------

const LINEAR_CANDIDATE_FIELDS: &str =
    "identifier title url state { name type } assignee { name } attachments(first: 10) { nodes { url } }";
const OPEN_ISSUE_FILTER: &str = "state: { type: { nin: [\"completed\", \"canceled\"] } }";

fn list_linear_issues(api_key: &str, query: &str, repo: Option<&str>) -> Option<Vec<Candidate>> {
    let exact = normalize_linear_identifier(query);
    let (graphql, variables) = if query.is_empty() {
        (
            format!(
                "query {{ viewer {{ assignedIssues(first: {LINEAR_ISSUE_LIMIT}, orderBy: updatedAt, filter: {{ {OPEN_ISSUE_FILTER} }}) {{ nodes {{ {LINEAR_CANDIDATE_FIELDS} }} }} }} }}"
            ),
            json!({}),
        )
    } else {
        let exact_selection = if exact.is_some() {
            format!("exact: issue(id: $id) {{ {LINEAR_CANDIDATE_FIELDS} }}")
        } else {
            String::new()
        };
        (
            format!(
                "query($q: String!{}) {{ {exact_selection} viewer {{ assignedIssues(first: {LINEAR_ISSUE_LIMIT}, orderBy: updatedAt, filter: {{ {OPEN_ISSUE_FILTER}, title: {{ containsIgnoreCase: $q }} }}) {{ nodes {{ {LINEAR_CANDIDATE_FIELDS} }} }} }} issues(first: {LINEAR_ISSUE_LIMIT}, orderBy: updatedAt, filter: {{ title: {{ containsIgnoreCase: $q }} }}) {{ nodes {{ {LINEAR_CANDIDATE_FIELDS} }} }} }}",
                if exact.is_some() { ", $id: String!" } else { "" },
            ),
            match &exact {
                Some(identifier) => json!({ "q": query, "id": identifier }),
                None => json!({ "q": query }),
            },
        )
    };
    // An unknown exact ID makes Linear answer with an error for that field only; the rest of the
    // answer still counts, so the call is retried without it.
    let body = linear_graphql(api_key, &graphql, variables.clone())
        .or_else(|error| {
            if exact.is_some() {
                let fallback = graphql
                    .replacen(
                        &format!("exact: issue(id: $id) {{ {LINEAR_CANDIDATE_FIELDS} }}"),
                        "",
                        1,
                    )
                    .replacen(", $id: String!", "", 1);
                linear_graphql(api_key, &fallback, json!({ "q": query }))
            } else {
                Err(error)
            }
        })
        .ok()?;
    Some(parse_linear_issue_search(&body, repo))
}

/// Linear's answer to the search above: the exact ID first, then the viewer's issues, then
/// everyone's, without repeats, and issues with a PR in the session's repo before the rest.
fn parse_linear_issue_search(body: &Value, repo: Option<&str>) -> Vec<Candidate> {
    let mut nodes: Vec<&Value> = Vec::new();
    if let Some(exact) = body
        .pointer("/data/exact")
        .filter(|issue| issue.is_object())
    {
        nodes.push(exact);
    }
    for pointer in ["/data/viewer/assignedIssues/nodes", "/data/issues/nodes"] {
        if let Some(items) = body.pointer(pointer).and_then(Value::as_array) {
            nodes.extend(items);
        }
    }
    let mut seen: Vec<String> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    for node in nodes {
        let Some(candidate) = linear_issue_candidate(node, repo) else {
            continue;
        };
        if seen.contains(&candidate.value) {
            continue;
        }
        seen.push(candidate.value.clone());
        candidates.push(candidate);
    }
    candidates.sort_by_key(|candidate| !candidate.own_repo);
    candidates
}

fn linear_issue_candidate(node: &Value, repo: Option<&str>) -> Option<Candidate> {
    let identifier = node.get("identifier").and_then(Value::as_str)?.to_string();
    let own_repo = repo.is_some_and(|repo| {
        node.pointer("/attachments/nodes")
            .and_then(Value::as_array)
            .is_some_and(|attachments| {
                attachments.iter().any(|attachment| {
                    attachment
                        .get("url")
                        .and_then(Value::as_str)
                        .and_then(github_repo_from_remote)
                        .is_some_and(|attached| attached == repo)
                })
            })
    });
    Some(Candidate {
        label: identifier.clone(),
        value: identifier,
        title: text_of(node, "title"),
        detail: join_detail([
            node.pointer("/assignee/name").and_then(Value::as_str),
            node.pointer("/state/name").and_then(Value::as_str),
        ]),
        url: node.get("url").and_then(Value::as_str).map(str::to_string),
        own_repo,
    })
}

/// Puts the issues the session already links at the top, fetching the ones the search did not
/// return, so a multi-select can untick them.
fn with_linked_linear_issues(
    api_key: &str,
    found: Vec<Candidate>,
    linked: &[String],
) -> Vec<Candidate> {
    let missing: Vec<&String> = linked
        .iter()
        .filter(|identifier| {
            !found
                .iter()
                .any(|candidate| &candidate.value == *identifier)
        })
        .take(MAX_LINKED_LOOKUPS)
        .collect();
    let mut fetched: Vec<Candidate> = Vec::new();
    if !missing.is_empty() {
        let mut declarations = Vec::new();
        let mut selections = Vec::new();
        let mut variables = Map::new();
        for (index, identifier) in missing.iter().enumerate() {
            declarations.push(format!("$i{index}: String!"));
            selections.push(format!(
                "i{index}: issue(id: $i{index}) {{ {LINEAR_CANDIDATE_FIELDS} }}"
            ));
            variables.insert(format!("i{index}"), json!(identifier));
        }
        let graphql = format!(
            "query({}) {{ {} }}",
            declarations.join(", "),
            selections.join(" ")
        );
        let body = linear_graphql(api_key, &graphql, Value::Object(variables)).ok();
        for (index, identifier) in missing.iter().enumerate() {
            let candidate = body
                .as_ref()
                .and_then(|body| body.pointer(&format!("/data/i{index}")))
                .and_then(|node| linear_issue_candidate(node, None))
                .unwrap_or_else(|| Candidate {
                    value: (*identifier).clone(),
                    label: (*identifier).clone(),
                    title: String::new(),
                    detail: None,
                    url: None,
                    own_repo: false,
                });
            fetched.push(candidate);
        }
    }
    let (mut first, rest): (Vec<Candidate>, Vec<Candidate>) = found
        .into_iter()
        .partition(|candidate| linked.contains(&candidate.value));
    first.extend(fetched);
    first.sort_by_key(|candidate| {
        linked
            .iter()
            .position(|identifier| identifier == &candidate.value)
            .unwrap_or(usize::MAX)
    });
    first.extend(rest);
    first
}

fn list_linear_projects(api_key: &str, query: &str) -> Option<Vec<Candidate>> {
    const FIELDS: &str = "name url status { name type } lead { name }";
    let (graphql, variables) = if query.is_empty() {
        (
            format!(
                "query {{ projects(first: 50, orderBy: updatedAt) {{ nodes {{ {FIELDS} }} }} }}"
            ),
            json!({}),
        )
    } else {
        (
            format!("query($q: String!) {{ projects(first: 50, orderBy: updatedAt, filter: {{ name: {{ containsIgnoreCase: $q }} }}) {{ nodes {{ {FIELDS} }} }} }}"),
            json!({ "q": query }),
        )
    };
    let body = linear_graphql(api_key, &graphql, variables).ok()?;
    Some(parse_linear_projects(&body))
}

/// `projects { nodes { name url status { name type } lead { name } } }`, finished and cancelled
/// projects left out.
fn parse_linear_projects(body: &Value) -> Vec<Candidate> {
    body.pointer("/data/projects/nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|node| {
            !matches!(
                node.pointer("/status/type").and_then(Value::as_str),
                Some("completed" | "canceled")
            )
        })
        .filter_map(|node| {
            let name = node.get("name").and_then(Value::as_str)?.trim();
            if name.is_empty() {
                return None;
            }
            Some(Candidate {
                value: name.to_string(),
                label: name.to_string(),
                title: String::new(),
                detail: join_detail([
                    node.pointer("/lead/name").and_then(Value::as_str),
                    node.pointer("/status/name").and_then(Value::as_str),
                ]),
                url: node.get("url").and_then(Value::as_str).map(str::to_string),
                own_repo: false,
            })
        })
        .collect()
}

// --- helpers -----------------------------------------------------------------------------------

fn text_of(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn join_detail<'a>(parts: impl IntoIterator<Item = Option<&'a str>>) -> Option<String> {
    let parts: Vec<&str> = parts
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}
