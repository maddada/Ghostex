//! `/api/beadsView`'s snapshot: bd's `ready` and `list` answers, read together and published
//! only when they describe the same moment.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ghostex_gx_protocol::beads_view::{
    capability, BeadsReadiness, BeadsViewRow, BeadsViewSnapshot, BeadsWorkspaceInfo,
    BeadsWorkspaceState,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::typed_operations::{
    beads_action_command, run_process_command_with_timeout, CommandOutput, ProcessCommand,
    TypedOperationContext,
};

use super::cache::{still_current, Published, WorkspaceEntry};
use super::change::ChangeToken;
use super::discovery::WorkspaceInfo;
use super::reads::{cached_reply, current_token, store_list_replies, workspace_mode};

/// Each bd read in a refresh, lock wait included, gives up after this long.
const READ_TIMEOUT: Duration = Duration::from_secs(15);
/// One refresh, waiting for the workspace lock included, gives up after this long.
const REFRESH_BUDGET: Duration = Duration::from_secs(15);
/// Readiness that bd could not give, or gave inconsistently, is asked again this often.
const READINESS_RETRY: Duration = Duration::from_secs(10);
/// A side read (the remotes) that failed is retried this often.
const SIDE_FACT_RETRY: Duration = Duration::from_secs(30);
const EMBEDDED_ATTEMPTS: usize = 3;

/// CDXC:ProjectBoard 2026-11-11 WHY:
/// A view poll never waits on bd for long: a refresh runs as its own background task under the workspace lock and one 15-second budget (lock wait, `ready`, `list` and the side reads together), and the poll waits at most 1.5 seconds for it before answering with what is published and `busy`. A sync or an agent holding the workspace therefore never stalls the view.
pub(crate) async fn read_snapshot(
    entry: Arc<WorkspaceEntry>,
    info: WorkspaceInfo,
    bd_executable_path: String,
    cwd: String,
    context: TypedOperationContext,
    known_revision: Option<String>,
    poll_deadline: tokio::time::Instant,
) -> BeadsViewSnapshot {
    let wanted = {
        let token = current_token(&info);
        needs_refresh(&entry, &token) || side_facts_missing(&entry)
    };
    let start = wanted && {
        let mut state = entry.state();
        let start = !state.refreshing;
        state.refreshing = true;
        start
    };
    if start {
        let task = tokio::spawn(run_refresh(
            entry.clone(),
            info.clone(),
            bd_executable_path,
            cwd,
            context,
        ));
        let _ = tokio::time::timeout_at(poll_deadline, task).await;
    } else if wanted {
        while entry.state().refreshing && tokio::time::Instant::now() < poll_deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    let busy = entry.state().refreshing || entry.lock_busy();
    build_reply(&entry, &info, busy, known_revision.as_deref())
}

async fn run_refresh(
    entry: Arc<WorkspaceEntry>,
    info: WorkspaceInfo,
    bd: String,
    cwd: String,
    context: TypedOperationContext,
) {
    let deadline = tokio::time::Instant::now() + REFRESH_BUDGET;
    // One cancellation deadline over everything done under the lock (its wait included): when it
    // passes, the work is dropped (bd and git children are killed) and the lock is let go.
    let work = async {
        let guard = entry.lock_owned().await;
        let token = current_token(&info);
        let error = if needs_refresh(&entry, &token) {
            refresh(&entry, &info, &bd, &cwd, &context, deadline).await
        } else {
            None
        };
        ensure_side_facts(&entry, &info, &bd, &cwd, &context, deadline).await;
        drop(guard);
        error
    };
    let error = tokio::time::timeout_at(deadline, work)
        .await
        .unwrap_or_else(|_| Some("Beads stayed busy. Showing what it last said.".to_string()));
    let mut state = entry.state();
    state.refreshing = false;
    state.last_error = error;
}

/// What is left of the refresh budget for one bd read, never more than a read's own limit.
fn budget(deadline: tokio::time::Instant) -> Duration {
    deadline
        .saturating_duration_since(tokio::time::Instant::now())
        .min(READ_TIMEOUT)
}

fn side_facts_missing(entry: &WorkspaceEntry) -> bool {
    let state = entry.state();
    state.actor.is_none()
        || (state.remote.is_none()
            && state
                .remote_failed_at
                .is_none_or(|failed| failed.elapsed() >= SIDE_FACT_RETRY))
}

fn needs_refresh(entry: &WorkspaceEntry, token: &Option<ChangeToken>) -> bool {
    let state = entry.state();
    // A refresh that just failed is not repeated on every poll: it waits out the readiness pause
    // unless the workspace changed since (a new token) or the user pressed Refresh (which clears
    // the failure).
    if let Some((failed_at, failed_token)) = &state.refresh_failed {
        if failed_at.elapsed() < READINESS_RETRY && failed_token == token {
            return false;
        }
    }
    let Some(published) = state.published.as_ref() else {
        return true;
    };
    if state.force || !still_current(&published.token, published.at, token, state.snapshot_stale) {
        return true;
    }
    if published.readiness != BeadsReadiness::Ok && published.at.elapsed() >= READINESS_RETRY {
        return true;
    }
    published
        .next_wake_at
        .is_some_and(|wake_at| Utc::now() >= wake_at)
}

fn bd_read(bd: &str, args: &[&str], cwd: &str) -> ProcessCommand {
    ProcessCommand::new(bd, args.to_vec(), cwd).with_env("BD_JSON_ENVELOPE", "1")
}

/// CDXC:ProjectBoard 2026-11-11 WHY:
/// `bd ready` can itself write (it wakes beads whose defer date passed), and another bd process can write between Ghostex's two reads, so a snapshot is published only when the change token read before `ready`, between the two reads and after `list` is the same; the first retry absorbs ready's own wake. A server-mode workspace has no token, so it reads once and cross-checks `ready` against `list`'s dependencies; when they disagree it publishes the rows without readiness ("paused") rather than retrying, which keeps it within two bd processes per refresh.
async fn refresh(
    entry: &WorkspaceEntry,
    info: &WorkspaceInfo,
    bd: &str,
    cwd: &str,
    context: &TypedOperationContext,
    deadline: tokio::time::Instant,
) -> Option<String> {
    let embedded = workspace_mode(info) == "embedded";
    let attempts = if embedded { EMBEDDED_ATTEMPTS } else { 1 };
    let ready_command = bd_read(bd, &["ready", "--explain", "--json", "--limit", "0"], cwd);
    let list_command = match beads_action_command("list", &Map::new(), context) {
        Ok(command) => command,
        Err(error) => return Some(error.message),
    };
    for _ in 0..attempts {
        let before = current_token(info);
        super::log::bd_call("refresh", "ready");
        let ready =
            run_process_command_with_timeout(&ready_command, context, budget(deadline)).await;
        let middle = current_token(info);
        super::log::bd_call("refresh", "list");
        let list = run_process_command_with_timeout(&list_command, context, budget(deadline)).await;
        let after = current_token(info);
        let list = match list {
            Ok(output) if output.exit_code == 0 && output.error.is_none() => output,
            Ok(output) => {
                entry.state().refresh_failed = Some((Instant::now(), current_token(info)));
                return Some(command_failure(&output));
            }
            Err(error) => {
                entry.state().refresh_failed = Some((Instant::now(), current_token(info)));
                return Some(error.message);
            }
        };
        if embedded && !(before == middle && middle == after) {
            continue;
        }
        let rows = match parse_rows(&list.stdout) {
            Ok(rows) => rows,
            Err(message) => {
                entry.state().refresh_failed = Some((Instant::now(), current_token(info)));
                return Some(message);
            }
        };
        let readiness = match ready {
            Ok(output) if output.exit_code == 0 && output.error.is_none() => {
                parse_readiness(&output.stdout)
            }
            _ => None,
        };
        store_list_replies(entry, context, &list, &after);
        publish(entry, &after, rows, readiness, embedded);
        let mut state = entry.state();
        state.settling = false;
        state.force = false;
        return None;
    }
    entry.state().settling = true;
    None
}

fn command_failure(output: &CommandOutput) -> String {
    if let Some(message) = output
        .error
        .as_ref()
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
    {
        return message.to_string();
    }
    let stderr = output.stderr.trim();
    if stderr.is_empty() {
        "Beads could not be read.".to_string()
    } else {
        stderr.to_string()
    }
}

fn unwrap_envelope(text: &str) -> Option<Value> {
    let parsed: Value = serde_json::from_str(text.trim()).ok()?;
    match parsed {
        Value::Object(mut object) if object.contains_key("data") => object.remove("data"),
        other => Some(other),
    }
}

/// bd's list rows, plus each row's `blocks` dependencies (`depends_on_id`s).
struct ListRow {
    row: BeadsViewRow,
    blocked_on: Vec<String>,
}

fn parse_rows(stdout: &str) -> Result<Vec<ListRow>, String> {
    let Some(Value::Array(items)) = unwrap_envelope(stdout) else {
        return Err("Beads printed a list Ghostex could not read.".to_string());
    };
    let text = |item: &Value, key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty())
    };
    Ok(items
        .iter()
        .filter(|item| item.is_object())
        .filter_map(|item| {
            let id = text(item, "id")?;
            let labels: Vec<String> = item
                .get("labels")
                .and_then(Value::as_array)
                .map(|labels| {
                    labels
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let status = text(item, "status").unwrap_or_else(|| "open".to_string());
            let blocked_on = item
                .get("dependencies")
                .and_then(Value::as_array)
                .map(|dependencies| {
                    dependencies
                        .iter()
                        .filter(|dependency| {
                            dependency.get("type").and_then(Value::as_str) == Some("blocks")
                        })
                        .filter_map(|dependency| text(dependency, "depends_on_id"))
                        .collect()
                })
                .unwrap_or_default();
            Some(ListRow {
                row: BeadsViewRow {
                    needs_decision: status != "closed"
                        && labels.iter().any(|label| label == "human"),
                    id,
                    title: text(item, "title").unwrap_or_default(),
                    priority: item.get("priority").and_then(Value::as_i64),
                    issue_type: text(item, "issue_type").unwrap_or_else(|| "task".to_string()),
                    assignee: text(item, "assignee"),
                    labels,
                    parent: text(item, "parent"),
                    defer_until: text(item, "defer_until"),
                    due_at: text(item, "due_at"),
                    updated_at: text(item, "updated_at").unwrap_or_default(),
                    closed_at: text(item, "closed_at"),
                    comment_count: item
                        .get("comment_count")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    status,
                    ready: None,
                    blocked_by: None,
                    blocks: Vec::new(),
                },
                blocked_on,
            })
        })
        .collect())
}

struct Readiness {
    ready: HashSet<String>,
    blocked: HashMap<String, Vec<String>>,
}

fn parse_readiness(stdout: &str) -> Option<Readiness> {
    let data = unwrap_envelope(stdout)?;
    let id_of = |value: &Value| {
        value
            .as_str()
            .map(str::to_string)
            .or_else(|| value.get("id").and_then(Value::as_str).map(str::to_string))
    };
    let ready = data
        .get("ready")?
        .as_array()?
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let blocked = data
        .get("blocked")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let id = row.get("id").and_then(Value::as_str)?.to_string();
                    let by = row
                        .get("blocked_by")
                        .and_then(Value::as_array)
                        .map(|by| by.iter().filter_map(id_of).collect())
                        .unwrap_or_default();
                    Some((id, by))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Readiness { ready, blocked })
}

/// Server mode: does bd's ready answer agree with the list's own facts? A ready bead must be one
/// bd counts as workable in the list (not closed, deferred, in progress or blocked) with no open
/// blocker; a blocked bead's blockers must name every open `blocks` dependency the list shows, and
/// each must still be open. Anything else means a write landed between the two reads.
fn readiness_agrees(rows: &[ListRow], readiness: &Readiness) -> bool {
    let by_id: HashMap<&str, &ListRow> =
        rows.iter().map(|row| (row.row.id.as_str(), row)).collect();
    let open = |id: &str| by_id.get(id).is_some_and(|row| row.row.status != "closed");
    let ready_ok = readiness.ready.iter().all(|id| {
        by_id.get(id.as_str()).is_some_and(|row| {
            !matches!(
                row.row.status.as_str(),
                "closed" | "deferred" | "in_progress" | "blocked"
            ) && !row.blocked_on.iter().any(|blocker| open(blocker))
                && !readiness.blocked.contains_key(id)
        })
    });
    let ancestors = |row: &ListRow| {
        let mut chain = Vec::new();
        let mut parent = row.row.parent.clone();
        while let Some(id) = parent.filter(|_| chain.len() < 32) {
            parent = by_id
                .get(id.as_str())
                .and_then(|row| row.row.parent.clone());
            chain.push(id);
        }
        chain
    };
    // Both directions: every open `blocks` dependency the list shows is reported, and every
    // reported blocker is still one of the row's `blocks` dependencies (or, as bd counts it, an
    // epic above the row), and still open.
    let blocked_ok = readiness.blocked.iter().all(|(id, by)| {
        by_id.get(id.as_str()).is_some_and(|row| {
            let above = ancestors(row);
            row.row.status != "closed"
                && by.iter().all(|blocker| {
                    open(blocker) && (row.blocked_on.contains(blocker) || above.contains(blocker))
                })
                && row
                    .blocked_on
                    .iter()
                    .filter(|blocker| open(blocker))
                    .all(|blocker| by.contains(blocker))
        })
    });
    ready_ok && blocked_ok
}

fn publish(
    entry: &WorkspaceEntry,
    token: &Option<ChangeToken>,
    rows: Vec<ListRow>,
    readiness: Option<Readiness>,
    embedded: bool,
) {
    let readiness_state = match &readiness {
        None => BeadsReadiness::Unavailable,
        Some(readiness) if !embedded && !readiness_agrees(&rows, readiness) => {
            BeadsReadiness::Paused
        }
        Some(_) => BeadsReadiness::Ok,
    };
    let mut blocks: HashMap<String, Vec<String>> = HashMap::new();
    for row in &rows {
        for blocker in &row.blocked_on {
            blocks
                .entry(blocker.clone())
                .or_default()
                .push(row.row.id.clone());
        }
    }
    let now = Utc::now();
    let mut next_wake_at: Option<DateTime<Utc>> = None;
    let rows: Vec<BeadsViewRow> = rows
        .into_iter()
        .map(|ListRow { mut row, .. }| {
            row.blocks = blocks.remove(&row.id).unwrap_or_default();
            if let (BeadsReadiness::Ok, Some(readiness)) = (&readiness_state, &readiness) {
                row.ready = Some(readiness.ready.contains(&row.id));
                row.blocked_by = Some(readiness.blocked.get(&row.id).cloned().unwrap_or_default());
            }
            let wake_at = row
                .defer_until
                .as_deref()
                .filter(|_| row.status == "deferred")
                .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
                .map(|time| time.with_timezone(&Utc))
                .filter(|wake_at| *wake_at > now);
            if let Some(wake_at) = wake_at {
                next_wake_at = Some(next_wake_at.map_or(wake_at, |current| current.min(wake_at)));
            }
            row
        })
        .collect();
    let mut state = entry.state();
    let custom_statuses = state
        .published
        .as_ref()
        .map(|published| published.custom_statuses.clone())
        .unwrap_or_default();
    let content_hash = content_hash(&rows, &readiness_state);
    let changed = state
        .published
        .as_ref()
        .is_none_or(|published| published.content_hash != content_hash);
    if changed {
        state.counter += 1;
    }
    let revision = format!("{:x}.{}", entry.epoch, state.counter);
    state.written = false;
    state.snapshot_stale = false;
    state.refresh_failed = None;
    state.published = Some(Published {
        token: token.clone(),
        at: Instant::now(),
        read_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        revision,
        content_hash,
        rows,
        readiness: readiness_state,
        custom_statuses,
        next_wake_at,
    });
}

fn content_hash(rows: &[BeadsViewRow], readiness: &BeadsReadiness) -> [u8; 32] {
    let encoded = serde_json::to_vec(&(rows, readiness)).unwrap_or_default();
    Sha256::digest(&encoded).into()
}

/// The custom statuses, the bd actor and the sync remote: read once per workspace (the statuses
/// through the cached `configGet` reply) and kept until a write or a sync.
async fn ensure_side_facts(
    entry: &WorkspaceEntry,
    info: &WorkspaceInfo,
    bd: &str,
    cwd: &str,
    context: &TypedOperationContext,
    deadline: tokio::time::Instant,
) {
    let token = current_token(info);
    let statuses_reply = match cached_reply(entry, "configGet", &token) {
        Some(reply) => Some(reply),
        None => read_config_statuses(entry, &token, context, budget(deadline)).await,
    };
    if let Some(reply) = statuses_reply {
        let statuses = parse_custom_statuses(&reply);
        let mut state = entry.state();
        let changed = state
            .published
            .as_ref()
            .is_some_and(|published| published.custom_statuses != statuses);
        if changed {
            state.counter += 1;
            let revision = format!("{:x}.{}", entry.epoch, state.counter);
            if let Some(published) = state.published.as_mut() {
                published.custom_statuses = statuses;
                published.revision = revision;
            }
        }
    }
    let needs_actor = entry.state().actor.is_none();
    if needs_actor {
        let actor = resolve_actor(cwd, budget(deadline)).await;
        entry.state().actor = Some(actor);
    }
    let needs_remote = {
        let state = entry.state();
        state.remote.is_none()
            && state
                .remote_failed_at
                .is_none_or(|failed| failed.elapsed() >= SIDE_FACT_RETRY)
    };
    if needs_remote {
        let command = bd_read(bd, &["dolt", "remote", "list", "--json"], cwd);
        super::log::bd_call("refresh", "remoteList");
        let read = run_process_command_with_timeout(&command, context, budget(deadline)).await;
        let mut state = entry.state();
        match read {
            // A read that worked is kept (no remote included) until a sync or the header's Refresh.
            Ok(output) if output.exit_code == 0 && output.error.is_none() => {
                state.remote = Some(parse_remote(&output.stdout));
                state.remote_failed_at = None;
            }
            // A failed read is not taken as "no remote": it is asked again later.
            _ => state.remote_failed_at = Some(Instant::now()),
        }
    }
}

async fn read_config_statuses(
    entry: &WorkspaceEntry,
    token: &Option<ChangeToken>,
    context: &TypedOperationContext,
    limit: Duration,
) -> Option<Value> {
    let command = beads_action_command("configGet", &Map::new(), context).ok()?;
    super::log::bd_call("refresh", "configGet");
    let output = run_process_command_with_timeout(&command, context, limit)
        .await
        .ok()?;
    if output.exit_code != 0 || output.error.is_some() {
        return None;
    }
    let reply = crate::typed_operations::finish_beads_action("configGet", &command, output).ok()?;
    entry.state().reads.insert(
        "configGet".to_string(),
        super::cache::CachedRead {
            token: token.clone(),
            at: Instant::now(),
            reply: reply.clone(),
        },
    );
    Some(reply)
}

fn parse_custom_statuses(reply: &Value) -> Vec<String> {
    let stdout = reply
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let value = unwrap_envelope(stdout)
        .and_then(|data| {
            data.get("value")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    value
        .split(',')
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_remote(stdout: &str) -> Option<ghostex_gx_protocol::beads_view::BeadsSyncRemote> {
    let remotes = unwrap_envelope(stdout)?;
    let remotes = remotes.as_array()?;
    let pick = remotes
        .iter()
        .find(|remote| remote.get("name").and_then(Value::as_str) == Some("origin"))
        .or_else(|| remotes.first())?;
    let text = |key: &str| {
        pick.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Some(ghostex_gx_protocol::beads_view::BeadsSyncRemote {
        name: text("name"),
        url: text("url"),
        status: text("status"),
    })
}

/// bd's own rule for who "you" are: `$BEADS_ACTOR`, then git's user.name in the workspace's
/// project, then `$USER` (`bd --help`, `--actor`).
async fn resolve_actor(cwd: &str, limit: Duration) -> Option<String> {
    if let Some(actor) = std::env::var("BEADS_ACTOR")
        .ok()
        .filter(|actor| !actor.trim().is_empty())
    {
        return Some(actor.trim().to_string());
    }
    let mut git = tokio::process::Command::new("git");
    git.args(["config", "user.name"])
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    crate::platform::process::NoConsoleWindow::no_console_window(&mut git);
    if let Ok(Ok(output)) =
        tokio::time::timeout(limit.min(Duration::from_secs(5)), git.output()).await
    {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() && !name.is_empty() {
            return Some(name);
        }
    }
    std::env::var("USER")
        .ok()
        .filter(|user| !user.trim().is_empty())
}

fn build_reply(
    entry: &WorkspaceEntry,
    info: &WorkspaceInfo,
    busy: bool,
    known_revision: Option<&str>,
) -> BeadsViewSnapshot {
    let state = entry.state();
    let workspace = BeadsWorkspaceInfo {
        state: BeadsWorkspaceState::Ready,
        key: entry.key.clone(),
        path: info.path.clone(),
        prefix: info.prefix.clone(),
        redirected: info.redirected,
        mode: workspace_mode(info).to_string(),
        actor: state.actor.clone().flatten(),
        message: None,
    };
    let mut snapshot = BeadsViewSnapshot {
        workspace,
        capabilities: capability::ALL
            .iter()
            .map(|name| name.to_string())
            .collect(),
        busy,
        settling: state.settling,
        error: state.last_error.clone(),
        sync: state.sync.info(state.remote.clone().flatten()),
        ..Default::default()
    };
    if let Some(published) = state.published.as_ref() {
        snapshot.revision = published.revision.clone();
        snapshot.read_at = Some(published.read_at.clone());
        snapshot.readiness = published.readiness.clone();
        snapshot.custom_statuses = published.custom_statuses.clone();
        if known_revision == Some(published.revision.as_str()) {
            snapshot.unchanged = true;
        } else {
            snapshot.beads = Some(published.rows.clone());
        }
    }
    snapshot
}

/// The reply for a project whose folder has no workspace, no bd, or a bd that failed.
pub(crate) fn workspace_less_reply(
    state: BeadsWorkspaceState,
    message: Option<String>,
) -> BeadsViewSnapshot {
    BeadsViewSnapshot {
        workspace: BeadsWorkspaceInfo {
            state,
            message,
            ..Default::default()
        },
        capabilities: capability::ALL
            .iter()
            .map(|name| name.to_string())
            .collect(),
        ..Default::default()
    }
}
