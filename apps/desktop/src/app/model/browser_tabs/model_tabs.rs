use super::*;
use crate::*;

impl BrowserTabModel {
    pub(crate) fn shell_address_only_with_profile(profile_id: BrowserProfileId) -> Self {
        let mut model = Self::shell_default_with_profile(profile_id);
        let first_tab_id = model.active_tab;
        let _ = model.close_tab(first_tab_id, profile_id, &[]);
        model
    }

    pub(crate) fn shell_default_with_profile(profile_id: BrowserProfileId) -> Self {
        /*
        CDXC:Browser 2026-06-22-05:56:
        Browser project-editor mode needs native shell-level tab identity before durable browser behavior exists. Keep tab ids, active tab, title/url, and address-only placeholder state in memory, while runtime CEF entities are owned separately by loaded tab id.

        CDXC:Browser 2026-06-22-11:05:
        Runtime favicon images and HTTP(S) fetch sources belong to tab records but remain transient metadata alongside CEF entities, so shell defaults and restoration must initialize them empty and persistence must keep only sanitized tab URL/history state.

        CDXC:Browser 2026-06-22-06:59:
        Browser tabs now own runtime CEF entities by tab id, while this persisted shell model remains limited to sanitized tab metadata. Address-only tabs keep no URL and should render an empty GPUI body rather than borrowing stale page content from another tab.

        CDXC:Browser 2026-06-22-19:52:
        Fresh Browser shell state must use only the static default URL until a real sidebar/project snapshot contract carries an explicit browser start URL. GPUI must not infer Browser project start URLs from .git, paths, workspace names, fixture names, or sidebar titles.

        CDXC:Browser 2026-06-22-07:14:
        Page-initiated target=_blank and window.open requests should become selected GPUI Browser shell tabs for the requested URL, reusing the same per-tab CEF surface creation path as address-bar navigation. The shell model can keep the raw runtime URL in memory, but persistence must continue using the existing Browser metadata sanitizer.

        CDXC:Browser 2026-06-23-11:43:
        Popup parity is explicit: only non-empty target URLs create Browser tabs. Empty CEF targets, including script-created blank popups with no transferable URL/content, are handled as no-ops without address-only tab creation, CEF surface creation, shell-state persistence, notification, import, or content-transfer fallback.

        CDXC:Browser 2026-06-22-09:02:
        Browser tabs now need shell-owned pane groups and left/right/top/bottom split order before full multi-CEF rendering exists. Keep BrowserTabId metadata and per-tab CEF ownership in one registry, while split leaves store only tab ids plus active selection so drag grouping and splitting never recreate Browser surfaces or persist raw page titles.

        CDXC:Browser 2026-06-22-09:55:
        Browser split panes should show the existing loaded CEF surface for each rendered leaf's active tab when Browser is awake and drags are not hiding native views. Restored or inactive loaded tabs without an existing CEF entity render restored/sleeping placeholder bodies until normal selection or wake materializes them, and address-only tabs never borrow another tab's surface.

        CDXC:Browser 2026-06-23-11:14:
        Fresh and restored GPUI Browser tabs are assigned a generated shell profile id at model construction. That id is runtime-safe profile plumbing for future CEF surface creation and is intentionally separate from sanitized URL/history persistence.
        */
        let default_url = browser_shell_default_url(None);
        let first_tab = BrowserTab {
            remote_machine_id: None,
            id: BrowserTabId(1),
            notification_target: uuid::Uuid::new_v4(),
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
            device_view: BrowserDeviceView::default(),
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

    pub(crate) fn select_tab_in_pane(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
    ) -> bool {
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

    pub(crate) fn find_renderer_open_reuse_tab(
        &self,
        url: &str,
        reuse: GpuiBrowserRendererOpenReuse,
        remote_machine_id: Option<&str>,
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
            .find(|tab| tab.remote_machine_id.as_deref() == remote_machine_id && tab.url == url)
            .map(|tab| tab.id);
        let tab_id = match exact {
            Some(tab_id) => Some(tab_id),
            None if reuse == GpuiBrowserRendererOpenReuse::Exact => None,
            None => {
                let origin = browser_url_origin_key(url)?;
                self.tabs
                    .iter()
                    .find(|tab| {
                        tab.remote_machine_id.as_deref() == remote_machine_id
                            && browser_url_origin_key(&tab.url).as_deref() == Some(&origin)
                    })
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
        tab.runtime_favicon_image =
            browser_favicon_cache_lookup(&url, tab.remote_machine_id.as_deref());
        tab.runtime_favicon_fetch = None;
        tab.runtime_is_loading = true;
        tab.runtime_can_go_back = false;
        tab.runtime_can_go_forward = false;
        tab.navigation_history.append_loaded_url(&url);
        tab.url = url;
        tab.state = BrowserTabState::Loaded;
        Some((tab.id, tab.profile_id))
    }

    pub(crate) fn set_tab_device_view(
        &mut self,
        tab_id: BrowserTabId,
        device_view: BrowserDeviceView,
    ) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return false;
        };
        if tab.device_view == device_view {
            return false;
        }
        tab.device_view = device_view;
        true
    }

    pub(crate) fn set_tab_profile(
        &mut self,
        tab_id: BrowserTabId,
        profile_id: BrowserProfileId,
    ) -> bool {
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

    pub(crate) fn record_page_address_change(&mut self, tab_id: BrowserTabId, url: String) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return false;
        };
        let url = url.trim().to_string();
        let title = browser_tab_title_for_url(&url);
        let history_changed = tab.navigation_history.record_address_change(&url);
        let cached_favicon_image =
            browser_favicon_cache_lookup(&url, tab.remote_machine_id.as_deref());
        let changed = tab.url != url
            || tab.title != title
            || tab.runtime_page_title.is_some()
            || tab.runtime_favicon_url.is_some()
            || tab.runtime_favicon_image != cached_favicon_image
            || tab.runtime_favicon_fetch.is_some()
            || tab.state != BrowserTabState::Loaded
            || history_changed;
        tab.title = title;
        tab.runtime_page_title = None;
        tab.runtime_favicon_url = None;
        tab.runtime_favicon_image = cached_favicon_image;
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
        let cache_key = browser_favicon_cache_key(&tab.url, tab.remote_machine_id.as_deref());
        if let (Some(cache_key), Some(favicon_image)) = (cache_key.as_deref(), &favicon_image) {
            browser_favicon_cache_store_image(cache_key, favicon_image);
        }
        // Remote HTTP favicons are fetched through the tab's SSH route, never GPUI's local HTTP client.
        let favicon_fetch = favicon_fetch
            .filter(|_| tab.remote_machine_id.is_none())
            .map(|source| BrowserFaviconFetchSource {
                cache_key: cache_key.clone(),
                ..source
            });
        // An HTTP(S) favicon has no bytes yet: the cached icon stays up while it loads.
        let favicon_image = favicon_image.or_else(|| {
            favicon_url
                .is_some()
                .then(|| tab.runtime_favicon_image.clone())
                .flatten()
        });
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

    /// CDXC:Browser 2026-09-24 WHY:
    /// A link opened from Linear or another website view used to append beside the Browser's unused empty placeholder. That placeholder stayed hidden until selecting the link, when an extra New Tab appeared. Reuse the sole empty placeholder for page popups; explicit tab and split creation still append through add_loaded_popup_tab.
    pub(crate) fn open_loaded_popup_tab(
        &mut self,
        requested_url: String,
        profile_id: BrowserProfileId,
        placement: cef::BrowserPopupPlacement,
    ) -> Option<BrowserTabId> {
        let requested_url = browser_loaded_popup_target_url(&requested_url)?;
        if self.tabs.len() == 1
            && let Some(tab) = self.active_tab()
            && tab.state == BrowserTabState::AddressOnly
            && tab.url.is_empty()
        {
            let tab_id = tab.id;
            self.set_tab_profile(tab_id, profile_id);
            return self
                .load_pane_active_tab_url(self.focused_pane, requested_url)
                .map(|(tab_id, _)| tab_id);
        }
        self.add_loaded_popup_tab(requested_url, profile_id, placement)
    }

    pub(crate) fn add_loaded_popup_tab(
        &mut self,
        requested_url: String,
        profile_id: BrowserProfileId,
        placement: cef::BrowserPopupPlacement,
    ) -> Option<BrowserTabId> {
        /*
        CDXC:Browser 2026-06-23-11:43:
        Empty popup targets are a shell-boundary no-op, not a request for an address-only Browser tab. Admit only trimmed non-empty target URLs here so blank script popups cannot mutate selection, create CEF surfaces, persist shell state, or start any content-transfer fallback.

        CDXC:Browser 2026-06-23-14:30:
        The popup target string is accepted only as the loaded-tab target identifier after leading/trailing whitespace is removed. Do not reinterpret an empty target as address-bar text, restored page content, an import request, a notification-worthy event, or a fallback tab.
        */
        let requested_url = browser_loaded_popup_target_url(&requested_url)?;

        /*
        CDXC:Browser 2026-08-18:
        Background placement is the middle-click/Cmd-click contract: append the
        loaded tab to the focused pane's strip and leave selection where it is,
        so the tab materializes its CEF surface later through the normal
        inactive-loaded-tab path.
        */
        let selects_new_tab = matches!(placement, cef::BrowserPopupPlacement::Selected);

        let tab_id = BrowserTabId(self.next_tab_id);
        self.next_tab_id += 1;
        let tab = BrowserTab {
            remote_machine_id: None,
            id: tab_id,
            notification_target: uuid::Uuid::new_v4(),
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
            device_view: BrowserDeviceView::default(),
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
}
