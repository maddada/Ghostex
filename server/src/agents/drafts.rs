/*
CDXC:DraftSessions 2026-08-28:
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
display title, the agent list the composer's "Agents" section renders, the
agent switch itself, and the boot-time sweep that discards drafts nobody typed
into. Everything else in the daemon asks here rather than testing the key.
*/

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::*;
use crate::domain::{DomainRepository, DomainStateError};
use crate::zmx::{dispatch_zmx_lifecycle_endpoint, ZmxServerContext};

pub(crate) const DRAFT_STATUS_KEY: &str = "draftStatus";
pub(crate) const DRAFT_STATUS_DRAFT: &str = "draft";

/// Sidebar rows are one line; anything past this is noise the tooltip already
/// carries. Measured in CHARACTERS, not bytes, so a prompt written in any
/// script clamps to the same visual length.
const DRAFT_DISPLAY_TITLE_MAX_CHARS: usize = 64;

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
   CDXC:ActivitySuppressionPolicy — and drafts lean on it rather than inventing
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
Re-arms layer 1 of CDXC:ActivitySuppressionPolicy on a draft — the same reset a
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
`kill_zmx_session` cleans up. Used on both paths that DELETE a draft row — the
client's navigate-away discard through `/api/removeSession` and the boot sweep —
because a removed row is the last thing pointing at that daemon: miss it and the
CLI runs forever with nothing in the sidebar to stop it.

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
CDXC:DraftSessions 2026-08-28 (stranded quick projects):
`/api/createQuickAgentSession` creates a throwaway workspace before it creates
the draft that lives in it: a project row marked `launchSettings.isQuick`, whose
`path` is a real directory `create_quick_project_params` made under
`~/ghostex/chats`. Discarding the draft therefore leaves TWO orphans behind, not
one — an empty project row in the sidebar and a directory on disk — so every
path that throws a draft away runs this afterwards.

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
    if !repository.list_sessions(Some(project_id))?.is_empty() {
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
CDXC:DraftSessions 2026-08-28 (discard revalidation):
`reason` on `/api/removeSession`, and the server-side recheck behind it. Mirrors
`DISCARD_EMPTY_DRAFT_SESSION_REASON` in `packages/shared/draft-sessions.ts`.

The navigate-away discard is necessarily client-side — only the client knows the
user just left the session — but the client's answer is computed from a
presentation snapshot that can be one delta out of date, and the race that opens
is destructive. The user types a prompt and presses Enter without ever blurring
the composer, so the synced draft row is never written; the send succeeds and
gxserver promotes the session; the user clicks another row before the promotion
delta lands, so the client still sees `isDraft` with no draft-derived title and
fires the discard. That request would have DELETED a live session with the
user's first prompt in flight, and because the row is no longer a draft
`kill_draft_session_provider` would have declined to kill it — orphaning the
running CLI as well.

So a discard is a REQUEST, not an instruction: the daemon re-derives both halves
of the predicate from its own current state and refuses if either has moved. A
plain removal (a user deleting a session on purpose) carries no reason and is
untouched by any of this.
*/
pub(crate) const DISCARD_EMPTY_DRAFT_SESSION_REASON: &str = "discardEmptyDraft";

pub(crate) fn is_empty_draft_discard_request(params: &Map<String, Value>) -> bool {
    read_text(params, "reason").as_deref() == Some(DISCARD_EMPTY_DRAFT_SESSION_REASON)
}

/// Whether this session is, right now, exactly what a navigate-away discard
/// claims it is: still a draft, and with no text in the synced composer row.
///
/// A FAILED draft read declines, for the same reason the boot sweep skips on
/// one: an unreadable answer is not evidence that nobody typed anything, and
/// this decision deletes a session.
fn session_is_discardable_empty_draft(db: &Connection, session: &Value) -> bool {
    if !session_is_draft(session) {
        return false;
    }
    let (Some(project_id), Some(session_id)) = (
        read_text_value(session, "projectId"),
        read_text_value(session, "sessionId"),
    ) else {
        return false;
    };
    matches!(
        crate::session_chat_queue::read_session_chat_draft_content(db, &project_id, &session_id),
        Ok(None)
    )
}

/*
The decline response, or `None` to let the removal proceed.

It is a SUCCESS, not an error: the client did nothing wrong, it merely raced the
daemon, and a rejected promise would leave the desktop sidebar sitting on the
optimistic `removePresentationSession` it performed before the call. The payload
therefore carries everything that client needs to put the row back — an explicit
`removed: false`, a `declined` code, and the session row as it stands now — and
the same fields are published for the successful case so a discard never has to
infer its outcome from an absent key.
*/
pub(crate) fn decline_empty_draft_discard(
    repository: &DomainRepository<'_>,
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Option<Value>, DomainStateError> {
    if !is_empty_draft_discard_request(params) {
        return Ok(None);
    }
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // A session that is already gone falls through to `remove_session`, which
    // answers with the same not-found error it always has.
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(None);
    };
    if session_is_discardable_empty_draft(db, &session) {
        return Ok(None);
    }
    Ok(Some(json!({
        "declined": if session_is_draft(&session) {
            "draftHasText"
        } else {
            "sessionIsNotADraft"
        },
        "removed": false,
        "session": session,
    })))
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

/*
The sidebar row's title while a draft waits: the first non-blank line of the
text the user has typed, so a draft reads as what it is about instead of as
"Claude Session". Projection-level only — the durable `title` stays the agent
default, which is what a promoted draft goes back to showing.
*/
pub(crate) fn draft_display_title(content: &str) -> Option<String> {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    if normalized.chars().count() <= DRAFT_DISPLAY_TITLE_MAX_CHARS {
        return Some(normalized);
    }
    let mut clamped = normalized
        .chars()
        .take(DRAFT_DISPLAY_TITLE_MAX_CHARS - 1)
        .collect::<String>()
        .trim_end()
        .to_string();
    clamped.push('\u{2026}');
    Some(clamped)
}

/*
CDXC:DraftSessions 2026-08-28 (agent switching):
Only chat-supported agents can be switched to, because the composer that offers
the switch can only read a transcript it has a decoder for. `openclaude` and
`grok-build` are alternate ids for the Claude and Grok families, so they map
onto them rather than adding families of their own; `omp` is its own family
despite sharing Pi's transcript format, because it paints different chrome.
*/
pub(crate) fn chat_supported_base_agent_id(value: Option<&str>) -> Option<&'static str> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "claude" | "openclaude" => Some("claude"),
        "codex" => Some("codex"),
        "grok" | "grok-build" => Some("grok"),
        "pi" => Some("pi"),
        "omp" => Some("omp"),
        _ => None,
    }
}

/// The five base families, in the order the composer renders them.
const CHAT_SUPPORTED_BASE_AGENTS: [(&str, &str, &str); 5] = [
    ("claude", "Claude", "claude"),
    ("codex", "Codex", "codex"),
    ("grok", "Grok Build", "grok-build"),
    ("pi", "Pi Agent", "pi"),
    ("omp", "OMP", "omp"),
];

#[derive(Clone)]
struct AvailableDraftAgent {
    agent_id: String,
    base_agent_id: String,
    icon: String,
    name: String,
}

impl AvailableDraftAgent {
    fn to_value(&self) -> Value {
        json!({
            "agentId": self.agent_id,
            "baseAgentId": self.base_agent_id,
            "icon": self.icon,
            "name": self.name,
        })
    }
}

/*
The "Agents" section of the chat composer's model dropdown, resolved by the
daemon that owns the project so every client (desktop, web, phone over SSH)
offers the same set without shipping its own copy of the project's agent
configuration.

The list is the five base families, each overridden by the project's stored
configuration for that id when there is one, plus every project custom agent
whose declared base family — its `icon`, which is how a custom agent states
what CLI it actually runs — is chat-supported. A hidden agent is dropped, which
is the same rule the sidebar launcher applies.
*/
pub(crate) fn available_draft_agents(project: &Value) -> Value {
    let mut agents = CHAT_SUPPORTED_BASE_AGENTS
        .iter()
        .map(|(agent_id, name, icon)| AvailableDraftAgent {
            agent_id: agent_id.to_string(),
            base_agent_id: agent_id.to_string(),
            icon: icon.to_string(),
            name: name.to_string(),
        })
        .collect::<Vec<_>>();
    let mut custom = Vec::new();
    for configured in project
        .get("customAgents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
    {
        let Some(agent_id) = read_text_from_map(configured, "agentId") else {
            continue;
        };
        let hidden = configured.get("hidden").and_then(Value::as_bool) == Some(true);
        let base_index = agents
            .iter()
            .position(|agent| agent.agent_id.eq_ignore_ascii_case(&agent_id));
        if let Some(index) = base_index {
            if hidden {
                agents.remove(index);
                continue;
            }
            if let Some(name) = read_text_from_map(configured, "name") {
                agents[index].name = name;
            }
            if let Some(icon) = read_text_from_map(configured, "icon") {
                agents[index].icon = icon;
            }
            continue;
        }
        if hidden {
            continue;
        }
        let icon = read_text_from_map(configured, "icon");
        let Some(base_agent_id) = chat_supported_base_agent_id(icon.as_deref()) else {
            continue;
        };
        custom.push(AvailableDraftAgent {
            base_agent_id: base_agent_id.to_string(),
            icon: icon.unwrap_or_else(|| base_agent_id.to_string()),
            name: read_text_from_map(configured, "name").unwrap_or_else(|| agent_id.clone()),
            agent_id,
        });
    }
    agents.extend(custom);
    Value::Array(agents.iter().map(AvailableDraftAgent::to_value).collect())
}

/*
CDXC:DraftSessions 2026-08-28 (agent switching):
`/api/switchDraftAgent`. A draft holds no conversation, so this needs no
confirmation and destroys nothing: forget the agent identity the old CLI
published, rebuild the launch plan through the SAME resolution
`/api/createAgentSession` uses (so a switched draft is indistinguishable from
one created with the new agent), and get the new CLI running.

CDXC:DraftSessions 2026-08-28 (live-pane switching):
"Get the new CLI running" has two shapes, and which one runs is decided by the
provider probe — never by a failure.

  * The pane is ALIVE (the common case: the user is looking at the draft they
    just opened). The switch then happens INSIDE that pane and the provider is
    never touched. The provider script is `<setup>; <agent CLI>;
    exec $SHELL -l`, so interrupting the CLI drops the same pty into its login
    shell, where the new agent's launch line can simply be typed.
  * The pane is MISSING (a slept or never-opened draft). There is nothing to
    reuse and nothing to interrupt, so the rewritten row is started the normal
    way. This is the wake path, not a fallback.

Killing and restarting the provider — what this endpoint did until 2026-08-28 —
is what made all four reported switch defects: the desktop's attach client
exits with the pane, and the exit poll closes the tab, which drops chat-mode
membership and DESTROYS the CEF chat page. That page is where the user's unsent
composer text lives (its 2s draft-sync debounce never gets to flush), and its
teardown is the visible chat → raw terminal → chat flash. Do not reintroduce a
kill here.

The refusal on a promoted session is the point of the whole endpoint being
draft-only: once a prompt has reached the agent, its transcript, resume plan,
and stored conversation id all belong to that agent, and rewriting the row
would strand every one of them.
*/
pub(crate) fn switch_draft_agent(
    repository: &DomainRepository<'_>,
    db: &Connection,
    params: &Map<String, Value>,
    context: &ZmxServerContext,
) -> Result<AgentEndpointOutput, AgentEndpointError> {
    let lifecycle = read_lifecycle(params)?;
    let project = require_project(repository, &lifecycle.project_id)?;
    let session = require_session(repository, &lifecycle)?;
    if !session_is_draft(&session) {
        return Err(DomainStateError {
            code: "invalidState",
            message:
                "This session has already been prompted, so its agent can no longer be changed."
                    .to_string(),
        }
        .into());
    }
    let agent_id = read_required_text(params.get("agentId"), "agentId")?;
    let previous_agent_id = read_text_value(&session, "agentId");
    if previous_agent_id
        .as_deref()
        .is_some_and(|previous| previous.eq_ignore_ascii_case(&agent_id))
    {
        return Ok(AgentEndpointOutput {
            presentation_session: None,
            result: json!({ "agentId": agent_id, "session": session }),
        });
    }
    /*
    Resolve the new agent BEFORE the row is touched. The rebuild below refuses a
    commandless agent id too, but by then the old CLI has already been
    interrupted and a typo would leave the user staring at a bare login shell.
    */
    let agent_config = resolve_project_agent_config(&project, &agent_id, None);
    if read_text_from_map(&agent_config, "command").is_none()
        && default_agent_command(&agent_id).is_none()
    {
        return Err(DomainStateError::bad_request(format!(
            "{agent_id} is not an agent this project can launch."
        ))
        .into());
    }
    let settings = read_agent_settings(db)?;

    /*
    The probe picks the switch's shape, and nothing else: `exists` means the
    pane below is reused, anything else means the rewritten row is started. It
    is also the reason neither branch kills: an unconditional kill would stamp
    the row `unknown` for a draft whose CLI was never started (nobody has opened
    it yet), which the sidebar reads as a broken session.
    */
    let probe = dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/probeSessionProvider",
        params,
        context,
        &settings,
    )?;
    let provider_exists = probe
        .result
        .get("providerState")
        .and_then(|state| state.get("lifecycleState"))
        .and_then(Value::as_str)
        == Some("exists");
    // The probe writes the freshly observed `providerState` back to the row, so
    // re-read before the rewrite below builds its update out of it.
    let session = require_session(repository, &lifecycle)?;

    /*
    Everything the OLD agent owns has to go before the new plan is built, or
    `create_agent_session_params_for_project` would resolve the new agent id
    against the previous agent's stored command and icon and quietly launch the
    wrong CLI. The identity keys go with them: the conversation the old CLI
    published at startup does not exist for the new one.

    `agentActivity` is dropped for two reasons at once. It carries the OLD
    agent's `agentName`, which would keep naming a CLI that is no longer running
    until the new one's first event; and dropping it makes the rebuild below
    mint a fresh `default_activity`, which arms layer 1 of
    CDXC:ActivitySuppressionPolicy — so the new CLI's startup spinner is folded
    back to idle instead of stamping `lastActiveAt` and promoting the very draft
    the user just switched.
    */
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    for key in [
        "agentActivity",
        "agentSessionId",
        "agentSessionPath",
        "agentCommand",
        "launchAgentId",
        "agentName",
    ] {
        runtime_settings.remove(key);
    }
    let mut launch_settings = object_field(&session, "launchSettings");
    for key in [
        "acceptAllMode",
        "agentCommand",
        "agentLaunchPlan",
        "icon",
        "runtimeRelevant",
    ] {
        launch_settings.remove(key);
    }

    let mut create_params = Map::new();
    create_params.insert("projectId".to_string(), json!(lifecycle.project_id.clone()));
    create_params.insert("agentId".to_string(), json!(agent_id.clone()));
    // Same guard remote agent starts use: refuse an unknown agent id instead of
    // leaving the draft pointing at a CLI that will never launch.
    create_params.insert("requireLaunchCommand".to_string(), json!(true));
    // The row is STILL a draft after the switch — only its agent changed. The
    // create-time resolution strips the marker unless it is asked for, which is
    // what keeps a client from minting a draft by hand.
    create_params.insert("draft".to_string(), json!(true));
    create_params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    create_params.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    let resolved = create_agent_session_params_for_project(db, &project, &create_params)?;
    /*
    The line the live pane will be typed, read out BEFORE the row is written so
    a plan that somehow carries no command leaves the draft exactly as it was
    instead of claiming an agent nobody can launch.
    */
    let reuse_command = provider_exists
        .then(|| draft_switch_reuse_command(&resolved))
        .transpose()?;

    let mut update = lifecycle_update(&lifecycle);
    update.insert("agentId".to_string(), json!(agent_id.clone()));
    update.insert("kind".to_string(), json!("agent"));
    if let Some(value) = resolved.get("launchSettings").cloned() {
        update.insert("launchSettings".to_string(), value);
    }
    /*
    The rebuild's own default would start this row at "working", because a
    create that carries launch startup text treats the launch as work. That is
    right for a session being created to run a first prompt and wrong for a
    draft, which is being relaunched with nothing to do — and it would hand the
    new CLI's startup spinner an already-expired suppression stamp to promote
    the draft through. Seed the idle default instead: it names the NEW agent and
    arms layer 1 of CDXC:ActivitySuppressionPolicy from this instant, so the
    window is already open before the CLI below is typed or started.
    */
    let mut next_runtime_settings = resolved
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    next_runtime_settings.insert(
        "agentActivity".to_string(),
        default_activity(Some(&agent_id), None),
    );
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(next_runtime_settings),
    );
    /*
    The row's title follows the agent only while it is still the PREVIOUS
    agent's default. A title the user typed, an auto-generated one, or a
    terminal-observed one is theirs and survives the switch untouched. The
    draft-text display title is projection-level and is unaffected either way.
    */
    if let Some(title) = next_draft_agent_title(
        &project,
        &session,
        previous_agent_id.as_deref(),
        resolved.get("title").and_then(Value::as_str),
    ) {
        update.insert("title".to_string(), json!(title));
    }
    let updated = repository.update_session(&update)?;

    if let Some(command) = reuse_command {
        /*
        The new CLI boots inside a provider that never restarted, so the re-arm
        `start_session_provider` performs for every draft start never runs for
        it. Re-arm here instead, or the new CLI's boot spinner is free to stamp
        `lastActiveAt` and promote the draft the user is still typing into.
        */
        let updated = arm_draft_launch_activity_suppression(repository, &updated)?;
        /*
        Cancel first so a send the user queued against the OLD agent cannot land
        between the interrupts and the new CLI, exactly as `interruptSessionChat`
        does; then enqueue the whole reuse as ONE job on the per-session worker.
        One job because CDXC:SessionChatSerializedWriters: separate jobs may be
        separated by somebody else's, and every byte below has to reach this pty
        in order.
        */
        crate::session_chat_send::cancel_session_chat_sends(
            &lifecycle.project_id,
            &lifecycle.session_id,
        );
        crate::session_chat_send::enqueue_session_write_sequence(
            &updated,
            &lifecycle.project_id,
            &lifecycle.session_id,
            DRAFT_AGENT_SWITCH_SEND_SOURCE,
            build_draft_agent_switch_steps(&command),
        )?;
        return Ok(AgentEndpointOutput {
            presentation_session: Some((
                lifecycle.project_id.clone(),
                lifecycle.session_id.clone(),
            )),
            result: json!({
                "agentId": agent_id,
                "session": updated,
            }),
        });
    }

    let provider = dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/startSessionProvider",
        params,
        context,
        &settings,
    )?;
    let session = provider.result.get("session").cloned().unwrap_or(updated);
    Ok(AgentEndpointOutput {
        presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
        result: json!({
            "agentId": agent_id,
            "provider": provider.result,
            "session": session,
        }),
    })
}

/// Attribution for every byte an agent switch types into a live draft pane, so
/// diagnostic input logs separate them from a user's own keystrokes and from
/// chat sends.
const DRAFT_AGENT_SWITCH_SEND_SOURCE: &str = "draft-agent-switch";
/// Ctrl+C. Three of these inside one second is the exit gesture the chat-
/// supported CLIs measure (a single one only cancels the current turn).
const DRAFT_SWITCH_INTERRUPT: &str = "\u{3}";
/// Spacing between the interrupts. Wide enough that each is its own stdin
/// chunk, narrow enough that all three land inside the CLIs' one-second window.
const DRAFT_SWITCH_INTERRUPT_SPACING_MS: u64 = 150;
/// Time the interrupted pane gets to become a login shell before the new
/// agent's line is typed into it. This settles a SHELL start (`exec $SHELL -l`
/// is the last line of the provider script), not a TUI repaint, which is why it
/// is several times the chat path's settles.
const DRAFT_SWITCH_SHELL_SETTLE_MS: u64 = 800;

/// The new agent's launch line, taken from the rebuilt plan's `startupText` —
/// already leading-space prefixed so the login shell keeps it out of atuin
/// history — with the plan's own trailing Enter stripped. That Enter is a
/// separate step in the sequence below, and sending both would submit twice:
/// the second one would land in the agent CLI that the first one started.
fn draft_switch_reuse_command(resolved: &Map<String, Value>) -> Result<String, DomainStateError> {
    let command = resolved
        .get("launchSettings")
        .and_then(|settings| settings.get("agentLaunchPlan"))
        .and_then(|plan| plan.get("startupText"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_end_matches(['\r', '\n']);
    if command.trim().is_empty() {
        return Err(DomainStateError {
            code: "invalidState",
            message: "The selected agent resolved to an empty launch command, so the draft's agent was left unchanged."
                .to_string(),
        });
    }
    Ok(command.to_string())
}

/*
The keystrokes an agent switch types into a draft's LIVE pane, in the order a
person sitting at that pane would type them, as one indivisible job.

Three interrupts, not one: every chat-supported CLI reads a single Ctrl+C as
"cancel this turn" and only a repeat inside its own short window as "exit", so
one would be swallowed. The burst spans 300ms — comfortably inside the second
those CLIs measure — and the third interrupt's spacing simply folds into the
shell settle that follows it.

The command and its Enter are separate writes for the reason every server-side
writer of an agent pty keeps them separate (see SESSION_CHAT_CLEAR_INPUT_SETTLE_MS
in `session_chat_send`): a submit that coalesces into the body's stdin chunk is
inserted as literal text instead of running it.
*/
fn build_draft_agent_switch_steps(
    command: &str,
) -> Vec<crate::session_chat_send::SessionChatSendStep> {
    use crate::session_chat_send::SessionChatSendStep;
    let mut steps = Vec::new();
    for _ in 0..3 {
        steps.push(SessionChatSendStep::Write(
            DRAFT_SWITCH_INTERRUPT.to_string(),
        ));
        steps.push(SessionChatSendStep::SleepMs(
            DRAFT_SWITCH_INTERRUPT_SPACING_MS,
        ));
    }
    steps.push(SessionChatSendStep::SleepMs(DRAFT_SWITCH_SHELL_SETTLE_MS));
    steps.push(SessionChatSendStep::Write(command.to_string()));
    steps.push(SessionChatSendStep::SleepMs(
        crate::session_chat_send::SESSION_CHAT_SUBMIT_DELAY_MS,
    ));
    steps.push(SessionChatSendStep::Write(
        crate::session_chat_send::SESSION_CHAT_SUBMIT.to_string(),
    ));
    steps
}

fn next_draft_agent_title(
    project: &Value,
    session: &Value,
    previous_agent_id: Option<&str>,
    next_title: Option<&str>,
) -> Option<String> {
    let next_title = next_title?;
    let current_title = read_text_value(session, "title")?;
    let previous_default = create_agent_session_default_title(
        read_text_from_map(
            &resolve_project_agent_config(project, previous_agent_id.unwrap_or_default(), None),
            "name",
        )
        .as_deref(),
        previous_agent_id,
    );
    (current_title == previous_default && current_title != next_title)
        .then(|| next_title.to_string())
}

/*
CDXC:DraftSessions 2026-08-28 (empty-draft discard):
Drafts nobody typed into are disposable, and a daemon that was killed while one
was open never got the client's navigate-away discard. Sweep them ONCE at boot,
before the title observers and chat followers below subscribe to sessions, so a
crash cannot accumulate abandoned rows and their zmx daemons across restarts.

"Empty" is the SYNCED draft content: the same cross-device text the sidebar
title is derived from. A draft with any text is permanent until it is sent or
deleted, which is the promise the composer makes when it stops warning about
losing work.
*/
pub(crate) fn sweep_empty_draft_sessions(
    repository: &DomainRepository<'_>,
    db: &Connection,
    home_dir: &Path,
) -> Result<Vec<(String, String)>, DomainStateError> {
    let mut removed = Vec::new();
    let mut targets = Vec::new();
    for session in repository.list_sessions(None)? {
        if !session_is_draft(&session) {
            continue;
        }
        let (Some(project_id), Some(session_id)) = (
            read_text_value(&session, "projectId"),
            read_text_value(&session, "sessionId"),
        ) else {
            continue;
        };
        /*
        Only a CONFIRMED-empty draft is disposable. A read that failed says
        nothing about whether the user typed into this session, and deleting on
        that answer would destroy their unsent prompt — so an error skips the
        row and leaves it for the next boot, exactly like a draft with text.
        */
        match crate::session_chat_queue::read_session_chat_draft_content(
            db,
            &project_id,
            &session_id,
        ) {
            Ok(None) => targets.push((project_id, session_id, session)),
            Ok(Some(_)) | Err(_) => continue,
        }
    }
    for (project_id, session_id, session) in targets {
        kill_draft_session_provider(&session);
        let mut remove_params = Map::new();
        remove_params.insert("projectId".to_string(), json!(project_id.clone()));
        remove_params.insert("sessionId".to_string(), json!(session_id.clone()));
        if repository.remove_session(&remove_params).is_ok() {
            /*
            A quick chat's workspace exists only to hold that one draft, so
            sweeping the draft has to sweep the workspace too or the sidebar
            fills with empty "Chat …" projects across restarts. A no-op unless
            this project is a quick one that now has no sessions at all.
            */
            let _ = discard_stranded_quick_project(repository, home_dir, &project_id);
            removed.push((project_id, session_id));
        }
    }
    Ok(removed)
}
