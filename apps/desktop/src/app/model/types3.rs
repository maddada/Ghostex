// C1 wave-3 extraction: a chunk (3/6, in original file order) of the remaining plain value-type enums/structs/small helper fns moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use crate::*;


impl BrowserTabModel {
    pub(crate) fn shell_default() -> Self {
        Self::shell_default_with_profile(BrowserProfileId::default_profile())
    }

    pub(crate) fn shell_address_only_with_profile(profile_id: BrowserProfileId) -> Self {
        let mut model = Self::shell_default_with_profile(profile_id);
        let first_tab_id = model.active_tab;
        let _ = model.close_tab(first_tab_id, profile_id);
        model
    }

    pub(crate) fn shell_default_with_profile(profile_id: BrowserProfileId) -> Self {
        /*
        CDXC:GPUIBrowserTabs 2026-06-22-05:56:
        Browser project-editor mode needs native shell-level tab identity before durable browser behavior exists. Keep tab ids, active tab, title/url, and address-only placeholder state in memory, while runtime CEF entities are owned separately by loaded tab id.

        CDXC:GPUIBrowserTabs 2026-06-22-11:05:
        Runtime favicon images and HTTP(S) fetch sources belong to tab records but remain transient metadata alongside CEF entities, so shell defaults and restoration must initialize them empty and persistence must keep only sanitized tab URL/history state.

        CDXC:GPUIBrowserTabs 2026-06-22-06:59:
        Browser tabs now own runtime CEF entities by tab id, while this persisted shell model remains limited to sanitized tab metadata. Address-only tabs keep no URL and should render an empty GPUI body rather than borrowing stale page content from another tab.

        CDXC:GPUIBrowserDefault 2026-06-22-19:52:
        Fresh Browser shell state must use only the static default URL until a real sidebar/project snapshot contract carries an explicit browser start URL. GPUI must not infer Browser project start URLs from .git, paths, workspace names, fixture names, or sidebar titles.

        CDXC:GPUIBrowserPopups 2026-06-22-07:14:
        Page-initiated target=_blank and window.open requests should become selected GPUI Browser shell tabs for the requested URL, reusing the same per-tab CEF surface creation path as address-bar navigation. The shell model can keep the raw runtime URL in memory, but persistence must continue using the existing Browser metadata sanitizer.

        CDXC:GPUIBrowserPopups 2026-06-23-11:43:
        Popup parity is explicit: only non-empty target URLs create Browser tabs. Empty CEF targets, including script-created blank popups with no transferable URL/content, are handled as no-ops without address-only tab creation, CEF surface creation, shell-state persistence, notification, import, or content-transfer fallback.

        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser tabs now need shell-owned pane groups and left/right/top/bottom split order before full multi-CEF rendering exists. Keep BrowserTabId metadata and per-tab CEF ownership in one registry, while split leaves store only tab ids plus active selection so drag grouping and splitting never recreate Browser surfaces or persist raw page titles/query strings.

        CDXC:GPUIBrowserSplits 2026-06-22-09:55:
        Browser split panes should show the existing loaded CEF surface for each rendered leaf's active tab when Browser is awake and drags are not hiding native views. Restored or inactive loaded tabs without an existing CEF entity render restored/sleeping placeholder bodies until normal selection or wake materializes them, and address-only tabs never borrow another tab's surface.

        CDXC:GPUIBrowserProfiles 2026-06-23-11:14:
        Fresh and restored GPUI Browser tabs are assigned a generated shell profile id at model construction. That id is runtime-safe profile plumbing for future CEF surface creation and is intentionally separate from sanitized URL/history persistence.
        */
        let default_url = browser_shell_default_url(None);
        let first_tab = BrowserTab {
            id: BrowserTabId(1),
            profile_id,
            title: browser_tab_title_for_url(&default_url),
            runtime_page_title: None,
            runtime_favicon_url: None,
            runtime_favicon_image: None,
            runtime_favicon_fetch: None,
            runtime_is_loading: false,
            runtime_can_go_back: false,
            runtime_can_go_forward: false,
            url: default_url.clone(),
            state: BrowserTabState::Loaded,
            navigation_history: BrowserNavigationHistory::loaded(&default_url),
        };
        let pane_id = BrowserPaneId(1);

        Self {
            active_tab: first_tab.id,
            focused_pane: pane_id,
            root: BrowserNode::Leaf(BrowserLeaf {
                pane_id,
                tab_group: BrowserTabGroup {
                    tabs: vec![BrowserPaneTab {
                        tab_id: first_tab.id,
                    }],
                    active_tab: first_tab.id,
                },
            }),
            tabs: vec![first_tab],
            next_pane_id: 2,
            next_split_id: 1,
            next_tab_id: 2,
        }
    }

    pub(crate) fn tab(&self, tab_id: BrowserTabId) -> Option<&BrowserTab> {
        self.tabs.iter().find(|tab| tab.id == tab_id)
    }

    pub(crate) fn has_tab(&self, tab_id: BrowserTabId) -> bool {
        self.tab(tab_id).is_some()
    }

    pub(crate) fn active_tab(&self) -> Option<&BrowserTab> {
        if let Some(tab) = self.tab(self.active_tab) {
            return Some(tab);
        }

        self.find_leaf(self.focused_pane)
            .and_then(|leaf| leaf.tab_group.active_tab_id())
            .and_then(|tab_id| self.tab(tab_id))
            .or_else(|| first_browser_tab_id(&self.root).and_then(|tab_id| self.tab(tab_id)))
            .or_else(|| self.tabs.first())
    }

    pub(crate) fn select_tab_in_pane(&mut self, pane_id: BrowserPaneId, tab_id: BrowserTabId) -> bool {
        if !self.has_tab(tab_id) {
            return false;
        }

        let selected = self.find_leaf_mut(pane_id).is_some_and(|leaf| {
            if leaf.tab_group.has_tab(tab_id) {
                leaf.tab_group.active_tab = tab_id;
                true
            } else {
                false
            }
        });

        if selected {
            self.focused_pane = pane_id;
            self.active_tab = tab_id;
        }
        selected
    }

    pub(crate) fn focus_pane(&mut self, pane_id: BrowserPaneId) -> bool {
        let Some(active_tab) = self
            .find_leaf(pane_id)
            .and_then(|leaf| leaf.tab_group.active_tab_id())
        else {
            return false;
        };
        if !self.has_tab(active_tab) {
            return false;
        }
        self.focused_pane = pane_id;
        self.active_tab = active_tab;
        true
    }

    pub(crate) fn cycle_tab_in_focused_pane(&mut self, reverse: bool) -> Option<BrowserTabId> {
        let pane_id = self.focused_pane;
        let tab_id = {
            let leaf = self.find_leaf_mut(pane_id)?;
            leaf.tab_group.cycle_active_tab(reverse)?
        };
        if !self.has_tab(tab_id) {
            return None;
        }
        self.focused_pane = pane_id;
        self.active_tab = tab_id;
        Some(tab_id)
    }

    pub(crate) fn add_address_placeholder_tab(&mut self, profile_id: BrowserProfileId) -> BrowserTabId {
        /*
        CDXC:GPUIFocusedNewTabs 2026-06-22-12:51:
        Browser new-tab commands, including Cmd+N and the clicked pane control, must insert the address-only placeholder immediately after the focused pane's active tab so creation stays adjacent to the user's current Browser work instead of appending to a long tab group.

        CDXC:GPUIBrowserProfiles 2026-06-23-11:14:
        New address-only Browser tabs inherit the currently selected generated Browser profile at creation time. Later profile selection changes affect future tabs/surfaces only and must not mutate existing tab profile ownership.
        */
        let tab_id = BrowserTabId(self.next_tab_id);
        self.next_tab_id += 1;
        self.tabs.push(BrowserTab {
            id: tab_id,
            profile_id,
            title: "New Tab".to_string(),
            runtime_page_title: None,
            runtime_favicon_url: None,
            runtime_favicon_image: None,
            runtime_favicon_fetch: None,
            runtime_is_loading: false,
            runtime_can_go_back: false,
            runtime_can_go_forward: false,
            url: String::new(),
            state: BrowserTabState::AddressOnly,
            navigation_history: BrowserNavigationHistory::empty(),
        });
        self.active_tab = tab_id;
        let tab = BrowserPaneTab { tab_id };
        if let Some(leaf) = self.find_leaf_mut(self.focused_pane) {
            let insertion_index = leaf
                .tab_group
                .active_tab_index()
                .map(|index| index + 1)
                .unwrap_or(leaf.tab_group.tabs.len());
            leaf.tab_group.insert_tab_at(tab, insertion_index);
            leaf.tab_group.active_tab = tab_id;
        } else {
            let pane_id = self.allocate_pane_id();
            self.root = BrowserNode::Leaf(BrowserLeaf {
                pane_id,
                tab_group: BrowserTabGroup {
                    tabs: vec![tab],
                    active_tab: tab_id,
                },
            });
            self.focused_pane = pane_id;
        }
        tab_id
    }

    pub(crate) fn find_renderer_open_reuse_tab(
        &self,
        url: &str,
        reuse: GpuiBrowserRendererOpenReuse,
    ) -> Option<(BrowserPaneId, BrowserTabId)> {
        /*
        macOS `findBrowserSessionInProjectForReuse` parity: `none` never reuses,
        an exact-URL tab always wins, and `similar` falls back to the first tab
        whose scheme+host origin matches. GPUI's Browser shell is app-global, so
        the reuse scope is the window's tab set instead of a per-project group.
        */
        if reuse == GpuiBrowserRendererOpenReuse::None {
            return None;
        }
        let exact = self
            .tabs
            .iter()
            .find(|tab| tab.url == url)
            .map(|tab| tab.id);
        let tab_id = match exact {
            Some(tab_id) => Some(tab_id),
            None if reuse == GpuiBrowserRendererOpenReuse::Exact => None,
            None => {
                let origin = browser_url_origin_key(url)?;
                self.tabs
                    .iter()
                    .find(|tab| browser_url_origin_key(&tab.url).as_deref() == Some(&origin))
                    .map(|tab| tab.id)
            }
        }?;
        let pane_id = find_browser_leaf_id_for_tab(&self.root, tab_id)?;
        Some((pane_id, tab_id))
    }

    pub(crate) fn load_pane_active_tab_url(
        &mut self,
        pane_id: BrowserPaneId,
        url: String,
    ) -> Option<(BrowserTabId, BrowserProfileId)> {
        let tab_id = self.active_tab_id_for_pane(pane_id)?;
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return None;
        };
        tab.title = browser_tab_title_for_url(&url);
        tab.runtime_page_title = None;
        tab.runtime_favicon_url = None;
        tab.runtime_favicon_image = None;
        tab.runtime_favicon_fetch = None;
        tab.runtime_is_loading = true;
        tab.runtime_can_go_back = false;
        tab.runtime_can_go_forward = false;
        tab.navigation_history.append_loaded_url(&url);
        tab.url = url;
        tab.state = BrowserTabState::Loaded;
        Some((tab.id, tab.profile_id))
    }

    pub(crate) fn set_tab_profile(&mut self, tab_id: BrowserTabId, profile_id: BrowserProfileId) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return false;
        };
        if tab.profile_id == profile_id {
            return false;
        }

        tab.profile_id = profile_id;
        tab.runtime_page_title = None;
        tab.runtime_favicon_url = None;
        tab.runtime_favicon_image = None;
        tab.runtime_favicon_fetch = None;
        tab.runtime_is_loading = tab.state == BrowserTabState::Loaded;
        tab.runtime_can_go_back = false;
        tab.runtime_can_go_forward = false;
        true
    }

    pub(crate) fn reload_loaded_tab_url(&mut self, tab_id: BrowserTabId, url: String) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return false;
        };
        if tab.state != BrowserTabState::Loaded {
            return false;
        }
        tab.title = browser_tab_title_for_url(&url);
        tab.runtime_page_title = None;
        tab.runtime_favicon_url = None;
        tab.runtime_favicon_image = None;
        tab.runtime_favicon_fetch = None;
        tab.runtime_is_loading = true;
        tab.runtime_can_go_back = false;
        tab.runtime_can_go_forward = false;
        tab.navigation_history.record_address_change(&url);
        tab.url = url;
        true
    }

    pub(crate) fn record_page_address_change(&mut self, tab_id: BrowserTabId, url: String) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return false;
        };
        let url = url.trim().to_string();
        let title = browser_tab_title_for_url(&url);
        let history_changed = tab.navigation_history.record_address_change(&url);
        let changed = tab.url != url
            || tab.title != title
            || tab.runtime_page_title.is_some()
            || tab.runtime_favicon_url.is_some()
            || tab.runtime_favicon_image.is_some()
            || tab.runtime_favicon_fetch.is_some()
            || tab.state != BrowserTabState::Loaded
            || history_changed;
        tab.title = title;
        tab.runtime_page_title = None;
        tab.runtime_favicon_url = None;
        tab.runtime_favicon_image = None;
        tab.runtime_favicon_fetch = None;
        tab.url = url;
        tab.state = BrowserTabState::Loaded;
        changed
    }

    pub(crate) fn record_page_title_change(&mut self, tab_id: BrowserTabId, title: String) -> bool {
        let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id && tab.state == BrowserTabState::Loaded)
        else {
            return false;
        };
        let title = title.trim();
        let runtime_page_title = if title.is_empty() {
            None
        } else {
            Some(title.to_string())
        };
        if tab.runtime_page_title == runtime_page_title {
            return false;
        }
        tab.runtime_page_title = runtime_page_title;
        true
    }

    pub(crate) fn record_page_favicon_url_change(
        &mut self,
        tab_id: BrowserTabId,
        favicon_url: Option<String>,
    ) -> bool {
        let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id && tab.state == BrowserTabState::Loaded)
        else {
            return false;
        };
        let (favicon_url, favicon_image, favicon_fetch) =
            browser_runtime_favicon_from_url(favicon_url.as_deref());
        if tab.runtime_favicon_url == favicon_url
            && tab.runtime_favicon_image == favicon_image
            && tab.runtime_favicon_fetch == favicon_fetch
        {
            return false;
        }
        tab.runtime_favicon_url = favicon_url;
        tab.runtime_favicon_image = favicon_image;
        tab.runtime_favicon_fetch = favicon_fetch;
        true
    }

    pub(crate) fn record_page_loading_state_change(
        &mut self,
        tab_id: BrowserTabId,
        is_loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    ) -> bool {
        let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id && tab.state == BrowserTabState::Loaded)
        else {
            return false;
        };
        let changed = tab.runtime_is_loading != is_loading
            || tab.runtime_can_go_back != can_go_back
            || tab.runtime_can_go_forward != can_go_forward;
        tab.runtime_is_loading = is_loading;
        tab.runtime_can_go_back = can_go_back;
        tab.runtime_can_go_forward = can_go_forward;
        changed
    }

    pub(crate) fn add_loaded_popup_tab(
        &mut self,
        requested_url: String,
        profile_id: BrowserProfileId,
        placement: cef::BrowserPopupPlacement,
    ) -> Option<BrowserTabId> {
        /*
        CDXC:GPUIBrowserPopups 2026-06-23-11:43:
        Empty popup targets are a shell-boundary no-op, not a request for an address-only Browser tab. Admit only trimmed non-empty target URLs here so blank script popups cannot mutate selection, create CEF surfaces, persist shell state, or start any content-transfer fallback.

        CDXC:GPUIBrowserPopups 2026-06-23-14:30:
        The popup target string is accepted only as the loaded-tab target identifier after leading/trailing whitespace is removed. Do not reinterpret an empty target as address-bar text, restored page content, an import request, a notification-worthy event, or a fallback tab.
        */
        let requested_url = browser_loaded_popup_target_url(&requested_url)?;

        /*
        CDXC:GPUIBrowserLinkNewTab 2026-08-18:
        Background placement is the middle-click/Cmd-click contract: append the
        loaded tab to the focused pane's strip and leave selection where it is,
        so the tab materializes its CEF surface later through the normal
        inactive-loaded-tab path.
        */
        let selects_new_tab = matches!(placement, cef::BrowserPopupPlacement::Selected);

        let tab_id = BrowserTabId(self.next_tab_id);
        self.next_tab_id += 1;
        let tab = BrowserTab {
            id: tab_id,
            profile_id,
            title: browser_tab_title_for_url(&requested_url),
            runtime_page_title: None,
            runtime_favicon_url: None,
            runtime_favicon_image: None,
            runtime_favicon_fetch: None,
            runtime_is_loading: false,
            runtime_can_go_back: false,
            runtime_can_go_forward: false,
            url: requested_url.clone(),
            state: BrowserTabState::Loaded,
            navigation_history: BrowserNavigationHistory::loaded(&requested_url),
        };
        self.tabs.push(tab);
        if selects_new_tab {
            self.active_tab = tab_id;
        }
        let pane_tab = BrowserPaneTab { tab_id };
        if let Some(leaf) = self.find_leaf_mut(self.focused_pane) {
            leaf.tab_group
                .insert_tab_at(pane_tab, leaf.tab_group.tabs.len());
            if selects_new_tab {
                leaf.tab_group.active_tab = tab_id;
            }
        } else {
            // A pane created for this tab has no other tab to keep selected,
            // so background placement collapses into the selected case.
            let pane_id = self.allocate_pane_id();
            self.root = BrowserNode::Leaf(BrowserLeaf {
                pane_id,
                tab_group: BrowserTabGroup {
                    tabs: vec![pane_tab],
                    active_tab: tab_id,
                },
            });
            self.focused_pane = pane_id;
            self.active_tab = tab_id;
        }
        Some(tab_id)
    }

    pub(crate) fn close_tab(&mut self, tab_id: BrowserTabId, active_profile_id: BrowserProfileId) -> bool {
        /*
        CDXC:GPUIBrowserTabs 2026-06-22-05:56:
        Closing Browser tabs must be modest but visible in the parity shell. Multi-tab closes remove the target and keep or select a neighboring tab in memory; closing the last tab resets it to an address-only placeholder so Browser mode never drops to an empty workspace while multiple CEF views and persisted tab lifecycles remain deferred.

        CDXC:GPUIBrowserProfiles 2026-06-23-11:14:
        Resetting the final Browser tab creates a new address-only placeholder in place, so it adopts the currently selected generated profile for any future load instead of retaining the closed page's profile ownership.
        */
        if !self.has_tab(tab_id) {
            return false;
        };

        if self.tabs.len() == 1 {
            let tab = &mut self.tabs[0];
            tab.profile_id = active_profile_id;
            tab.title = "New Tab".to_string();
            tab.runtime_page_title = None;
            tab.runtime_favicon_url = None;
            tab.runtime_favicon_image = None;
            tab.runtime_favicon_fetch = None;
            tab.runtime_is_loading = false;
            tab.runtime_can_go_back = false;
            tab.runtime_can_go_forward = false;
            tab.url.clear();
            tab.state = BrowserTabState::AddressOnly;
            tab.navigation_history.clear();
            self.active_tab = tab.id;
            let reset_tab_id = tab.id;
            if let Some(leaf) = self.find_leaf_mut(self.focused_pane) {
                leaf.tab_group.tabs = vec![BrowserPaneTab {
                    tab_id: reset_tab_id,
                }];
                leaf.tab_group.active_tab = reset_tab_id;
            } else {
                let pane_id = BrowserPaneId(1);
                self.root = BrowserNode::Leaf(BrowserLeaf {
                    pane_id,
                    tab_group: BrowserTabGroup {
                        tabs: vec![BrowserPaneTab {
                            tab_id: reset_tab_id,
                        }],
                        active_tab: reset_tab_id,
                    },
                });
                self.focused_pane = pane_id;
                self.next_pane_id = self.next_pane_id.max(2);
            }
            return true;
        }

        let closing_active = self.active_tab == tab_id;
        let Some(source_pane_id) = find_browser_leaf_id_for_tab(&self.root, tab_id) else {
            return false;
        };
        let Some((_tab, source_is_empty)) = self.remove_tab_for_move(source_pane_id, tab_id) else {
            return false;
        };
        self.tabs.retain(|tab| tab.id != tab_id);

        if source_is_empty {
            self.collapse_empty_leaf(source_pane_id);
        }
        if self.find_leaf(self.focused_pane).is_none()
            && let Some(first_leaf_id) = first_browser_leaf_id(&self.root)
        {
            self.focused_pane = first_leaf_id;
        }
        if closing_active || !self.has_tab(self.active_tab) {
            if let Some(active_tab) = self
                .find_leaf(self.focused_pane)
                .and_then(|leaf| leaf.tab_group.active_tab_id())
                .or_else(|| first_browser_tab_id(&self.root))
            {
                self.active_tab = active_tab;
            }
        }
        true
    }

    pub(crate) fn reorder_tab_within_pane(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        insertion_index: usize,
    ) -> bool {
        /*
        CDXC:GPUIBrowserDragDrop 2026-06-22-07:41:
        Browser tab-strip drops are same-strip reorder only for this slice. Reorder the existing BrowserTab records in place, preserve the active BrowserTabId, and leave runtime titles, current URLs, address-only state, and the tab-owned CEF surface map untouched so dragging a tab never reloads or recreates a page surface.

        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Same-strip Browser reorders are scoped to the dragged tab's source Browser pane. The metadata registry and CEF map stay keyed by BrowserTabId, while only that pane's tab-id order changes and persists through sanitized shell state.
        */
        let Some(leaf) = self.find_leaf_mut(pane_id) else {
            return false;
        };
        let active_tab = leaf.tab_group.active_tab;
        let Some(tab) = leaf.tab_group.remove_tab(tab_id) else {
            return false;
        };
        leaf.tab_group.insert_tab_at(tab, insertion_index);
        leaf.tab_group.active_tab = active_tab;
        self.focused_pane = pane_id;
        self.active_tab = active_tab;
        true
    }

    pub(crate) fn group_tab_into_pane(
        &mut self,
        source_pane_id: BrowserPaneId,
        target_pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
    ) -> bool {
        if !self.has_tab(tab_id) || self.find_leaf(target_pane_id).is_none() {
            return false;
        }

        if source_pane_id == target_pane_id {
            return self.select_tab_in_pane(target_pane_id, tab_id);
        }

        let Some((tab, source_is_empty)) = self.remove_tab_for_move(source_pane_id, tab_id) else {
            return false;
        };

        if source_is_empty {
            self.collapse_empty_leaf(source_pane_id);
        }

        let Some(target_leaf) = self.find_leaf_mut(target_pane_id) else {
            return false;
        };
        target_leaf
            .tab_group
            .insert_tab_at(tab, target_leaf.tab_group.tabs.len());
        target_leaf.tab_group.active_tab = tab_id;
        self.focused_pane = target_pane_id;
        self.active_tab = tab_id;
        true
    }

    pub(crate) fn split_tab_to_pane(
        &mut self,
        source_pane_id: BrowserPaneId,
        target_pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        zone: WorkspaceDropZone,
    ) -> bool {
        /*
        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser pane-body drops mirror Agents behavior in the placeholder shell: center groups the dragged tab into the target Browser tab group, while left/right/top/bottom edge drops split that same BrowserTabId into a new pane. This is layout/state only; CEF surface identity stays keyed by BrowserTabId and split rendering decides visibility without recreating page state.
        */
        if matches!(zone, WorkspaceDropZone::Center) {
            return self.group_tab_into_pane(source_pane_id, target_pane_id, tab_id);
        }

        if !self.has_tab(tab_id) || self.find_leaf(target_pane_id).is_none() {
            return false;
        }

        if source_pane_id == target_pane_id
            && self.pane_tab_count(source_pane_id).unwrap_or_default() <= 1
        {
            return false;
        }

        let Some((tab, source_is_empty)) = self.remove_tab_for_move(source_pane_id, tab_id) else {
            return false;
        };

        if source_is_empty {
            self.collapse_empty_leaf(source_pane_id);
        }

        let pane_id = self.allocate_pane_id();
        let split_id = self.allocate_split_id();
        let new_leaf = BrowserLeaf {
            pane_id,
            tab_group: BrowserTabGroup {
                tabs: vec![tab],
                active_tab: tab_id,
            },
        };
        let axis = match zone {
            WorkspaceDropZone::Left | WorkspaceDropZone::Right => WorkspaceSplitAxis::Horizontal,
            WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => WorkspaceSplitAxis::Vertical,
            WorkspaceDropZone::Center => unreachable!("center grouping handled above"),
        };
        let dragged_first = matches!(zone, WorkspaceDropZone::Left | WorkspaceDropZone::Top);

        if insert_browser_leaf_split(
            &mut self.root,
            target_pane_id,
            new_leaf,
            axis,
            dragged_first,
            split_id,
        ) {
            self.focused_pane = pane_id;
            self.active_tab = tab_id;
            true
        } else {
            false
        }
    }

    pub(crate) fn split_new_loaded_tab_to_pane(
        &mut self,
        target_pane_id: BrowserPaneId,
        zone: WorkspaceDropZone,
        profile_id: BrowserProfileId,
        url: String,
    ) -> Option<BrowserTabId> {
        /*
        CDXC:GPUIBrowserPaneActions 2026-06-22-13:46:
        Browser pane-menu split actions create a selected address-only tab in a new split pane by focusing the clicked pane, using the normal address-only tab creation path, then moving that new tab through the existing Browser split helper. This preserves Browser split ordering, tab metadata persistence, and CEF ownership because address-only tabs do not create or load browser surfaces.
        */
        if matches!(zone, WorkspaceDropZone::Center) || !self.focus_pane(target_pane_id) {
            return None;
        }

        let tab_id =
            self.add_loaded_popup_tab(url, profile_id, cef::BrowserPopupPlacement::Selected)?;
        self.split_tab_to_pane(target_pane_id, target_pane_id, tab_id, zone)
            .then_some(tab_id)
    }

    pub(crate) fn pane_tab_count(&self, pane_id: BrowserPaneId) -> Option<usize> {
        self.find_leaf(pane_id)
            .map(|leaf| leaf.tab_group.tabs.len())
    }

    pub(crate) fn remove_tab_for_move(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
    ) -> Option<(BrowserPaneTab, bool)> {
        let leaf = self.find_leaf_mut(pane_id)?;
        let tab = leaf.tab_group.remove_tab(tab_id)?;
        let source_is_empty = leaf.tab_group.tabs.is_empty();
        Some((tab, source_is_empty))
    }

    pub(crate) fn collapse_empty_leaf(&mut self, pane_id: BrowserPaneId) {
        let root_is_empty = collapse_empty_browser_leaf(&mut self.root, pane_id);
        if root_is_empty {
            self.root = browser_dummy_node();
        }

        if self.focused_pane == pane_id
            && let Some(first_leaf_id) = first_browser_leaf_id(&self.root)
        {
            self.focused_pane = first_leaf_id;
        }
    }

    pub(crate) fn find_leaf(&self, pane_id: BrowserPaneId) -> Option<&BrowserLeaf> {
        find_browser_leaf(&self.root, pane_id)
    }

    pub(crate) fn find_leaf_mut(&mut self, pane_id: BrowserPaneId) -> Option<&mut BrowserLeaf> {
        find_browser_leaf_mut(&mut self.root, pane_id)
    }

    pub(crate) fn rendered_leaf_order(&self) -> Vec<BrowserPaneId> {
        let mut pane_ids = Vec::new();
        collect_browser_leaf_ids(&self.root, &mut pane_ids);
        pane_ids
    }

    pub(crate) fn active_loaded_tab_id_for_leaf(&self, leaf: &BrowserLeaf) -> Option<BrowserTabId> {
        let tab_id = leaf.tab_group.active_tab_id()?;
        self.tab(tab_id)
            .filter(|tab| tab.state == BrowserTabState::Loaded)
            .map(|tab| tab.id)
    }

    pub(crate) fn rendered_active_loaded_tab_ids(&self) -> HashSet<BrowserTabId> {
        /*
        CDXC:GPUIBrowserSplits 2026-06-22-09:55:
        Visible Browser CEF bodies are derived from rendered split leaves, not from the focused/global active Browser tab alone. This helper intentionally returns loaded tab ids only; it does not create CEF entities, so restored active tabs without existing surfaces and address-only tabs keep black placeholder bodies.
        */
        self.rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| self.find_leaf(pane_id))
            .filter_map(|leaf| self.active_loaded_tab_id_for_leaf(leaf))
            .collect()
    }

    pub(crate) fn split_ratio(&self, split_id: BrowserSplitId) -> Option<f32> {
        find_browser_split(&self.root, split_id).map(|split| workspace_split_ratio(split.ratio))
    }

    pub(crate) fn set_split_ratio(&mut self, split_id: BrowserSplitId, ratio: f32) -> bool {
        let next_ratio = workspace_split_ratio(ratio);
        let Some(split) = find_browser_split_mut(&mut self.root, split_id) else {
            return false;
        };

        if (workspace_split_ratio(split.ratio) - next_ratio).abs() < 0.001 {
            return false;
        }

        split.ratio = next_ratio;
        true
    }

    pub(crate) fn reset_split_ratio(&mut self, split_id: BrowserSplitId) -> bool {
        self.set_split_ratio(split_id, 0.5)
    }

    pub(crate) fn split_drag_ratio_bounds(
        &self,
        split_id: BrowserSplitId,
        content_span: f32,
    ) -> Option<(f32, f32)> {
        let split = find_browser_split(&self.root, split_id)?;
        let minimum = split_pane_resize_minimum_for_axis(split.axis);
        split_drag_ratio_bounds_from_minimums(
            browser_node_axis_pane_count(&split.first, split.axis) as f32 * minimum,
            browser_node_axis_pane_count(&split.second, split.axis) as f32 * minimum,
            content_span,
        )
    }

    pub(crate) fn allocate_pane_id(&mut self) -> BrowserPaneId {
        let pane_id = BrowserPaneId(self.next_pane_id);
        self.next_pane_id += 1;
        pane_id
    }

    pub(crate) fn allocate_split_id(&mut self) -> BrowserSplitId {
        let split_id = BrowserSplitId(self.next_split_id);
        self.next_split_id += 1;
        split_id
    }

    pub(crate) fn active_address_value(&self) -> String {
        self.active_tab()
            .map(BrowserTab::address_value)
            .unwrap_or_default()
    }

    pub(crate) fn active_tab_id_for_pane(&self, pane_id: BrowserPaneId) -> Option<BrowserTabId> {
        self.find_leaf(pane_id)
            .and_then(|leaf| leaf.tab_group.active_tab_id())
    }

    pub(crate) fn active_tab_for_pane(&self, pane_id: BrowserPaneId) -> Option<&BrowserTab> {
        self.active_tab_id_for_pane(pane_id)
            .and_then(|tab_id| self.tab(tab_id))
    }

    pub(crate) fn address_value_for_pane(&self, pane_id: BrowserPaneId) -> String {
        self.active_tab_for_pane(pane_id)
            .map(BrowserTab::address_value)
            .unwrap_or_default()
    }

    pub(crate) fn pane_history_rows(&self, pane_id: BrowserPaneId, max_rows: usize) -> Vec<BrowserHistoryRow> {
        self.active_tab_for_pane(pane_id)
            .filter(|tab| tab.state == BrowserTabState::Loaded)
            .map(|tab| tab.navigation_history.rows_around_current(max_rows))
            .unwrap_or_default()
    }
}


impl BrowserTab {
    pub(crate) fn address_value(&self) -> String {
        match self.state {
            BrowserTabState::Loaded => self.url.clone(),
            BrowserTabState::AddressOnly => self.url.clone(),
        }
    }

    pub(crate) fn cef_url(&self) -> String {
        match self.state {
            BrowserTabState::Loaded if !self.url.trim().is_empty() => self.url.clone(),
            BrowserTabState::Loaded | BrowserTabState::AddressOnly => {
                BROWSER_ADDRESS_ONLY_CEF_URL.to_string()
            }
        }
    }

    pub(crate) fn display_title(&self) -> String {
        match self.state {
            BrowserTabState::Loaded => self
                .runtime_page_title
                .clone()
                .unwrap_or_else(|| self.title.clone()),
            BrowserTabState::AddressOnly => "New Tab".to_string(),
        }
    }
}


impl BrowserTabGroup {
    pub(crate) fn active_tab_id(&self) -> Option<BrowserTabId> {
        self.tabs
            .iter()
            .find(|tab| tab.tab_id == self.active_tab)
            .or_else(|| self.tabs.first())
            .map(|tab| tab.tab_id)
    }

    pub(crate) fn active_tab_index(&self) -> Option<usize> {
        let active_tab_id = self.active_tab_id()?;
        self.tabs.iter().position(|tab| tab.tab_id == active_tab_id)
    }

    pub(crate) fn has_tab(&self, tab_id: BrowserTabId) -> bool {
        self.tabs.iter().any(|tab| tab.tab_id == tab_id)
    }

    pub(crate) fn cycle_active_tab(&mut self, reverse: bool) -> Option<BrowserTabId> {
        if self.tabs.is_empty() {
            return None;
        }

        let current_index = self
            .tabs
            .iter()
            .position(|tab| tab.tab_id == self.active_tab)
            .unwrap_or(0);
        let next_index = if reverse {
            current_index
                .checked_sub(1)
                .unwrap_or(self.tabs.len().saturating_sub(1))
        } else {
            (current_index + 1) % self.tabs.len()
        };
        self.active_tab = self.tabs[next_index].tab_id;
        Some(self.active_tab)
    }

    pub(crate) fn remove_tab(&mut self, tab_id: BrowserTabId) -> Option<BrowserPaneTab> {
        let tab_index = self.tabs.iter().position(|tab| tab.tab_id == tab_id)?;
        let tab = self.tabs.remove(tab_index);

        if self.active_tab == tab_id
            && let Some(next_active_tab) = self.tabs.get(tab_index).or_else(|| self.tabs.last())
        {
            self.active_tab = next_active_tab.tab_id;
        }

        Some(tab)
    }

    pub(crate) fn insert_tab_at(&mut self, tab: BrowserPaneTab, insertion_index: usize) {
        let mut target_index = insertion_index.min(self.tabs.len());

        if let Some(existing_index) = self
            .tabs
            .iter()
            .position(|candidate| candidate.tab_id == tab.tab_id)
        {
            let existing_tab = self.tabs.remove(existing_index);
            if existing_index < target_index {
                target_index -= 1;
            }
            self.tabs
                .insert(target_index.min(self.tabs.len()), existing_tab);
        } else {
            self.tabs.insert(target_index, tab);
        }
    }
}


#[derive(Clone, Copy)]
pub(crate) enum CommandPaneControlAction {
    NewCommandPlaceholder,
    TogglePinned,
    ToggleExpanded,
}


pub(crate) fn command_pane_control_action_selects_clicked_group_before_dispatch(
    action: CommandPaneControlAction,
) -> bool {
    /*
    CDXC:GPUICommandPaneControls 2026-06-25-22:01:
    Native command titlebar actions call through the clicked owner titlebar and focus that command session before dispatch. GPUI fixed command-panel controls must retarget the clicked command group for Pin/Unpin and Minimize instead of falling back to the previously focused group; New Terminal already carries explicit insertion targeting.
    */
    matches!(
        action,
        CommandPaneControlAction::TogglePinned | CommandPaneControlAction::ToggleExpanded
    )
}


pub(crate) fn command_pane_focus_clicked_control_group(
    command_pane: &mut CommandPaneModel,
    action: CommandPaneControlAction,
    target_group_id: Option<CommandPaneGroupId>,
) -> bool {
    if !command_pane_control_action_selects_clicked_group_before_dispatch(action) {
        return true;
    }

    let Some(group_id) = target_group_id else {
        return true;
    };

    command_pane.focus_group(group_id)
}


pub(crate) fn command_pane_control_action_focuses_command_pane(
    action: CommandPaneControlAction,
    was_expanded: bool,
    is_expanded_after: bool,
) -> bool {
    /*
    CDXC:GPUICommandPaneFocus 2026-06-25-18:27:
    Native command titlebar actions focus the command terminal before dispatch, including Pin/Unpin on already-expanded panels. GPUI command-pane controls use the same focus policy so action clicks update command-pane focus without clearing project-editor workspace mode.

    CDXC:GPUICommandAttention 2026-06-25-19:58:
    Any titlebar control that focuses an existing command session should also acknowledge that session's Attention state, matching native command titlebar focus while leaving non-focusing Minimize and new idle command creation out of the acknowledgement path.
    */
    match action {
        CommandPaneControlAction::NewCommandPlaceholder => true,
        CommandPaneControlAction::TogglePinned => is_expanded_after,
        CommandPaneControlAction::ToggleExpanded => !was_expanded && is_expanded_after,
    }
}


#[derive(Clone, Copy)]
pub(crate) enum BrowserToolbarAction {
    Back,
    Forward,
    Reload,
    StopLoading,
    Home,
    FeedbackTool,
    ResetZoom,
    ResetMediaPermissions,
    HistoryMenu,
    ProfileMenu,
    DevTools,
}


#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiBrowserFindState {
    pub(crate) query: String,
    pub(crate) match_count: i32,
    pub(crate) active_match_ordinal: i32,
    pub(crate) final_update: bool,
}


/*
CDXC:GPUIBrowserMediaPermissions 2026-07-27:
Browser panes answer CEF microphone/camera requests with a real in-pane
permission prompt instead of the Alloy default (silent deny). The prompt is a
normal layout row between the toolbar and the page body — no overlay, no
hit-test routing — and the answer is remembered per browser profile + page
origin so a site asks once. Persistence stores only the scheme+authority
origin marker plus allow/block, matching the favicon-marker privacy rule: no
paths, query strings, fragments, credentials, or page content.

Requests arrive one at a time per tab but a page can ask for the microphone
and the camera in separate calls, so pending prompts queue per tab and the
front one renders. Dropping an unanswered prompt cancels its CEF request, so
closing the tab or navigating away releases the page's pending promise.
*/
pub(crate) struct GpuiBrowserMediaPermissionPrompt {
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) origin: String,
    /// Everything the page asked for; the answer grants the allowed subset of
    /// this, including devices allowed by an earlier prompt.
    pub(crate) requested: cef::BrowserMediaAccessKinds,
    /// The undecided subset this prompt actually asks about, so an origin that
    /// already allowed the microphone is only asked about the camera.
    pub(crate) pending: cef::BrowserMediaAccessKinds,
    pub(crate) request: cef::BrowserMediaAccessRequest,
}


#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiBrowserMediaPermissionDecision {
    pub(crate) microphone: Option<bool>,
    pub(crate) camera: Option<bool>,
}


impl GpuiBrowserMediaPermissionDecision {
    pub(crate) fn is_empty(self) -> bool {
        self.microphone.is_none() && self.camera.is_none()
    }

    /// Requested devices this origin has no stored answer for yet.
    pub(crate) fn undecided(self, requested: cef::BrowserMediaAccessKinds) -> cef::BrowserMediaAccessKinds {
        cef::BrowserMediaAccessKinds {
            microphone: requested.microphone && self.microphone.is_none(),
            camera: requested.camera && self.camera.is_none(),
        }
    }

    /// Requested devices this origin is already allowed to use.
    pub(crate) fn granted(self, requested: cef::BrowserMediaAccessKinds) -> cef::BrowserMediaAccessKinds {
        cef::BrowserMediaAccessKinds {
            microphone: requested.microphone && self.microphone == Some(true),
            camera: requested.camera && self.camera == Some(true),
        }
    }

    pub(crate) fn record(&mut self, kinds: cef::BrowserMediaAccessKinds, allow: bool) {
        if kinds.microphone {
            self.microphone = Some(allow);
        }
        if kinds.camera {
            self.camera = Some(allow);
        }
    }
}


#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiBrowserMediaPermissionDecisions {
    pub(crate) origins: HashMap<String, GpuiBrowserMediaPermissionDecision>,
}


impl GpuiBrowserMediaPermissionDecisions {
    pub(crate) fn decision(
        &self,
        profile_id: BrowserProfileId,
        origin: &str,
    ) -> GpuiBrowserMediaPermissionDecision {
        self.origins
            .get(&gpui_browser_media_permission_key(profile_id, origin))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn record(
        &mut self,
        profile_id: BrowserProfileId,
        origin: &str,
        kinds: cef::BrowserMediaAccessKinds,
        allow: bool,
    ) {
        self.origins
            .entry(gpui_browser_media_permission_key(profile_id, origin))
            .or_default()
            .record(kinds, allow);
    }

    /// Returns true when a stored decision was actually removed, so callers
    /// only reload the page when the site will really be asked again.
    pub(crate) fn forget(&mut self, profile_id: BrowserProfileId, origin: &str) -> bool {
        self.origins
            .remove(&gpui_browser_media_permission_key(profile_id, origin))
            .is_some_and(|decision| !decision.is_empty())
    }
}


pub(crate) fn gpui_browser_media_permission_key(profile_id: BrowserProfileId, origin: &str) -> String {
    format!("{}|{origin}", profile_id.cef_profile_string())
}


/// Normalizes a CEF requesting origin down to the scheme+authority marker used
/// as the stored permission key. Opaque or authority-less origins (`null`,
/// `data:`, `about:`) have no stable identity to remember, so they get no key
/// and are never prompted for.
pub(crate) fn gpui_browser_media_permission_origin(raw: &str) -> Option<String> {
    let (scheme, rest) = raw.trim().split_once("://")?;
    let scheme = scheme.trim().to_ascii_lowercase();
    if scheme.is_empty() {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
        .to_ascii_lowercase();
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}"))
}


pub(crate) fn gpui_browser_media_permission_display_origin(origin: &str) -> String {
    origin
        .split_once("://")
        .map_or(origin, |(_, authority)| authority)
        .to_string()
}


pub(crate) fn gpui_browser_media_permission_kinds_label(kinds: cef::BrowserMediaAccessKinds) -> &'static str {
    match (kinds.microphone, kinds.camera) {
        (true, true) => "your microphone and camera",
        (true, false) => "your microphone",
        _ => "your camera",
    }
}


#[derive(Clone, Copy)]
pub(crate) enum GpuiFocusedSurfaceZoomCommand {
    In,
    Out,
    Reset,
}


#[derive(Clone, Copy)]
pub(crate) enum WorkspaceTabActionIcon {
    NewTerminal,
    NewBrowser,
    Overflow,
}


/*
CDXC:GPUIAgentsTerminalStartupState 2026-06-22-23:50:
Failed Agents terminal startup is a first-class presentation state so the tab survives launch failure with clear retry UI. The durable shell state stores only the safe `startup-failed` slug; runtime ids, launch errors, command text, cwd/path, env, process ids, stdout/stderr, and terminal content remain runtime-only or absent.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalSessionPresentationState {
    Running,
    Sleeping,
    Mounting,
    StartupFailed,
    RestoredUnmounted,
    PoppedOutPlaceholder,
}


impl TerminalSessionPresentationState {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "sleeping" => Some(Self::Sleeping),
            "mounting" => Some(Self::Mounting),
            "startup-failed" => Some(Self::StartupFailed),
            "restored-unmounted" => Some(Self::RestoredUnmounted),
            "popped-out-placeholder" => Some(Self::PoppedOutPlaceholder),
            _ => None,
        }
    }

    pub(crate) fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    pub(crate) fn activation_pending_state(self) -> Option<Self> {
        match self {
            Self::Sleeping
            | Self::StartupFailed
            | Self::RestoredUnmounted
            | Self::PoppedOutPlaceholder => Some(Self::Mounting),
            Self::Running | Self::Mounting => None,
        }
    }

    pub(crate) fn counts_as_focus_mode_visible(self) -> bool {
        !matches!(self, Self::Sleeping)
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Sleeping => "sleeping",
            Self::Mounting => "mounting",
            Self::StartupFailed => "startup-failed",
            Self::RestoredUnmounted => "restored-unmounted",
            Self::PoppedOutPlaceholder => "popped-out-placeholder",
        }
    }

    pub(crate) fn tab_badge_label(self) -> Option<&'static str> {
        match self {
            Self::Running => None,
            Self::Sleeping => None,
            Self::Mounting => Some("MNT"),
            Self::StartupFailed => Some("ERR"),
            Self::RestoredUnmounted => Some("RST"),
            Self::PoppedOutPlaceholder => Some("OUT"),
        }
    }

    pub(crate) fn placeholder_label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Sleeping => "Sleeping",
            Self::Mounting => "Mounting",
            Self::StartupFailed => "Startup failed",
            Self::RestoredUnmounted => "Restored",
            Self::PoppedOutPlaceholder => "Popped out",
        }
    }

    pub(crate) fn placeholder_title(self) -> &'static str {
        match self {
            Self::Running => "Terminal",
            Self::Sleeping => "Sleeping terminal",
            Self::Mounting => "Mounting terminal",
            Self::StartupFailed => "Terminal startup failed",
            Self::RestoredUnmounted => "Restored terminal",
            Self::PoppedOutPlaceholder => "Terminal popped out",
        }
    }

    /*
    CDXC:GPUITerminalPresentationState 2026-06-24-07:35:
    Terminal presentation-state copy is user-visible terminal copy, even though this source helper still represents the current shell body. Sleeping and mounting messages must remain source-only, non-launching, and private-detail-free while accurately describing pending wake/materialization state.
    */
    pub(crate) fn placeholder_message(self) -> &'static str {
        match self {
            Self::Running => "",
            Self::Sleeping => {
                "This tab stays sleeping when selected. Activate this terminal to enter wake startup pending until the runtime is ready."
            }
            Self::Mounting => {
                "Startup or materialization is pending until the terminal runtime and surface are ready. Clicking only focuses this pending terminal and does not mark it running."
            }
            Self::StartupFailed => {
                "The terminal runtime did not start. Retry keeps this tab in place and returns it to startup pending without storing private launch details."
            }
            Self::RestoredUnmounted => {
                "This restored tab remains in the workspace tree while its runtime surface is unmounted. Activate it to enter materialization pending."
            }
            Self::PoppedOutPlaceholder => {
                "The session is represented in this pane while its active surface belongs to a popped-out window. Activate it to enter reattach pending."
            }
        }
    }

    pub(crate) fn placeholder_action_label(self) -> &'static str {
        match self {
            Self::Running => "",
            Self::Sleeping => "Wake",
            Self::Mounting => "Pending startup",
            Self::StartupFailed => "Retry",
            Self::RestoredUnmounted => "Materialize",
            Self::PoppedOutPlaceholder => "Reattach",
        }
    }
}


/*
CDXC:GPUIAgentsTabStatus 2026-06-22-16:27:
Agents running-tab dots must use semantic placeholder state from the macOS sidebar vocabulary instead of arbitrary session-id colors. Persist only the safe shell metadata: idle/working/attention activity plus a Delayed Send boolean; never persist deadlines, labels, command text, paths, stdout/stderr, terminal content, tokens, or user-owned titles.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentTerminalActivity {
    Idle,
    Working,
    Attention,
}


impl Default for AgentTerminalActivity {
    fn default() -> Self {
        Self::Idle
    }
}


impl AgentTerminalActivity {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(Self::Idle),
            "working" => Some(Self::Working),
            "attention" => Some(Self::Attention),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Attention => "attention",
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentTerminalTabStatus {
    Idle,
    Working,
    Attention,
    DelayedSend,
}


impl AgentTerminalTabStatus {
    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Attention => "attention",
            Self::DelayedSend => "delayed-send",
        }
    }
}


pub(crate) fn agent_terminal_tab_status(
    activity: AgentTerminalActivity,
    delayed_send_active: bool,
) -> AgentTerminalTabStatus {
    if delayed_send_active {
        AgentTerminalTabStatus::DelayedSend
    } else {
        match activity {
            AgentTerminalActivity::Idle => AgentTerminalTabStatus::Idle,
            AgentTerminalActivity::Working => AgentTerminalTabStatus::Working,
            AgentTerminalActivity::Attention => AgentTerminalTabStatus::Attention,
        }
    }
}


/*
CDXC:GPUIAgentsTabChrome 2026-06-22-17:07:
Agents workspace tab chrome is focus-invariant: pane focus may change pane borders and keyboard ownership, but tab-bar visuals derive only from a tab's presentation lifecycle, semantic status, and active membership inside its own tab group.

CDXC:GPUIAgentsTabChrome 2026-06-22-17:27:
Selected Agents tabs use selected chrome even when their terminal lifecycle is sleeping, mounting, failed startup, restored/unmounted, or popped out. Workspace tab fill and title colors mirror the native AppKit tab strip: selected tabs use the active white overlay, and all inactive tabs share the inactive overlay while lifecycle state moves to the trailing status slot or placeholder badge.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceTabLifecycleVisualRole {
    SelectedRunning,
    InactiveRunning,
    SelectedNonRunning,
    InactiveNonRunning,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceTabLifecycleVisualTone {
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) role: WorkspaceTabLifecycleVisualRole,
}


impl WorkspaceTabLifecycleVisualTone {
    pub(crate) fn new(
        presentation_state: TerminalSessionPresentationState,
        active_in_tab_group: bool,
    ) -> Self {
        let role = match (presentation_state.is_running(), active_in_tab_group) {
            (true, true) => WorkspaceTabLifecycleVisualRole::SelectedRunning,
            (true, false) => WorkspaceTabLifecycleVisualRole::InactiveRunning,
            (false, true) => WorkspaceTabLifecycleVisualRole::SelectedNonRunning,
            (false, false) => WorkspaceTabLifecycleVisualRole::InactiveNonRunning,
        };

        Self {
            presentation_state,
            role,
        }
    }

    pub(crate) fn uses_selected_treatment(self) -> bool {
        matches!(
            self.role,
            WorkspaceTabLifecycleVisualRole::SelectedRunning
                | WorkspaceTabLifecycleVisualRole::SelectedNonRunning
        )
    }

    pub(crate) fn uses_inactive_running_treatment(self) -> bool {
        matches!(self.role, WorkspaceTabLifecycleVisualRole::InactiveRunning)
    }

    pub(crate) fn uses_subdued_non_running_treatment(self) -> bool {
        matches!(
            self.role,
            WorkspaceTabLifecycleVisualRole::InactiveNonRunning
        )
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceTabChromeSignature {
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) tab_status: AgentTerminalTabStatus,
    pub(crate) active_in_tab_group: bool,
    pub(crate) lifecycle_visual_tone: WorkspaceTabLifecycleVisualTone,
}


pub(crate) fn workspace_tab_chrome_signature(
    tab_group: &WorkspaceTabGroup,
    session_id: TerminalSessionId,
    session: Option<&TerminalSession>,
) -> WorkspaceTabChromeSignature {
    let presentation_state = session
        .map(|session| session.presentation_state)
        .unwrap_or(TerminalSessionPresentationState::Running);
    let active_in_tab_group = tab_group.active_session_id() == Some(session_id);
    WorkspaceTabChromeSignature {
        presentation_state,
        tab_status: session
            .map(TerminalSession::tab_status)
            .unwrap_or(AgentTerminalTabStatus::Idle),
        active_in_tab_group,
        lifecycle_visual_tone: WorkspaceTabLifecycleVisualTone::new(
            presentation_state,
            active_in_tab_group,
        ),
    }
}


pub(crate) fn workspace_tab_icon_element(
    element_id: impl Into<String>,
    agent_icon: Option<&'static str>,
    visual_tone: WorkspaceTabLifecycleVisualTone,
) -> AnyElement {
    let element_id = element_id.into();
    if let Some(agent_icon) = agent_icon
        && let Some(icon_path) = workspace_tab_agent_icon_path(agent_icon)
    {
        return workspace_tab_agent_icon_element(element_id, agent_icon, icon_path, visual_tone);
    }
    workspace_tab_terminal_icon_element(element_id, visual_tone)
}


pub(crate) fn workspace_tab_agent_icon_element(
    element_id: impl Into<String>,
    agent_icon: &'static str,
    icon_path: &'static str,
    visual_tone: WorkspaceTabLifecycleVisualTone,
) -> AnyElement {
    div()
        .id(element_id.into())
        .flex()
        .flex_shrink_0()
        .size(px(WORKSPACE_TAB_AGENT_ICON_SIZE))
        .items_center()
        .justify_center()
        .child(
            svg()
                .path(icon_path)
                .size(px(workspace_tab_agent_svg_size(agent_icon)))
                .text_color(workspace_tab_agent_icon_text_color(agent_icon, visual_tone)),
        )
        .into_any_element()
}


pub(crate) fn workspace_tab_terminal_icon_element(
    element_id: impl Into<String>,
    visual_tone: WorkspaceTabLifecycleVisualTone,
) -> AnyElement {
    let presentation_state = visual_tone.presentation_state;
    div()
        .id(element_id.into())
        .relative()
        .flex_shrink_0()
        .w(px(WORKSPACE_TAB_ICON_WIDTH))
        .h(px(11.0))
        .rounded(px(2.0))
        .border_1()
        .border_color(if visual_tone.uses_selected_treatment() {
            workspace_tab_terminal_icon_active_color(presentation_state)
        } else {
            workspace_tab_terminal_icon_inactive_color(presentation_state)
        })
        .bg(if visual_tone.uses_selected_treatment() {
            workspace_tab_terminal_icon_active_background(presentation_state)
        } else {
            workspace_tab_terminal_icon_inactive_background(presentation_state)
        })
        .child(
            div()
                .absolute()
                .left(px(3.0))
                .top(px(3.0))
                .w(px(3.0))
                .h(px(1.0))
                .bg(workspace_tab_terminal_icon_glyph_color(visual_tone)),
        )
        .child(
            div()
                .absolute()
                .left(px(6.0))
                .top(px(6.0))
                .w(px(5.0))
                .h(px(1.0))
                .bg(workspace_tab_terminal_icon_glyph_color(visual_tone)),
        )
        .into_any_element()
}


pub(crate) fn workspace_tab_status_indicator_visible(
    visual_tone: WorkspaceTabLifecycleVisualTone,
    tab_status: AgentTerminalTabStatus,
    tab_hovered: bool,
) -> bool {
    visual_tone.presentation_state.is_running()
        && !tab_hovered
        && !matches!(tab_status, AgentTerminalTabStatus::Idle)
}


pub(crate) fn workspace_tab_status_title_trailing_reserved_width(
    visual_tone: WorkspaceTabLifecycleVisualTone,
    tab_status: AgentTerminalTabStatus,
) -> f32 {
    if visual_tone.presentation_state == TerminalSessionPresentationState::Sleeping {
        WORKSPACE_TAB_SLEEP_TITLE_RESERVED_WIDTH
    } else if workspace_tab_status_indicator_visible(visual_tone, tab_status, false) {
        WORKSPACE_TAB_STATUS_TITLE_RESERVED_WIDTH
    } else {
        0.0
    }
}


pub(crate) fn workspace_tab_status_indicator_element(
    element_id: impl Into<String>,
    visual_tone: WorkspaceTabLifecycleVisualTone,
    tab_status: AgentTerminalTabStatus,
) -> AnyElement {
    div()
        .id(element_id.into())
        .absolute()
        .right(px(WORKSPACE_TAB_STATUS_INDICATOR_TRAILING_PADDING))
        .top(px((WORKSPACE_TAB_BAR_HEIGHT
            - WORKSPACE_TAB_STATUS_INDICATOR_SIZE)
            / 2.0))
        .size(px(WORKSPACE_TAB_STATUS_INDICATOR_SIZE))
        .rounded_full()
        .bg(workspace_tab_status_dot_color(visual_tone, tab_status))
        .into_any_element()
}


pub(crate) fn workspace_tab_sleep_icon_visible(
    visual_tone: WorkspaceTabLifecycleVisualTone,
    tab_hovered: bool,
) -> bool {
    visual_tone.presentation_state == TerminalSessionPresentationState::Sleeping && !tab_hovered
}


pub(crate) fn workspace_tab_sleep_icon_color() -> Hsla {
    rgb(0xdbdbdb).opacity(0.42).into()
}


impl Render for WorkspaceTabDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let visual_tone = WorkspaceTabLifecycleVisualTone::new(self.presentation_state, true);
        let show_status_indicator =
            workspace_tab_status_indicator_visible(visual_tone, self.tab_status, false);
        let title_trailing_reserved_width =
            workspace_tab_status_title_trailing_reserved_width(visual_tone, self.tab_status);

        div()
            .relative()
            .flex()
            .h(px(WORKSPACE_TAB_BAR_HEIGHT))
            .w(px(WORKSPACE_TAB_WIDTH))
            .items_center()
            .overflow_hidden()
            .rounded(px(5.0))
            .border_1()
            .border_color(workspace_drop_feedback_border_color())
            .bg(workspace_tab_drag_preview_color())
            .pl(px(11.0))
            .pr(px(10.0))
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(workspace_tab_active_text_color())
            .shadow_md()
            .child(workspace_tab_icon_element(
                "ghostex-gpui-workspace-tab-drag-preview-icon",
                self.agent_icon,
                visual_tone,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .ml(px(8.0))
                    .pr(px(title_trailing_reserved_width))
                    .child(self.title.clone()),
            )
            .when(show_status_indicator, |this| {
                this.child(workspace_tab_status_indicator_element(
                    format!(
                        "ghostex-gpui-workspace-tab-drag-preview-status-{}",
                        self.tab_status.element_slug()
                    ),
                    visual_tone,
                    self.tab_status,
                ))
            })
    }
}


impl Render for CommandTabDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let show_status_indicator =
            command_terminal_tab_status_indicator_visible(self.tab_status, false);
        let title_trailing_reserved_width =
            command_terminal_tab_status_title_trailing_reserved_width(self.tab_status);

        div()
            .relative()
            .flex()
            .h(px(COMMAND_PANE_TAB_BAR_HEIGHT))
            .w(px(COMMAND_PANE_TAB_MAX_WIDTH))
            .items_center()
            .overflow_hidden()
            .rounded(px(5.0))
            .border_1()
            .border_color(workspace_drop_feedback_border_color())
            .bg(command_pane_tab_background_color(true, false))
            .pl(px(8.0))
            .pr(px(0.0))
            .text_size(px(COMMAND_PANE_TAB_TITLE_FONT_SIZE))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(command_pane_tab_title_text_color(true, false))
            .shadow_md()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .pr(px(title_trailing_reserved_width))
                    .child(self.title.clone()),
            )
            .when(show_status_indicator, |this| {
                this.child(command_pane_tab_status_indicator_element(
                    "ghostex-gpui-command-tab-drag-preview-status",
                    self.tab_status,
                ))
            })
    }
}


impl Render for BrowserTabDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .h(px(BROWSER_TAB_BAR_HEIGHT))
            .w(px(BROWSER_TAB_MAX_WIDTH))
            .items_center()
            .overflow_hidden()
            .rounded(px(5.0))
            .border_1()
            .border_color(workspace_drop_feedback_border_color())
            .bg(workspace_tab_drag_preview_color())
            .pl(px(8.0))
            .pr(px(10.0))
            .text_size(px(13.0))
            .font_weight(FontWeight::NORMAL)
            .text_color(browser_tab_text_color(self.state, true))
            .shadow_md()
            .child(browser_tab_icon_element(
                self.profile_id,
                self.chrome_status,
                self.runtime_favicon_url.as_deref(),
                self.runtime_favicon_image.as_ref(),
                self.runtime_favicon_fetch.as_ref(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .ml(px(5.0))
                    .child(self.title.clone()),
            )
    }
}


#[derive(Clone)]
pub(crate) struct TerminalSession {
    pub(crate) id: TerminalSessionId,
    pub(crate) title: String,
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) startup_eligible_when_mounting: bool,
    /*
    CDXC:GPUISessionTitleOverlay 2026-08-04:
    The generating-title flag belongs to the reconciled runtime tab session
    that owns the visible pane. Keeping it only in the separate sidebar focus
    snapshot made Windows render depend on a reverse project/session mapping
    that can be absent while a WSL terminal is attaching. This flag is
    presentation-only and must not enter shell-state persistence.
    */
    pub(crate) is_generating_first_prompt_title: bool,
    pub(crate) activity: AgentTerminalActivity,
    pub(crate) agent_icon: Option<&'static str>,
    pub(crate) kind: AgentsWorkspaceSessionKind,
    pub(crate) delayed_send_active: bool,
    /*
    CDXC:GPUIZmxPersistenceRefresh 2026-07-06:
    Runtime-only zmx persistence session name captured from gxserver attach
    metadata, mirroring macOS `session.sessionPersistenceName`. Terminal-content
    clicks use it for zmx's conditional grid-size refresh. Never persisted to
    shell-state JSON and never derived from titles, paths, or renderer text.
    */
    pub(crate) zmx_session_name: Option<String>,
}


#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AgentsWorkspaceSessionKind {
    #[default]
    Terminal,
}


impl AgentsWorkspaceSessionKind {
    pub(crate) fn from_sidebar_kind(kind: &str) -> Option<Self> {
        match kind {
            "agent" | "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }

    pub(crate) fn shell_state_slug(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
        }
    }
}


impl TerminalSession {
    pub(crate) fn placeholder(
        id: TerminalSessionId,
        title: String,
        presentation_state: TerminalSessionPresentationState,
    ) -> Self {
        Self {
            id,
            title,
            startup_eligible_when_mounting: presentation_state
                == TerminalSessionPresentationState::Mounting,
            presentation_state,
            is_generating_first_prompt_title: false,
            activity: AgentTerminalActivity::Idle,
            agent_icon: None,
            kind: AgentsWorkspaceSessionKind::Terminal,
            delayed_send_active: false,
            zmx_session_name: None,
        }
    }

    pub(crate) fn with_activity(mut self, activity: AgentTerminalActivity) -> Self {
        self.activity = activity;
        self
    }

    pub(crate) fn with_agent_icon(mut self, agent_icon: Option<&'static str>) -> Self {
        self.agent_icon = agent_icon;
        self
    }

    pub(crate) fn with_kind(mut self, kind: AgentsWorkspaceSessionKind) -> Self {
        self.kind = kind;
        self
    }

    pub(crate) fn with_delayed_send_active(mut self, delayed_send_active: bool) -> Self {
        self.delayed_send_active = delayed_send_active;
        self
    }

    pub(crate) fn can_enter_startup_pipeline(&self) -> bool {
        self.presentation_state == TerminalSessionPresentationState::Mounting
            && self.startup_eligible_when_mounting
    }

    pub(crate) fn set_presentation_state(&mut self, presentation_state: TerminalSessionPresentationState) {
        self.set_presentation_state_with_startup_eligibility(
            presentation_state,
            presentation_state == TerminalSessionPresentationState::Mounting,
        );
    }

    pub(crate) fn set_presentation_state_with_startup_eligibility(
        &mut self,
        presentation_state: TerminalSessionPresentationState,
        startup_eligible_when_mounting: bool,
    ) {
        /*
        CDXC:GPUITerminalActivationRuntimeGuard 2026-06-23-18:00:
        `Mounting` can mean a true new/retry/materialize startup or a wake/reattach placeholder waiting for an exact parked owner move. Keep that startup eligibility runtime-only and out of shell-state JSON so restored shell-state `mounting` after restart, sleeping wake, and popped-out reattach cannot accidentally create a duplicate Ghostty process, while new terminals, explicit restored-unmounted activation, and in-process failed-startup retry remain eligible.
        */
        self.presentation_state = presentation_state;
        self.startup_eligible_when_mounting = presentation_state
            == TerminalSessionPresentationState::Mounting
            && startup_eligible_when_mounting;
    }

    pub(crate) fn tab_status(&self) -> AgentTerminalTabStatus {
        agent_terminal_tab_status(self.activity, self.delayed_send_active)
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceTab {
    pub(crate) session_id: TerminalSessionId,
}


#[derive(Clone)]
pub(crate) struct WorkspaceLeaf {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) tab_group: WorkspaceTabGroup,
}


/*
CDXC:GPUITerminalTextInput 2026-06-23-20:34:
Focused terminal text mount targets derive Debug for compile-time diagnostics and tests, so their Agents and command slot IDs must carry Debug too. The derived output contains only stable numeric IDs, not user-owned terminal text or paths.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalBodyMountSlotId {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CommandTerminalBodyMountSlotId {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ProjectEditorCompanionTerminalBodyMountSlotId {
    pub(crate) mode: TitlebarMode,
    pub(crate) session_id: TerminalSessionId,
}


#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ProjectEditorCompanionTerminalSlot {
    #[default]
    Top,
    Bottom,
}


/*
CDXC:GPUIZmxPersistenceRefresh 2026-07-06:
Runtime-only identity of the terminal slot that currently owns shell focus,
compared across renders to mirror macOS
`refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged`. Carries only slot ids;
never persisted or logged with titles, paths, or terminal content.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZmxPersistenceFocusedTerminalSlot {
    Agents(AgentsTerminalBodyMountSlotId),
    Command(CommandTerminalBodyMountSlotId),
    Companion(ProjectEditorCompanionTerminalBodyMountSlotId),
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneVisibleBodyOwner {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
    pub(crate) is_sleeping: bool,
}


impl CommandPaneVisibleBodyOwner {
    pub(crate) fn mount_slot_id(self) -> Option<CommandTerminalBodyMountSlotId> {
        (!self.is_sleeping).then_some(CommandTerminalBodyMountSlotId {
            group_id: self.group_id,
            session_id: self.session_id,
        })
    }
}


pub(crate) trait TerminalSurfaceMountSlotKey: Copy + Eq + std::hash::Hash {
    fn terminal_surface_sort_key(self) -> (u8, u64, u64);
}


impl TerminalSurfaceMountSlotKey for AgentsTerminalBodyMountSlotId {
    fn terminal_surface_sort_key(self) -> (u8, u64, u64) {
        (0, self.pane_id.0, self.session_id.0)
    }
}


impl TerminalSurfaceMountSlotKey for CommandTerminalBodyMountSlotId {
    fn terminal_surface_sort_key(self) -> (u8, u64, u64) {
        (1, self.group_id.0, self.session_id.0)
    }
}


impl TerminalSurfaceMountSlotKey for ProjectEditorCompanionTerminalBodyMountSlotId {
    fn terminal_surface_sort_key(self) -> (u8, u64, u64) {
        (2, self.mode.switcher_index(), self.session_id.0)
    }
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalBodyMousePosition {
    pub(crate) x: f64,
    pub(crate) y: f64,
}


pub(crate) fn terminal_body_relative_mouse_position(
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
) -> Option<TerminalBodyMousePosition> {
    if !bounds.contains(&position) {
        return None;
    }

    Some(TerminalBodyMousePosition {
        x: f64::from(position.x.as_f32() - bounds.origin.x.as_f32()),
        y: f64::from(position.y.as_f32() - bounds.origin.y.as_f32()),
    })
}


pub(crate) fn terminal_body_relative_mouse_position_for_slot<MountSlotId: Copy + Eq + std::hash::Hash>(
    bounds_by_slot: &HashMap<MountSlotId, Bounds<Pixels>>,
    slot_id: MountSlotId,
    position: Point<Pixels>,
) -> Option<TerminalBodyMousePosition> {
    terminal_body_relative_mouse_position(*bounds_by_slot.get(&slot_id)?, position)
}


/*
CDXC:GPUITerminalPaste 2026-06-23-09:59:
Terminal paste may read the platform clipboard only at the command-action boundary and may forward only explicit string entries to the exact focused mounted Ghostty surface. Do not use ClipboardItem::text here because it can synthesize local file paths from external-path entries; path, image, metadata-only, and empty clipboard data must no-op without logging, persistence, or fallback text.

CDXC:GPUITerminalClipboard 2026-06-23-19:07:
Runtime clipboard drains may use the app-thread standard clipboard only after the caller has re-fetched the exact currently mounted Ghostty surface owner from the Agents or command surface map. The handoff reads through the explicit-string boundary used by Cmd+V, writes only runtime-provided text as a new string ClipboardItem, leaves selection clipboard unsupported by Ghostty runtime config, and does not log, persist, inspect, or store clipboard content beyond the closure call.

CDXC:GPUICommandTerminalClipboard 2026-06-27-04:10:
Command runtime clipboard drains need source-level regression evidence that requester identity comes from still-mounted command owners, not focused-shell fallback. Keep authorization as the intersection of a snapped owner key and the current mounted-owner map before any app-thread clipboard read or runtime-text write can run.

CDXC:GPUITerminalImagePaste 2026-06-27-10:23:
GPUI command-pane paste needs a pure normalization helper that keeps Paste previewable images disabled behavior identical to explicit-string-only paste, but when enabled converts only validated local image file references or raw clipboard image bytes into numbered Markdown links. Do not call ClipboardItem::text, do not synthesize non-image paths, and do not persist anything except saved raw image bytes under the resolved Ghostex image directory.
*/
pub(crate) fn terminal_clipboard_explicit_string_text(item: &ClipboardItem) -> Option<String> {
    terminal_clipboard_paste_text(item, false, false)
}


pub(crate) fn terminal_clipboard_paste_text(
    item: &ClipboardItem,
    paste_previewable_images_enabled: bool,
    factory_droid_image_padding: bool,
) -> Option<String> {
    if paste_previewable_images_enabled {
        if let Some(markdown) = terminal_clipboard_previewable_image_markdown_text(item) {
            return Some(if factory_droid_image_padding {
                format!("  {markdown}")
            } else {
                markdown
            });
        }
    }

    terminal_clipboard_explicit_string_entries_text(item)
}


pub(crate) fn terminal_clipboard_explicit_string_entries_text(item: &ClipboardItem) -> Option<String> {
    let mut text = String::new();

    for entry in item.entries() {
        if let ClipboardEntry::String(clipboard_string) = entry {
            text.push_str(clipboard_string.text());
        }
    }

    if text.is_empty() { None } else { Some(text) }
}


pub(crate) fn terminal_clipboard_previewable_image_markdown_text(item: &ClipboardItem) -> Option<String> {
    let file_paths = terminal_clipboard_image_file_paths(item);
    if !file_paths.is_empty() {
        return Some(terminal_clipboard_markdown_image_references(&file_paths));
    }

    terminal_clipboard_saved_image_markdown_text(item)
}


pub(crate) fn terminal_clipboard_image_file_paths(item: &ClipboardItem) -> Vec<PathBuf> {
    let external_paths = terminal_clipboard_external_image_file_paths(item);
    if !external_paths.is_empty() {
        return external_paths;
    }

    terminal_clipboard_string_image_file_paths(item)
}


pub(crate) fn terminal_clipboard_external_image_file_paths(item: &ClipboardItem) -> Vec<PathBuf> {
    let mut image_paths = Vec::new();
    let mut seen = HashSet::new();

    for entry in item.entries() {
        if let ClipboardEntry::ExternalPaths(paths) = entry {
            for path in paths.paths() {
                if is_project_board_image_file_path(path) && seen.insert(path.clone()) {
                    image_paths.push(path.clone());
                }
            }
        }
    }

    image_paths
}


pub(crate) fn terminal_clipboard_string_image_file_paths(item: &ClipboardItem) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for entry in item.entries() {
        if let ClipboardEntry::String(clipboard_string) = entry {
            candidates.extend(
                clipboard_string
                    .text()
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty()),
            );
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    let mut image_paths = Vec::new();
    let mut seen = HashSet::new();
    for candidate in candidates {
        let Some(path) = project_board_image_path_from_reference(candidate) else {
            return Vec::new();
        };
        if !is_project_board_image_file_path(&path) {
            return Vec::new();
        }
        if seen.insert(path.clone()) {
            image_paths.push(path);
        }
    }

    image_paths
}


pub(crate) fn terminal_clipboard_saved_image_markdown_text(item: &ClipboardItem) -> Option<String> {
    for entry in item.entries() {
        if let ClipboardEntry::Image(image) = entry {
            let bytes = image.bytes();
            if bytes.is_empty() || bytes.len() > PROJECT_BOARD_CLIPBOARD_IMAGE_MAX_BYTES {
                return None;
            }

            let path =
                unique_project_board_image_path(project_board_image_extension(image.format()))
                    .ok()?;
            let path = terminal_clipboard_absolute_path(path)?;
            fs::write(&path, bytes).ok()?;
            return Some(terminal_clipboard_markdown_image_reference(&path, 1));
        }
    }

    None
}


/*
CDXC:GPUITerminalRemoteImagePaste 2026-08-21:
The remote paste route needs the clipboard image *before* it is written
anywhere, because a remote terminal's reference has to point at a file on the
remote machine. This extractor keeps the exact acceptance order the local
Markdown helper uses (validated image file references first, raw clipboard
image bytes second) so local and remote paste accept and reject the same
clipboard shapes; only the destination differs.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TerminalClipboardImagePayload {
    FilePaths(Vec<PathBuf>),
    Bytes {
        bytes: Vec<u8>,
        extension: &'static str,
    },
}


pub(crate) fn terminal_clipboard_image_payload(item: &ClipboardItem) -> Option<TerminalClipboardImagePayload> {
    let file_paths = terminal_clipboard_image_file_paths(item);
    if !file_paths.is_empty() {
        return Some(TerminalClipboardImagePayload::FilePaths(file_paths));
    }

    for entry in item.entries() {
        if let ClipboardEntry::Image(image) = entry {
            let bytes = image.bytes();
            if bytes.is_empty() || bytes.len() > PROJECT_BOARD_CLIPBOARD_IMAGE_MAX_BYTES {
                return None;
            }

            return Some(TerminalClipboardImagePayload::Bytes {
                bytes: bytes.to_vec(),
                extension: project_board_image_extension(image.format()),
            });
        }
    }

    None
}


pub(crate) fn terminal_clipboard_absolute_path(path: PathBuf) -> Option<PathBuf> {
    if path.is_absolute() {
        Some(path)
    } else {
        env::current_dir()
            .ok()
            .map(|current_dir| current_dir.join(path))
    }
}


pub(crate) fn terminal_clipboard_markdown_image_references(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| terminal_clipboard_markdown_image_reference(path, index + 1))
        .collect::<Vec<_>>()
        .join("\n")
}


pub(crate) fn terminal_clipboard_markdown_image_reference(path: &Path, image_number: usize) -> String {
    format!("[Image #{image_number}]({})", path.to_string_lossy())
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTerminalAttachmentKind {
    Image,
    File,
    Folder,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTerminalAttachmentReference {
    pub(crate) kind: GpuiTerminalAttachmentKind,
    pub(crate) path: String,
}


pub(crate) fn gpui_local_terminal_attachment_reference(
    path: &Path,
) -> Result<GpuiTerminalAttachmentReference, String> {
    let metadata = fs::metadata(path)
        .map_err(|_| "The selected file or folder is no longer available.".to_string())?;
    let kind = if metadata.is_dir() {
        GpuiTerminalAttachmentKind::Folder
    } else if metadata.is_file() && is_project_board_image_file_path(path) {
        GpuiTerminalAttachmentKind::Image
    } else if metadata.is_file() {
        GpuiTerminalAttachmentKind::File
    } else {
        return Err("The selected item is not a file or folder.".to_string());
    };

    #[cfg(target_os = "windows")]
    let referenced_path = if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { .. })
    ) {
        windows_terminal_backend::wsl_path_for_windows_path(path)
            .map_err(|_| "The selected path could not be converted for WSL.".to_string())?
    } else {
        path.to_string_lossy().into_owned()
    };
    #[cfg(not(target_os = "windows"))]
    let referenced_path = path.to_string_lossy().into_owned();

    Ok(GpuiTerminalAttachmentReference {
        kind,
        path: referenced_path,
    })
}


pub(crate) fn gpui_terminal_attachment_markdown_text(
    references: &[GpuiTerminalAttachmentReference],
) -> String {
    let mut image_number = 0usize;
    let mut file_number = 0usize;
    let mut folder_number = 0usize;
    references
        .iter()
        .map(|reference| {
            let (label, number) = match reference.kind {
                GpuiTerminalAttachmentKind::Image => {
                    image_number += 1;
                    ("Image", image_number)
                }
                GpuiTerminalAttachmentKind::File => {
                    file_number += 1;
                    ("File", file_number)
                }
                GpuiTerminalAttachmentKind::Folder => {
                    folder_number += 1;
                    ("Folder", folder_number)
                }
            };
            format!("[{label} #{number}]({})", reference.path)
        })
        .collect::<Vec<_>>()
        .join(" ")
}


pub(crate) fn terminal_runtime_clipboard_read_explicit_string_text(
    read_standard_clipboard: impl FnOnce() -> Option<ClipboardItem>,
) -> Option<String> {
    terminal_runtime_clipboard_read_text(read_standard_clipboard, false)
}


pub(crate) fn terminal_runtime_clipboard_read_text(
    read_standard_clipboard: impl FnOnce() -> Option<ClipboardItem>,
    paste_previewable_images_enabled: bool,
) -> Option<String> {
    read_standard_clipboard().as_ref().and_then(|item| {
        terminal_clipboard_paste_text(item, paste_previewable_images_enabled, false)
    })
}


pub(crate) fn terminal_runtime_clipboard_write_standard_text(
    text: String,
    mut write_standard_clipboard: impl FnMut(ClipboardItem),
) {
    write_standard_clipboard(ClipboardItem::new_string(text));
}


pub(crate) fn terminal_runtime_clipboard_authorized_mounted_slot_ids<SlotId, Owner>(
    snapshot_slot_ids: impl IntoIterator<Item = SlotId>,
    owners_by_slot: &HashMap<SlotId, Owner>,
) -> Vec<SlotId>
where
    SlotId: Copy + Eq + std::hash::Hash,
{
    snapshot_slot_ids
        .into_iter()
        .filter(|slot_id| owners_by_slot.contains_key(slot_id))
        .collect()
}


/*
CDXC:GPUITerminalTextInput 2026-06-23-10:13:
Phase 2 terminal input parity forwards only committed GPUI `key_char` text. Do not synthesize from `key`, do not map physical keys without a native keycode, reject Cmd/Super and Control modified events so shortcuts and control-key terminal bindings can use a later Ghostty key-event bridge, and keep Option-generated characters when GPUI has already committed them as text.

CDXC:GPUITerminalTextInput 2026-06-23-10:18:
Focused text delivery must choose an explicit terminal target before touching a surface: Agents requires Agents mode plus an Agents-pane focus target, command requires command-pane focus, and Browser/project-editor focus must no-op instead of falling through to a terminal helper.

CDXC:GPUITerminalTextInput 2026-06-23-10:45:
Terminal IME/preedit delivery is a text-service path, not a keyboard fallback. GPUI may register input handling only for the exact focused mounted terminal body, then send committed text/preedit bytes to the matching Ghostty owner while retaining only sanitized UTF-16 marked ranges and no raw typed or terminal content.

CDXC:GPUITerminalKeyInput 2026-06-23-11:50:
The local GPUI app event API exposes `KeyDownEvent { keystroke, is_held, prefer_character_input }`; `Keystroke` exposes only modifiers, layout-derived `key`, and committed `key_char`. The macOS backend uses `NSEvent.keyCode()` while constructing `Keystroke` but drops that native keycode before app listeners run, so Ghostty physical-key forwarding must remain blocked until GPUI exposes a stable native keycode or UIEvents-code physical key identity.

CDXC:GPUITerminalKeyInput 2026-06-23-14:23:
Committed `key_char` text is a text-input signal, not evidence for physical-key or key-binding parity. Layout `key` values, Control shortcuts, and Cmd/Super shortcuts must stay rejected by this helper until GPUI can pass a stable native keycode or UIEvents-code identity to Ghostty without guessing.
*/
pub(crate) fn committed_terminal_text_from_key_down_event(event: &KeyDownEvent) -> Option<&str> {
    committed_terminal_text_from_keystroke(&event.keystroke)
}


pub(crate) fn committed_terminal_text_from_keystroke(keystroke: &Keystroke) -> Option<&str> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.control {
        return None;
    }

    let text = keystroke.key_char.as_deref()?;
    if text.is_empty() { None } else { Some(text) }
}


pub(crate) fn command_pane_sleeping_placeholder_keystroke_requests_wake(keystroke: &Keystroke) -> bool {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-25-14:49:
    Sleeping command placeholders wake on plain alphanumeric key-downs like native AppKit. Reject Cmd, Control, and Option/Alt modified keys, and use GPUI's layout key only as a wake-affordance identity for shifted digits rather than as terminal input.

    CDXC:GPUICommandSleepingPlaceholder 2026-06-25-19:02:
    Native AppKit rejects Function-modified wake keys before inspecting alphanumeric text. GPUI must keep Function inert too, while still allowing plain and Shift-only letters/digits that AppKit treats as `charactersIgnoringModifiers`.
    */
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.control || modifiers.alt || modifiers.function {
        return false;
    }

    command_pane_sleeping_placeholder_wake_text_is_alphanumeric(keystroke.key_char.as_deref())
        || command_pane_sleeping_placeholder_wake_text_is_alphanumeric(Some(&keystroke.key))
}


pub(crate) fn command_pane_sleeping_placeholder_wake_text_is_alphanumeric(text: Option<&str>) -> bool {
    let Some(text) = text else {
        return false;
    };
    let mut chars = text.chars();
    let Some(character) = chars.next() else {
        return false;
    };
    chars.next().is_none() && character.is_ascii_alphanumeric()
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedTerminalTextTarget {
    Agents,
    Command,
    ProjectEditorCompanion,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedTerminalTextMountTarget {
    Agents(AgentsTerminalBodyMountSlotId),
    Command(CommandTerminalBodyMountSlotId),
    ProjectEditorCompanion(ProjectEditorCompanionTerminalBodyMountSlotId),
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalTextMarkedRange {
    pub(crate) target: FocusedTerminalTextMountTarget,
    pub(crate) range: Range<usize>,
}


pub(crate) fn focused_terminal_text_target(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
) -> Option<FocusedTerminalTextTarget> {
    match shell_focus {
        ShellFocusTarget::AgentsPane(_) if active_mode == TitlebarMode::Agents => {
            Some(FocusedTerminalTextTarget::Agents)
        }
        ShellFocusTarget::CommandPane => Some(FocusedTerminalTextTarget::Command),
        ShellFocusTarget::ProjectEditorCompanion(mode)
            if active_mode == mode && mode.is_project_editor_mode() =>
        {
            Some(FocusedTerminalTextTarget::ProjectEditorCompanion)
        }
        ShellFocusTarget::AgentsPane(_)
        | ShellFocusTarget::BrowserSurface
        | ShellFocusTarget::BrowserPane(_)
        | ShellFocusTarget::ProjectEditorSurface(_)
        | ShellFocusTarget::ProjectEditorCompanion(_) => None,
    }
}


pub(crate) fn terminal_text_marked_range_for_preedit(
    replacement_range: Option<Range<usize>>,
    new_text: &str,
) -> Option<Range<usize>> {
    let marked_len_utf16 = new_text.encode_utf16().count();
    if marked_len_utf16 == 0 {
        return None;
    }

    let marked_start = replacement_range.map_or(0, |range| range.start);
    Some(marked_start..marked_start.saturating_add(marked_len_utf16))
}


pub(crate) fn terminal_ime_bounds_from_ghostty_point(
    body_bounds: Bounds<Pixels>,
    ime_point: terminal_ghostty_surface::GhosttySurfaceImePoint,
) -> Option<Bounds<Pixels>> {
    let x = ime_point.x as f32;
    let y = ime_point.y as f32;
    let width = ime_point.width as f32;
    let height = ime_point.height as f32;

    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || width < 0.0
        || height < 0.0
    {
        return None;
    }

    Some(Bounds::new(
        gpui::point(body_bounds.origin.x + px(x), body_bounds.origin.y + px(y)),
        size(px(width), px(height)),
    ))
}


/*
CDXC:GPUITerminalMouseModifiers 2026-06-23-09:45:
Mounted Agents and command terminal bodies must translate GPUI mouse-event keyboard modifiers into Ghostty input.Mods bits for pointer position and button events. Map shift, control, alt, and platform to Ghostty shift, ctrl, alt, and super while intentionally ignoring function; scroll events keep Ghostty ScrollMods precision-only and forward keyboard modifiers only through the preceding pointer position update.
*/
pub(crate) fn ghostty_mouse_mods_from_gpui_modifiers(
    modifiers: Modifiers,
) -> ghostty_kit::ffi::ghostty_input_mods_e {
    let mut mods = GHOSTTY_MOUSE_ZERO_MODS;

    if modifiers.shift {
        mods |= GHOSTTY_MOUSE_SHIFT_MOD;
    }
    if modifiers.control {
        mods |= GHOSTTY_MOUSE_CTRL_MOD;
    }
    if modifiers.alt {
        mods |= GHOSTTY_MOUSE_ALT_MOD;
    }
    if modifiers.platform {
        mods |= GHOSTTY_MOUSE_SUPER_MOD;
    }

    mods
}


/*
CDXC:GPUITerminalMouseButtons 2026-06-23-10:23:
Mounted Agents and command terminal bodies must forward Ghostty's exact left, right, and middle mouse button values while rejecting GPUI navigation buttons. Keep the mapper pure so button parity does not store raw input state, coordinates, modifiers, terminal content, command text, paths, URLs, or titles.

CDXC:GPUITerminalMouseButtons 2026-06-23-12:42:
The bounded non-left parity audit keeps the existing implementation: mounted Agents and command terminal bodies use this shared mapper for right/middle press, in-body release, and capture-gated body-level mouse-up-out while preserving current-slot, recorded-bounds, exact-surface/runtime, and mapped-modifier gates.
*/
pub(crate) fn ghostty_mouse_button_from_gpui_button(
    button: MouseButton,
) -> Option<ghostty_kit::ffi::ghostty_input_mouse_button_e> {
    match button {
        MouseButton::Left => Some(ghostty_kit::ffi::GHOSTTY_MOUSE_LEFT),
        MouseButton::Right => Some(ghostty_kit::ffi::GHOSTTY_MOUSE_RIGHT),
        MouseButton::Middle => Some(ghostty_kit::ffi::GHOSTTY_MOUSE_MIDDLE),
        MouseButton::Navigate(_) => None,
    }
}


/*
CDXC:GPUITerminalPressureForwarding 2026-06-23-09:51:
Mounted Agents and command terminal pressure events must preserve the GPUI stage contract when crossing into Ghostty. Map Zero to none, Normal to normal, and Force to deep without clamping, fallback stages, logging, persistence, or raw input storage.
*/
pub(crate) fn ghostty_mouse_pressure_stage_from_gpui_stage(stage: PressureStage) -> u32 {
    match stage {
        PressureStage::Zero => GHOSTTY_MOUSE_PRESSURE_STAGE_NONE,
        PressureStage::Normal => GHOSTTY_MOUSE_PRESSURE_STAGE_NORMAL,
        PressureStage::Force => GHOSTTY_MOUSE_PRESSURE_STAGE_DEEP,
    }
}


/*
CDXC:GPUITerminalScrollForwarding 2026-06-23-09:32:
Mounted Running Agents terminal bodies must forward wheel deltas to Ghostty without inventing fallback behavior.

CDXC:GPUICommandTerminalInputForwarding 2026-06-23-09:41:
Agents and command terminal bodies share the same wheel-delta conversion: pixel deltas use raw GPUI pixels and set Ghostty ScrollMods precision bit 0, line deltas use raw GPUI lines with zero scroll mods, and keyboard modifiers are forwarded through mouse position input instead of encoded into ScrollMods.
*/
pub(crate) fn terminal_ghostty_scroll_delta(
    delta: ScrollDelta,
) -> (f64, f64, ghostty_kit::ffi::ghostty_input_scroll_mods_t) {
    match delta {
        ScrollDelta::Pixels(delta) => (
            f64::from(delta.x.as_f32()),
            f64::from(delta.y.as_f32()),
            GHOSTTY_SCROLL_PRECISION_MOD,
        ),
        ScrollDelta::Lines(delta) => (f64::from(delta.x), f64::from(delta.y), 0),
    }
}


pub(crate) fn agents_terminal_body_relative_mouse_position_for_slot(
    bounds_by_slot: &HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    slot_id: AgentsTerminalBodyMountSlotId,
    position: Point<Pixels>,
) -> Option<TerminalBodyMousePosition> {
    terminal_body_relative_mouse_position_for_slot(bounds_by_slot, slot_id, position)
}


pub(crate) fn command_terminal_body_relative_mouse_position_for_slot(
    bounds_by_slot: &HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
    slot_id: CommandTerminalBodyMountSlotId,
    position: Point<Pixels>,
) -> Option<TerminalBodyMousePosition> {
    terminal_body_relative_mouse_position_for_slot(bounds_by_slot, slot_id, position)
}


/*
CDXC:GPUILibghosttyMountBoundary 2026-06-22-20:14:
Phase 2 libghostty parity records the pane id and selected session id for each rendered Agents terminal body before any real surface exists. Keep the boundary explicit so sleeping, mounting, failed-startup, restored/unmounted, popped-out, missing-session, inactive running, and non-focused running tabs stay classified instead of receiving fake or fallback surfaces.

CDXC:GPUILibghosttyMountBoundary 2026-06-22-20:14:
The future libghostty mount slot is the normal WorkspaceLeaf body child below the Agents tab bar. Keep that body as a non-overlapping layout sibling with no hidden hit regions, transparent overlays, root hit-test routing, or synthetic coordinate routing.

CDXC:GPUILibghosttyMountBoundary 2026-06-22-22:45:
All rendered visible Agents leaves whose selected session is Running are real terminal mount slots. Focus mode limits the rendered leaf set naturally, inactive tabs stay hidden, and sleeping/restored/mounting/failed-startup/popped-out/missing selections keep placeholder bodies without fake surfaces or extra hit regions.

CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
Command-pane terminal mount slots are runtime-only group/session body identities, separate from Agents pane/session ids and shell persistence. Only expanded visible active command bodies may record bounds or mount surfaces; inactive command tabs, collapsed panes, missing sessions, titles, status labels, project paths, commands, env, input, and terminal content must not become launch payload or durable state.

CDXC:GPUITerminalMouseForwarding 2026-06-23-08:32:
Running Agents terminal mouse input is accepted only through the current body mount slot and the recorded body rectangle. Missing or stale bounds produce a no-op so focus and placeholder activation semantics stay intact without adding overlays, hidden hit regions, root/window hit-test routing, synthetic coordinate routing, logs, persistence, or raw input storage.

CDXC:GPUITerminalScrollForwarding 2026-06-23-09:32:
Running Agents body scroll input uses the same current-slot and recorded-body-boundary gate as mouse movement and buttons. Forwarding updates Ghostty's body-relative pointer position first, then sends the wheel delta only to the exact current surface without overlays, hidden hit regions, root/window routing, coordinate rerouting, logging, persistence, or placeholder changes.

CDXC:GPUICommandTerminalInputForwarding 2026-06-23-09:41:
Mounted command-pane terminal bodies mirror Running Agents mouse and scroll forwarding through the normal body element only. Current command mount slots use recorded body bounds, body-relative pointer coordinates, mapped mouse modifier bits, exact Ghostty surface identity, and runtime-only state while preserving command-pane focus/drop ownership without overlays, hidden hit regions, input routing, logging, persistence, or raw input storage.

CDXC:GPUITerminalSelectionDrag 2026-06-23-12:43:
Ghostty owns terminal selection state; GPUI must not store selection text, raw drag coordinates, or per-terminal drag state. Mounted Agents and command terminal body selection is represented by the normal body-scoped Ghostty event stream: button press, body-relative pointer moves including press-held moves delivered by the body, and button release. Outside-body release stays capture-gated only, with no transparent overlay, hidden hit region, broad hit-test routing, root/window pre-dispatch routing, global capture, or synthetic coordinate routing.

CDXC:GPUITerminalPressureForwarding 2026-06-23-09:51:
Mounted Agents and command terminal pressure input uses the same current-slot, recorded-body-bounds, exact-surface, and macOS gates as pointer and scroll forwarding. Forward body-relative pointer position with mapped modifiers first, then pass the GPUI pressure value and mapped pressure stage to Ghostty without clamping, fallback behavior, logging, persistence, coordinate routing, or placeholder changes.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentsTerminalBodyPresentation {
    MountSlot,
    RunningPlaceholder,
    LifecyclePlaceholder,
    MissingSessionPlaceholder,
    EmptyWorkspacePlaceholder,
}


impl AgentsTerminalBodyPresentation {
    pub(crate) fn element_slug(
        self,
        presentation_state: Option<TerminalSessionPresentationState>,
    ) -> &'static str {
        match self {
            Self::MountSlot => "libghostty-mount-slot",
            Self::RunningPlaceholder => "running-black-placeholder",
            Self::LifecyclePlaceholder => presentation_state
                .map(TerminalSessionPresentationState::element_slug)
                .unwrap_or("lifecycle-placeholder"),
            Self::MissingSessionPlaceholder => "missing-session-placeholder",
            Self::EmptyWorkspacePlaceholder => "empty-workspace-placeholder",
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentsTerminalBodyClickAction {
    FocusRunningMountSlot(AgentsTerminalBodyMountSlotId),
    ActivatePlaceholder {
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    },
    None,
}


pub(crate) fn agents_terminal_body_click_action(
    mount_candidate: AgentsTerminalBodyMountCandidate,
    fallback_session_id: TerminalSessionId,
) -> AgentsTerminalBodyClickAction {
    /*
    CDXC:GPUTerminalAppKitFocus 2026-06-22-23:11:
    Clicking an eligible running Agents terminal body must focus the shell pane and then the real Ghostty/AppKit host surface for that mounted slot. Non-running placeholders keep the existing activation path so sleeping/restored/popped-out/failed-startup tabs remain explicit wake/materialize/reattach/retry actions and Mounting/missing states do not fabricate a terminal.
    */
    if mount_candidate.active_session_id.is_none() {
        AgentsTerminalBodyClickAction::None
    } else if let Some(slot_id) = mount_candidate.mount_slot_id() {
        AgentsTerminalBodyClickAction::FocusRunningMountSlot(slot_id)
    } else {
        AgentsTerminalBodyClickAction::ActivatePlaceholder {
            pane_id: mount_candidate.pane_id,
            session_id: fallback_session_id,
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalBodyMountCandidate {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) active_session_id: Option<TerminalSessionId>,
    pub(crate) presentation: AgentsTerminalBodyPresentation,
}


impl AgentsTerminalBodyMountCandidate {
    pub(crate) fn eligible_for_terminal_surface(self) -> bool {
        matches!(self.presentation, AgentsTerminalBodyPresentation::MountSlot)
    }

    pub(crate) fn mount_slot_id(self) -> Option<AgentsTerminalBodyMountSlotId> {
        if !self.eligible_for_terminal_surface() {
            return None;
        }

        Some(AgentsTerminalBodyMountSlotId {
            pane_id: self.pane_id,
            session_id: self.active_session_id?,
        })
    }

    pub(crate) fn renders_placeholder_child(self) -> bool {
        matches!(
            self.presentation,
            AgentsTerminalBodyPresentation::LifecyclePlaceholder
                | AgentsTerminalBodyPresentation::MissingSessionPlaceholder
        )
    }
}


pub(crate) fn selected_agents_terminal_body_mount_candidate(
    leaf: &WorkspaceLeaf,
    terminal_sessions: &[TerminalSession],
    rendered_leaf_order: &[WorkspacePaneId],
) -> AgentsTerminalBodyMountCandidate {
    let active_session_id = leaf.tab_group.active_session_id();
    let active_session = active_session_id.and_then(|session_id| {
        terminal_sessions
            .iter()
            .find(|session| session.id == session_id)
    });
    let presentation_state = active_session.map(|session| session.presentation_state);
    let pane_is_rendered = rendered_leaf_order.contains(&leaf.pane_id);
    let presentation = match presentation_state {
        Some(TerminalSessionPresentationState::Running) if pane_is_rendered => {
            AgentsTerminalBodyPresentation::MountSlot
        }
        Some(TerminalSessionPresentationState::Running) => {
            AgentsTerminalBodyPresentation::RunningPlaceholder
        }
        Some(_) => AgentsTerminalBodyPresentation::LifecyclePlaceholder,
        None if active_session_id.is_none() => {
            AgentsTerminalBodyPresentation::EmptyWorkspacePlaceholder
        }
        None => AgentsTerminalBodyPresentation::MissingSessionPlaceholder,
    };

    AgentsTerminalBodyMountCandidate {
        pane_id: leaf.pane_id,
        active_session_id,
        presentation,
    }
}


#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct WorkspaceSplit {
    pub(crate) id: WorkspaceSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) ratio: f32,
    pub(crate) default_ratio: f32,
    pub(crate) first: Box<WorkspaceNode>,
    pub(crate) second: Box<WorkspaceNode>,
}


#[allow(dead_code)]
#[derive(Clone)]
pub(crate) enum WorkspaceNode {
    Split(WorkspaceSplit),
    Leaf(WorkspaceLeaf),
}


pub(crate) fn workspace_terminal_session_mapping_get(
    key: &GpuiWorkspaceTerminalSessionKey,
    local_workspace_session_mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    remote_attach_sessions: &HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
) -> Option<TerminalSessionId> {
    match key {
        GpuiWorkspaceTerminalSessionKey::Local(key) => {
            local_workspace_session_mappings.get(key).copied()
        }
        GpuiWorkspaceTerminalSessionKey::Remote(key) => remote_attach_sessions.get(key).copied(),
    }
}


pub(crate) fn workspace_terminal_session_mapping_insert(
    key: GpuiWorkspaceTerminalSessionKey,
    shell_session_id: TerminalSessionId,
    local_workspace_session_mappings: &mut HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    remote_attach_sessions: &mut HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
) {
    match key {
        GpuiWorkspaceTerminalSessionKey::Local(key) => {
            local_workspace_session_mappings.insert(key, shell_session_id);
        }
        GpuiWorkspaceTerminalSessionKey::Remote(key) => {
            remote_attach_sessions.insert(key, shell_session_id);
        }
    }
}


pub(crate) fn workspace_terminal_session_mapping_remove(
    key: &GpuiWorkspaceTerminalSessionKey,
    local_workspace_session_mappings: &mut HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    remote_attach_sessions: &mut HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
) {
    match key {
        GpuiWorkspaceTerminalSessionKey::Local(key) => {
            local_workspace_session_mappings.remove(key);
        }
        GpuiWorkspaceTerminalSessionKey::Remote(key) => {
            remote_attach_sessions.remove(key);
        }
    }
}


pub(crate) fn collect_command_tabs(
    node: &CommandPaneNode,
    tabs: &mut Vec<(CommandPaneGroupId, CommandSessionId)>,
) {
    match node {
        CommandPaneNode::Leaf(leaf) => {
            tabs.extend(
                leaf.tab_group
                    .tabs
                    .iter()
                    .map(|tab| (leaf.group_id, tab.session_id)),
            );
        }
        CommandPaneNode::Split(split) => {
            collect_command_tabs(&split.first, tabs);
            collect_command_tabs(&split.second, tabs);
        }
    }
}


pub(crate) fn first_command_leaf(node: &CommandPaneNode) -> Option<&CommandPaneLeaf> {
    match node {
        CommandPaneNode::Leaf(leaf) if !leaf.tab_group.tabs.is_empty() => Some(leaf),
        CommandPaneNode::Leaf(_) => None,
        CommandPaneNode::Split(split) => {
            first_command_leaf(&split.first).or_else(|| first_command_leaf(&split.second))
        }
    }
}


pub(crate) fn first_command_leaf_id(node: &CommandPaneNode) -> Option<CommandPaneGroupId> {
    first_command_leaf(node).map(|leaf| leaf.group_id)
}


pub(crate) fn find_command_leaf(
    node: &CommandPaneNode,
    group_id: CommandPaneGroupId,
) -> Option<&CommandPaneLeaf> {
    match node {
        CommandPaneNode::Leaf(leaf) => (leaf.group_id == group_id).then_some(leaf),
        CommandPaneNode::Split(split) => find_command_leaf(&split.first, group_id)
            .or_else(|| find_command_leaf(&split.second, group_id)),
    }
}


pub(crate) fn find_command_leaf_mut(
    node: &mut CommandPaneNode,
    group_id: CommandPaneGroupId,
) -> Option<&mut CommandPaneLeaf> {
    match node {
        CommandPaneNode::Leaf(leaf) => (leaf.group_id == group_id).then_some(leaf),
        CommandPaneNode::Split(split) => find_command_leaf_mut(&mut split.first, group_id)
            .or_else(|| find_command_leaf_mut(&mut split.second, group_id)),
    }
}


pub(crate) fn find_command_split(
    node: &CommandPaneNode,
    split_id: CommandPaneSplitId,
) -> Option<&CommandPaneSplit> {
    match node {
        CommandPaneNode::Leaf(_) => None,
        CommandPaneNode::Split(split) => {
            if split.id == split_id {
                Some(split)
            } else {
                find_command_split(&split.first, split_id)
                    .or_else(|| find_command_split(&split.second, split_id))
            }
        }
    }
}


pub(crate) fn find_command_split_mut(
    node: &mut CommandPaneNode,
    split_id: CommandPaneSplitId,
) -> Option<&mut CommandPaneSplit> {
    match node {
        CommandPaneNode::Leaf(_) => None,
        CommandPaneNode::Split(split) => {
            if split.id == split_id {
                Some(split)
            } else {
                find_command_split_mut(&mut split.first, split_id)
                    .or_else(|| find_command_split_mut(&mut split.second, split_id))
            }
        }
    }
}


pub(crate) fn command_node_contains_group(node: &CommandPaneNode, group_id: CommandPaneGroupId) -> bool {
    find_command_leaf(node, group_id).is_some()
}


pub(crate) fn insert_command_leaf_split(
    node: &mut CommandPaneNode,
    target_group_id: CommandPaneGroupId,
    new_leaf: CommandPaneLeaf,
    axis: WorkspaceSplitAxis,
    dragged_first: bool,
    split_id: CommandPaneSplitId,
) -> bool {
    let should_rebalance_default_axis_chain =
        command_insert_target_axis_chain_uses_native_default_ratios(node, target_group_id, axis);
    let inserted = insert_command_leaf_split_inner(
        node,
        target_group_id,
        new_leaf,
        axis,
        dragged_first,
        split_id,
    );
    if inserted && should_rebalance_default_axis_chain {
        rebalance_command_split_axis_chain_containing_group(node, target_group_id, axis);
    }
    inserted
}


pub(crate) fn insert_command_leaf_split_inner(
    node: &mut CommandPaneNode,
    target_group_id: CommandPaneGroupId,
    new_leaf: CommandPaneLeaf,
    axis: WorkspaceSplitAxis,
    dragged_first: bool,
    split_id: CommandPaneSplitId,
) -> bool {
    match node {
        CommandPaneNode::Leaf(leaf) if leaf.group_id == target_group_id => {
            let existing = std::mem::replace(node, command_pane_dummy_node());
            let new_node = CommandPaneNode::Leaf(new_leaf);
            let (first, second) = if dragged_first {
                (new_node, existing)
            } else {
                (existing, new_node)
            };

            *node = CommandPaneNode::Split(CommandPaneSplit {
                id: split_id,
                axis,
                ratio: 0.5,
                first: Box::new(first),
                second: Box::new(second),
            });
            true
        }
        CommandPaneNode::Leaf(_) => false,
        CommandPaneNode::Split(split) => {
            let target_in_first = command_node_contains_group(&split.first, target_group_id);
            let target_in_second = command_node_contains_group(&split.second, target_group_id);
            if split.axis == axis && (target_in_first || target_in_second) {
                insert_command_leaf_at_same_axis_split(
                    split,
                    target_in_first,
                    new_leaf,
                    dragged_first,
                    split_id,
                );
                true
            } else if target_in_first {
                insert_command_leaf_split_inner(
                    &mut split.first,
                    target_group_id,
                    new_leaf,
                    axis,
                    dragged_first,
                    split_id,
                )
            } else {
                insert_command_leaf_split_inner(
                    &mut split.second,
                    target_group_id,
                    new_leaf,
                    axis,
                    dragged_first,
                    split_id,
                )
            }
        }
    }
}


pub(crate) fn insert_command_leaf_at_same_axis_split(
    split: &mut CommandPaneSplit,
    target_in_first: bool,
    new_leaf: CommandPaneLeaf,
    dragged_first: bool,
    split_id: CommandPaneSplitId,
) {
    /*
    CDXC:GPUICommandPaneSplits 2026-06-25-16:18:
    Native same-direction command split insertion happens beside the matching child of the existing split, not inside that child. Preserve that boundary in GPUI's binary model so explicit first-child ratios keep native meaning when the user later inserts before or after a command pane in a resized split.
    */
    let existing_first = take_command_node(&mut split.first);
    let existing_second = take_command_node(&mut split.second);
    let new_node = CommandPaneNode::Leaf(new_leaf);
    let axis = split.axis;

    let (first, second_first, second_second) = match (target_in_first, dragged_first) {
        (true, true) => (new_node, existing_first, existing_second),
        (true, false) => (existing_first, new_node, existing_second),
        (false, true) => (existing_first, new_node, existing_second),
        (false, false) => (existing_first, existing_second, new_node),
    };

    split.first = Box::new(first);
    split.second = Box::new(CommandPaneNode::Split(CommandPaneSplit {
        id: split_id,
        axis,
        ratio: 0.5,
        first: Box::new(second_first),
        second: Box::new(second_second),
    }));
}


pub(crate) fn command_insert_target_axis_chain_uses_native_default_ratios(
    node: &CommandPaneNode,
    target_group_id: CommandPaneGroupId,
    axis: WorkspaceSplitAxis,
) -> bool {
    match node {
        CommandPaneNode::Leaf(leaf) => leaf.group_id == target_group_id,
        CommandPaneNode::Split(split) => {
            let target_in_first = command_node_contains_group(&split.first, target_group_id);
            let target_in_second = command_node_contains_group(&split.second, target_group_id);
            if !target_in_first && !target_in_second {
                return false;
            }

            if split.axis == axis {
                command_split_axis_tree_uses_native_default_ratios(node, axis)
            } else if target_in_first {
                command_insert_target_axis_chain_uses_native_default_ratios(
                    &split.first,
                    target_group_id,
                    axis,
                )
            } else {
                command_insert_target_axis_chain_uses_native_default_ratios(
                    &split.second,
                    target_group_id,
                    axis,
                )
            }
        }
    }
}


pub(crate) fn command_split_axis_tree_uses_native_default_ratios(
    node: &CommandPaneNode,
    axis: WorkspaceSplitAxis,
) -> bool {
    match node {
        CommandPaneNode::Leaf(_) => true,
        CommandPaneNode::Split(split) => {
            (split.axis != axis || command_split_ratio_matches_native_default(split))
                && command_split_axis_tree_uses_native_default_ratios(&split.first, axis)
                && command_split_axis_tree_uses_native_default_ratios(&split.second, axis)
        }
    }
}


pub(crate) fn command_split_ratio_matches_native_default(split: &CommandPaneSplit) -> bool {
    let Some(expected_ratio) = command_split_native_default_ratio(split) else {
        return false;
    };
    (workspace_split_ratio(split.ratio) - expected_ratio).abs() < 0.001
}


pub(crate) fn command_split_native_default_ratio(split: &CommandPaneSplit) -> Option<f32> {
    let first_count = command_node_leaf_count(&split.first);
    let second_count = command_node_leaf_count(&split.second);
    let total_count = first_count + second_count;
    if total_count < 2 {
        return None;
    }
    Some(workspace_split_ratio(
        first_count as f32 / total_count as f32,
    ))
}


pub(crate) fn command_node_leaf_count(node: &CommandPaneNode) -> usize {
    match node {
        CommandPaneNode::Leaf(leaf) => usize::from(!leaf.tab_group.tabs.is_empty()),
        CommandPaneNode::Split(split) => {
            command_node_leaf_count(&split.first) + command_node_leaf_count(&split.second)
        }
    }
}

