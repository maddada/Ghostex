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
//!
//! CDXC:Updates 2026-11-09 DECISION:
//! User: "on the update page in settings we should show a dropdown at the top that lets me select
//! which machine we're configuring updates for", "also we should have a button to update all
//! machines". The page has the machine picker Agents, Integrations and Voice agent use (one pick
//! serves them all; it shows once a remote machine is saved) and shows only the picked machine's
//! cards, Up to date and Not installed, replacing the heading per computer above. Update all in the
//! page's header updates the shown machine; "Update all machines" beside the picker updates every
//! connected machine, and a line per machine under the picker says how that went (a failure names
//! its machine and why). Alone, the page has no picker and its Update all is this computer's.
//! SEE-ALSO: server/src/updates/mod.rs (all of the user's picks), app/updates/mod.rs.
mod card;

use super::super::super::native_modal_kit::*;
use super::super::catalog::SettingOption;
use super::super::fields::FieldStates;
use super::super::fields::ROW_PADDING_X;
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
use super::machine_picker;
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
    /// The last "Update all machines": each computer's id and name with the ids it was sent, so
    /// the lines under the picker can say how each went. Kept until the page closes.
    all_run: Vec<(String, String, Vec<String>)>,
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

/// A card shows in the grid while there is something to see or do; the rest go in Up to date
/// (an install stays a pill in Not installed while it runs).
fn on_card(item: &Value) -> bool {
    matches!(
        item["state"].as_str(),
        Some("available" | "updating" | "failed" | "needsYou")
    ) && !installing(item)
}

/// A Not installed item whose install runs (gxserver lists it `updating` until it lands).
pub(super) fn installing(item: &Value) -> bool {
    item["state"] == "updating" && item["canInstall"] == true
}

/// The computer an item is on (`local` for this one).
fn computer_of(item: &Value) -> &str {
    item["computer"].as_str().unwrap_or(THIS_COMPUTER)
}

const THIS_COMPUTER: &str = "local";

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
        machine_picker::request_machine_states(&store, cx);
        // The preview binary's `updates-computers-run`: an Update all machines that is running.
        let all_run = match store.read(cx).request().preview_state.as_deref() {
            Some("updates-computers-run") => vec![
                (
                    THIS_COMPUTER.to_string(),
                    "Mac Studio".to_string(),
                    vec!["app".to_string(), "agent:claude".to_string()],
                ),
                (
                    "remote-linux-box".to_string(),
                    "Linux box".to_string(),
                    vec!["remote:remote-linux-box:agent:claude".to_string()],
                ),
                (
                    "remote-air".to_string(),
                    "MacBook Air".to_string(),
                    Vec::new(),
                ),
                (
                    "remote-mini".to_string(),
                    "Old Mini".to_string(),
                    Vec::new(),
                ),
            ],
            _ => Vec::new(),
        };
        Self {
            store,
            fields: FieldStates::default(),
            filter: "all".to_string(),
            pending_auto: HashMap::new(),
            all_run,
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
        items: &[&Value],
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
        // The shown machine's updates (Update all machines, by the picker, runs every machine's).
        let update_ids: Vec<String> = items
            .iter()
            .filter(|item| updatable(item))
            .filter_map(|item| item["id"].as_str().map(str::to_string))
            .collect();
        let update_all = update_ids.len();
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
                        move |tab: &mut Self, _window, cx| {
                            tab.post(
                                json!({ "type": "updates", "op": "install", "ids": update_ids }),
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
            rows.push(pill_row(pills));
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
    /// Not installed: what the machine could have that Ghostex installs there, as quiet pills that
    /// install on a click (never through Update all), and a row for each install that failed,
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
        // An install that runs stays here as a pill with a spinner until it lands.
        let pills: Vec<AnyElement> = items
            .iter()
            .filter(|item| {
                installing(item) || (item["state"] == "notInstalled" && !item["error"].is_string())
            })
            .map(|item| card::install_pill(p, item, cx))
            .collect();
        let mut rows: Vec<AnyElement> = Vec::new();
        if !pills.is_empty() {
            rows.push(pill_row(pills));
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
        // CDXC:Updates 2026-11-09 DECISION: User: "please don't say 'Nothing installs until you
        // click on it' that's clear man". The heading says what the part is.
        settings_section(p, "Not installed", None, None, rows).map(IntoElement::into_any_element)
    }

    /// The machine picker, with "Update all machines" beside its heading and, after a run, a line
    /// per machine saying how it went. `None` while no remote machine is saved.
    fn picker(
        &mut self,
        p: &SettingsPalette,
        items: &[Value],
        computers: &[Value],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Every connected machine's updates, by machine (an offline one has no items listed).
        let plan: Vec<(String, String, Vec<String>)> = computers
            .iter()
            .map(|computer| {
                let id = computer["id"].as_str().unwrap_or(THIS_COMPUTER).to_string();
                let name = computer["name"].as_str().unwrap_or("Computer").to_string();
                let ids = items
                    .iter()
                    .filter(|item| computer_of(item) == id && updatable(item))
                    .filter_map(|item| item["id"].as_str().map(str::to_string))
                    .collect();
                (id, name, ids)
            })
            .collect();
        let total: usize = plan.iter().map(|(_, _, ids)| ids.len()).sum();
        let button = settings_button_sized(
            p,
            "updates-update-all-machines",
            if total > 0 {
                format!("Update all machines · {total}")
            } else {
                "Update all machines".to_string()
            },
            Some("modals/settings/download.svg"),
            ButtonVariant::Default,
            ButtonSize::Sm,
            total == 0,
            Some("Every connected machine is up to date.".into()),
            move |tab: &mut Self, _window, cx| {
                let ids: Vec<String> = plan
                    .iter()
                    .flat_map(|(_, _, ids)| ids.iter().cloned())
                    .collect();
                tab.all_run = plan.clone();
                tab.post(
                    json!({ "type": "updates", "op": "install", "ids": ids }),
                    cx,
                );
                cx.notify();
            },
            cx,
        );
        let rows: Vec<AnyElement> = self
            .all_run
            .iter()
            .map(|(id, name, sent)| {
                let computer = computers
                    .iter()
                    .find(|computer| computer["id"] == id.as_str());
                let (line, outcome) = run_line(computer, sent, items);
                run_row(p, id, name, line, outcome)
            })
            .collect();
        let store = self.store.clone();
        machine_picker::render_machine_picker_with(
            self,
            p,
            &store,
            |shown| {
                match shown {
                Some(name) => format!(
                    "Showing the updates on {name}. Updates and installs run there, through its Ghostex."
                ),
                None => "Showing this computer's updates. Pick a connected machine to see and update its own.".to_string(),
            }
            },
            Some(button),
            rows,
            window,
            cx,
        )
    }
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

/// A wrap of pills with even gaps, inset like Settings' rows.
fn pill_row(pills: Vec<AnyElement>) -> AnyElement {
    h_flex()
        .w_full()
        .flex_wrap()
        .gap(px(8.0))
        .px(px(ROW_PADDING_X))
        .py(px(16.0))
        .children(pills)
        .into_any_element()
}

/// How one machine's part of the last "Update all machines" went, from the list now: `None`
/// while it runs (with the line), `Some(false)` when something failed, `Some(true)` when done.
fn run_line(computer: Option<&Value>, sent: &[String], items: &[Value]) -> (String, Option<bool>) {
    let skipped = match computer.and_then(|computer| computer["state"].as_str()) {
        Some("offline") => Some("Skipped: not connected".to_string()),
        Some("tooOld") => Some("Skipped: its Ghostex is too old to list updates".to_string()),
        Some("failed") => computer
            .and_then(|computer| computer["line"].as_str())
            .map(|line| format!("Skipped. {line}")),
        _ => None,
    };
    if let Some(skipped) = skipped {
        if sent.is_empty() {
            // Nothing was sent there: a quiet line, not a failure.
            return (skipped, Some(true));
        }
        // It dropped while its updates ran: this page can't see how they end.
        return (
            "Lost the connection; its updates may still finish there".to_string(),
            Some(false),
        );
    }
    if sent.is_empty() {
        return ("Nothing to update".to_string(), Some(true));
    }
    let sent_items: Vec<&Value> = items
        .iter()
        .filter(|item| {
            item["id"]
                .as_str()
                .is_some_and(|id| sent.iter().any(|sent| sent == id))
        })
        .collect();
    let running = sent_items
        .iter()
        .filter(|item| matches!(item["state"].as_str(), Some("updating" | "available")))
        .count();
    let failed: Vec<&&Value> = sent_items
        .iter()
        .filter(|item| item["state"] == "failed")
        .collect();
    let done = sent.len() - running - failed.len();
    if running > 0 {
        return (
            format!("Updating {} of {}…", done + failed.len() + 1, sent.len()),
            None,
        );
    }
    match failed.as_slice() {
        [] => (
            if done == 1 {
                "Updated 1".to_string()
            } else {
                format!("Updated {done}")
            },
            Some(true),
        ),
        [only] => (
            format!(
                "{}{} failed: {}",
                if done > 0 {
                    format!("Updated {done}, ")
                } else {
                    String::new()
                },
                only["name"].as_str().unwrap_or("An update"),
                only["error"].as_str().unwrap_or("it stopped")
            ),
            Some(false),
        ),
        many => (
            format!(
                "{}{} failed; their cards say why",
                if done > 0 {
                    format!("Updated {done}, ")
                } else {
                    String::new()
                },
                many.len()
            ),
            Some(false),
        ),
    }
}

/// One machine's line under the picker: its name, then how its updates went.
fn run_row(
    p: &SettingsPalette,
    id: &str,
    name: &str,
    line: String,
    outcome: Option<bool>,
) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap(px(10.0))
        .px(px(ROW_PADDING_X))
        .py(px(10.0))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .child(match outcome {
            None => card::spinner(p, &format!("updates-run-{id}"), 13.0),
            Some(ok) => div()
                .flex_shrink_0()
                .size(px(7.0))
                .mx(px(3.0))
                .rounded_full()
                .bg(hsla(if ok { p.muted } else { p.destructive }))
                .into_any_element(),
        })
        .child(
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .child(name.to_string()),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(hsla(p.muted))
                .child(line),
        )
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
        let computers: Vec<Value> = payload
            .as_ref()
            .and_then(|payload| payload["computers"].as_array().cloned())
            .unwrap_or_default();

        // The machine the page shows: the picker's (shared with Agents and Integrations), or this
        // computer.
        let picked = machine_picker::picked_machine_choice(&self.store, cx);
        let shown_id = picked
            .as_ref()
            .and_then(|choice| choice.machine_id.clone())
            .unwrap_or_else(|| THIS_COMPUTER.to_string());
        let computer = computers
            .iter()
            .find(|computer| computer["id"] == shown_id.as_str());
        // A machine that can't be read shows its one line, never items from before.
        let line: Option<String> = if shown_id == THIS_COMPUTER {
            None
        } else {
            match computer {
                Some(computer) => computer["line"].as_str().map(str::to_string),
                None => Some(
                    picked
                        .as_ref()
                        .and_then(|choice| choice.unavailable.clone())
                        .unwrap_or_else(|| "Not connected".to_string()),
                ),
            }
        };
        let readable = line.is_none() || computer.is_some_and(|c| c["state"] == "checking");
        let here: Vec<&Value> = items
            .iter()
            .filter(|item| readable && computer_of(item) == shown_id)
            .collect();
        let shown: Vec<&Value> = here
            .iter()
            .copied()
            .filter(|item| self.filter == "all" || item["group"] == self.filter.as_str())
            .collect();

        if let Some(picker) = self.picker(&p, &items, &computers, window, cx) {
            blocks.push(PageBlock::section("machinePicker", picker));
        }

        let header = self.header(&p, payload.as_ref(), &here, cx);
        let cards: Vec<GridCell> = shown
            .iter()
            .filter(|item| on_card(item))
            .map(|item| GridCell::card(self.card(&p, item, cx)))
            .collect();
        let mut top = v_flex().w_full().gap(px(12.0)).child(header);
        if !cards.is_empty() {
            top = top.child(card_grid(
                cards,
                None,
                grid_columns(page_column_width(window)),
            ));
        } else {
            let message = match (&line, payload.is_none()) {
                (Some(line), _) => line.as_str(),
                (None, true) => "Checking what can be updated…",
                (None, false) if shown_id == THIS_COMPUTER => {
                    "Everything Ghostex can update is up to date."
                }
                (None, false) => "Everything here is up to date.",
            };
            top = top.child(plain_line(&p, message));
        }
        blocks.push(PageBlock::section("updates", top));
        if let Some(section) = self.up_to_date(&p, &shown, cx) {
            blocks.push(PageBlock::section("upToDate", section));
        }
        if let Some(section) = self.not_installed(&p, &shown, cx) {
            blocks.push(PageBlock::section("notInstalled", section));
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
