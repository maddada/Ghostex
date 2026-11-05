//! The Updates page: what this computer uses that Ghostex can update, as a grid of cards (problems
//! first, then updates), one Update all, an "Update automatically" switch per card, and one Up to
//! date card for the rest. It draws the app's `updatesState` (app/updates/) and posts `updates`
//! messages back; the app runs every update through the path its own Settings row uses.
//!
//! CDXC:Updates 2026-11-05 DECISION:
//! User (Q1360): the Updates page is a Settings tab, first in the rail with a count. The cards follow
//! the look the user approved (updates-page-v2): Settings' own palette, raised cards with a hairline
//! and no coloured borders, the real agent logos, and status as one word beside the name (only
//! Failed gets a red dot). Q1365: every card has an "Update automatically" switch, off by default.
//! SEE-ALSO: server/src/updates/mod.rs (all of the user's picks), app/updates/mod.rs.
mod card;

use super::super::super::native_modal_kit::*;
use super::super::catalog::SettingOption;
use super::super::fields::FieldStates;
use super::super::fields::settings_segmented;
use super::super::fields::{
    ButtonSize, ButtonVariant, SettingsPage, settings_button_sized, settings_list_item,
    settings_section,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, SettingsStoreEvent, post_store_message};
use super::extensions::cards::{GridCell, card_grid, grid_columns, page_column_width};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::collections::HashMap;

pub(crate) fn updates_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| UpdatesTab::new(store.clone(), cx)).into()
}

pub(crate) struct UpdatesTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The group the cards are filtered to: `all`, `ghostex`, `agents`, `tools` or `extensions`.
    filter: String,
    /// Switches flipped here, shown until the app's next list confirms them.
    pending_auto: HashMap<String, bool>,
}

impl SettingsPage for UpdatesTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// The groups the filter offers, in order, with their labels.
const GROUPS: [(&str, &str); 4] = [
    ("ghostex", "Ghostex"),
    ("agents", "Agents"),
    ("tools", "Tools"),
    ("extensions", "Extensions"),
];

/// A card shows in the grid while there is something to see or do; the rest go in Up to date.
fn on_card(item: &Value) -> bool {
    matches!(
        item["state"].as_str(),
        Some("available" | "updating" | "failed" | "needsYou")
    )
}

/// What Update all runs.
fn updatable(item: &Value) -> bool {
    matches!(item["state"].as_str(), Some("available" | "failed")) && item["canUpdate"] == true
}

impl UpdatesTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        cx.subscribe(
            &store,
            |tab: &mut Self, _store, event: &SettingsStoreEvent, cx| {
                let SettingsStoreEvent::HostPayload(kind) = event;
                if kind == "updatesState" {
                    tab.pending_auto.clear();
                    cx.notify();
                }
            },
        )
        .detach();
        post_store_message(&store, json!({ "type": "updates", "op": "refresh" }), cx);
        Self {
            store,
            fields: FieldStates::default(),
            filter: "all".to_string(),
            pending_auto: HashMap::new(),
        }
    }

    pub(super) fn post(&self, message: Value, cx: &mut App) {
        post_store_message(&self.store, message, cx);
    }

    pub(super) fn install(&self, id: &str, cx: &mut App) {
        self.post(
            json!({ "type": "updates", "op": "install", "ids": [id] }),
            cx,
        );
    }

    pub(super) fn set_auto(&mut self, id: &str, enabled: bool, cx: &mut Context<Self>) {
        self.pending_auto.insert(id.to_string(), enabled);
        self.post(
            json!({ "type": "updates", "op": "setAuto", "id": id, "enabled": enabled }),
            cx,
        );
        cx.notify();
    }

    pub(super) fn auto(&self, item: &Value) -> bool {
        let id = item["id"].as_str().unwrap_or_default();
        self.pending_auto
            .get(id)
            .copied()
            .unwrap_or(item["auto"] == true)
    }

    /// An item's Review…: the page where the user finishes it (an extension's new permissions).
    pub(super) fn review(&self, item: &Value, cx: &mut App) {
        let tab = match item["kind"].as_str() {
            Some("extension") => SettingsTabId::Extensions,
            Some("agent") => SettingsTabId::Agents,
            Some("helper") => SettingsTabId::Accounts,
            Some("machine") => SettingsTabId::Remote,
            _ => SettingsTabId::Integrations,
        };
        self.store
            .update(cx, |store, cx| store.set_active_tab(tab, cx));
    }

    fn header(
        &self,
        p: &SettingsPalette,
        payload: Option<&Value>,
        items: &[Value],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let running = payload.is_some_and(|payload| payload["running"] == true)
            || items.iter().any(|item| item["state"] == "updating");
        let checked = match payload {
            None => "Checking for updates…".to_string(),
            Some(payload) if payload["error"].is_string() => format!(
                "Couldn't check: {}",
                payload["error"].as_str().unwrap_or_default()
            ),
            Some(_) if running => "Updating…".to_string(),
            Some(payload) => checked_ago(payload["checkedAt"].as_i64()),
        };
        let update_all = items.iter().filter(|item| updatable(item)).count();
        let mut options = vec![SettingOption {
            label: format!("All {}", items.iter().filter(|item| on_card(item)).count()),
            value: "all".to_string(),
        }];
        for (group, label) in GROUPS {
            let count = items
                .iter()
                .filter(|item| on_card(item) && item["group"] == group)
                .count();
            if count > 0 {
                options.push(SettingOption {
                    label: format!("{label} {count}"),
                    value: group.to_string(),
                });
            }
        }
        let filter = if options.iter().any(|option| option.value == self.filter) {
            self.filter.clone()
        } else {
            "all".to_string()
        };
        h_flex()
            .w_full()
            .items_end()
            .justify_between()
            .gap(px(16.0))
            .px(px(2.0))
            .child(
                v_flex()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .line_height(px(20.8))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("Updates"),
                    )
                    .child(
                        h_flex()
                            .gap(px(4.0))
                            .text_size(px(13.0))
                            .line_height(px(18.85))
                            .text_color(hsla(p.muted))
                            .child(checked)
                            .when(!running, |this| {
                                let hover = p.foreground;
                                this.child("·").child(
                                    div()
                                        .id("updates-check-now")
                                        .cursor_pointer()
                                        .underline()
                                        .hover(move |this| this.text_color(hsla(hover)))
                                        .on_click(cx.listener(|tab, _, _, cx| {
                                            tab.post(
                                                json!({ "type": "updates", "op": "check" }),
                                                cx,
                                            );
                                        }))
                                        .child("Check now"),
                                )
                            }),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .when(options.len() > 2, |this| {
                        this.child(settings_segmented(
                            p,
                            "updates-filter",
                            &options,
                            Some(filter.as_str()),
                            |tab: &mut Self, value, _window, cx| {
                                tab.filter = value;
                                cx.notify();
                            },
                            cx,
                        ))
                    })
                    .child(settings_button_sized(
                        p,
                        "updates-update-all",
                        if update_all > 0 {
                            format!("Update all · {update_all}")
                        } else {
                            "Update all".to_string()
                        },
                        Some("modals/settings/download.svg"),
                        ButtonVariant::Default,
                        ButtonSize::Sm,
                        update_all == 0,
                        Some("Everything here is up to date.".into()),
                        |tab: &mut Self, _window, cx| {
                            tab.post(
                                json!({ "type": "updates", "op": "install", "all": true }),
                                cx,
                            );
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    /// The Up to date card: one line of names and versions, then a row for each item Ghostex
    /// can't read the version of.
    fn up_to_date(
        &self,
        p: &SettingsPalette,
        items: &[&Value],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let current: Vec<String> = items
            .iter()
            .filter(|item| item["state"] == "upToDate")
            .map(|item| match item["installed"].as_str() {
                Some(version) => format!("{} {version}", item["name"].as_str().unwrap_or_default()),
                None => item["name"].as_str().unwrap_or_default().to_string(),
            })
            .collect();
        let mut rows: Vec<AnyElement> = Vec::new();
        if !current.is_empty() {
            rows.push(
                div()
                    .w_full()
                    .px(px(16.0))
                    .py(px(14.0))
                    .text_size(px(13.0))
                    .line_height(px(18.85))
                    .text_color(hsla(p.foreground_alpha(0.75)))
                    .child(current.join(", "))
                    .into_any_element(),
            );
        }
        for item in items
            .iter()
            .filter(|item| matches!(item["state"].as_str(), Some("unknown" | "checking")))
        {
            let id = item["id"].as_str().unwrap_or_default().to_string();
            let detail = if item["state"] == "checking" {
                item["note"]
                    .as_str()
                    .unwrap_or("Checking for an update…")
                    .to_string()
            } else {
                item["error"]
                    .as_str()
                    .or(item["note"].as_str())
                    .unwrap_or("Ghostex can't tell its version from outside.")
                    .to_string()
            };
            let control = (item["state"] == "unknown"
                && item["kind"] == "agent"
                && item["canUpdate"] == true)
                .then(|| {
                    let id = id.clone();
                    settings_button_sized(
                        p,
                        SharedString::from(format!("updates-run-{id}")),
                        "Run its updater",
                        None,
                        ButtonVariant::Default,
                        ButtonSize::Xs,
                        false,
                        None,
                        move |tab: &mut Self, _window, cx| tab.install(&id, cx),
                        cx,
                    )
                });
            rows.push(settings_list_item(
                p,
                None,
                Some(card::item_icon(p, item, 16.0)),
                SharedString::from(item["name"].as_str().unwrap_or_default().to_string()),
                Some(div().child(detail).into_any_element()),
                control,
            ));
        }
        settings_section(p, "Up to date", None, None, rows).map(IntoElement::into_any_element)
    }
}

/// "Checked 4 min ago" from milliseconds since 1970.
fn checked_ago(at_ms: Option<i64>) -> String {
    let Some(at_ms) = at_ms else {
        return "Checking for updates…".to_string();
    };
    let minutes = (chrono::Utc::now().timestamp_millis() - at_ms).max(0) / 60_000;
    match minutes {
        0 => "Checked just now".to_string(),
        1..=59 => format!("Checked {minutes} min ago"),
        _ => format!("Checked {} h ago", minutes / 60),
    }
}

impl Render for UpdatesTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching, payload) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::Updates),
                matching,
                store.host_payload("updatesState").cloned(),
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Updates,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
            return settings_page(&self.store, SettingsTabId::Updates, &p, blocks, cx);
        }
        let items: Vec<Value> = payload
            .as_ref()
            .and_then(|payload| payload["items"].as_array().cloned())
            .unwrap_or_default();
        let shown: Vec<&Value> = items
            .iter()
            .filter(|item| self.filter == "all" || item["group"] == self.filter.as_str())
            .collect();
        let cards: Vec<GridCell> = shown
            .iter()
            .filter(|item| on_card(item))
            .map(|item| GridCell::card(self.card(&p, item, cx)))
            .collect();
        let mut top =
            v_flex()
                .w_full()
                .gap(px(12.0))
                .child(self.header(&p, payload.as_ref(), &items, cx));
        if cards.is_empty() {
            let message = if payload.is_none() {
                "Checking what can be updated…"
            } else {
                "Everything Ghostex can update is up to date."
            };
            top = top.child(
                div()
                    .w_full()
                    .rounded(px(MODAL_RADIUS_SECTION))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.raised))
                    .px(px(16.0))
                    .py(px(18.0))
                    .text_size(px(13.0))
                    .line_height(px(18.85))
                    .text_color(hsla(p.muted))
                    .child(message),
            );
        } else {
            top = top.child(card_grid(
                cards,
                None,
                grid_columns(page_column_width(window)),
            ));
        }
        blocks.push(PageBlock::section("updates", top));
        if let Some(section) = self.up_to_date(&p, &shown, cx) {
            blocks.push(PageBlock::section("upToDate", section));
        }
        blocks.push(PageBlock::plain(
            div()
                .w_full()
                .px(px(2.0))
                .text_size(px(12.0))
                .line_height(px(17.14))
                .text_color(hsla(p.muted))
                .child(
                    "Kept current with Ghostex, not listed here: agent hooks and plugins, bundled skills and connectors.",
                ),
        ));
        settings_page(&self.store, SettingsTabId::Updates, &p, blocks, cx)
    }
}
