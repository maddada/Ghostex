//! Which Browser pages are an agent's: the pages of its own project, and the one it last opened
//! (`ghostex browser open`), so the browser tools (`ghostex browser mcp`) drive that page and
//! never one of another project.
//!
//! CDXC:Browser 2026-10-31 WHY:
//! The browser tools used to pick from the DevTools endpoint's list, which holds every page of every project and puts the one the user last brought up first, so an agent that had not pinned a page id drove the user's tab, and lost its own as soon as the user switched tabs (a Discord report, 2026-10-27). The app is the only side that knows which page is which tab of which project, so it answers `listBrowserPages` with one project's pages, each under its DevTools target id, and marks the page the asking agent opened.
//! SEE-ALSO: server/src/ghostex_cli/browser_mcp/agent_pages.rs (how the tools choose from this list), packages/gx-core/src/renderer_commands/verbs.rs (`ListBrowserPages`).

use serde_json::{Value, json};

use crate::app::model::*;
use crate::*;

/// One Browser page of a project, as `listBrowserPages` answers it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentBrowserPage {
    pub(crate) tab_id: BrowserTabId,
    /// `None` while the tab sleeps (no page) or its page has not told its id yet.
    pub(crate) target_id: Option<String>,
    pub(crate) url: String,
    pub(crate) title: String,
    /// The page in front of the project's focused pane.
    pub(crate) front: bool,
}

impl GhostexGpuiApp {
    /// After `ghostex browser open` from `agent_session` has opened or reused a tab of
    /// `project_id`, remembers that tab as the agent's page: the tab the open put in front of the
    /// project, live or parked.
    pub(crate) fn note_agent_browser_page(&mut self, agent_session: String, project_id: &str) {
        let tab_id = if self.browser_tabs_project_id.as_deref() == Some(project_id) {
            self.browser_tabs
                .active_tab_id_for_pane(self.browser_tabs.focused_pane)
        } else {
            self.pending_parked_browser_opens.get(project_id).copied()
        };
        if let Some(tab_id) = tab_id {
            self.browser_agent_pages
                .insert(agent_session, (project_id.to_string(), tab_id));
        }
    }

    /// `listBrowserPages`: the pages of `project_id`'s Browser, live or parked, and which of them
    /// `agent_session` opened last.
    pub(crate) fn list_agent_browser_pages(
        &self,
        project_id: &str,
        agent_session: Option<&str>,
        cx: &gpui::App,
    ) -> Value {
        let (tabs, surfaces) = if self.browser_tabs_project_id.as_deref() == Some(project_id) {
            (Some(&self.browser_tabs), Some(&self.browser_surfaces))
        } else {
            (
                self.parked_browser_tabs_by_project.get(project_id),
                self.parked_browser_runtimes_by_project
                    .get(project_id)
                    .map(|runtime| &runtime.surfaces),
            )
        };
        let front = tabs.and_then(|tabs| tabs.active_tab_id_for_pane(tabs.focused_pane));
        let pages = tabs
            .map(|tabs| {
                tabs.tabs
                    .iter()
                    .map(|tab| AgentBrowserPage {
                        tab_id: tab.id,
                        target_id: surfaces
                            .and_then(|surfaces| surfaces.get(&tab.id))
                            .and_then(|surface| {
                                surface.read(cx).devtools_target_id().map(str::to_string)
                            }),
                        url: tab.url.clone(),
                        title: tab.display_title(),
                        front: front == Some(tab.id),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let agent_tab = agent_session
            .and_then(|session| self.browser_agent_pages.get(session))
            .filter(|(owner, _)| owner == project_id)
            .map(|(_, tab_id)| *tab_id);
        agent_browser_pages_json(project_id, &pages, agent_tab)
    }
}

/// The answer's shape: `agentPage` is the tab the asking agent opened last, while it still exists.
pub(crate) fn agent_browser_pages_json(
    project_id: &str,
    pages: &[AgentBrowserPage],
    agent_tab: Option<BrowserTabId>,
) -> Value {
    let agent_tab = agent_tab.filter(|tab_id| pages.iter().any(|page| page.tab_id == *tab_id));
    json!({
        "ok": true,
        "projectId": project_id,
        "agentTabId": agent_tab.map(|tab_id| tab_id.0),
        "pages": pages.iter().map(|page| json!({
            "tabId": page.tab_id.0,
            "targetId": page.target_id,
            "url": page.url,
            "title": page.title,
            "front": page.front,
            "agentPage": Some(page.tab_id) == agent_tab,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(tab: u64, target: Option<&str>, front: bool) -> AgentBrowserPage {
        AgentBrowserPage {
            tab_id: BrowserTabId(tab),
            target_id: target.map(str::to_string),
            url: format!("https://example.com/{tab}"),
            title: format!("Page {tab}"),
            front,
        }
    }

    #[test]
    fn the_agents_page_stays_marked_whichever_tab_is_in_front() {
        // The user brought tab 2 to the front; the agent opened tab 1.
        let pages = [page(1, Some("T1"), false), page(2, Some("T2"), true)];
        let answer = agent_browser_pages_json("P1", &pages, Some(BrowserTabId(1)));
        assert_eq!(answer["agentTabId"], json!(1));
        assert_eq!(answer["pages"][0]["agentPage"], json!(true));
        assert_eq!(answer["pages"][1]["agentPage"], json!(false));
        assert_eq!(answer["pages"][1]["front"], json!(true));
    }

    #[test]
    fn a_closed_agent_tab_is_not_reported() {
        let pages = [page(2, Some("T2"), true)];
        let answer = agent_browser_pages_json("P1", &pages, Some(BrowserTabId(1)));
        assert_eq!(answer["agentTabId"], Value::Null);
    }
}
