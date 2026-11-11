//! The Extensions page's On tab: everything that is on, from every other tab, under the tab it
//! belongs to.
//!
//! CDXC:Extensions 2026-11-11 DECISION:
//! User, picking way A on the "Everything that is on, in one place" options page (for 11.4.0): "An On tab, first in the strip", covering the Extensions tabs only. The first tab of the strip is On with its count; it lists every built-in, view of your own, installed extension and Collab that is on, grouped under the tab it belongs to with that group's count ("Built-in 7 on"), one card per row, each with its switch, and a card limited to some projects or Spaces says so on its row (its scope label). Turning something off here leaves its card in the list, switched off, until you leave the tab (another tab, another Settings page, or closing Settings), so a slip is one click to undo. A Community entry is never on by itself: once installed it is listed under Installed. The Settings search works as on every tab (it shows every tab's matches), and the strip's other counts are unchanged.
use super::super::super::palette::SettingsPalette;
use super::ExtensionsTab;
use super::cards::{
    EXTENSION_LIST_COLUMNS, GridCell, card_grid, card_group_with_note, empty_state,
};
use super::collab_card::collab_matches;
use super::data::{
    extension_view_scope_key, filter_store, is_official_enabled, official_blocked_by,
    official_categories, official_extensions,
};
use super::page_tabs::PageTab;
use super::sections::plain_section;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
};
use gpui_component::v_flex;

/// The tabs whose cards can be on, in the strip's order. Community lists only what is not
/// installed yet, so nothing in it is on.
const ON_GROUPS: [PageTab; 5] = [
    PageTab::BuiltIn,
    PageTab::Views,
    PageTab::PanelsMenus,
    PageTab::Installed,
    PageTab::Experimental,
];

impl ExtensionsTab {
    /// How many cards are on under the current search: the On tab's count in the strip.
    pub(crate) fn on_count(&self, cx: &gpui::App) -> usize {
        let filter = self.page_filter(cx);
        let values = self.store.read(cx).values();
        let built_in = official_extensions()
            .iter()
            .filter(|extension| {
                is_official_enabled(&values, extension)
                    && official_blocked_by(&values, extension).is_none()
                    && self.official_matches(extension, &filter, cx)
            })
            .count();
        let custom = self
            .ordered_custom_views(cx)
            .iter()
            .filter(|view| view.enabled() && filter.matches(&view.filter_subject()))
            .count();
        let installed = if self.store.read(cx).request().gxserver_rpc_available {
            let (installed, _) = filter_store(
                &filter,
                self.browser.catalog_entries(),
                &self.browser.installed,
            );
            let collab = usize::from(collab_matches(&filter) && self.collab_installed(cx));
            installed
                .into_iter()
                .filter(|index| self.browser.installed[*index].enabled())
                .count()
                + collab
        } else {
            0
        };
        built_in + custom + installed
    }

    /// Forgets the cards switched off on the On tab, when the tab is left.
    pub(crate) fn forget_on_tab_cards(&mut self) {
        self.on_tab_kept.clear();
    }

    /// Whether a card shows on the On tab: it is on, or it was shown during this visit.
    fn keep_on_card(&mut self, key: String, on: bool) -> bool {
        if on {
            self.on_tab_kept.insert(key);
            return true;
        }
        self.on_tab_kept.contains(&key)
    }

    pub(crate) fn render_on_section(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filter = self.page_filter(cx);
        let values = self.store.read(cx).values();
        let status = self
            .store
            .read(cx)
            .host_payload("pluginSettingsStatus")
            .cloned();
        let has_transport = self.store.read(cx).request().gxserver_rpc_available;
        let mut groups: Vec<AnyElement> = Vec::new();
        for tab in ON_GROUPS {
            let mut cells: Vec<GridCell> = Vec::new();
            let mut on = 0;
            for category in official_categories() {
                if !tab.categories().contains(&category.id.as_str()) {
                    continue;
                }
                for extension in official_extensions() {
                    if extension.category != category.id
                        || !self.official_matches(extension, &filter, cx)
                    {
                        continue;
                    }
                    let enabled = is_official_enabled(&values, extension)
                        && official_blocked_by(&values, extension).is_none();
                    if !self.keep_on_card(format!("official-{}", extension.id), enabled) {
                        continue;
                    }
                    on += usize::from(enabled);
                    cells.push(self.official_cell(
                        p,
                        extension,
                        &category.type_label,
                        &values,
                        status.as_ref(),
                        window,
                        cx,
                    ));
                }
            }
            if tab == PageTab::Views {
                for view in self.ordered_custom_views(cx) {
                    if !filter.matches(&view.filter_subject())
                        || !self.keep_on_card(format!("custom-{}", view.id()), view.enabled())
                    {
                        continue;
                    }
                    on += usize::from(view.enabled());
                    cells.push(GridCell::card(
                        self.custom_view_tile(p, &view, None, false, cx),
                    ));
                }
            }
            if tab == PageTab::Installed && has_transport {
                let (installed, _) = filter_store(
                    &filter,
                    self.browser.catalog_entries(),
                    &self.browser.installed,
                );
                for index in installed {
                    let extension = self.browser.installed[index].clone();
                    if !self
                        .keep_on_card(format!("installed-{}", extension.id()), extension.enabled())
                    {
                        continue;
                    }
                    on += usize::from(extension.enabled());
                    let card = self.installed_card(p, &extension, cx);
                    let key = extension_view_scope_key(&extension.id());
                    let wide = self.render_scope_editor_for(p, &key, window, cx);
                    cells.push(GridCell { card, wide });
                }
                let collab_on = self.collab_installed(cx);
                if collab_matches(&filter)
                    && self.keep_on_card("collab".to_string(), collab_on)
                    && let Some(card) = self.render_collab_card(p, &filter, cx)
                {
                    on += usize::from(collab_on);
                    cells.push(GridCell::card(card));
                }
            }
            if cells.is_empty() {
                continue;
            }
            let first = groups.is_empty();
            groups.push(card_group_with_note(
                p,
                tab.label(),
                Some(SharedString::from(format!("{on} on"))),
                first,
                card_grid(cells, None, EXTENSION_LIST_COLUMNS),
            ));
        }
        let body = if groups.is_empty() {
            empty_state(
                p,
                "modals/settings/puzzle.svg",
                "Nothing is on",
                "Turn something on in the other tabs and it shows here.",
                None,
            )
        } else {
            v_flex().w_full().children(groups).into_any_element()
        };
        plain_section(
            p,
            "On",
            div()
                .child("Everything you have turned on, from every tab. Something you turn off here stays until you leave the tab.")
                .into_any_element(),
            None,
            body,
        )
    }
}
