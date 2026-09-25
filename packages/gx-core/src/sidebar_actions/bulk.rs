//! The bulk, project and collection actions: which rows they act on, and in what order.
//!
//! CDXC:Sessions 2026-09-20 WHY:
//! Almost nothing here is new behaviour. Every one of these payloads ends in the per-session
//! actions the store already owns (sleep, wake, close), so a port that reimplemented them would
//! have written a second copy of the declined leg, the replacement focus and the echo guard. What
//! these payloads really are is a SET and an ORDER, and that is what this file answers: which rows
//! the action touches, in which order, and whether the fan-out is paced.
//!
//! Two shapes reach it.
//!
//! - A `batch` is the renderer's own envelope. The bulk menu and every collection menu build one
//!   ("Sleep Selected", a collection's Pin, Tag or Full Reload) as a list of ordinary per-session
//!   messages, optionally clearing the multi-selection first, and the controller posts each one.
//!   So the batch needs no set of its own and no pacing: it is exactly the messages the menu built.
//! - The plural payloads (`setSessionsSleeping`, `closeSessions`, `setGroupSleeping`,
//!   `sleepInactiveProjectSessions`, `closeInactiveProjectSessions`,
//!   `wakeProjectSleepingSessions`) resolve a set here and then fan out into the same per-session
//!   messages.
//!
//! **The pacing is a decision, not an implementation detail.** A bulk SLEEP through
//! `setSessionsSleeping` runs one request at a time with 350 ms between them, and wake and close do
//! not (`CDXC:SessionSleep 2026-06-27-02:05`: restoring a session needs no terminal teardown
//! throttling). A batch of per-session sleeps from the bulk MENU is not paced either, because it
//! never goes through the plural payload. Getting that backwards is invisible in any list
//! comparison and would either hammer the daemon or make Sleep Selected feel broken, so the plan
//! carries the interval (the parity gate compared it while the TypeScript still ran).
//!
//! Ported from `setSessionsSleeping`, `setGroupSleeping`, `collectInactiveProjectSessionIds` and
//! `wakeProjectSleepingSessions` in the deleted `gxserver-runtime/auto-sleep.ts` (frozen for the
//! gates in the since-deleted `tooling/gx-core/bulk-sleep-pacing-frozen.ts`) and the deleted
//! sidebar page's `controller.ts` (the `batch` arm).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_bulk.rs.

use serde_json::{Map, Value, json};

use crate::core::Core;
use crate::keys::{ProjectKey, SessionKey};

use super::resolve::text_field;

use ghostex_gx_protocol::{LifecycleState, SessionActivity};

/// `GPUI_SIDEBAR_BULK_SLEEP_INTERVAL_MS`.
pub const BULK_SLEEP_INTERVAL_MS: u64 = 350;

/// What the renderer's `batch` envelope asks for: clear the multi-selection, then post each
/// message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatchPlan {
    pub clear_selection: bool,
    pub messages: Vec<Value>,
}

impl BatchPlan {
    pub fn to_json(&self) -> Value {
        json!({ "clearSelection": self.clear_selection, "messages": self.messages })
    }
}

/// The `batch` command, or `None` when this is not one.
///
/// The messages are passed on exactly as the menu built them, in order. Nothing is resolved,
/// filtered or reordered here: the menu already decided which rows it offers the action for, which
/// is what `createNativeBulkMenu` and `createNativeCollectionMenu` do, and re-deciding it would be
/// a second place for the two to differ.
pub fn plan_batch(command: &Value) -> Option<BatchPlan> {
    if text_field(command, "type")? != "batch" {
        return None;
    }
    let messages = command.get("messages")?.as_array()?.clone();
    Some(BatchPlan {
        clear_selection: command
            .get("clearSelection")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        messages,
    })
}

/// Which per-session action a plural payload fans out into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulkAction {
    Sleep,
    Wake,
    Close,
}

impl BulkAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sleep => "sleep",
            Self::Wake => "wake",
            Self::Close => "close",
        }
    }

    /// The per-session message the fan-out posts, which is the payload the single-session path
    /// already answers.
    fn message(self, sidebar_session_id: &str) -> Value {
        match self {
            Self::Sleep => json!({
                "type": "setSessionSleeping",
                "sessionId": sidebar_session_id,
                "sleeping": true,
            }),
            Self::Wake => json!({
                "type": "setSessionSleeping",
                "sessionId": sidebar_session_id,
                "sleeping": false,
            }),
            Self::Close => json!({ "type": "closeSession", "sessionId": sidebar_session_id }),
        }
    }
}

/// A plural payload, resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkRequest {
    pub action: BulkAction,
    /// The per-session messages, in the order the TypeScript issues them.
    pub messages: Vec<Value>,
    /// Milliseconds between requests, or zero for the concurrent fan-out.
    pub interval_ms: u64,
    /// `focusProjectId` runs BEFORE the fan-out for a project wake, and for nothing else. A REMOTE
    /// project wake leaves it `None`: the remote branch of `wakeProjectSleepingSessions` never
    /// moves the active project.
    pub focus_project: Option<ProjectKey>,
    /// The project a project-scoped payload resolved, or `None` for the two explicit id lists,
    /// whose ids can name any machine. Kept so a host can report which machine acted without
    /// reading an id back out of a message.
    pub project: Option<ProjectKey>,
}

impl BulkRequest {
    /// Whether each request must come HOME before the interval starts and the next one goes out.
    ///
    /// CDXC:SessionSleep 2026-09-21 WHY:
    /// `runGpuiSidebarBulkSleepPaced` awaits `sleepTarget` and only then waits the interval, so a
    /// paced sleep is one request at a time in the strict sense: the second is not sent while the
    /// first is still in flight. The host used to post one message every 350 ms without waiting,
    /// which overlaps teardowns whenever a sleep takes longer than the interval, and a remote sleep
    /// always does (the tunnel, then the machine's own snapshot re-read). Wake and close go through
    /// `Promise.all` and wait for nothing.
    pub fn waits_for_each(&self) -> bool {
        self.interval_ms > 0
    }

    /// Whether the resolved project is on a remote machine. `false` for the two explicit id lists,
    /// which carry no project of their own.
    pub fn is_remote_project(&self) -> bool {
        self.project
            .as_ref()
            .is_some_and(|project| !project.machine.is_local())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "action": self.action.as_str(),
            "intervalMs": self.interval_ms,
            "waitsForEach": self.waits_for_each(),
            "remoteProject": self.is_remote_project(),
            "focusProject": self
                .focus_project
                .as_ref()
                .map(ProjectKey::to_workspace_project_id),
            "messages": self.messages,
        })
    }
}

/// Every plural payload this file answers.
pub const BULK_MESSAGE_TYPES: [&str; 6] = [
    "setSessionsSleeping",
    "closeSessions",
    "setGroupSleeping",
    "sleepInactiveProjectSessions",
    "closeInactiveProjectSessions",
    "wakeProjectSleepingSessions",
];

/// Whether this payload is one this file answers, without resolving anything.
pub fn owns_bulk_message(message: &Value) -> bool {
    text_field(message, "type").is_some_and(|kind| BULK_MESSAGE_TYPES.contains(&kind))
}

/// Whether this command is the renderer's batch envelope.
pub fn owns_batch_command(command: &Value) -> bool {
    text_field(command, "type") == Some("batch")
}

/// The plural payloads, or `None` when this file does not own one.
///
/// CDXC:SessionSleep 2026-09-21 DECISION:
/// Asked whether Sleep, Wake, Sleep Inactive and Close Inactive on a whole project should also
/// affect that project's browser tabs, the user chose SESSIONS ONLY: "Project actions touch only
/// the project's sessions; browser tabs are managed from the view tab strip, where they live now."
/// So these four sets are the project's daemon rows and nothing else, on this computer and on a
/// remote machine, in the store and in the old runtime alike. This supersedes the browser hand-off
/// refusal of 2026-09-20 and its `BrowserTabsInput` "not supplied refuses" guard, both removed: the
/// payloads no longer read an app-tab list, so there is nothing for a missing one to be mistaken
/// for. Wake used to open each sleeping tab in the Browser view one after another, which is what
/// "waking" a tab meant there.
///
/// A REMOTE group is answered here too, since the per-row legs were ported: each row becomes the
/// same `setSessionSleeping` or `closeSession` message a local row does, carrying that machine's
/// scoped session id, and the host sends it down that machine's tunnel. The one thing the remote
/// branch does NOT do is move the active project: `wakeProjectSleepingSessions` calls
/// `focusProjectId` only on its local branch, so a remote project wake wakes the rows and leaves
/// the user where they are.
///
/// Refused, with the reason at each refusal:
///
/// - **A machine whose rows this store has not loaded.** For this computer that is
///   `!this.presentation`, an early return that happens BEFORE a project wake moves the active
///   project. For a remote machine the reason is different and stronger: the old runtime keeps the
///   LAST SEEN presentation of a machine that is not connected, so it can still resolve a set the
///   store has no rows for, and answering with zero rows would silently do nothing where the old
///   runtime acts.
/// - **A user-made session group** (`gpui-wsg:`), whose membership is the workspace session groups
///   document, the fourth client-storage key with its own writer and its own pending-push guard.
/// - **A group id that does not parse**, which is the TypeScript's own early return.
/// - **An explicit id list naming a row this store cannot resolve** is NOT refused: the plural
///   payloads parse each id independently and the single-session path answers each one, exactly as
///   the fan-out does there.
///
/// Ported from `setGroupSleeping`, `collectInactiveProjectSessionIds` and
/// `wakeProjectSleepingSessions` in `gxserver-runtime/auto-sleep.ts` (deleted with QuickJS on
/// 2026-09-25).
///
/// SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_bulk.rs.
pub fn plan_bulk_request(core: &Core, message: &Value) -> Option<BulkRequest> {
    match text_field(message, "type")? {
        // An explicit list from a multi-selection. The ids are the menu's own and are fanned out
        // untouched; only the direction decides the pacing.
        "setSessionsSleeping" => {
            let sleeping = message.get("sleeping")?.as_bool()?;
            let ids = explicit_session_ids(message)?;
            let action = match sleeping {
                true => BulkAction::Sleep,
                false => BulkAction::Wake,
            };
            Some(bulk(action, ids, None, None))
        }
        "closeSessions" => {
            let ids = explicit_session_ids(message)?;
            Some(bulk(BulkAction::Close, ids, None, None))
        }
        // The project-scoped ones resolve their own set.
        kind => {
            let project = project_of_group(message)?;
            // `if (!projectId || !this.presentation) return`. Three of the four payloads would
            // answer a machine with no presentation with an empty set, which is what the early
            // return does anyway, but `wakeProjectSleepingSessions` moves the active project FIRST
            // and the early return happens before it: answering here would jump the user to a
            // project whose rows nobody has yet. This is the store's not-loaded state, not an empty
            // one, and it is the refusal PLAN.md asks for rather than a guess at zero rows.
            //
            // CORRECTED 2026-09-21: the reason written here for the REMOTE side was wrong, and it
            // mattered the moment the store started holding last-seen rows. It said the old runtime
            // can resolve the set from its last-seen copy, so a refusal hands it work. It cannot:
            // all four payloads read `this.remotePresentations`, which a machine that has not
            // streamed in this run is absent from, while the copy it DRAWS is the separate
            // `remoteLastSeenPresentations`. So a project action on an offline remote machine does
            // nothing over there, and `loaded_live` keeps it doing nothing here rather than firing
            // one doomed request per row down a tunnel that does not exist.
            core.presentation().loaded_live(&project.machine)?;
            let (action, rows) = match kind {
                "setGroupSleeping" => {
                    let sleeping = message.get("sleeping")?.as_bool()?;
                    let action = match sleeping {
                        true => BulkAction::Sleep,
                        false => BulkAction::Wake,
                    };
                    // `lifecycleState === (sleeping ? 'running' : 'sleeping')`: only the rows the
                    // action can move, so a group of sleeping sessions asked to sleep calls
                    // nothing at all.
                    let wanted = match sleeping {
                        true => LifecycleState::Running,
                        false => LifecycleState::Sleeping,
                    };
                    (
                        action,
                        project_rows(core, &project, |row| row.lifecycle_state == wanted),
                    )
                }
                "wakeProjectSleepingSessions" => (
                    BulkAction::Wake,
                    project_rows(core, &project, |row| {
                        row.lifecycle_state == LifecycleState::Sleeping
                    }),
                ),
                "sleepInactiveProjectSessions" => {
                    (BulkAction::Sleep, project_rows(core, &project, is_inactive))
                }
                "closeInactiveProjectSessions" => {
                    (BulkAction::Close, project_rows(core, &project, is_inactive))
                }
                _ => return None,
            };
            let ids = rows
                .into_iter()
                .map(|session_id| {
                    SessionKey {
                        machine: project.machine.clone(),
                        project_id: project.project_id.clone(),
                        session_id,
                    }
                    .to_sidebar_session_id()
                })
                .collect();
            // Only the project wake moves the active project, and it does so BEFORE the fan-out,
            // which is why it is part of the request rather than a follow-up. `focusProjectId` is
            // on the LOCAL branch of `wakeProjectSleepingSessions` only; the remote branch resolves
            // its set and calls nothing else, so a remote wake must not move the user.
            let focus = match kind {
                "wakeProjectSleepingSessions" if project.machine.is_local() => {
                    Some(project.clone())
                }
                _ => None,
            };
            Some(bulk(action, ids, focus, Some(project)))
        }
    }
}

/// The plan for one resolved set. The pacing rule lives here so the four project payloads and the
/// two explicit ones cannot answer it differently.
fn bulk(
    action: BulkAction,
    ids: Vec<String>,
    focus_project: Option<ProjectKey>,
    project: Option<ProjectKey>,
) -> BulkRequest {
    BulkRequest {
        messages: ids.iter().map(|id| action.message(id)).collect(),
        // `runGpuiSidebarBulkSleepPaced` is reached only by the SLEEP direction of
        // `setSessionsSleeping`; wake and close go through `Promise.all`.
        interval_ms: match action {
            BulkAction::Sleep => BULK_SLEEP_INTERVAL_MS,
            _ => 0,
        },
        action,
        focus_project,
        project,
    }
}

fn explicit_session_ids(message: &Value) -> Option<Vec<String>> {
    Some(
        message
            .get("sessionIds")?
            .as_array()?
            .iter()
            .filter_map(|id| id.as_str().map(str::to_string))
            .collect(),
    )
}

/// `parseGpuiRemotePresentationGroupId` then `parseGxserverPresentationProjectGroupId`, which are
/// string parses and ask the store nothing. A user-made session group (`gpui-wsg:`), the Chats
/// collection and anything else that is not a project group fail the parse and are refused here.
fn project_of_group(message: &Value) -> Option<ProjectKey> {
    let group_id = text_field(message, "groupId")?;
    ProjectKey::parse_sidebar_group_id(group_id)
}

/// `isGpuiInactiveProjectPresentationSession`: awake, and neither working nor waiting on the user.
/// Stopped history that is pinned, tagged or starred stays in the presentation and is deliberately
/// NOT included, because sleeping it would promote it back into the active shelf.
/// CDXC:SessionSleep 2026-09-24 DECISION:
/// User: a session with a background shell or monitor still running (the grey dot) is not inactive; Sleep Inactive and Close Inactive leave it alone.
pub(super) fn is_inactive(row: &ghostex_gx_protocol::PresentationSession) -> bool {
    row.lifecycle_state == LifecycleState::Running
        && row.activity != SessionActivity::Working
        && row.activity != SessionActivity::Attention
        && row.background_work_detected_at.is_none()
}

/// The project's rows that pass a test, in the daemon's own array order.
///
/// The order is rebuilt from `sortKey` rather than read off a list, for the reason
/// `localProjectTransitionSessionIds` gives: the store keeps rows by id, the daemon orders its
/// array by the byte order of that key, and a store built from deltas has no array order to read.
/// It matters here because the order is the order the requests go out in, and a paced sleep makes
/// that visible: the rows go to sleep one at a time, in this order, 350 ms apart.
pub(super) fn project_rows(
    core: &Core,
    project: &ProjectKey,
    keep: impl Fn(&ghostex_gx_protocol::PresentationSession) -> bool,
) -> Vec<String> {
    // `loaded_live`: every caller is behind the same refusal, and a second reader of the same
    // rule that used the looser test is how the two drift.
    let Some(loaded) = core.presentation().loaded_live(&project.machine) else {
        return Vec::new();
    };
    let mut rows: Vec<(&str, &str)> = loaded
        .server_sessions()
        .filter(|row| row.project_id == project.project_id && keep(row))
        .map(|row| (row.sort_key.as_str(), row.session_id.as_str()))
        .collect();
    rows.sort_unstable();
    rows.into_iter()
        .map(|(_, session_id)| session_id.to_string())
        .collect()
}

/// The counters a host reports, named here so the record and the gate agree on what they mean.
pub fn bulk_request_summary(request: &BulkRequest) -> Value {
    let mut summary = Map::new();
    summary.insert(
        "action".to_string(),
        Value::String(request.action.as_str().to_string()),
    );
    summary.insert(
        "rows".to_string(),
        Value::Number((request.messages.len() as u64).into()),
    );
    summary.insert(
        "intervalMs".to_string(),
        Value::Number(request.interval_ms.into()),
    );
    summary.insert(
        "focusProject".to_string(),
        Value::Bool(request.focus_project.is_some()),
    );
    summary.insert(
        "remoteProject".to_string(),
        Value::Bool(request.is_remote_project()),
    );
    Value::Object(summary)
}
