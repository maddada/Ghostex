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
//!
//! CDXC:Updates 2026-11-07 DECISION:
//! User: "I need to make it show all connected machines" and "We also need the ability to install
//! agents/etc that aren't installed already on each machine through update page". With remote
//! computers connected, the page is grouped by computer (this one first; the `computers` the app
//! sends), each with its own cards, Up to date pills, and a quieter Not installed part whose pills
//! install on a click on that computer. An offline computer, or one whose Ghostex is too old, shows
//! its one plain line. Alone, the page looks as it did, plus Not installed.
//! SEE-ALSO: server/src/updates/mod.rs (all of the user's picks), app/updates/mod.rs.
mod card;

use super::super::super::native_modal_kit::*;
use super::super::catalog::SettingOption;
use super::super::fields::FieldStates;
use super::super::fields::settings_segmented;
use super::super::fields::{
    ButtonSize, ButtonVariant, SettingsPage, settings_button_sized, settings_section,
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

    pub(super) fn dismiss(&self, id: &str, error: &str, cx: &mut App) {
        self.post(
            json!({ "type": "updates", "op": "dismiss", "id": id, "error": error }),
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
        let other_computer = item["computer"]
            .as_str()
            .is_some_and(|computer| computer != "local");
        let tab = match item["kind"].as_str() {
            // Another computer's item is finished on that computer, reached from Settings > Remote.
            _ if other_computer => SettingsTabId::Remote,
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

    /// The Up to date card: a wrap of pills (logo, name, version), then a row for each item
    /// Ghostex can't read the version of.
    ///
    /// CDXC:Updates 2026-11-06 DECISION:
    /// User: "are u fking serious with the list of up to date ones there at the bottom???? why not
    /// pills or something nicer". Each current item is a soft pill in Settings' muted fill (no
    /// border, no dark fill) with the same logo its card would use, its name, and its version in the
    /// muted tone; the rows under them use the cards' logo tile and type sizes.
    fn up_to_date(
        &self,
        p: &SettingsPalette,
        items: &[&Value],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pills: Vec<AnyElement> = items
            .iter()
            .filter(|item| item["state"] == "upToDate")
            .map(|item| card::up_to_date_pill(p, item))
            .collect();
        let mut rows: Vec<AnyElement> = Vec::new();
        if !pills.is_empty() {
            rows.push(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap(px(6.0))
                    .px(px(14.0))
                    .py(px(14.0))
                    .children(pills)
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
            rows.push(card::up_to_date_row(p, item, detail, control));
        }
        settings_section(p, "Up to date", None, None, rows).map(IntoElement::into_any_element)
    }
}

impl UpdatesTab {
    /// Not installed: what this computer could have that Ghostex installs there, as quiet pills
    /// that install on a click (never through Update all), and a row for each install that failed,
    /// saying why, with Retry.
    fn not_installed(
        &self,
        p: &SettingsPalette,
        items: &[&Value],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let missing: Vec<&&Value> = items
            .iter()
            .filter(|item| item["state"] == "notInstalled")
            .collect();
        let pills: Vec<AnyElement> = missing
            .iter()
            .filter(|item| !item["error"].is_string())
            .map(|item| card::install_pill(p, item, cx))
            .collect();
        let mut rows: Vec<AnyElement> = Vec::new();
        if !pills.is_empty() {
            rows.push(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap(px(6.0))
                    .px(px(14.0))
                    .py(px(14.0))
                    .children(pills)
                    .into_any_element(),
            );
        }
        for item in missing.iter().filter(|item| item["error"].is_string()) {
            let id = item["id"].as_str().unwrap_or_default().to_string();
            let detail = format!(
                "Install failed: {}",
                item["error"].as_str().unwrap_or_default()
            );
            let retry = settings_button_sized(
                p,
                SharedString::from(format!("updates-install-{id}")),
                "Retry",
                None,
                ButtonVariant::Default,
                ButtonSize::Xs,
                false,
                None,
                move |tab: &mut Self, _window, cx| tab.install(&id, cx),
                cx,
            );
            rows.push(card::up_to_date_row(p, item, detail, Some(retry)));
        }
        settings_section(
            p,
            "Not installed",
            Some("Ghostex can install these here. Nothing installs until you click it.".into()),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    /// One computer's part of the page: its cards (or `line` when it can't be read, or a quiet
    /// "up to date"), its Up to date pills and its Not installed part.
    fn computer_body(
        &self,
        p: &SettingsPalette,
        items: &[&Value],
        line: Option<&str>,
        empty_message: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut parts = Vec::new();
        let cards: Vec<GridCell> = items
            .iter()
            .filter(|item| on_card(item))
            .map(|item| GridCell::card(self.card(p, item, cx)))
            .collect();
        if !cards.is_empty() {
            parts.push(
                card_grid(cards, None, grid_columns(page_column_width(window))).into_any_element(),
            );
        }
        let message = line.or(cards_empty_message(items, empty_message));
        if let Some(message) = message {
            parts.push(plain_line(p, message));
        }
        parts.extend(self.up_to_date(p, items, cx));
        parts.extend(self.not_installed(p, items, cx));
        parts
    }
}

/// The quiet line a group shows when it has no card.
fn cards_empty_message<'a>(items: &[&Value], message: &'a str) -> Option<&'a str> {
    (!items.iter().any(|item| on_card(item))).then_some(message)
}

/// One plain line in a raised panel (no cards, an offline computer, a too-old Ghostex).
fn plain_line(p: &SettingsPalette, message: &str) -> AnyElement {
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
        .child(message.to_string())
        .into_any_element()
}

/// A computer's heading: its name, and "This computer" beside this one's.
fn computer_heading(p: &SettingsPalette, computer: &Value) -> AnyElement {
    h_flex()
        .w_full()
        .items_baseline()
        .gap(px(8.0))
        .px(px(2.0))
        .pt(px(6.0))
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(19.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(p.foreground))
                .child(computer["name"].as_str().unwrap_or("Computer").to_string()),
        )
        .when(computer["thisComputer"] == true, |this| {
            this.child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(hsla(p.muted))
                    .child("This computer"),
            )
        })
        .into_any_element()
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
        let computers: Vec<Value> = payload
            .as_ref()
            .and_then(|payload| payload["computers"].as_array().cloned())
            .unwrap_or_default();
        let header = self.header(&p, payload.as_ref(), &items, cx);
        if computers.len() <= 1 {
            // Only this computer: the page as it always was, plus Not installed.
            let message = if payload.is_none() {
                "Checking what can be updated…"
            } else {
                "Everything Ghostex can update is up to date."
            };
            let mut parts = self.computer_body(&p, &shown, None, message, window, cx);
            let mut top = v_flex().w_full().gap(px(12.0)).child(header);
            if !parts.is_empty() {
                top = top.child(parts.remove(0));
            }
            blocks.push(PageBlock::section("updates", top));
            for (index, part) in parts.into_iter().enumerate() {
                blocks.push(PageBlock::section(format!("updates-{index}"), part));
            }
        } else {
            blocks.push(PageBlock::section("updates", header));
            for computer in &computers {
                let id = computer["id"].as_str().unwrap_or_default();
                let group: Vec<&Value> = shown
                    .iter()
                    .copied()
                    .filter(|item| item["computer"].as_str().unwrap_or("local") == id)
                    .collect();
                let line = computer["line"].as_str();
                // A computer that can't be read shows its line alone, never items from before.
                let group = if line.is_some() && computer["state"] != "checking" {
                    Vec::new()
                } else {
                    group
                };
                let message = if computer["thisComputer"] == true && payload.is_none() {
                    "Checking what can be updated…"
                } else {
                    "Everything here is up to date."
                };
                let column = v_flex()
                    .w_full()
                    .gap(px(12.0))
                    .child(computer_heading(&p, computer))
                    .children(self.computer_body(&p, &group, line, message, window, cx));
                blocks.push(PageBlock::section(format!("computer-{id}"), column));
            }
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
