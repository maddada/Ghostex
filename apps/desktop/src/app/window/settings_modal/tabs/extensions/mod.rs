//! The Extensions page (packages/core-ui/settings-modal/tabs/extensions.tsx (deleted 2026-10-01)): Views (Arrange views),
//! one filter bar over every extension, the Built-in cards grouped by category, the Extensions
//! Store (installed and available cards, the detail page that replaces the whole page, the install
//! consent) and Your views (custom views with their editor and templates).
//!
//! CDXC:Extensions 2026-10-23 DECISION:
//! The page ends with Your views and has no "Related settings" link section: the user said "remove
//! this" about it. The account usage stars live only on Settings > Accounts.
//!
//! CDXC:Extensions 2026-09-28 WHY:
//! The page calls gxserver through the Settings host (`GxserverRpc`, `HttpGet`) instead of the
//! React page's `fetch` with the injected bootstrap, so it works without CEF; `has_transport` is
//! the host's "gxserver reachable" flag, the React `createExtensionsModalTransport()` check.
mod browser;
mod cards;
pub(crate) mod data;
mod delete_view;
mod detail;
mod filter_bar;
mod remembered_form;
mod scope_editor;
mod sections;
mod view_editor;
mod view_order;
mod views_data;

use super::super::fields::{DropdownState, FieldStates, SettingsPage};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::search::should_show_section;
use super::super::store::{SettingsStore, SettingsStoreEvent, post_store_message};
use browser::BrowserState;
use data::{ExtensionFilter, ViewScope, custom_views, official_agent_clis, view_scope};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyView, App, AppContext as _, Bounds, Context, Entity, FocusHandle, IntoElement,
    ParentElement as _, Pixels, Render, Styled as _, Window, div, px,
};
use gpui_component::input::InputState;
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;
use view_editor::ViewEditorState;
#[allow(unused_imports)]
pub(crate) use views_data::titlebar_view_order_rank;

/// Creates the Extensions page view.
pub(crate) fn extensions_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| ExtensionsTab::new(store.clone(), window, cx))
        .into()
}

/// `ViewScopeEditorState`: the scope being edited, which view it belongs to, and its title.
#[derive(Clone)]
pub(crate) struct ScopeEditorState {
    pub(crate) draft: ViewScope,
    pub(crate) key: String,
    pub(crate) title: String,
}

/// A drag of one custom view card: its index in the shown views and the slot it is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GridDrag {
    pub(crate) from: usize,
    pub(crate) over: usize,
}

pub(crate) struct ExtensionsTab {
    pub(super) store: Entity<SettingsStore>,
    fields: FieldStates,
    pub(crate) browser: BrowserState,
    pub(crate) filter: ExtensionFilter,
    pub(crate) filter_search: Option<Entity<InputState>>,
    pub(crate) type_dropdown: DropdownState,
    pub(crate) category_dropdown: DropdownState,
    pub(crate) view_editor: Option<ViewEditorState>,
    /// The view a Delete is asking about (`delete_view.rs`).
    pub(crate) delete_view: Option<delete_view::DeleteViewState>,
    pub(crate) delete_view_focus: FocusHandle,
    pub(crate) scope_editor: Option<ScopeEditorState>,
    pub(crate) scope_targets: DropdownState,
    pub(crate) scope_keep: DropdownState,
    pub(crate) choosing_template: bool,
    pub(crate) template_query: Option<Entity<InputState>>,
    pub(crate) arrange_open: bool,
    pub(crate) arrange_focus: FocusHandle,
    pub(crate) consent_focus: FocusHandle,
    /// Which of `OFFICIAL_EXTENSION_AGENT_CLIS` this computer has (`useInstalledAgentClis`).
    pub(crate) installed_clis: HashSet<String>,
    /// The deep links already acted on (`targetedCustomViewId`, `targetedViewScopeKey`).
    targeted_view: Option<String>,
    targeted_scope: Option<String>,
    /// Scroll the custom view editor to the top and focus its first field once laid out.
    pub(crate) focus_view_editor: Rc<Cell<bool>>,
    /// Scroll the scope editor to the middle of the page once laid out.
    pub(crate) center_scope_editor: Rc<Cell<bool>>,
    /// The list's scroll offset while it shows, restored when the detail page closes.
    list_scroll_top: f32,
    detail_was_open: bool,
    was_active: bool,
    pub(crate) plugin_status_loading: bool,
    /// The custom view card being dragged (`DragDropProvider` of "Your views").
    pub(crate) grid_drag: Option<GridDrag>,
    /// Where the filter bar sits in the page (for the sticky copy) and the page's viewport.
    pub(crate) filter_slot: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The preferences being edited on an installed extension's page.
    pub(crate) preferences_draft: Option<(String, serde_json::Map<String, Value>)>,
    /// The inputs masked as passwords (`sync_masked`).
    pub(crate) masked_inputs: std::collections::HashMap<gpui::SharedString, bool>,
    /// The page opened a preview state's dialog or menu already.
    preview_applied: bool,
}

impl SettingsPage for ExtensionsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl ExtensionsTab {
    pub(crate) fn new(
        store: Entity<SettingsStore>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // The page view outlives a switch to another page, which is when it closes and reopens.
        cx.observe(&store, |page, _, cx| {
            page.sync_active(cx);
            cx.notify();
        })
        .detach();
        cx.subscribe(&store, |page, _, event: &SettingsStoreEvent, cx| {
            let SettingsStoreEvent::HostPayload(kind) = event;
            if kind == "pluginSettingsStatus" {
                page.plugin_status_loading = false;
                cx.notify();
            }
        })
        .detach();
        let mut page = Self {
            store,
            fields: FieldStates::default(),
            browser: BrowserState::default(),
            filter: ExtensionFilter::default(),
            filter_search: None,
            type_dropdown: DropdownState::default(),
            category_dropdown: DropdownState::default(),
            view_editor: None,
            delete_view: None,
            delete_view_focus: cx.focus_handle(),
            scope_editor: None,
            scope_targets: DropdownState::default(),
            scope_keep: DropdownState::default(),
            choosing_template: false,
            template_query: None,
            arrange_open: false,
            arrange_focus: cx.focus_handle(),
            consent_focus: cx.focus_handle(),
            installed_clis: HashSet::new(),
            targeted_view: None,
            targeted_scope: None,
            focus_view_editor: Rc::new(Cell::new(false)),
            center_scope_editor: Rc::new(Cell::new(false)),
            list_scroll_top: 0.0,
            detail_was_open: false,
            was_active: false,
            plugin_status_loading: false,
            grid_drag: None,
            filter_slot: Rc::new(Cell::new(None)),
            preferences_draft: None,
            masked_inputs: std::collections::HashMap::new(),
            preview_applied: false,
        };
        page.request_plugin_status_if_missing(cx);
        page.read_installed_clis(cx);
        page.restore_open_form(cx);
        page
    }

    pub(crate) fn settings_store_ref<'a>(&self, cx: &'a App) -> &'a SettingsStore {
        self.store.read(cx)
    }

    /// The modal host asks for the component status when Settings opens without one.
    fn request_plugin_status_if_missing(&mut self, cx: &mut Context<Self>) {
        if self
            .store
            .read(cx)
            .host_payload("pluginSettingsStatus")
            .is_some()
        {
            return;
        }
        self.request_plugin_status(cx);
    }

    /// `onRequestPluginSettingsStatus`.
    pub(crate) fn request_plugin_status(&mut self, cx: &mut Context<Self>) {
        self.plugin_status_loading = true;
        post_store_message(
            &self.store,
            json!({ "type": "requestPluginSettingsStatus" }),
            cx,
        );
        cx.notify();
    }

    /// `onReinstallPlugin(pluginId)`.
    pub(crate) fn reinstall_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        self.plugin_status_loading = true;
        post_store_message(
            &self.store,
            json!({ "pluginId": plugin_id, "type": "reinstallPlugin" }),
            cx,
        );
        cx.notify();
    }

    /// `useInstalledAgentClis(OFFICIAL_EXTENSION_AGENT_CLIS)`: reads each CLI through gxserver's
    /// agent detection; only a found CLI is remembered.
    fn read_installed_clis(&mut self, cx: &mut Context<Self>) {
        if !self.has_transport(cx) {
            return;
        }
        for agent_id in official_agent_clis() {
            let this = cx.weak_entity();
            let id = agent_id.clone();
            super::super::store::store_gxserver_rpc(
                &self.store.clone(),
                "/api/agentCliMaintenance",
                json!({ "action": "read", "agentId": agent_id }),
                Duration::from_secs(60),
                move |result, cx| {
                    let found = result
                        .ok()
                        .and_then(|state| state.get("executablePath").cloned())
                        .is_some_and(|path| path.as_str().is_some_and(|path| !path.is_empty()));
                    if found {
                        let _ = this.update(cx, |page, cx| {
                            page.installed_clis.insert(id);
                            cx.notify();
                        });
                    }
                },
                cx,
            );
        }
    }

    /// `showOfficial(key)`: the Settings search leaves the entry and its agent CLI is found.
    pub(crate) fn show_official(&self, key: &str, cx: &App) -> bool {
        let search = self.store.read(cx).tab_search(SettingsTabId::Extensions);
        let section = search.section("official");
        if !super::super::search::should_show_setting(&section, key, None) {
            return false;
        }
        let required = data::official_extensions()
            .iter()
            .find(|extension| extension.id == key)
            .and_then(|extension| extension.requires_agent_cli.clone());
        required.is_none_or(|cli| self.installed_clis.contains(&cli))
            && self.official_gate_open(key, cx)
    }

    /// The setting gating the entry (`gatedBy`: Enable Experimental Features) is on;
    /// while it is off the entry is not offered at all (CDXC:Extensions 2026-10-19).
    pub(crate) fn official_gate_open(&self, key: &str, cx: &App) -> bool {
        let store = self.store.read(cx);
        ghostex_settings_catalog::built_in_extensions::gate_open_with(key, |setting| {
            Some(store.bool(setting))
        })
    }

    /// The entry's agent CLI (if it needs one) was found; the Settings search is not asked.
    pub(crate) fn official_cli_found(&self, key: &str) -> bool {
        data::official_extensions()
            .iter()
            .find(|extension| extension.id == key)
            .and_then(|extension| extension.requires_agent_cli.as_ref())
            .is_none_or(|cli| self.installed_clis.contains(cli))
    }

    /// The page became (in)active: the browser state resets and reloads as the React effect on
    /// `active` did.
    fn sync_active(&mut self, cx: &mut Context<Self>) {
        let active = self.store.read(cx).active_tab() == SettingsTabId::Extensions;
        if active == self.was_active {
            return;
        }
        self.was_active = active;
        if !active {
            self.targeted_view = None;
            self.targeted_scope = None;
            return;
        }
        self.browser.selected_installed = None;
        self.browser.selected_store = None;
        self.browser.consent = None;
        self.load_browser(cx);
    }

    /// CDXC:Extensions 2026-09-26 DECISION:
    /// User: Configure view on a view tab scrolls Settings to that view. Views without their own editor (Storybook, built-in and store views) open their scope editor instead, so the page scrolls to that editor the same way it scrolls to a custom view's.
    /// `initialCustomViewId` / `initialViewScopeKey`: open that view's editor once.
    fn apply_deep_links(&mut self, cx: &mut Context<Self>) {
        let (view_id, scope_key, values) = {
            let store = self.store.read(cx);
            (
                store.request().initial_custom_view_id.clone(),
                store.request().initial_view_scope_key.clone(),
                store.values(),
            )
        };
        if let Some(view_id) = view_id
            && self.targeted_view.as_deref() != Some(view_id.as_str())
            && let Some(view) = custom_views(&values)
                .into_iter()
                .find(|view| view.id() == view_id)
        {
            self.targeted_view = Some(view_id.clone());
            self.focus_view_editor.set(true);
            self.choosing_template = false;
            self.view_editor = Some(ViewEditorState::edit(&view));
        }
        if let Some(key) = scope_key
            && self.targeted_scope.as_deref() != Some(key.as_str())
            && let Some(title) = self.scope_editor_title(&key, cx)
        {
            self.targeted_scope = Some(key.clone());
            self.center_scope_editor.set(true);
            let draft = view_scope(&values.value("viewScopes"), &key);
            self.scope_editor = Some(ScopeEditorState { draft, key, title });
        }
    }

    /// `viewScopeEditorTitle(key)`.
    fn scope_editor_title(&self, key: &str, cx: &App) -> Option<String> {
        if let Some(official) = data::official_extensions()
            .iter()
            .find(|extension| data::official_view_scope_key(&extension.id) == key)
        {
            return Some(official.title.clone());
        }
        let values = self.store.read(cx).values();
        if let Some(view) = custom_views(&values)
            .into_iter()
            .find(|view| data::extension_view_scope_key(&view.id()) == key)
        {
            return Some(view.name());
        }
        self.browser
            .installed
            .iter()
            .find(|extension| data::extension_view_scope_key(&extension.id()) == key)
            .map(|extension| extension.title())
    }

    /// The preview binary's state: the dialogs and menus no open message reaches.
    fn apply_preview_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_applied {
            return;
        }
        let Some(state) = self.store.read(cx).request().preview_state.clone() else {
            self.preview_applied = true;
            return;
        };
        match state.as_str() {
            "extensions-arrange" => {
                self.preview_applied = true;
                self.open_arrange(window, cx);
            }
            "extensions-filter" => {
                self.preview_applied = true;
                self.filter.type_ = "view".into();
                self.filter.category = "Project websites".into();
            }
            "extensions-empty" => {
                self.preview_applied = true;
                self.filter.query = "zzzz".into();
                if let Some(input) = self.filter_search.clone() {
                    input.update(cx, |input, cx| input.set_value("zzzz", window, cx));
                }
            }
            "extensions-select" => {
                if self.type_dropdown.trigger_bounds.get().is_some() {
                    self.preview_applied = true;
                    self.toggle_type_dropdown(window, cx);
                } else {
                    cx.notify();
                }
            }
            "extensions-scope-menu" => {
                if self.scope_targets.trigger_bounds.get().is_some() {
                    self.preview_applied = true;
                    self.toggle_scope_dropdown(true, window, cx);
                } else {
                    cx.notify();
                }
            }
            "extensions-detail" | "extensions-store-detail" | "extensions-consent" => {
                if self.browser.catalog.is_none() {
                    return;
                }
                self.preview_applied = true;
                match state.as_str() {
                    "extensions-detail" => {
                        self.browser.selected_installed = Some("storybook-runner".into());
                    }
                    "extensions-store-detail" => {
                        self.select_store_entry(Some("port-watch".into()), cx);
                    }
                    _ => {
                        self.select_store_entry(Some("port-watch".into()), cx);
                        self.open_consent("port-watch".into(), window, cx);
                    }
                }
            }
            "extensions-delete-view" => {
                let values = self.store.read(cx).values();
                let editing = self
                    .view_editor
                    .as_ref()
                    .and_then(|editor| editor.id.clone());
                let Some(view) = custom_views(&values)
                    .into_iter()
                    .find(|view| editing.as_ref().is_none_or(|id| *id == view.id()))
                else {
                    return;
                };
                self.preview_applied = true;
                self.request_delete_view(view.id(), view.name().to_string(), window, cx);
            }
            "extensions-store" | "extensions-views" | "extensions-templates" => {
                if self.browser.catalog.is_none() {
                    return;
                }
                self.preview_applied = true;
                self.choosing_template = state == "extensions-templates";
                let section = match state.as_str() {
                    "extensions-store" => "store",
                    _ => "customViews",
                };
                let store = self.store.clone();
                store.update(cx, |store, cx| {
                    store.scroll_to_section(SettingsTabId::Extensions, section, cx)
                });
            }
            _ => self.preview_applied = true,
        }
    }

    /// Opens the install consent for a catalog entry.
    pub(crate) fn open_consent(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.browser.consent = Some(name);
        self.consent_focus.focus(window, cx);
        cx.notify();
    }

    /// Remembers the list's scroll offset while it shows; starts the detail page at the top and
    /// puts the list back where it was when the detail page closes.
    fn sync_detail_scroll(&mut self, detail_open: bool, cx: &mut Context<Self>) {
        let handle = self.store.update(cx, |store, _| {
            store.scroll_handle(SettingsTabId::Extensions)
        });
        if detail_open == self.detail_was_open {
            if !detail_open {
                self.list_scroll_top = (-f32::from(handle.offset().y)).max(0.0);
            }
            return;
        }
        self.detail_was_open = detail_open;
        let offset = handle.offset();
        let top = if detail_open {
            0.0
        } else {
            self.list_scroll_top
        };
        handle.set_offset(gpui::point(offset.x, px(-top)));
    }
}

impl Render for ExtensionsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_active(cx);
        self.apply_deep_links(cx);
        self.apply_preview_state(window, cx);
        let (p, search, searching, has_transport) = {
            let store = self.store.read(cx);
            (
                store.palette(),
                store.tab_search(SettingsTabId::Extensions),
                store.is_searching(),
                store.request().gxserver_rpc_available,
            )
        };
        let detail_open = has_transport && self.browser.detail_open();
        self.sync_detail_scroll(detail_open, cx);
        let mut blocks: Vec<PageBlock> = Vec::new();
        if detail_open {
            let detail = self.render_detail(&p, window, cx);
            blocks.push(PageBlock::plain(div().w_full().pt(px(20.0)).child(detail)));
            if let Some(consent) = self.render_consent(&p, window, cx) {
                blocks.push(PageBlock::plain(consent));
            }
            return settings_page(&self.store, SettingsTabId::Extensions, &p, blocks, cx);
        }
        if searching && !search.has_matches() {
            let store = self.store.clone();
            let matching: Vec<SettingsTabId> = rail_pages(self.store.read(cx))
                .into_iter()
                .map(|page| page.tab)
                .collect();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Extensions,
                &matching,
                move |tab, _, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        if should_show_section(&search.section("viewOrder"), None)
            && let Some(section) = self.render_views_section(&p, cx)
        {
            blocks.push(PageBlock::section("viewOrder", section));
        }
        let counts = self.extension_counts(cx);
        if counts.any_section {
            let bar = self.render_filter_bar(&p, &counts, window, cx);
            blocks.push(PageBlock::plain(bar));
        }
        if counts.any_section && self.filter.is_active() && counts.shown == 0 {
            blocks.push(PageBlock::plain(cards::empty_state_filter(
                &p,
                |page: &mut Self, window, cx| page.clear_filter(window, cx),
                cx,
            )));
        }
        if counts.built_in_visible {
            let section = self.render_built_in_section(&p, window, cx);
            blocks.push(PageBlock::section("official", section));
        }
        if counts.store_visible {
            let section = self.render_store_section(&p, window, cx);
            blocks.push(PageBlock::section("store", section));
        }
        if counts.custom_visible {
            let section = self.render_custom_views_section(&p, window, cx);
            blocks.push(PageBlock::section("customViews", section));
        }
        if let Some(dialog) = self.render_arrange_dialog(&p, window, cx) {
            blocks.push(PageBlock::plain(dialog));
        }
        if let Some(consent) = self.render_consent(&p, window, cx) {
            blocks.push(PageBlock::plain(consent));
        }
        if let Some(confirm) = self.render_delete_view(&p, window, cx) {
            blocks.push(PageBlock::plain(confirm));
        }
        let page = settings_page(&self.store, SettingsTabId::Extensions, &p, blocks, cx);
        let sticky = self.render_sticky_filter_bar(&p, &counts, window, cx);
        div()
            .relative()
            .size_full()
            .child(page)
            .when_some(sticky, |this, sticky| this.child(sticky))
            .into_any_element()
    }
}
