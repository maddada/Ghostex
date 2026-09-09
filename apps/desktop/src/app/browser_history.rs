use base64::Engine as _;

use crate::browser_history::{self, HistoryPage};
use crate::*;

fn history_page(tab: &BrowserTab, project_id: &str, project_name: &str) -> Option<HistoryPage> {
    let url = sanitize_browser_tab_url_for_state(&tab.url)?;
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
                    let Some(url) = sanitize_browser_tab_url_for_state(url) else {
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
        let modal = GpuiAppModalKind::BrowserHistory;
        let sidebar_state = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        self.open_gpui_app_modal_window(
            modal,
            serde_json::json!({
                "type": "open", "modal": modal.modal_id(), "paneId": pane_id.0,
                "runtimeKey": self.browser_tabs_runtime_key,
            }),
            sidebar_state,
            Some(window),
            cx,
        );
    }

    pub(crate) fn receive_browser_history_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(host) = self.app_modal_window else {
            return;
        };
        match message["type"].as_str() {
            Some("browserHistoryQuery") => {
                let request_id = message["requestId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let project_id = (message["scope"].as_str() == Some("current"))
                    .then(|| self.browser_tabs_project_id.clone().unwrap_or_default());
                let query = message["query"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(1024)
                    .collect();
                let before = message["before"]["visitedAt"].as_i64().zip(
                    message["before"]["id"]
                        .as_str()
                        .and_then(|id| id.parse::<i64>().ok()),
                );
                cx.spawn(async move |_, cx| {
                    let result = browser_history::query(project_id, query, before).await;
                    let mut payload = match result {
                        Ok(result) => result,
                        Err(error) => {
                            serde_json::json!({"error": error, "entries": [], "hasMore": false})
                        }
                    };
                    payload["type"] = "browserHistoryResult".into();
                    payload["requestId"] = request_id.into();
                    let _ = host.update(cx, |host, _, cx| {
                        if host.current_modal == GpuiAppModalKind::BrowserHistory {
                            host.dispatch_transient_message(payload, cx);
                        }
                    });
                })
                .detach();
            }
            Some("browserHistoryOpen") => {
                let Some(id) = message["id"].as_str().and_then(|id| id.parse::<i64>().ok()) else {
                    return;
                };
                let Some(pane_id) = message["paneId"].as_u64().map(BrowserPaneId) else {
                    return;
                };
                let runtime_key = message["runtimeKey"].as_u64();
                cx.spawn(async move |this, cx| {
                    let result = browser_history::get(id).await;
                    let _ =
                        this.update_in(cx, |app, window, cx| {
                            if runtime_key != Some(app.browser_tabs_runtime_key)
                                || app.app_modal_window != Some(host)
                            {
                                return;
                            }
                            let is_history = host
                                .read_with(cx, |host, _| {
                                    host.current_modal == GpuiAppModalKind::BrowserHistory
                                })
                                .unwrap_or(false);
                            if !is_history {
                                return;
                            }
                            match result {
                                Ok(Some(page)) => {
                                    let Some(url) = sanitize_browser_tab_url_for_state(&page.url)
                                    else {
                                        return;
                                    };
                                    if !app.browser_tabs.focus_pane(pane_id) {
                                        return;
                                    }
                                    app.close_gpui_app_modal_window_and_restore_command_focus(cx);
                                    app.open_browser_popup_tab(
                                        url,
                                        page.remote_machine_id,
                                        cef::BrowserPopupPlacement::Selected,
                                        window,
                                        cx,
                                    );
                                }
                                result => {
                                    let error = match result {
                                        Err(error) => error,
                                        _ => "This history entry is no longer available.".into(),
                                    };
                                    let _ =
                                        host.update(cx, |host, _, cx| {
                                            host.dispatch_transient_message(serde_json::json!({
                                        "type": "browserHistoryOpenError", "error": error,
                                    }), cx);
                                        });
                                }
                            }
                        });
                })
                .detach();
            }
            _ => {}
        }
    }
}
