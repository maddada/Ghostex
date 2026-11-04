//! The Settings modal's shared state: the settings draft and how it is saved
//! (packages/core-ui/settings-modal/settings-persistence.ts (deleted 2026-10-01)), the search results every page reads,
//! the latest hydrate and host payloads, page and scroll memory
//! (settings-modal/navigation-memory.ts (deleted 2026-10-01)), and the scroll anchors each page registers.
//!
//! Every page is its own entity holding an `Entity<SettingsStore>`; it reads values here and
//! writes through `update_setting` / `update_setting_debounced` / `apply_patch`, and it re-renders
//! when the store notifies (`cx.observe`).
use super::catalog::settings_catalog;
use super::model::{SettingsModalCommand, SettingsModalHost, SettingsOpenRequest, SettingsTabId};
use super::palette::SettingsPalette;
use super::restore::{OpenFormMemory, request_deep_links};
use super::search::{GeneralSearch, SectionSearch, TabSearch, extra_tab_search, js_trim};
use gpui::{
    App, AppContext as _, Bounds, Context, Entity, EventEmitter, Pixels, ScrollHandle, Task, px,
};
use serde_json::{Map, Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

/// `NUMERIC_SETTINGS_DEBOUNCE_MS`.
const NUMERIC_SETTINGS_DEBOUNCE: Duration = Duration::from_millis(180);
/// `SETTINGS_MODAL_NAVIGATION_SCROLL_DEBOUNCE_MS`.
const NAVIGATION_SCROLL_DEBOUNCE: Duration = Duration::from_millis(220);
/// `MAX_SETTINGS_MODAL_SCROLL_TOP`.
const MAX_SCROLL_TOP: f32 = 1_000_000.0;
/// `.settings-section-anchor { scroll-margin-top: 1rem }`.
const SECTION_SCROLL_MARGIN: f32 = 16.0;

thread_local! {
    /// `rememberedSettingsModalTab`: the page for the rest of this app run.
    static REMEMBERED_TAB: Cell<Option<SettingsTabId>> = const { Cell::new(None) };
    /// `rememberedSettingsModalScrollTopByTab`.
    static REMEMBERED_SCROLL_TOPS: RefCell<HashMap<SettingsTabId, f32>> = RefCell::new(HashMap::new());
}

fn remembered_scroll_top(tab: SettingsTabId) -> Option<f32> {
    REMEMBERED_SCROLL_TOPS.with(|tops| tops.borrow().get(&tab).copied())
}

fn remember_scroll_top(tab: SettingsTabId, top: f32) {
    REMEMBERED_SCROLL_TOPS.with(|tops| {
        tops.borrow_mut().insert(tab, top);
    });
}

/// Events for pages that want more than a re-render.
pub(crate) enum SettingsStoreEvent {
    /// A host payload of this `type` arrived (`agentHookStatus`, `ghostexCliStatus`, ...).
    HostPayload(String),
}

/// A page's scroll anchors: the ids of the scroll container's children that are section
/// anchors (`None` for a child that is not one), and the anchor mostly in view.
#[derive(Default)]
pub(crate) struct SectionTracker {
    pub(crate) children: Vec<Option<String>>,
    pub(crate) active: Option<String>,
    /// A section to scroll to once its bounds are known (a deep link or a rail click).
    pub(crate) pending_scroll: Option<String>,
    /// The scroll offset of the last frame: the active section is measured only after the page
    /// actually scrolled, as React measures on scroll events.
    pub(crate) last_offset: Option<f32>,
    /// The section scroll in flight (`scrollIntoView({ behavior: 'smooth' })`).
    pub(crate) smooth: Option<SmoothScroll>,
    /// The remembered scroll position still being restored.
    pub(crate) restoring: Option<ScrollRestore>,
}

/// How long a page may take to grow tall enough for its remembered scroll position.
const SCROLL_RESTORE_WAIT: Duration = Duration::from_secs(3);

/// A remembered scroll top being restored. A page whose content arrives after its first frame
/// (the daemon's answers on Remote, the catalog on Extensions, the accounts list) is too short to
/// reach it at first, so the page follows its content down to it until it gets there, the user
/// scrolls, a section scroll starts, or `SCROLL_RESTORE_WAIT` runs out.
#[derive(Clone, Copy)]
pub(crate) struct ScrollRestore {
    pub(crate) top: f32,
    pub(crate) until: std::time::Instant,
    /// The offset this restore set last, to notice the user scrolling over it.
    pub(crate) last: Option<f32>,
}

impl ScrollRestore {
    pub(crate) fn new(top: f32) -> Self {
        Self {
            top,
            until: std::time::Instant::now() + SCROLL_RESTORE_WAIT,
            last: None,
        }
    }
}

/// A programmatic smooth scroll the way Chromium runs `scrollIntoView({ behavior: 'smooth' })`:
/// cc's `ScrollOffsetAnimationCurve` with `DurationBehavior::kDeltaBased`, lasting
/// sqrt(|distance|) frames at 60Hz and at most 12 (200ms), on the ease-in-out curve
/// `cubic-bezier(0.42, 0, 0.58, 1)`. A scroll the user makes meanwhile cancels it.
#[derive(Clone, Copy)]
pub(crate) struct SmoothScroll {
    from: f32,
    to: f32,
    start: std::time::Instant,
    duration: f32,
    /// The offset this animation set last, to notice the user scrolling over it.
    pub(crate) last: f32,
}

impl SmoothScroll {
    /// `from` and `to` are scroll tops (positive, content pixels).
    pub(crate) fn new(from: f32, to: f32) -> Self {
        let duration = (to - from).abs().sqrt().min(12.0) / 60.0;
        Self {
            from,
            to,
            start: std::time::Instant::now(),
            duration,
            last: from,
        }
    }

    /// The scroll top for now, and whether the animation has finished.
    pub(crate) fn sample(&self) -> (f32, bool) {
        let elapsed = self.start.elapsed().as_secs_f32();
        if self.duration <= 0.0 || elapsed >= self.duration {
            return (self.to, true);
        }
        let progress = ease_in_out(elapsed / self.duration);
        (self.from + (self.to - self.from) * progress, false)
    }
}

/// `cubic-bezier(0.42, 0, 0.58, 1)` at `x` (bisection on the curve's x, then its y).
fn ease_in_out(x: f32) -> f32 {
    let bezier = |t: f32, p1: f32, p2: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
    };
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    let mut t = x;
    for _ in 0..24 {
        let value = bezier(t, 0.42, 0.58);
        if (value - x).abs() < 1e-5 {
            break;
        }
        if value < x {
            low = t;
        } else {
            high = t;
        }
        t = (low + high) / 2.0;
    }
    bezier(t, 0.0, 1.0)
}

/// The settings draft: stored values with `DEFAULT_ghostex_SETTINGS` behind them.
#[derive(Clone, Default)]
pub(crate) struct SettingsValues {
    settings: Rc<Map<String, Value>>,
}

impl SettingsValues {
    pub(crate) fn from_map(settings: Map<String, Value>) -> Self {
        Self {
            settings: Rc::new(settings),
        }
    }

    /// The stored object.
    pub(crate) fn map(&self) -> &Map<String, Value> {
        &self.settings
    }

    fn set(&mut self, key: &str, value: Value) {
        Rc::make_mut(&mut self.settings).insert(key.to_string(), value);
    }

    /// The draft value of `key`, or its default (`normalizeghostexSettings` falls back the same way).
    pub(crate) fn value(&self, key: &str) -> Value {
        self.settings
            .get(key)
            .cloned()
            .or_else(|| settings_catalog().default_value(key).cloned())
            .unwrap_or(Value::Null)
    }

    pub(crate) fn bool(&self, key: &str) -> bool {
        match self.settings.get(key).and_then(Value::as_bool) {
            Some(value) => value,
            None => settings_catalog()
                .default_value(key)
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    pub(crate) fn f64(&self, key: &str) -> f64 {
        match self.settings.get(key).and_then(Value::as_f64) {
            Some(value) if value.is_finite() => value,
            _ => settings_catalog()
                .default_value(key)
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        }
    }

    pub(crate) fn string(&self, key: &str) -> String {
        match self.settings.get(key).and_then(Value::as_str) {
            Some(value) => value.to_string(),
            None => settings_catalog()
                .default_value(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }
    }

    /// A string setting constrained to its options (the normalizers' fallback to the default).
    pub(crate) fn choice(&self, key: &str, allowed: &[String]) -> String {
        let value = self.string(key);
        if allowed.contains(&value) {
            return value;
        }
        settings_catalog()
            .default_value(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    /// `String(draft[key])` for the numeric selects.
    pub(crate) fn number_string(&self, key: &str) -> String {
        super::catalog::js_number_string(self.f64(key))
    }

    /// `!Object.is(draft[key], DEFAULT_ghostex_SETTINGS[key])`. For arrays and objects this compares
    /// contents: the normalizers hand back the default's own array when a list is empty, and the
    /// rows that care about a list's order pass their own comparison.
    pub(crate) fn is_modified(&self, key: &str) -> bool {
        let Some(default) = settings_catalog().default_value(key) else {
            return false;
        };
        let value = self.value(key);
        match (&value, default) {
            (Value::Number(left), Value::Number(right)) => left.as_f64() != right.as_f64(),
            _ => value != *default,
        }
    }
}

pub(crate) struct SettingsStore {
    host: SettingsModalHost,
    palette: SettingsPalette,
    request: SettingsOpenRequest,
    settings: SettingsValues,
    /// The last `sidebarState` hydrate (the React modal's `useSidebarStore` view): `hud.agents`,
    /// `hud.projects`, `customSessionTags`, and so on.
    sidebar_state: Value,
    /// The latest transient payload per `type` (`agentHookStatus`, `ghostexCliStatus`, ...).
    host_payloads: HashMap<String, Value>,
    active_tab: SettingsTabId,
    /// The pages this open has visited, newest last, and where Back/Forward stand in them.
    page_history: Vec<SettingsTabId>,
    page_history_index: usize,
    search_query: String,
    general_search: GeneralSearch,
    tab_searches: HashMap<String, TabSearch>,
    scroll_handles: HashMap<SettingsTabId, ScrollHandle>,
    trackers: HashMap<SettingsTabId, Rc<RefCell<SectionTracker>>>,
    /// `pendingSettingsPatchRef`: debounced numeric edits not yet posted.
    pending_patch: Option<Map<String, Value>>,
    pending_patch_task: Option<Task<()>>,
    navigation_task: Option<Task<()>>,
    /// The scroll top restored for each page on its first frame, so it is applied once.
    restored_scroll: HashMap<SettingsTabId, bool>,
    /// The pages whose Advanced fold is open (CDXC:Settings 2026-10-29 in search.rs). Kept here,
    /// not on the page view, so opening it re-renders the whole modal like a setting change: a
    /// page-only notify left the scroll area's reach at the pre-fold height until the next
    /// store change (the fast layers' tracked scroll handle), so the opened rows could not be
    /// scrolled to.
    advanced_folds: HashMap<SettingsTabId, bool>,
    closed: bool,
    /// The dialogs and forms the last visit left open (settings_modal/restore.rs).
    open_forms: OpenFormMemory,
    /// Whether the rail's Extensions group is open (`extensionsGroupOpen` in
    /// `settingsModalNavigation`; see `SettingsTabId::EXTENSIONS_GROUP`).
    extensions_group_open: bool,
    /// The remote machine Agents and Integrations show (tabs/machine_picker.rs); `None` is this
    /// computer. Shared by both pages and forgotten when Settings closes.
    picked_machine: Option<String>,
    /// The Windows Developer Mode notice of Agents and Integrations is folded to its icon
    /// (`windowsLinkNoticeCollapsed` in `settingsModalNavigation`, tabs/machine_picker.rs).
    windows_link_notice_collapsed: bool,
}

impl EventEmitter<SettingsStoreEvent> for SettingsStore {}

impl SettingsStore {
    pub(crate) fn new(
        host: SettingsModalHost,
        palette: SettingsPalette,
        request: SettingsOpenRequest,
        sidebar_state: Value,
        open_forms: OpenFormMemory,
    ) -> Self {
        let settings = SettingsValues {
            settings: Rc::new(
                sidebar_state
                    .get("hud")
                    .and_then(|hud| hud.get("settings"))
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
            ),
        };
        let mut store = Self {
            host,
            palette,
            request,
            settings,
            sidebar_state,
            host_payloads: HashMap::new(),
            active_tab: SettingsTabId::Chat,
            page_history: Vec::new(),
            page_history_index: 0,
            search_query: String::new(),
            general_search: GeneralSearch::default(),
            tab_searches: HashMap::new(),
            scroll_handles: HashMap::new(),
            trackers: HashMap::new(),
            pending_patch: None,
            pending_patch_task: None,
            navigation_task: None,
            restored_scroll: HashMap::new(),
            advanced_folds: HashMap::new(),
            closed: false,
            open_forms,
            extensions_group_open: false,
            picked_machine: None,
            windows_link_notice_collapsed: false,
        };
        store.picked_machine = store.request.initial_machine_id.clone();
        store.windows_link_notice_collapsed = store
            .value("settingsModalNavigation")
            .get("windowsLinkNoticeCollapsed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        store.active_tab = store.initial_tab();
        store.extensions_group_open = store
            .value("settingsModalNavigation")
            .get("extensionsGroupOpen")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || store
                .request
                .initial_tab
                .is_some_and(|tab| store.active_tab == tab && tab.in_extensions_group());
        if request_deep_links(&store.request) {
            store.open_forms.defer(store.active_tab);
        }
        store.page_history = vec![store.active_tab];
        let clears_search = store.request.initial_agents_section.is_some()
            || store.request.initial_custom_view_id.is_some()
            || store.request.initial_view_scope_key.is_some();
        let query = store
            .request
            .initial_search_query
            .as_deref()
            .map(js_trim)
            .filter(|query| !query.is_empty() && !clears_search)
            .unwrap_or_default()
            .to_string();
        store.search_query = query;
        store.refresh_search();
        REMEMBERED_TAB.with(|tab| tab.set(Some(store.active_tab)));
        store
    }

    // ---- settings values -------------------------------------------------------------------

    /// A cheap snapshot of the draft for a page's render (the store cannot stay borrowed while
    /// the page builds its listeners).
    pub(crate) fn values(&self) -> SettingsValues {
        self.settings.clone()
    }

    pub(crate) fn value(&self, key: &str) -> Value {
        self.settings.value(key)
    }

    pub(crate) fn bool(&self, key: &str) -> bool {
        self.settings.bool(key)
    }

    pub(crate) fn f64(&self, key: &str) -> f64 {
        self.settings.f64(key)
    }

    pub(crate) fn string(&self, key: &str) -> String {
        self.settings.string(key)
    }

    pub(crate) fn is_modified(&self, key: &str) -> bool {
        self.settings.is_modified(key)
    }

    /// The whole draft (for `applySettings` callers that start from it).
    pub(crate) fn settings(&self) -> &Map<String, Value> {
        self.settings.map()
    }

    /// `updateDraft(key, value)`: saves one setting now.
    pub(crate) fn update_setting(&mut self, key: &str, value: Value, cx: &mut Context<Self>) {
        let mut patch = Map::new();
        patch.insert(key.to_string(), value);
        self.apply_patch(patch, "settings:control", cx);
    }

    /// `applySettingsPatch(patch, source)`: merges and saves a patch now, together with any
    /// debounced edit still waiting.
    pub(crate) fn apply_patch(
        &mut self,
        patch: Map<String, Value>,
        source: &str,
        cx: &mut Context<Self>,
    ) {
        // A debounced edit still waiting is saved with this patch instead of being dropped.
        let mut patch = patch;
        if let Some(pending) = self.pending_patch.take() {
            for (key, value) in pending {
                patch.entry(key).or_insert(value);
            }
        }
        for (key, value) in &patch {
            self.settings.set(key, value.clone());
        }
        self.pending_patch_task = None;
        self.refresh_search();
        cx.notify();
        if patch.is_empty() {
            return;
        }
        (self.host)(
            SettingsModalCommand::SavePatch {
                patch,
                source: source.to_string(),
            },
            cx,
        );
    }

    /// `updateDraftDebounced(key, value)`: shows the value now and saves it after 180ms without
    /// further edits.
    pub(crate) fn update_setting_debounced(
        &mut self,
        key: &str,
        value: Value,
        cx: &mut Context<Self>,
    ) {
        self.settings.set(key, value.clone());
        self.pending_patch
            .get_or_insert_with(Map::new)
            .insert(key.to_string(), value);
        cx.notify();
        self.pending_patch_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(NUMERIC_SETTINGS_DEBOUNCE)
                .await;
            let _ = this.update(cx, |this, cx| this.flush_pending_patch(cx));
        }));
    }

    fn flush_pending_patch(&mut self, cx: &mut Context<Self>) {
        self.pending_patch_task = None;
        let Some(patch) = self.pending_patch.take() else {
            return;
        };
        if patch.is_empty() {
            return;
        }
        (self.host)(
            SettingsModalCommand::SavePatch {
                patch,
                source: "settings:control".to_string(),
            },
            cx,
        );
    }

    /// `applySettings(nextSettings, source)`: a whole-settings save.
    pub(crate) fn apply_settings(
        &mut self,
        settings: Map<String, Value>,
        source: &str,
        cx: &mut Context<Self>,
    ) {
        self.settings = SettingsValues::from_map(settings.clone());
        self.pending_patch = None;
        self.pending_patch_task = None;
        self.refresh_search();
        cx.notify();
        (self.host)(
            SettingsModalCommand::SaveSettings {
                settings,
                source: source.to_string(),
            },
            cx,
        );
    }

    /// `resetSetting(key)`: back to `DEFAULT_ghostex_SETTINGS[key]`.
    pub(crate) fn reset_setting(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(default) = settings_catalog().default_value(key).cloned() else {
            return;
        };
        self.update_setting(key, default, cx);
    }

    /// `vscode.postMessage(message)`.
    pub(crate) fn post_message(&self, message: Value, cx: &mut App) {
        (self.host)(SettingsModalCommand::PostMessage(message), cx);
    }

    /// Opens the system colour panel for a colour setting (see `SettingsModalCommand::PickSystemColor`).
    pub(crate) fn pick_system_color(&self, key: &str, initial: &str, cx: &mut App) {
        (self.host)(
            SettingsModalCommand::PickSystemColor {
                key: key.to_string(),
                initial: initial.to_string(),
            },
            cx,
        );
    }

    pub(crate) fn toast(&self, level: &str, title: &str, description: &str, cx: &mut App) {
        (self.host)(
            SettingsModalCommand::Toast {
                level: level.to_string(),
                title: title.to_string(),
                description: description.to_string(),
            },
            cx,
        );
    }

    // ---- what the app pushes -----------------------------------------------------------------

    /// A new `sidebarState` hydrate (after a save anywhere): the settings the modal shows follow
    /// it, keeping a debounced edit that has not been posted yet.
    pub(crate) fn receive_sidebar_state(&mut self, message: Value, cx: &mut Context<Self>) {
        if let Some(settings) = message
            .get("hud")
            .and_then(|hud| hud.get("settings"))
            .and_then(Value::as_object)
        {
            let mut settings = settings.clone();
            if let Some(pending) = &self.pending_patch {
                for (key, value) in pending {
                    settings.insert(key.clone(), value.clone());
                }
            }
            self.settings = SettingsValues::from_map(settings);
        }
        self.sidebar_state = message;
        self.refresh_search();
        cx.notify();
    }

    /// A transient `sidebarState` payload or a modal message for this modal.
    pub(crate) fn receive_host_payload(&mut self, payload: Value, cx: &mut Context<Self>) {
        let Some(kind) = payload
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        // A `partial` payload (the permission rows' quick re-check) updates only its own fields
        // of the last full answer, and waits for a full one when there is none yet.
        if payload.get("partial").and_then(Value::as_bool) == Some(true) {
            let Some(existing) = self
                .host_payloads
                .get_mut(&kind)
                .and_then(Value::as_object_mut)
            else {
                return;
            };
            if let Some(fields) = payload.as_object() {
                for (key, value) in fields {
                    if key != "partial" {
                        existing.insert(key.clone(), value.clone());
                    }
                }
            }
            // Its own event kind, so a page waiting for the full answer keeps waiting.
            cx.emit(SettingsStoreEvent::HostPayload(format!("{kind}:partial")));
            cx.notify();
            return;
        }
        self.apply_host_answer(&kind, &payload, cx);
        self.host_payloads.insert(kind.clone(), payload);
        cx.emit(SettingsStoreEvent::HostPayload(kind));
        cx.notify();
    }

    /// The answers the React modal applied at the modal level (use-app-icon-settings.ts (deleted 2026-10-01)), whatever
    /// page is open: a picked terminal background image or glass picture or video lands in the
    /// draft like a typed path (a refused video's error stays in its payload for the Theme page),
    /// and an `appIconState` confirms the icon a `setAppIcon` (Reset all settings) asked for.
    fn apply_host_answer(&mut self, kind: &str, payload: &Value, cx: &mut Context<Self>) {
        let light = payload.get("appearance").and_then(Value::as_str) == Some("light");
        let path = payload
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string);
        let has_error = payload
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|error| !error.is_empty());
        let key = match kind {
            "terminalBackgroundImageFilePicked" => "terminalBackgroundImage",
            "windowGlassImageFilePicked" if light => "windowGlassImageLight",
            "windowGlassImageFilePicked" => "windowGlassImageDark",
            "windowGlassVideoFilePicked" if has_error => return,
            "windowGlassVideoFilePicked" if light => "windowGlassVideoLight",
            "windowGlassVideoFilePicked" => "windowGlassVideoDark",
            "appIconState" => {
                if payload.get("ok").and_then(Value::as_bool) == Some(true)
                    && let Some(source) = payload.get("selectedId").and_then(Value::as_str)
                    && self.string("appIconSourceId") != source
                {
                    self.update_setting("appIconSourceId", Value::String(source.to_string()), cx);
                }
                return;
            }
            _ => return,
        };
        if let Some(path) = path {
            self.update_setting(key, Value::String(path), cx);
        }
    }

    pub(crate) fn host_payload(&self, kind: &str) -> Option<&Value> {
        self.host_payloads.get(kind)
    }

    /// The remote machine Agents and Integrations show; `None` is this computer.
    pub(crate) fn picked_machine(&self) -> Option<&str> {
        self.picked_machine.as_deref()
    }

    pub(crate) fn set_picked_machine(
        &mut self,
        machine_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.picked_machine != machine_id {
            self.picked_machine = machine_id;
            cx.notify();
        }
    }

    pub(crate) fn sidebar_state(&self) -> &Value {
        &self.sidebar_state
    }

    /// `hud` of the hydrate (agents, projects, commands, ...).
    pub(crate) fn hud(&self) -> Option<&Value> {
        self.sidebar_state.get("hud")
    }

    pub(crate) fn request(&self) -> &SettingsOpenRequest {
        &self.request
    }

    /// Clears a one-shot deep-link field once a page has acted on it.
    pub(crate) fn request_mut(&mut self) -> &mut SettingsOpenRequest {
        &mut self.request
    }

    pub(crate) fn palette(&self) -> SettingsPalette {
        self.palette
    }

    // ---- search -----------------------------------------------------------------------------

    pub(crate) fn search_query(&self) -> &str {
        &self.search_query
    }

    /// `isSettingsSearching`.
    pub(crate) fn is_searching(&self) -> bool {
        !js_trim(&self.search_query).is_empty()
    }

    pub(crate) fn set_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.search_query == query {
            return;
        }
        self.search_query = query;
        self.refresh_search();
        cx.notify();
    }

    fn refresh_search(&mut self) {
        let show_experimental = self.bool("showBetaFeatures");
        let mut hidden_keys =
            ghostex_settings_catalog::built_in_extensions::hidden_setting_keys_with(|key| {
                Some(self.bool(key))
            });
        hidden_keys.extend(ghostex_settings_catalog::availability::hidden_row_keys(
            ghostex_settings_catalog::Platform::current(),
            |key| Some(self.string(key)),
        ));
        let tab_hidden_keys = hidden_keys.clone();
        if self.general_search.query != self.search_query
            || self.general_search.show_experimental != show_experimental
            || self.general_search.hidden_keys != hidden_keys
            || self.general_search.sections.is_empty()
        {
            self.general_search =
                GeneralSearch::new(&self.search_query, show_experimental, hidden_keys);
        }
        let debugging_mode = self.bool("debuggingMode");
        self.tab_searches = settings_catalog()
            .extra_tabs
            .iter()
            .map(|tab| {
                (
                    tab.id.clone(),
                    extra_tab_search(
                        &self.search_query,
                        &tab.id,
                        debugging_mode,
                        &tab_hidden_keys,
                    ),
                )
            })
            .collect();
    }

    /// The search and visibility of the pages General split into (and Theme).
    pub(crate) fn general_search(&self) -> &GeneralSearch {
        &self.general_search
    }

    /// `extraSettingsTabSearches[tab]` for Integrations, Extensions, OS Integration, Remote,
    /// Projects, Agents, Accounts, Actions, Open In, Debugging and About.
    pub(crate) fn tab_search(&self, tab: SettingsTabId) -> TabSearch {
        self.tab_searches.get(tab.id()).cloned().unwrap_or_default()
    }

    /// `result.isSearching ? mainSettingVisible(result, key) : true` for the Theme page.
    pub(crate) fn theme_row_visible(&self, section: &str, key: &str) -> bool {
        let result: SectionSearch = self.general_search.section(section);
        !result.is_searching || self.general_search.setting_visible(section, key)
    }

    // ---- pages, scrolling, navigation memory -------------------------------------------------

    pub(crate) fn active_tab(&self) -> SettingsTabId {
        self.active_tab
    }

    /// `showOSIntegrationSettingsTab`.
    pub(crate) fn os_integration_visible(&self) -> bool {
        self.bool("showBetaFeatures")
    }

    /// CDXC:Connectors 2026-10-16 DECISION:
    /// User: Connectors is experimental, so "hide all these under 'experimental' until i test it well and tell you": the Connectors page shows only while Enable Experimental Features is on, the same switch gxserver's Connectors routes follow (server/src/connectors/gate.rs).
    pub(crate) fn connectors_visible(&self) -> bool {
        self.bool("showBetaFeatures")
    }

    /// A page a whole-feature built-in extension owns (Actions, Open In) leaves the rail and the
    /// search while that extension is off (`built_in_extensions::page_shown_with`).
    pub(crate) fn built_in_extension_allows_page(&self, tab: SettingsTabId) -> bool {
        ghostex_settings_catalog::built_in_extensions::page_shown_with(tab.id(), |key| {
            Some(self.bool(key))
        })
    }

    /// `resolveSettingsModalTabForVisibility`.
    pub(crate) fn resolve_tab(&self, tab: SettingsTabId) -> SettingsTabId {
        if (tab == SettingsTabId::OsIntegration && !self.os_integration_visible())
            || (tab == SettingsTabId::Connectors && !self.connectors_visible())
            || !self.built_in_extension_allows_page(tab)
        {
            SettingsTabId::Chat
        } else {
            tab
        }
    }

    /// `getInitialSettingsModalTab`: an explicit page wins; a plain Settings open returns to the
    /// page remembered this app run, else the one saved in `settingsModalNavigation`.
    ///
    /// CDXC:Settings 2026-10-05 WHY:
    /// An open that names General (`initialTab: "settings"`, as `ghostex settings open <key>` sends for every General setting) or carries a General deep link (`initialSection`, the Sidebar Tags `createTag` action) used to read as a plain open and returned to the remembered page, so the prefilled search ran on Agents or Accounts and said "No settings on this page match your search" for rows that exist under General. Only an open that names no page returns to the remembered one.
    ///
    /// CDXC:Settings 2026-10-29 WHY (in model.rs): General is several pages now, so a General
    /// section deep link (`initialSection: "power"`, `"sidebarTags"` with `createTag`) opens the
    /// page that renders that section, whichever General page (or `settings`) the message named.
    fn initial_tab(&self) -> SettingsTabId {
        let section_page = self
            .request
            .initial_section
            .as_deref()
            .and_then(super::search::general_page_of_section)
            .or_else(|| {
                self.request
                    .initial_sidebar_tags_action
                    .as_ref()
                    .map(|_| SettingsTabId::Sidebar)
            });
        if let Some(requested) = self.request.initial_tab {
            let tab = match section_page {
                Some(page) if requested.is_general_page() => page,
                _ => requested,
            };
            return self.resolve_tab(tab);
        }
        if let Some(page) = section_page {
            return self.resolve_tab(page);
        }
        let requested = self.request.requested_tab();
        let remembered = REMEMBERED_TAB.with(Cell::get).or_else(|| {
            self.value("settingsModalNavigation")
                .get("activeTab")
                .and_then(Value::as_str)
                .and_then(SettingsTabId::from_id)
        });
        self.resolve_tab(remembered.unwrap_or(requested))
    }

    /// `setActiveTab(nextTab)`: remembers where the page was scrolled, switches, and saves the
    /// navigation right away.
    pub(crate) fn set_active_tab(&mut self, tab: SettingsTabId, cx: &mut Context<Self>) {
        let tab = self.resolve_tab(tab);
        if self.page_history.get(self.page_history_index) != Some(&tab) {
            self.page_history.truncate(self.page_history_index + 1);
            self.page_history.push(tab);
            let overflow = self.page_history.len().saturating_sub(100);
            self.page_history.drain(..overflow);
            self.page_history_index = self.page_history.len() - 1;
        }
        self.show_tab(tab, cx);
    }

    /// CDXC:Settings 2026-09-28 DECISION:
    /// User: keep the current close behavior and use the mouse Back/Forward buttons to navigate between Settings pages.
    /// Pages hidden since they were visited (OS Integration, Connectors, a switched-off extension's page) are skipped, and the search clears.
    pub(crate) fn navigate_page_history(&mut self, back: bool, cx: &mut Context<Self>) {
        let mut index = self.page_history_index;
        loop {
            index = match (back, index) {
                (true, 0) => return,
                (true, index) => index - 1,
                (false, index) if index + 1 >= self.page_history.len() => return,
                (false, index) => index + 1,
            };
            let tab = self.page_history[index];
            let hidden = (tab == SettingsTabId::OsIntegration && !self.os_integration_visible())
                || (tab == SettingsTabId::Connectors && !self.connectors_visible())
                || !self.built_in_extension_allows_page(tab);
            if tab != self.active_tab && !hidden {
                self.page_history_index = index;
                self.set_search_query(String::new(), cx);
                self.show_tab(tab, cx);
                return;
            }
        }
    }

    fn show_tab(&mut self, tab: SettingsTabId, cx: &mut Context<Self>) {
        self.remember_active_scroll();
        if tab.in_extensions_group() {
            self.extensions_group_open = true;
        }
        REMEMBERED_TAB.with(|remembered| remembered.set(Some(tab)));
        self.active_tab = tab;
        self.persist_navigation(cx);
        cx.notify();
    }

    /// The page's scroll container handle.
    pub(crate) fn scroll_handle(&mut self, tab: SettingsTabId) -> ScrollHandle {
        self.scroll_handles.entry(tab).or_default().clone()
    }

    /// The page's scroll anchors.
    pub(crate) fn tracker(&mut self, tab: SettingsTabId) -> Rc<RefCell<SectionTracker>> {
        self.trackers.entry(tab).or_default().clone()
    }

    /// The General or Hotkeys section mostly in view (`activeMainSettingsSectionId`).
    pub(crate) fn active_section(&self, tab: SettingsTabId) -> Option<String> {
        self.trackers
            .get(&tab)
            .and_then(|tracker| tracker.borrow().active.clone())
    }

    /// Selects a page's section from the rail or a deep link: remembers it as active and scrolls
    /// it to the top of the page (under its 16px scroll margin) once it is laid out.
    pub(crate) fn scroll_to_section(
        &mut self,
        tab: SettingsTabId,
        section: &str,
        cx: &mut Context<Self>,
    ) {
        let tracker = self.tracker(tab);
        let anchor = if tab.is_general_page() {
            super::search::general_scroll_anchor(section)
        } else {
            section
        };
        {
            let mut tracker = tracker.borrow_mut();
            tracker.active = Some(section.to_string());
            tracker.pending_scroll = Some(anchor.to_string());
        }
        if self.active_tab != tab {
            self.set_active_tab(tab, cx);
        }
        cx.notify();
    }

    fn remember_active_scroll(&mut self) {
        let tab = self.active_tab;
        // A page still growing toward its remembered position keeps that position.
        if self
            .trackers
            .get(&tab)
            .is_some_and(|tracker| tracker.borrow().restoring.is_some())
        {
            return;
        }
        if let Some(handle) = self.scroll_handles.get(&tab) {
            remember_scroll_top(tab, (-f32::from(handle.offset().y)).max(0.0));
        }
    }

    /// Where page `tab` should open scrolled to: this run's memory, else the saved navigation.
    pub(crate) fn initial_scroll_top(&self, tab: SettingsTabId) -> f32 {
        remembered_scroll_top(tab).unwrap_or_else(|| {
            self.value("settingsModalNavigation")
                .get("scrollTopByTab")
                .and_then(|tops| tops.get(tab.id()))
                .and_then(Value::as_f64)
                .unwrap_or_default() as f32
        })
    }

    /// Restores page `tab`'s scroll position the first time it is laid out on this open.
    pub(crate) fn take_scroll_restore(&mut self, tab: SettingsTabId) -> Option<f32> {
        if self.restored_scroll.insert(tab, true).is_some() {
            return None;
        }
        // An explicit Agents section owns the scroll target for this open.
        if tab == SettingsTabId::Agents && self.request.initial_agents_section.is_some() {
            return None;
        }
        Some(self.initial_scroll_top(tab))
    }

    /// A page scrolled: remember it and save the navigation once scrolling settles.
    pub(crate) fn page_scrolled(&mut self, tab: SettingsTabId, cx: &mut Context<Self>) {
        if tab != self.active_tab {
            return;
        }
        self.remember_active_scroll();
        self.navigation_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(NAVIGATION_SCROLL_DEBOUNCE)
                .await;
            let _ = this.update(cx, |this, cx| this.persist_navigation(cx));
        }));
    }

    /// Whether page `tab`'s Advanced fold is open.
    pub(crate) fn advanced_fold_open(&self, tab: SettingsTabId) -> bool {
        self.advanced_folds.get(&tab).copied().unwrap_or(false)
    }

    /// Opens page `tab`'s Advanced fold without a re-render (a deep link found its section there
    /// while the page was already rendering).
    pub(crate) fn open_advanced_fold(&mut self, tab: SettingsTabId) {
        self.advanced_folds.insert(tab, true);
    }

    pub(crate) fn toggle_advanced_fold(&mut self, tab: SettingsTabId, cx: &mut Context<Self>) {
        let open = !self.advanced_fold_open(tab);
        self.advanced_folds.insert(tab, open);
        cx.notify();
    }

    /// `getRememberedSettingsModalNavigationState(activeTab, stored)`.
    fn navigation_state(&self) -> Value {
        let stored = self.value("settingsModalNavigation");
        let mut tops = Map::new();
        for tab in SettingsTabId::RAIL_ORDER {
            let top = remembered_scroll_top(tab).or_else(|| {
                stored
                    .get("scrollTopByTab")
                    .and_then(|tops| tops.get(tab.id()))
                    .and_then(Value::as_f64)
                    .map(|top| top as f32)
            });
            if let Some(top) = top.filter(|top| top.is_finite() && *top > 0.0) {
                tops.insert(
                    tab.id().to_string(),
                    json!(top.clamp(0.0, MAX_SCROLL_TOP).round()),
                );
            }
        }
        json!({
            "activeTab": self.active_tab.id(),
            "extensionsGroupOpen": self.extensions_group_open,
            "windowsLinkNoticeCollapsed": self.windows_link_notice_collapsed,
            "scrollTopByTab": Value::Object(tops),
            "version": 1,
        })
    }

    /// `persistSettingsModalNavigation`: posts the debounced edit and the page/scroll memory when
    /// either changed, as one patch.
    pub(crate) fn persist_navigation(&mut self, cx: &mut Context<Self>) {
        self.navigation_task = None;
        self.pending_patch_task = None;
        let pending = self.pending_patch.take();
        let next = self.navigation_state();
        let navigation_changed = self.value("settingsModalNavigation") != next;
        if pending.is_none() && !navigation_changed {
            return;
        }
        let source = if pending.is_some() {
            "settings:control"
        } else {
            "settings:navigation"
        };
        let mut patch = pending.unwrap_or_default();
        if navigation_changed {
            self.settings.set("settingsModalNavigation", next.clone());
            patch.insert("settingsModalNavigation".to_string(), next);
        }
        (self.host)(
            SettingsModalCommand::SavePatch {
                patch,
                source: source.to_string(),
            },
            cx,
        );
    }

    /// Whether the rail's Extensions group is open (a search shows it open whatever this says).
    pub(crate) fn extensions_group_open(&self) -> bool {
        self.extensions_group_open
    }

    /// The Extensions group's header: opens or closes the group and saves it.
    pub(crate) fn toggle_extensions_group(&mut self, cx: &mut Context<Self>) {
        self.extensions_group_open = !self.extensions_group_open;
        self.persist_navigation(cx);
        cx.notify();
    }

    /// The Windows Developer Mode notice is folded to its icon.
    pub(crate) fn windows_link_notice_collapsed(&self) -> bool {
        self.windows_link_notice_collapsed
    }

    /// Folds or opens the Windows Developer Mode notice on both pages, and saves it.
    pub(crate) fn toggle_windows_link_notice(&mut self, cx: &mut Context<Self>) {
        self.windows_link_notice_collapsed = !self.windows_link_notice_collapsed;
        self.persist_navigation(cx);
        cx.notify();
    }

    /// The dialogs and forms the last visit left open, for the pages to take back.
    pub(crate) fn open_forms_mut(&mut self) -> &mut OpenFormMemory {
        &mut self.open_forms
    }

    /// The app's handler of this modal's commands, which outlives the modal window.
    pub(crate) fn host(&self) -> SettingsModalHost {
        self.host.clone()
    }

    /// `closeSettingsModal`: saves the navigation and any pending edit, then tells the host.
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.remember_active_scroll();
        self.persist_navigation(cx);
        (self.host)(SettingsModalCommand::Close, cx);
    }
}

/// The scroll top that puts `child` (its window bounds this frame) `SECTION_SCROLL_MARGIN` below
/// the top of the viewport (`scrollIntoView({ block: 'start' })` with the anchors' scroll margin).
pub(crate) fn scroll_top_for_child(handle: &ScrollHandle, child: Bounds<Pixels>) -> f32 {
    let viewport: Bounds<Pixels> = handle.bounds();
    let offset = handle.offset();
    let content_top = child.origin.y - viewport.origin.y - offset.y;
    let max = handle.max_offset().y;
    f32::from(
        (content_top - px(SECTION_SCROLL_MARGIN))
            .max(px(0.0))
            .min(max),
    )
}

/// `vscode.postMessage(message)` from a page holding the store's entity.
pub(crate) fn post_store_message(store: &Entity<SettingsStore>, message: Value, cx: &mut App) {
    let host = store.read(cx).host.clone();
    host(SettingsModalCommand::PostMessage(message), cx);
}

/// A POST to the local gxserver from a page (`RemoteSetupRpc`, `AgentCliConnection.request`);
/// `reply` runs on the main thread with the result or the error message.
pub(crate) fn store_gxserver_rpc(
    store: &Entity<SettingsStore>,
    path: &str,
    params: Value,
    timeout: Duration,
    reply: impl FnOnce(Result<Value, String>, &mut App) + 'static,
    cx: &mut App,
) {
    let host = store.read(cx).host.clone();
    host(
        SettingsModalCommand::GxserverRpc {
            path: path.to_string(),
            params,
            timeout,
            reply: Box::new(reply),
        },
        cx,
    );
}

/// [`store_gxserver_rpc`] on the machine the page shows: this computer's gxserver for `None`, else
/// that remote machine's (`SettingsModalCommand::RemoteGxserverRpc`).
pub(crate) fn store_machine_gxserver_rpc(
    store: &Entity<SettingsStore>,
    machine_id: Option<&str>,
    path: &str,
    params: Value,
    timeout: Duration,
    reply: impl FnOnce(Result<Value, String>, &mut App) + 'static,
    cx: &mut App,
) {
    let Some(machine_id) = machine_id else {
        store_gxserver_rpc(store, path, params, timeout, reply, cx);
        return;
    };
    let host = store.read(cx).host.clone();
    host(
        SettingsModalCommand::RemoteGxserverRpc {
            machine_id: machine_id.to_string(),
            path: path.to_string(),
            params,
            timeout,
            reply: Box::new(reply),
        },
        cx,
    );
}

/// Copies `text` with the app's copy feedback (`playCopySound` and the "Copied!" bubble).
pub(crate) fn store_copy_to_clipboard(store: &Entity<SettingsStore>, text: String, cx: &mut App) {
    let host = store.read(cx).host.clone();
    host(SettingsModalCommand::CopyToClipboard(text), cx);
}

/// A plain HTTP GET (see `SettingsModalCommand::HttpGet`); `reply` runs on the main thread.
pub(crate) fn store_http_get(
    store: &Entity<SettingsStore>,
    url: String,
    reply: impl FnOnce(Result<Vec<u8>, String>, &mut App) + 'static,
    cx: &mut App,
) {
    let host = store.read(cx).host.clone();
    host(
        SettingsModalCommand::HttpGet {
            url,
            reply: Box::new(reply),
        },
        cx,
    );
}

/// `postAppModalHostMessage(message)`: a message for the app-modal host itself.
pub(crate) fn store_host_message(store: &Entity<SettingsStore>, message: Value, cx: &mut App) {
    let host = store.read(cx).host.clone();
    host(SettingsModalCommand::HostMessage(message), cx);
}

/// Creates the store entity.
pub(crate) fn new_settings_store(
    host: SettingsModalHost,
    palette: SettingsPalette,
    request: SettingsOpenRequest,
    sidebar_state: Value,
    open_forms: OpenFormMemory,
    cx: &mut App,
) -> Entity<SettingsStore> {
    cx.new(|_| SettingsStore::new(host, palette, request, sidebar_state, open_forms))
}
