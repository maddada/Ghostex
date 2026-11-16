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
    sanitize_browser_tab_url_with_policy(url, false)
}

/// Persisted Browser tabs and history preserve local `file://` URLs alongside HTTP(S) so local
/// HTML pages and artifacts reopen after restarts, while remote-machine tabs stay HTTP(S)-only.
pub(crate) fn sanitize_browser_tab_url_for_tab(
    url: &str,
    remote_machine_id: Option<&str>,
) -> Option<String> {
    sanitize_browser_tab_url_with_policy(url, remote_machine_id.is_none())
}

fn sanitize_browser_tab_url_with_policy(url: &str, allow_local_file: bool) -> Option<String> {
    /*
    CDXC:Browser 2026-09-09 DECISION:
    User: after an app restart, a Browser tab must reopen the same complete HTTP(S) URL, including its query string and fragment, rather than only the page path. Continue removing URL credentials before persistence.
    */
    let trimmed = url.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL)
        || trimmed.chars().any(char::is_control)
    {
        return None;
    }
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme == "file" {
        return allow_local_file
            .then(|| sanitize_local_file_url(trimmed, rest))
            .flatten();
    }
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

fn sanitize_local_file_url(trimmed: &str, rest: &str) -> Option<String> {
    if rest.starts_with('\\') || rest.starts_with("/\\") {
        return None;
    }
    let suffix_start = rest.find(['?', '#']).unwrap_or(rest.len());
    let before_suffix = &rest[..suffix_start];
    let raw_path = if before_suffix.starts_with('/') {
        before_suffix
    } else if let Some((authority, _)) = before_suffix.split_once('/') {
        if authority.contains('@') || !authority.eq_ignore_ascii_case("localhost") {
            return None;
        }
        &before_suffix[authority.len()..]
    } else {
        return None;
    };
    if raw_file_path_resolves_to_unc_or_empty(raw_path) {
        return None;
    }
    let parsed = url::Url::parse(trimmed).ok()?;
    if parsed.scheme() != "file" || !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    if parsed
        .host_str()
        .is_some_and(|host| !host.eq_ignore_ascii_case("localhost"))
    {
        return None;
    }
    let path = parsed.path();
    if !path.starts_with('/') || path.len() <= 1 {
        return None;
    }
    if path
        .as_bytes()
        .get(1)
        .is_some_and(|byte| matches!(byte, b'/' | b'\\'))
    {
        return None;
    }
    if path.get(1..4).is_some_and(|prefix| {
        prefix.eq_ignore_ascii_case("%2f") || prefix.eq_ignore_ascii_case("%5c")
    }) {
        return None;
    }
    Some(parsed.into())
}

// Check raw dot-segment resolution before `url::Url::parse` because WHATWG `file:` parsing
// collapses `file:////host/...` and `..//` empty segments into local-looking `file:///host/...`.
fn raw_file_path_resolves_to_unc_or_empty(raw_path: &str) -> bool {
    let Some(after_root) = raw_path.strip_prefix('/') else {
        return true;
    };
    if after_root.is_empty() {
        return true;
    }
    let mut stack: Vec<&str> = Vec::new();
    let mut has_drive_root = false;
    for segment in after_root.split(['/', '\\']) {
        if is_single_dot_segment(segment) {
            continue;
        }
        if is_double_dot_segment(segment) {
            if stack.len() > usize::from(has_drive_root) {
                stack.pop();
            }
            continue;
        }
        if stack.is_empty() && is_windows_drive_segment(segment) {
            has_drive_root = true;
        }
        stack.push(segment);
    }
    let Some(first) = stack.first().copied() else {
        return true;
    };
    first.is_empty()
        || first.get(0..3).is_some_and(|prefix| {
            prefix.eq_ignore_ascii_case("%2f") || prefix.eq_ignore_ascii_case("%5c")
        })
}

fn is_windows_drive_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && matches!(bytes[1], b':' | b'|')
}

fn is_single_dot_segment(segment: &str) -> bool {
    segment == "." || segment.eq_ignore_ascii_case("%2e")
}

fn is_double_dot_segment(segment: &str) -> bool {
    let Some(rest) = segment
        .strip_prefix('.')
        .or_else(|| strip_ascii_prefix_ignore_case(segment, "%2e"))
    else {
        return false;
    };
    rest == "." || rest.eq_ignore_ascii_case("%2e")
}

fn strip_ascii_prefix_ignore_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let head = value.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &value[prefix.len()..])
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
