use super::*;
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
CDXC:CommandPane 2026-06-25-19:14:
Native AppKit command tabs arm a potential tab selection on left mouse-down, but commit selection only on the matching mouse-up while the gesture stayed a click. Keep GPUI's pending state as a runtime-only tab id token so a command-tab drag start can cancel selection without overlays, root hit-test routing, synthetic coordinates, persistence, or logging.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPanePendingTabClick {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
    pub(crate) expand_on_click: bool,
}

#[derive(Clone)]
pub(crate) struct BrowserNavigationHistory {
    pub(crate) entries: Vec<String>,
    pub(crate) current_index: Option<usize>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BrowserFaviconImage {
    pub(crate) image: Arc<Image>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct BrowserFaviconFetchSource {
    pub(crate) url: String,
    /// Where the fetched icon is kept for the next start; see `browser_favicon_cache_key`.
    pub(crate) cache_key: Option<String>,
}

#[derive(Clone)]
pub(crate) struct DraggedBrowserTab {
    pub(crate) source_pane_id: BrowserPaneId,
    pub(crate) tab_id: BrowserTabId,
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) title: gpui::SharedString,
    pub(crate) runtime_favicon_image: Option<BrowserFaviconImage>,
    pub(crate) runtime_favicon_fetch: Option<BrowserFaviconFetchSource>,
    pub(crate) state: BrowserTabState,
    pub(crate) chrome_status: BrowserTabChromeStatus,
}

pub(crate) struct BrowserTabDragPreview {
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) title: gpui::SharedString,
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

    CDXC:Browser 2026-09-21 WHY:
    Shell state carries no favicon data: raw CEF favicon URLs stay out of it and out of logs, and the tab keeps only a scheme+authority marker, capped decoded data:image bytes, or a capped HTTP(S) fetch source. The icon survives a restart through the per-origin favicon cache instead (`browser_favicon_cache.rs`), which also fills the slot on navigation until the page reports its own icon. Supersedes the 2026-06-22 runtime-only favicon notes.

    CDXC:Browser 2026-06-22-10:09:
    The per-tab navigation list restores back/forward state. The history popup reads the independent persistent visit store across projects; both remain separate from CEF internals.

    CDXC:Telemetry 2026-06-22-10:09:
    The per-tab back/forward list stores sanitized loaded URLs and a current index; rebuild invalid or missing navigation from the tab's sanitized loaded URL. Titles and favicons belong to the independent Browser visit store used by the history popup.

    CDXC:Browser 2026-06-23-11:14:
    Each GPUI Browser tab carries its selected generated profile id. Changing a tab's profile recreates only that tab's CEF surface with the selected request context, and shell-state persistence stores only this safe numeric id so different tabs keep different profiles across restart without persisting profile names, paths, cookies, credentials, history, or user-entered browser data.
    */
    pub(crate) id: BrowserTabId,
    /// What a page notification names the tab by (browser_page_notifications.rs): random for each
    /// tab of this run and never saved, so it stays with the tab while its page sleeps or its
    /// project is parked, and a notification from an earlier run or a closed tab matches nothing.
    pub(crate) notification_target: uuid::Uuid,
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
    pub(crate) device_view: BrowserDeviceView,
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
    PaneBody(WorkspaceDropZone),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserDropFeedback {
    pub(crate) pane_id: BrowserPaneId,
    pub(crate) target: BrowserTabDropTarget,
}

/// A page that another page opened, and that page: closing it while it is still the tab on screen
/// goes back there (`view_strip_successor`).
#[derive(Clone, Copy)]
pub(crate) struct BrowserTabOpener {
    pub(crate) tab: BrowserTabId,
    pub(crate) opener: BrowserTabId,
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
