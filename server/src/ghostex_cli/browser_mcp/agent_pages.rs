//! Which embedded page the browser tools drive for an agent: one of its own project's Browser
//! pages, never another project's.
//!
//! CDXC:Browser 2026-10-31 WHY:
//! Without a page id the tools used the first page of the DevTools endpoint's list, which holds every page of every project and puts the one the user last brought up first, and the bridge is started afresh for most calls, so it remembered nothing: an agent drove the user's tab, and lost its own each time the user switched tabs (a Discord report, 2026-10-27). The app says which pages are the agent's project's (`listBrowserPages`), and the bridge picks among those only: the page this bridge already uses, the one this agent chose or used last (kept on disk per agent session, since a new bridge starts for most calls), the one it opened last with `ghostex browser open`, then the page in front of its project. An agent with no page of its own gets an error saying so, never someone else's tab. An explicit page id still reaches any page.
//! SEE-ALSO: apps/desktop/src/app/browser_agent_pages.rs (the list), command.rs (`browser open` forgets the remembered page, so the page it opens is the one used next).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::ghostex_cli::args::Flags;
use crate::ghostex_cli::rpc::{self, CliError, CliResult};

/// How long a remembered page is kept for an agent session that stops asking.
const REMEMBERED_PAGE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// The agent the bridge works for and the project it belongs to.
#[derive(Clone, Debug, Default)]
pub(super) struct AgentScope {
    /// `--project-id`, else none.
    pub(super) project_id: Option<String>,
    /// `--project-path`, else the bridge's folder, as `ghostex browser open` defaults to.
    pub(super) project_path: Option<String>,
    /// The project of the agent's own session, used when its folder is not a project.
    pub(super) session_project_id: Option<String>,
    /// The agent session (`GHOSTEX_GLOBAL_SESSION_REF`, else `GHOSTEX_SESSION_ID`).
    pub(super) agent_session: Option<String>,
}

impl AgentScope {
    pub(super) fn from_env(project_id: Option<String>, project_path: Option<String>) -> Self {
        let read = |key: &str| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let global_ref = read("GHOSTEX_GLOBAL_SESSION_REF");
        let session_project_id = global_ref.as_deref().and_then(|reference| {
            ghostex_gx_protocol::session_ref::parse_global_session_ref(reference)
                .map(|parts| parts.project_id.to_string())
        });
        let project_path = project_path.or_else(|| {
            project_id.is_none().then(|| {
                std::env::current_dir()
                    .map(|dir| dir.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
        });
        AgentScope {
            project_id,
            project_path: project_path.filter(|path| !path.is_empty()),
            session_project_id,
            agent_session: global_ref.or_else(|| read("GHOSTEX_SESSION_ID")),
        }
    }
}

/// The agent session `ghostex browser open` runs in, sent with the open so the app knows whose
/// page it opened.
pub(crate) fn browser_agent_session_from_env() -> Option<String> {
    AgentScope::from_env(None, Some(String::new())).agent_session
}

/// One page of the agent's project, as the app lists it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct OwnPage {
    pub(super) target_id: Option<String>,
    pub(super) url: String,
    pub(super) title: String,
    pub(super) front: bool,
    pub(super) agent_page: bool,
}

/// The agent's project and its Browser pages.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ProjectPages {
    pub(super) project_id: String,
    pub(super) pages: Vec<OwnPage>,
}

impl ProjectPages {
    pub(super) fn from_answer(answer: &Value) -> Self {
        let text = |page: &Value, key: &str| {
            page.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let flag = |page: &Value, key: &str| page.get(key).and_then(Value::as_bool) == Some(true);
        ProjectPages {
            project_id: text(answer, "projectId"),
            pages: answer
                .get("pages")
                .and_then(Value::as_array)
                .map(|pages| {
                    pages
                        .iter()
                        .map(|page| OwnPage {
                            target_id: page
                                .get("targetId")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            url: text(page, "url"),
                            title: text(page, "title"),
                            front: flag(page, "front"),
                            agent_page: flag(page, "agentPage"),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub(super) fn owns(&self, target_id: &str) -> bool {
        self.pages
            .iter()
            .any(|page| page.target_id.as_deref() == Some(target_id))
    }

    /// The agent's opened page is listed but has not told the app its DevTools id yet (it was
    /// made a moment ago).
    fn agent_page_still_starting(&self) -> bool {
        self.pages
            .iter()
            .any(|page| page.agent_page && page.target_id.is_none())
    }
}

/// What the bridge knows about the agent's pages: its project's, or nothing because the agent is
/// in no project this Ghostex knows.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum AgentPages {
    Project(ProjectPages),
    NoProject,
}

/// Asks the app for the agent's project's pages. A page the agent has just opened is waited for
/// (briefly) until it can be named.
pub(super) fn fetch_agent_pages(scope: &AgentScope) -> CliResult<AgentPages> {
    let mut answer = ask_for_project_pages(scope)?;
    for _ in 0..10 {
        match &answer {
            AgentPages::Project(project) if project.agent_page_still_starting() => {
                std::thread::sleep(std::time::Duration::from_millis(150));
                answer = ask_for_project_pages(scope)?;
            }
            _ => break,
        }
    }
    Ok(answer)
}

fn ask_for_project_pages(scope: &AgentScope) -> CliResult<AgentPages> {
    let ask = |project: Value| -> CliResult<Option<ProjectPages>> {
        let mut payload = project;
        if let Some(session) = &scope.agent_session {
            payload["agentSessionId"] = json!(session);
        }
        match crate::ghostex_cli::actions::send_gxserver_cli_action(
            "listBrowserPages",
            &payload,
            &Flags::default(),
        ) {
            Ok(answer) => Ok(Some(ProjectPages::from_answer(&answer))),
            Err(error) if error.to_string().contains("No matching project") => Ok(None),
            Err(error) => Err(error),
        }
    };
    let named = match (&scope.project_id, &scope.project_path) {
        (Some(project_id), _) => ask(json!({ "projectId": project_id }))?,
        (None, Some(project_path)) => ask(json!({ "projectPath": project_path }))?,
        (None, None) => None,
    };
    if let Some(pages) = named {
        return Ok(AgentPages::Project(pages));
    }
    // A folder that is not a project (a session's own worktree, a subfolder): the agent's session
    // still belongs to one.
    if scope.project_id.is_none() {
        if let Some(project_id) = &scope.session_project_id {
            if let Some(pages) = ask(json!({ "projectId": project_id }))? {
                return Ok(AgentPages::Project(pages));
            }
        }
    }
    Ok(AgentPages::NoProject)
}

/// The page to drive when the call names none: the bridge's own choice and the agent's remembered
/// page while the DevTools endpoint still lists them (`live`; each was the agent's own pick or one
/// of its project's pages), then the page it opened, then its project's front page, each only
/// while it is one of the project's pages still listed. `None` when the project has none.
pub(super) fn choose_agent_page(
    live: &[&str],
    project: &ProjectPages,
    in_process: Option<&str>,
    remembered: Option<&str>,
) -> Option<String> {
    let usable = |target_id: &str| project.owns(target_id) && live.contains(&target_id);
    let listed = |pick: fn(&OwnPage) -> bool| {
        project
            .pages
            .iter()
            .filter(|page| pick(page))
            .filter_map(|page| page.target_id.as_deref())
            .find(|target_id| usable(target_id))
    };
    in_process
        .filter(|target_id| live.contains(target_id))
        .or_else(|| remembered.filter(|target_id| live.contains(target_id)))
        .or_else(|| listed(|page| page.agent_page))
        .or_else(|| listed(|page| page.front))
        .or_else(|| listed(|_| true))
        .map(str::to_string)
}

/// The error for a call that names no page when the agent's project has none to drive.
pub(super) fn no_agent_page_error(agent_pages: &AgentPages, scope: &AgentScope) -> CliError {
    match agent_pages {
        AgentPages::Project(project) => CliError::Other(format!(
            "No open Browser page belongs to this agent's project ({}). Open one with `ghostex browser open <url>`; pages of other projects are never used without a pageId.",
            project.project_id
        )),
        AgentPages::NoProject => CliError::Other(format!(
            "This agent is not in a Ghostex project ({}), so the browser tools cannot tell which page is its own. Run them from the project's folder, pass --project-id to `ghostex browser mcp`, or pass a pageId from ghostex_list_pages.",
            scope.project_path.as_deref().unwrap_or("no folder")
        )),
    }
}

fn remembered_pages_path() -> PathBuf {
    rpc::ghostex_home()
        .join("cli")
        .join("browser-agent-pages.json")
}

fn read_remembered_pages() -> BTreeMap<String, Value> {
    std::fs::read_to_string(remembered_pages_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The page this agent session chose or used last, from an earlier bridge.
pub(super) fn remembered_agent_page(agent_session: &str) -> Option<String> {
    read_remembered_pages()
        .get(agent_session)?
        .get("pageId")?
        .as_str()
        .map(str::to_string)
}

/// Keeps `page_id` as this agent session's page for the bridges after this one; `None` forgets it.
pub(crate) fn remember_agent_page(agent_session: &str, page_id: Option<&str>) {
    let now = chrono::Utc::now().timestamp_millis();
    let mut pages = read_remembered_pages();
    pages.retain(|_, entry| {
        entry
            .get("updatedAt")
            .and_then(Value::as_i64)
            .is_some_and(|updated| now - updated < REMEMBERED_PAGE_TTL_MS)
    });
    match page_id {
        Some(page_id) => {
            pages.insert(
                agent_session.to_string(),
                json!({ "pageId": page_id, "updatedAt": now }),
            );
        }
        None => {
            if pages.remove(agent_session).is_none() {
                return;
            }
        }
    }
    let path = remembered_pages_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&pages) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_open_from_a_folder_that_is_no_project_goes_to_the_sessions_project() {
        let default_folder = json!({ "projectPath": "/Users/me/dev/app-session-worktree", "reuse": "similar", "url": "https://example.com" });
        let retry =
            super::super::browser_open_in_session_project(&default_folder, false, Some("P1"))
                .expect("retried in the session's project");
        assert_eq!(retry["projectId"], json!("P1"));
        assert!(retry.get("projectPath").is_none());
        assert_eq!(retry["url"], json!("https://example.com"));
        // A folder the caller named, another selector, or no session keeps the error.
        assert!(
            super::super::browser_open_in_session_project(&default_folder, true, Some("P1"))
                .is_none()
        );
        assert!(super::super::browser_open_in_session_project(
            &json!({ "projectId": "P9" }),
            false,
            Some("P1")
        )
        .is_none());
        assert!(
            super::super::browser_open_in_session_project(&default_folder, false, None).is_none()
        );
    }

    fn own(target: &str, front: bool, agent_page: bool) -> OwnPage {
        OwnPage {
            target_id: Some(target.to_string()),
            url: format!("https://example.com/{target}"),
            title: target.to_string(),
            front,
            agent_page,
        }
    }

    fn project(pages: Vec<OwnPage>) -> ProjectPages {
        ProjectPages {
            project_id: "P1".to_string(),
            pages,
        }
    }

    #[test]
    fn the_users_page_of_another_project_is_never_picked() {
        // The DevTools list starts with the user's tab of another project (USER), as it does
        // after the user brings it up; the agent's project has only its own page (MINE).
        let mine = project(vec![own("MINE", false, true)]);
        assert_eq!(
            choose_agent_page(&["USER", "MINE"], &mine, None, None).as_deref(),
            Some("MINE")
        );
    }

    #[test]
    fn a_page_the_agent_chose_by_id_stays_chosen() {
        // ghostex_select_page with a pageId reaches any page, and later calls keep it.
        let mine = project(vec![own("MINE", false, true)]);
        assert_eq!(
            choose_agent_page(&["USER", "MINE"], &mine, None, Some("USER")).as_deref(),
            Some("USER")
        );
    }

    #[test]
    fn switching_tabs_by_hand_does_not_move_the_agent() {
        // The agent opened A; the user then brought B, of the same project, to the front.
        let pages = project(vec![own("A", false, true), own("B", true, false)]);
        assert_eq!(
            choose_agent_page(&["B", "A"], &pages, None, None).as_deref(),
            Some("A")
        );
        // A page the agent chose in an earlier bridge stays its page, whatever is in front.
        let pages = project(vec![own("A", false, false), own("B", true, false)]);
        assert_eq!(
            choose_agent_page(&["B", "A"], &pages, None, Some("A")).as_deref(),
            Some("A")
        );
    }

    #[test]
    fn a_closed_remembered_page_falls_back_within_the_project() {
        let pages = project(vec![own("B", true, false), own("C", false, false)]);
        assert_eq!(
            choose_agent_page(&["C", "B"], &pages, None, Some("GONE")).as_deref(),
            Some("B")
        );
    }

    #[test]
    fn an_agent_with_no_page_of_its_own_gets_none() {
        let empty = project(vec![]);
        assert_eq!(choose_agent_page(&["USER"], &empty, None, None), None);
        // A page the app lists but the endpoint does not (asleep, or still starting) is not used.
        let asleep = project(vec![OwnPage {
            target_id: None,
            ..own("X", true, true)
        }]);
        assert_eq!(choose_agent_page(&["USER"], &asleep, None, None), None);
        let message =
            no_agent_page_error(&AgentPages::Project(empty), &AgentScope::default()).to_string();
        assert!(message.contains("No open Browser page belongs to this agent's project (P1)"));
    }

    #[test]
    fn the_apps_answer_is_read() {
        let answer = json!({
            "ok": true,
            "projectId": "P1",
            "agentTabId": 1,
            "pages": [
                { "tabId": 1, "targetId": "A", "url": "https://a", "title": "A", "front": false, "agentPage": true },
                { "tabId": 2, "targetId": null, "url": "https://b", "title": "B", "front": true, "agentPage": false },
            ],
        });
        let pages = ProjectPages::from_answer(&answer);
        assert_eq!(pages.project_id, "P1");
        assert!(pages.owns("A"));
        assert!(!pages.owns("B"));
        assert!(pages.pages[1].front);
        assert!(!pages.agent_page_still_starting());
    }
}
