//! The actions that only read: the copy actions, Open Folder, and Open in Editor.
//!
//! These are the first of the roughly forty-five payloads to move into the store, because none of
//! them changes a session, writes client storage, or needs an optimistic update: each one resolves
//! an id and makes exactly one call. What they establish is the shape the rest follow, so the
//! resolution is ported line by line from the shipped TypeScript and the calls are values a gate
//! can enumerate.
//!
//! Ported from these files of the deleted `apps/desktop/sidebar/gxserver-runtime/` (see git
//! history): `core.ts` (the `handleSidebarMessage` arms), `app-shot-and-misc.ts`
//! (`postProjectPathActionForGroup`, `copyWorkspaceProjectRemoteUrl`), `sessions-and-focus.ts`
//! (`copySessionDetails`) and `remote-machines.ts` (`postRemoteSessionNativeAction`,
//! `postRemoteProjectNativeAction`, `postRemoteToast`).

use serde_json::Value;

use crate::core::Core;
use crate::keys::{ProjectKey, SessionKey};
use crate::sidebar_view::SidebarInputs;

use super::plan::{ActionEffect, SidebarActionPlan, ToastLevel};
use super::resolve::{
    local_project_group_project_id, native_project_path_action, non_empty, text_field,
};

/// Every message type this file answers. The host checks it before it handles a command (it kept
/// such commands out of the old runtime, and the deleted parity gate enumerated it), so the set
/// lives in one place.
pub const READ_ONLY_MESSAGE_TYPES: [&str; 7] = [
    "copySessionDetails",
    "copyResumeCommand",
    "copyAttachCommand",
    "copyWorkspaceProjectPathForGroup",
    "copyWorkspaceProjectRemoteUrl",
    "openWorkspaceProjectInFinderForGroup",
    "openWorkspaceProjectInIdeForGroup",
];

/// The calls one read-only message makes, or `None` when this file does not own that message.
///
/// `message` is the inner payload of a `{ type: 'command', message }` menu command, which is what
/// `GpuiSidebarRuntime.handleSidebarMessage` receives.
pub fn plan_read_only_action(
    core: &Core,
    inputs: &SidebarInputs,
    message: &Value,
) -> Option<SidebarActionPlan> {
    let kind = text_field(message, "type")?;
    match kind {
        // `copySessionDetails`: the text the menu built, straight to the clipboard. The value is
        // NOT trimmed on the way, because `normalizeNonEmptyString` tests the trim and returns the
        // original string.
        "copySessionDetails" => Some(copy_text(text_field(message, "detailsText"))),
        // `copyWorkspaceProjectRemoteUrl`: the same call with the menu's URL.
        "copyWorkspaceProjectRemoteUrl" => Some(copy_text(text_field(message, "remoteUrl"))),
        // Both command copies exist for REMOTE sessions only, on either side: a local session's
        // rows are built but the runtime answers them with `handleUnsupportedSidebarMessage`.
        "copyResumeCommand" => Some(remote_session_action(message, "copyRemoteResumeCommand")),
        "copyAttachCommand" => Some(remote_session_action(message, "copyRemoteAttachCommand")),
        "copyWorkspaceProjectPathForGroup" => Some(project_path_action(
            core,
            inputs,
            message,
            ProjectPathAction::CopyPath,
        )),
        "openWorkspaceProjectInFinderForGroup" => Some(project_path_action(
            core,
            inputs,
            message,
            ProjectPathAction::OpenInFinder,
        )),
        "openWorkspaceProjectInIdeForGroup" => Some(project_path_action(
            core,
            inputs,
            message,
            ProjectPathAction::OpenInIde,
        )),
        _ => None,
    }
}

fn copy_text(value: Option<&str>) -> SidebarActionPlan {
    match non_empty(value) {
        Some(text) => SidebarActionPlan::one(ActionEffect::CopyText {
            text: text.to_string(),
        }),
        None => SidebarActionPlan::nothing(),
    }
}

fn remote_session_action(message: &Value, action: &str) -> SidebarActionPlan {
    let Some(session) = text_field(message, "sessionId")
        .and_then(SessionKey::parse_remote_scoped_session_id)
        .filter(|session| !session.machine.is_local())
    else {
        return SidebarActionPlan::nothing();
    };
    // The bridge is handed the machine-scoped id back, which is the string it was parsed from;
    // `postRemoteSessionNativeAction` rebuilds it the same way.
    match native_project_path_action(action, &session.to_focus_state_session_id()) {
        Some(effect) => SidebarActionPlan::one(effect),
        None => SidebarActionPlan::nothing(),
    }
}

/// The three actions `postProjectPathActionForGroup` accepts, with what each one does on a remote
/// project. Only Copy Path and Open in Editor have a remote form; the local file manager cannot
/// open a path on another computer, and the TypeScript says so in a toast rather than failing
/// silently.
#[derive(Clone, Copy)]
enum ProjectPathAction {
    CopyPath,
    OpenInFinder,
    OpenInIde,
}

impl ProjectPathAction {
    fn local_action(self) -> &'static str {
        match self {
            Self::CopyPath => "copyWorkspaceProjectPath",
            Self::OpenInFinder => "openWorkspaceProjectInFinder",
            Self::OpenInIde => "openWorkspaceProjectInIde",
        }
    }

    fn remote_action(self) -> Option<&'static str> {
        match self {
            Self::CopyPath => Some("copyRemoteProjectPath"),
            Self::OpenInIde => Some("openRemoteWorkspaceProjectInIde"),
            Self::OpenInFinder => None,
        }
    }
}

fn project_path_action(
    core: &Core,
    inputs: &SidebarInputs,
    message: &Value,
    action: ProjectPathAction,
) -> SidebarActionPlan {
    let Some(group_id) = text_field(message, "groupId") else {
        return SidebarActionPlan::nothing();
    };
    let remote =
        ProjectKey::parse_sidebar_group_id(group_id).filter(|project| !project.machine.is_local());
    if let Some(project) = remote {
        let Some(remote_action) = action.remote_action() else {
            return SidebarActionPlan::one(ActionEffect::Toast {
                level: ToastLevel::Warning,
                title: "Remote project open unavailable".to_string(),
                description: Some(
                    "Remote project locations cannot be opened in the local file manager."
                        .to_string(),
                ),
            });
        };
        return match native_project_path_action(remote_action, &project.to_workspace_project_id()) {
            Some(effect) => SidebarActionPlan::one(effect),
            None => SidebarActionPlan::nothing(),
        };
    }
    let Some(project_id) = local_project_group_project_id(core, inputs, group_id) else {
        return SidebarActionPlan::nothing();
    };
    match native_project_path_action(action.local_action(), &project_id) {
        Some(effect) => SidebarActionPlan::one(effect),
        None => SidebarActionPlan::nothing(),
    }
}
