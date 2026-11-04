use super::*;
use crate::*;

impl BrowserNavigationHistory {
    pub(crate) fn empty() -> Self {
        Self {
            entries: Vec::new(),
            current_index: None,
        }
    }

    pub(crate) fn loaded(url: &str) -> Self {
        let url = url.trim();
        if url.is_empty() || url.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL) {
            return Self::empty();
        }

        Self {
            entries: vec![url.to_string()],
            current_index: Some(0),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.current_index = None;
    }

    pub(crate) fn append_loaded_url(&mut self, url: &str) {
        let url = url.trim();
        if url.is_empty() || url.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL) {
            self.clear();
            return;
        }

        if let Some(current_index) = self.current_index {
            self.entries.truncate(current_index.saturating_add(1));
        } else {
            self.entries.clear();
        }
        self.entries.push(url.to_string());
        self.current_index = Some(self.entries.len().saturating_sub(1));
        self.enforce_cap();
    }

    pub(crate) fn record_address_change(&mut self, url: &str) -> bool {
        let url = url.trim();
        if url.is_empty() || url.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL) {
            let changed = !self.entries.is_empty() || self.current_index.is_some();
            self.clear();
            return changed;
        }

        let Some(current_index) = self
            .current_index
            .filter(|index| *index < self.entries.len())
        else {
            self.entries = vec![url.to_string()];
            self.current_index = Some(0);
            return true;
        };

        if self.entries[current_index] == url {
            return false;
        }

        if current_index > 0 && self.entries[current_index - 1] == url {
            self.current_index = Some(current_index - 1);
            return true;
        }

        if current_index + 1 < self.entries.len() && self.entries[current_index + 1] == url {
            self.current_index = Some(current_index + 1);
            return true;
        }

        self.entries.truncate(current_index.saturating_add(1));
        self.entries.push(url.to_string());
        self.current_index = Some(self.entries.len().saturating_sub(1));
        self.enforce_cap();
        true
    }

    pub(crate) fn enforce_cap(&mut self) {
        if self.entries.len() <= BROWSER_HISTORY_MAX_ENTRIES {
            return;
        }

        let remove_count = self.entries.len() - BROWSER_HISTORY_MAX_ENTRIES;
        self.entries.drain(0..remove_count);
        if let Some(current_index) = self.current_index {
            self.current_index = Some(current_index.saturating_sub(remove_count));
        }
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
        self.display_title_ref().to_string()
    }

    /// The title the tab shows, borrowed, for callers that keep their own copy only if they need
    /// one (the strip and the Browser's page row share one `SharedString` per page).
    pub(crate) fn display_title_ref(&self) -> &str {
        match self.state {
            BrowserTabState::Loaded => self.runtime_page_title.as_deref().unwrap_or(&self.title),
            BrowserTabState::AddressOnly => "New Tab",
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

    /// After `closed` left this pane, select the pane's tab drawn nearest to it in `drawn_order`
    /// (its right, else its left). A pane none of whose tabs are drawn keeps the choice
    /// `remove_tab` made.
    pub(crate) fn select_drawn_neighbour(
        &mut self,
        closed: BrowserTabId,
        drawn_order: &[BrowserTabId],
    ) {
        let Some(index) = drawn_order.iter().position(|tab_id| *tab_id == closed) else {
            return;
        };
        let (left, right) = drawn_order.split_at(index);
        if let Some(next) = right[1..]
            .iter()
            .chain(left.iter().rev())
            .find(|tab_id| self.has_tab(**tab_id))
        {
            self.active_tab = *next;
        }
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

pub(crate) fn sanitize_browser_tab_url_for_state(url: &str) -> Option<String> {
    /*
    CDXC:Browser 2026-09-09 DECISION:
    User: after an app restart, a Browser tab must reopen the same complete HTTP(S) URL, including its query string and fragment, rather than only the page path. Continue removing URL credentials before persistence.
    */
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL) {
        return None;
    }
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let navigation_suffix = &rest[authority_end..];
    let authority = authority.rsplit('@').next().unwrap_or(authority).trim();
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}{navigation_suffix}"))
}

pub(crate) fn browser_placeholder_safe_origin_url(sanitized_url: &str) -> Option<String> {
    let (scheme, rest) = sanitized_url.trim().split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }

    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest[..authority_end]
        .rsplit('@')
        .next()
        .unwrap_or("")
        .trim();
    if authority.is_empty() {
        return None;
    }

    Some(format!("{scheme}://{authority}"))
}
