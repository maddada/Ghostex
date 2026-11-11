//! Every `/api/runBeadsAction` call goes through here: under the workspace's lock, with the
//! recurring reads answered from the cache while the workspace is unchanged.

use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::toolchain::require_system_bd;
use crate::typed_operations::{
    beads_action_command, finish_beads_action, resolve_beads_cwd, run_beads_action_direct,
    run_process_command, TypedOperationContext, TypedOperationError,
};

use super::cache::{self, still_current, CachedRead, WorkspaceEntry};
use super::change::{embedded_noms_dir, read_change_token, ChangeToken};
use super::discovery::{discover, Discovered, WorkspaceInfo};

/// Today's typed-operation limit; a call that waits this long on another bd call fails as a
/// timeout, as a slow bd always did.
const ACTION_LOCK_WAIT: Duration = Duration::from_secs(120);

/// The reads the Board repeats on every poll, all answered from one cache.
const CACHED_READS: [&str; 6] = [
    "board",
    "list",
    "listAllLabels",
    "configGet",
    "configGetIssuePrefix",
    "status",
];
/// These three run the same `bd list --all -n 0 --json`, so one read fills all three.
const LIST_READS: [&str; 3] = ["board", "list", "listAllLabels"];
const READ_ONLY: [&str; 2] = ["show", "search"];

pub(crate) fn current_token(info: &WorkspaceInfo) -> Option<ChangeToken> {
    embedded_noms_dir(&info.database_path).and_then(|noms| read_change_token(&noms))
}

pub(crate) fn workspace_mode(info: &WorkspaceInfo) -> &'static str {
    if embedded_noms_dir(&info.database_path).is_some() {
        "embedded"
    } else {
        "server"
    }
}

/// CDXC:ProjectBoard 2026-11-11 WHY:
/// Embedded Dolt lets one bd process at a time open a workspace and a waiting bd never gives up, so the Board's 8-second poll, the Beads view and agents' own bd calls slowed each other down. Every Ghostex bd call for a workspace now takes that workspace's one lock, and the reads the Board repeats are answered from the cache until the change token moves, with replies built by the same code from the same bd output, so they are what bd's output always gave. Without bd or without a workspace, calls run exactly as before.
pub(crate) async fn run_beads_action_in_workspace(
    action: &str,
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let Ok(bd) = require_system_bd() else {
        return run_beads_action_direct(action, params, context).await;
    };
    let cwd = resolve_beads_cwd(context);
    let discovered = discover(&cwd, &bd.executable_path, context).await;
    if action == "storageExists" {
        let exists = matches!(discovered, Discovered::Workspace(_));
        return Ok(json!({
            "action": action,
            "exitCode": if exists { 0 } else { 1 },
            "stderr": "",
            "stdout": if exists { "true" } else { "false" },
        }));
    }
    let Discovered::Workspace(info) = discovered else {
        return run_beads_action_direct(action, params, context).await;
    };
    let entry = cache::entry(&info.path);
    if matches!(action, "doltPull" | "doltPush") {
        return super::sync::start_sync(entry, info, action, context.clone());
    }
    if CACHED_READS.contains(&action) {
        return cached_read(&entry, &info, action, params, context).await;
    }
    let Some(_guard) = entry.lock_within(ACTION_LOCK_WAIT).await else {
        return Ok(busy_reply(action));
    };
    super::log::bd_call("action", action);
    let result = run_beads_action_direct(action, params, context).await;
    if !READ_ONLY.contains(&action) {
        entry.forget_reads();
    }
    result
}

fn busy_reply(action: &str) -> Value {
    json!({
        "action": action,
        "error": {
            "code": "timeout",
            "message": "Beads stayed busy for two minutes. Try again in a moment.",
        },
        "exitCode": 1,
        "stderr": "",
        "stdout": "",
    })
}

async fn cached_read(
    entry: &WorkspaceEntry,
    info: &WorkspaceInfo,
    action: &str,
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let Some(_guard) = entry.lock_within(ACTION_LOCK_WAIT).await else {
        return Ok(busy_reply(action));
    };
    let before = current_token(info);
    if let Some(reply) = cached_reply(entry, action, &before) {
        return Ok(reply);
    }
    let command = beads_action_command(action, params, context)?;
    super::log::bd_call("read", action);
    let output = run_process_command(&command, context).await?;
    let after = current_token(info);
    let ok = output.exit_code == 0 && output.error.is_none() && before == after;
    let reply = finish_beads_action(action, &command, output.clone());
    if ok {
        let mut state = entry.state();
        // The cached reads are current again; the published snapshot keeps its own staleness
        // (`snapshot_stale`), which only a refresh clears.
        state.written = false;
        let names: &[&str] = if LIST_READS.contains(&action) {
            &LIST_READS
        } else {
            std::slice::from_ref(&action)
        };
        for name in names {
            let built = if *name == action {
                reply.clone()
            } else {
                finish_beads_action(name, &command, output.clone())
            };
            if let Ok(built) = built {
                state.reads.insert(
                    (*name).to_string(),
                    CachedRead {
                        token: after.clone(),
                        at: Instant::now(),
                        reply: with_action(built, name),
                    },
                );
            }
        }
    }
    reply
}

pub(crate) fn cached_reply(
    entry: &WorkspaceEntry,
    action: &str,
    token: &Option<ChangeToken>,
) -> Option<Value> {
    let state = entry.state();
    let cached = state.reads.get(action)?;
    still_current(&cached.token, cached.at, token, state.written).then(|| cached.reply.clone())
}

/// Stores a reply the snapshot's own `bd list` produced, as if the action had run.
pub(crate) fn store_list_replies(
    entry: &WorkspaceEntry,
    command_context: &TypedOperationContext,
    output: &crate::typed_operations::CommandOutput,
    token: &Option<ChangeToken>,
) {
    let params = Map::new();
    let Ok(command) = beads_action_command("list", &params, command_context) else {
        return;
    };
    let mut state = entry.state();
    for name in LIST_READS {
        if let Ok(reply) = finish_beads_action(name, &command, output.clone()) {
            state.reads.insert(
                name.to_string(),
                CachedRead {
                    token: token.clone(),
                    at: Instant::now(),
                    reply: with_action(reply, name),
                },
            );
        }
    }
}

/// The reply names the action that was asked; one `bd list` answers three of them.
fn with_action(mut reply: Value, action: &str) -> Value {
    if let Some(object) = reply.as_object_mut() {
        object.insert("action".to_string(), json!(action));
    }
    reply
}
