//! Settings > Updates > Checking for updates: a fold, closed by default, with how often Ghostex
//! checks each kind on its own (Ghostex, Agents, Tools, Extensions) and one quiet line saying
//! agents turned off are never checked. The settings and the user's decision are on
//! packages/settings-catalog/src/update_checks.rs; gxserver applies the last three
//! (server/src/updates/items.rs), the app the first (app/os_integration/updater.rs).
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    FoldFrame, FoldRow, ROW_PADDING_X, RowSpec, reset_key, select_field, settings_fold_row,
};
use super::super::super::model::SettingsTabId;
use super::super::super::palette::SettingsPalette;
use super::UpdatesTab;
use ghostex_settings_catalog::update_checks::{
    UPDATE_CHECK_DEFAULT, UPDATE_CHECK_OPTIONS, UPDATES_CHECK_AGENTS, UPDATES_CHECK_EXTENSIONS,
    UPDATES_CHECK_GHOSTEX, UPDATES_CHECK_TOOLS,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use gpui_component::v_flex;
use serde_json::json;

/// The fold's search section (packages/settings-catalog/src/pages/updates.rs).
pub(super) const CHECKS_SECTION: &str = "checking";

/// Each row: its setting, title and the line under it.
const ROWS: [(&str, &str, &str); 4] = [
    (
        UPDATES_CHECK_GHOSTEX,
        "Ghostex",
        "New versions of Ghostex on this computer.",
    ),
    (
        UPDATES_CHECK_AGENTS,
        "Agents",
        "Claude Code, Codex and the other agents you turned on.",
    ),
    (
        UPDATES_CHECK_TOOLS,
        "Tools",
        "What Ghostex installs for you, such as uv and Claude Swap.",
    ),
    (
        UPDATES_CHECK_EXTENSIONS,
        "Extensions",
        "Extensions you added from the catalog.",
    ),
];

fn options() -> Vec<SettingOption> {
    UPDATE_CHECK_OPTIONS
        .iter()
        .map(|option| SettingOption {
            label: option.label.to_string(),
            value: option.value.to_string(),
        })
        .collect()
}

impl UpdatesTab {
    /// The fold. While a search matches its rows it stays open, showing only those.
    pub(super) fn checks_fold(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (values, search) = {
            let store = self.store.read(cx);
            (store.values(), store.tab_search(SettingsTabId::Updates))
        };
        let pending = self.store.update(cx, |store, _| {
            store
                .tracker(SettingsTabId::Updates)
                .borrow()
                .pending_scroll
                .clone()
        });
        let searching = search.tab.is_searching;
        let shown: Vec<_> = ROWS
            .into_iter()
            .filter(|(key, _, _)| search.row_visible(CHECKS_SECTION, key))
            .collect();
        if searching && (shown.is_empty() || !search.section(CHECKS_SECTION).has_visible()) {
            return None;
        }
        // A deep link to the section opens the fold.
        if pending.as_deref() == Some(CHECKS_SECTION) {
            self.checks_open = true;
        }
        let open = searching || self.checks_open;
        let options = options();
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let label_of = |value: &str| {
            options
                .iter()
                .find(|option| option.value == value)
                .map(|option| option.label.clone())
                .unwrap_or_default()
        };
        let chosen: Vec<String> = ROWS
            .iter()
            .map(|(key, _, _)| values.choice(key, &allowed))
            .collect();
        // Closed, the summary: the one time all four share, else that they differ.
        let summary = if chosen.iter().all(|value| *value == chosen[0]) {
            label_of(&chosen[0])
        } else {
            "Set per kind".to_string()
        };
        let header = settings_fold_row(
            p,
            FoldRow {
                id: "settings-updates-checks-fold",
                label: "Checking for updates",
                open,
                frame: FoldFrame::PanelHeader,
                trailing: (!open).then(|| {
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(hsla(p.muted))
                        .child(summary)
                        .into_any_element()
                }),
                held_open: searching.then_some("Open while the search matches settings in it."),
            },
            |tab: &mut Self, _window, cx| {
                tab.checks_open = !tab.checks_open;
                cx.notify();
            },
            cx,
        );
        let mut panel = v_flex()
            .w_full()
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .overflow_hidden()
            .child(header);
        if open {
            let hairline = hsla(p.hairline);
            for (index, (key, title, line)) in shown.into_iter().enumerate() {
                let value = values.choice(key, &allowed);
                let value = if value.is_empty() {
                    UPDATE_CHECK_DEFAULT.to_string()
                } else {
                    value
                };
                let row = select_field(
                    self,
                    p,
                    key,
                    RowSpec::new(title).description(line).keyed(&values, key),
                    Some(reset_key::<Self>(key)),
                    &options,
                    &value,
                    None,
                    move |tab: &mut Self, next, _window, cx| {
                        tab.store
                            .update(cx, |store, cx| store.update_setting(key, json!(next), cx));
                    },
                    window,
                    cx,
                );
                // The open header draws the line under itself.
                panel = panel.child(
                    div()
                        .w_full()
                        .when(index > 0, |row| row.border_t_1().border_color(hairline))
                        .child(row),
                );
            }
            panel = panel.child(
                div()
                    .w_full()
                    .border_t_1()
                    .border_color(hairline)
                    .px(px(ROW_PADDING_X))
                    .py(px(12.0))
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(hsla(p.muted))
                    .child(
                        "Agents turned off in Settings > Agents are never checked. Check now always checks everything.",
                    ),
            );
        }
        Some(panel.into_any_element())
    }
}
