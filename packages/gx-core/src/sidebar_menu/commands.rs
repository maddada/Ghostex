//! Every payload a sidebar menu row can carry, in one place.
//!
//! CDXC:ContextMenus 2026-09-20 WHY:
//! A menu row's action is a `NativeSidebarCommand` the host hands back to the runtime that owns
//! the side effect (a gxserver call, an app modal, or an intent this store already answers). The
//! wire shape is what makes the Rust menus interchangeable with the TypeScript ones row by row, so
//! every shape is written here and nowhere else: that is both the inventory of what the menus can
//! do and the one file to change when a payload moves into the store.
//!
//! SEE-ALSO: packages/shared/native-sidebar.ts (deleted 2026-10-01) (`NativeSidebarCommand`),
//! the deleted sidebar page's `controller.ts` (the one dispatcher), and
//! apps/desktop/src/app/native_sidebar/actions.rs.

use serde_json::{json, Map, Value};

/// A menu row's action, as the renderer and the runtime exchange it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuCommand(Value);

impl MenuCommand {
    pub fn to_json(&self) -> Value {
        self.0.clone()
    }

    pub fn as_json(&self) -> &Value {
        &self.0
    }

    /// A top-level command the host answers itself (the workspace tile's menu,
    /// `sidebar_menu/workspace.rs`).
    pub(crate) fn host(command: Value) -> Self {
        Self(command)
    }

    /// `{ type: 'command', message }`: forwarded to the runtime's gxserver client untouched.
    pub(crate) fn command(message: Value) -> Self {
        Self(json!({ "type": "command", "message": message }))
    }

    /// `{ type: 'batch', messages }`, optionally clearing the multi-selection first.
    pub(crate) fn batch(messages: Vec<Value>, clear_selection: bool) -> Self {
        let mut object = Map::new();
        object.insert("type".to_string(), Value::String("batch".to_string()));
        if clear_selection {
            object.insert("clearSelection".to_string(), Value::Bool(true));
        }
        object.insert("messages".to_string(), Value::Array(messages));
        Self(Value::Object(object))
    }

    /// `{ type: 'sessionAction', sessionId, action }` and its two extra fields.
    pub(crate) fn session_action(session_id: &str, action: &str) -> Self {
        Self(json!({ "type": "sessionAction", "sessionId": session_id, "action": action }))
    }

    pub(crate) fn snooze(session_id: &str, preset: &str) -> Self {
        Self(json!({
            "type": "sessionAction",
            "sessionId": session_id,
            "action": "snooze",
            "preset": preset,
        }))
    }

    /// The snooze command with the tag the submenu row would set (`null` clears it).
    pub(crate) fn snooze_with_tag(session_id: &str, preset: &str, tag: Option<&str>) -> Self {
        let mut object = Self::snooze(session_id, preset).0;
        object["sessionTag"] = tag.map_or(Value::Null, |tag| Value::String(tag.to_string()));
        Self(object)
    }

    /// `{ type: 'sessionMenu', sessionId, action?, ownerId }`: the lazy request that asks for the
    /// full menu of the row the user actually opened.
    pub(crate) fn session_menu(session_id: &str, action: Option<&str>, owner_id: &str) -> Self {
        let mut object = Map::new();
        object.insert("type".to_string(), Value::String("sessionMenu".to_string()));
        object.insert(
            "sessionId".to_string(),
            Value::String(session_id.to_string()),
        );
        if let Some(action) = action {
            object.insert("action".to_string(), Value::String(action.to_string()));
        }
        object.insert("ownerId".to_string(), Value::String(owner_id.to_string()));
        Self(Value::Object(object))
    }

    pub(crate) fn session_accounts_load(session_id: &str) -> Self {
        Self(json!({ "type": "sessionAccounts", "action": "load", "sessionId": session_id }))
    }

    /// `{ type: 'sessionAccounts', sessionId, action, accountId? }`: the account flyout's own
    /// rows (a pick, Try Again).
    pub(crate) fn session_accounts(
        session_id: &str,
        action: &str,
        account_id: Option<&str>,
    ) -> Self {
        let mut object = Map::new();
        object.insert(
            "type".to_string(),
            Value::String("sessionAccounts".to_string()),
        );
        object.insert(
            "sessionId".to_string(),
            Value::String(session_id.to_string()),
        );
        object.insert("action".to_string(), Value::String(action.to_string()));
        if let Some(account_id) = account_id {
            object.insert(
                "accountId".to_string(),
                Value::String(account_id.to_string()),
            );
        }
        Self(Value::Object(object))
    }

    /// `{ type: 'projectAction', action: 'agent', groupId, agentId, accountId? }`: a launcher
    /// account row, which launches the agent signed in as that account.
    pub(crate) fn agent_run_as(group_id: &str, agent_id: &str, account_id: Option<&str>) -> Self {
        let mut command = Self::project_action(group_id, "agent", Some(agent_id));
        if let (Some(account_id), Value::Object(object)) = (account_id, &mut command.0) {
            object.insert(
                "accountId".to_string(),
                Value::String(account_id.to_string()),
            );
        }
        command
    }

    pub(crate) fn sidebar_action(action: &str) -> Self {
        Self(json!({ "type": "sidebarAction", "action": action }))
    }

    pub(crate) fn toggle_tag_filter(tag: &str) -> Self {
        Self(json!({ "type": "toggleTagFilter", "tag": tag }))
    }

    pub(crate) fn rename_group(group_id: &str) -> Self {
        Self(json!({ "type": "renameGroup", "groupId": group_id }))
    }

    pub(crate) fn confirm_close_group(group_id: &str) -> Self {
        Self(json!({ "type": "confirmCloseGroup", "groupId": group_id }))
    }

    pub(crate) fn rename_collection(collection_id: &str) -> Self {
        Self(json!({ "type": "renameCollection", "collectionId": collection_id }))
    }

    /// `{ type: 'collectionAction', collectionId, action, value? }`.
    pub(crate) fn collection_action(
        collection_id: &str,
        action: &str,
        value: Option<&str>,
    ) -> Self {
        let mut object = Map::new();
        object.insert(
            "type".to_string(),
            Value::String("collectionAction".to_string()),
        );
        object.insert(
            "collectionId".to_string(),
            Value::String(collection_id.to_string()),
        );
        object.insert("action".to_string(), Value::String(action.to_string()));
        if let Some(value) = value {
            object.insert("value".to_string(), Value::String(value.to_string()));
        }
        Self(Value::Object(object))
    }

    /// `{ type: 'projectMembership', groupId, action, collectionId? }`.
    pub(crate) fn project_membership(
        group_id: &str,
        action: &str,
        collection_id: Option<&str>,
    ) -> Self {
        let mut object = Map::new();
        object.insert(
            "type".to_string(),
            Value::String("projectMembership".to_string()),
        );
        object.insert("action".to_string(), Value::String(action.to_string()));
        object.insert("groupId".to_string(), Value::String(group_id.to_string()));
        if let Some(collection_id) = collection_id {
            object.insert(
                "collectionId".to_string(),
                Value::String(collection_id.to_string()),
            );
        }
        Self(Value::Object(object))
    }

    /// `{ type: 'spaceMembership', spaceId?, collectionId?, projectId? }`. No `spaceId` means
    /// "make a new Space with this member in it".
    pub(crate) fn space_membership(
        space_id: Option<&str>,
        collection_id: Option<&str>,
        project_id: Option<&str>,
    ) -> Self {
        let mut object = Map::new();
        object.insert(
            "type".to_string(),
            Value::String("spaceMembership".to_string()),
        );
        if let Some(space_id) = space_id {
            object.insert("spaceId".to_string(), Value::String(space_id.to_string()));
        }
        if let Some(collection_id) = collection_id {
            object.insert(
                "collectionId".to_string(),
                Value::String(collection_id.to_string()),
            );
        }
        if let Some(project_id) = project_id {
            object.insert(
                "projectId".to_string(),
                Value::String(project_id.to_string()),
            );
        }
        Self(Value::Object(object))
    }

    /// `{ type: 'projectAction', groupId, action, agentId? }`.
    pub(crate) fn project_action(group_id: &str, action: &str, agent_id: Option<&str>) -> Self {
        let mut object = Map::new();
        object.insert(
            "type".to_string(),
            Value::String("projectAction".to_string()),
        );
        object.insert("action".to_string(), Value::String(action.to_string()));
        object.insert("groupId".to_string(), Value::String(group_id.to_string()));
        if let Some(agent_id) = agent_id {
            object.insert("agentId".to_string(), Value::String(agent_id.to_string()));
        }
        Self(Value::Object(object))
    }

    /// `{ type: 'agentAccounts', groupId, action, agentId? }`.
    pub(crate) fn agent_accounts(group_id: &str, action: &str, agent_id: Option<&str>) -> Self {
        let mut object = Map::new();
        object.insert(
            "type".to_string(),
            Value::String("agentAccounts".to_string()),
        );
        object.insert("groupId".to_string(), Value::String(group_id.to_string()));
        object.insert("action".to_string(), Value::String(action.to_string()));
        if let Some(agent_id) = agent_id {
            object.insert("agentId".to_string(), Value::String(agent_id.to_string()));
        }
        Self(Value::Object(object))
    }

    /// `{ type: 'agentAccounts', groupId, action, runLocation? }`: the launcher's Run in a Box
    /// pages (`box` lists the ready locations, `boxAgents` the agents one location can run).
    pub(crate) fn agent_box_page(group_id: &str, action: &str, run_location: Option<&str>) -> Self {
        let mut command = Self::agent_accounts(group_id, action, None);
        if let (Some(run_location), Value::Object(object)) = (run_location, &mut command.0) {
            object.insert(
                "runLocation".to_string(),
                Value::String(run_location.to_string()),
            );
        }
        command
    }

    /// `{ type: 'projectAction', action: 'agent', groupId, agentId, runLocation }`: a Run in a Box
    /// row, which launches the agent in that box (no account: the box has its own sign-in).
    pub(crate) fn agent_run_in_box(group_id: &str, agent_id: &str, run_location: &str) -> Self {
        let mut command = Self::project_action(group_id, "agent", Some(agent_id));
        if let Value::Object(object) = &mut command.0 {
            object.insert(
                "runLocation".to_string(),
                Value::String(run_location.to_string()),
            );
        }
        command
    }

    /// `{ type: 'agentboxSessionAction', sessionId, action }`: a box session's Open Box Web App
    /// (`openWeb`), Open Box Screen (`openScreen`), Stop Box (`stop`) and Destroy Box (`destroy`).
    pub(crate) fn agentbox_session(session_id: &str, action: &str) -> Self {
        Self(json!({ "type": "agentboxSessionAction", "sessionId": session_id, "action": action }))
    }

    /// `{ type: 'claudeCloudSessionAction', sessionId, action }`: a Claude cloud session's Open on
    /// claude.ai (`open`), Bring to This Computer (`bringBack`) and Start a Copy on Another
    /// Account (`copy`).
    pub(crate) fn claude_cloud_session(session_id: &str, action: &str) -> Self {
        Self(
            json!({ "type": "claudeCloudSessionAction", "sessionId": session_id, "action": action }),
        )
    }

    /// `{ type: 'coordinatorSessionAction', sessionId, action }`: an orchestrator's Goal &
    /// Instructions (`goal`) and Memory (`memory`) editors, a thread's Mark Thread Done
    /// (`markDone`) and Reopen Thread (`reopen`).
    pub(crate) fn coordinator_session(session_id: &str, action: &str) -> Self {
        Self(
            json!({ "type": "coordinatorSessionAction", "sessionId": session_id, "action": action }),
        )
    }

    /// `{ type: 'sessionAction', sessionId, action: 'linkWork', kind }`: opens the Link to picker
    /// for one kind (`pullRequest`, `linearIssue`, `linearProject`, `githubIssue`).
    pub(crate) fn link_work(session_id: &str, kind: &str) -> Self {
        Self(json!({
            "type": "sessionAction",
            "sessionId": session_id,
            "action": "linkWork",
            "kind": kind,
        }))
    }
}

/// The gxserver-bound messages a `command` row carries. One function per message so the set the
/// menus can send is greppable.
pub(crate) mod message {
    use serde_json::{json, Value};

    pub(crate) fn set_session_tag(session_id: &str, tag: Option<&str>) -> Value {
        json!({
            "type": "setSessionTag",
            "sessionId": session_id,
            "sessionTag": tag.map_or(Value::Null, |tag| Value::String(tag.to_string())),
        })
    }

    pub(crate) fn set_session_parked(session_id: &str, parked: bool) -> Value {
        json!({ "type": "setSessionParked", "sessionId": session_id, "parked": parked })
    }

    pub(crate) fn set_session_sleeping(session_id: &str, sleeping: bool) -> Value {
        json!({ "type": "setSessionSleeping", "sessionId": session_id, "sleeping": sleeping })
    }

    pub(crate) fn set_session_pinned(session_id: &str, pinned: bool) -> Value {
        json!({ "type": "setSessionPinned", "sessionId": session_id, "pinned": pinned })
    }

    /// A row menu's Close. `closeSource` names the door for gxserver's close log (the row's ×
    /// button, which runs the same item, renames it to `sidebarRowButton`).
    pub(crate) fn close_session(session_id: &str) -> Value {
        json!({ "type": "closeSession", "sessionId": session_id, "closeSource": "sidebarMenu" })
    }

    /// A multi-selection's Close Selected, one per selected row.
    pub(crate) fn close_selected_session(session_id: &str) -> Value {
        json!({ "type": "closeSession", "sessionId": session_id, "closeSource": "multiSelect" })
    }

    /// Several rows closed by one menu item; `door` is that item (`closeBelow`, …).
    pub(crate) fn close_sessions(session_ids: &[String], door: &str) -> Value {
        json!({ "type": "closeSessions", "sessionIds": session_ids, "closeSource": door })
    }

    pub(crate) fn sleep_sessions_below(session_ids: &[String]) -> Value {
        json!({
            "type": "setSessionsSleeping",
            "sessionIds": session_ids,
            "sleeping": true,
            "source": "sleepBelow",
        })
    }

    pub(crate) fn sleep_sessions(session_ids: &[String]) -> Value {
        json!({ "type": "setSessionsSleeping", "sessionIds": session_ids, "sleeping": true })
    }

    pub(crate) fn unsnooze_session(session_id: &str) -> Value {
        json!({ "type": "unsnoozeSession", "sessionId": session_id })
    }

    pub(crate) fn toggle_close_after_done(session_id: &str) -> Value {
        json!({ "type": "toggleCloseAfterDone", "sessionId": session_id })
    }

    pub(crate) fn fork_session(session_id: &str) -> Value {
        json!({ "type": "forkSession", "sessionId": session_id })
    }

    pub(crate) fn full_reload_session(session_id: &str) -> Value {
        json!({ "type": "fullReloadSession", "sessionId": session_id })
    }

    pub(crate) fn export_session_transcript(session_id: &str) -> Value {
        json!({ "type": "exportSessionTranscript", "sessionId": session_id })
    }

    pub(crate) fn generate_session_title(session_id: &str, title: &str) -> Value {
        json!({
            "type": "renameSession",
            "sessionId": session_id,
            "title": title,
            "shouldGenerateTitle": true,
        })
    }

    pub(crate) fn create_group_from_session(session_id: &str) -> Value {
        json!({ "type": "createGroupFromSession", "sessionId": session_id })
    }

    /// The drag's own `moveSessionToGroup` (sidebar_drag/order_write.rs): a user-made group's id
    /// moves the row into it, the project's own group id takes it out of every user-made group.
    pub(crate) fn move_session_to_group(session_id: &str, group_id: &str) -> Value {
        json!({ "type": "moveSessionToGroup", "sessionId": session_id, "groupId": group_id })
    }

    pub(crate) fn focus_session_mode(session_id: &str) -> Value {
        json!({ "type": "focusSessionMode", "sessionId": session_id })
    }

    pub(crate) fn copy_session_details(session_id: &str, details_text: &str) -> Value {
        json!({
            "type": "copySessionDetails",
            "sessionId": session_id,
            "detailsText": details_text,
        })
    }

    /// Any text a Copy row offers (a branch, a Linear ID, a link), straight to the clipboard.
    pub(crate) fn copy_text(text: &str) -> Value {
        json!({ "type": "copyText", "text": text })
    }

    pub(crate) fn postpone_delayed_send(session_id: &str, delay_ms: i64) -> Value {
        json!({ "type": "postponeDelayedSend", "sessionId": session_id, "delayMs": delay_ms })
    }

    pub(crate) fn cancel_delayed_send(session_id: &str) -> Value {
        json!({ "type": "cancelDelayedSend", "sessionId": session_id })
    }

    pub(crate) fn full_reload_group(group_id: &str) -> Value {
        json!({ "type": "fullReloadGroup", "groupId": group_id })
    }

    pub(crate) fn set_group_sleeping(group_id: &str, sleeping: bool) -> Value {
        json!({ "type": "setGroupSleeping", "groupId": group_id, "sleeping": sleeping })
    }

    pub(crate) fn close_group(group_id: &str) -> Value {
        json!({ "type": "closeGroup", "groupId": group_id })
    }

    pub(crate) fn copy_project_path(group_id: &str) -> Value {
        json!({ "type": "copyWorkspaceProjectPathForGroup", "groupId": group_id })
    }

    pub(crate) fn open_project_in_finder(group_id: &str) -> Value {
        json!({ "type": "openWorkspaceProjectInFinderForGroup", "groupId": group_id })
    }

    /// `target_label` names the app in the warning a remote project gets when the target cannot
    /// open a folder on another computer.
    pub(crate) fn open_project_in_target(
        group_id: &str,
        target_id: &str,
        target_label: &str,
    ) -> Value {
        json!({
            "type": "openWorkspaceProjectInTargetForGroup",
            "groupId": group_id,
            "targetId": target_id,
            "targetLabel": target_label,
        })
    }

    pub(crate) fn prompt_rename_worktree(group_id: &str) -> Value {
        json!({ "type": "promptRenameWorktreeForGroup", "groupId": group_id })
    }

    pub(crate) fn prompt_delete_worktree(group_id: &str) -> Value {
        json!({ "type": "promptDeleteWorktreeForGroup", "groupId": group_id })
    }

    pub(crate) fn remove_worktree_project(group_id: &str) -> Value {
        json!({ "type": "removeWorkspaceProjectForGroup", "groupId": group_id })
    }

    pub(crate) fn copy_project_remote_url(remote_url: &str) -> Value {
        json!({ "type": "copyWorkspaceProjectRemoteUrl", "remoteUrl": remote_url })
    }

    /// Answered by `/api/setSessionWorkLinks` (server/src/work_mode/links.rs): `links` is merged
    /// into the session's hand-set links (`{ clear: true }` puts every kind back to automatic).
    pub(crate) fn set_session_work_links(session_id: &str, links: Value) -> Value {
        json!({ "type": "setSessionWorkLinks", "sessionId": session_id, "links": links })
    }

    pub(crate) fn wake_project_sleeping_sessions(group_id: &str) -> Value {
        json!({ "type": "wakeProjectSleepingSessions", "groupId": group_id })
    }

    pub(crate) fn sleep_inactive_project_sessions(group_id: &str) -> Value {
        json!({ "type": "sleepInactiveProjectSessions", "groupId": group_id })
    }

    /// Answered by `plan_bulk_request` (sidebar_actions/bulk.rs), which resolves the threads.
    pub(crate) fn sleep_inactive_coordinator_threads(session_id: &str) -> Value {
        json!({ "type": "sleepInactiveCoordinatorThreads", "sessionId": session_id })
    }

    pub(crate) fn full_reload_project_sessions(group_id: &str) -> Value {
        json!({ "type": "fullReloadProjectZmxSessions", "groupId": group_id })
    }

    /// Advanced > Autopilot: puts the project (and its worktrees) in Autopilot or takes it out
    /// (`/api/autopilot` `setProject`, server/src/server/autopilot_http.rs).
    pub(crate) fn set_project_autopilot(project_id: &str, on: bool) -> Value {
        json!({ "type": "setProjectAutopilot", "projectId": project_id, "on": on })
    }

    pub(crate) fn close_inactive_project_sessions(group_id: &str) -> Value {
        json!({ "type": "closeInactiveProjectSessions", "groupId": group_id })
    }

    pub(crate) fn close_project(group_id: &str) -> Value {
        json!({ "type": "closeWorkspaceProjectForGroup", "groupId": group_id })
    }

    pub(crate) fn create_session_in_group(group_id: &str) -> Value {
        json!({ "type": "createSessionInGroup", "groupId": group_id })
    }

    pub(crate) fn run_sidebar_git_action(group_id: &str, action: &str) -> Value {
        json!({ "type": "runSidebarGitAction", "groupId": group_id, "action": action })
    }

    pub(crate) fn open_browser_pane_in_group(group_id: &str) -> Value {
        json!({ "type": "openBrowserPaneInGroup", "groupId": group_id })
    }

    pub(crate) fn create_project_terminal(group_id: &str) -> Value {
        json!({ "type": "createProjectTerminal", "groupId": group_id })
    }

    /// A bot's Edit SOUL or Edit config: `filePath` opens in the Code view of the bot's own
    /// project, `projectPath` (gx_store/create/bot.rs).
    pub(crate) fn open_bot_file(group_id: &str, project_path: &str, file_path: &str) -> Value {
        json!({
            "type": "openBotFile",
            "groupId": group_id,
            "projectPath": project_path,
            "filePath": file_path,
        })
    }

    pub(crate) fn run_sidebar_command(command_id: &str, scope: &str, group_id: &str) -> Value {
        json!({
            "type": "runSidebarCommand",
            "commandId": command_id,
            "scope": scope,
            "groupId": group_id,
        })
    }

    pub(crate) fn search_previous_sessions_by_text() -> Value {
        json!({ "type": "searchPreviousSessionsByText" })
    }

    pub(crate) fn open_automations_page() -> Value {
        json!({ "type": "openAutomationsPage" })
    }

    pub(crate) fn start_keep_awake(duration_minutes: i64) -> Value {
        json!({
            "type": "runTitlebarKeepAwakeCommand",
            "action": "start",
            "durationMinutes": duration_minutes,
        })
    }

    pub(crate) fn stop_keep_awake() -> Value {
        json!({ "type": "runTitlebarKeepAwakeCommand", "action": "stop" })
    }

    pub(crate) fn open_external_url(url: &str) -> Value {
        json!({ "type": "openExternalUrl", "url": url })
    }

    /// `action` is one of [`crate::APP_LIFECYCLE_ACTIONS`]'s ids.
    pub(crate) fn app_lifecycle(action: &str) -> Value {
        json!({ "type": crate::APP_LIFECYCLE_MESSAGE_TYPE, "action": action })
    }
}
