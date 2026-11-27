//! The Debugging page (packages/core-ui/settings-modal/tabs/debugging.tsx (deleted 2026-10-01)), listed in the rail
//! under its built-in extension: Show debug UI controls, then the diagnostic log areas. Use agent
//! hooks moved to Settings > Agents (CDXC:AgentHooks 2026-11-19 in tabs/agents/config.rs). Both
//! rows are Power User Flags (CDXC:Settings 2026-11-27 in packages/settings-catalog
//! power_user_flags.rs): they sit in the page's Power User Flags fold, which shows on Experience >
//! Advanced or while one of them is changed; otherwise the page says how to see them.
use super::super::fields::{
    FieldStates, RowSpec, SettingsPage, diagnostic_logging_field, push_power_user_flags, reset_key,
    settings_note_row, settings_section, toggle_field,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::SettingsStore;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, Render, Window,
};

/// `.settings-page-width grid gap-6 px-5 py-5`: 20px above the first section on top of its own 12px.
const PAGE_TOP_PADDING: f32 = 20.0;

pub(crate) fn debugging_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| DebuggingTab::new(store.clone(), cx)).into()
}

/// CDXC:Diagnostics 2026-09-26 DECISION:
/// User: debugging lives on its own Settings page above About, with Show debug UI controls first. User: hide and disable the Storage section (storage usage and folder statistics); this supersedes the 2026-09-16 decision that showed it behind Show debug UI controls.
pub(crate) struct DebuggingTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
}

impl DebuggingTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            fields: FieldStates::default(),
        }
    }
}

impl SettingsPage for DebuggingTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl Render for DebuggingTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, values, search, matching, flags_shown, searching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.values(),
                store.tab_search(SettingsTabId::Debugging),
                matching,
                store.power_user_flags_shown(SettingsTabId::Debugging),
                store.is_searching(),
            )
        };
        let debugging_mode = values.bool("debuggingMode");
        let mut rows: Vec<AnyElement> = vec![toggle_field(
            self,
            &p,
            "debuggingMode",
            RowSpec::new("Show debug UI controls")
                .description(
                    "Show diagnostic logs. Warnings, errors, and crashes are always captured.",
                )
                .keyed(&values, "debuggingMode"),
            debugging_mode,
            cx,
        )];
        if debugging_mode && search.row_visible("controls", "diagnosticLogging") {
            let value = values.value("diagnosticLogging");
            rows.push(diagnostic_logging_field(
                self,
                &p,
                RowSpec::new("Diagnostic logs")
                    .description(
                        "Pick the areas to log while you reproduce an issue. Warnings, errors, and crashes are always captured.",
                    )
                    .dependent()
                    .modified(values.is_modified("diagnosticLogging")),
                Some(reset_key::<Self>("diagnosticLogging")),
                &value,
                window,
                cx,
            ));
        }
        let mut blocks = Vec::new();
        if !flags_shown {
            if !searching {
                let note = settings_note_row(
                    &p,
                    "Show debug UI controls and Diagnostic logs are Power User Flags. Pick Advanced in Settings > Experience to see them.",
                );
                if let Some(section) = settings_section(&p, "", None, None, vec![note]) {
                    let mut block = PageBlock::section("controls", section);
                    block.margin_top += PAGE_TOP_PADDING;
                    blocks.push(block);
                }
            }
        } else if let Some(section) = settings_section(&p, "", None, None, rows) {
            let mut block = PageBlock::section("controls", section);
            if searching {
                block.margin_top += PAGE_TOP_PADDING;
                blocks.push(block);
            } else {
                push_power_user_flags(
                    &self.store,
                    &p,
                    SettingsTabId::Debugging,
                    &mut blocks,
                    vec![block],
                    cx,
                );
                if let Some(first) = blocks.first_mut() {
                    first.margin_top += PAGE_TOP_PADDING;
                }
            }
        }
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Debugging,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        settings_page(&self.store, SettingsTabId::Debugging, &p, blocks, cx)
    }
}
