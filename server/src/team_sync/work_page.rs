//! The Work page's team data from a Work workspace's Convex project (`packages/team-sync/convex/
//! workPage.ts`): each list row's Slack-thread count (one batched query per workspace) and one
//! ticket's Slack threads, working thread, validation post and the sessions the team runs on it.
//! Blocking; a workspace with no team connection reads as `None` and the page shows nothing new.
//!
//! CDXC:WorkMode 2026-10-09 WHY:
//! The page reaches this through `/api/listWorkItems` and `/api/readWorkItem` instead of its own
//! calls: gxserver already knows each project's workspace and holds the member token, so the
//! rows and the details arrive with their Slack data in one answer, cached like the Linear and
//! `gh` feeds.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;
use crate::work_mode::{media_embed, pull_request_url_parts, TeamTicketFacts, WorkProjectInput};

use super::connections::{read_team_connection, TeamConnection};
use super::convex_http::ConvexCallKind;
use super::operations::member_call;

/// A row's count is re-read after this long (the list itself re-reads every 90 seconds).
const SUMMARY_TTL: Duration = Duration::from_secs(60);
/// Re-opening a ticket within this long shows what was read a moment ago, like its Linear data.
const DETAILS_TTL: Duration = Duration::from_secs(45);

/// The ticket key the team's Convex project stores: `SPX-1234`, or `owner/repo#12` (lower-case
/// repo) for a GitHub issue or PR (`packages/team-sync/convex/lib/tickets.ts`).
pub(crate) fn team_ticket_key(item: &Value) -> Option<String> {
    if let Some(identifier) = item.get("linearIssue").and_then(Value::as_str) {
        return Some(identifier.to_ascii_uppercase());
    }
    if item.get("githubIssue").and_then(Value::as_u64).is_some() {
        let key = item.get("key").and_then(Value::as_str)?;
        let repo_and_number = key.strip_prefix("issue:")?;
        return repo_and_number
            .contains('/')
            .then(|| repo_and_number.to_ascii_lowercase());
    }
    let url = item.pointer("/pullRequest/url").and_then(Value::as_str)?;
    let (repo, number) = pull_request_url_parts(url)?;
    Some(format!("{}#{number}", repo.to_ascii_lowercase()))
}

fn number(value: &Value, pointer: &str) -> u64 {
    // Convex's JSON format sends every number as a float.
    value
        .pointer(pointer)
        .and_then(|value| value.as_u64().or_else(|| value.as_f64().map(|f| f as u64)))
        .unwrap_or(0)
}

/// The text a failed call shows; an older deployment lacks these functions.
pub(super) fn team_error(connection: &TeamConnection, error: String) -> String {
    let team = connection.team_name.as_deref().unwrap_or("the team");
    if error.contains("Could not find public function") {
        format!("Team {team}: its Convex functions are older than this Ghostex. Run `ghostex team deploy` to update them.")
    } else {
        format!("Team {team}: {error}")
    }
}

struct CachedSummary {
    /// `None`: the team has nothing for the ticket.
    summary: Option<Value>,
    fetched_at: Instant,
}

#[derive(Default)]
struct TeamCache {
    /// By `(workspace id, ticket)`.
    summaries: HashMap<(String, String), CachedSummary>,
    /// The last failure per workspace, so a team that cannot be reached is not asked on every read.
    summary_errors: HashMap<String, (String, Instant)>,
    details: HashMap<(String, String), (Result<Value, String>, Instant)>,
}

fn cache() -> &'static Mutex<TeamCache> {
    static CACHE: OnceLock<Mutex<TeamCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(TeamCache::default()))
}

/// The tickets of a list, grouped by the workspace of each row's project.
pub(crate) fn team_tickets_by_workspace(
    projects: &[WorkProjectInput],
    list: &Value,
) -> Vec<(String, Vec<String>)> {
    let mut groups: HashMap<String, HashSet<String>> = HashMap::new();
    for item in list
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(workspace_id) = item
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|id| projects.iter().find(|input| input.project_id == id))
            .map(|input| input.workspace_id.clone())
        else {
            continue;
        };
        if let Some(ticket) = team_ticket_key(item) {
            groups.entry(workspace_id).or_default().insert(ticket);
        }
    }
    let mut groups: Vec<(String, Vec<String>)> = groups
        .into_iter()
        .map(|(workspace, tickets)| {
            let mut tickets: Vec<String> = tickets.into_iter().collect();
            tickets.sort();
            (workspace, tickets)
        })
        .collect();
    groups.sort();
    groups
}

/// Reads the summaries the cache lacks (or all of them with `force`), one call per connected
/// workspace. Workspaces with no team are skipped without a call.
pub(crate) fn refresh_team_ticket_summaries(
    paths: &GxserverPaths,
    groups: &[(String, Vec<String>)],
    force: bool,
) {
    for (workspace_id, tickets) in groups {
        let Some(connection) = read_team_connection(paths, workspace_id) else {
            continue;
        };
        let wanted: Vec<String> = {
            let Ok(cache) = cache().lock() else { return };
            if !force
                && cache
                    .summary_errors
                    .get(workspace_id)
                    .is_some_and(|(_, at)| at.elapsed() < SUMMARY_TTL)
            {
                continue;
            }
            tickets
                .iter()
                .filter(|ticket| {
                    force
                        || !cache
                            .summaries
                            .get(&(workspace_id.clone(), (*ticket).clone()))
                            .is_some_and(|cached| cached.fetched_at.elapsed() < SUMMARY_TTL)
                })
                .cloned()
                .collect()
        };
        if wanted.is_empty() {
            continue;
        }
        let mut args = Map::new();
        args.insert("tickets".to_string(), json!(wanted));
        let answer = member_call(
            &connection,
            ConvexCallKind::Query,
            "workPage:ticketSummaries",
            args,
        );
        let Ok(mut cache) = cache().lock() else {
            return;
        };
        match answer {
            Ok(summaries) => {
                cache.summary_errors.remove(workspace_id);
                let now = Instant::now();
                for ticket in wanted {
                    let summary = summaries.get(&ticket).cloned();
                    cache.summaries.insert(
                        (workspace_id.clone(), ticket),
                        CachedSummary {
                            summary,
                            fetched_at: now,
                        },
                    );
                }
            }
            Err(error) => {
                cache.summary_errors.insert(
                    workspace_id.clone(),
                    (team_error(&connection, error), Instant::now()),
                );
            }
        }
    }
}

/// Sets `slackThreadCount` on every row the cache has a summary for, and adds a team's error.
pub(crate) fn apply_team_ticket_summaries(
    paths: &GxserverPaths,
    projects: &[WorkProjectInput],
    list: &mut Value,
) {
    // A workspace disconnected since its summaries were cached shows nothing.
    let connected: HashSet<String> = super::connections::read_team_connections(paths)
        .into_iter()
        .map(|connection| connection.workspace_id)
        .collect();
    if connected.is_empty() {
        return;
    }
    let Ok(cache) = cache().lock() else { return };
    let mut errors = Vec::new();
    if let Some(items) = list.get_mut("items").and_then(Value::as_array_mut) {
        for item in items.iter_mut() {
            let workspace_id = item
                .get("projectId")
                .and_then(Value::as_str)
                .and_then(|id| projects.iter().find(|input| input.project_id == id))
                .map(|input| input.workspace_id.clone())
                .filter(|workspace_id| connected.contains(workspace_id));
            let (Some(workspace_id), Some(ticket)) = (workspace_id, team_ticket_key(item)) else {
                continue;
            };
            if let Some((error, _)) = cache.summary_errors.get(&workspace_id) {
                if !errors.contains(error) {
                    errors.push(error.clone());
                }
            }
            let count = cache
                .summaries
                .get(&(workspace_id, ticket))
                .and_then(|cached| cached.summary.as_ref())
                .map(|summary| number(summary, "/threadCount"))
                .unwrap_or(0);
            if count > 0 {
                item["slackThreadCount"] = json!(count);
            }
        }
    }
    if let Some(list_errors) = list.get_mut("errors").and_then(Value::as_array_mut) {
        list_errors.extend(errors.into_iter().map(Value::String));
    }
}

/// Drops what is cached for one ticket, so the next read shows a change this computer just made.
pub(super) fn forget_team_ticket(workspace_id: &str, ticket: &str) {
    let key = (workspace_id.to_string(), ticket.to_string());
    if let Ok(mut cache) = cache().lock() {
        cache.summaries.remove(&key);
        cache.details.remove(&key);
    }
}

/// One ticket's team data: `None` when the workspace has no team connection.
pub(crate) fn read_team_ticket(
    paths: &GxserverPaths,
    workspace_id: &str,
    ticket: &str,
    force: bool,
) -> Option<Result<Value, String>> {
    let connection = read_team_connection(paths, workspace_id)?;
    let key = (workspace_id.to_string(), ticket.to_string());
    if !force {
        if let Some((value, _)) = cache().lock().ok().and_then(|cache| {
            cache
                .details
                .get(&key)
                .filter(|(_, at)| at.elapsed() < DETAILS_TTL)
                .cloned()
        }) {
            return Some(value);
        }
    }
    let mut args = Map::new();
    args.insert("ticket".to_string(), json!(ticket));
    let value = member_call(
        &connection,
        ConvexCallKind::Action,
        "workPage:ticketDetails",
        args,
    )
    .map(|mut details| {
        add_message_media(&mut details);
        details["teamName"] = json!(connection.team_name);
        details
    })
    .map_err(|error| team_error(&connection, error));
    if let Ok(mut cache) = cache().lock() {
        cache
            .details
            .retain(|_, (_, at)| at.elapsed() < DETAILS_TTL);
        cache.details.insert(key, (value.clone(), Instant::now()));
    }
    Some(value)
}

/// Every Loom, YouTube or video link of a message as a player the page shows under it.
fn add_message_media(details: &mut Value) {
    let Some(threads) = details.get_mut("threads").and_then(Value::as_array_mut) else {
        return;
    };
    for message in threads
        .iter_mut()
        .filter_map(|thread| thread.get_mut("messages").and_then(Value::as_array_mut))
        .flatten()
    {
        let text = message
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut seen = Vec::new();
        let mut media = Vec::new();
        for word in text.split(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '|')) {
            if !word.starts_with("http://") && !word.starts_with("https://") {
                continue;
            }
            if let Some(embed) = media_embed(word) {
                let key = embed["embedUrl"].as_str().unwrap_or_default().to_string();
                if !seen.contains(&key) {
                    seen.push(key);
                    media.push(embed);
                }
            }
        }
        message["media"] = Value::Array(media);
    }
}

/// The team data the details page draws: the threads as Convex sent them, and only the team
/// sessions the ticket's local rows do not already show (a session on this computer, or a cloud
/// session this computer started, is listed once, as yours).
pub(crate) fn team_for_page(
    mut team: Value,
    local_session_ids: &[&str],
    local_cloud_urls: &[&str],
) -> Value {
    if let Some(sessions) = team.get_mut("sessions").and_then(Value::as_array_mut) {
        sessions.retain(|session| {
            let local = session
                .get("sessionId")
                .and_then(Value::as_str)
                .is_some_and(|id| local_session_ids.contains(&id));
            let cloud = session
                .get("sessionUrl")
                .and_then(Value::as_str)
                .is_some_and(|url| local_cloud_urls.contains(&url));
            !local && !cloud
        });
    }
    team["connected"] = json!(true);
    team
}

/// What the team-flow rules read from a ticket's team data.
pub(crate) fn team_ticket_facts(team: &Value) -> TeamTicketFacts {
    let text = |pointer: &str| {
        team.pointer(pointer)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let channel_name = |channel_id: Option<String>| {
        let channel_id = channel_id?;
        team.get("threads")
            .and_then(Value::as_array)?
            .iter()
            .find(|thread| thread.get("channelId").and_then(Value::as_str) == Some(&channel_id))
            .and_then(|thread| thread.get("channelName").and_then(Value::as_str))
            .map(|name| format!("#{name}"))
    };
    let validation = team
        .pointer("/summary/validation")
        .filter(|value| value.is_object());
    let validation_detail =
        validation.map(
            |validation| match validation.get("source").and_then(Value::as_str) {
                Some("finalPost") => "result posted".to_string(),
                _ => channel_name(text("/summary/validation/channelId"))
                    .unwrap_or_else(|| "posted".to_string()),
            },
        );
    TeamTicketFacts {
        has_working_thread: team
            .pointer("/summary/workingThread")
            .is_some_and(Value::is_object),
        working_thread_channel: channel_name(text("/summary/workingThread/channelId")),
        working_thread_url: text("/summary/workingThread/permalink"),
        validation_posted: validation.is_some(),
        validation_detail,
        validation_url: text("/summary/validation/permalink"),
    }
}
