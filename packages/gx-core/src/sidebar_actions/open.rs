//! The payloads whose whole answer is "open a thing": the More menu's rows, a machine's Configure,
//! the Space editor, and a project header's Add Worktree and History.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! These nine are the ones the sidebar answers ITSELF rather than by posting a runtime message, so
//! they lived in the sidebar page and died with it. What they all turn out to be, once
//! `openQuickAccess` is followed to its end, is one app-modal-host message: that helper is a
//! translation table over `openAppModal`, and `openAppModal` is `postAppModalHostMessage`. So the
//! whole family is a CLOSE of whatever modal is open followed by an OPEN with a built payload, and
//! the payload is the thing that has to match, not the sentence that describes it. `loadSessions`
//! is the one that is not a modal at all: it starts this computer's gxserver client and nothing
//! else, which is why it has its own effect rather than an empty plan, and why it has no close:
//! the CONTROLLER answers it an arm before `runNativeSidebarAction`, whose own `loadSessions` case
//! is dead code sitting after that function's close.
//!
//! **The close is not decoration.** `runNativeSidebarAction` calls
//! `closeAppModal('SettingsDismissal:sidebarNavigation')` before its switch, and it does so for the
//! navigation rows only: `showHidden`, `toggleProjects` and the two sort rows return BEFORE it.
//! Getting that backwards leaves a Settings window standing behind the one about to appear, or
//! closes a modal a toggle had no business closing, and neither is visible in a comparison of the
//! drawn list.
//!
//! Not answered here, each by its own owner: `projectAction: agent` (the agent launcher's run,
//! `agent_run.rs`), every `machineAction` but Configure (Hide Machine's settings patch,
//! `machine_disable.rs`), and the two toggles, which are not opens at all and are answered in
//! `sidebar_ui/`. The host asks those two files first. The two sort rows are not opens either; they are answered here with
//! the empty plan that `sort.rs` explains, because their TypeScript path ends in a no-op.
//!
//! Ported from the deleted sidebar page's `runNativeSidebarAction`, `runNativeProjectAction`,
//! `editNativeSidebarSpace` (all three frozen in the since-deleted
//! `tooling/gx-core/sidebar-page-frozen/`) and its `controller.ts` (the `machineAction` arm).
//!
//! SEE-ALSO: packages/core-ui/app-modal-host-bridge.ts (`openQuickAccess`, the translation this reproduces),
//! apps/desktop/src/app/gx_store/sidebar_open.rs.

use serde_json::{json, Map, Value};

use crate::keys::ProjectKey;
use crate::sidebar_view::{SidebarView, LOCAL_MACHINE_ID};

use super::plan::{ActionEffect, SidebarActionPlan};
use super::resolve::text_field;
use super::sort::plan_sort_action;

/// Every renderer command type this file answers. `sidebarAction` is here even though four of its
/// actions are not opens, because the host has to ask this file before it decides.
pub const OPEN_COMMAND_TYPES: [&str; 4] = [
    "sidebarAction",
    "machineAction",
    "editSpace",
    "projectAction",
];

/// Whether this command is one this file may answer, without resolving anything.
///
/// It is the TOP-LEVEL command, not a `{ type: 'command', message }` payload: every one of these is
/// a RENDERER command the controller answers itself (controller.ts, the arms after `post`), which
/// is the distinction that made the whole dialog port dead in piece 3d.
pub fn owns_open_command(command: &Value) -> bool {
    text_field(command, "type").is_some_and(|kind| OPEN_COMMAND_TYPES.contains(&kind))
}

/// What one of these commands does, or `None` when it is not ported and must reach the old
/// runtime untouched.
pub fn plan_open_action(view: &SidebarView, command: &Value) -> Option<SidebarActionPlan> {
    match text_field(command, "type")? {
        "sidebarAction" => plan_sidebar_action(view, text_field(command, "action")?),
        "machineAction" => plan_machine_action(view, command),
        "editSpace" => Some(plan_edit_space(view, text_field(command, "spaceId"))),
        "projectAction" => plan_project_action(view, command),
        _ => None,
    }
}

/// The More menu and the empty state's rows.
fn plan_sidebar_action(view: &SidebarView, action: &str) -> Option<SidebarActionPlan> {
    // `loadSessions` NEVER reaches `runNativeSidebarAction`: the controller intercepts it one arm
    // earlier and answers it with `runtime.startLocalGxserver()` alone, which makes that function's
    // own `case 'loadSessions': return` dead code. So there is NO close here. The first cut of this
    // port read the dead case, saw it sitting after `closeAppModal`, and planned a close that the
    // app never performs; the gate caught it, because the two sides post a different number of
    // messages to the app modal host and nothing about the drawn list would have moved.
    if action == "loadSessions" {
        return Some(SidebarActionPlan::one(ActionEffect::StartLocalGxserver));
    }
    // Not opens, and each answered by its own owner. They return BEFORE the close, so a port that
    // answered them here would also close a modal the TypeScript leaves open. The sort rows are
    // `sort.rs`'s, and their answer is the empty plan.
    if let Some(plan) = plan_sort_action(action) {
        return Some(plan);
    }
    if matches!(action, "showHidden" | "toggleProjects") {
        return None;
    }
    let machine_id = view.selected_machine_id.as_str();
    let open = match action {
        "newTag" => json!({
            "type": "open",
            "modal": "settings",
            "initialSection": "sidebarTags",
            "initialSidebarTagsAction": "createTag",
        }),
        "accounts" => json!({ "type": "open", "modal": "settings", "initialTab": "accounts" }),
        "addProject" => {
            let mut open = json!({ "type": "open", "modal": "addProject" });
            if machine_id != LOCAL_MACHINE_ID {
                open["machineId"] = Value::String(machine_id.to_string());
            }
            open
        }
        "editMachine" => json!({
            "type": "open",
            "modal": "settings",
            "initialTab": "remote",
            "initialRemoteMachineId": machine_id,
        }),
        // `openQuickAccess('recentSessions', { sessionScope: 'all' })`, followed to its end: the
        // helper opens `previousSessions` with the two initial fields, and `initialProjectId` is
        // `undefined` here, which `postAppModalHostMessage` drops on the way through `JSON`.
        "sessions" => quick_access_sessions(None, "all"),
        "importSessions" => quick_access_sessions(None, "external"),
        // `openQuickAccess('commands')` falls through the helper's ifs to its last line.
        "commands" => json!({ "type": "open", "modal": "commandPalette", "initialQuery": "" }),
        "powerSettings" => json!({
            "type": "open",
            "modal": "settings",
            "initialSearchQuery": "Keep awake",
        }),
        // The `default:` arm, which opens a modal named by the action itself. Reproduced as the
        // named set rather than as "whatever arrived", because the TypeScript's `action` is a union
        // and a string this store invented would open a modal that does not exist.
        "agentsHub" | "remoteSetup" | "hotkeys" | "settings" => {
            json!({ "type": "open", "modal": action })
        }
        _ => return None,
    };
    Some(close_then_open(open))
}

/// `openQuickAccess('recentSessions', …)`, with the helper's own payload.
fn quick_access_sessions(project_id: Option<&str>, scope: &str) -> Value {
    let mut open = Map::new();
    open.insert("type".to_string(), Value::String("open".to_string()));
    open.insert(
        "modal".to_string(),
        Value::String("previousSessions".to_string()),
    );
    // `initialProjectId: undefined` is an ABSENT key by the time the host sees it, and the host
    // tells the two apart, so an explicit null here would be a different message.
    if let Some(project_id) = project_id {
        open.insert(
            "initialProjectId".to_string(),
            Value::String(project_id.to_string()),
        );
    }
    open.insert(
        "initialSessionScope".to_string(),
        Value::String(scope.to_string()),
    );
    Value::Object(open)
}

/// A machine tab's own menu. Only Configure is answered here; every other action is Hide
/// Machine's settings patch, `machine_disable.rs`'s, which the host asks first.
fn plan_machine_action(view: &SidebarView, command: &Value) -> Option<SidebarActionPlan> {
    match text_field(command, "action")? {
        // No close: this arm is not inside `runNativeSidebarAction` and has none.
        "configure" => Some(SidebarActionPlan::one(ActionEffect::OpenAppModal {
            payload: json!({ "type": "open", "modal": "settings", "initialTab": "remote" }),
        })),
        _ => {
            let _ = view;
            None
        }
    }
}

/// The Space editor, opened to create or to edit depending on whether the id names a Space the
/// selected machine has. No close: this arm has none either.
fn plan_edit_space(view: &SidebarView, space_id: Option<&str>) -> SidebarActionPlan {
    let space = space_id.and_then(|space_id| {
        view.spaces
            .iter()
            .find(|candidate| candidate.id == space_id)
    });
    let machine_id = view.selected_machine_id.as_str();
    let mut open = Map::new();
    open.insert("type".to_string(), Value::String("open".to_string()));
    open.insert(
        "modal".to_string(),
        Value::String("sidebarSpaceEditor".to_string()),
    );
    open.insert(
        "mode".to_string(),
        Value::String(
            match space.is_some() {
                true => "edit",
                false => "create",
            }
            .to_string(),
        ),
    );
    // `ui.sectionKey`, which is the string the Space editor writes its selection back under.
    open.insert(
        "sectionKey".to_string(),
        Value::String(match machine_id == LOCAL_MACHINE_ID {
            true => LOCAL_MACHINE_ID.to_string(),
            false => format!("remote:{machine_id}"),
        }),
    );
    if machine_id != LOCAL_MACHINE_ID {
        open.insert(
            "remoteMachineId".to_string(),
            Value::String(machine_id.to_string()),
        );
    }
    if let Some(space) = space {
        open.insert("spaceId".to_string(), Value::String(space.id.clone()));
        open.insert("spaceName".to_string(), Value::String(space.name.clone()));
        open.insert("spaceIcon".to_string(), Value::String(space.icon.clone()));
        open.insert("spaceColor".to_string(), Value::String(space.color.clone()));
    }
    SidebarActionPlan::one(ActionEffect::OpenAppModal {
        payload: Value::Object(open),
    })
}

/// Add Worktree and History on a project header. Both read the DRAWN group, which is the same
/// early return the TypeScript has: `if (!group || !project) return`, an answer of nothing rather
/// than a hand-off, because a row that is not drawn cannot have been clicked.
fn plan_project_action(view: &SidebarView, command: &Value) -> Option<SidebarActionPlan> {
    let action = text_field(command, "action")?;
    // The agent launcher's run is `agent_run.rs`'s, which the host asks first.
    if action == "agent" {
        return None;
    }
    let group_id = text_field(command, "groupId")?;
    let Some(group) = view.group(group_id) else {
        return Some(SidebarActionPlan::nothing());
    };
    let Some(project) = group.core.project_context.as_ref() else {
        return Some(SidebarActionPlan::nothing());
    };
    // `projectContext.editor.projectId` is the WORKSPACE project id, which is the raw id on this
    // computer and `remote:<machine>:project:<id>` on another. `ProjectContextView::project_id` is
    // the RAW one on both, so it is the group id that carries the right string, and taking the
    // easy field would send a remote worktree dialog a project id no workspace holds.
    let workspace_project_id =
        ProjectKey::parse_sidebar_group_id(group_id)?.to_workspace_project_id();
    match action {
        "worktree" => {
            let mut open = Map::new();
            open.insert("type".to_string(), Value::String("open".to_string()));
            open.insert("modal".to_string(), Value::String("worktree".to_string()));
            // `projectContext.editor.projectId` is the WORKSPACE project id, which is the raw id
            // on this computer and `remote:<machine>:project:<id>` on another, and that is exactly
            // what the group id encodes.
            open.insert(
                "projectId".to_string(),
                Value::String(workspace_project_id.clone()),
            );
            open.insert(
                "projectName".to_string(),
                Value::String(group.core.title.clone()),
            );
            open.insert(
                "projectPath".to_string(),
                Value::String(project.path.clone()),
            );
            // Both ride only when the group has a machine, because `undefined` is an absent key.
            if let Some(machine) = group.core.remote_machine.as_ref() {
                open.insert(
                    "remoteMachineId".to_string(),
                    Value::String(machine.machine_id.clone()),
                );
                open.insert(
                    "remoteMachineName".to_string(),
                    Value::String(machine.machine_name.clone()),
                );
            }
            Some(SidebarActionPlan::one(ActionEffect::OpenAppModal {
                payload: Value::Object(open),
            }))
        }
        // `getQuickAccessSessionProjectId`: the machine-scoped id for a remote project and the
        // editor's own id otherwise, which are the same string here (the workspace project id),
        // except for a remote group whose machine context carries no project id. That is the
        // machine's Chats group, which has no `projectContext` and returned above.
        "history" => Some(SidebarActionPlan::one(ActionEffect::OpenAppModal {
            payload: quick_access_sessions(Some(workspace_project_id.as_str()), "closed"),
        })),
        _ => None,
    }
}

/// The two effects every navigation row runs, in the TypeScript's order.
fn close_then_open(open: Value) -> SidebarActionPlan {
    SidebarActionPlan {
        effects: vec![
            ActionEffect::CloseAppModal,
            ActionEffect::OpenAppModal { payload: open },
        ],
    }
}
