use super::*;
use crate::*;

impl BrowserTabModel {
    /// `drawn_order` is the order the strip draws the pages in: a pane whose selected tab closes
    /// selects the tab drawn next to it (`BrowserTabGroup::select_drawn_neighbour`).
    pub(crate) fn close_tab(
        &mut self,
        tab_id: BrowserTabId,
        active_profile_id: BrowserProfileId,
        drawn_order: &[BrowserTabId],
    ) -> bool {
        /*
        CDXC:Browser 2026-06-22-05:56:
        Closing Browser tabs must be modest but visible in the parity shell. Multi-tab closes remove the target and keep or select a neighboring tab in memory; closing the last tab resets it to an address-only placeholder so Browser mode never drops to an empty workspace while multiple CEF views and persisted tab lifecycles remain deferred.

        CDXC:Browser 2026-06-23-11:14:
        Resetting the final Browser tab creates a new address-only placeholder in place, so it adopts the currently selected generated profile for any future load instead of retaining the closed page's profile ownership.
        */
        if !self.has_tab(tab_id) {
            return false;
        };

        if self.tabs.len() == 1 {
            let tab = &mut self.tabs[0];
            // The placeholder keeps the closed tab's id, not what its notifications name.
            tab.notification_target = uuid::Uuid::new_v4();
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
        let closing_pane_active = self.active_tab_id_for_pane(source_pane_id) == Some(tab_id);
        let Some((_tab, source_is_empty)) = self.remove_tab_for_move(source_pane_id, tab_id) else {
            return false;
        };
        self.tabs.retain(|tab| tab.id != tab_id);

        if source_is_empty {
            self.collapse_empty_leaf(source_pane_id);
        } else if closing_pane_active && let Some(leaf) = self.find_leaf_mut(source_pane_id) {
            leaf.tab_group.select_drawn_neighbour(tab_id, drawn_order);
        }
        if self.find_leaf(self.focused_pane).is_none()
            && let Some(first_leaf_id) = first_browser_leaf_id(&self.root)
        {
            self.focused_pane = first_leaf_id;
        }
        if (closing_active || !self.has_tab(self.active_tab))
            && let Some(active_tab) = self
                .find_leaf(self.focused_pane)
                .and_then(|leaf| leaf.tab_group.active_tab_id())
                .or_else(|| first_browser_tab_id(&self.root))
        {
            self.active_tab = active_tab;
        }
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
        CDXC:Browser 2026-06-22-09:02:
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
        CDXC:Browser 2026-06-22-13:46:
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

    /// Every tab, pane by pane in layout order.
    pub(crate) fn tab_ids_by_pane(&self) -> Vec<BrowserTabId> {
        self.rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| self.find_leaf(pane_id))
            .flat_map(|leaf| leaf.tab_group.tabs.iter().map(|tab| tab.tab_id))
            .collect()
    }

    pub(crate) fn active_loaded_tab_id_for_leaf(&self, leaf: &BrowserLeaf) -> Option<BrowserTabId> {
        let tab_id = leaf.tab_group.active_tab_id()?;
        self.tab(tab_id)
            .filter(|tab| tab.state == BrowserTabState::Loaded)
            .map(|tab| tab.id)
    }

    pub(crate) fn rendered_active_loaded_tab_ids(&self) -> HashSet<BrowserTabId> {
        /*
        CDXC:Browser 2026-06-22-09:55:
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
}
