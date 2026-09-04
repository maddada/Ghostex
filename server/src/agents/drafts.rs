/*
CDXC:Drafts 2026-08-28:
A DRAFT is a real, durable gxserver session row whose agent CLI is running in
the background but which has never received a user prompt. Every new agent
session created from the sidebar starts this way, so trust / login / upgrade
screens surface while the user is still typing, and so the row is visible on
every client instead of only on the machine that created it.

The whole feature hangs off ONE marker: `runtimeSettings.draftStatus = "draft"`.
It is written at creation (`GxserverCreateSessionParams.draft`) and REMOVED —
never set to a second value — the moment the first user prompt actually reaches
the agent. A promoted draft is byte-for-byte an ordinary session; there is no
"was a draft" state to reason about anywhere downstream.

This module owns the marker, the promotion choke point, the draft-derived
display title, the agent list the composer's "Agents" section renders, and the
agent switch itself. Everything else in the daemon asks here rather than
testing the key.

CDXC:Drafts 2026-08-29 (drafts are durable):
A draft is never thrown away on its own. It survives navigating to another
session, sleeping, and daemon restarts, whether or not anything has been typed
into it, and leaves the sidebar by exactly two routes: the user deletes it, or
it is promoted. There is no navigate-away discard and no boot-time sweep of
empty drafts any more — an empty draft is a session the user made on purpose.
*/

mod agent_switch;
mod presentation;

pub(crate) use agent_switch::*;
pub(crate) use presentation::*;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::*;
use crate::domain::{DomainRepository, DomainStateError};
use crate::zmx::{dispatch_zmx_lifecycle_endpoint, ZmxServerContext};

pub(crate) const DRAFT_STATUS_KEY: &str = "draftStatus";
pub(crate) const DRAFT_STATUS_DRAFT: &str = "draft";

pub(crate) fn runtime_settings_are_draft(runtime_settings: &Map<String, Value>) -> bool {
    read_text_from_map(runtime_settings, DRAFT_STATUS_KEY).as_deref() == Some(DRAFT_STATUS_DRAFT)
}

pub(crate) fn session_is_draft(session: &Value) -> bool {
    runtime_settings_are_draft(&object_field(session, "runtimeSettings"))
}

/// Arms the marker at creation. The marker is server-owned: a client that puts
/// `draftStatus` in `runtimeSettings` by hand does not get a draft, and a
/// create call without `draft: true` always strips it.
pub(crate) fn apply_draft_session_create_param(
    params: &Map<String, Value>,
    runtime_settings: &mut Map<String, Value>,
) {
    if params.get("draft").and_then(Value::as_bool) == Some(true) {
        runtime_settings.insert(DRAFT_STATUS_KEY.to_string(), json!(DRAFT_STATUS_DRAFT));
    } else {
        runtime_settings.remove(DRAFT_STATUS_KEY);
    }
}

pub(crate) fn clear_draft_status(runtime_settings: &mut Map<String, Value>) -> bool {
    runtime_settings.remove(DRAFT_STATUS_KEY).is_some()
}

/*
The TERMINAL-DIRECT half of the promotion choke point: a user who ignores chat
and types straight into the agent's pane. It has two signals, in order of how
much they actually prove.

1. `promote_draft_on_prompt_evidence` — an agent hook that says, in so many
   words, that the user submitted a prompt. This is the signal we want, and for
   every agent whose hooks Ghostex has installed it is the one that fires.

2. `promote_draft_on_first_activity` — the first working/attention transition,
   for agents with no hooks, where a terminal title is the only evidence there
   is. A title cannot distinguish "the user prompted me" from "I am booting":
   Codex's startup spinner paints exactly the same working title (see
   `default_activity`). The daemon already has a policy for that — layer 1 of
   CDXC:AgentScreenDetection — and drafts lean on it rather than inventing
   a second one: `arm_draft_launch_activity_suppression` re-arms the launch
   window on EVERY draft provider start, so a boot spinner is folded back to
   idle and never reaches this function with a timestamp.

Both are deliberately keyed on "this row had NO `lastActiveAt` and is about to
get one" rather than on the presence of a timestamp in the update:
`compute_activity_update` carries the session's existing `lastActiveAt` forward
on every idle transition, so the timestamp alone says nothing about whether the
agent was ever prompted.
*/
pub(crate) fn promote_draft_on_first_activity(
    session: &Value,
    runtime_settings: &mut Map<String, Value>,
    next_last_active_at: Option<&str>,
) -> bool {
    if read_text_value(session, "lastActiveAt").is_some() {
        return false;
    }
    if next_last_active_at
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return false;
    }
    clear_draft_status(runtime_settings)
}

/// Whether an agent-hook event is positive evidence that the USER submitted a
/// prompt: Claude's `UserPromptSubmit`, or any hook carrying the prompt text
/// itself. Nothing a CLI does while starting up produces either.
pub(crate) fn is_draft_prompt_evidence(params: &Map<String, Value>) -> bool {
    is_explicit_user_prompt_submit_event(params) || read_text(params, "firstUserMessage").is_some()
}

pub(crate) fn promote_draft_on_prompt_evidence(
    params: &Map<String, Value>,
    runtime_settings: &mut Map<String, Value>,
) -> bool {
    if !is_draft_prompt_evidence(params) {
        return false;
    }
    clear_draft_status(runtime_settings)
}

/*
Re-arms layer 1 of CDXC:AgentScreenDetection on a draft — the same reset a
launch, resume or wake performs — so passive title signals are folded back to
idle for the window instead of stamping `lastActiveAt`.

Drafts need this at moments an ordinary session does not, because for a draft
that stamp is not merely a wrong "Last Active" label: it is the promotion that
takes the row out of the draft state permanently. Called on every draft provider
start (a cold desktop attach and the restart inside an agent switch both go
through `/api/startSessionProvider`, and neither is a wake), and after Ghostex
itself types an option command into a draft's terminal.

Scoped to drafts on purpose: no non-draft session's activity changes because of
this.
*/
pub(crate) fn arm_draft_launch_activity_suppression(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Result<Value, DomainStateError> {
    if !session_is_draft(session) {
        return Ok(session.clone());
    }
    let (Some(project_id), Some(session_id)) = (
        read_text_value(session, "projectId"),
        read_text_value(session, "sessionId"),
    ) else {
        return Ok(session.clone());
    };
    let update =
        crate::session_status::compute_activity_update(session, &Map::new(), Some("launch"));
    let mut runtime_settings = object_field(session, "runtimeSettings");
    if runtime_settings.get("agentActivity") == Some(&update.activity) {
        return Ok(session.clone());
    }
    runtime_settings.insert("agentActivity".to_string(), update.activity);
    let mut session_update = Map::new();
    session_update.insert("projectId".to_string(), json!(project_id));
    session_update.insert("sessionId".to_string(), json!(session_id));
    session_update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&session_update)
}

/*
Kills a draft's background agent CLI, including the macOS launchd job
`kill_zmx_session` cleans up. Runs when a draft row is DELETED (the user
deleting it, through `/api/removeSession`), because that row is the last thing
pointing at that daemon: miss it and the CLI runs forever with nothing in the
sidebar to stop it.

Deliberately unconditional rather than probe-then-kill. The row is being deleted
either way, so the `unknown` provider state a kill of an already-dead daemon
writes never reaches a client, while a probe costs a second subprocess and opens
a window where a provider that was still starting reads as `missing` and is
orphaned anyway.
*/
pub(crate) fn kill_draft_session_provider(session: &Value) {
    if !session_is_draft(session) {
        return;
    }
    let Some(zmx_name) = read_text_value(session, "zmxName") else {
        return;
    };
    let Ok(zmx) = crate::toolchain::require_bundled_zmx() else {
        return;
    };
    crate::zmx::kill_zmx_session(&zmx_name, &zmx.executable_path);
}

/*
CDXC:Drafts 2026-08-28 (stranded quick projects):
`/api/createQuickAgentSession` creates a throwaway workspace before it creates
the draft that lives in it: a project row marked `launchSettings.isQuick`, whose
`path` is a real directory `create_quick_project_params` made under
`~/ghostex/chats`. Deleting the draft therefore leaves TWO orphans behind, not
one — an empty project row in the sidebar and a directory on disk — so the
removal path runs this afterwards.

The directory delete is the destructive part of the feature, so it is guarded
four ways and every guard must pass:

  1. the project is marked quick (the same `isQuick` flag the creator writes);
  2. the project has NO sessions left at all, so nothing can still be using it;
  3. the path is a real directory and NOT a symlink, checked through
     `symlink_metadata` so a link is never followed to its target; and
  4. the path's parent, CANONICALIZED, is exactly the canonical
     `<home>/ghostex/chats`, and the leaf name matches the exact
     `<timestamp>-<kind>-<suffix>` shape the creator builds.

Canonicalizing both sides is what makes guard 4 real rather than cosmetic: it
resolves `..` segments and symlinks before the comparison, so neither a crafted
project path nor a swapped-in link can name a directory outside the quick-chats
parent. A path that fails any guard is LEFT ON DISK with a warning — the row is
still cleaned up, because removing a stranded row destroys nothing, but nothing
outside that one directory is ever deleted on a guess.
*/
fn project_is_quick_chat_workspace(project: &Value) -> bool {
    let flagged = |key: &str| {
        project.get(key) == Some(&Value::Bool(true))
            || project
                .get("launchSettings")
                .and_then(|settings| settings.get(key))
                == Some(&Value::Bool(true))
    };
    flagged("isQuick")
}

/// The leaf `create_quick_project_params` builds:
/// `{%Y-%m-%d-%H%M%S%3f}-{terminal|agent}-{8 hex}`, e.g.
/// `2026-08-28-143005123-agent-1a2b3c4d`.
fn is_quick_chat_directory_name(name: &str) -> bool {
    let Some((rest, suffix)) = name.rsplit_once('-') else {
        return false;
    };
    if suffix.len() != 8 || !suffix.chars().all(|value| value.is_ascii_hexdigit()) {
        return false;
    }
    let Some((timestamp, kind)) = rest.rsplit_once('-') else {
        return false;
    };
    if !matches!(kind, "terminal" | "agent") {
        return false;
    }
    let parts = timestamp.split('-').collect::<Vec<_>>();
    let [year, month, day, time] = parts.as_slice() else {
        return false;
    };
    [(year, 4), (month, 2), (day, 2), (time, 9)]
        .iter()
        .all(|(part, width)| {
            part.len() == *width && part.chars().all(|value| value.is_ascii_digit())
        })
}

/// The directory this quick project owns, or `None` when the path does not
/// provably sit directly inside `<home>/ghostex/chats` under a name the creator
/// would have produced. `None` always means "do not delete anything".
fn quick_chat_directory_to_remove(home_dir: &Path, project: &Value) -> Option<PathBuf> {
    let path = PathBuf::from(read_text_value(project, "path")?);
    // A symlink reports `is_dir() == false` here, so this rejects links without
    // ever resolving one — the delete must never leave the chats directory.
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    if !is_quick_chat_directory_name(path.file_name()?.to_str()?) {
        return None;
    }
    let canonical_parent = std::fs::canonicalize(path.parent()?).ok()?;
    let canonical_chats = std::fs::canonicalize(home_dir.join("ghostex").join("chats")).ok()?;
    (canonical_parent == canonical_chats).then_some(path)
}

/// Removes the quick workspace a just-discarded draft was the last session of.
/// Returns whether the project ROW was removed, so the caller can publish the
/// matching presentation delta. A no-op for every project that is not an empty
/// quick workspace — a real user project is never touched.
pub(crate) fn discard_stranded_quick_project(
    repository: &DomainRepository<'_>,
    home_dir: &Path,
    project_id: &str,
) -> Result<bool, DomainStateError> {
    let Some(project) = repository.get_project(project_id)? else {
        return Ok(false);
    };
    if !project_is_quick_chat_workspace(&project) {
        return Ok(false);
    }
    if repository.has_sessions(project_id)? {
        return Ok(false);
    }
    let directory = quick_chat_directory_to_remove(home_dir, &project);
    repository.remove_project(project_id)?;
    match directory {
        Some(directory) => {
            if let Err(error) = std::fs::remove_dir_all(&directory) {
                log_quick_project_directory_kept(project_id, "removeFailed", &error.to_string());
            }
        }
        None => {
            /*
            The row is gone but the directory stays. This is the deliberate
            outcome for a quick project whose path was hand-edited, relocated,
            replaced with a symlink, or already deleted: we would be guessing,
            and the guess is a recursive delete.
            */
            log_quick_project_directory_kept(
                project_id,
                "pathOutsideQuickChats",
                read_text_value(&project, "path")
                    .as_deref()
                    .unwrap_or_default(),
            );
        }
    }
    Ok(true)
}

static QUICK_PROJECT_LOGGER: OnceLock<crate::logging::GxserverLogger> = OnceLock::new();

/// Unconditional (not scenario-gated): leaving a directory behind is a warning
/// the user may need to act on. Records the reason and the project id only —
/// never the path's contents.
fn log_quick_project_directory_kept(project_id: &str, reason: &str, detail: &str) {
    let logger = QUICK_PROJECT_LOGGER.get_or_init(|| {
        crate::logging::GxserverLogger::new(crate::paths::get_gxserver_paths(None))
    });
    let _ = logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: "quickProjectDirectoryKept".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(reason.to_string()),
        details: Some(json!({ "detail": detail, "projectId": project_id })),
    });
}

/*
The CHAT half of the promotion choke point: one durable write, called after a
chat/queue send has actually handed its bytes to the agent. Returns whether the
row changed, so the caller can skip a presentation delta for the overwhelmingly
common case of a send into a session that was never a draft.
*/
pub(crate) fn promote_draft_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<bool, DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(false);
    };
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    if !clear_draft_status(&mut runtime_settings) {
        return Ok(false);
    }
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update)?;
    Ok(true)
}
