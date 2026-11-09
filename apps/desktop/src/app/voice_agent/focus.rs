//! A voice call about one session (a "focused" call), and which computer a call runs on: the
//! session the work area shows, the header arrow's menu rows, starting, refocusing and widening a
//! call, and showing its session. gxserver limits the agent to the session
//! (`server/src/voice_agent/`); the panel's focus strip draws what this keeps
//! (`panel_focus_strip.rs`). Protocol: the contract's "Focused calls" section
//! (docs/2026-10-20/voice-mode/voice-agent-client-contract.md).
//!
//! CDXC:VoiceMode 2026-10-23 DECISION: User, asked "Can a call switch between one session and all sessions without hanging up?", picked "A. Yes, both ways". While a call is open, the arrow menu's other row, the "Voice call about this session" hotkey and the panel's All sessions button change the open call's scope (a `focus` frame) instead of starting a second call; by voice the agent can only widen it.
//! CDXC:VoiceMode 2026-10-23 DECISION: User, asked "Which sessions can you start a call about?", picked "A. Any session: a thread, an orchestrator, or a plain session". "This session" is whatever session the work area shows, terminal-only ones included; only a browser row is not a session.
//! CDXC:VoiceMode 2026-10-23 WHY: the user was asked whether reading the session in a focused call should switch the desktop to it, didn't answer, and didn't object to the proposal: reads leave the window where it is (the user picked the session and may be reading another one), and asking the agent to show it does show it. gxserver decides; a call on another computer shows here (`show_voice_call_session`).

use ghostex_gx_chat_client::Endpoint;
use ghostex_gx_core::{MachineId, SessionKey};
use serde_json::{Value, json};

use super::conversation::{VoicePhase, VoiceRow};
use super::endpoint::local_voice_endpoint;
use crate::*;

/// What a row reads in place of a computer's name for this one.
pub(crate) const THIS_COMPUTER: &str = "this computer";
/// A title longer than this is cut in the arrow menu, so one long name cannot widen the menu.
const MENU_TITLE_MAX_CHARS: usize = 40;

/// The session a call can be about: the one the work area shows, as the sidebar has it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VoiceCallTarget {
    pub(crate) key: SessionKey,
    pub(crate) title: String,
    pub(crate) project: String,
    /// The connected computer's name, for a session of another computer.
    pub(crate) machine_name: Option<String>,
    pub(crate) agent_icon: Option<String>,
    /// The GBot whose chat the session is, `(botId, name)`: the header arrow offers a call with it
    /// instead of a call about the session (`gbot_call.rs`).
    pub(crate) gbot: Option<(String, String)>,
}

/// The computer a call runs on (this one until a call about another computer's session starts).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallMachine(pub(crate) MachineId);

impl Default for CallMachine {
    fn default() -> Self {
        Self(MachineId::Local)
    }
}

impl std::ops::Deref for CallMachine {
    type Target = MachineId;

    fn deref(&self) -> &MachineId {
        &self.0
    }
}

/// What an open call is about, as gxserver last said (`ready.focus` and `focus` frames).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VoiceFocus {
    pub(crate) key: SessionKey,
    pub(crate) title: String,
    pub(crate) project: String,
    /// The orchestrator's title when the session is one of its threads.
    pub(crate) orchestrator: Option<String>,
    /// The session was closed during the call; the call goes on.
    pub(crate) closed: bool,
    /// The agent's icon key, from this app's sidebar (gxserver doesn't send it).
    pub(crate) agent_icon: Option<String>,
    /// A call with a GBot: the GBot whose chat the session is (`gbot_call.rs`).
    pub(crate) gbot: Option<super::gbot_call::CallGbot>,
}

/// What `ready` says about the call's scope.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReadyFocus {
    /// No `focus` key: a gxserver older than focused calls.
    Unsupported,
    /// `focus: null`: about all sessions.
    All,
    Session(VoiceFocus),
}

/// `{projectId, sessionId, title, project, orchestrator, closed}` on `machine`'s gxserver.
pub(crate) fn parse_focus(value: &Value, machine: &MachineId) -> Option<VoiceFocus> {
    let text = |key: &str| {
        value[key]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let project_id = text("projectId")?;
    let session_id = text("sessionId")?;
    Some(VoiceFocus {
        key: SessionKey {
            machine: machine.clone(),
            project_id,
            session_id,
        },
        title: text("title").unwrap_or_else(|| "this session".to_string()),
        project: text("project").unwrap_or_default(),
        orchestrator: text("orchestrator"),
        closed: value["closed"].as_bool().unwrap_or(false),
        agent_icon: None,
        gbot: value.get("gbot").and_then(super::gbot_call::CallGbot::parse),
    })
}

/// The connected machine a tool frame's `computerSession` is on, from its gxserver server id.
fn computer_session_machine(session: &Value) -> Option<MachineId> {
    let server_id = session["serverId"].as_str()?;
    crate::app::remote_conn::connected_computers::gpui_remote_machine_for_server_id(server_id)
        .map(MachineId::Remote)
}

/// The scope a `ready` frame names.
pub(crate) fn ready_focus(event: &Value, machine: &MachineId) -> ReadyFocus {
    match event.get("focus") {
        None => ReadyFocus::Unsupported,
        Some(Value::Null) => ReadyFocus::All,
        Some(focus) => parse_focus(focus, machine).map_or(ReadyFocus::All, ReadyFocus::Session),
    }
}

/// The `focus` query value and frame value: `<projectId>:<sessionId>`, raw ids of the session's own
/// computer.
pub(crate) fn focus_ref(key: &SessionKey) -> String {
    format!("{}:{}", key.project_id, key.session_id)
}

/// "Update Ghostex on <computer> to have a voice call about one session."
pub(crate) fn outdated_sentence(machine_name: Option<&str>) -> String {
    format!(
        "Update Ghostex on {} to have a voice call about one session.",
        machine_name.unwrap_or(THIS_COMPUTER)
    )
}

/// A missing or refused key on another computer, in general words (that computer's Settings
/// holds it, not this one's).
pub(crate) fn remote_key_sentence(code: &str, machine_name: Option<&str>) -> String {
    let (subject, place) = match machine_name {
        Some(name) => (name.to_string(), name.to_string()),
        None => (
            "The other computer".to_string(),
            "that computer".to_string(),
        ),
    };
    if code == "badKey" {
        format!("{subject} refused its voice key. Check it in Settings › Voice Agent on {place}.")
    } else {
        format!(
            "{subject} has no voice key yet. Add one in Settings › Voice Agent on {place} to have a voice call about its sessions."
        )
    }
}

/// What a row of the header arrow's menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoiceMenuPick {
    AllSessions,
    ThisSession,
}

/// One row of the header arrow's menu.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VoiceMenuRow {
    pub(crate) label: String,
    pub(crate) checked: bool,
    pub(crate) disabled: bool,
    pub(crate) pick: VoiceMenuPick,
}

/// The open call, as the menu needs it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OpenCall<'a> {
    pub(crate) machine: &'a MachineId,
    pub(crate) focus: Option<&'a SessionKey>,
}

/// The arrow menu: "All sessions" and "This session: <title>", the call's scope ticked.
/// `all_label` is "All sessions" with its hotkey. "This session" stays enabled for a session of
/// another computer than the open call's: picking it asks first (`hang_up_and_call_question`).
pub(crate) fn voice_menu_rows(
    call: Option<OpenCall<'_>>,
    target: Option<&VoiceCallTarget>,
    all_label: &str,
) -> Vec<VoiceMenuRow> {
    let mut rows = vec![VoiceMenuRow {
        label: all_label.to_string(),
        checked: call.is_some_and(|call| call.focus.is_none()),
        disabled: false,
        pick: VoiceMenuPick::AllSessions,
    }];
    let Some(target) = target else {
        rows.push(VoiceMenuRow {
            label: "This session: none open".to_string(),
            checked: false,
            disabled: true,
            pick: VoiceMenuPick::ThisSession,
        });
        return rows;
    };
    rows.push(VoiceMenuRow {
        label: format!("This session: {}", menu_title(&target.title)),
        checked: call.is_some_and(|call| call.focus == Some(&target.key)),
        disabled: false,
        pick: VoiceMenuPick::ThisSession,
    });
    rows
}

/// The question "This session" asks while a call is open on another computer than the session's:
/// its title and its detail. `here` runs the open call; `there` runs the session.
pub(crate) fn hang_up_and_call_question(here: &str, title: &str, there: &str) -> (String, String) {
    (
        format!("A voice call is open on {here}"),
        format!(
            "A call stays on the computer it started on. Hang up and start a voice call about {title} on {there}?"
        ),
    )
}

fn menu_title(title: &str) -> String {
    let title = title.trim();
    if title.chars().count() <= MENU_TITLE_MAX_CHARS {
        return title.to_string();
    }
    let cut: String = title.chars().take(MENU_TITLE_MAX_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// What "This session" (the menu row or the hotkey) does to the call that is open now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ThisSessionStep {
    /// No call open: start one about the session, on its computer.
    Start,
    /// The call is already about it.
    Nothing,
    /// Send `{"type":"focus","session":<ref>}` on the open call.
    Refocus,
    /// The open call runs on another computer than the session's: ask to keep it or hang up and
    /// call.
    OtherComputer,
    /// The open call's gxserver is older than focused calls.
    Outdated,
}

pub(crate) fn this_session_step(
    call: Option<OpenCall<'_>>,
    focus_supported: bool,
    target: &SessionKey,
) -> ThisSessionStep {
    match call {
        None => ThisSessionStep::Start,
        Some(call) if *call.machine != target.machine => ThisSessionStep::OtherComputer,
        Some(call) if call.focus == Some(target) => ThisSessionStep::Nothing,
        Some(_) if !focus_supported => ThisSessionStep::Outdated,
        Some(_) => ThisSessionStep::Refocus,
    }
}

/// Whether a call on another computer's `show` asks this app to show the session now.
pub(crate) fn remote_show_wanted(show: Option<&str>, typing: bool) -> bool {
    match show {
        Some("now") => true,
        Some("follow") => !typing,
        _ => false,
    }
}

impl GhostexGpuiApp {
    /// The session the work area shows, or `None` when none is (or it is a browser row).
    pub(crate) fn voice_call_target(&self) -> Option<VoiceCallTarget> {
        let key = self.gx_store.core.focus().focused_session.clone()?;
        self.voice_call_target_for(key)
    }

    /// `key`'s title, project, computer and agent, as the sidebar has them.
    pub(crate) fn voice_call_target_for(&self, key: SessionKey) -> Option<VoiceCallTarget> {
        let sidebar_id = key.to_sidebar_session_id();
        let row = self.native_sidebar.snapshot.as_ref().and_then(|snapshot| {
            snapshot.groups.iter().find_map(|group| {
                group
                    .sessions
                    .iter()
                    .find(|session| session.session_id == sidebar_id)
                    .map(|session| (group, session))
            })
        });
        if row.is_some_and(|(_, session)| session.is_browser()) {
            return None;
        }
        let machine_name = row
            .and_then(|(group, _)| group.remote_machine_context.as_ref())
            .and_then(|context| context.get("machineName").and_then(Value::as_str))
            .map(str::to_string)
            .or_else(|| key.machine.remote_id().map(str::to_string));
        Some(VoiceCallTarget {
            title: row
                .map(|(_, session)| session.title().to_string())
                .unwrap_or_else(|| "this session".to_string()),
            project: row
                .map(|(group, _)| group.title.clone())
                .unwrap_or_default(),
            machine_name: if key.machine.is_local() {
                None
            } else {
                machine_name
            },
            agent_icon: row.and_then(|(_, session)| session.agent_icon.clone()),
            gbot: row
                .and_then(|(group, _)| group.gbot())
                .map(|look| (look.bot_id, look.name)),
            key,
        })
    }

    /// The open (or opening) call, for the menu and the steps above.
    pub(crate) fn voice_open_call(&self) -> Option<OpenCall<'_>> {
        let agent = &self.voice_agent;
        agent.active().then(|| OpenCall {
            machine: &agent.machine,
            focus: agent.focus.as_ref().map(|focus| &focus.key),
        })
    }

    /// Where a call on `machine` talks to: this computer's gxserver, or another computer's over the
    /// SSH forward the app keeps for it. Answers why not, in a sentence, when it can't.
    pub(crate) fn voice_agent_endpoint(&self, machine: &MachineId) -> Result<Endpoint, String> {
        match machine {
            MachineId::Local => local_voice_endpoint().ok_or_else(|| {
                "Ghostex's service is not running yet. Try again in a moment.".to_string()
            }),
            MachineId::Remote(machine_id) => self
                .gpui_remote_gxserver_request_target(machine_id)
                .map(|target| {
                    Endpoint::new(
                        format!("http://127.0.0.1:{}", target.local_port),
                        &target.token,
                    )
                })
                .filter(Endpoint::usable)
                .ok_or_else(|| {
                    "That computer isn't connected right now. Connect to it and try again."
                        .to_string()
                }),
        }
    }

    /// The arrow menu's "All sessions": starts the all-sessions call, or widens the open one.
    pub(crate) fn voice_call_all_sessions(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.voice_agent.active() {
            self.start_voice_agent_conversation(cx);
            return;
        }
        if self.voice_agent.phase == VoicePhase::Live && self.voice_agent.focus.is_some() {
            self.send_voice_agent_focus(None, cx);
        }
    }

    /// The arrow menu's "This session" and the "Voice call about this session" hotkey: starts a call
    /// about the session the work area shows, or moves the open call to it.
    pub(crate) fn voice_call_about_this_session(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(target) = self.voice_call_target() {
            self.voice_call_about(target, window, cx);
        }
    }

    /// Starts a call about `target`, or moves the open call to it.
    pub(crate) fn voice_call_about(
        &mut self,
        target: VoiceCallTarget,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // Until `ready` it isn't known whether the call's gxserver can change scope; while hanging
        // up there is nothing left to change.
        if matches!(
            self.voice_agent.phase,
            VoicePhase::Starting | VoicePhase::Ending
        ) {
            return;
        }
        let step = this_session_step(
            self.voice_open_call(),
            self.voice_agent.focus_supported,
            &target.key,
        );
        match step {
            ThisSessionStep::Start => self.start_voice_agent_call(Some(target), cx),
            ThisSessionStep::Nothing => {}
            ThisSessionStep::Refocus => self.send_voice_agent_focus(Some(&target.key), cx),
            ThisSessionStep::OtherComputer => self.ask_to_hang_up_and_call(target, window, cx),
            ThisSessionStep::Outdated => {
                let text = outdated_sentence(self.voice_agent.machine_name.as_deref());
                self.push_voice_agent_announcement(text, cx);
            }
        }
    }

    /// A call can't move computers: its provider session, captions and tools live in the gxserver
    /// it was opened on. So a call about a session of another computer than the open call's asks
    /// first.
    ///
    /// CDXC:VoiceMode 2026-10-24 DECISION: User picked "Both ask Keep or Hang up" (Q484): with a call open on another computer, starting a call about a session here asks the same as the phone, "Keep the call" or "Hang up and call", instead of greying out "This session". Hang up and call ends the open call and starts the call about the session on its own computer once the hang-up is through (`call_after_hang_up`). SEE-ALSO: the phone's `apps/mobile/app/src/voice/callAbout.ts`.
    fn ask_to_hang_up_and_call(
        &mut self,
        target: VoiceCallTarget,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let here = self
            .voice_agent
            .machine_name
            .clone()
            .unwrap_or_else(|| THIS_COMPUTER.to_string());
        let there = target.machine_name.as_deref().unwrap_or(THIS_COMPUTER);
        let (title, detail) = hang_up_and_call_question(&here, &target.title, there);
        // Keep the call is the cancel choice (Escape keeps the call), as on the phone.
        let answer = window.prompt(
            gpui::PromptLevel::Info,
            &title,
            Some(&detail),
            &[
                gpui::PromptButton::cancel("Keep the call"),
                gpui::PromptButton::new("Hang up and call"),
            ],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update(cx, |this, cx| this.hang_up_and_call(target, cx));
            }
        })
        .detach();
    }

    /// Hangs up the open call and starts the call about `target` once it is over.
    fn hang_up_and_call(&mut self, target: VoiceCallTarget, cx: &mut gpui::Context<Self>) {
        // The call may have ended while the question was open.
        if !self.voice_agent.active() {
            self.start_voice_agent_call(Some(target), cx);
            return;
        }
        self.voice_agent.call_after_hang_up = Some(target);
        if self.voice_agent.phase != VoicePhase::Ending {
            self.hang_up_voice_agent(cx);
        }
    }

    /// Asks gxserver to change the open call's scope; its `focus` frame answers and redraws.
    fn send_voice_agent_focus(&mut self, key: Option<&SessionKey>, cx: &mut gpui::Context<Self>) {
        if !self.voice_agent.focus_supported {
            let text = outdated_sentence(self.voice_agent.machine_name.as_deref());
            self.push_voice_agent_announcement(text, cx);
            return;
        }
        let session = key.map_or(Value::Null, |key| json!(focus_ref(key)));
        self.send_voice_agent_frame(&json!({ "type": "focus", "session": session }));
    }

    fn push_voice_agent_announcement(&mut self, text: String, cx: &mut gpui::Context<Self>) {
        self.voice_agent.rows.push(VoiceRow::Announcement { text });
        self.sync_voice_agent_panel(cx);
    }

    /// Takes the scope a `ready` or `focus` frame names. The agent's icon comes from this app's
    /// sidebar store, which gxserver doesn't send.
    pub(super) fn apply_voice_agent_focus(&mut self, focus: Option<VoiceFocus>) {
        let focus = focus.map(|mut focus| {
            focus.agent_icon = self
                .gx_store
                .core
                .presentation()
                .session(&focus.key)
                .and_then(|session| session.agent_icon.clone());
            focus
        });
        self.voice_agent.focus = focus;
    }

    /// The strip's title, and a session a call on another computer showed: this app shows it, since
    /// that computer's gxserver can only switch its own screen.
    pub(crate) fn show_voice_call_session(
        &mut self,
        key: &SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let sidebar_id = key.to_sidebar_session_id();
        // Switches the sidebar to the session's computer and reveals its row, the way a
        // notification click does (gx_store/activation_focus.rs).
        let _ = self.dispatch_gpui_status_activation(&sidebar_id, cx);
    }

    /// A finished tool of a call on another computer that names a session to show (`show`, sent
    /// with `clientRemote=1`): shown here, as this computer's gxserver shows it for a call that runs
    /// here. `now` (the user asked to see it) always shows; `follow` (an all-sessions call touched it)
    /// shows unless the user is typing.
    ///
    /// A session of another connected computer comes as `computerSession`, with that computer's
    /// server id, from a call on any computer: the call's gxserver can only switch its own screen.
    pub(super) fn follow_remote_voice_call_tool(
        &mut self,
        event: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let elsewhere = &event["computerSession"];
        if elsewhere.is_null() && self.voice_agent.machine.is_local() {
            return;
        }
        if !remote_show_wanted(event["show"].as_str(), super::typing::user_is_typing()) {
            return;
        }
        let shown = if elsewhere.is_null() {
            parse_focus(&event["session"], &self.voice_agent.machine)
        } else {
            computer_session_machine(elsewhere).and_then(|machine| parse_focus(elsewhere, &machine))
        };
        if let Some(shown) = shown {
            self.show_voice_call_session(&shown.key, cx);
        }
    }

    /// Whether the open call is about this sidebar row (its sidebar session id), for the row's
    /// headset badge.
    pub(crate) fn voice_call_is_about(&self, sidebar_session_id: &str) -> bool {
        let agent = &self.voice_agent;
        matches!(agent.phase, VoicePhase::Starting | VoicePhase::Live)
            && agent
                .focus
                .as_ref()
                .is_some_and(|focus| focus.key.to_sidebar_session_id() == sidebar_session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(session: &str) -> SessionKey {
        SessionKey::local("P1", session)
    }

    fn target(key: SessionKey, title: &str) -> VoiceCallTarget {
        VoiceCallTarget {
            key,
            title: title.to_string(),
            project: "Ghostex".to_string(),
            machine_name: None,
            agent_icon: None,
            gbot: None,
        }
    }

    #[test]
    fn ready_without_focus_key_is_an_older_gxserver() {
        let machine = MachineId::Local;
        assert_eq!(
            ready_focus(&json!({"type": "ready"}), &machine),
            ReadyFocus::Unsupported
        );
        assert_eq!(
            ready_focus(&json!({"type": "ready", "focus": null}), &machine),
            ReadyFocus::All
        );
        let ReadyFocus::Session(focus) = ready_focus(
            &json!({"type": "ready", "focus": {
                "projectId": "P1", "sessionId": "G1", "title": "Voice mode",
                "project": "Ghostex", "orchestrator": "Main Coordinator", "closed": false,
            }}),
            &machine,
        ) else {
            panic!("a focus");
        };
        assert_eq!(focus.key, local("G1"));
        assert_eq!(focus.title, "Voice mode");
        assert_eq!(focus.orchestrator.as_deref(), Some("Main Coordinator"));
        assert!(!focus.closed);
    }

    #[test]
    fn a_focus_frame_keeps_the_calls_computer_and_the_closed_flag() {
        let machine = MachineId::Remote("remote-ab12".to_string());
        let focus = parse_focus(
            &json!({"projectId": "P9", "sessionId": "G9", "title": "Build", "orchestrator": null, "closed": true}),
            &machine,
        )
        .unwrap();
        assert_eq!(focus.key, SessionKey::remote("remote-ab12", "P9", "G9"));
        assert!(focus.closed);
        assert_eq!(focus.orchestrator, None);
        assert_eq!(parse_focus(&json!({"projectId": "P9"}), &machine), None);
    }

    #[test]
    fn menu_without_a_call_offers_both_rows_unticked() {
        let shown = target(local("G1"), "Voice mode");
        let rows = voice_menu_rows(None, Some(&shown), "All sessions (⌥⌃V)");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "All sessions (⌥⌃V)");
        assert_eq!(rows[0].pick, VoiceMenuPick::AllSessions);
        assert!(!rows[0].checked && !rows[1].checked);
        assert_eq!(rows[1].label, "This session: Voice mode");
        assert!(!rows[1].disabled);
    }

    #[test]
    fn menu_ticks_the_open_calls_scope() {
        let shown = target(local("G1"), "Voice mode");
        let machine = MachineId::Local;
        let all = OpenCall {
            machine: &machine,
            focus: None,
        };
        let rows = voice_menu_rows(Some(all), Some(&shown), "All sessions");
        assert!(rows[0].checked && !rows[1].checked);
        let key = local("G1");
        let focused = OpenCall {
            machine: &machine,
            focus: Some(&key),
        };
        let rows = voice_menu_rows(Some(focused), Some(&shown), "All sessions");
        assert!(!rows[0].checked && rows[1].checked);
        let other = local("G2");
        let elsewhere = OpenCall {
            machine: &machine,
            focus: Some(&other),
        };
        let rows = voice_menu_rows(Some(elsewhere), Some(&shown), "All sessions");
        assert!(!rows[0].checked && !rows[1].checked && !rows[1].disabled);
    }

    #[test]
    fn menu_offers_a_session_of_another_computer_than_the_calls() {
        let mut shown = target(SessionKey::remote("remote-ab12", "P9", "G9"), "Build");
        shown.machine_name = Some("studio".to_string());
        let machine = MachineId::Local;
        let call = OpenCall {
            machine: &machine,
            focus: None,
        };
        let rows = voice_menu_rows(Some(call), Some(&shown), "All sessions");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].label, "This session: Build");
        assert!(!rows[1].disabled && !rows[1].checked);
        assert_eq!(
            this_session_step(Some(call), true, &shown.key),
            ThisSessionStep::OtherComputer
        );
    }

    #[test]
    fn another_computers_session_asks_like_the_phone() {
        let (title, detail) = hang_up_and_call_question("this computer", "Build", "studio");
        assert_eq!(title, "A voice call is open on this computer");
        assert_eq!(
            detail,
            "A call stays on the computer it started on. Hang up and start a voice call about Build on studio?"
        );
    }

    #[test]
    fn menu_without_a_shown_session_disables_this_session() {
        let rows = voice_menu_rows(None, None, "All sessions");
        assert_eq!(rows[1].label, "This session: none open");
        assert!(rows[1].disabled);
    }

    #[test]
    fn menu_cuts_a_long_title() {
        let long = "A".repeat(80);
        let shown = target(local("G1"), &long);
        let rows = voice_menu_rows(None, Some(&shown), "All sessions");
        assert_eq!(
            rows[1].label.chars().count(),
            "This session: ".len() + MENU_TITLE_MAX_CHARS
        );
        assert!(rows[1].label.ends_with('…'));
    }

    #[test]
    fn this_session_starts_refocuses_or_refuses() {
        let shown = local("G1");
        assert_eq!(
            this_session_step(None, false, &shown),
            ThisSessionStep::Start
        );
        let machine = MachineId::Local;
        let all = OpenCall {
            machine: &machine,
            focus: None,
        };
        assert_eq!(
            this_session_step(Some(all), true, &shown),
            ThisSessionStep::Refocus
        );
        assert_eq!(
            this_session_step(Some(all), false, &shown),
            ThisSessionStep::Outdated
        );
        let focused = OpenCall {
            machine: &machine,
            focus: Some(&shown),
        };
        assert_eq!(
            this_session_step(Some(focused), true, &shown),
            ThisSessionStep::Nothing
        );
        let remote = SessionKey::remote("remote-ab12", "P1", "G1");
        assert_eq!(
            this_session_step(Some(all), true, &remote),
            ThisSessionStep::OtherComputer
        );
    }

    #[test]
    fn a_remote_call_shows_what_the_user_asked_for_and_follows_unless_typing() {
        assert!(remote_show_wanted(Some("now"), true));
        assert!(remote_show_wanted(Some("follow"), false));
        assert!(!remote_show_wanted(Some("follow"), true));
        assert!(!remote_show_wanted(None, false));
    }

    #[test]
    fn sentences_name_the_computer() {
        assert_eq!(
            outdated_sentence(Some("studio")),
            "Update Ghostex on studio to have a voice call about one session."
        );
        assert_eq!(
            outdated_sentence(None),
            "Update Ghostex on this computer to have a voice call about one session."
        );
        assert!(
            remote_key_sentence("noKey", Some("studio"))
                .starts_with("studio has no voice key yet.")
        );
        assert!(
            remote_key_sentence("badKey", Some("studio"))
                .starts_with("studio refused its voice key.")
        );
        assert!(remote_key_sentence("noKey", None).contains("on that computer"));
        assert_eq!(focus_ref(&local("G1")), "P1:G1");
    }
}
