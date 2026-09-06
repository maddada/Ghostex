// C1 wave-3 re-cluster: browser tab/profile/navigation-history shell-state persistence, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

pub(crate) fn browser_tab_model_to_shell_state_json(model: &BrowserTabModel) -> serde_json::Value {
    serde_json::json!({
        "activeTabId": model.active_tab.0,
        "focusedPaneId": model.focused_pane.0,
        "nextPaneId": model.next_pane_id,
        "nextSplitId": model.next_split_id,
        "nextTabId": model.next_tab_id,
        "root": browser_node_to_shell_state_json(&model.root),
        "tabs": model
            .tabs
            .iter()
            .map(|tab| {
                let sanitized_url = sanitize_browser_tab_url_for_state(&tab.url);
                let state = if tab.state == BrowserTabState::Loaded && sanitized_url.is_some() {
                    BrowserTabState::Loaded
                } else {
                    BrowserTabState::AddressOnly
                };
                let history = if state == BrowserTabState::Loaded {
                    browser_navigation_history_to_shell_state_json(
                        &tab.navigation_history,
                        sanitized_url.as_deref(),
                    )
                } else {
                    None
                };
                /*
                CDXC:Browser 2026-07-12:
                Persist the tab's last displayed title so restart shows the
                same sidebar/tab-strip label instead of regressing to the
                URL-host fallback (e.g. "Google.com" for a tab that showed
                "New Tab"). Only Loaded tabs with a sanitized URL carry a
                cached title, and it is bounded before serialization.
                */
                let cached_title = if state == BrowserTabState::Loaded {
                    sanitize_browser_tab_cached_title(&tab.display_title())
                } else {
                    None
                };
                serde_json::json!({
                    "id": tab.id.0,
                    "profileId": tab.profile_id.0,
                    "remoteMachineId": tab.remote_machine_id,
                    "state": state.element_slug(),
                    "url": sanitized_url.unwrap_or_default(),
                    "history": history,
                    "cachedTitle": cached_title,
                })
            })
            .collect::<Vec<_>>(),
    })
}

pub(crate) fn browser_navigation_history_to_shell_state_json(
    history: &BrowserNavigationHistory,
    sanitized_tab_url: Option<&str>,
) -> Option<serde_json::Value> {
    /*
    CDXC:Telemetry 2026-06-22-10:09:
    Serialize Browser history at the writer boundary with the same strict tab URL sanitizer used for Browser shell state. If the current entry cannot be represented as the tab's sanitized loaded URL, omit the history object so restore rebuilds from the safe loaded URL instead of persisting raw navigation details.
    */
    let current_index = history
        .current_index
        .filter(|index| *index < history.entries.len())?;
    let mut sanitized_entries = Vec::new();
    let mut sanitized_current_index = None;
    for (index, url) in history.entries.iter().enumerate() {
        let sanitized_url = sanitize_browser_tab_url_for_state(url)?;
        if index == current_index {
            sanitized_current_index = Some(sanitized_entries.len());
        }
        sanitized_entries.push(sanitized_url);
    }

    let sanitized_current_index = sanitized_current_index?;
    if sanitized_entries.is_empty()
        || sanitized_entries.len() > BROWSER_HISTORY_MAX_ENTRIES
        || sanitized_tab_url.is_some_and(|url| {
            sanitized_entries
                .get(sanitized_current_index)
                .map(String::as_str)
                != Some(url)
        })
    {
        return None;
    }

    Some(serde_json::json!({
        "entries": sanitized_entries,
        "currentIndex": sanitized_current_index,
    }))
}

pub(crate) fn browser_node_to_shell_state_json(node: &BrowserNode) -> serde_json::Value {
    match node {
        BrowserNode::Leaf(leaf) => serde_json::json!({
            "type": "leaf",
            "paneId": leaf.pane_id.0,
            "activeTabId": leaf.tab_group.active_tab.0,
            "tabs": leaf
                .tab_group
                .tabs
                .iter()
                .map(|tab| serde_json::json!(tab.tab_id.0))
                .collect::<Vec<_>>(),
        }),
        BrowserNode::Split(split) => serde_json::json!({
            "type": "split",
            "splitId": split.id.0,
            "axis": split.axis.element_slug(),
            "ratio": json_number_f32(workspace_split_ratio(split.ratio)),
            "first": browser_node_to_shell_state_json(&split.first),
            "second": browser_node_to_shell_state_json(&split.second),
        }),
    }
}

pub(crate) fn browser_tab_model_from_shell_state(
    value: &serde_json::Value,
    browser_profiles: &BrowserProfileModel,
) -> Option<BrowserTabModel> {
    let object = value.as_object()?;
    let tabs = json_array_field(object, "tabs")?
        .iter()
        .map(|value| browser_tab_from_shell_state(value, browser_profiles))
        .collect::<Option<Vec<_>>>()?;
    if tabs.is_empty() || has_duplicate_u64(&tabs.iter().map(|tab| tab.id.0).collect::<Vec<_>>()) {
        return None;
    }
    let tab_ids = tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
    let root = if let Some(root_value) = object.get("root") {
        browser_node_from_shell_state(root_value, &tab_ids)?
    } else {
        /*
        CDXC:Browser 2026-06-22-09:02:
        Pre-split shell state stored Browser tabs as a flat strip. Restore that older private-data-safe shape as one Browser pane so users keep sanitized tab ids/URLs while the new split tree becomes the durable layout representation after the next save.
        */
        BrowserNode::Leaf(BrowserLeaf {
            pane_id: BrowserPaneId(1),
            tab_group: BrowserTabGroup {
                active_tab: json_u64_field(object, "activeTabId")
                    .map(BrowserTabId)
                    .filter(|tab_id| tab_ids.contains(tab_id))
                    .unwrap_or(tabs[0].id),
                tabs: tab_ids
                    .iter()
                    .copied()
                    .map(|tab_id| BrowserPaneTab { tab_id })
                    .collect(),
            },
        })
    };

    let mut pane_ids = Vec::new();
    collect_browser_leaf_ids(&root, &mut pane_ids);
    if pane_ids.is_empty()
        || has_duplicate_u64(&pane_ids.iter().map(|pane_id| pane_id.0).collect::<Vec<_>>())
    {
        return None;
    }

    let mut referenced_tab_ids = Vec::new();
    collect_browser_tab_ids(&root, &mut referenced_tab_ids);
    if referenced_tab_ids.is_empty()
        || has_duplicate_u64(
            &referenced_tab_ids
                .iter()
                .map(|tab_id| tab_id.0)
                .collect::<Vec<_>>(),
        )
        || referenced_tab_ids
            .iter()
            .any(|tab_id| !tab_ids.contains(tab_id))
    {
        return None;
    }

    let tabs = tabs
        .into_iter()
        .filter(|tab| referenced_tab_ids.contains(&tab.id))
        .collect::<Vec<_>>();
    if tabs.is_empty() {
        return None;
    }
    let first_pane_id = pane_ids.first().copied()?;
    let focused_pane = json_u64_field(object, "focusedPaneId")
        .map(BrowserPaneId)
        .filter(|pane_id| pane_ids.contains(pane_id))
        .unwrap_or(first_pane_id);
    let focused_pane_active_tab =
        find_browser_leaf(&root, focused_pane).and_then(|leaf| leaf.tab_group.active_tab_id());
    let active_tab = json_u64_field(object, "activeTabId")
        .map(BrowserTabId)
        .filter(|tab_id| Some(*tab_id) == focused_pane_active_tab)
        .or(focused_pane_active_tab)
        .or_else(|| first_browser_tab_id(&root))
        .unwrap_or(tabs[0].id);
    let next_pane_id = json_u64_field(object, "nextPaneId")
        .unwrap_or(0)
        .max(pane_ids.iter().map(|pane_id| pane_id.0).max().unwrap_or(0) + 1);
    let mut split_ids = Vec::new();
    collect_browser_split_ids(&root, &mut split_ids);
    if has_duplicate_u64(
        &split_ids
            .iter()
            .map(|split_id| split_id.0)
            .collect::<Vec<_>>(),
    ) {
        return None;
    }
    let next_split_id = json_u64_field(object, "nextSplitId").unwrap_or(0).max(
        split_ids
            .iter()
            .map(|split_id| split_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    let next_tab_id = json_u64_field(object, "nextTabId")
        .unwrap_or(0)
        .max(tabs.iter().map(|tab| tab.id.0).max().unwrap_or(0) + 1);

    Some(BrowserTabModel {
        tabs,
        root,
        focused_pane,
        active_tab,
        next_pane_id,
        next_split_id,
        next_tab_id,
    })
}

pub(crate) fn browser_node_from_shell_state(
    value: &serde_json::Value,
    tab_ids: &[BrowserTabId],
) -> Option<BrowserNode> {
    let object = value.as_object()?;
    match json_string_field(object, "type")? {
        "leaf" => {
            let pane_id = BrowserPaneId(json_u64_field(object, "paneId")?);
            if pane_id.0 == 0 {
                return None;
            }
            let tabs = json_array_field(object, "tabs")?
                .iter()
                .map(json_u64_value)
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .map(BrowserTabId)
                .collect::<Vec<_>>();
            if tabs.is_empty()
                || has_duplicate_u64(&tabs.iter().map(|tab_id| tab_id.0).collect::<Vec<_>>())
                || tabs.iter().any(|tab_id| !tab_ids.contains(tab_id))
            {
                return None;
            }
            let active_tab = json_u64_field(object, "activeTabId")
                .map(BrowserTabId)
                .filter(|tab_id| tabs.contains(tab_id))
                .unwrap_or(tabs[0]);
            Some(BrowserNode::Leaf(BrowserLeaf {
                pane_id,
                tab_group: BrowserTabGroup {
                    tabs: tabs
                        .into_iter()
                        .map(|tab_id| BrowserPaneTab { tab_id })
                        .collect(),
                    active_tab,
                },
            }))
        }
        "split" => {
            let split_id = BrowserSplitId(json_u64_field(object, "splitId")?);
            if split_id.0 == 0 {
                return None;
            }
            Some(BrowserNode::Split(BrowserSplit {
                id: split_id,
                axis: json_string_field(object, "axis").and_then(WorkspaceSplitAxis::from_slug)?,
                ratio: json_f32_field(object, "ratio")
                    .map(workspace_split_ratio)
                    .unwrap_or(0.5),
                first: Box::new(browser_node_from_shell_state(
                    object.get("first")?,
                    tab_ids,
                )?),
                second: Box::new(browser_node_from_shell_state(
                    object.get("second")?,
                    tab_ids,
                )?),
            }))
        }
        _ => None,
    }
}

pub(crate) fn browser_navigation_history_from_shell_state(
    value: Option<&serde_json::Value>,
    current_url: &str,
) -> BrowserNavigationHistory {
    let fallback = BrowserNavigationHistory::loaded(current_url);
    let Some(value) = value else {
        return fallback;
    };
    let Some(history) = browser_navigation_history_value_from_shell_state(value, current_url)
    else {
        return fallback;
    };
    history
}

pub(crate) fn browser_navigation_history_value_from_shell_state(
    value: &serde_json::Value,
    current_url: &str,
) -> Option<BrowserNavigationHistory> {
    let object = value.as_object()?;
    let entries = json_array_field(object, "entries")?
        .iter()
        .map(valid_sanitized_browser_history_url)
        .collect::<Option<Vec<_>>>()?;
    let current_index = usize::try_from(json_u64_field(object, "currentIndex")?).ok()?;
    if entries.is_empty()
        || entries.len() > BROWSER_HISTORY_MAX_ENTRIES
        || current_index >= entries.len()
        || entries.get(current_index).map(String::as_str) != Some(current_url)
    {
        return None;
    }

    Some(BrowserNavigationHistory {
        entries,
        current_index: Some(current_index),
    })
}

pub(crate) fn valid_sanitized_browser_history_url(value: &serde_json::Value) -> Option<String> {
    let url = value.as_str()?.trim();
    let sanitized_url = sanitize_browser_tab_url_for_state(url)?;
    (sanitized_url == url).then_some(sanitized_url)
}

pub(crate) fn browser_tab_from_shell_state(
    value: &serde_json::Value,
    browser_profiles: &BrowserProfileModel,
) -> Option<BrowserTab> {
    let object = value.as_object()?;
    let id = BrowserTabId(json_u64_field(object, "id")?);
    if id.0 == 0 {
        return None;
    }
    let requested_state = json_string_field(object, "state")
        .and_then(BrowserTabState::from_slug)
        .unwrap_or(BrowserTabState::AddressOnly);
    let sanitized_url =
        json_string_field(object, "url").and_then(sanitize_browser_tab_url_for_state);
    let (state, url) = match (requested_state, sanitized_url) {
        (BrowserTabState::Loaded, Some(url)) => (BrowserTabState::Loaded, url),
        (BrowserTabState::AddressOnly, Some(url)) => (BrowserTabState::AddressOnly, url),
        _ => (BrowserTabState::AddressOnly, String::new()),
    };

    /*
    CDXC:Browser 2026-07-12:
    Restore the persisted last-displayed title into the runtime title slot so
    the sidebar and tab strip show the pre-restart label until the page loads
    and reports a fresh document title.
    */
    let cached_title = if state == BrowserTabState::Loaded {
        json_string_field(object, "cachedTitle")
            .and_then(|title| sanitize_browser_tab_cached_title(&title))
    } else {
        None
    };
    Some(BrowserTab {
        id,
        remote_machine_id: json_string_field(object, "remoteMachineId")
            .and_then(crate::app::helpers::gpui_normalize_remote_machine_id),
        profile_id: json_u64_field(object, "profileId")
            .map(BrowserProfileId)
            .filter(|profile_id| browser_profiles.contains_profile(*profile_id))
            .unwrap_or_else(|| browser_profiles.active_profile_id()),
        title: if state == BrowserTabState::Loaded {
            browser_tab_title_for_url(&url)
        } else {
            "New Tab".to_string()
        },
        runtime_page_title: cached_title,
        runtime_favicon_url: None,
        runtime_favicon_image: None,
        runtime_favicon_fetch: None,
        runtime_is_loading: false,
        runtime_can_go_back: false,
        runtime_can_go_forward: false,
        url: url.clone(),
        state,
        navigation_history: if state == BrowserTabState::Loaded {
            browser_navigation_history_from_shell_state(object.get("history"), &url)
        } else {
            BrowserNavigationHistory::empty()
        },
    })
}
