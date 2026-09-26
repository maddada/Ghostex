//! Full Reload over a set: a project's inactive zmx rows, or a user-made group's members.
//!
//! Neither payload is new behaviour. `fullReloadProjectZmxSessions` and `fullReloadGroup` resolve a
//! SET and an ORDER and then run the single-session Full Reload on each row, one at a time, which
//! is what `for (const id of ids) await this.fullReloadSession(id)` does. So the plan here is the
//! list of `fullReloadSession` payloads, and each one is answered by the path that already owns it
//! (`reload.rs` for a local row, `remote.rs` for a remote one), with its own two legs, remount and
//! stop rule.
//!
//! **The set stops at the first reload that fails.** That loop has no `try`, so a reload whose call
//! rejects (its sleep, or its wake) rejects the whole payload and every row after it is left
//! alone. A reload the daemon DECLINED is not a failure and the set goes on. `step_after` is that
//! rule, in one place for the host (and, until the TypeScript was deleted, for the parity gate).
//!
//! Ported from `fullReloadProjectZmxSessions` and `fullReloadWorkspaceGroup` in the deleted
//! `gxserver-runtime/sessions-and-focus.ts` (see git history).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_reload.rs.

use serde_json::{json, Value};

use crate::core::Core;
use crate::keys::{parse_workspace_subgroup_id, ProjectKey, SessionKey};
use crate::workspace_groups::WorkspaceGroupsDocument;

use super::bulk::{is_inactive, project_rows};
use super::resolve::text_field;

use ghostex_gx_protocol::SessionPersistenceProvider;

/// The payloads this file answers.
pub const RELOAD_SET_MESSAGE_TYPES: [&str; 2] = ["fullReloadProjectZmxSessions", "fullReloadGroup"];

/// The rows a set reload reloads, in the order it reloads them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReloadSetPlan {
    /// `fullReloadSession` payloads, which the single-session paths answer.
    pub messages: Vec<Value>,
}

impl ReloadSetPlan {
    /// The row after row `index`, or `None` when the set is done. `completed` is whether that row's
    /// reload came home without a failed call.
    pub fn step_after(&self, index: usize, completed: bool) -> Option<usize> {
        (completed && index + 1 < self.messages.len()).then_some(index + 1)
    }

    pub fn to_json(&self) -> Value {
        Value::Array(self.messages.clone())
    }
}

pub fn owns_reload_set_message(message: &Value) -> bool {
    text_field(message, "type").is_some_and(|kind| RELOAD_SET_MESSAGE_TYPES.contains(&kind))
}

/// The set, or `None` when the store must not answer the payload.
///
/// `document` is the workspace session groups document the host holds, which is the one the page
/// reads as `this.workspaceGroups`: the page's copy is handed back from it after every change.
///
/// Refused, with the reason at each refusal:
///
/// - **A group id that does not parse** as a project group, which is the TypeScript's own early
///   return, so the hand-off costs nothing.
/// - **A project on a machine the store holds no rows for.** This computer's is
///   `if (!this.presentation) return`; a remote machine's is answered by the old runtime from the
///   last-seen copy it keeps (declared difference 17), which the store does not have.
///
/// A user-made group needs no presentation at all: its members come from the document, every one
/// of them, whether or not the row still exists. A member the daemon no longer knows fails its
/// reload and stops the set there, exactly as it does in the TypeScript.
pub fn plan_reload_set(
    core: &Core,
    document: &WorkspaceGroupsDocument,
    message: &Value,
) -> Option<ReloadSetPlan> {
    if !owns_reload_set_message(message) {
        return None;
    }
    let group_id = text_field(message, "groupId")?;
    // `fullReloadWorkspaceGroup`: a user-made group reloads its members, and anything else is the
    // project form.
    if text_field(message, "type") == Some("fullReloadGroup") {
        if let Some((project, subgroup_id)) = parse_workspace_subgroup_id(group_id) {
            let members = document
                .projects
                .get(&project.to_workspace_project_id())
                .and_then(|groups| {
                    groups
                        .groups
                        .iter()
                        .find(|group| group.group_id == subgroup_id)
                })
                .map(|group| group.session_ids.clone())
                .unwrap_or_default();
            return Some(reload_each(&project, members));
        }
    }
    let project = ProjectKey::parse_sidebar_group_id(group_id)?;
    // `loaded_live`, not `loaded`: `fullReloadProjectZmxSessions` resolves its rows from
    // `this.remotePresentations`, which an offline remote machine is absent from, so it reloads
    // nothing there. The same rule and the same reason as the project-scoped bulk sets.
    core.presentation().loaded_live(&project.machine)?;
    // `sessionPersistenceProvider === 'zmx' && isGpuiInactiveProjectPresentationSession(session)`,
    // in the daemon's own array order, through the one function the bulk sets use for it.
    let rows = project_rows(core, &project, |row| {
        row.session_persistence_provider == Some(SessionPersistenceProvider::Zmx)
            && is_inactive(row)
    });
    Some(reload_each(&project, rows))
}

fn reload_each(project: &ProjectKey, session_ids: Vec<String>) -> ReloadSetPlan {
    ReloadSetPlan {
        messages: session_ids
            .into_iter()
            .map(|session_id| {
                json!({
                    "type": "fullReloadSession",
                    "sessionId": SessionKey {
                        machine: project.machine.clone(),
                        project_id: project.project_id.clone(),
                        session_id,
                    }
                    .to_sidebar_session_id(),
                })
            })
            .collect(),
    }
}
