//! Claude cloud sessions: the rules every client shares about a session whose agent runs on
//! claude.ai's computers (server/src/claude_cloud/ owns the sessions).
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User picked a separate "Claude (cloud)" entry in the agent list. The phone shows cloud rows
//! too, tap opening the session's claude.ai page (confirmed, built after the computer part), so
//! these rules stay client-neutral.
//!
//! CDXC:ClaudeCloud 2026-10-29 DECISION:
//! User: "also no need for this machine/cloud since I can just pick cloud claude there in agents
//! list". The New Thread picker's Run on row no longer offers Claude cloud as a place (it did from
//! 2026-10-23, when the user had asked for it next to This computer and the Cloud Boxes, lit by
//! picking Claude (cloud) and dimmed by every other agent); "Claude (cloud)" in the agent list is
//! the one way to start a cloud session, and the row is back to This computer and the boxes,
//! shown only with agentbox installed. Its icon is an orange cloud (gx-core
//! sidebar_menu/agent_logos.rs).
//!
//! SEE-ALSO: server/src/claude_cloud/ (the sessions), apps/desktop/src/app/window/new_thread_picker_run_on.rs
//! (the picker's Run on row), apps/desktop/src/app/native_sidebar/claude_cloud.rs (the row).

use crate::core::Core;
use crate::keys::SessionKey;

/// The built-in agent that starts a Claude cloud session.
pub const CLAUDE_CLOUD_AGENT_ID: &str = "claude-cloud";

/// The `runLocation` of a Claude cloud session.
pub const CLAUDE_CLOUD_RUN_LOCATION: &str = "claude-cloud";

/// The Run on row's one line about this computer, its This computer chip's tooltip.
pub const THIS_COMPUTER_DESCRIPTION: &str = "Any agent. Stops when this computer sleeps.";

/// Whether a launcher agent is the Claude (cloud) agent.
pub fn is_claude_cloud_agent(agent_id: &str) -> bool {
    agent_id.trim() == CLAUDE_CLOUD_AGENT_ID
}

/// Whether a `runLocation` names Claude cloud.
pub fn is_claude_cloud_run_location(run_location: Option<&str>) -> bool {
    run_location.map(str::trim) == Some(CLAUDE_CLOUD_RUN_LOCATION)
}

/// The claude.ai page of a Claude cloud session: set once the session exists on claude.ai and
/// until it is brought to this computer. `None` for every other session, and for a cloud session
/// still waiting for its task or starting. The desktop shows it in the session's chat pane; a
/// client that cannot embed claude.ai (the web build) opens it in a browser tab on a row click.
///
/// CDXC:ClaudeCloud 2026-10-26 DECISION:
/// User: "so it should open terminal in bg but show me chat view where i can type the first message to send. once i send it i wanna see the actual browser appear on the chat pane! please implement it this way (we need to link a sidebar thread with an actual link to the cloud conversation that was started". A cloud session opens in Chat View with its terminal running behind it; the first message starts it, and from then on the desktop's chat pane is the session's claude.ai page (app/claude_cloud_chat_page.rs), signed in as the session's account. The row keeps the link (`url` in its record): Open on claude.ai and Copy claude.ai Link use it. Supersedes the 2026-10-23 decision that a row click opened the page in Ghostex's Browser.
pub fn session_claude_cloud_page(core: &Core, session: &SessionKey) -> Option<String> {
    let row = core.presentation().session(session)?;
    let cloud = row.claude_cloud.as_ref()?;
    (cloud.state == "running")
        .then(|| cloud.url.clone())
        .flatten()
        .filter(|url| url.starts_with("https://claude.ai/code/"))
}

/// Whether a session is a Claude cloud session (any state).
pub fn is_claude_cloud_session(core: &Core, session: &SessionKey) -> bool {
    core.presentation()
        .session(session)
        .is_some_and(|session| session.claude_cloud.is_some())
}

/// Whether Close on this row puts the session to sleep instead of closing it: an awake cloud
/// session that exists (or is being created) on claude.ai. gxserver's rule, read from the row
/// (server/src/claude_cloud/record.rs `close_keeps_session_asleep`, which holds the user's DECISION:
/// "Keep it listed, asleep").
pub fn close_keeps_claude_cloud_row(core: &Core, session: &SessionKey) -> bool {
    core.presentation().session(session).is_some_and(|row| {
        row.lifecycle_state == ghostex_gx_protocol::LifecycleState::Running
            && row
                .claude_cloud
                .as_ref()
                .is_some_and(|cloud| matches!(cloud.state.as_str(), "starting" | "running"))
    })
}

/// The cloud session a claude.ai page belongs to on this computer, and the saved Claude account it
/// runs on (`None` inside: the current CLI login). `None` for a page that is not one of them.
pub fn claude_cloud_page_account(core: &Core, url: &str) -> Option<Option<String>> {
    let page = claude_cloud_session_link(url)?;
    core.presentation()
        .loaded(&crate::MachineId::Local)?
        .server_sessions()
        .filter_map(|session| session.claude_cloud.as_ref())
        .find(|cloud| {
            cloud
                .url
                .as_deref()
                .and_then(claude_cloud_session_link)
                .is_some_and(|link| link == page)
        })
        .map(|cloud| cloud.account_id.clone())
}

/// Whether a claude.ai page is the page of a cloud session that is running on claude.ai right now.
pub fn is_running_claude_cloud_page(core: &Core, url: &str) -> bool {
    let Some(page) = claude_cloud_session_link(url) else {
        return false;
    };
    core.presentation()
        .loaded(&crate::MachineId::Local)
        .is_some_and(|loaded| {
            loaded
                .server_sessions()
                .filter_map(|session| session.claude_cloud.as_ref())
                .any(|cloud| {
                    cloud.state == "running"
                        && cloud
                            .url
                            .as_deref()
                            .and_then(claude_cloud_session_link)
                            .is_some_and(|link| link == page)
                })
        })
}

/// `https://claude.ai/code/session_…` without its query or fragment, for any link into one.
pub fn claude_cloud_session_link(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://claude.ai/code/")?;
    let id: String = rest
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect();
    (id.starts_with("session_") || id.starts_with("cse_"))
        .then(|| format!("https://claude.ai/code/{id}"))
}

/// The account of the project's newest cloud session, if it had one: what a new cloud session's
/// account list highlights first.
///
/// CDXC:ClaudeCloud 2026-10-23 DECISION:
/// User picked "the account of this project's last cloud session" for the account New session highlights first for a cloud session, falling back to the rule local sessions use when the project never had one (a repo is often set up for the cloud on one account only). gxserver applies the same order when a create names no account (server/src/claude_cloud/create.rs `pick_claude_account`).
pub fn project_last_cloud_account(core: &Core, project_id: &str) -> Option<String> {
    core.presentation()
        .loaded(&crate::MachineId::Local)?
        .server_sessions()
        .filter(|session| session.project_id == project_id)
        .filter_map(|session| {
            let account = session.claude_cloud.as_ref()?.account_id.clone()?;
            Some((session.created_at.clone(), account))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, account)| account)
}

/// What a row of the header's Claude cloud menu does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeCloudMenuPick {
    /// The session's terminal, where Ghostex's cloud client runs.
    ShowTerminal,
    /// Back to the chat pane: the composer before the start, the claude.ai page after it.
    ShowChat,
    /// The session's claude.ai page in a Browser tab.
    OpenOnClaudeAi,
    CopyLink,
    /// Run the session on this saved Claude account.
    Account(String),
    CopyOnAnotherAccount,
    BringBack,
}

/// One row of the header's Claude cloud menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeCloudMenuRow {
    pub label: String,
    pub pick: ClaudeCloudMenuPick,
    pub checked: bool,
    pub disabled: bool,
}

/// One entry of the header's Claude cloud menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeCloudMenuEntry {
    Row(ClaudeCloudMenuRow),
    Separator,
    /// "Account: <name>" and the saved Claude accounts to run on.
    Accounts {
        label: String,
        rows: Vec<ClaudeCloudMenuRow>,
    },
}

/// A saved Claude account the menu offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeCloudMenuAccount {
    pub id: String,
    pub name: String,
    /// Signed in and usable right now.
    pub ready: bool,
}

fn menu_row(label: &str, pick: ClaudeCloudMenuPick, disabled: bool) -> ClaudeCloudMenuEntry {
    ClaudeCloudMenuEntry::Row(ClaudeCloudMenuRow {
        label: label.to_string(),
        pick,
        checked: false,
        disabled,
    })
}

/// The header's Claude cloud menu for a cloud session: what the session is on screen as
/// (`showing_terminal`), its claude.ai page, the account it runs on and the rest of what only a
/// cloud session does. Nothing in it is offered for other sessions.
///
/// CDXC:ClaudeCloud 2026-10-27 DECISION:
/// User (Q633), asked where the switch to the terminal sits once the claude.ai page fills the chat pane: "menu", "but make this a new button in the header above the chat that has just stuff related to claude cloud thingy pls". A cloud session gets a Claude cloud button in the work area header whose menu holds only cloud actions: the terminal and back, the page on claude.ai, its link, the account it runs on (switchable until it starts, CDXC:ClaudeCloud in server/src/claude_cloud/account_pick.rs), a copy on another account while that one is out of usage, and Bring to This Computer. claude.ai's own composer takes follow-ups once the page shows (Q634, "claude.ai composer").
pub fn claude_cloud_menu(
    cloud: &ghostex_gx_protocol::PresentationClaudeCloud,
    showing_terminal: bool,
    accounts: &[ClaudeCloudMenuAccount],
) -> Vec<ClaudeCloudMenuEntry> {
    let has_page = cloud.url.is_some();
    let mut menu = vec![if showing_terminal {
        menu_row(
            if has_page {
                "Show claude.ai Page"
            } else {
                "Show Chat"
            },
            ClaudeCloudMenuPick::ShowChat,
            false,
        )
    } else {
        menu_row("Show Terminal", ClaudeCloudMenuPick::ShowTerminal, false)
    }];
    menu.push(ClaudeCloudMenuEntry::Separator);
    menu.push(menu_row(
        "Open on claude.ai",
        ClaudeCloudMenuPick::OpenOnClaudeAi,
        !has_page,
    ));
    menu.push(menu_row(
        "Copy claude.ai Link",
        ClaudeCloudMenuPick::CopyLink,
        !has_page,
    ));
    menu.push(ClaudeCloudMenuEntry::Separator);
    let can_switch = matches!(cloud.state.as_str(), "draft" | "failed");
    let current = cloud.account_id.as_deref();
    let name = cloud
        .account_name
        .clone()
        .or_else(|| {
            accounts
                .iter()
                .find(|account| Some(account.id.as_str()) == current)
                .map(|account| account.name.clone())
        })
        .unwrap_or_else(|| "Claude's own login".to_string());
    menu.push(ClaudeCloudMenuEntry::Accounts {
        label: format!("Account: {name}"),
        rows: accounts
            .iter()
            .map(|account| {
                let checked = Some(account.id.as_str()) == current;
                ClaudeCloudMenuRow {
                    label: account.name.clone(),
                    pick: ClaudeCloudMenuPick::Account(account.id.clone()),
                    checked,
                    disabled: checked || !can_switch || !account.ready,
                }
            })
            .collect(),
    });
    if cloud.limit_notice.is_some() && cloud.state == "running" {
        menu.push(menu_row(
            "Start a Copy on Another Account…",
            ClaudeCloudMenuPick::CopyOnAnotherAccount,
            false,
        ));
    }
    menu.push(ClaudeCloudMenuEntry::Separator);
    menu.push(menu_row(
        "Bring to This Computer",
        ClaudeCloudMenuPick::BringBack,
        !has_page || cloud.state == "moved",
    ));
    menu
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::{close_as_claude_cloud_sleep, Core, MachineId, SessionKey};

    /// One local project whose rows are the given `(session id, lifecycle, cloud state)`.
    fn core(rows: &[(&str, &str, Option<&str>)]) -> Core {
        let sessions: Vec<Value> = rows
            .iter()
            .map(|(session_id, lifecycle, cloud)| {
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
                    "lifecycleState": lifecycle,
                    "createdAt": "2026-10-23T01:00:00.000Z",
                    "updatedAt": "2026-10-23T01:00:00.000Z",
                });
                if let Some(state) = cloud {
                    session["claudeCloud"] = json!({
                        "state": state,
                        "url": "https://claude.ai/code/session_01Test",
                    });
                }
                session
            })
            .collect();
        let snapshot = json!({
            "type": "presentationSnapshot",
            "protocolVersion": crate::protocol::GXSERVER_PROTOCOL_VERSION,
            "serverId": "local",
            "revision": 1,
            "snapshot": {
                "revision": 1,
                "generatedAt": "2026-10-23T01:00:00.000Z",
                "projects": [{
                    "projectId": "P1",
                    "title": "P1",
                    "path": "/tmp/P1",
                    "pathState": "available",
                    "groupIds": ["P1:active"],
                    "sortKey": "1:P1",
                    "createdAt": "2026-10-23T01:00:00.000Z",
                    "updatedAt": "2026-10-23T01:00:00.000Z",
                }],
                "groups": [{
                    "groupId": "P1:active",
                    "projectId": "P1",
                    "title": "Active",
                    "sessionIds": rows.iter().map(|row| row.0).collect::<Vec<_>>(),
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

    #[test]
    fn a_cloud_session_keeps_chat_view_where_its_page_can_show() {
        let core = core(&[
            ("D", "running", Some("draft")),
            ("R", "running", Some("running")),
            ("M", "sleeping", Some("moved")),
            ("L", "running", None),
        ]);
        let key = |id: &str| SessionKey::local("P1", id);
        // The web build and the phone: chat takes the first message, the terminal shows the rest.
        assert!(!crate::session_chat_view_unavailable(&core, &key("D")));
        assert!(crate::session_chat_view_unavailable(&core, &key("R")));
        assert!(crate::session_chat_view_unavailable(&core, &key("M")));
        // The desktop shows the claude.ai page in the chat pane.
        for id in ["D", "R", "M", "L"] {
            assert!(
                !crate::session_chat_view_unavailable_showing_cloud_pages(&core, &key(id)),
                "{id}"
            );
        }
    }

    #[test]
    fn a_cloud_session_opens_in_chat_whatever_the_default_view() {
        let settings = crate::PreferredInterfaceSettings {
            default_interface: "terminal".to_string(),
            overrides: vec![],
        };
        assert_eq!(settings.resolve(Some("claude-cloud")), Some("chat"));
        assert_eq!(settings.resolve(Some("claude")), Some("terminal"));
        let chosen = crate::PreferredInterfaceSettings {
            default_interface: "chat".to_string(),
            overrides: vec![("claude-cloud".to_string(), "terminal".to_string())],
        };
        assert_eq!(chosen.resolve(Some("claude-cloud")), Some("terminal"));
    }

    fn sidebar_id(session_id: &str) -> String {
        SessionKey::local("P1", session_id).to_sidebar_session_id()
    }

    fn close(core: &Core, session_id: &str) -> Option<Value> {
        close_as_claude_cloud_sleep(
            core,
            &json!({ "type": "closeSession", "sessionId": sidebar_id(session_id) }),
        )
    }

    #[test]
    fn close_sleeps_an_awake_cloud_row_that_lives_on_claude_ai() {
        let core = core(&[
            ("G1run", "running", Some("running")),
            ("G2sta", "running", Some("starting")),
        ]);
        for session_id in ["G1run", "G2sta"] {
            assert_eq!(
                close(&core, session_id),
                Some(json!({
                    "type": "setSessionSleeping",
                    "sessionId": sidebar_id(session_id),
                    "sleeping": true,
                })),
                "{session_id}"
            );
        }
    }

    #[test]
    fn close_closes_every_other_row() {
        let core = core(&[
            ("G1slp", "sleeping", Some("running")),
            ("G2drf", "running", Some("draft")),
            ("G3fai", "running", Some("failed")),
            ("G4mov", "running", Some("moved")),
            ("G5loc", "running", None),
        ]);
        for session_id in ["G1slp", "G2drf", "G3fai", "G4mov", "G5loc"] {
            assert_eq!(close(&core, session_id), None, "{session_id}");
        }
        assert_eq!(
            close_as_claude_cloud_sleep(
                &core,
                &json!({ "type": "setSessionSleeping", "sessionId": sidebar_id("G1run"), "sleeping": true }),
            ),
            None
        );
    }

    fn cloud(state: &str, url: Option<&str>) -> ghostex_gx_protocol::PresentationClaudeCloud {
        ghostex_gx_protocol::PresentationClaudeCloud {
            state: state.to_string(),
            url: url.map(str::to_string),
            account_id: Some("a".to_string()),
            account_name: Some("ST".to_string()),
            ..Default::default()
        }
    }

    fn accounts() -> Vec<crate::ClaudeCloudMenuAccount> {
        ["a", "b"]
            .into_iter()
            .map(|id| crate::ClaudeCloudMenuAccount {
                id: id.to_string(),
                name: id.to_uppercase(),
                ready: true,
            })
            .collect()
    }

    fn rows(menu: &[crate::ClaudeCloudMenuEntry]) -> Vec<(String, bool, bool)> {
        menu.iter()
            .flat_map(|entry| match entry {
                crate::ClaudeCloudMenuEntry::Row(row) => vec![row.clone()],
                crate::ClaudeCloudMenuEntry::Accounts { rows, .. } => rows.clone(),
                crate::ClaudeCloudMenuEntry::Separator => Vec::new(),
            })
            .map(|row| (row.label, row.checked, row.disabled))
            .collect()
    }

    #[test]
    fn a_draft_switches_account_and_has_no_page_yet() {
        let menu = crate::claude_cloud_menu(&cloud("draft", None), false, &accounts());
        assert_eq!(
            rows(&menu),
            vec![
                ("Show Terminal".to_string(), false, false),
                ("Open on claude.ai".to_string(), false, true),
                ("Copy claude.ai Link".to_string(), false, true),
                ("A".to_string(), true, true),
                ("B".to_string(), false, false),
                ("Bring to This Computer".to_string(), false, true),
            ]
        );
        assert!(menu.iter().any(|entry| matches!(
            entry,
            crate::ClaudeCloudMenuEntry::Accounts { label, .. } if label == "Account: ST"
        )));
    }

    #[test]
    fn a_running_session_keeps_its_account_and_offers_its_page() {
        let url = "https://claude.ai/code/session_01";
        let mut running = cloud("running", Some(url));
        let menu = crate::claude_cloud_menu(&running, true, &accounts());
        let rows = rows(&menu);
        assert_eq!(rows[0], ("Show claude.ai Page".to_string(), false, false));
        assert!(rows.contains(&("Open on claude.ai".to_string(), false, false)));
        assert!(rows.contains(&("B".to_string(), false, true)));
        assert!(!rows.iter().any(|row| row.0.starts_with("Start a Copy")));
        running.limit_notice = Some("ST hit its 5h limit".to_string());
        let menu = crate::claude_cloud_menu(&running, true, &accounts());
        assert!(super::tests::rows(&menu).contains(&(
            "Start a Copy on Another Account…".to_string(),
            false,
            false
        )));
    }

    #[test]
    fn claude_cloud_draws_an_orange_cloud_and_claude_keeps_its_logo() {
        let cloud = crate::colored_agent_logo("claude-cloud").expect("the cloud logo");
        let claude = crate::colored_agent_logo("claude").expect("the Claude logo");
        assert_ne!(cloud, claude);
        assert!(cloud.contains("%23d67556"));
        assert!(claude.contains("%23d97757"));
        assert!(
            crate::agent_logo_svg("claude-cloud").is_some_and(|svg| svg.contains("M10.04 4.305"))
        );
    }
}
