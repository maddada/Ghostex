//! The pages the General page split into (CDXC:Settings 2026-10-29 in model.rs), one `GeneralTab`
//! per page: Chat; Sidebar & Sessions (Sidebar, Session Cards, Sidebar Tags, Sleeping Sessions);
//! Terminal (Terminal, Terminal Behavior, Terminal Scrolling); Notifications (Sounds); Files &
//! Links (File opening, Browser, Editor, Dev Servers); Power & Sleep (When your computer starts,
//! Memory, Auto Sleep, Power). They
//! share the General search (`GeneralSearch`). Each page's advanced rows render in a second pass
//! into a collapsed Advanced fold at its bottom (CDXC:Settings 2026-10-29 in search.rs).
//! Experimental and Reset to defaults moved to About.
mod chat_tools;
mod commands_pane;
mod computer_startup;
mod marked_sections_preview;
mod memory_guard;
mod powershell;
mod sidebar;
mod system;
mod terminal;
pub(super) mod terminal_font;
mod terminal_transparency;

use super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::fields::advanced_fold_header;
use super::super::fields::{
    FieldStates, PageAction, RowSpec, SettingsPage, SliderBinding, reset_key, segmented_field,
    select_field, slider_number_field, toggle_field,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::search::{GeneralSearch, RowTier};
use super::super::store::{SettingsStore, SettingsValues};
use gpui::{
    AnyElement, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Window, div,
};
use serde_json::{Value, json};

pub(crate) struct GeneralTab {
    store: Entity<SettingsStore>,
    /// Which of the General pages this is (`SettingsTabId::is_general_page`).
    page: SettingsTabId,
    fields: FieldStates,
    powershell: powershell::PowerShellState,
    /// The Marked sections style Preview dialog, while it is open (its focus, for Escape).
    marked_preview: Option<gpui::FocusHandle>,
    /// The playbook overrides and their editor.
    orchestrator_playbook: super::orchestrator_playbook::OrchestratorPlaybookState,
}

impl GeneralTab {
    pub(crate) fn new(
        store: Entity<SettingsStore>,
        tab: SettingsTabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        debug_assert!(tab.is_general_page(), "{tab:?} is not a General page");
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let preview_state = store.read(cx).request().preview_state.clone();
        let chat = tab == SettingsTabId::Chat;
        let mut page = Self {
            store,
            page: tab,
            fields: FieldStates::default(),
            powershell: powershell::PowerShellState::default(),
            // The preview binary's `marked-preview` state opens the Marked sections Preview.
            marked_preview: (chat && preview_state.as_deref() == Some("marked-preview"))
                .then(|| cx.focus_handle()),
            orchestrator_playbook: Default::default(),
        };
        if tab == SettingsTabId::Terminal
            && settings_catalog().flag(module::SEARCH_CATALOG, "IS_WINDOWS_HOST")
        {
            page.load_powershell(cx);
        }
        // The preview binary's `orchestrator-playbook…-edit` and `…-armed` states open its editor.
        if let Some(state) = preview_state
            .as_deref()
            .filter(|_| chat)
            .filter(|state| super::orchestrator_playbook::preview_opens_editor(state))
            .filter(|state| !state.starts_with("projects"))
        {
            super::orchestrator_playbook::open_playbook_sheet(&mut page, window, cx);
            if state.ends_with("-armed") {
                super::orchestrator_playbook::preview_arm_switch(&mut page);
            }
        }
        // `orchestrator-playbook…-view` opens the read-only playbook.
        if preview_state.as_deref().is_some_and(|state| {
            chat && state.starts_with("orchestrator-playbook") && state.ends_with("-view")
        }) {
            super::orchestrator_playbook::open_playbook_view(&mut page, window, cx);
        }
        page
    }
}

impl super::orchestrator_playbook::HasOrchestratorPlaybook for GeneralTab {
    fn orchestrator_playbook(
        &mut self,
    ) -> &mut super::orchestrator_playbook::OrchestratorPlaybookState {
        &mut self.orchestrator_playbook
    }
}

impl SettingsPage for GeneralTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// What every General section reads while it renders.
#[derive(Clone)]
pub(super) struct GeneralCx {
    pub(super) p: SettingsPalette,
    pub(super) values: SettingsValues,
    pub(super) search: GeneralSearch,
    /// Power shows only while Keep Awake is on (Settings > Extensions > Experimental).
    pub(super) power_visible: bool,
    /// The Sidebar Tags deep link asked for the New tag form.
    pub(super) create_tag: bool,
    /// The anchors of the sections the page already shows above the Advanced fold.
    pub(super) shown_above_fold: Vec<String>,
}

impl GeneralCx {
    /// CDXC:Settings 2026-11-07 WHY:
    /// The Advanced fold reuses each page's section builders, so a section with both everyday and advanced rows used to show its heading twice ("Sidebar" and "Session Cards" again under Advanced on Sidebar & Sessions). Inside the fold such a section is titled "More <title> settings"; a section whose rows are all advanced keeps its own title, since it shows only once.
    pub(super) fn section_title(&self, anchor: &str, title: &str) -> SharedString {
        if self.shown_above_fold.iter().any(|shown| shown == anchor) {
            format!("More {title} settings").into()
        } else {
            SharedString::from(title.to_string())
        }
    }

    /// `mainSettingVisible(settingsSearch[section], key)`.
    pub(super) fn visible(&self, section: &str, key: &str) -> bool {
        self.search.setting_visible(section, key)
    }

    /// A row that stands for several settings (`GeneralSearch::any_visible`).
    pub(super) fn any_visible(&self, section: &str, keys: &[&str]) -> bool {
        self.search.any_visible(section, keys)
    }

    pub(super) fn options(&self, name: &str) -> Vec<SettingOption> {
        settings_catalog().options(module::SETTINGS, name)
    }

    pub(super) fn number(&self, name: &str) -> f64 {
        settings_catalog().number(module::SETTINGS, name)
    }

    /// The keyed row of a setting with `getSettingModificationProps(key)`.
    pub(super) fn spec(&self, key: &str, label: &str, description: &str) -> RowSpec {
        RowSpec::new(label.to_string())
            .description(description.to_string())
            .keyed(&self.values, key)
    }
}

fn save<V: SettingsPage>(page: &mut V, key: &'static str, value: Value, cx: &mut Context<V>) {
    let store = page.settings_store().clone();
    store.update(cx, |store, cx| store.update_setting(key, value, cx));
}

impl GeneralTab {
    /// A `ToggleField` bound to `key`.
    pub(super) fn toggle(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        label: &str,
        description: &str,
        dependent: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let mut spec = g.spec(key, label, description);
        if dependent {
            spec = spec.dependent();
        }
        Some(toggle_field(self, &g.p, key, spec, g.values.bool(key), cx))
    }

    /// A `SelectField` bound to a string `key`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn select(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        label: &str,
        description: &str,
        options: Vec<SettingOption>,
        width: Option<f32>,
        dependent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let mut spec = g.spec(key, label, description);
        if dependent {
            spec = spec.dependent();
        }
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = g.values.choice(key, &allowed);
        Some(select_field(
            self,
            &g.p,
            key,
            spec,
            Some(reset_key::<Self>(key)),
            &options,
            &value,
            width,
            move |page: &mut Self, next, _window, cx| save(page, key, json!(next), cx),
            window,
            cx,
        ))
    }

    /// A `SelectField` over numeric options (`String(draft[key])`, `Number(value)`).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn number_select(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        label: &str,
        description: &str,
        options: Vec<SettingOption>,
        dependent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let mut spec = g.spec(key, label, description);
        if dependent {
            spec = spec.dependent();
        }
        let value = g.values.number_string(key);
        Some(select_field(
            self,
            &g.p,
            key,
            spec,
            Some(reset_key::<Self>(key)),
            &options,
            &value,
            None,
            move |page: &mut Self, next, _window, cx| {
                let number: f64 = next.parse().unwrap_or_default();
                let value = if number.fract() == 0.0 {
                    json!(number as i64)
                } else {
                    json!(number)
                };
                save(page, key, value, cx);
            },
            window,
            cx,
        ))
    }

    /// A `SliderNumberField` bound to `key`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn slider(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        label: &str,
        description: &str,
        range: (f64, f64, f64),
        dependent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let mut spec = g.spec(key, label, description);
        if dependent {
            spec = spec.dependent();
        }
        let (min, max, step) = range;
        Some(slider_number_field(
            self,
            &g.p,
            spec,
            Some(reset_key::<Self>(key)),
            SliderBinding {
                key,
                min,
                max,
                step,
            },
            g.values.f64(key),
            window,
            cx,
        ))
    }

    /// A segmented field bound to a string `key`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn segmented(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        spec: RowSpec,
        on_reset: Option<PageAction<Self>>,
        options: Vec<SettingOption>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = g.values.choice(key, &allowed);
        Some(segmented_field(
            &g.p,
            key,
            spec,
            on_reset,
            &options,
            Some(&value),
            None,
            move |page: &mut Self, next, _window, cx| save(page, key, json!(next), cx),
            cx,
        ))
    }
}

impl Render for GeneralTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (g, matching_pages) = {
            let store = self.store.read(cx);
            let g = GeneralCx {
                p: store.palette(),
                values: store.values(),
                search: store.general_search().clone(),
                power_visible: store.keep_awake_on(),
                create_tag: store.request().initial_sidebar_tags_action.as_deref()
                    == Some("createTag"),
                shown_above_fold: Vec::new(),
            };
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (g, matching)
        };
        let tab = self.page;
        let sections =
            |page: &mut Self, g: &GeneralCx, window: &mut Window, cx: &mut Context<Self>| match tab
            {
                SettingsTabId::Chat => chat_tools::chat_sections(page, g, window, cx),
                SettingsTabId::Sidebar => sidebar::sections(page, g, window, cx),
                SettingsTabId::Terminal => terminal::sections(page, g, window, cx),
                SettingsTabId::Notifications => system::notifications_sections(page, g, window, cx),
                SettingsTabId::FilesLinks => chat_tools::files_links_sections(page, g, window, cx),
                SettingsTabId::PowerSleep => system::power_sleep_sections(page, g, window, cx),
                _ => Vec::new(),
            };
        let has_visible = tab
            .general_group()
            .is_some_and(|group| g.search.group_visible(group));
        // The everyday rows, then the advanced rows for the fold (CDXC:Settings 2026-10-29 in
        // search.rs). A search renders every match in the first pass and nothing in the second.
        let everyday = GeneralCx {
            search: g.search.clone().with_tier(RowTier::Everyday),
            ..g.clone()
        };
        let mut blocks: Vec<PageBlock> = sections(self, &everyday, window, cx);
        let advanced = GeneralCx {
            search: g.search.clone().with_tier(RowTier::Advanced),
            shown_above_fold: blocks
                .iter()
                .filter_map(|block| block.anchor.clone())
                .collect(),
            ..g.clone()
        };
        let advanced_blocks = sections(self, &advanced, window, cx);
        if !advanced_blocks.is_empty() {
            // A deep link to a section that only has advanced rows opens the fold.
            let pending = self.store.update(cx, |store, _| {
                store.tracker(tab).borrow().pending_scroll.clone()
            });
            if let Some(anchor) = pending
                && !blocks
                    .iter()
                    .any(|block| block.anchor.as_deref() == Some(&anchor))
                && advanced_blocks
                    .iter()
                    .any(|block| block.anchor.as_deref() == Some(&anchor))
            {
                self.store
                    .update(cx, |store, _| store.open_advanced_fold(tab));
            }
            let open = self.store.read(cx).advanced_fold_open(tab);
            blocks.push(PageBlock::section(
                "advancedFold",
                advanced_fold_header(
                    &g.p,
                    open,
                    move |page: &mut Self, _window, cx| {
                        let store = page.store.clone();
                        store.update(cx, |store, cx| store.toggle_advanced_fold(tab, cx));
                    },
                    cx,
                ),
            ));
            if open {
                blocks.extend(advanced_blocks);
            }
        }
        if !has_visible {
            let store = self.store.clone();
            // `SettingsSearchNoMatches` is not a section: no 12px section margin above it.
            blocks.push(PageBlock::plain(render_no_matches(
                &g.p,
                tab,
                &matching_pages,
                move |tab, _window, cx| {
                    store.update(cx, |store, cx| store.set_active_tab(tab, cx));
                },
            )));
        }
        // The preview binary's `select` state opens a dropdown; its `pick-color` state (and a host
        // without a system colour panel) the terminal background's Pick Color dialog.
        if let Some(id) = self
            .store
            .update(cx, |store, _| store.request_mut().open_select.take())
        {
            self.fields.pending_open_select = Some(id.into());
            cx.notify();
        }
        let open_picker = self
            .store
            .update(cx, |store, _| store.request_mut().open_color_picker.take());
        if open_picker.as_deref() == Some("workspaceBackgroundColor") {
            super::super::fields::open_picker(
                self,
                "workspaceBackgroundColor",
                "#121212",
                window,
                cx,
            );
        }
        let preview = self.marked_sections_preview(&g, window, cx);
        let instructions =
            super::orchestrator_playbook::render_playbook_sheet(self, &g.p, window, cx);
        let playbook_view =
            super::orchestrator_playbook::render_playbook_view_sheet(self, &g.p, window, cx);
        div()
            .size_full()
            .child(settings_page(&self.store, tab, &g.p, blocks, cx))
            .children(preview)
            .children(instructions)
            .children(playbook_view)
    }
}
