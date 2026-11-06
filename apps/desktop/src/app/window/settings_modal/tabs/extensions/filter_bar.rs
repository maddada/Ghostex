//! `ExtensionsFilterBar` (extensions-modal/extension-filter-bar.tsx (deleted 2026-10-01)) and the page's counts: the
//! source select (only when more than one source is on the page), the type and category selects
//! (searchable: eight or more items) and "N shown". The text filter is the Settings search above
//! it. The bar sits above the page's scroll area, so the cards scroll under its hairline.
//!
//! CDXC:Extensions 2026-11-06 DECISION:
//! User, of the Extensions page showing Settings' "Search settings" over the bar's own "Search extensions" on a black sticky bar: "i dont like seeing 2 searches there, and the bg for search of black". The Settings search filters the cards (the bar's substring match, plus the Settings search's own fuzzy match of a built-in's catalog row) and the bar has no search field. The bar is drawn outside the scroll area on the window's own surface with no fill: the old sticky copy floated over the scrolling cards and needed an opaque fill, near-black under glass (memory: no black on the glass design).
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    DropdownAlign, DropdownRow, DropdownState, icon, searchable_dropdown, settings_icon,
    settings_select, toggle_dropdown,
};
use super::super::super::model::SettingsTabId;
use super::super::super::page::{CONTENT_GUTTER, CONTENT_MAX_WIDTH};
use super::super::super::palette::SettingsPalette;
use super::super::super::shell::BODY_PADDING;
use super::ExtensionsTab;
use super::data::{
    EXTENSION_TYPE_FILTERS, ExtensionFilter, OfficialExtension, SourceFilter,
    built_in_category_labels, built_in_filter_subject, custom_views, extension_type_label,
    filter_store, official_extensions, store_categories, view_order_items,
};
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::h_flex;

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

fn type_dropdown(page: &mut ExtensionsTab) -> &mut DropdownState {
    &mut page.type_dropdown
}

fn category_dropdown(page: &mut ExtensionsTab) -> &mut DropdownState {
    &mut page.category_dropdown
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

    pub(crate) fn toggle_type_dropdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = EXTENSION_TYPE_FILTERS
            .iter()
            .position(|value| *value == self.filter.type_);
        self.category_dropdown.open = false;
        toggle_dropdown(self, type_dropdown, "Search...", selected, window, cx);
    }

    fn toggle_category_dropdown(
        &mut self,
        categories: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = if self.filter.category == "all" {
            Some(0)
        } else {
            categories
                .iter()
                .position(|category| *category == self.filter.category)
                .map(|index| index + 1)
        };
        self.type_dropdown.open = false;
        toggle_dropdown(self, category_dropdown, "Search...", selected, window, cx);
    }

    /// CDXC:Extensions 2026-09-24 DECISION:
    /// User: one filter bar (source, type, category, "N shown", the Settings search as its text) covers every extension on the Settings Extensions page, not only the Store list. Every group (built-in categories, installed, Store, the user's own views) is matched against the page's one filter and a group with no match disappears; the Settings-wide search still narrows the page first.
    /// The bar's content (`.extensions-filter-bar`).
    fn filter_bar_content(
        &mut self,
        p: &SettingsPalette,
        counts: &ExtensionCounts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let source_select = (counts.sources.len() > 1).then(|| {
            let mut options = vec![SettingOption {
                label: SourceFilter::All.label().to_string(),
                value: SourceFilter::All.id().to_string(),
            }];
            options.extend(counts.sources.iter().map(|source| SettingOption {
                label: source.label().to_string(),
                value: source.id().to_string(),
            }));
            settings_select(
                self,
                p,
                "extensions-source-filter",
                &options,
                self.filter.source.id(),
                Some(128.0),
                false,
                None,
                |page: &mut Self, value, _window, cx| {
                    page.filter.source = SourceFilter::from_id(&value);
                    cx.notify();
                },
                window,
                cx,
            )
        });
        let type_label = extension_type_label(&self.filter.type_);
        let type_trigger = self.dropdown_trigger(
            p,
            "extensions-type-filter",
            type_label,
            128.0,
            self.type_dropdown.trigger_bounds.clone(),
            self.type_dropdown.open,
            |page, window, cx| page.toggle_type_dropdown(window, cx),
            cx,
        );
        let category_trigger = (!counts.categories.is_empty()).then(|| {
            let categories = counts.categories.clone();
            let label = if self.filter.category == "all" {
                "All categories".to_string()
            } else {
                self.filter.category.clone()
            };
            // A Select of eight or more items is the searchable one; fewer stay a plain Select.
            if categories.len() + 1 < 8 {
                let mut options = vec![SettingOption {
                    label: "All categories".to_string(),
                    value: "all".to_string(),
                }];
                options.extend(categories.iter().map(|category| SettingOption {
                    label: category.clone(),
                    value: category.clone(),
                }));
                let value = self.filter.category.clone();
                return settings_select(
                    self,
                    p,
                    "extensions-category-plain",
                    &options,
                    &value,
                    Some(160.0),
                    false,
                    None,
                    |page: &mut Self, value, _window, cx| {
                        page.filter.category = value;
                        cx.notify();
                    },
                    window,
                    cx,
                );
            }
            self.dropdown_trigger(
                p,
                "extensions-category-filter",
                label,
                160.0,
                self.category_dropdown.trigger_bounds.clone(),
                self.category_dropdown.open,
                move |page, window, cx| page.toggle_category_dropdown(&categories, window, cx),
                cx,
            )
        });
        let count_text = if counts.shown == counts.total {
            format!("{} shown", counts.total)
        } else {
            format!("{} of {} shown", counts.shown, counts.total)
        };
        let type_menu = self.render_type_dropdown(p, window, cx);
        let category_menu = self.render_category_dropdown(p, &counts.categories, window, cx);
        h_flex()
            .w_full()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .children(source_select)
            .child(type_trigger)
            .children(category_trigger)
            .child(
                div()
                    .flex_1()
                    .flex_shrink_0()
                    .text_right()
                    .whitespace_nowrap()
                    .text_size(px(13.0))
                    .line_height(px(18.57))
                    .text_color(hsla(p.muted))
                    .child(count_text),
            )
            .children(type_menu)
            .children(category_menu)
            .into_any_element()
    }

    /// A searchable select's trigger (`SelectTrigger` with the chevron of a searchable select).
    #[allow(clippy::too_many_arguments)]
    fn dropdown_trigger(
        &mut self,
        p: &SettingsPalette,
        id: &'static str,
        label: String,
        width: f32,
        bounds: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
        open: bool,
        on_toggle: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = p.raised_hover;
        div()
            .flex_shrink_0()
            .w(px(width))
            .on_children_prepainted(capture_child_bounds(bounds, 0))
            .child(
                h_flex()
                    .id(id)
                    .w_full()
                    .h(px(32.0))
                    .px(px(12.0))
                    .gap(px(6.0))
                    .items_center()
                    .justify_between()
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(if open { p.focus_border } else { p.hairline }))
                    .bg(hsla(p.raised))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.foreground))
                    .cursor_pointer()
                    .hover(move |this| this.bg(hsla(hover)))
                    .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| {
                        on_toggle(page, window, cx);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(label),
                    )
                    .child(settings_icon(icon::CHEVRON_DOWN, 16.0, p.muted).flex_shrink_0()),
            )
            .into_any_element()
    }

    fn render_type_dropdown(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.type_dropdown.open {
            return None;
        }
        let query = self.type_dropdown.query(cx);
        let values: Vec<&'static str> = EXTENSION_TYPE_FILTERS
            .iter()
            .copied()
            .filter(|value| modal_select_filter_matches(&query, &extension_type_label(value)))
            .collect();
        let rows: Vec<DropdownRow> = values
            .iter()
            .map(|value| DropdownRow::new(extension_type_label(value)))
            .collect();
        searchable_dropdown(
            p,
            "extensions-type-menu",
            &self.type_dropdown,
            type_dropdown,
            &rows,
            "No results.",
            DropdownAlign::End,
            None,
            false,
            None,
            move |page: &mut Self, index, _window, cx| {
                if let Some(value) = values.get(index) {
                    page.filter.type_ = value.to_string();
                    cx.notify();
                }
            },
            window,
            cx,
        )
    }

    fn render_category_dropdown(
        &mut self,
        p: &SettingsPalette,
        categories: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.category_dropdown.open {
            return None;
        }
        let query = self.category_dropdown.query(cx);
        let mut values: Vec<(String, String)> =
            vec![("all".to_string(), "All categories".to_string())];
        values.extend(
            categories
                .iter()
                .map(|category| (category.clone(), category.clone())),
        );
        values.retain(|(_, label)| modal_select_filter_matches(&query, label));
        let rows: Vec<DropdownRow> = values
            .iter()
            .map(|(_, label)| DropdownRow::new(label.clone()))
            .collect();
        searchable_dropdown(
            p,
            "extensions-category-menu",
            &self.category_dropdown,
            category_dropdown,
            &rows,
            "No results.",
            DropdownAlign::End,
            None,
            false,
            None,
            move |page: &mut Self, index, _window, cx| {
                if let Some((value, _)) = values.get(index) {
                    page.filter.category = value.clone();
                    cx.notify();
                }
            },
            window,
            cx,
        )
    }

    /// `.extensions-filter-bar` over the page's scroll area, in the page's column (the gutters
    /// and the 924px width `settings_page` lays the sections out in): 8px above, 10px below, a
    /// hairline under it, and no fill, so it sits on the window like the Settings search does.
    pub(crate) fn render_filter_bar(
        &mut self,
        p: &SettingsPalette,
        counts: &ExtensionCounts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = self.filter_bar_content(p, counts, window, cx);
        div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .justify_center()
            .pl(px(CONTENT_GUTTER))
            .pr(px(CONTENT_GUTTER + BODY_PADDING))
            .child(
                div()
                    .id("extensions-filter-bar")
                    .w_full()
                    .max_w(px(CONTENT_MAX_WIDTH))
                    .pt(px(8.0))
                    .pb(px(10.0))
                    .border_b_1()
                    .border_color(hsla(p.hairline))
                    .child(content),
            )
            .into_any_element()
    }
}

/// `SharedString` of a label.
pub(crate) fn label(text: impl Into<SharedString>) -> SharedString {
    text.into()
}
