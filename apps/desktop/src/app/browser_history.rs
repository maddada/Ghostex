use base64::Engine as _;

use crate::browser_history::{self, HistoryPage};
use crate::*;

fn history_page(tab: &BrowserTab, project_id: &str, project_name: &str) -> Option<HistoryPage> {
    let url = sanitize_browser_tab_url_for_tab(&tab.url, tab.remote_machine_id.as_deref())?;
    let favicon_url = if let Some(favicon) = &tab.runtime_favicon_image {
        Some(format!(
            "data:{};base64,{}",
            favicon.image.format().mime_type(),
            base64::engine::general_purpose::STANDARD.encode(favicon.image.bytes())
        ))
    } else {
        tab.runtime_favicon_fetch
            .as_ref()
            .map(|source| source.url.clone())
    };
    Some(HistoryPage {
        project_id: project_id.to_string(),
        project_name: project_name.to_string(),
        url,
        title: tab
            .runtime_page_title
            .as_deref()
            .unwrap_or_default()
            .chars()
            .take(1024)
            .collect(),
        favicon_url,
        remote_machine_id: tab.remote_machine_id.clone(),
    })
}

impl GhostexGpuiApp {
    pub(crate) fn record_browser_history_page(
        &self,
        runtime_key: u64,
        tab_id: BrowserTabId,
        original_project_name: &str,
        navigation: bool,
    ) {
        let (tabs, project_id, project_name) = match self.browser_runtime_owner_for_key(runtime_key)
        {
            Some(BrowserRuntimeOwner::Live) => (
                &self.browser_tabs,
                self.browser_tabs_project_id.as_deref().unwrap_or_default(),
                self.project_name.as_str(),
            ),
            Some(BrowserRuntimeOwner::Parked(ref id)) => {
                let Some(tabs) = self.parked_browser_tabs_by_project.get(id) else {
                    return;
                };
                let Some(tab) = tabs.tab(tab_id) else {
                    return;
                };
                if let Some(page) = history_page(tab, id, original_project_name) {
                    browser_history::record((runtime_key, tab_id.0), page, navigation);
                }
                return;
            }
            None => return,
        };
        if let Some(page) = tabs
            .tab(tab_id)
            .and_then(|tab| history_page(tab, project_id, project_name))
        {
            browser_history::record((runtime_key, tab_id.0), page, navigation);
        }
    }

    /// CDXC:Browser 2026-09-09 DECISION:
    /// User: Cmd+Y on macOS and Ctrl+H on Windows/Linux open history only in Browser view, through the same popup as the toolbar.
    pub(crate) fn show_browser_history_popup(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.active_mode != TitlebarMode::Browser
            || self.browser_tabs.find_leaf(pane_id).is_none()
        {
            return;
        }
        let mut remembered = Vec::new();
        for (project_id, tabs) in std::iter::once((
            self.browser_tabs_project_id.as_deref().unwrap_or_default(),
            &self.browser_tabs,
        ))
        .chain(
            self.parked_browser_tabs_by_project
                .iter()
                .map(|(id, tabs)| (id.as_str(), tabs)),
        ) {
            for tab in &tabs.tabs {
                for url in &tab.navigation_history.entries {
                    let Some(url) =
                        sanitize_browser_tab_url_for_tab(url, tab.remote_machine_id.as_deref())
                    else {
                        continue;
                    };
                    let Some(mut page) = history_page(
                        tab,
                        project_id,
                        if self.browser_tabs_project_id.as_deref() == Some(project_id) {
                            &self.project_name
                        } else {
                            ""
                        },
                    ) else {
                        continue;
                    };
                    if page.url != url {
                        page.title.clear();
                        page.favicon_url = None;
                    }
                    page.url = url;
                    remembered.push(page);
                }
            }
        }
        browser_history::import(remembered);
        // The window is native (browser_history_modal_lifecycle.rs) and takes no sidebar state.
        let modal = GpuiAppModalKind::BrowserHistory;
        self.open_gpui_app_modal_window(
            modal,
            serde_json::json!({
                "type": "open", "modal": modal.modal_id(), "paneId": pane_id.0,
                "runtimeKey": self.browser_tabs_runtime_key,
            }),
            serde_json::Value::Null,
            Some(window),
            cx,
        );
    }
}
