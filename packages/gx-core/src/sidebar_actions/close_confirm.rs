//! Whether a close asks first, and what the question says.
//!
//! CDXC:Sessions 2026-11-02 DECISION:
//! User: closing a coordinator, or any agent session that is running (mid-turn or with a live
//! agent), shows a confirmation first, on every way to close it; idle sessions close as today, and
//! the dialog names what will close ("Close Main Coordinator and its 3 open threads?"). No "don't
//! ask again": no other confirmation here has one. A Main Coordinator closed by a stray tab close
//! on 2026-11-02 is what asked for it.
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_close_confirm.rs (the desktop and web doors that
//! ask), apps/mobile/app/src/screens/sessions-screen/close-confirm.ts (the phone's words), server/src/server/coordinator_close.rs
//! (closing a coordinator closes its open threads).

use serde_json::Value;

use crate::core::Core;
use crate::keys::SessionKey;
use ghostex_gx_protocol::{LifecycleState, PresentationSession, SessionActivity, SessionKind};

use super::bulk::plan_bulk_request;
use super::resolve::text_field;

/// The question a close asks before it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseConfirmation {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
}

/// Why one session asks before it closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Busy {
    /// A coordinator always asks: its open threads close with it.
    Coordinator {
        open_threads: usize,
    },
    Working,
    Waiting,
    Background,
}

/// A coordinator's threads that are open in the sidebar: the ones closing it closes.
const OPEN_THREAD_STATES: [&str; 4] = ["waiting", "working", "finished", "sleeping"];

fn open_thread_count(core: &Core, coordinator: &SessionKey) -> usize {
    super::bulk::coordinator_thread_keys(core, coordinator, |row| {
        row.coordinator_thread_state
            .as_deref()
            .is_some_and(|state| OPEN_THREAD_STATES.contains(&state))
    })
    .len()
}

fn busy(core: &Core, session: &SessionKey, row: &PresentationSession) -> Option<Busy> {
    if row.lifecycle_state == LifecycleState::Stopped {
        return None;
    }
    if row.coordinator_role.as_deref() == Some("coordinator") {
        return Some(Busy::Coordinator {
            open_threads: open_thread_count(core, session),
        });
    }
    if row.lifecycle_state != LifecycleState::Running
        || (row.kind != SessionKind::Agent && row.agent_id.is_none())
    {
        return None;
    }
    if row.has_question() || row.awaiting_approval {
        return Some(Busy::Waiting);
    }
    if row.activity == SessionActivity::Working {
        return Some(Busy::Working);
    }
    row.background_work_detected_at
        .is_some()
        .then_some(Busy::Background)
}

fn name(row: &PresentationSession) -> String {
    let title = [&row.display_title, &row.primary_title]
        .into_iter()
        .flatten()
        .map(|title| title.trim())
        .find(|title| !title.is_empty())
        .unwrap_or_else(|| row.title.trim());
    let title = if title.is_empty() {
        "this session"
    } else {
        title
    };
    match title.chars().count() > 60 {
        true => format!("{}…", title.chars().take(59).collect::<String>()),
        false => title.to_string(),
    }
}

fn threads(count: usize) -> String {
    match count {
        1 => "1 open thread".to_string(),
        count => format!("{count} open threads"),
    }
}

/// One line saying what closing this session stops.
fn reason(name: &str, busy: Busy) -> String {
    match busy {
        Busy::Coordinator { open_threads: 0 } => format!("{name} is a coordinator."),
        Busy::Coordinator { open_threads } => format!(
            "{name} and its {} close together. Restoring {name} from Previous Sessions brings them back.",
            threads(open_threads)
        ),
        Busy::Working => format!("{name} is in the middle of a turn, which stops."),
        Busy::Waiting => format!("{name} is waiting for your answer to a question or approval."),
        Busy::Background => format!("{name} still has background work running, which stops."),
    }
}

/// The question for closing `sessions` (every session the close takes, in order), or `None` when
/// none of them is busy and the close runs as it always has.
pub fn close_confirmation(core: &Core, sessions: &[SessionKey]) -> Option<CloseConfirmation> {
    let asking: Vec<(String, Busy)> = sessions
        .iter()
        .filter_map(|session| {
            let row = core.presentation().session(session)?;
            Some((name(&row), busy(core, session, &row)?))
        })
        .collect();
    let (first_name, first_busy) = asking.first()?.clone();
    const KEEP: &str = "Conversations stay in Previous Sessions, where you can restore them.";
    if sessions.len() == 1 {
        let title = match first_busy {
            Busy::Coordinator { open_threads } if open_threads > 0 => {
                format!("Close {first_name} and its {}?", threads(open_threads))
            }
            _ => format!("Close {first_name}?"),
        };
        let message = match first_busy {
            Busy::Coordinator { open_threads } if open_threads > 0 => {
                reason(&first_name, first_busy)
            }
            _ => format!(
                "{} Its conversation stays in Previous Sessions, where you can restore it.",
                reason(&first_name, first_busy)
            ),
        };
        return Some(CloseConfirmation {
            title,
            message,
            confirm_label: "Close".to_string(),
        });
    }
    let mut lines: Vec<String> = asking
        .iter()
        .take(3)
        .map(|(name, busy)| reason(name, *busy))
        .collect();
    if asking.len() > 3 {
        lines.push(format!("{} more are busy too.", asking.len() - 3));
    }
    lines.push(KEEP.to_string());
    Some(CloseConfirmation {
        title: format!("Close {} sessions?", sessions.len()),
        message: lines.join("\n"),
        confirm_label: "Close all".to_string(),
    })
}

/// Every session a sidebar close command closes, in order: a row's Close (`closeSession`), a
/// multi-selection's (`closeSessions`), a project's Close Inactive, and a menu `batch` of them.
/// Empty for any other command, and for a browser tab, which is not a session.
pub fn sidebar_close_targets(core: &Core, command: &Value) -> Vec<SessionKey> {
    let message = match text_field(command, "type") {
        Some("command") => command.get("message").unwrap_or(command),
        _ => command,
    };
    match text_field(message, "type") {
        Some("batch") => message
            .get("messages")
            .and_then(Value::as_array)
            .map(|messages| {
                messages
                    .iter()
                    .flat_map(|message| sidebar_close_targets(core, message))
                    .collect()
            })
            .unwrap_or_default(),
        Some("closeSession") => text_field(message, "sessionId")
            .filter(|id| !id.starts_with("gpui-browser:"))
            .and_then(SessionKey::parse_sidebar_session_id)
            .into_iter()
            .collect(),
        Some("closeSessions" | "closeInactiveProjectSessions") => plan_bulk_request(core, message)
            .map(|request| {
                request
                    .messages
                    .iter()
                    .flat_map(|message| sidebar_close_targets(core, message))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::MachineId;

    fn session(session_id: &str, extra: Value) -> Value {
        let mut session = json!({
            "sessionId": session_id,
            "projectId": "P1",
            "groupId": "P1:active",
            "kind": "agent",
            "surface": "workspace",
            "zmxName": format!("S90-P1-{session_id}"),
            "sortKey": format!("000000000000:0:1:{session_id}"),
            "visibleInSidebarByDefault": true,
            "alias": session_id,
            "activity": "idle",
            "lifecycleState": "running",
            "title": session_id,
            "displayTitle": session_id,
            "createdAt": "2026-11-02T01:00:00.000Z",
            "updatedAt": "2026-11-02T01:00:00.000Z",
        });
        for (key, value) in extra.as_object().expect("object") {
            session[key] = value.clone();
        }
        session
    }

    fn thread(session_id: &str, state: &str) -> Value {
        session(
            session_id,
            json!({
                "coordinatorRole": "thread",
                "coordinatorProjectId": "P1",
                "coordinatorSessionId": "Main Coordinator",
                "coordinatorThreadState": state,
            }),
        )
    }

    fn core() -> Core {
        let sessions = vec![
            session(
                "Main Coordinator",
                json!({ "coordinatorRole": "coordinator" }),
            ),
            thread("t-working", "working"),
            thread("t-finished", "finished"),
            thread("t-sleeping", "sleeping"),
            thread("t-done", "done"),
            session("Busy", json!({ "activity": "working" })),
            session("Asking", json!({ "pendingQuestionCount": 1 })),
            session("Idle", json!({})),
            session(
                "Shell",
                json!({ "kind": "terminal", "activity": "working" }),
            ),
        ];
        let ids: Vec<Value> = sessions
            .iter()
            .map(|row| row["sessionId"].clone())
            .collect();
        let snapshot = json!({
            "type": "presentationSnapshot",
            "protocolVersion": crate::protocol::GXSERVER_PROTOCOL_VERSION,
            "serverId": "local",
            "revision": 1,
            "snapshot": {
                "revision": 1,
                "generatedAt": "2026-11-02T01:00:00.000Z",
                "projects": [{
                    "projectId": "P1",
                    "title": "P1",
                    "path": "/tmp/P1",
                    "pathState": "available",
                    "groupIds": ["P1:active"],
                    "sortKey": "1:P1",
                    "createdAt": "2026-11-02T01:00:00.000Z",
                    "updatedAt": "2026-11-02T01:00:00.000Z",
                }],
                "groups": [{
                    "groupId": "P1:active",
                    "projectId": "P1",
                    "title": "Active",
                    "sessionIds": ids,
                    "sortKey": "1:P1:active",
                }],
                "sessions": sessions,
            }
        });
        let mut core = Core::new();
        core.handle_raw_frame(MachineId::Local, &snapshot.to_string(), 0)
            .expect("the frame parses");
        core
    }

    fn key(session_id: &str) -> SessionKey {
        SessionKey::local("P1", session_id)
    }

    #[test]
    fn a_coordinator_names_its_open_threads() {
        let asked = close_confirmation(&core(), &[key("Main Coordinator")]).expect("asks");
        assert_eq!(
            asked.title,
            "Close Main Coordinator and its 3 open threads?"
        );
        assert_eq!(asked.confirm_label, "Close");
    }

    #[test]
    fn a_busy_agent_asks_and_an_idle_one_or_a_terminal_does_not() {
        let core = core();
        assert_eq!(
            close_confirmation(&core, &[key("Busy")])
                .expect("asks")
                .title,
            "Close Busy?"
        );
        assert!(close_confirmation(&core, &[key("Asking")]).is_some());
        assert_eq!(close_confirmation(&core, &[key("Idle")]), None);
        assert_eq!(close_confirmation(&core, &[key("Shell")]), None);
    }

    #[test]
    fn several_sessions_ask_once_for_all() {
        let asked = close_confirmation(
            &core(),
            &[key("Idle"), key("Busy"), key("Main Coordinator")],
        )
        .expect("asks");
        assert_eq!(asked.title, "Close 3 sessions?");
        assert_eq!(asked.confirm_label, "Close all");
    }

    #[test]
    fn the_sidebar_close_doors_name_their_sessions() {
        let core = core();
        let row = key("Busy").to_sidebar_session_id();
        assert_eq!(
            sidebar_close_targets(
                &core,
                &json!({ "type": "command", "message": { "type": "closeSession", "sessionId": row } })
            ),
            vec![key("Busy")]
        );
        assert_eq!(
            sidebar_close_targets(
                &core,
                &json!({ "type": "command", "message": { "type": "setSessionPinned", "sessionId": row } })
            ),
            Vec::<SessionKey>::new()
        );
    }

    #[test]
    fn a_close_names_its_door_for_the_close_log() {
        let core = core();
        let row = key("Busy").to_sidebar_session_id();
        let plan = |message: Value| {
            super::super::close::plan_close_request(&core, &message)
                .expect("a close")
                .rpc_params["closeSource"]
                .clone()
        };
        assert_eq!(
            plan(
                json!({ "type": "closeSession", "sessionId": row, "closeSource": "sidebarRowButton" })
            ),
            json!("desktop.sidebarRowButton")
        );
        assert_eq!(
            plan(json!({ "type": "closeSession", "sessionId": row })),
            json!("desktop.sidebar")
        );
        let below = super::super::bulk::plan_bulk_request(
            &core,
            &json!({ "type": "closeSessions", "sessionIds": [row], "closeSource": "closeBelow" }),
        )
        .expect("a bulk close");
        assert_eq!(below.messages[0]["closeSource"], json!("closeBelow"));
    }
}
