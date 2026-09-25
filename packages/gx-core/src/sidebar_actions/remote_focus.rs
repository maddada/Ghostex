//! Clicking a row that lives on ANOTHER machine: which session, which pane, the one native action
//! that opens it, and the attention acknowledgement that goes with it.
//!
//! CDXC:RemoteMachines 2026-09-21 WHY:
//! The old runtime's remote branch of `focusSession` is four steps and the ORDER of them is the
//! whole contract: acknowledge the attention, work out `keepView` from the group that is active
//! right now, read the agent's Default Agent View off THAT machine's presentation, and post
//! `openRemoteSessionTerminal` through the fixed native project-path bridge; then it moves its
//! remote focus marks (`setRemotePresentationSessionFocus`) and publishes them. The post and the
//! acknowledgement's timers are the two things this crate cannot do, so the plan ends at two
//! payloads the host performs in that order: the acknowledgement for the old runtime's attention
//! subsystem (which keeps the minimum visible window and its one timer implementation), and the
//! open. The marks are not a third payload: the open ends in the tab-selected callback, whose
//! remote branch is exactly `setRemotePresentationSessionFocus` plus the publish
//! ([`RemoteFocusPlan::tab_selection`] is what it carries), so the command no longer goes to the
//! old runtime at all.
//!
//! **What the payload actually reaches.** The bridge arm for this action is
//! `handle_gpui_remote_session_native_action`, which resolves the machine's SSH configuration and
//! its tunnel target and ends in `begin_gpui_remote_attach_terminal_open`. That is the function the
//! old path ends in, and it is the function the host calls, through the same
//! `receive_sidebar_native_project_path_action_payload` entry the bridge message lands on, so the
//! remote revalidation contract stays one implementation.
//!
//! **`keepView` is read from the sidebar group id, not from the typed active group.** The
//! TypeScript asks `parseGpuiRemotePresentationGroupId(activeGroupId)` and compares the machine and
//! the project it finds, so a Space's subgroup id, the machine's Chats group and a local group all
//! answer "this focus changes the project". Reproducing that on the string keeps the two sides
//! reading one rule instead of two.
//!
//! **And the group id is the OLD RUNTIME's, handed in, never the core's focus.**
//! CDXC:RemoteMachines 2026-09-21 WHY:
//! Since remote focus part 2 step 2 the core's focus DOES follow a remote focus (the host's tab selection takes it), but it still cannot stand in for the runtime's group, for two reasons. The core names the user-made group a row sits in (`group_of_session`), where `setRemotePresentationSessionFocus` always names the project's own group or the machine's Chats, and the string rule above answers differently for the two. And the runtime's `activeGroupId` still moves on paths the store does not see in the moment (a group attach from navigation history or a Space restore, a lifecycle replacement and its restores); the store learns those from the runtime's publish, which lags the command. Planning from `core.focus()` before step 2 sent a `keepView` the runtime did not on every second click inside the active remote project. So the group is what the runtime holds at the moment of the click, which [`RuntimeActiveGroup`] tracks from what the host SENT the runtime and what the runtime last PUBLISHED; its comment has the proof.
//!
//! Refused, each with its reason: a LOCAL row (the store's own focus path owns it), a browser row
//! (an app tab, not a session), and an id that does not parse as a remote session. A machine whose
//! rows did not come from THIS run's stream (not loaded yet, or drawn from the stored last-seen
//! copy) is answered too, the way the old runtime answered it: see [`RemoteFocusPlan::live`].
//!
//! Ported from the deleted `gxserver-runtime/sessions-and-focus.ts` (`focusSession`'s remote
//! branch, `focusChangesActiveProject`, `sessionPreferredAgentInterface`, `splitSessionRight`).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_remote_focus.rs,
//! apps/desktop/src/app/remote_conn/native_action.rs.

use serde_json::{json, Map, Value};

use crate::core::Core;
use crate::focus::ActiveGroup;
use crate::keys::{MachineId, ProjectKey, SessionKey};
use crate::selectors::is_chat_project_path;

use super::resolve::{
    text_field, NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE, NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
};

/// `ghostex.gpui.sidebar.workspaceSessionAttentionAcknowledge`, the bridge the store already uses to
/// acknowledge a local row (`GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE` in
/// the desktop's consts.rs). A remote row rides it with machine-scoped ids, which the old runtime's
/// `normalizeGpuiWorkspaceRemoteSessionAttentionAcknowledge` accepts only as a matching pair.
pub const SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.workspaceSessionAttentionAcknowledge";
pub const SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION: i64 = 1;

/// The two messages whose remote branch is this file's. `focusSession` is what
/// `selectNativeSidebarSession` posts for a row click, and what a Space restore posts with
/// `keepView`; `splitSessionRight` is the same open with a placement.
pub const REMOTE_FOCUS_MESSAGE_TYPES: [&str; 2] = ["focusSession", "splitSessionRight"];

/// `resolveEffectivePreferredAgentInterface`'s two inputs, as plain data.
///
/// The Default Agent View lives in the shared settings document, which this crate does not read.
/// The host hands over the normalized global value and the per-agent overrides it already parses
/// (`gpui_preferred_agent_interface_from_settings` and its override sibling), so the resolution
/// itself stays here where the gate can drive it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreferredInterfaceSettings {
    /// `"chat"` or `"terminal"`, already normalized.
    pub default_interface: String,
    /// Agent id to `"chat"` or `"terminal"`. Any other value is an absent override.
    pub overrides: Vec<(String, String)>,
}

impl PreferredInterfaceSettings {
    /// `sessionPreferredAgentInterface`: nothing at all for a row with no agent id, because the
    /// desktop drops the intent for such a row anyway and an unset value keeps the extra
    /// attach-metadata preview off the plain-terminal path.
    pub fn resolve(&self, agent_id: Option<&str>) -> Option<&str> {
        let agent_id = agent_id.map(str::trim).filter(|id| !id.is_empty())?;
        let override_value = self
            .overrides
            .iter()
            .find(|(id, _)| id == agent_id)
            .map(|(_, value)| value.as_str())
            .filter(|value| *value == "chat" || *value == "terminal");
        Some(override_value.unwrap_or(self.default_interface.as_str()))
    }
}

/// A remote row's click, resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFocusPlan {
    /// The row, as a key that carries its machine.
    pub session: SessionKey,
    /// Whether the machine's rows came from THIS run's stream.
    ///
    /// CDXC:RemoteMachines 2026-09-25 WHY:
    /// For a machine that is offline and showing its last-seen rows, or connected with no snapshot
    /// yet, the old runtime still posted the open, but WITHOUT `preferredInterface` (it read the
    /// agent from `this.remotePresentations`, which holds only what a stream delivered), and its
    /// attention acknowledgement found no row and did nothing. The planner used to refuse such a
    /// click and hand it back, but no runtime path was left to take it: the click reached nobody.
    /// It is answered here now with that same payload, and the host skips the acknowledgement.
    pub live: bool,
    /// The acknowledgement the host sends the old runtime BEFORE the open, as `focusSession` and
    /// `splitSessionRight` both acknowledge first. Always sent: whether the row is in attention,
    /// and whether the minimum visible window defers the clear, is the runtime's to decide, so the
    /// timer stays one implementation.
    pub attention_acknowledgement: Value,
    /// Whether the destination keeps its own remembered view instead of switching to Agents.
    pub keep_view: bool,
    /// The agent's Default Agent View, absent for a row with no agent.
    pub preferred_interface: Option<String>,
    /// Split Right, which is this same open with a placement.
    pub split_right: bool,
    /// The `openRemoteSessionTerminal` payload, ready for the native project-path entry point.
    pub native_action: Value,
    /// The group the old runtime's `activeGroupId` holds once the open's tab-selected callback has
    /// run: [`remote_focus_group`] of the row. Not part of either payload; the host's callback feeds
    /// it to [`RuntimeActiveGroup::sent_remote_focus`], and the gate checks the two agree.
    pub focus_group: String,
}

impl RemoteFocusPlan {
    pub fn machine_id(&self) -> &str {
        self.session.machine.remote_id().unwrap_or_default()
    }

    /// The ids the open's tab-selected callback carries (`set_sidebar_gxserver_remote_attach_focus_state`
    /// builds them from the attach key with `gpui_remote_scoped_project_id` and
    /// `gpui_remote_scoped_session_id`). Not sent by the host: the open sends it. The deleted parity
    /// gate replayed it through `handleGpuiWorkspaceTabSessionSelected` to prove it moved the marks
    /// the forwarded command used to move.
    pub fn tab_selection(&self) -> Value {
        json!({
            "projectId": self.session.project_key().to_workspace_project_id(),
            "sessionId": self.session.to_focus_state_session_id(),
        })
    }

    /// The plan in the shape the parity gate compares: the payload that opens the pane and the
    /// marks that follow it.
    pub fn to_json(&self) -> Value {
        json!({
            "session": self.session.to_sidebar_session_id(),
            "attentionAcknowledgement": self.attention_acknowledgement,
            "tabSelection": self.tab_selection(),
            "keepView": self.keep_view,
            "preferredInterface": self.preferred_interface,
            "splitRight": self.split_right,
            "nativeAction": self.native_action,
        })
    }
}

/// What a click on a remote row does, or `None` when this file does not own the payload and the old
/// runtime must answer it whole.
///
/// `runtime_active_group` is the sidebar group id the old runtime's `activeGroupId` holds at the
/// moment of the click ([`RuntimeActiveGroup::current`]); `None` is "no group", which, like every
/// local group, answers "the project changes".
pub fn plan_remote_focus(
    core: &Core,
    message: &Value,
    settings: &PreferredInterfaceSettings,
    runtime_active_group: Option<&str>,
) -> Option<RemoteFocusPlan> {
    let kind = text_field(message, "type")?;
    if !REMOTE_FOCUS_MESSAGE_TYPES.contains(&kind) {
        return None;
    }
    let sidebar_session_id = text_field(message, "sessionId")?;
    // A browser row is an app tab and never reaches the remote branch: `focusSession` answers it
    // one arm earlier, before the id is parsed at all.
    if sidebar_session_id.starts_with("gpui-browser:") {
        return None;
    }
    let session = SessionKey::parse_remote_scoped_session_id(sidebar_session_id)?;
    // `sessionPreferredAgentInterface` reads the row out of the machine's LIVE presentation only
    // (see `RemoteFocusPlan::live`), so a machine this run has not streamed opens without it.
    let live = core.presentation().loaded_live(&session.machine).is_some();
    let split_right = kind == "splitSessionRight";
    // Split Right hands `focusLocalWorkspaceSession` a placement and NOTHING else: no `keepView`,
    // and no preferred interface either, so the two fields are the focus click's alone.
    let keep_view = match split_right {
        true => false,
        false => {
            message.get("keepView") == Some(&Value::Bool(true))
                || focus_changes_active_project(runtime_active_group, &session)
        }
    };
    let preferred_interface = match split_right || !live {
        true => None,
        false => settings
            .resolve(agent_id_of(core, &session))
            .map(str::to_string),
    };
    let native_action = open_remote_session_terminal(
        sidebar_session_id,
        keep_view,
        preferred_interface.as_deref(),
        split_right,
    );
    let focus_group = remote_focus_group(core, &session);
    let attention_acknowledgement = json!({
        "projectId": session.project_key().to_workspace_project_id(),
        "sessionId": session.to_focus_state_session_id(),
        "type": SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE,
        "version": SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION,
    });
    Some(RemoteFocusPlan {
        live,
        session,
        attention_acknowledgement,
        keep_view,
        preferred_interface,
        split_right,
        native_action,
        focus_group,
    })
}

/// `postNativeProjectPathAction`'s payload, with the three options that ride only when they are
/// set: the TypeScript spreads `options.placement ? { placement } : {}`, and an absent key is a
/// different message from a `false` or a `null` to the strict parser on the other side.
pub fn open_remote_session_terminal(
    scoped_session_id: &str,
    keep_view: bool,
    preferred_interface: Option<&str>,
    split_right: bool,
) -> Value {
    let mut payload = Map::new();
    payload.insert(
        "action".to_string(),
        Value::String("openRemoteSessionTerminal".to_string()),
    );
    if split_right {
        payload.insert(
            "placement".to_string(),
            Value::String("splitRight".to_string()),
        );
    }
    if let Some(preferred_interface) = preferred_interface {
        payload.insert(
            "preferredInterface".to_string(),
            Value::String(preferred_interface.to_string()),
        );
    }
    if keep_view {
        payload.insert("keepView".to_string(), Value::Bool(true));
    }
    payload.insert(
        "projectId".to_string(),
        Value::String(scoped_session_id.to_string()),
    );
    payload.insert(
        "type".to_string(),
        Value::String(NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE.to_string()),
    );
    payload.insert(
        "version".to_string(),
        Value::from(NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION),
    );
    Value::Object(payload)
}

/// `focusChangesActiveProject` for a remote target: the active group id is parsed as a remote
/// PROJECT group, and the focus stays inside the project only when that parse names this machine
/// and this project. A subgroup id, the machine's Chats group and every local group all fail that
/// parse and answer "the project changes", which is the rule the string form encodes.
fn focus_changes_active_project(runtime_active_group: Option<&str>, session: &SessionKey) -> bool {
    let Some(project) = runtime_active_group.and_then(ProjectKey::parse_sidebar_group_id) else {
        return true;
    };
    project.machine != session.machine || project.project_id != session.project_id
}

/// The group `setRemotePresentationSessionFocus` makes active for a remote row: the machine's Chats
/// group when THAT machine's live presentation stores the project under a chats folder, else the
/// remote project's own group. Never a user-made subgroup, whatever the row's group is: the old
/// runtime does not look at subgroups here. A machine with no live rows answers the project group,
/// as `remotePresentations.get(machineId)` finding nothing does.
pub fn remote_focus_group(core: &Core, session: &SessionKey) -> String {
    let chat = core
        .presentation()
        .loaded_live(&session.machine)
        .and_then(|loaded| loaded.project(&session.project_id))
        .and_then(|project| project.path.as_deref())
        .is_some_and(is_chat_project_path);
    match chat {
        true => ActiveGroup::Chats(session.machine.clone()).to_sidebar_group_id(),
        false => session.project_key().to_sidebar_group_id(),
    }
}

/// How long a group the host sent is believed without the runtime publishing it. The runtime
/// publishes in the same turn it moves the group (`setRemotePresentationSessionFocus` ends in
/// `postGxserverPresentationFocusState`), so a group it has not published after this long is one it
/// did not take (its post refused, or a later change of its own), and its publish is the truth.
pub const RUNTIME_GROUP_SENT_TRUST_MS: u64 = 2_000;

/// The old runtime's `activeGroupId` as a command sent NOW will find it.
///
/// CDXC:RemoteMachines 2026-09-21 WHY:
/// The runtime runs every script the host sends on one FIFO queue, on its own thread, and its
/// publishes come back later. So its last publish lags: after a remote click, a second click in the
/// same project is planned before the first click's publish is back, and after a local click the
/// store's tell is flushed right before the next command while the last publish still names the
/// remote group. The group the command finds is the one set by the LAST focus-moving script the
/// host queued before it, so that is tracked, and the publish is used only once it can be shown to
/// come after that script:
///
/// - A remote focus the host sent (the tab-selected callback for a remote tab, which the store's
///   own open of a remote row sends and which runs `setRemotePresentationSessionFocus`) is
///   believed until a publish names that same group, a later script replaces it, or
///   [`RUNTIME_GROUP_SENT_TRUST_MS`] passes.
/// - A local selection reaches the runtime as a tell stamped with the store's focus stamp, and
///   every publish echoes the newest stamp it has heard. The tell is flushed before any remote
///   focus is sent, so a tell newer than the sent remote focus, or one still pending, means the command
///   finds the local group the tell set, until a publish echoes that stamp; from then on the
///   publish is fresher than both, because the runtime handled the tell after everything sent
///   before it.
///
/// Anything the runtime does to its group on its own, or on a script not tracked here (a group
/// header's `focusGroup`, a lifecycle replacement), is seen when it publishes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeActiveGroup {
    published: Option<String>,
    published_stamp: u64,
    sent: Option<SentRemoteFocus>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SentRemoteFocus {
    group: String,
    /// The stamp of the newest tell sent before this focus.
    told_stamp: u64,
    at_ms: u64,
}

impl RuntimeActiveGroup {
    /// The runtime published its focus state: its `activeGroupId` and the `focusStamp` it echoes.
    pub fn observe_publish(&mut self, active_group_id: Option<&str>, focus_stamp: u64) {
        self.published = active_group_id.map(str::to_string);
        self.published_stamp = focus_stamp;
        if self
            .sent
            .as_ref()
            .is_some_and(|sent| Some(sent.group.as_str()) == active_group_id)
        {
            self.sent = None;
        }
    }

    /// The host queued a script that runs `setRemotePresentationSessionFocus`, which leaves `group`
    /// active ([`remote_focus_group`]). `told_stamp` is the stamp of the newest tell sent so far.
    pub fn sent_remote_focus(&mut self, group: String, told_stamp: u64, now_ms: u64) {
        self.sent = Some(SentRemoteFocus {
            group,
            told_stamp,
            at_ms: now_ms,
        });
    }

    /// The group a command queued now finds. `told_stamp` is the stamp of the newest tell sent,
    /// `tell_pending` whether a newer one is still waiting (it is flushed before the command).
    /// `None` is a local selection or no group, which both answer "the project changes".
    pub fn current(&self, told_stamp: u64, tell_pending: bool, now_ms: u64) -> Option<&str> {
        if tell_pending {
            return None;
        }
        if let Some(sent) = &self.sent {
            if sent.told_stamp == told_stamp
                && now_ms.saturating_sub(sent.at_ms) < RUNTIME_GROUP_SENT_TRUST_MS
            {
                return Some(sent.group.as_str());
            }
        }
        match self.published_stamp >= told_stamp {
            true => self.published.as_deref(),
            false => None,
        }
    }

    /// The runtime's last publish, whatever came after it. For the gate's mutations.
    pub fn published(&self) -> Option<&str> {
        self.published.as_deref()
    }

    /// The last remote focus sent and not yet published, whatever came after it. For the gate's
    /// mutations.
    pub fn sent_group(&self) -> Option<&str> {
        self.sent.as_ref().map(|sent| sent.group.as_str())
    }
}

/// The agent id of the row on ITS machine, which is what `sessionPreferredAgentInterface` reads out
/// of `remotePresentations.get(machineId)`: the live rows only, never the last-seen copy. A row that
/// machine does not list has none.
fn agent_id_of<'a>(core: &'a Core, session: &SessionKey) -> Option<&'a str> {
    let machine: &MachineId = &session.machine;
    core.presentation()
        .loaded_live(machine)?
        .server_session(&session.project_id, &session.session_id)?
        .agent_id
        .as_deref()
}
