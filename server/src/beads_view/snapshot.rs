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

/// A view poll waits this long for a refresh another caller is running before it answers with
/// the data it has and `busy`.
const POLL_LOCK_WAIT: Duration = Duration::from_millis(1500);
/// Each bd read in a refresh, lock wait included, gives up after this long.
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const EMBEDDED_ATTEMPTS: usize = 3;

pub(crate) async fn read_snapshot(
    entry: Arc<WorkspaceEntry>,
    info: &WorkspaceInfo,
    bd_executable_path: &str,
    cwd: &str,
    context: &TypedOperationContext,
    known_revision: Option<&str>,
) -> BeadsViewSnapshot {
    let guard = entry.lock_within(POLL_LOCK_WAIT).await;
    let busy = guard.is_none();
    let mut error = None;
    if guard.is_some() {
        let token = current_token(info);
        if needs_refresh(&entry, &token) {
            error = refresh(&entry, info, bd_executable_path, cwd, context).await;
        }
        ensure_side_facts(&entry, info, bd_executable_path, cwd, context).await;
    }
    drop(guard);
    build_reply(&entry, info, busy, error, known_revision)
}

fn needs_refresh(entry: &WorkspaceEntry, token: &Option<ChangeToken>) -> bool {
    let state = entry.state();
    let Some(published) = state.published.as_ref() else {
        return true;
    };
    if !still_current(&published.token, published.at, token, state.written) {
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
) -> Option<String> {
    let embedded = workspace_mode(info) == "embedded";
    let attempts = if embedded { EMBEDDED_ATTEMPTS } else { 1 };
    let ready_command = bd_read(
        bd,
        &["ready", "--explain", "--json", "--limit", "0"],
        cwd,
    );
    let list_command = match beads_action_command("list", &Map::new(), context) {
        Ok(command) => command,
        Err(error) => return Some(error.message),
    };
    for _ in 0..attempts {
        let before = current_token(info);
        super::log::bd_call("refresh", "ready");
        let ready = run_process_command_with_timeout(&ready_command, context, READ_TIMEOUT).await;
        let middle = current_token(info);
        super::log::bd_call("refresh", "list");
        let list = run_process_command_with_timeout(&list_command, context, READ_TIMEOUT).await;
        let after = current_token(info);
        let list = match list {
            Ok(output) if output.exit_code == 0 && output.error.is_none() => output,
            Ok(output) => return Some(command_failure(&output)),
            Err(error) => return Some(error.message),
        };
        if embedded && !(before == middle && middle == after) {
            continue;
        }
        let rows = match parse_rows(&list.stdout) {
            Ok(rows) => rows,
            Err(message) => return Some(message),
        };
        let readiness = match ready {
            Ok(output) if output.exit_code == 0 && output.error.is_none() => {
                parse_readiness(&output.stdout)
            }
            _ => None,
        };
        store_list_replies(entry, context, &list, &after);
        publish(entry, &after, rows, readiness, embedded);
        entry.state().settling = false;
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

/// Server mode: does bd's ready answer agree with the list's own dependency facts?
fn readiness_agrees(rows: &[ListRow], readiness: &Readiness) -> bool {
    let by_id: HashMap<&str, &ListRow> = rows.iter().map(|row| (row.row.id.as_str(), row)).collect();
    let open = |id: &str| {
        by_id
            .get(id)
            .is_some_and(|row| row.row.status != "closed")
    };
    let ready_ok = readiness.ready.iter().all(|id| {
        by_id.get(id.as_str()).is_some_and(|row| {
            row.row.status != "closed"
                && row.row.status != "deferred"
                && !row.blocked_on.iter().any(|blocker| open(blocker))
        })
    });
    let blocked_ok = readiness.blocked.iter().all(|(id, by)| {
        by_id.get(id.as_str()).is_some_and(|row| {
            by.iter()
                .all(|blocker| row.blocked_on.contains(blocker) && open(blocker))
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
    state.published = Some(Published {
        token: token.clone(),
        at: Instant::now(),
        read_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        revision,
        content_hash,
        rows,
        readiness: readiness_state,
        custom_statuses,
        error: None,
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
) {
    let token = current_token(info);
    let statuses_reply = match cached_reply(entry, "configGet", &token) {
        Some(reply) => Some(reply),
        None => read_config_statuses(entry, &token, context).await,
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
        let actor = resolve_actor(cwd).await;
        entry.state().actor = Some(actor);
    }
    let needs_remote = entry.state().remote.is_none();
    if needs_remote {
        let command = bd_read(bd, &["dolt", "remote", "list", "--json"], cwd);
        super::log::bd_call("refresh", "remoteList");
        let remote = match run_process_command_with_timeout(&command, context, READ_TIMEOUT).await
        {
            Ok(output) if output.exit_code == 0 => parse_remote(&output.stdout),
            _ => None,
        };
        entry.state().remote = Some(remote);
    }
}

async fn read_config_statuses(
    entry: &WorkspaceEntry,
    token: &Option<ChangeToken>,
    context: &TypedOperationContext,
) -> Option<Value> {
    let command = beads_action_command("configGet", &Map::new(), context).ok()?;
    super::log::bd_call("refresh", "configGet");
    let output = run_process_command_with_timeout(&command, context, READ_TIMEOUT)
        .await
        .ok()?;
    if output.exit_code != 0 || output.error.is_some() {
        return None;
    }
    let reply =
        crate::typed_operations::finish_beads_action("configGet", &command, output).ok()?;
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
        .and_then(|data| data.get("value").and_then(Value::as_str).map(str::to_string))
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
async fn resolve_actor(cwd: &str) -> Option<String> {
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
        tokio::time::timeout(Duration::from_secs(5), git.output()).await
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
    error: Option<String>,
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
        capabilities: capability::ALL.iter().map(|name| name.to_string()).collect(),
        busy,
        settling: state.settling,
        error: error.or_else(|| state.published.as_ref().and_then(|p| p.error.clone())),
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
pub(crate) fn workspace_less_reply(state: BeadsWorkspaceState, message: Option<String>) -> BeadsViewSnapshot {
    BeadsViewSnapshot {
        workspace: BeadsWorkspaceInfo {
            state,
            message,
            ..Default::default()
        },
        capabilities: capability::ALL.iter().map(|name| name.to_string()).collect(),
        ..Default::default()
    }
}
