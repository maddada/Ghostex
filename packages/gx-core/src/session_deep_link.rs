//! What opening a `ghostex://session/<Global Ref>` link does: which computer the ref names, what
//! may happen to the session there, and the words every outcome says. The host performs it
//! (apps/desktop/src/app/gx_store/session_deep_link.rs).
//!
//! CDXC:OsIntegration 2026-10-31 DECISION:
//! User (GitHub issue #373): "Build it, remote machines too", and a closed session offers to restore it, while the link "must only focus or offer to restore, and never type, run, start an agent, or restore without the prompt". So a focus that would start the session's agent asks first, as a restore does. A link opened on another person's computer finds no machine with the ref's server id: it names what it cannot find and does nothing else.
//!
//! The ref's server id picks the machine, and its project and session ids pick the row. `ghostex focus` compares the same ids in the CLI, against this computer's daemon alone; the app holds every machine's rows, so it reads those and ends in the focus `ghostex focus` itself reaches (`focusSession`).
//! SEE-ALSO: packages/gx-protocol/src/session_ref.rs (the link's grammar), server/src/ghostex_cli/selector.rs (`resolve_live_or_closed_session`), server/src/previous_session_restore.rs (`closed_row`, the same read of Previous Sessions).

use ghostex_gx_protocol::{
    GlobalSessionRef, LifecycleState, PresentationSession, ProviderSessionState,
};
use serde_json::{Map, Value};

use crate::keys::{MachineId, SessionKey};
use crate::presentation_store::PresentationStore;
use crate::sidebar_view::{machine_state_is_busy, MachineTabInput};

/// The most of a session's title a prompt quotes.
const PROMPT_TITLE_MAX_CHARS: usize = 120;

/// Where a link's session lives, as far as this computer can tell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionLinkPlace {
    /// A machine that can still answer has not yet: ask again shortly.
    Waiting,
    /// This computer, or a connected machine.
    Reachable(SessionKey),
    /// A saved machine that holds the session and is not connected.
    NotConnected { label: String },
    /// No machine here has the ref's server. `unconnected` are the saved machines that are not
    /// connected, whose server this run has not learned: the session may be on one of them.
    NotKnown { unconnected: Vec<String> },
}

/// Finds the machine a ref names. `machines` are the host's tabs; `launching` says the app has just
/// started, when a machine that reads `disconnected` may not have begun connecting yet.
pub fn place_session_link(
    store: &PresentationStore,
    machines: &[MachineTabInput],
    target: &GlobalSessionRef<'_>,
    launching: bool,
) -> SessionLinkPlace {
    let local_server = store
        .loaded(&MachineId::Local)
        .map(|loaded| loaded.server_id.as_str())
        .filter(|server_id| !server_id.is_empty());
    let Some(local_server) = local_server else {
        return SessionLinkPlace::Waiting;
    };
    let key_on = |machine: MachineId| SessionKey {
        machine,
        project_id: target.project_id.to_string(),
        session_id: target.session_id.to_string(),
    };
    if local_server == target.server_id {
        return SessionLinkPlace::Reachable(key_on(MachineId::Local));
    }
    // The tabs are empty only until the host has read its settings.
    if machines.is_empty() {
        return SessionLinkPlace::Waiting;
    }
    let may_still_answer = |machine: &MachineTabInput| {
        machine_state_is_busy(&machine.state)
            || (launching && machine.state == "disconnected")
            || (machine.is_connected()
                && store
                    .loaded_live(&MachineId::Remote(machine.machine_id.clone()))
                    .is_none())
    };
    let remotes = || machines.iter().filter(|machine| !machine.is_local());
    for machine in remotes() {
        let id = MachineId::Remote(machine.machine_id.clone());
        let Some(loaded) = store.loaded(&id) else {
            continue;
        };
        // The stored last-seen copy carries no server id, so a machine that has not connected in
        // this run is recognised by the rows it held.
        let named = if loaded.server_id.is_empty() {
            loaded
                .server_session(target.project_id, target.session_id)
                .is_some()
        } else {
            loaded.server_id == target.server_id
        };
        if !named {
            continue;
        }
        if machine.is_connected() && !loaded.last_seen {
            return SessionLinkPlace::Reachable(key_on(id));
        }
        if may_still_answer(machine) {
            return SessionLinkPlace::Waiting;
        }
        return SessionLinkPlace::NotConnected {
            label: machine.label.clone(),
        };
    }
    if remotes().any(may_still_answer) {
        return SessionLinkPlace::Waiting;
    }
    SessionLinkPlace::NotKnown {
        unconnected: remotes()
            .filter(|machine| !machine.is_connected())
            .map(|machine| machine.label.clone())
            .collect(),
    }
}

/// What a link may do to the session on a machine that can answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionLinkStep {
    /// Focus it; nothing starts.
    Focus,
    /// Focus it and leave it asleep, on its wake placeholder.
    FocusKeepingAsleep,
    /// A focus would start its agent, so the user is asked first.
    AskToStart { title: String },
    /// The machine lists no open session by these ids: Previous Sessions answers.
    LookInPreviousSessions,
}

/// `click_to_wake` is Click to Wake Sleeping Panes, the one setting under which a local focus can
/// show a sleeping session without waking it. A remote focus attaches, which wakes.
pub fn session_link_step(
    store: &PresentationStore,
    session: &SessionKey,
    click_to_wake: bool,
) -> SessionLinkStep {
    let Some(row) = store.session(session) else {
        return SessionLinkStep::LookInPreviousSessions;
    };
    if row.lifecycle_state == LifecycleState::Stopped {
        return SessionLinkStep::LookInPreviousSessions;
    }
    if row.lifecycle_state == LifecycleState::Sleeping
        && click_to_wake
        && session.machine.is_local()
    {
        return SessionLinkStep::FocusKeepingAsleep;
    }
    if focus_would_start_agent(&row) {
        return SessionLinkStep::AskToStart {
            title: session_title(&row),
        };
    }
    SessionLinkStep::Focus
}

/// A sleeping session wakes on a focus, and the attach of a row with no daemon behind it starts one,
/// which is how a draft's agent first starts (`ghostex create-agent --defer-start`).
fn focus_would_start_agent(row: &PresentationSession) -> bool {
    row.lifecycle_state == LifecycleState::Sleeping
        || row.provider_session_state == ProviderSessionState::Missing
        || (row.is_draft && row.provider_session_state != ProviderSessionState::Exists)
}

fn session_title(row: &PresentationSession) -> String {
    [
        row.display_title.as_deref(),
        row.primary_title.as_deref(),
        row.terminal_title.as_deref(),
        Some(row.title.as_str()),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|title| !title.is_empty())
    .unwrap_or_default()
    .to_string()
}

/// The closed session's title out of an `/api/listPreviousSessions` result; `None` when it is not
/// there, so nothing can restore it. An untitled row answers with an empty title.
pub fn previous_session_title(
    previous: &Value,
    project_id: &str,
    session_id: &str,
) -> Option<String> {
    let text = |row: &'_ Map<String, Value>, key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let row = previous
        .get("results")?
        .as_array()?
        .iter()
        .filter_map(Value::as_object)
        .find(|row| text(row, "projectId") == project_id && text(row, "sessionId") == session_id)?;
    Some(
        ["displayTitle", "primaryTitle", "title"]
            .iter()
            .map(|key| text(row, key))
            .find(|title| !title.is_empty())
            .unwrap_or_default(),
    )
}

/// The one toast an outcome shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLinkNotice {
    pub title: String,
    pub description: String,
}

fn notice(title: impl Into<String>, description: impl Into<String>) -> SessionLinkNotice {
    SessionLinkNotice {
        title: title.into(),
        description: description.into(),
    }
}

/// What a `ghostex://terminal` link gets now (`is_terminal_link_route` in gx-protocol).
pub fn terminal_link_gone_notice() -> SessionLinkNotice {
    notice(
        "This link type is gone",
        "Ghostex no longer starts terminals from ghostex://terminal links. Nothing was run.",
    )
}

pub fn session_link_invalid_notice() -> SessionLinkNotice {
    notice(
        "This link is broken",
        "Ask for a new link. Nothing was opened.",
    )
}

pub fn session_link_not_connected_notice(label: &str) -> SessionLinkNotice {
    notice(
        format!("{label} isn't connected"),
        format!("This chat is on {label}. Connect to it, then open the link again."),
    )
}

/// Also what someone else's link shows: the names of the user's own machines, never the server id
/// the link carries (CDXC:Notices 2026-11-06, Q1410: an id tells a person nothing).
pub fn session_link_not_known_notice(
    _server_id: &str,
    unconnected: &[String],
) -> SessionLinkNotice {
    let mut description = "That chat isn't on this computer or on one connected to it.".to_string();
    match unconnected {
        [] => {}
        [one] => description.push_str(&format!(" It may be on {one}, which isn't connected.")),
        many => description.push_str(&format!(
            " It may be on {}, which aren't connected.",
            many.join(", ")
        )),
    }
    description.push_str(" Nothing was opened.");
    notice("Link is for another computer", description)
}

/// `machine` is the session's machine by name; `None` for this computer.
pub fn session_link_gone_notice(machine: Option<&str>) -> SessionLinkNotice {
    let place = machine.unwrap_or("this computer");
    notice(
        "This chat no longer exists",
        format!("It isn't open or in Previous Sessions on {place}."),
    )
}

pub fn session_link_unanswered_notice(machine: Option<&str>) -> SessionLinkNotice {
    let place = machine.unwrap_or("Ghostex");
    notice(
        "Couldn't look up this session",
        format!("{place} didn't answer. Nothing was opened; try the link again."),
    )
}

pub fn session_link_restore_failed_notice() -> SessionLinkNotice {
    notice(
        "Couldn't restore this session",
        "Open Previous Sessions to try again.",
    )
}

/// The one question an outcome asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLinkPrompt {
    pub title: String,
    pub detail: String,
    pub confirm: String,
}

fn quoted(title: &str, fallback: &str) -> String {
    let line = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return fallback.to_string();
    }
    if line.chars().count() <= PROMPT_TITLE_MAX_CHARS {
        return format!("\"{line}\"");
    }
    let clipped: String = line.chars().take(PROMPT_TITLE_MAX_CHARS).collect();
    format!("\"{clipped}…\"")
}

fn on_machine(machine: Option<&str>) -> String {
    machine
        .map(|name| format!(" on {name}"))
        .unwrap_or_default()
}

pub fn session_link_restore_prompt(title: &str, machine: Option<&str>) -> SessionLinkPrompt {
    SessionLinkPrompt {
        title: "Restore this session?".to_string(),
        detail: format!(
            "{}{} is closed. Restoring it from Previous Sessions starts its agent again.",
            quoted(title, "This session"),
            on_machine(machine),
        ),
        confirm: "Restore".to_string(),
    }
}

pub fn session_link_start_prompt(title: &str, machine: Option<&str>) -> SessionLinkPrompt {
    SessionLinkPrompt {
        title: "Start this session?".to_string(),
        detail: format!(
            "{}{} isn't running. Opening it starts its agent.",
            quoted(title, "This session"),
            on_machine(machine),
        ),
        confirm: "Open".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation_store::SnapshotOrigin;
    use ghostex_gx_protocol::{parse_global_session_ref, PresentationSnapshot};
    use serde_json::json;

    const LINK: &str = "S9:P1:G1";

    fn target() -> GlobalSessionRef<'static> {
        parse_global_session_ref(LINK).expect("a ref")
    }

    fn snapshot(sessions: Vec<Value>) -> PresentationSnapshot {
        serde_json::from_value(json!({
            "revision": 1,
            "generatedAt": "2026-10-31T00:00:00.000Z",
            "projects": [],
            "groups": [],
            "sessions": sessions,
        }))
        .expect("a snapshot")
    }

    fn session(extra: Value) -> Value {
        let mut row = json!({
            "sessionId": "G1",
            "projectId": "P1",
            "groupId": "P1:active",
            "kind": "agent",
            "surface": "workspace",
            "zmxName": "z",
            "title": "Fix the build",
            "activity": "idle",
            "lifecycleState": "running",
            "providerSessionState": "exists",
        });
        for (key, value) in extra.as_object().expect("an object") {
            row[key] = value.clone();
        }
        row
    }

    fn store_with(machine: &MachineId, server_id: &str, sessions: Vec<Value>) -> PresentationStore {
        let mut store = PresentationStore::default();
        store.apply_snapshot(
            machine,
            server_id,
            snapshot(sessions),
            SnapshotOrigin::Stream,
        );
        store
    }

    fn local_store() -> PresentationStore {
        store_with(&MachineId::Local, "S1", vec![])
    }

    fn tab(machine_id: &str, label: &str, state: &str) -> MachineTabInput {
        MachineTabInput {
            machine_id: machine_id.to_string(),
            label: label.to_string(),
            short_name: String::new(),
            state: state.to_string(),
            message: None,
            fed: true,
            this_computer: false,
        }
    }

    fn this_computer() -> MachineTabInput {
        tab(crate::sidebar_view::LOCAL_MACHINE_ID, "Local", "connected")
    }

    fn remote() -> MachineId {
        MachineId::Remote("m1".to_string())
    }

    fn remote_key() -> SessionKey {
        SessionKey {
            machine: remote(),
            project_id: "P1".to_string(),
            session_id: "G1".to_string(),
        }
    }

    fn local_key() -> SessionKey {
        SessionKey {
            machine: MachineId::Local,
            ..remote_key()
        }
    }

    #[test]
    fn this_computers_ref_is_reachable_and_waits_for_the_daemon() {
        let store = store_with(&MachineId::Local, "S9", vec![]);
        assert_eq!(
            place_session_link(&store, &[], &target(), false),
            SessionLinkPlace::Reachable(local_key())
        );
        // Opened while the app starts: nothing has loaded yet, and that is no answer.
        assert_eq!(
            place_session_link(&PresentationStore::default(), &[], &target(), false),
            SessionLinkPlace::Waiting
        );
    }

    #[test]
    fn a_connected_machine_with_the_refs_server_is_reachable() {
        let mut store = local_store();
        store.apply_snapshot(&remote(), "S9", snapshot(vec![]), SnapshotOrigin::Stream);
        assert_eq!(
            place_session_link(&store, &[tab("m1", "Mini", "connected")], &target(), false),
            SessionLinkPlace::Reachable(remote_key())
        );
    }

    #[test]
    fn a_machine_that_dropped_says_it_is_not_connected() {
        let mut store = local_store();
        store.apply_snapshot(&remote(), "S9", snapshot(vec![]), SnapshotOrigin::Stream);
        assert_eq!(
            place_session_link(&store, &[tab("m1", "Mini", "sshFailed")], &target(), false),
            SessionLinkPlace::NotConnected {
                label: "Mini".to_string()
            }
        );
    }

    #[test]
    fn a_machine_not_yet_connected_this_run_is_known_by_its_last_seen_rows() {
        let mut store = local_store();
        store.seed_last_seen(&remote(), snapshot(vec![session(json!({}))]));
        assert_eq!(
            place_session_link(
                &store,
                &[tab("m1", "Mini", "disconnected")],
                &target(),
                false
            ),
            SessionLinkPlace::NotConnected {
                label: "Mini".to_string()
            }
        );
        // Connected but its stream has not replaced the stored copy: the live rows are coming.
        assert_eq!(
            place_session_link(&store, &[tab("m1", "Mini", "connected")], &target(), false),
            SessionLinkPlace::Waiting
        );
        assert_eq!(
            place_session_link(&store, &[tab("m1", "Mini", "connecting")], &target(), false),
            SessionLinkPlace::Waiting
        );
    }

    #[test]
    fn a_stranger_s_link_names_what_it_cannot_find() {
        // Before the host has read its settings there is no telling yet.
        assert_eq!(
            place_session_link(&local_store(), &[], &target(), false),
            SessionLinkPlace::Waiting
        );
        // Another person's computer: no saved machines at all.
        assert_eq!(
            place_session_link(&local_store(), &[this_computer()], &target(), false),
            SessionLinkPlace::NotKnown {
                unconnected: vec![]
            }
        );
        let notice = session_link_not_known_notice("S9", &[]);
        assert_eq!(notice.title, "Link is for another computer");
        assert!(!notice.description.contains("S9"));
        assert!(notice.description.contains("Nothing was opened"));
    }

    #[test]
    fn an_unknown_server_mentions_only_the_machines_it_could_be_on() {
        let mut store = local_store();
        store.apply_snapshot(&remote(), "S5", snapshot(vec![]), SnapshotOrigin::Stream);
        let machines = [
            tab("m1", "Mini", "connected"),
            tab("m2", "Studio", "disconnected"),
        ];
        let place = place_session_link(&store, &machines, &target(), false);
        assert_eq!(
            place,
            SessionLinkPlace::NotKnown {
                unconnected: vec!["Studio".to_string()]
            }
        );
        assert!(session_link_not_known_notice("S9", &["Studio".to_string()])
            .description
            .contains("It may be on Studio, which isn't connected."));
    }

    #[test]
    fn a_launch_waits_for_machines_that_have_not_begun_connecting() {
        let machines = [tab("m2", "Studio", "disconnected")];
        assert_eq!(
            place_session_link(&local_store(), &machines, &target(), true),
            SessionLinkPlace::Waiting
        );
        assert!(matches!(
            place_session_link(&local_store(), &machines, &target(), false),
            SessionLinkPlace::NotKnown { .. }
        ));
    }

    #[test]
    fn a_running_session_only_takes_the_focus() {
        let store = store_with(&MachineId::Local, "S9", vec![session(json!({}))]);
        assert_eq!(
            session_link_step(&store, &local_key(), true),
            SessionLinkStep::Focus
        );
    }

    #[test]
    fn a_sleeping_session_never_wakes_without_being_asked() {
        let asleep = session(json!({ "lifecycleState": "sleeping" }));
        let store = store_with(&MachineId::Local, "S9", vec![asleep.clone()]);
        assert_eq!(
            session_link_step(&store, &local_key(), true),
            SessionLinkStep::FocusKeepingAsleep
        );
        let ask = SessionLinkStep::AskToStart {
            title: "Fix the build".to_string(),
        };
        assert_eq!(session_link_step(&store, &local_key(), false), ask);
        // A remote focus attaches, whatever the setting says.
        let store = store_with(&remote(), "S9", vec![asleep]);
        assert_eq!(session_link_step(&store, &remote_key(), true), ask);
    }

    #[test]
    fn a_session_with_no_daemon_behind_it_asks_before_starting_one() {
        for extra in [
            json!({ "providerSessionState": "missing" }),
            json!({ "isDraft": true, "providerSessionState": "unknown" }),
        ] {
            let store = store_with(&MachineId::Local, "S9", vec![session(extra)]);
            assert!(matches!(
                session_link_step(&store, &local_key(), true),
                SessionLinkStep::AskToStart { .. }
            ));
        }
        // A draft the user already opened has its daemon up.
        let draft = session(json!({ "isDraft": true, "providerSessionState": "exists" }));
        let store = store_with(&MachineId::Local, "S9", vec![draft]);
        assert_eq!(
            session_link_step(&store, &local_key(), true),
            SessionLinkStep::Focus
        );
    }

    #[test]
    fn a_session_the_machine_does_not_list_goes_to_previous_sessions() {
        let store = store_with(&MachineId::Local, "S9", vec![]);
        assert_eq!(
            session_link_step(&store, &local_key(), true),
            SessionLinkStep::LookInPreviousSessions
        );
        let stopped = session(json!({ "lifecycleState": "stopped" }));
        let store = store_with(&MachineId::Local, "S9", vec![stopped]);
        assert_eq!(
            session_link_step(&store, &local_key(), true),
            SessionLinkStep::LookInPreviousSessions
        );
    }

    #[test]
    fn previous_sessions_answer_with_the_exact_session_only() {
        let previous = json!({ "results": [
            { "projectId": "P1", "sessionId": "G11", "title": "Close match" },
            { "projectId": "P2", "sessionId": "G1", "title": "Other project" },
            { "projectId": "P1", "sessionId": "G1", "title": "  ", "displayTitle": "Fix the build" },
        ]});
        assert_eq!(
            previous_session_title(&previous, "P1", "G1"),
            Some("Fix the build".to_string())
        );
        assert_eq!(previous_session_title(&previous, "P1", "G2"), None);
        assert_eq!(previous_session_title(&json!({}), "P1", "G1"), None);
        let untitled = json!({ "results": [{ "projectId": "P1", "sessionId": "G1" }] });
        assert_eq!(
            previous_session_title(&untitled, "P1", "G1"),
            Some(String::new())
        );
    }

    #[test]
    fn a_retired_terminal_link_says_nothing_ran() {
        let notice = terminal_link_gone_notice();
        assert_eq!(notice.title, "This link type is gone");
        assert!(notice.description.contains("Nothing was run"));
    }

    #[test]
    fn prompts_say_what_confirming_starts() {
        let prompt = session_link_restore_prompt("Fix\n the  build", Some("Mini"));
        assert_eq!(prompt.confirm, "Restore");
        assert_eq!(
            prompt.detail,
            "\"Fix the build\" on Mini is closed. Restoring it from Previous Sessions starts its agent again."
        );
        let prompt = session_link_start_prompt("", None);
        assert_eq!(
            prompt.detail,
            "This session isn't running. Opening it starts its agent."
        );
        let long = session_link_start_prompt(&"x".repeat(500), None);
        assert!(long.detail.chars().count() < 200);
    }
}
