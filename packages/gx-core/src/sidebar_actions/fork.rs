//! Fork: the action that creates a session and moves a pane, and the one where nothing local
//! happens until the daemon has already done the work.
//!
//! CDXC:SessionFork 2026-09-20 WHY:
//! The question asked of every destructive or pane-moving action in this port is "what does the
//! user see if the call fails after the local change", and for fork the answer is that the case
//! does not exist. The pane move is `postLocalWorkspaceTerminalFocus` with the source row as its
//! placement target, and it runs only after `/api/forkSession` has returned a fork WITH a session
//! id. There is no window in which a pane has moved and the call then fails, so there is nothing
//! to reverse and no optimistic row to take back.
//!
//! One thing DOES happen before the call and is not reversed: the source session's project and
//! group become active. That is kept rather than fixed. It is not a pane and not a row, it is
//! where the user is looking, and a failed fork that also threw the user back to another project
//! would be a second surprise on top of the error. The TypeScript left it too, and the toast is
//! what says the fork did not happen.
//!
//! Ported from `forkSession` in the deleted `gxserver-runtime/sessions-and-focus.ts`.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_lifecycle.rs.

use serde_json::{json, Value};

use crate::core::Core;
use crate::focus::ActiveGroup;
use crate::keys::SessionKey;
use crate::workspace_groups::WorkspaceGroupsDocument;

use super::plan::ToastLevel;
use super::resolve::text_field;

/// What the host must call, and the one thing that happens before it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkRequest {
    pub session: SessionKey,
    pub rpc_path: &'static str,
    pub rpc_params: Value,
    /// The SOURCE group to make active before the call, when it is not active already. `None`
    /// means the user is already looking at the right place. It is a GROUP and not just a project
    /// because the TypeScript compares both and a project can be active with another of its groups
    /// selected, and because the source of a fork from inside a user-made group is that group.
    pub activate: Option<ActiveGroup>,
    /// The user-made group the SOURCE row sits in, which the forked session joins once the daemon
    /// has answered. `None` for a row in the project's own list, which is most of them.
    pub source_subgroup: Option<String>,
}

impl ForkRequest {
    pub fn to_json(&self) -> Value {
        json!({
            "rpc": { "path": self.rpc_path, "params": self.rpc_params },
            "activate": self.activate.as_ref().map(ActiveGroup::to_sidebar_group_id),
            "sourceSubgroup": self.source_subgroup,
        })
    }
}

/// What the host does once the daemon has answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForkFollowUp {
    /// The workspace session groups document after the forked session joined the source row's
    /// user-made group. It goes through the same `WorkspaceGroupsSync::edit` an order write uses,
    /// and it comes FIRST, because the selection that follows reads which group the new session is
    /// in and would otherwise land on the project's own group for one frame.
    EditDocument { document: WorkspaceGroupsDocument },
    /// Select the new session and append its pane beside the row it was forked from.
    PlacePane {
        session: SessionKey,
        placement_target: SessionKey,
    },
    /// `postSidebarActionToast('error', 'Could not fork session', ...)`.
    Toast {
        level: ToastLevel,
        title: String,
        /// Whether the toast carries the failure text. The text itself is the daemon's or the
        /// transport's and the two clients word it differently by construction, so it is reported
        /// as presence and never compared.
        has_description: bool,
    },
}

impl ForkFollowUp {
    pub fn to_json(&self) -> Value {
        match self {
            Self::EditDocument { document } => {
                json!({ "follow": "editDocument", "document": document.to_json() })
            }
            Self::PlacePane {
                session,
                placement_target,
            } => json!({
                "follow": "placePane",
                "session": session.to_sidebar_session_id(),
                "placementTarget": placement_target.to_sidebar_session_id(),
            }),
            Self::Toast {
                level,
                title,
                has_description,
            } => json!({
                "follow": "toast",
                "level": level.as_str(),
                "title": title,
                "hasDescription": has_description,
            }),
        }
    }
}

/// The `forkSession` payload, or `None` when this file does not own it.
///
/// Refused, with the reason at each refusal:
///
/// - A REMOTE row, which `remote.rs` answers: the fork is made by that machine's daemon over its
///   tunnel, with the same body (`fork_params`) and none of the activation or pane placement below,
///   because the remote leg of `forkSession` has neither.
/// - A row the store does not hold: the TypeScript returns before the call for exactly this,
///   because a fork of a row that is not in the presentation has no source.
/// A row inside a USER-MADE session group is NOT refused any more. The reason it was, that the
/// fork is also written into the workspace session groups document and that document had a second
/// writer, stopped holding when the store became its owner: the edit goes through the same
/// `WorkspaceGroupsSync` an order write uses, with the same debounce, retry and pending-push guard,
/// and the host asks for the stored document before it plans, as every other reader of it does.
pub fn plan_fork_request(core: &Core, message: &Value) -> Option<ForkRequest> {
    if text_field(message, "type")? != "forkSession" {
        return None;
    }
    let session = SessionKey::parse_sidebar_session_id(text_field(message, "sessionId")?)?;
    if !session.machine.is_local() {
        return None;
    }
    // `this.presentation?.sessions.some(...)`: the daemon's own row, overlays not consulted,
    // because what the fork needs is a source the daemon knows about.
    core.presentation()
        .loaded(&session.machine)?
        .server_session(&session.project_id, &session.session_id)?;
    let project = session.project_key();
    // CDXC:SessionFork 2026-09-25 WHY:
    // A CHAT project's session used to be refused here and left to the old runtime, which
    // activated `createGxserverPresentationProjectGroupId(projectId)`, a group the sidebar draws
    // no row for. The runtime is going away (its session controls' Fork now lands here too), so a
    // chat session forks like any other: `group_of_session` names the Chats group, which the host
    // does not activate, and the fork is placed beside its source.
    // `workspaceSubgroupSidebarIdForSession(projectId, sessionId) ?? the project's own group id`:
    // the source group is the row's, not the user's. `group_of_session` is the same question the
    // focus asks, over the same document, so a fork and a click cannot disagree about which group
    // a row is in.
    let source_group = core.presentation().group_of_session(&session);
    let source_subgroup = match &source_group {
        ActiveGroup::Subgroup { group_id, .. } => Some(group_id.clone()),
        _ => None,
    };
    let focus = core.focus();
    let already_active = focus.active_project.as_ref() == Some(&project)
        && focus.active_group.as_ref() == Some(&source_group);
    Some(ForkRequest {
        rpc_path: "/api/forkSession",
        rpc_params: fork_params(&session),
        activate: (!already_active).then_some(source_group),
        source_subgroup,
        session,
    })
}

/// The body of `/api/forkSession`, which is the same on either machine: that machine's raw ids.
pub(super) fn fork_params(session: &SessionKey) -> Value {
    json!({
        "projectId": session.project_id,
        "reason": "gpui-sidebar",
        "sessionId": session.session_id,
    })
}

/// What to do with the answer. `/api/forkSession` answers `{ fork: { session: { sessionId } } }`,
/// and a result without that id is a failure even though the call succeeded: the TypeScript
/// throws its own error for it and lands in the same toast.
///
/// The document is read HERE and not when the fork was planned, because the TypeScript reads
/// `this.workspaceGroups` at this moment too: a group made, renamed or emptied while the call was
/// in flight is part of the document the fork joins.
pub fn apply_fork_answer(
    document: &WorkspaceGroupsDocument,
    request: &ForkRequest,
    result: Result<&Value, &str>,
) -> Vec<ForkFollowUp> {
    let forked = result.ok().and_then(|value| {
        let session_id = value.pointer("/fork/session/sessionId")?.as_str()?;
        // `normalizeNonEmptyString`: a blank id is no id.
        (!session_id.trim().is_empty()).then(|| session_id.to_string())
    });
    let Some(session_id) = forked else {
        return vec![ForkFollowUp::Toast {
            level: ToastLevel::Error,
            title: "Could not fork session".to_string(),
            has_description: true,
        }];
    };
    let session = SessionKey {
        machine: request.session.machine.clone(),
        project_id: request.session.project_id.clone(),
        session_id,
    };
    let mut follow_ups = Vec::new();
    // `moveGpuiWorkspaceSessionToSubgroup(..., sourceSubgroup.groupId)` then
    // `persistWorkspaceGroups()`. A group that is gone by now hands back the document unchanged
    // there, and an unchanged document is not written here: the sync would book a push of what it
    // already holds, and a write that changes nothing is not a write.
    if let Some(group_id) = request.source_subgroup.as_deref() {
        let project_id = request.session.project_key().to_workspace_project_id();
        if let Some(edited) = document.move_session_to_subgroup(
            &project_id,
            &session.session_id,
            Some(group_id),
            None,
        ) {
            if &edited != document {
                follow_ups.push(ForkFollowUp::EditDocument { document: edited });
            }
        }
    }
    follow_ups.push(ForkFollowUp::PlacePane {
        session,
        // The placement target is the row the user clicked, not whichever pane happens to be
        // focused when the call comes back.
        placement_target: request.session.clone(),
    });
    follow_ups
}

/// Whether this payload is one this file answers.
pub fn owns_fork_message(message: &Value) -> bool {
    text_field(message, "type") == Some("forkSession")
}
