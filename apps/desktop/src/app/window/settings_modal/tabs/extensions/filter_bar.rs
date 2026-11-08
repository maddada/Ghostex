//! The Extensions page's tab strip (Built-in, Views, Panels & menus, Installed, Community,
//! Experimental) with each tab's count, and the page's counts. The text filter is the Settings
//! search above it. The strip sits above the page's scroll area, so the cards scroll under it.
//!
//! CDXC:Extensions 2026-11-06 DECISION:
//! User, of the Extensions page showing Settings' "Search settings" over the bar's own "Search extensions" on a black sticky bar: "i dont like seeing 2 searches there, and the bg for search of black". The Settings search filters the cards (the bar's substring match, plus the Settings search's own fuzzy match of a built-in's catalog row) and the bar has no search field. The bar is drawn outside the scroll area on the window's own surface with no fill: the old sticky copy floated over the scrolling cards and needed an opaque fill, near-black under glass (memory: no black on the glass design).
use super::super::super::super::native_modal_kit::*;
use super::super::super::model::SettingsTabId;
use super::super::super::page::{CONTENT_GUTTER, CONTENT_MAX_WIDTH};
use super::super::super::palette::SettingsPalette;
use super::super::super::shell::BODY_PADDING;
use super::ExtensionsTab;
use super::data::{
    ExtensionFilter, OfficialExtension, SourceFilter, built_in_category_labels,
    built_in_filter_subject, custom_views, filter_store, official_extensions, store_categories,
    view_order_items,
};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};

/// What every group of the page shows under the current search and filter.
pub(crate) struct ExtensionCounts {
    pub(crate) show_built_in: bool,
    pub(crate) show_store: bool,
    pub(crate) show_custom: bool,
    pub(crate) built_in_shown: usize,
    pub(crate) store_shown: usize,
    pub(crate) custom_shown: usize,
    pub(crate) shown: usize,
    pub(crate) total: usize,
    pub(crate) built_in_visible: bool,
    pub(crate) store_visible: bool,
    pub(crate) custom_visible: bool,
    pub(crate) any_section: bool,
    pub(crate) categories: Vec<String>,
    pub(crate) sources: Vec<SourceFilter>,
}

impl ExtensionsTab {
    /// The page's filter: its source, type and category, and the Settings search as its text.
    pub(crate) fn page_filter(&self, cx: &gpui::App) -> ExtensionFilter {
        ExtensionFilter {
            query: self.store.read(cx).search_query().to_string(),
            ..self.filter.clone()
        }
    }

    /// A built-in card shows: it is offered here and it matches `filter`, by its own text or, for
    /// a typo the substring match misses, by the Settings search's fuzzy match of its catalog row.
    pub(crate) fn official_matches(
        &self,
        extension: &OfficialExtension,
        filter: &ExtensionFilter,
        cx: &gpui::App,
    ) -> bool {
        if !self.show_official(&extension.id, cx) {
            return false;
        }
        let subject = built_in_filter_subject(extension);
        if filter.matches(&subject) {
            return true;
        }
        let unsearched = ExtensionFilter {
            query: String::new(),
            ..filter.clone()
        };
        unsearched.matches(&subject)
            && self
                .store
                .read(cx)
                .tab_search(SettingsTabId::Extensions)
                .section("official")
                .visible_keys
                .contains(&extension.id)
    }

    /// `builtInCounts`, `filterStoreExtensions`, the custom views and the page's totals.
    pub(crate) fn extension_counts(&self, cx: &gpui::App) -> ExtensionCounts {
        // The Settings search narrows these groups by their cards, not by the catalog's one row
        // per group, so a Store extension the search names still shows.
        let has_transport = self.store.read(cx).request().gxserver_rpc_available;
        let show_built_in = true;
        let show_store = has_transport;
        let show_custom = true;
        let filter = &self.page_filter(cx);
        let (built_in_shown, built_in_total) = if show_built_in {
            let matching = official_extensions()
                .iter()
                .filter(|extension| self.official_matches(extension, filter, cx))
                .count();
            // CDXC:Extensions 2026-10-04 DECISION:
            // User: the count must be honest. Entries that need an agent CLI this computer does not have (Bots and Bot automations need the Hermes CLI) are not offered, so they are left out of the total as well as the list, and "N of M shown" no longer reads 37 of 39 with every filter on All.
            let offered = official_extensions()
                .iter()
                .filter(|extension| {
                    self.official_cli_found(&extension.id)
                        && self.official_gate_open(&extension.id, cx)
                })
                .count();
            (matching, offered)
        } else {
            (0, 0)
        };
        let catalog = self.browser.catalog_entries();
        let installed = &self.browser.installed;
        let (store_shown, store_total) = if show_store {
            let (installed_matches, store_matches) = filter_store(filter, catalog, installed);
            let (all_installed, all_store) =
                filter_store(&ExtensionFilter::default(), catalog, installed);
            (
                installed_matches.len() + store_matches.len(),
                all_installed.len() + all_store.len(),
            )
        } else {
            (0, 0)
        };
        let ordered = self.ordered_custom_views(cx);
        let (custom_shown, custom_total) = if show_custom {
            (
                ordered
                    .iter()
                    .filter(|view| filter.matches(&view.filter_subject()))
                    .count(),
                ordered.len(),
            )
        } else {
            (0, 0)
        };
        let active = filter.is_active();
        let mut categories = built_in_category_labels();
        let mut extra = store_categories(catalog, installed);
        extra.sort_by(|left, right| super::data::locale_compare(left, right));
        for category in extra {
            if !categories.contains(&category) {
                categories.push(category);
            }
        }
        let mut sources = Vec::new();
        if show_built_in {
            sources.push(SourceFilter::BuiltIn);
        }
        if show_store {
            sources.push(SourceFilter::Installed);
            sources.push(SourceFilter::Store);
        }
        if show_custom {
            sources.push(SourceFilter::Custom);
        }
        ExtensionCounts {
            show_built_in,
            show_store,
            show_custom,
            built_in_shown,
            store_shown,
            custom_shown,
            shown: built_in_shown + store_shown + custom_shown,
            total: built_in_total + store_total + custom_total,
            built_in_visible: show_built_in && (!active || built_in_shown > 0),
            store_visible: show_store && (!active || store_shown > 0),
            custom_visible: show_custom && (!active || custom_shown > 0),
            any_section: show_built_in || show_store || show_custom,
            categories,
            sources,
        }
    }

    /// Each tab's count for the strip, in `PageTab::ALL` order: what it lists, or its matches
    /// while a search is typed.
    pub(crate) fn page_tab_counts(&self, cx: &gpui::App) -> Vec<usize> {
        use super::page_tabs::PageTab;
        let filter = self.page_filter(cx);
        let built_in = |tab: PageTab| {
            official_extensions()
                .iter()
                .filter(|extension| {
                    tab.categories().contains(&extension.category.as_str())
                        && self.official_matches(extension, &filter, cx)
                })
                .count()
        };
        let (installed, community) = if self.store.read(cx).request().gxserver_rpc_available {
            let (installed, community) = filter_store(
                &filter,
                self.browser.catalog_entries(),
                &self.browser.installed,
            );
            (installed.len(), community.len())
        } else {
            (0, 0)
        };
        let collab = usize::from(super::collab_card::collab_matches(&filter));
        let custom = self
            .ordered_custom_views(cx)
            .iter()
            .filter(|view| filter.matches(&view.filter_subject()))
            .count();
        PageTab::ALL
            .iter()
            .map(|tab| match tab {
                PageTab::Views => built_in(*tab) + custom,
                PageTab::Installed => installed + collab,
                PageTab::Community => community,
                _ => built_in(*tab),
            })
            .collect()
    }

    /// `orderedCustomViews`: the custom views in the view order.
    pub(crate) fn ordered_custom_views(&self, cx: &gpui::App) -> Vec<super::data::CustomView> {
        let values = self.store.read(cx).values();
        let views = custom_views(&values);
        view_order_items(&values, &self.browser.installed)
            .into_iter()
            .filter_map(|item| {
                views
                    .iter()
                    .find(|view| format!("extension:{}", view.id()) == item.id)
                    .cloned()
            })
            .collect()
    }

    /// Clear filters: the bar's selects and the Settings search.
    pub(crate) fn clear_filter(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.filter = ExtensionFilter::default();
        self.store
            .update(cx, |store, cx| store.set_search_query(String::new(), cx));
        cx.notify();
    }

    /// CDXC:Extensions 2026-11-09 DECISION:
    /// User, on the Extensions tabs page: tabs across the top (Q2040) for Built-in, Views, Panels
    /// & menus, Installed, Community and Experimental, Experimental "the last one on the right" and
    /// "Get more should be Community" (Q2041); one search box over every tab and no source, type
    /// or category dropdowns (Q2042: the Settings search is that box); the page reopens on the
    /// tab you had open (Q2044, `extensionsTab` in `settingsModalNavigation`). Each tab carries
    /// its count, the matches while a search is typed, which shows every tab's matches at once.
    /// The strip sits above the page's scroll area, in the page's column, like the filter bar it
    /// replaces (CDXC:Extensions 2026-11-06).
    pub(crate) fn render_page_tabs(
        &mut self,
        p: &SettingsPalette,
        counts: &[usize],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let searching = self.store.read(cx).is_searching();
        let selected = super::page_tabs::PageTab::ALL
            .iter()
            .position(|tab| *tab == self.page_tab(cx))
            .unwrap_or(0);
        let items: Vec<ModalRailItem> = super::page_tabs::PageTab::ALL
            .iter()
            .zip(counts)
            .map(|(tab, count)| ModalRailItem {
                label: tab.label().into(),
                trailing: Some(count.to_string().into()),
                dot: None,
            })
            .collect();
        let strip = modal_bordered_segmented_control(
            p.foreground,
            p.hairline,
            p.muted,
            "extensions-page-tabs",
            &items,
            if searching { usize::MAX } else { selected },
            |page: &mut Self, index, _window, cx| {
                if let Some(tab) = super::page_tabs::PageTab::ALL.get(index) {
                    page.set_page_tab(*tab, cx);
                }
            },
            cx,
        )
        .w_full();
        div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .justify_center()
            .pl(px(CONTENT_GUTTER))
            .pr(px(CONTENT_GUTTER + BODY_PADDING))
            .child(
                div()
                    .id("extensions-page-tabs-bar")
                    .w_full()
                    .max_w(px(CONTENT_MAX_WIDTH))
                    .pt(px(8.0))
                    .pb(px(10.0))
                    .child(strip),
            )
            .into_any_element()
    }
}

/// `SharedString` of a label.
pub(crate) fn label(text: impl Into<SharedString>) -> SharedString {
    text.into()
}
