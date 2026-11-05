//! Reading linked PRs and issues in batches: one GraphQL call reads a repo's due PRs and issues,
//! a REST read of an issue sends the ETag it got last time, and what was read (with when and its
//! ETag) can be put back after a restart from `sessionLinks.read`.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User, asked whether to make the links cache leaner: "Yes: batch, ETags and slower idle refresh". A pass reads each repo's due PRs and issues in one `gh api graphql` call (40 at a time) with the fields `gh pr view` gave (title, state, checks, review, closing issues); only what the batch cannot answer (a number that is not the kind asked for, a repo name GraphQL cannot take) is read alone, an issue through REST with `If-None-Match`, where a 304 only marks it fresh. Links of sessions idle for more than a day are re-read every 30 minutes instead of every 2 (PRs) or 5 (issues), and opening a session's links list asks for its links at once (`/api/refreshSessionLinks`). Read times and ETags are kept with the session's links, so a restart does not read everything again.
//!
//! SEE-ALSO: server/src/server/session_links_sync.rs and work_mode/refresh.rs (the two passes),
//! server/src/session_links/presentation.rs (`sessionLinks.read`).

use std::collections::BTreeMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use super::github::{
    cache, cache_key, parse_pull_request_value, refresh_work_pull_request, Cached,
    WorkGithubIssueInfo, WorkPullRequestInfo,
};
use crate::session_git_status::{parse_gh_pull_request_state, run_gh_command_any_status};

/// How long a linked PR stays fresh while its session is in use: checks finish within minutes.
pub(crate) const ACTIVE_PULL_REQUEST_MAX_AGE: Duration = Duration::from_secs(120);
pub(crate) const ACTIVE_ISSUE_MAX_AGE: Duration = Duration::from_secs(5 * 60);
/// A session idle for longer than this has its links read less often.
pub(crate) const IDLE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
pub(crate) const IDLE_MAX_AGE: Duration = Duration::from_secs(30 * 60);
/// A links list someone just opened: read at once, but not again for this long.
pub(crate) const WANTED_MAX_AGE: Duration = Duration::from_secs(20);
/// A read that got no answer (a PR or issue that is gone, not visible to the signed-in account,
/// or a number of the other kind) is tried again after this long.
///
/// CDXC:SessionLinks 2026-11-05 WHY:
/// A read without an answer counted as never read, and never-read links are read on every pass, so each such link cost the batch call plus one `gh` alone every minute, for as long as its session stayed open (found while tracing about 11 `gh` a minute from an idle Windows gxserver, 2026-11-05).
pub(crate) const FAILED_READ_RETRY: Duration = Duration::from_secs(10 * 60);
/// PRs and issues one GraphQL call asks for.
const BATCH_SIZE: usize = 40;

/// One PR or issue a pass wants read, asked for the way its caller looks it up: by URL (`cwd`
/// empty, `selector` the URL) or by number from a checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GithubRead {
    /// `owner/repo`.
    pub(crate) repo: String,
    pub(crate) number: u64,
    pub(crate) pull_request: bool,
    pub(crate) cwd: String,
    pub(crate) selector: String,
}

impl GithubRead {
    /// A read named by URL (session links), or `None` for anything but a PR or issue URL.
    pub(crate) fn of_url(url: &str, pull_request: bool) -> Option<Self> {
        let (repo, number) = repo_and_number_of_url(url)?;
        Some(Self {
            repo,
            number,
            pull_request,
            cwd: String::new(),
            selector: url.to_string(),
        })
    }

    fn key(&self) -> String {
        cache_key(&self.cwd, &self.selector)
    }
}

/// `owner/repo` and the number of `https://github.com/owner/repo/(pull|issues)/N`.
pub(crate) fn repo_and_number_of_url(url: &str) -> Option<(String, u64)> {
    let path = url.split_once("github.com/")?.1.split(['?', '#']).next()?;
    let mut parts = path.split('/');
    let (owner, name, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if !matches!(kind, "pull" | "issues") {
        return None;
    }
    Some((format!("{owner}/{name}"), number.parse().ok()?))
}

/// How fresh a link must be: idle sessions' links every 30 minutes, a list just opened at once.
pub(crate) fn github_read_max_age(pull_request: bool, idle: bool, wanted: bool) -> Duration {
    if wanted {
        WANTED_MAX_AGE
    } else if idle {
        IDLE_MAX_AGE
    } else if pull_request {
        ACTIVE_PULL_REQUEST_MAX_AGE
    } else {
        ACTIVE_ISSUE_MAX_AGE
    }
}

/// Whether a session counts as idle: no activity for more than a day (`lastActiveAt`).
pub(crate) fn session_is_idle(session: &Value, now: SystemTime) -> bool {
    let Some(last) = session
        .get("lastActiveAt")
        .and_then(Value::as_str)
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
    else {
        return false;
    };
    let last = UNIX_EPOCH + Duration::from_secs(last.timestamp().max(0) as u64);
    now.duration_since(last).is_ok_and(|age| age > IDLE_AFTER)
}

/// Whether the read was never made or is older than `max_age`.
pub(crate) fn github_read_due(read: &GithubRead, max_age: Duration) -> bool {
    let key = read.key();
    let Ok(cache) = cache().lock() else {
        return false;
    };
    let fetched_at = if read.pull_request {
        cache
            .pull_requests
            .get(&key)
            .map(|cached| cached.fetched_at)
    } else {
        cache.issues.get(&key).map(|cached| cached.fetched_at)
    };
    fetched_at.is_none_or(|at| at.elapsed() >= max_age)
}

/// Whether a read is due among the links a pass follows: one never tried at once, one tried
/// without an answer after `FAILED_READ_RETRY`, and an answered one after `max_age` (`None`: read
/// once).
pub(crate) fn github_link_read_due(read: &GithubRead, max_age: Option<Duration>) -> bool {
    if !github_read_tried(read) {
        return true;
    }
    if !github_read_known(read) {
        return github_read_due(read, FAILED_READ_RETRY);
    }
    max_age.is_some_and(|max_age| github_read_due(read, max_age))
}

/// Whether the read was made at all, answered or not.
fn github_read_tried(read: &GithubRead) -> bool {
    let key = read.key();
    cache().lock().ok().is_some_and(|cache| {
        if read.pull_request {
            cache.pull_requests.contains_key(&key)
        } else {
            cache.issues.contains_key(&key)
        }
    })
}

/// Whether the read has an answer yet (a PR or issue it found).
pub(crate) fn github_read_known(read: &GithubRead) -> bool {
    let key = read.key();
    cache().lock().ok().is_some_and(|cache| {
        if read.pull_request {
            cache
                .pull_requests
                .get(&key)
                .is_some_and(|cached| cached.value.is_some())
        } else {
            cache
                .issues
                .get(&key)
                .is_some_and(|cached| cached.value.is_some())
        }
    })
}

/// Reads `reads` (already due, in the order they matter) with at most `budget` `gh` calls: one
/// GraphQL call per repo and 40 reads, then alone what it could not answer. Returns the calls made.
pub(crate) fn refresh_github_reads(reads: &[GithubRead], budget: usize) -> usize {
    let mut by_repo: BTreeMap<String, Vec<&GithubRead>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for read in reads {
        if !by_repo.contains_key(&read.repo) {
            order.push(read.repo.clone());
        }
        by_repo.entry(read.repo.clone()).or_default().push(read);
    }
    let mut calls = 0;
    let mut alone: Vec<&GithubRead> = Vec::new();
    for repo in order {
        let Some(repo_reads) = by_repo.get(&repo) else {
            continue;
        };
        let Some((owner, name)) = graphql_repo(&repo) else {
            alone.extend(repo_reads.iter().copied());
            continue;
        };
        for chunk in repo_reads.chunks(BATCH_SIZE) {
            if calls >= budget {
                return calls;
            }
            let query = batch_query(owner, name, chunk);
            calls += 1;
            let answer = run_gh_command_any_status(
                None,
                &["api", "graphql", "-f", &format!("query={query}")],
            )
            .and_then(|output| serde_json::from_str::<Value>(output.trim()).ok());
            let repository = answer
                .as_ref()
                .and_then(|answer| answer.get("data"))
                .and_then(|data| data.get("repository"))
                .filter(|repository| repository.is_object());
            // No repository at all (the call failed or timed out, GitHub refused it, or the repo
            // is not visible): reading each one alone would fail the same way, one `gh` each.
            let Some(repository) = repository else {
                for read in chunk {
                    note_failed_read(read);
                }
                continue;
            };
            for (index, read) in chunk.iter().enumerate() {
                if !store_batch_node(read, repository.get(alias(index))) {
                    alone.push(read);
                }
            }
        }
    }
    for read in alone {
        if calls >= budget {
            break;
        }
        calls += 1;
        if read.pull_request {
            force_single_pull_request(read);
        } else {
            refresh_issue_rest(read);
        }
    }
    calls
}

/// The owner and name GraphQL may take as string literals.
fn graphql_repo(repo: &str) -> Option<(&str, &str)> {
    let (owner, name) = repo.split_once('/')?;
    let plain = |text: &str| {
        !text.is_empty()
            && text
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    };
    (plain(owner) && plain(name)).then_some((owner, name))
}

fn alias(index: usize) -> String {
    format!("r{index}")
}

/// One query for a repo's PRs and issues: `pullRequest` for a PR, `issue` for an issue, each
/// under an alias that maps back to its read.
pub(crate) fn batch_query(owner: &str, name: &str, reads: &[&GithubRead]) -> String {
    let mut fields = String::new();
    for (index, read) in reads.iter().enumerate() {
        let alias = alias(index);
        if read.pull_request {
            fields.push_str(&format!(
                "{alias}: pullRequest(number: {}) {{ number state isDraft url title reviewDecision headRefName \
                 closingIssuesReferences(first: 20) {{ nodes {{ url }} }} \
                 commits(last: 1) {{ nodes {{ commit {{ statusCheckRollup {{ contexts(first: 100) \
                 {{ nodes {{ __typename ... on CheckRun {{ status conclusion }} \
                 ... on StatusContext {{ state }} }} }} }} }} }} }} }} ",
                read.number
            ));
        } else {
            fields.push_str(&format!(
                "{alias}: issue(number: {}) {{ number title state url }} ",
                read.number
            ));
        }
    }
    format!("query {{ repository(owner: \"{owner}\", name: \"{name}\") {{ {fields}}} }}")
}

/// A GraphQL PR in the shape `gh pr view --json` gives, for `parse_pull_request_value`.
pub(crate) fn pull_request_value_of_node(node: &Value) -> Value {
    let rollup: Vec<Value> = node
        .pointer("/commits/nodes/0/commit/statusCheckRollup/contexts/nodes")
        .and_then(Value::as_array)
        .map(|contexts| {
            contexts
                .iter()
                .map(|context| {
                    json!({
                        "status": context.get("status").cloned().unwrap_or(Value::Null),
                        "conclusion": context.get("conclusion").cloned().unwrap_or(Value::Null),
                        "state": context.get("state").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "number": node.get("number"),
        "state": node.get("state"),
        "isDraft": node.get("isDraft"),
        "url": node.get("url"),
        "title": node.get("title"),
        "reviewDecision": node.get("reviewDecision"),
        "headRefName": node.get("headRefName"),
        "statusCheckRollup": rollup,
        "closingIssuesReferences": node
            .pointer("/closingIssuesReferences/nodes")
            .cloned()
            .unwrap_or_else(|| json!([])),
    })
}

pub(crate) fn issue_of_node(node: &Value) -> Option<WorkGithubIssueInfo> {
    Some(WorkGithubIssueInfo {
        number: node.get("number").and_then(Value::as_u64)?,
        title: node
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        state: node
            .get("state")
            .and_then(Value::as_str)
            .map(|state| state.to_ascii_lowercase()),
        url: node
            .get("url")
            .or_else(|| node.get("html_url"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Stores what the batch said about one read; `false` when it said nothing usable.
fn store_batch_node(read: &GithubRead, node: Option<&Value>) -> bool {
    let Some(node) = node.filter(|node| node.is_object()) else {
        return false;
    };
    let Ok(mut cache) = cache().lock() else {
        return true;
    };
    if read.pull_request {
        let Some(info) = parse_pull_request_value(&pull_request_value_of_node(node)) else {
            return false;
        };
        cache
            .pull_requests
            .insert(read.key(), Cached::now(Some(info)));
    } else {
        let Some(info) = issue_of_node(node) else {
            return false;
        };
        cache.issues.insert(read.key(), Cached::now(Some(info)));
    }
    true
}

/// A read that could not be made: what was known stays and the read waits a full period
/// (`refresh_issue_rest` does the same for a failed REST read).
fn note_failed_read(read: &GithubRead) {
    let Ok(mut cache) = cache().lock() else {
        return;
    };
    let key = read.key();
    if read.pull_request {
        match cache.pull_requests.get_mut(&key) {
            Some(cached) => cached.fetched_at = Instant::now(),
            None => {
                cache.pull_requests.insert(key, Cached::now(None));
            }
        }
    } else {
        match cache.issues.get_mut(&key) {
            Some(cached) => cached.fetched_at = Instant::now(),
            None => {
                cache.issues.insert(key, Cached::now(None));
            }
        }
    }
}

/// A PR the batch could not answer, read alone even when its cache entry is fresh.
fn force_single_pull_request(read: &GithubRead) {
    if let Ok(mut cache) = cache().lock() {
        cache.pull_requests.remove(&read.key());
    }
    refresh_work_pull_request(&read.cwd, &read.selector);
}

/// What `gh api -i` printed: the status code, the ETag and the body.
pub(crate) fn parse_rest_response(output: &str) -> Option<(u16, Option<String>, &str)> {
    let output = output.trim_start();
    let status: u16 = output
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    let (head, body) = output
        .split_once("\r\n\r\n")
        .or_else(|| output.split_once("\n\n"))
        .unwrap_or((output, ""));
    let etag = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("etag")
            .then(|| value.trim().to_string())
    });
    Some((status, etag, body))
}

/// An issue read alone through REST, sending the ETag of the last read: a 304 only marks it fresh
/// (and costs no rate limit).
fn refresh_issue_rest(read: &GithubRead) {
    let key = read.key();
    let etag = cache().lock().ok().and_then(|cache| {
        cache
            .issues
            .get(&key)
            .and_then(|cached| cached.etag.clone())
    });
    let path = format!("repos/{}/issues/{}", read.repo, read.number);
    let header = etag.as_ref().map(|etag| format!("If-None-Match: {etag}"));
    let mut args = vec!["api", "-i"];
    if let Some(header) = header.as_deref() {
        args.extend(["-H", header]);
    }
    args.push(&path);
    let output = run_gh_command_any_status(None, &args);
    let Ok(mut cache) = cache().lock() else {
        return;
    };
    match output.as_deref().and_then(parse_rest_response) {
        Some((304, _, _)) => {
            if let Some(cached) = cache.issues.get_mut(&key) {
                cached.fetched_at = Instant::now();
                cached.read_at = SystemTime::now();
            }
        }
        Some((200, etag, body)) => {
            let info = serde_json::from_str::<Value>(body.trim())
                .ok()
                .and_then(|value| issue_of_node(&value));
            let mut cached = Cached::now(info);
            cached.etag = etag;
            cache.issues.insert(key, cached);
        }
        _ => {
            // A failed read keeps what was known and waits a full period before trying again.
            match cache.issues.get_mut(&key) {
                Some(cached) => cached.fetched_at = Instant::now(),
                None => {
                    cache.issues.insert(key, Cached::now(None));
                }
            }
        }
    }
}

/// When a read was made (seconds since the epoch) and its ETag, for `sessionLinks.read`.
pub(crate) fn github_read_meta(read: &GithubRead) -> Option<(u64, Option<String>)> {
    let key = read.key();
    let cache = cache().lock().ok()?;
    let (read_at, etag) = if read.pull_request {
        let cached = cache.pull_requests.get(&key)?;
        (cached.read_at, cached.etag.clone())
    } else {
        let cached = cache.issues.get(&key)?;
        (cached.read_at, cached.etag.clone())
    };
    Some((read_at.duration_since(UNIX_EPOCH).ok()?.as_secs(), etag))
}

/// Puts back what `sessionLinks.read` kept about a read (its fields as the lists show them, when
/// it was read and its ETag) when nothing newer is cached, so a restart reads only what is due.
pub(crate) fn seed_github_read(read: &GithubRead, snapshot: &Map<String, Value>) {
    let Some(read_secs) = snapshot.get("readAt").and_then(Value::as_u64) else {
        return;
    };
    let read_at = UNIX_EPOCH + Duration::from_secs(read_secs);
    let age = SystemTime::now()
        .duration_since(read_at)
        .unwrap_or_default();
    let Some(fetched_at) = Instant::now().checked_sub(age) else {
        return;
    };
    let text = |field: &str| {
        snapshot
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let etag = text("etag");
    let key = read.key();
    let Ok(mut cache) = cache().lock() else {
        return;
    };
    if read.pull_request {
        if cache.pull_requests.contains_key(&key) {
            return;
        }
        let state = text("state").and_then(|state| match state.as_str() {
            "draft" => parse_gh_pull_request_state("OPEN", true),
            other => parse_gh_pull_request_state(other, false),
        });
        let Some(state) = state else {
            return;
        };
        let info = WorkPullRequestInfo {
            number: read.number,
            state,
            url: text("url"),
            checks: match text("checks").as_deref() {
                Some("passing") => Some("passing"),
                Some("failing") => Some("failing"),
                Some("pending") => Some("pending"),
                _ => None,
            },
            title: text("title"),
            review: match text("review").as_deref() {
                Some("reviewRequired") => Some("reviewRequired"),
                Some("approved") => Some("approved"),
                Some("changesRequested") => Some("changesRequested"),
                _ => None,
            },
            closing_issues: Vec::new(),
            head_ref: text("branch"),
        };
        cache.pull_requests.insert(
            key,
            Cached {
                value: Some(info),
                fetched_at,
                read_at,
                etag,
            },
        );
    } else {
        if cache.issues.contains_key(&key) {
            return;
        }
        let info = WorkGithubIssueInfo {
            number: read.number,
            title: text("title"),
            state: text("state"),
            url: text("url"),
        };
        cache.issues.insert(
            key,
            Cached {
                value: Some(info),
                fetched_at,
                read_at,
                etag,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_git_status::PullRequestState;

    fn read(number: u64, pull_request: bool) -> GithubRead {
        let kind = if pull_request { "pull" } else { "issues" };
        GithubRead::of_url(
            &format!("https://github.com/maddada/ghostex/{kind}/{number}"),
            pull_request,
        )
        .unwrap()
    }

    #[test]
    fn urls_name_their_repo_and_number() {
        assert_eq!(
            repo_and_number_of_url("https://github.com/maddada/ghostex/pull/286"),
            Some(("maddada/ghostex".to_string(), 286))
        );
        assert_eq!(
            repo_and_number_of_url("https://github.com/maddada/ghostex/issues/283#top"),
            Some(("maddada/ghostex".to_string(), 283))
        );
        assert_eq!(
            repo_and_number_of_url("https://github.com/maddada/ghostex"),
            None
        );
    }

    #[test]
    fn idle_sessions_read_every_thirty_minutes_and_an_opened_list_at_once() {
        assert_eq!(
            github_read_max_age(true, false, false),
            Duration::from_secs(120)
        );
        assert_eq!(
            github_read_max_age(false, false, false),
            Duration::from_secs(300)
        );
        assert_eq!(
            github_read_max_age(true, true, false),
            Duration::from_secs(1800)
        );
        assert_eq!(github_read_max_age(false, true, true), WANTED_MAX_AGE);
        let now = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
        let at = |secs_ago: i64| json!({ "lastActiveAt": chrono::DateTime::from_timestamp(2_000_000_000 - secs_ago, 0).unwrap().to_rfc3339() });
        assert!(!session_is_idle(&at(3600), now));
        assert!(session_is_idle(&at(2 * 24 * 3600), now));
        assert!(!session_is_idle(&json!({}), now));
    }

    #[test]
    fn one_query_asks_for_every_pr_and_issue_of_a_repo() {
        let reads = [read(286, true), read(283, false)];
        let query = batch_query("maddada", "ghostex", &reads.iter().collect::<Vec<_>>());
        assert!(query.starts_with("query { repository(owner: \"maddada\", name: \"ghostex\")"));
        assert!(query.contains("r0: pullRequest(number: 286)"));
        assert!(query.contains("closingIssuesReferences"));
        assert!(query.contains("statusCheckRollup"));
        assert!(query.contains("r1: issue(number: 283) { number title state url }"));
        assert_eq!(graphql_repo("bad\"name/x"), None);
    }

    #[test]
    fn a_graphql_pr_reads_like_gh_pr_view() {
        let node = json!({
            "number": 286, "state": "OPEN", "isDraft": false,
            "url": "https://github.com/maddada/Ghostex/pull/286", "title": "feat: x",
            "reviewDecision": "CHANGES_REQUESTED",
            "headRefName": "feat/turn-orchestrator",
            "closingIssuesReferences": { "nodes": [{ "url": "https://github.com/maddada/Ghostex/issues/283" }] },
            "commits": { "nodes": [{ "commit": { "statusCheckRollup": { "contexts": { "nodes": [
                { "__typename": "CheckRun", "status": "COMPLETED", "conclusion": "SUCCESS" },
                { "__typename": "StatusContext", "state": "PENDING" }
            ] } } } }] }
        });
        let info = parse_pull_request_value(&pull_request_value_of_node(&node)).unwrap();
        assert_eq!(info.state, PullRequestState::Open);
        assert_eq!(info.checks, Some("pending"));
        assert_eq!(info.review, Some("changesRequested"));
        assert_eq!(info.head_ref.as_deref(), Some("feat/turn-orchestrator"));
        assert_eq!(info.title.as_deref(), Some("feat: x"));
        assert_eq!(
            info.closing_issues,
            vec!["https://github.com/maddada/Ghostex/issues/283".to_string()]
        );
        let merged =
            json!({ "number": 1, "state": "MERGED", "isDraft": false, "commits": { "nodes": [] } });
        let info = parse_pull_request_value(&pull_request_value_of_node(&merged)).unwrap();
        assert_eq!(info.state, PullRequestState::Merged);
        assert_eq!(info.checks, None);
    }

    #[test]
    fn a_304_is_read_from_the_status_line() {
        let not_modified =
            "HTTP/2.0 304 Not Modified\r\nEtag: \"abc\"\r\nServer: github.com\r\n\r\n";
        let (status, etag, body) = parse_rest_response(not_modified).unwrap();
        assert_eq!((status, etag.as_deref(), body), (304, Some("\"abc\""), ""));
        let ok = "HTTP/2.0 200 OK\nEtag: W/\"def\"\n\n{\"number\": 283, \"state\": \"closed\", \"title\": \"t\", \"html_url\": \"u\"}";
        let (status, etag, body) = parse_rest_response(ok).unwrap();
        assert_eq!((status, etag.as_deref()), (200, Some("W/\"def\"")));
        let issue = issue_of_node(&serde_json::from_str(body).unwrap()).unwrap();
        assert_eq!(issue.state.as_deref(), Some("closed"));
        assert_eq!(issue.url.as_deref(), Some("u"));
    }

    #[test]
    fn a_restart_puts_back_what_was_read_and_when() {
        let pr = GithubRead::of_url("https://github.com/test-seed/repo/pull/7", true).unwrap();
        let read_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 60;
        let snapshot =
            json!({ "state": "merged", "title": "Seeded", "readAt": read_at, "etag": "\"e\"" });
        seed_github_read(&pr, snapshot.as_object().unwrap());
        assert!(github_read_known(&pr));
        // Read a minute ago: fresh for an active session's two minutes, due for an opened list.
        assert!(!github_read_due(&pr, ACTIVE_PULL_REQUEST_MAX_AGE));
        assert!(github_read_due(&pr, WANTED_MAX_AGE));
        assert_eq!(
            github_read_meta(&pr),
            Some((read_at, Some("\"e\"".to_string())))
        );
    }
}
