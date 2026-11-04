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
//! (`copySessionDetails`) and `remote-machines.ts` (`postRemoteProjectNativeAction`,
//! `postRemoteToast`).

use serde_json::Value;

use crate::core::Core;
use crate::keys::ProjectKey;
use crate::sidebar_view::text::js_trim;
use crate::sidebar_view::SidebarInputs;

use super::plan::{ActionEffect, SidebarActionPlan, ToastButton, ToastLevel};
use super::resolve::{
    local_project_group_project_id, native_project_path_action, non_empty, text_field,
};

/// Every message type this file answers. The host checks it before it handles a command (it kept
/// such commands out of the old runtime, and the deleted parity gate enumerated it), so the set
/// lives in one place.
pub const READ_ONLY_MESSAGE_TYPES: [&str; 7] = [
    "copySessionDetails",
    "copyText",
    "copyWorkspaceProjectPathForGroup",
    "copyWorkspaceProjectRemoteUrl",
    "openWorkspaceProjectInFinderForGroup",
    "openWorkspaceProjectInIdeForGroup",
    "openWorkspaceProjectInTargetForGroup",
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
        // `copyText`: a session menu's Copy row (a branch, a Linear ID, a link).
        "copyText" => Some(copy_text(text_field(message, "text"))),
        // `copyWorkspaceProjectRemoteUrl`: the same call with the menu's URL.
        "copyWorkspaceProjectRemoteUrl" => Some(copy_text(text_field(message, "remoteUrl"))),
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
        // A bot row's Open in names one Open In target by id; the host runs the target it finds
        // under that id in its own Settings, so no command text travels with the message.
        "openWorkspaceProjectInTargetForGroup" => Some(
            text_field(message, "targetId")
                .map(js_trim)
                .filter(|target_id| !target_id.is_empty())
                .map_or_else(SidebarActionPlan::nothing, |target_id| {
                    project_path_action(
                        core,
                        inputs,
                        message,
                        ProjectPathAction::OpenInTarget(target_id),
                    )
                }),
        ),
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

/// The actions `postProjectPathActionForGroup` accepts, plus a bot row's Open in. A remote
/// project answers each one through [`plan_remote_project_open`] or its own remote action.
#[derive(Clone, Copy)]
enum ProjectPathAction<'a> {
    CopyPath,
    OpenInFinder,
    OpenInIde,
    /// The Open In target id the row names.
    OpenInTarget(&'a str),
}

impl ProjectPathAction<'_> {
    fn local_action(self) -> &'static str {
        match self {
            Self::CopyPath => "copyWorkspaceProjectPath",
            Self::OpenInFinder => "openWorkspaceProjectInFinder",
            Self::OpenInIde => "openWorkspaceProjectInIde",
            Self::OpenInTarget(_) => "openWorkspaceProjectInTarget",
        }
    }
}

/// The Open In targets that can open a folder on another computer over SSH: the VS Code family
/// through Remote-SSH (`--remote ssh-remote+host path`) and Zed through its `ssh://` address.
/// Every other target (the JetBrains IDEs, Kiro, custom commands) can only open a path on this
/// computer.
pub const REMOTE_SSH_OPEN_TARGET_IDS: [&str; 8] = [
    "vscode",
    "vscode-insiders",
    "vscodium",
    "cursor",
    "windsurf",
    "trae",
    "antigravity",
    "zed",
];

/// The id of the Open In target that is the file manager (Finder, Explorer, Files), shown as Open
/// Location.
const FILE_MANAGER_OPEN_TARGET_ID: &str = "finder";

/// What the user asked to open a remote project in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteProjectOpen<'a> {
    /// Open Folder: the file manager.
    Folder,
    /// One Open In target, by id, with the label the menu showed for it.
    Target { id: &'a str, label: Option<&'a str> },
}

/// CDXC:RemoteMachines 2026-10-27 DECISION:
/// User (Q630): a remote project's Open Folder opens it "in Ghostex's Files view", which already
/// browses remote projects. Open In apps that can work over SSH (VS Code, Cursor, the other VS Code
/// family editors, and Zed) open the remote folder through SSH; every other app keeps a short warning
/// naming it, with a Copy Path button. The sidebar menu, the header's Open In and Quick Access all
/// plan here, so every client answers the same way.
/// Supersedes the toast "Remote project locations cannot be opened in the local file manager."
pub fn plan_remote_project_open(
    project: &ProjectKey,
    open: RemoteProjectOpen<'_>,
) -> SidebarActionPlan {
    let project_id = project.to_workspace_project_id();
    let effect = match open {
        RemoteProjectOpen::Folder => {
            native_project_path_action("openRemoteProjectInFiles", &project_id)
        }
        RemoteProjectOpen::Target { id, .. } if id == FILE_MANAGER_OPEN_TARGET_ID => {
            native_project_path_action("openRemoteProjectInFiles", &project_id)
        }
        RemoteProjectOpen::Target { id, .. } if REMOTE_SSH_OPEN_TARGET_IDS.contains(&id) => {
            let mut effect =
                native_project_path_action("openRemoteWorkspaceProjectInTarget", &project_id);
            if let Some(ActionEffect::NativeProjectPathAction { payload }) = effect.as_mut() {
                payload["targetId"] = Value::String(id.to_string());
            }
            effect
        }
        RemoteProjectOpen::Target { label, .. } => {
            let app = label
                .map(js_trim)
                .filter(|label| !label.is_empty())
                .unwrap_or("This app");
            let button = match native_project_path_action("copyRemoteProjectPath", &project_id) {
                Some(ActionEffect::NativeProjectPathAction { payload }) => Some(ToastButton {
                    label: "Copy Path".to_string(),
                    payload,
                }),
                _ => None,
            };
            Some(ActionEffect::Toast {
                level: ToastLevel::Warning,
                title: format!("{app} can't open a remote project"),
                description: Some(
                    "It only opens folders on this computer. Copy the path to use it on the remote computer."
                        .to_string(),
                ),
                button,
            })
        }
    };
    effect.map_or_else(SidebarActionPlan::nothing, SidebarActionPlan::one)
}

fn project_path_action(
    core: &Core,
    inputs: &SidebarInputs,
    message: &Value,
    action: ProjectPathAction<'_>,
) -> SidebarActionPlan {
    let Some(group_id) = text_field(message, "groupId") else {
        return SidebarActionPlan::nothing();
    };
    let remote =
        ProjectKey::parse_sidebar_group_id(group_id).filter(|project| !project.machine.is_local());
    if let Some(project) = remote {
        let remote_action = match action {
            ProjectPathAction::CopyPath => "copyRemoteProjectPath",
            ProjectPathAction::OpenInIde => "openRemoteWorkspaceProjectInIde",
            ProjectPathAction::OpenInFinder => {
                return plan_remote_project_open(&project, RemoteProjectOpen::Folder);
            }
            ProjectPathAction::OpenInTarget(target_id) => {
                return plan_remote_project_open(
                    &project,
                    RemoteProjectOpen::Target {
                        id: target_id,
                        label: text_field(message, "targetLabel"),
                    },
                );
            }
        };
        return native_project_path_action(remote_action, &project.to_workspace_project_id())
            .map_or_else(SidebarActionPlan::nothing, SidebarActionPlan::one);
    }
    let Some(project_id) = local_project_group_project_id(core, inputs, group_id) else {
        return SidebarActionPlan::nothing();
    };
    let mut effect = native_project_path_action(action.local_action(), &project_id);
    if let (
        ProjectPathAction::OpenInTarget(target_id),
        Some(ActionEffect::NativeProjectPathAction { payload }),
    ) = (action, effect.as_mut())
    {
        payload["targetId"] = Value::String(target_id.to_string());
    }
    match effect {
        Some(effect) => SidebarActionPlan::one(effect),
        None => SidebarActionPlan::nothing(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::keys::MachineId;
    use crate::sidebar_actions::resolve::{
        NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE, NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
    };

    const REMOTE_GROUP: &str = "remote:win:group:P9";
    const REMOTE_PROJECT: &str = "remote:win:project:P9";

    fn local_core() -> Core {
        let snapshot = json!({
            "type": "presentationSnapshot",
            "protocolVersion": crate::protocol::GXSERVER_PROTOCOL_VERSION,
            "serverId": "local",
            "revision": 1,
            "snapshot": {
                "revision": 1,
                "generatedAt": "2026-10-27T01:00:00.000Z",
                "projects": [{
                    "projectId": "P1",
                    "title": "P1",
                    "path": "/Users/me/P1",
                    "pathState": "available",
                    "groupIds": [],
                    "sortKey": "1:P1",
                    "createdAt": "2026-10-27T01:00:00.000Z",
                    "updatedAt": "2026-10-27T01:00:00.000Z",
                }],
                "groups": [],
                "sessions": [],
            }
        });
        let mut core = Core::new();
        core.handle_raw_frame(MachineId::Local, &snapshot.to_string(), 0)
            .expect("the frame parses");
        core
    }

    fn plan(core: &Core, message: Value) -> Value {
        plan_read_only_action(core, &SidebarInputs::default(), &message)
            .expect("a read-only message")
            .to_json()
    }

    fn native(action: &str, project_id: &str) -> Value {
        json!({ "call": "nativeProjectPathAction", "payload": {
            "action": action,
            "projectId": project_id,
            "type": NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
            "version": NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
        }})
    }

    #[test]
    fn local_project_actions_are_unchanged() {
        let core = local_core();
        let group = ProjectKey::local("P1").to_sidebar_group_id();
        assert_eq!(
            plan(
                &core,
                json!({ "type": "openWorkspaceProjectInFinderForGroup", "groupId": group })
            ),
            json!([native("openWorkspaceProjectInFinder", "P1")])
        );
        let mut in_target = native("openWorkspaceProjectInTarget", "P1");
        in_target["payload"]["targetId"] = json!("idea");
        assert_eq!(
            plan(
                &core,
                json!({ "type": "openWorkspaceProjectInTargetForGroup", "groupId": group,
                    "targetId": "idea", "targetLabel": "IntelliJ IDEA" })
            ),
            json!([in_target])
        );
        assert_eq!(
            plan(
                &core,
                json!({ "type": "copyWorkspaceProjectPathForGroup", "groupId": group })
            ),
            json!([native("copyWorkspaceProjectPath", "P1")])
        );
    }

    #[test]
    fn remote_open_folder_opens_the_files_view() {
        let core = Core::new();
        let expected = json!([native("openRemoteProjectInFiles", REMOTE_PROJECT)]);
        assert_eq!(
            plan(
                &core,
                json!({ "type": "openWorkspaceProjectInFinderForGroup", "groupId": REMOTE_GROUP })
            ),
            expected
        );
        // Open Location is the file manager too.
        assert_eq!(
            plan(
                &core,
                json!({ "type": "openWorkspaceProjectInTargetForGroup", "groupId": REMOTE_GROUP,
                    "targetId": "finder", "targetLabel": "Open Location" })
            ),
            expected
        );
    }

    #[test]
    fn remote_open_in_an_ssh_editor_opens_the_remote_folder() {
        let core = Core::new();
        for target_id in ["vscode", "cursor", "zed"] {
            let mut expected = native("openRemoteWorkspaceProjectInTarget", REMOTE_PROJECT);
            expected["payload"]["targetId"] = json!(target_id);
            assert_eq!(
                plan(
                    &core,
                    json!({ "type": "openWorkspaceProjectInTargetForGroup",
                        "groupId": REMOTE_GROUP, "targetId": target_id })
                ),
                json!([expected])
            );
        }
        // Open in Editor keeps its own remote action.
        assert_eq!(
            plan(
                &core,
                json!({ "type": "openWorkspaceProjectInIdeForGroup", "groupId": REMOTE_GROUP })
            ),
            json!([native("openRemoteWorkspaceProjectInIde", REMOTE_PROJECT)])
        );
    }

    #[test]
    fn remote_open_in_another_app_warns_with_copy_path() {
        let core = Core::new();
        let copy = native("copyRemoteProjectPath", REMOTE_PROJECT)["payload"].clone();
        assert_eq!(
            plan(
                &core,
                json!({ "type": "openWorkspaceProjectInTargetForGroup", "groupId": REMOTE_GROUP,
                    "targetId": "idea", "targetLabel": "IntelliJ IDEA" })
            ),
            json!([{
                "call": "toast",
                "level": "warning",
                "title": "IntelliJ IDEA can't open a remote project",
                "description": "It only opens folders on this computer. Copy the path to use it on the remote computer.",
                "button": { "label": "Copy Path", "payload": copy },
            }])
        );
        let custom = plan_remote_project_open(
            &ProjectKey::remote("win", "P9"),
            RemoteProjectOpen::Target {
                id: "custom:iterm",
                label: None,
            },
        )
        .to_json();
        assert_eq!(
            custom[0]["title"],
            json!("This app can't open a remote project")
        );
    }
}
