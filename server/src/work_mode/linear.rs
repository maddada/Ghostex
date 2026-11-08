//! Linear's GraphQL API: issue status, team keys and key checks, behind TTL caches the
//! presentation projection only reads.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const LINEAR_GRAPHQL_URL: &str = "https://api.linear.app/graphql";
const LINEAR_TIMEOUT: Duration = Duration::from_secs(10);
/// Issue status changes on human timescales; two minutes keeps cards fresh without hammering
/// Linear's rate limit.
const LINEAR_ISSUE_TTL: Duration = Duration::from_secs(120);
/// An issue a session links while it has no PR yet is re-read on every 60s pass instead: Linear's
/// GitHub integration attaches a new PR within seconds, and the card should show it as soon.
const LINEAR_ISSUE_AWAITING_PR_TTL: Duration = Duration::from_secs(45);
const LINEAR_TEAM_KEYS_TTL: Duration = Duration::from_secs(60 * 60);
/// A ticket Linear says does not exist is asked about again only after this long, so a few
/// deleted tickets cannot use up the requests the others need.
const LINEAR_MISSING_ISSUE_TTL: Duration = Duration::from_secs(10 * 60);
/// After a failed call (bad key, no network) a key is left alone for this long.
const LINEAR_FAILURE_BACKOFF: Duration = Duration::from_secs(5 * 60);
const MAX_LINEAR_ISSUES_PER_REQUEST: usize = 25;
const LINEAR_ISSUE_FIELDS: &str = "identifier title url state { name type } project { name url } attachments(first: 20) { nodes { url } }";
/// Requests one background pass may send to Linear.
pub(crate) const MAX_LINEAR_REQUESTS_PER_PASS: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinearIssueInfo {
    pub(crate) identifier: String,
    pub(crate) title: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) state_type: Option<String>,
    pub(crate) state_name: Option<String>,
    pub(crate) project_name: Option<String>,
    pub(crate) project_url: Option<String>,
    /// GitHub PRs Linear's GitHub integration attached to the issue, canonical.
    pub(crate) pull_request_urls: Vec<String>,
}

impl LinearIssueInfo {
    /// How an issue Linear says does not exist is published: state `notFound`, "Not found".
    pub(crate) fn not_found(identifier: &str) -> Self {
        Self {
            identifier: identifier.to_string(),
            title: None,
            url: None,
            state_type: Some("notFound".to_string()),
            state_name: Some("Not found".to_string()),
            project_name: None,
            project_url: None,
            pull_request_urls: Vec::new(),
        }
    }
}

struct Cached<T> {
    value: T,
    fetched_at: Instant,
}

#[derive(Default)]
struct LinearCache {
    team_keys: HashMap<u64, Cached<Vec<String>>>,
    failed_at: HashMap<u64, Instant>,
    /// `None` = Linear said there is no such issue.
    issues: HashMap<(u64, String), Cached<Option<LinearIssueInfo>>>,
    /// Issues a batch left out, each waiting for a read of its own (`refresh_linear_issues`), with
    /// when that read last got an answer it could not use (the issue alone then backs off).
    left_out: HashMap<(u64, String), Option<Instant>>,
    /// Which key each project's calls use, refreshed by the background pass so the projection
    /// never reads the credentials file.
    project_keys: HashMap<String, u64>,
}

fn cache() -> &'static Mutex<LinearCache> {
    static CACHE: OnceLock<Mutex<LinearCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(LinearCache::default()))
}

/// A stable stand-in for a key, so caches never hold the key itself.
pub(crate) fn linear_key_fingerprint(api_key: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    api_key.trim().hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn remember_project_linear_key(project_id: &str, fingerprint: Option<u64>) {
    if let Ok(mut cache) = cache().lock() {
        match fingerprint {
            Some(fingerprint) => {
                cache
                    .project_keys
                    .insert(project_id.to_string(), fingerprint);
            }
            None => {
                cache.project_keys.remove(project_id);
            }
        }
    }
}

pub(crate) fn project_linear_key(project_id: &str) -> Option<u64> {
    cache().lock().ok()?.project_keys.get(project_id).copied()
}

pub(crate) fn cached_linear_team_keys(fingerprint: u64) -> Option<Vec<String>> {
    let cache = cache().lock().ok()?;
    cache
        .team_keys
        .get(&fingerprint)
        .map(|cached| cached.value.clone())
}

/// What the cache knows about an issue.
pub(crate) enum LinearIssueRead {
    Unread,
    /// Linear said there is no such issue: deleted, or never visible to this key.
    NotFound,
    Found(LinearIssueInfo),
}

/// What is known about an issue, in one look at the cache (the projection asks per link).
pub(crate) fn cached_linear_issue_read(fingerprint: u64, identifier: &str) -> LinearIssueRead {
    let Ok(cache) = cache().lock() else {
        return LinearIssueRead::Unread;
    };
    match cache
        .issues
        .get(&(fingerprint, identifier.to_string()))
        .map(|cached| &cached.value)
    {
        None => LinearIssueRead::Unread,
        Some(None) => LinearIssueRead::NotFound,
        Some(Some(info)) => LinearIssueRead::Found(info.clone()),
    }
}

pub(crate) fn cached_linear_issue(fingerprint: u64, identifier: &str) -> Option<LinearIssueInfo> {
    match cached_linear_issue_read(fingerprint, identifier) {
        LinearIssueRead::Found(info) => Some(info),
        LinearIssueRead::Unread | LinearIssueRead::NotFound => None,
    }
}

#[cfg(test)]
pub(crate) fn seed_linear_team_keys(fingerprint: u64, team_keys: &[&str]) {
    cache().lock().unwrap().team_keys.insert(
        fingerprint,
        Cached {
            value: team_keys.iter().map(|key| key.to_string()).collect(),
            fetched_at: Instant::now(),
        },
    );
}

/// `None` stands for an issue Linear said does not exist.
#[cfg(test)]
pub(crate) fn seed_linear_issue(fingerprint: u64, identifier: &str, info: Option<LinearIssueInfo>) {
    cache().lock().unwrap().issues.insert(
        (fingerprint, identifier.to_string()),
        Cached {
            value: info,
            fetched_at: Instant::now(),
        },
    );
}

/// Adds PR URLs a fresher read of the issue found (the Work details) to the cached issue, so a
/// session card linking it shows the PR without waiting for the issue's TTL. Returns whether the
/// cached issue gained a URL; an issue not cached yet is left to the background pass.
pub(crate) fn remember_linear_issue_pull_requests(
    fingerprint: u64,
    identifier: &str,
    urls: &[String],
) -> bool {
    let Ok(mut cache) = cache().lock() else {
        return false;
    };
    let Some(Some(issue)) = cache
        .issues
        .get_mut(&(fingerprint, identifier.to_string()))
        .map(|cached| cached.value.as_mut())
    else {
        return false;
    };
    let mut changed = false;
    for url in urls {
        if !issue.pull_request_urls.contains(url) {
            issue.pull_request_urls.push(url.clone());
            changed = true;
        }
    }
    changed
}

fn in_backoff(cache: &LinearCache, fingerprint: u64) -> bool {
    cache
        .failed_at
        .get(&fingerprint)
        .is_some_and(|failed_at| failed_at.elapsed() < LINEAR_FAILURE_BACKOFF)
}

/// One agent for every Linear call: keeps the TLS connection to Linear alive between calls.
fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        crate::http_agent::config()
            .timeout_global(Some(LINEAR_TIMEOUT))
            .build()
            .into()
    })
}

/// One GraphQL call. `Err` carries a sentence fit to show a person.
pub(super) fn linear_graphql(
    api_key: &str,
    query: &str,
    variables: Value,
) -> Result<Value, String> {
    let body = linear_graphql_answer(api_key, query, variables)?;
    if body.get("data").is_none_or(Value::is_null) {
        let message = body
            .pointer("/errors/0/message")
            .and_then(Value::as_str)
            .unwrap_or("Linear returned no data.");
        return Err(message.to_string());
    }
    Ok(body)
}

/// One GraphQL call's answer, GraphQL errors included; `Err` only when Linear could not be asked
/// or refused the key.
fn linear_graphql_answer(api_key: &str, query: &str, variables: Value) -> Result<Value, String> {
    #[cfg(test)]
    if let Some(answer) = LINEAR_TEST_ANSWER.with(|answer| answer.borrow().clone()) {
        return answer(query, &variables);
    }
    let api_key = api_key.trim();
    // Personal API keys go in as they are; OAuth tokens need the Bearer scheme.
    let authorization = if api_key.starts_with("lin_oauth_") {
        format!("Bearer {api_key}")
    } else {
        api_key.to_string()
    };
    let response = agent()
        .post(LINEAR_GRAPHQL_URL)
        .header("Authorization", &authorization)
        .header("Content-Type", "application/json")
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(json!({ "query": query, "variables": variables }));
    let body: Value = match response {
        // A 400 carries the GraphQL errors.
        Ok(response) => match response.status().as_u16() {
            401 | 403 => return Err("Linear did not accept this API key.".to_string()),
            code if code > 400 => return Err(format!("Linear answered with HTTP {code}.")),
            _ => crate::http_agent::read_json(response)
                .map_err(|error| format!("Linear sent an unreadable answer: {error}"))?,
        },
        Err(error) => return Err(format!("Could not reach Linear: {error}")),
    };
    Ok(body)
}

#[cfg(test)]
type LinearTestAnswer = std::rc::Rc<dyn Fn(&str, &Value) -> Result<Value, String>>;

#[cfg(test)]
thread_local! {
    static LINEAR_TEST_ANSWER: std::cell::RefCell<Option<LinearTestAnswer>> = const { std::cell::RefCell::new(None) };
}

/// Puts Linear's calls back to what answered them before when dropped.
#[cfg(test)]
#[must_use = "bind it for the whole test: `let _linear = ...`; a bare `_` drops it at once"]
pub(crate) struct LinearAnsweredForTest {
    previous: Option<LinearTestAnswer>,
}

#[cfg(test)]
impl Drop for LinearAnsweredForTest {
    fn drop(&mut self) {
        LINEAR_TEST_ANSWER.with(|answer| *answer.borrow_mut() = self.previous.take());
    }
}

/// Answers this thread's Linear calls with `answer(query, variables)` instead of asking Linear,
/// for as long as the guard is held, so a test can see what a pass asks for and nothing it runs
/// reaches Linear. Per thread for the reason `assume_gh_cli_available_for_test` is.
#[cfg(test)]
pub(crate) fn answer_linear_for_test(
    answer: impl Fn(&str, &Value) -> Result<Value, String> + 'static,
) -> LinearAnsweredForTest {
    let answer: LinearTestAnswer = std::rc::Rc::new(answer);
    LinearAnsweredForTest {
        previous: LINEAR_TEST_ANSWER.with(|current| current.replace(Some(answer))),
    }
}

/// Checks a key and says whose it is: `{ id, name, organization }` (`id` is the Linear user's).
pub(crate) fn verify_linear_api_key(api_key: &str) -> Result<Value, String> {
    let body = linear_graphql(
        api_key,
        "query { viewer { id name email } organization { name urlKey } }",
        json!({}),
    )?;
    Ok(json!({
        "id": body.pointer("/data/viewer/id").cloned().unwrap_or(Value::Null),
        "name": body.pointer("/data/viewer/name").cloned().unwrap_or(Value::Null),
        "organization": body.pointer("/data/organization/name").cloned().unwrap_or(Value::Null),
    }))
}

/// Refreshes the team keys behind a key when they are stale. Returns whether a request was sent.
pub(crate) fn refresh_linear_team_keys(api_key: &str) -> bool {
    let fingerprint = linear_key_fingerprint(api_key);
    {
        let Ok(cache) = cache().lock() else {
            return false;
        };
        let fresh = cache
            .team_keys
            .get(&fingerprint)
            .is_some_and(|cached| cached.fetched_at.elapsed() < LINEAR_TEAM_KEYS_TTL);
        if fresh || in_backoff(&cache, fingerprint) {
            return false;
        }
    }
    let result = linear_graphql(
        api_key,
        "query { teams(first: 250) { nodes { key } } }",
        json!({}),
    );
    let Ok(mut cache) = cache().lock() else {
        return true;
    };
    match result {
        Ok(body) => {
            let keys = body
                .pointer("/data/teams/nodes")
                .and_then(Value::as_array)
                .map(|nodes| {
                    nodes
                        .iter()
                        .filter_map(|node| node.get("key").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            cache.team_keys.insert(
                fingerprint,
                Cached {
                    value: keys,
                    fetched_at: Instant::now(),
                },
            );
            cache.failed_at.remove(&fingerprint);
        }
        Err(_) => {
            cache.failed_at.insert(fingerprint, Instant::now());
        }
    }
    true
}

/// Fetches the stale ones among `identifiers`, at most `request_budget` requests. Returns how
/// many requests were sent. The ones in `awaiting_pull_request` go stale sooner, and the ones
/// Linear said do not exist later.
///
/// CDXC:SessionLinks 2026-10-25 WHY:
/// Tickets used to be read 25 at a time as aliased `issue(id:)` fields, but that field cannot be null, so one deleted ticket made Linear answer `data: null` with "Entity not found: Issue" for its whole batch (naming only the first missing alias): the key backed off for 5 minutes, that batch and every one after it were never cached, and the same batch failed again after the backoff (2026-10-25: 104 of 165 links "Loading…" behind one deleted ticket). The batch is now `issues(filter: { id: { in } })`, which matches identifiers, lowercase ones, UUIDs, archived and trashed tickets as `issue(id:)` does and leaves a missing one out instead. It does not match the old identifier of a ticket moved to another team, which `issue(id:)` still resolves, so a ticket the batch leaves out waits (`left_out`, across passes, never batched again meanwhile) to be asked about alone before it counts as not found. A pass spends its budget on never-read tickets, then the waiting ones, then refreshes of tickets read before; when batches alone used up the budget, a waiting ticket was never asked. A ticket already read alone and found missing that a batch leaves out again stays not found without another read of its own, so deleted tickets cost nothing past their place in a batch and cannot hold up refreshes of the others; and an answer to that read that is neither the ticket nor "Entity not found" backs off that ticket alone, not the key.
pub(crate) fn refresh_linear_issues(
    api_key: &str,
    identifiers: &[String],
    awaiting_pull_request: &[String],
    request_budget: usize,
) -> usize {
    let fingerprint = linear_key_fingerprint(api_key);
    let (unread, mut waiting, due) = {
        let Ok(cache) = cache().lock() else {
            return 0;
        };
        if in_backoff(&cache, fingerprint) {
            return 0;
        }
        let mut wanted = identifiers.to_vec();
        wanted.sort();
        wanted.dedup();
        let (mut unread, mut waiting, mut due) = (Vec::new(), VecDeque::new(), Vec::new());
        for identifier in wanted {
            let key = (fingerprint, identifier);
            if let Some(failed_at) = cache.left_out.get(&key) {
                if failed_at.is_none_or(|at| at.elapsed() >= LINEAR_FAILURE_BACKOFF) {
                    waiting.push_back(key.1);
                }
                continue;
            }
            match cache.issues.get(&key) {
                None => unread.push(key.1),
                Some(cached) => {
                    let ttl = if cached.value.is_none() {
                        LINEAR_MISSING_ISSUE_TTL
                    } else if awaiting_pull_request.contains(&key.1) {
                        LINEAR_ISSUE_AWAITING_PR_TTL
                    } else {
                        LINEAR_ISSUE_TTL
                    };
                    if cached.fetched_at.elapsed() >= ttl {
                        due.push(key.1);
                    }
                }
            }
        }
        (unread, waiting, due)
    };
    let mut sent = 0;
    for batch in [unread, due] {
        for chunk in batch.chunks(MAX_LINEAR_ISSUES_PER_REQUEST) {
            if sent >= request_budget {
                return sent;
            }
            sent += 1;
            let Some(left_out) = read_linear_issue_batch(api_key, fingerprint, chunk) else {
                return sent;
            };
            waiting.extend(left_out);
        }
        while sent < request_budget {
            let Some(identifier) = waiting.pop_front() else {
                break;
            };
            sent += 1;
            if !read_linear_issue_alone(api_key, fingerprint, &identifier) {
                return sent;
            }
        }
    }
    sent
}

/// One request for up to 25 issues; caches the ones Linear found and returns the ones it left out,
/// which wait in `left_out` to be read alone. `None` when the request failed and the key backs off.
fn read_linear_issue_batch(
    api_key: &str,
    fingerprint: u64,
    identifiers: &[String],
) -> Option<Vec<String>> {
    let query = format!(
        "query($ids: [ID!]) {{ issues(first: {MAX_LINEAR_ISSUES_PER_REQUEST}, includeArchived: true, filter: {{ id: {{ in: $ids }} }}) {{ nodes {{ {LINEAR_ISSUE_FIELDS} }} }} }}"
    );
    let body = linear_graphql(api_key, &query, json!({ "ids": identifiers })).ok();
    // Matched and parsed before the cache is locked, so the lock only covers the inserts.
    let read = body
        .as_ref()
        .and_then(|body| body.pointer("/data/issues/nodes")?.as_array())
        .map(|nodes| {
            let (mut found, mut left_out) = (Vec::new(), Vec::new());
            for identifier in identifiers {
                let node = nodes.iter().find(|node| {
                    node.get("identifier")
                        .and_then(Value::as_str)
                        .is_some_and(|found| found.eq_ignore_ascii_case(identifier))
                });
                match node {
                    Some(issue) => found.push((identifier.clone(), parse_issue(identifier, issue))),
                    None => left_out.push(identifier.clone()),
                }
            }
            (found, left_out)
        });
    let mut cache = cache().lock().ok()?;
    let Some((found, left_out)) = read else {
        cache.failed_at.insert(fingerprint, Instant::now());
        return None;
    };
    cache.failed_at.remove(&fingerprint);
    let fetched_at = Instant::now();
    for (identifier, info) in found {
        cache.issues.insert(
            (fingerprint, identifier),
            Cached {
                value: Some(info),
                fetched_at,
            },
        );
    }
    // One already read alone and found missing is not asked about alone again: still left out, it
    // is still missing.
    let mut waiting = Vec::new();
    for identifier in left_out {
        let key = (fingerprint, identifier);
        match cache.issues.get_mut(&key) {
            Some(cached) if cached.value.is_none() => cached.fetched_at = fetched_at,
            _ => {
                cache.left_out.insert(key.clone(), None);
                waiting.push(key.1);
            }
        }
    }
    Some(waiting)
}

/// How Linear's answer to `issue(id:)` for an issue that does not exist starts ("Entity not found:
/// Issue", measured 2026-10-25).
const LINEAR_ISSUE_NOT_FOUND: &str = "Entity not found";

/// One request for an issue a batch left out: the moved issue its old identifier names, or Linear's
/// "Entity not found: Issue", cached as `None`. Any other answer backs off this issue alone, which
/// keeps waiting; `false` when Linear could not be asked, and the key then backs off.
fn read_linear_issue_alone(api_key: &str, fingerprint: u64, identifier: &str) -> bool {
    let query = format!("query($id: String!) {{ issue(id: $id) {{ {LINEAR_ISSUE_FIELDS} }} }}");
    let answer = linear_graphql_answer(api_key, &query, json!({ "id": identifier }));
    let value = answer.as_ref().ok().and_then(|body| {
        if let Some(issue) = body
            .pointer("/data/issue")
            .filter(|issue| issue.get("identifier").is_some_and(Value::is_string))
        {
            return Some(Some(parse_issue(identifier, issue)));
        }
        body.pointer("/errors/0/message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.starts_with(LINEAR_ISSUE_NOT_FOUND))
            .then_some(None)
    });
    let Ok(mut cache) = cache().lock() else {
        return false;
    };
    let key = (fingerprint, identifier.to_string());
    if answer.is_err() {
        cache.failed_at.insert(fingerprint, Instant::now());
        return false;
    }
    let Some(value) = value else {
        cache.left_out.insert(key, Some(Instant::now()));
        return true;
    };
    cache.left_out.remove(&key);
    cache.issues.insert(
        key,
        Cached {
            value,
            fetched_at: Instant::now(),
        },
    );
    true
}

fn parse_issue(identifier: &str, issue: &Value) -> LinearIssueInfo {
    let text = |pointer: &str| {
        issue
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    LinearIssueInfo {
        identifier: text("/identifier").unwrap_or_else(|| identifier.to_string()),
        title: text("/title"),
        url: text("/url"),
        state_type: text("/state/type"),
        state_name: text("/state/name"),
        project_name: text("/project/name"),
        project_url: text("/project/url"),
        pull_request_urls: issue
            .pointer("/attachments/nodes")
            .and_then(Value::as_array)
            .map(|nodes| {
                super::canonical_github_pull_request_urls(
                    nodes
                        .iter()
                        .filter_map(|node| node.get("url").and_then(Value::as_str)),
                )
            })
            .unwrap_or_default(),
    }
}
