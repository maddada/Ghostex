//! Sessions on connected computers (`computers.rs`): reading another computer's rows, one merged
//! waiting-first list, names and refs that tell two computers apart, and the routes a tool takes.
//! The end-to-end check through a real relay is `server/tests/voice_remote_sessions.rs`.

use serde_json::{json, Value};

use super::computers::{self, Computer, Unavailable};
use super::sessions::{self, Match, ProjectInfo, SessionInfo, ThreadParent, VoiceState};

fn omarchy() -> Computer {
    Computer {
        server_id: "S9r".to_string(),
        name: "Omarchy".to_string(),
    }
}

fn session(project: &str, id: &str, title: &str, state: VoiceState) -> SessionInfo {
    SessionInfo {
        project_id: project.to_string(),
        session_id: id.to_string(),
        title: title.to_string(),
        project: "Ghostex".to_string(),
        agent: "Claude".to_string(),
        state,
        waiting: None,
        last_active_at: String::new(),
        turn_event: None,
        has_chat: true,
        thread_of: None,
        threads: None,
        computer: None,
    }
}

fn remote(project: &str, id: &str, title: &str, state: VoiceState) -> SessionInfo {
    SessionInfo {
        computer: Some(omarchy()),
        ..session(project, id, title, state)
    }
}

fn titles(rows: &[SessionInfo]) -> Vec<&str> {
    rows.iter().map(|row| row.title.as_str()).collect()
}

#[test]
fn another_computers_rows_carry_its_name_and_a_global_ref() {
    let rows = json!({"sessions": [
        {"projectId": "P1", "sessionId": "G1", "title": "Linux build fix", "agentId": "claude", "lifecycleState": "running"},
        {"projectId": "P1", "sessionId": "G2", "title": "Old one", "agentId": "codex", "lifecycleState": "stopped"},
        {"projectId": "P1", "sessionId": "G3", "title": "Napping", "agentId": "codex", "lifecycleState": "sleeping"},
    ]});
    let projects =
        json!({"projects": [{"projectId": "P1", "name": "Ghostex", "path": "/home/me/Ghostex"}]});
    let (rows, projects) = computers::read_listing_for_tests(&omarchy(), &rows, &projects);

    assert_eq!(
        titles(&rows),
        ["Linux build fix", "Napping"],
        "a stopped row is left out"
    );
    let build = &rows[0];
    assert_eq!(build.reference(), "S9r:P1:G1");
    assert_eq!(build.project, "Ghostex");
    assert_eq!(rows[1].state, VoiceState::Sleeping);
    let listed = build.to_tool_value();
    assert_eq!(listed["computer"], "Omarchy");
    assert_eq!(listed["ref"], "S9r:P1:G1");
    assert_eq!(build.brief()["computer"], "Omarchy");
    assert_eq!(build.spoken_title(), "\"Linux build fix\" on Omarchy");
    assert_eq!(projects[0].reference(), "S9r:P1");
    assert!(projects[0].holds(build));
    assert!(!projects[0].holds(&session("P1", "G1", "Here", VoiceState::Working)));

    let here = session("P1", "G1", "Here", VoiceState::Working);
    assert_eq!(here.reference(), "P1:G1");
    assert!(here.to_tool_value().get("computer").is_none());
    assert_eq!(here.spoken_title(), "\"Here\"");
}

#[test]
fn one_list_puts_waiting_sessions_first_and_keeps_threads_under_their_coordinator() {
    let mut main = session("P1", "Main", "Main Coordinator", VoiceState::Finished);
    main.threads = Some(1);
    let mut thread = session("P1", "T1", "Voice thread", VoiceState::Working);
    thread.thread_of = Some(ThreadParent {
        reference: "P1:Main".to_string(),
        title: "Main Coordinator".to_string(),
        resolved: false,
    });
    let local = vec![
        main,
        thread,
        session("P2", "Z", "Sleepy", VoiceState::Sleeping),
    ];
    let elsewhere = vec![
        remote("P1", "A", "Needs approval", VoiceState::WaitingOnApproval),
        remote("P1", "B", "Remote build", VoiceState::Working),
    ];
    assert_eq!(
        titles(&sessions::merge(local, elsewhere)),
        [
            "Needs approval",
            "Main Coordinator",
            "Voice thread",
            "Remote build",
            "Sleepy"
        ]
    );
}

#[test]
fn refs_and_the_computers_name_tell_two_computers_sessions_apart() {
    // Two computers can hold the same project and session ids.
    let all = vec![
        session("P1", "G1", "Sidebar fix", VoiceState::Working),
        remote("P1", "G1", "Sidebar fix", VoiceState::Working),
    ];
    let one = |query: &str| match sessions::match_session(&all, query) {
        Match::One(session) => session.reference(),
        Match::Ambiguous(found) => format!("ambiguous: {}", found.len()),
        Match::None => "none".to_string(),
    };
    assert_eq!(one("S9r:P1:G1"), "S9r:P1:G1");
    assert_eq!(one("P1:G1"), "P1:G1");
    assert_eq!(one("the sidebar fix on Omarchy"), "S9r:P1:G1");
    assert_eq!(one("sidebar fix"), "ambiguous: 2", "never a guess");
}

#[test]
fn a_project_on_another_computer_is_picked_by_its_ref_or_the_computers_name() {
    let here = ProjectInfo {
        project_id: "P1".to_string(),
        name: "Ghostex".to_string(),
        path: "/Users/me/dev/Ghostex".to_string(),
        computer: None,
    };
    let there = ProjectInfo {
        project_id: "P7".to_string(),
        name: "Ghostex".to_string(),
        path: "/Users/me/dev/Ghostex".to_string(),
        computer: Some(omarchy()),
    };
    let projects = vec![here, there];
    let one = |query: &str| match sessions::match_project(&projects, query) {
        Match::One(project) => project.reference(),
        Match::Ambiguous(found) => format!("ambiguous: {}", found.len()),
        Match::None => "none".to_string(),
    };
    assert_eq!(one("Ghostex"), "ambiguous: 2");
    assert_eq!(one("Ghostex on Omarchy"), "S9r:P7");
    assert_eq!(one("S9r:P7"), "S9r:P7");
    assert_eq!(
        one("/Users/me/dev/Ghostex"),
        "P1",
        "the same folder means this computer's"
    );
}

#[test]
fn every_tool_request_to_another_computer_goes_through_the_relay_and_its_allowlist() {
    let computer = omarchy();
    assert_eq!(
        computers::route(None, "/api/setSessionChatDraft").unwrap(),
        "/api/setSessionChatDraft"
    );
    // What the voice tools send to a session on another computer: all of them open to agents.
    for path in [
        "/api/listSessions",
        "/api/listProjects",
        "/api/readSessionChat",
        "/api/sendSessionChatMessage",
        "/api/sendSessionText",
        "/api/sendSessionEnter",
        "/api/answerSessionChatPrompt",
        "/api/interruptSessionChat",
        "/api/readSidebarHud",
        "/api/createAgentSession",
        "/api/startSessionProvider",
    ] {
        assert_eq!(
            computers::route(Some(&computer), path).unwrap(),
            format!("/api/computers/S9r/relay{path}")
        );
    }
    // What only this computer does: refused before anything is sent, in words the model can say.
    for path in [
        "/api/setSessionChatDraft",
        "/api/focusSession",
        "/api/dispatchRendererCommand",
        "/api/restoreRecentProject",
    ] {
        let refused = computers::route(Some(&computer), path).unwrap_err();
        assert!(refused.contains("Omarchy"), "{path}: {refused}");
    }
}

#[test]
fn a_missed_name_says_which_computers_could_not_be_read() {
    let unavailable = vec![Unavailable {
        computer: "Omarchy".to_string(),
        reason: "Agent access to Omarchy is off.".to_string(),
    }];
    let missed = format!("{} \"the linux one\".", super::tools::NO_MATCH);
    let said = computers::with_unreachable(missed.clone(), &unavailable);
    assert!(said.starts_with(&missed));
    assert!(said.contains("Agent access to Omarchy is off."), "{said}");
    assert_eq!(
        computers::with_unreachable("Ambiguous: two.".to_string(), &unavailable),
        "Ambiguous: two."
    );
    assert_eq!(computers::with_unreachable(missed.clone(), &[]), missed);
}

#[test]
fn a_shown_session_on_another_computer_names_it_for_the_desktop() {
    let output = json!({"ok": true, "session": {"ref": "S9r:P1:G1", "title": "Linux build fix", "computer": "Omarchy"}});
    let shown = super::show::shown_session("send_prompt", &output).expect("shown");
    assert_eq!(
        shown,
        json!({"projectId": "P1", "sessionId": "G1", "title": "Linux build fix", "serverId": "S9r", "computer": "Omarchy"})
    );
    let started = json!({"ok": true, "ref": "S9r:P1:G2", "agent": "Claude", "project": "Ghostex", "computer": "Omarchy"});
    let shown = super::show::shown_session("start_agent", &started).expect("shown");
    assert_eq!(shown["serverId"], "S9r");
    assert_eq!(shown["title"], "Claude in Ghostex");
    let here = json!({"ok": true, "session": {"ref": "P1:G1", "title": "Here"}});
    let shown: Value = super::show::shown_session("read_session", &here).expect("shown");
    assert!(shown.get("serverId").is_none());
}
