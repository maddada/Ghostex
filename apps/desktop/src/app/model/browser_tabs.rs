// C1 wave-3 re-cluster: browser tab/pane/split/leaf/node model types, tab drag state, navigation history, and favicon types, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BrowserTabId(pub(crate) u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BrowserPaneId(pub(crate) u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BrowserSplitId(pub(crate) u64);

#[derive(Clone)]
pub(crate) struct DraggedCommandTab {
    pub(crate) source_group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
    pub(crate) title: String,
    pub(crate) tab_status: CommandTerminalTabStatus,
}

pub(crate) struct CommandTabDragPreview {
    pub(crate) title: String,
    pub(crate) tab_status: CommandTerminalTabStatus,
}

#[derive(Clone, Copy)]
pub(crate) struct CommandPaneTab {
    pub(crate) session_id: CommandSessionId,
}

pub(crate) struct CommandPaneLeaf {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) tab_group: CommandPaneTabGroup,
}

pub(crate) struct CommandPaneSplit {
    pub(crate) id: CommandPaneSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) ratio: f32,
    pub(crate) first: Box<CommandPaneNode>,
    pub(crate) second: Box<CommandPaneNode>,
}

pub(crate) enum CommandPaneNode {
    Split(CommandPaneSplit),
    Leaf(CommandPaneLeaf),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandPaneDropTarget {
    TabStrip(usize),
    PaneBody(WorkspaceDropZone),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandPaneDropFeedback {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) target: CommandPaneDropTarget,
}

/*
CDXC:Workarea 2026-06-26-06:34:
Native Agents pane tabs select on same-gesture mouse-up, not on mouse-down. Keep this pending tab click as runtime-only pane/session identity so drag start, mouse-up-out, and double-click Focus can share AppKit's gesture ownership without adding overlays, broad hit-test routing, persistence, terminal lifecycle changes, or synthetic input paths.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkspacePendingTabClick {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
}

/*
CDXC:CommandPane 2026-06-25-19:14:
Native AppKit command tabs arm a potential tab selection on left mouse-down, but commit selection only on the matching mouse-up while the gesture stayed a click. Keep GPUI's pending state as a runtime-only tab id token so a command-tab drag start can cancel selection without overlays, root hit-test routing, synthetic coordinates, persistence, or logging.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPanePendingTabClick {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
    pub(crate) expand_on_click: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserTabState {
    Loaded,
    AddressOnly,
}

impl BrowserTabState {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "loaded" => Some(Self::Loaded),
            "address-only" => Some(Self::AddressOnly),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::AddressOnly => "address-only",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserTabChromeStatus {
    LoadedSurface,
    RestoredPlaceholder,
    AddressOnly,
}

impl BrowserTabChromeStatus {
    /*
    CDXC:Browser 2026-06-22-16:48:
    Browser tab chrome must distinguish a loaded tab backed by a live CEF surface from a restored loaded placeholder that has no materialized surface. Derive this render-only status from BrowserTabState plus runtime browser_surfaces membership so shell-state persistence and favicon privacy stay unchanged.
    */
    pub(crate) fn from_state(state: BrowserTabState, has_cef_surface: bool) -> Self {
        match (state, has_cef_surface) {
            (BrowserTabState::Loaded, true) => Self::LoadedSurface,
            (BrowserTabState::Loaded, false) => Self::RestoredPlaceholder,
            (BrowserTabState::AddressOnly, _) => Self::AddressOnly,
        }
    }

    pub(crate) fn allows_runtime_favicon(self) -> bool {
        self == Self::LoadedSurface
    }
}

/*
CDXC:Browser 2026-06-22-17:13:
Browser tab chrome is focus-invariant: pane focus may drive toolbar ownership and CEF surface sync, but tab-bar brightness derives only from a tab's shell state, runtime surface presence, and active membership inside its own Browser tab group. BrowserTabModel.focused_pane is intentionally excluded.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserTabChromeSignature {
    pub(crate) state: BrowserTabState,
    pub(crate) chrome_status: BrowserTabChromeStatus,
    pub(crate) active_in_tab_group: bool,
}

pub(crate) fn browser_tab_chrome_signature(
    tab_group: &BrowserTabGroup,
    tab_id: BrowserTabId,
    state: BrowserTabState,
    has_cef_surface: bool,
) -> BrowserTabChromeSignature {
    BrowserTabChromeSignature {
        state,
        chrome_status: BrowserTabChromeStatus::from_state(state, has_cef_surface),
        active_in_tab_group: tab_group.active_tab_id() == Some(tab_id),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserRuntimeLifecycleInput {
    pub(crate) active_mode: TitlebarMode,
    pub(crate) browser_awake: bool,
    pub(crate) browser_tab_drag_active: bool,
    pub(crate) command_tab_drag_active: bool,
    pub(crate) workspace_tab_drag_active: bool,
}

impl BrowserRuntimeLifecycleInput {
    /*
    CDXC:Workarea 2026-07-03:
    Workspace/Agents tab drags join the same hide-during-drag gate as Browser and command tab drags so the GPUI drag ghost and drop feedback can never sit under a native CEF child view. Hide-and-hold only; no CEF teardown, recreation, or overlay layering.
    */
    pub(crate) fn allows_cef_child_views(self) -> bool {
        !self.browser_tab_drag_active
            && !self.command_tab_drag_active
            && !self.workspace_tab_drag_active
            && self.active_mode == TitlebarMode::Browser
            && self.browser_awake
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserRuntimeSurfacePolicy {
    Visible,
    HiddenHold,
    RestoredPlaceholder,
}

impl BrowserRuntimeSurfacePolicy {
    /*
    CDXC:Browser 2026-06-23-11:32:
    Phase 8 Browser runtime lifecycle is hide-and-hold at the native CEF child-view boundary. Browser sleep, non-Browser modes, Browser tab drags, and command-tab drags hide existing CEF views while retaining tab-owned CEF entities and shell tab metadata; visibility must not teardown, recreate, or materialize restored loaded tabs.

    CDXC:Browser 2026-06-23-14:30:
    Hide-and-hold is the only current Browser runtime lifecycle decision. HiddenHold means an existing tab-owned CEF child view is not shown; it must not be expanded into CEF teardown, CEF suspend, CEF recreation, restored-tab materialization, shell-state writes, or popup content transfer.
    */
    pub(crate) fn for_tab(
        lifecycle: BrowserRuntimeLifecycleInput,
        tab_state: Option<BrowserTabState>,
        rendered_active_loaded: bool,
        has_cef_surface: bool,
    ) -> Self {
        if !lifecycle.allows_cef_child_views() {
            return Self::HiddenHold;
        }

        match (tab_state, rendered_active_loaded, has_cef_surface) {
            (Some(BrowserTabState::Loaded), true, true) => Self::Visible,
            (Some(BrowserTabState::Loaded), true, false) => Self::RestoredPlaceholder,
            _ => Self::HiddenHold,
        }
    }

    pub(crate) fn shows_cef_child_view(self) -> bool {
        matches!(self, Self::Visible)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserPopupTargetPolicy {
    OpenLoadedTab,
    IgnoreWithoutTransfer,
}

impl BrowserPopupTargetPolicy {
    /*
    CDXC:Browser 2026-06-23-12:48:
    Phase 8 popup handling is intentionally source-only until a compatible blank-popup content-transfer contract exists. Only non-empty CEF target URLs may create loaded Browser tabs; empty script-created popups are handled as no-ops with no address-only tab, CEF surface, persistence, notification, page-content copy, or fallback transfer path.

    CDXC:Browser 2026-06-23-14:30:
    Blank or whitespace script-created popup targets remain blocked at the shell boundary. Empty targets must no-op without address-only fallback, Browser notification, CEF surface creation, shell-state write, page-content copy, or any synthetic content-transfer path; non-empty targets are trimmed only as target identifiers for loaded-tab creation.
    */
    pub(crate) fn for_target_url(target_url: &str) -> Self {
        if target_url.trim().is_empty() {
            Self::IgnoreWithoutTransfer
        } else {
            Self::OpenLoadedTab
        }
    }

    pub(crate) fn opens_loaded_tab(self) -> bool {
        matches!(self, Self::OpenLoadedTab)
    }
}

pub(crate) fn browser_loaded_popup_target_url(requested_url: &str) -> Option<String> {
    BrowserPopupTargetPolicy::for_target_url(requested_url)
        .opens_loaded_tab()
        .then(|| requested_url.trim().to_string())
}

pub(crate) fn browser_runtime_visible_surface_tab_ids(
    lifecycle: BrowserRuntimeLifecycleInput,
    browser_tabs: &BrowserTabModel,
    surface_tab_ids: impl IntoIterator<Item = BrowserTabId>,
) -> HashSet<BrowserTabId> {
    /*
    CDXC:Browser 2026-06-23-14:30:
    The visibility pass is a pure hide-and-hold filter over already-owned CEF surfaces. It must not create missing restored-tab surfaces, suspend or tear down hidden CEF entities, infer popup content transfer, or mutate Browser shell state.
    */
    let rendered_active_loaded_tab_ids = browser_tabs.rendered_active_loaded_tab_ids();
    surface_tab_ids
        .into_iter()
        .filter(|tab_id| {
            BrowserRuntimeSurfacePolicy::for_tab(
                lifecycle,
                browser_tabs.tab(*tab_id).map(|tab| tab.state),
                rendered_active_loaded_tab_ids.contains(tab_id),
                true,
            )
            .shows_cef_child_view()
        })
        .collect()
}

#[derive(Clone)]
pub(crate) struct BrowserNavigationHistory {
    pub(crate) entries: Vec<String>,
    pub(crate) current_index: Option<usize>,
}

pub(crate) struct BrowserHistoryRow {
    pub(crate) index: usize,
    pub(crate) url: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BrowserFaviconImage {
    pub(crate) image: Arc<Image>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct BrowserFaviconFetchSource {
    pub(crate) url: String,
}

#[derive(Clone)]
pub(crate) struct DraggedBrowserTab {
    pub(crate) source_pane_id: BrowserPaneId,
    pub(crate) tab_id: BrowserTabId,
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) title: String,
    pub(crate) runtime_favicon_url: Option<String>,
    pub(crate) runtime_favicon_image: Option<BrowserFaviconImage>,
    pub(crate) runtime_favicon_fetch: Option<BrowserFaviconFetchSource>,
    pub(crate) state: BrowserTabState,
    pub(crate) chrome_status: BrowserTabChromeStatus,
}

pub(crate) struct BrowserTabDragPreview {
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) title: String,
    pub(crate) runtime_favicon_url: Option<String>,
    pub(crate) runtime_favicon_image: Option<BrowserFaviconImage>,
    pub(crate) runtime_favicon_fetch: Option<BrowserFaviconFetchSource>,
    pub(crate) state: BrowserTabState,
    pub(crate) chrome_status: BrowserTabChromeStatus,
}

#[derive(Clone)]
pub(crate) struct BrowserTab {
    /*
    CDXC:Browser 2026-06-22-07:23:
    Browser tab titles have two tiers: `title` is the URL-derived fallback that can be regenerated from sanitized shell state, while `runtime_page_title` comes from CEF DisplayHandler callbacks.

    CDXC:Browser 2026-07-12:
    The last displayed title is persisted into shell state as a bounded `cachedTitle` and restored into `runtime_page_title`, so the sidebar and tab strip keep the pre-restart label until the page reports a fresh document title.

    CDXC:Browser 2026-06-22-09:11:
    Browser favicon metadata is runtime-only like page titles, but even stricter for persistence: keep only a safe HTTP(S) origin marker or capped decoded data:image bytes for visible tab chrome, clear favicon state on navigation/address-only transitions, and never serialize raw favicon URLs or image cache data into shell state.

    CDXC:Browser 2026-06-22-10:41:
    Browser tab icons can render capped runtime favicon image bytes from safe data:image URLs without persisting favicon bytes or raw URLs. Clear decoded images and HTTP(S) fetch sources wherever favicon URL metadata clears, and use the URL-only shell marker as fallback.

    CDXC:Browser 2026-06-22-11:05:
    Browser tab icons may fetch safe HTTP(S) favicons only through a favicon-specific runtime asset source. Keep raw CEF favicon URLs out of shell state and logging, store only a scheme+authority marker on BrowserTab, cap URL length, redirects, body bytes, formats, and decode dimensions, and fall back to the marker or generic icon on every failure.

    CDXC:Browser 2026-06-22-10:09:
    GPUI Browser tabs need their own compact navigation history so the toolbar History menu can show rows for the focused tab without borrowing another tab's CEF internals. Keep history keyed by BrowserTabId and show history through OS-owned NativeMenus, not Back/Forward dropdown toggles, in-layout panels, or overlay hit regions.

    CDXC:Telemetry 2026-06-22-10:09:
    Browser history persistence is limited to sanitized loaded URLs and a current index. Do not store page titles, labels, favicon URLs, query strings, fragments, credentials, cookies, tokens, local paths, command text, stdout/stderr, or user-owned content; rebuild invalid or missing history from the tab's sanitized loaded URL.

    CDXC:Browser 2026-06-23-11:14:
    Each GPUI Browser tab carries its selected generated profile id. Changing a tab's profile recreates only that tab's CEF surface with the selected request context, and shell-state persistence stores only this safe numeric id so different tabs keep different profiles across restart without persisting profile names, paths, cookies, credentials, history, or user-entered browser data.
    */
    pub(crate) id: BrowserTabId,
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) title: String,
    pub(crate) runtime_page_title: Option<String>,
    /// Saved machine identity keeps localhost routing attached to the tab across project switches and restores.
    pub(crate) remote_machine_id: Option<String>,
    pub(crate) runtime_favicon_url: Option<String>,
    pub(crate) runtime_favicon_image: Option<BrowserFaviconImage>,
    pub(crate) runtime_favicon_fetch: Option<BrowserFaviconFetchSource>,
    pub(crate) runtime_is_loading: bool,
    pub(crate) runtime_can_go_back: bool,
    pub(crate) runtime_can_go_forward: bool,
    pub(crate) url: String,
    pub(crate) state: BrowserTabState,
    pub(crate) navigation_history: BrowserNavigationHistory,
}

pub(crate) struct BrowserBodyPlaceholder {
    pub(crate) state: BrowserTabState,
    pub(crate) safe_title: Option<String>,
    #[allow(dead_code)]
    // placeholder shape: kept alongside safe_title so the browser body placeholder carries the full sanitised tab identity
    pub(crate) safe_url: Option<String>,
    pub(crate) has_cef_surface: bool,
}

impl BrowserBodyPlaceholder {
    pub(crate) fn blank() -> Self {
        Self {
            state: BrowserTabState::AddressOnly,
            safe_title: None,
            safe_url: None,
            has_cef_surface: false,
        }
    }

    pub(crate) fn from_tab(tab: &BrowserTab, has_cef_surface: bool) -> Self {
        let sanitized_url = if tab.state == BrowserTabState::Loaded {
            sanitize_browser_tab_url_for_state(&tab.url)
        } else {
            None
        };

        Self {
            state: tab.state,
            safe_title: sanitized_url.as_deref().map(browser_tab_title_for_url),
            safe_url: sanitized_url
                .as_deref()
                .and_then(browser_placeholder_safe_origin_url),
            has_cef_surface,
        }
    }
}

#[derive(Clone)]
pub(crate) struct BrowserTabModel {
    pub(crate) tabs: Vec<BrowserTab>,
    pub(crate) root: BrowserNode,
    pub(crate) focused_pane: BrowserPaneId,
    pub(crate) active_tab: BrowserTabId,
    pub(crate) next_pane_id: u64,
    pub(crate) next_split_id: u64,
    pub(crate) next_tab_id: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserTabDropTarget {
    TabStrip(usize),
    PaneBody(WorkspaceDropZone),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserDropFeedback {
    pub(crate) pane_id: BrowserPaneId,
    pub(crate) target: BrowserTabDropTarget,
}

#[derive(Clone)]
pub(crate) struct BrowserPaneTab {
    pub(crate) tab_id: BrowserTabId,
}

#[derive(Clone)]
pub(crate) struct BrowserTabGroup {
    pub(crate) tabs: Vec<BrowserPaneTab>,
    pub(crate) active_tab: BrowserTabId,
}

#[derive(Clone)]
pub(crate) struct BrowserLeaf {
    pub(crate) pane_id: BrowserPaneId,
    pub(crate) tab_group: BrowserTabGroup,
}

#[derive(Clone)]
pub(crate) struct BrowserSplit {
    pub(crate) id: BrowserSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) ratio: f32,
    pub(crate) first: Box<BrowserNode>,
    pub(crate) second: Box<BrowserNode>,
}

#[derive(Clone)]
pub(crate) enum BrowserNode {
    Split(BrowserSplit),
    Leaf(BrowserLeaf),
}

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

    pub(crate) fn rows_around_current(&self, max_rows: usize) -> Vec<BrowserHistoryRow> {
        let Some(current_index) = self
            .current_index
            .filter(|index| *index < self.entries.len())
        else {
            return Vec::new();
        };
        let visible_count = max_rows.max(1).min(self.entries.len());
        let mut start_index = current_index.saturating_sub(visible_count / 2);
        if start_index + visible_count > self.entries.len() {
            start_index = self.entries.len().saturating_sub(visible_count);
        }
        let end_index = start_index + visible_count;

        self.entries[start_index..end_index]
            .iter()
            .enumerate()
            .map(|(offset, url)| {
                let index = start_index + offset;
                BrowserHistoryRow {
                    index,
                    url: url.clone(),
                }
            })
            .collect()
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

impl BrowserTabModel {
    #[allow(dead_code)] // no live caller: only the superseded native browser tab strip built a default tab model
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
        Browser tabs now need shell-owned pane groups and left/right/top/bottom split order before full multi-CEF rendering exists. Keep BrowserTabId metadata and per-tab CEF ownership in one registry, while split leaves store only tab ids plus active selection so drag grouping and splitting never recreate Browser surfaces or persist raw page titles/query strings.

        CDXC:Browser 2026-06-22-09:55:
        Browser split panes should show the existing loaded CEF surface for each rendered leaf's active tab when Browser is awake and drags are not hiding native views. Restored or inactive loaded tabs without an existing CEF entity render restored/sleeping placeholder bodies until normal selection or wake materializes them, and address-only tabs never borrow another tab's surface.

        CDXC:Browser 2026-06-23-11:14:
        Fresh and restored GPUI Browser tabs are assigned a generated shell profile id at model construction. That id is runtime-safe profile plumbing for future CEF surface creation and is intentionally separate from sanitized URL/history persistence.
        */
        let default_url = browser_shell_default_url(None);
        let first_tab = BrowserTab {
            remote_machine_id: None,
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

    #[allow(dead_code)] // no live caller: only the superseded native browser tab strip added placeholder tabs
    pub(crate) fn add_address_placeholder_tab(
        &mut self,
        profile_id: BrowserProfileId,
    ) -> BrowserTabId {
        /*
        CDXC:FocusMode 2026-06-22-12:51:
        Browser new-tab commands, including Cmd+N and the clicked pane control, must insert the address-only placeholder immediately after the focused pane's active tab so creation stays adjacent to the user's current Browser work instead of appending to a long tab group.

        CDXC:Browser 2026-06-23-11:14:
        New address-only Browser tabs inherit the currently selected generated Browser profile at creation time. Later profile selection changes affect future tabs/surfaces only and must not mutate existing tab profile ownership.
        */
        let tab_id = BrowserTabId(self.next_tab_id);
        self.next_tab_id += 1;
        self.tabs.push(BrowserTab {
            remote_machine_id: None,
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

    #[allow(dead_code)] // no live caller: reloads go through the CEF browser chrome
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
        // Remote HTTP favicons are fetched through the tab's SSH route, never GPUI's local HTTP client.
        let favicon_fetch = favicon_fetch.filter(|_| tab.remote_machine_id.is_none());
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

    pub(crate) fn close_tab(
        &mut self,
        tab_id: BrowserTabId,
        active_profile_id: BrowserProfileId,
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
        CDXC:Browser 2026-06-22-07:41:
        Browser tab-strip drops are same-strip reorder only for this slice. Reorder the existing BrowserTab records in place, preserve the active BrowserTabId, and leave runtime titles, current URLs, address-only state, and the tab-owned CEF surface map untouched so dragging a tab never reloads or recreates a page surface.

        CDXC:Browser 2026-06-22-09:02:
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

    pub(crate) fn pane_history_rows(
        &self,
        pane_id: BrowserPaneId,
        max_rows: usize,
    ) -> Vec<BrowserHistoryRow> {
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

pub(crate) fn sanitize_browser_tab_url_for_state(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL) {
        return None;
    }
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let content_end = rest.find(['?', '#']).unwrap_or(rest.len());
    let without_query = &rest[..content_end];
    let authority_end = without_query.find('/').unwrap_or(without_query.len());
    let authority = &without_query[..authority_end];
    let path = &without_query[authority_end..];
    let authority = authority.rsplit('@').next().unwrap_or(authority).trim();
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}{path}"))
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

/*
CDXC:Browser 2026-08-18:
One pending sidebar reveal for a Browser tab the user just opened, held until
that tab reaches the sidebar in a published tab snapshot.
*/
pub(crate) struct PendingSidebarBrowserTabReveal {
    pub(crate) project_id: String,
    pub(crate) tab_id: BrowserTabId,
}
