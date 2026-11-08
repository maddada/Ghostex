//! Settings > Security: this computer's security log (`/api/securityLog`, server/src/audit/),
//! read when the page opens and every 10 seconds while it is the open page. The first row says
//! whether the log is intact; a broken chain is drawn as a warning with what broke.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User (Q1724, GitHub #495): an append-only, hash-chained log "in Settings > Security, and a notification for every sensitive action. Who (session, device), what, when; you notice in real time."
//! SEE-ALSO: server/src/audit/ (the log and its check), packages/settings-catalog/src/pages/security.rs (its search rows), apps/desktop/src/app/gx_store/notifications/commands.rs (a security row in the bell opens this page).
use super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, SettingsPage, settings_button, settings_list_item,
    settings_section, static_note,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, store_gxserver_rpc};
use gpui::{
    AnyView, App, AppContext as _, Context, Div, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Task, Window, div, px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::time::Duration;

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const REFRESH_EVERY: Duration = Duration::from_secs(10);
const SHOWN_ENTRIES: u64 = 200;

pub(crate) fn security_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| SecurityTab::new(store.clone(), cx)).into()
}

pub(crate) struct SecurityTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The `/api/securityLog` answer: `entries` newest first and `check`.
    view: Option<Value>,
    error: Option<String>,
    was_active: bool,
    _refresh: Option<Task<()>>,
}

impl SettingsPage for SecurityTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// "12 Nov, 14:05" in local time.
fn when(at_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(at_ms)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%-d %b, %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// What the entry's outcome means, and how its dot is drawn.
pub(crate) fn outcome_words(outcome: &str) -> (&'static str, ListItemStatus) {
    match outcome {
        "done" => ("Done", ListItemStatus::Success),
        "approved" => ("Approved by you", ListItemStatus::Success),
        "pending" => ("Waiting for your approval", ListItemStatus::Warning),
        "denied" => ("You said no", ListItemStatus::Neutral),
        "refused" => ("Refused", ListItemStatus::Warning),
        "failed" => ("Failed", ListItemStatus::Warning),
        _ => ("Recorded", ListItemStatus::Neutral),
    }
}

/// One line under an entry's title: how it ended, who asked, what it was about, when.
pub(crate) fn entry_detail(entry: &Value) -> String {
    let (outcome, _) = outcome_words(entry["outcome"].as_str().unwrap_or_default());
    let mut parts = vec![
        outcome.to_string(),
        format!(
            "asked by {}",
            entry
                .pointer("/actor/label")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
    ];
    if let Some(target) = entry["target"].as_str() {
        parts.push(target.to_string());
    }
    if let Some(reason) = entry["reason"].as_str() {
        parts.push(reason.to_string());
    }
    parts.push(when(entry["atMs"].as_i64().unwrap_or_default()));
    parts.join(" \u{b7} ")
}

impl SecurityTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |page: &mut Self, store, cx| {
            let active = store.read(cx).active_tab() == SettingsTabId::Security;
            if active && !page.was_active {
                page.reload(cx);
            }
            page.was_active = active;
            cx.notify();
        })
        .detach();
        let refresh = cx.spawn(async move |page, cx| {
            loop {
                let _ = page.update(cx, |page, cx| {
                    if page.store.read(cx).active_tab() == SettingsTabId::Security {
                        page.reload(cx);
                    }
                });
                cx.background_executor().timer(REFRESH_EVERY).await;
            }
        });
        Self {
            store,
            fields: FieldStates::default(),
            view: None,
            error: None,
            was_active: true,
            _refresh: Some(refresh),
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        if !self.store.read(cx).request().gxserver_rpc_available {
            self.error = Some("Ghostex's background service is not reachable.".to_string());
            cx.notify();
            return;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/securityLog",
            json!({ "limit": SHOWN_ENTRIES }),
            CALL_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    match result {
                        Ok(answer) => {
                            page.view = Some(answer);
                            page.error = None;
                        }
                        Err(error) => page.error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// The chain check, then every entry, newest first.
    fn log_section(&mut self, p: &SettingsPalette) -> Option<Div> {
        let view = self.view.as_ref()?;
        let check = &view["check"];
        let intact = check["intact"].as_bool() == Some(true);
        let count = check["entries"].as_u64().unwrap_or(0);
        let mut rows = vec![if intact {
            settings_list_item(
                p,
                Some(ListItemStatus::Success),
                None,
                "Log intact",
                Some(
                    div()
                        .child(format!(
                            "{count} entries, each chained to the one before it. Nothing was changed or removed."
                        ))
                        .into_any_element(),
                ),
                None,
            )
        } else {
            settings_list_item(
                p,
                Some(ListItemStatus::Warning),
                None,
                "The security log was changed",
                Some(
                    div()
                        .child(format!(
                            "{} Something on this computer edited the log after Ghostex wrote it: check what happened before trusting the entries.",
                            check
                                .pointer("/problem/message")
                                .and_then(Value::as_str)
                                .unwrap_or("The chain is broken.")
                        ))
                        .into_any_element(),
                ),
                None,
            )
        }];
        let entries = view["entries"].as_array().cloned().unwrap_or_default();
        if entries.is_empty() {
            rows.push(
                static_note(
                    p,
                    "Nothing yet. Pairing a phone, Easy Connect changes, sharing a chat and the other sensitive actions show here.",
                    false,
                )
                .into_any_element(),
            );
        }
        for entry in &entries {
            let (_, status) = outcome_words(entry["outcome"].as_str().unwrap_or_default());
            rows.push(settings_list_item(
                p,
                Some(status),
                None,
                entry["summary"]
                    .as_str()
                    .unwrap_or("Sensitive action")
                    .to_string(),
                Some(div().child(entry_detail(entry)).into_any_element()),
                None,
            ));
        }
        settings_section(
            p,
            "Security log",
            Some("Every sensitive action on this computer and who asked for it, newest first. The important ones also show in the notification bell and on your paired phones.".into()),
            None,
            rows,
        )
    }
}

impl Render for SecurityTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::Security),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Security,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
            return settings_page(&self.store, SettingsTabId::Security, &p, blocks, cx);
        }
        if let Some(error) = self.error.clone() {
            let retry = settings_button(
                &p,
                "security-retry",
                "Try again",
                Some("modals/settings/refresh.svg"),
                ButtonVariant::Outline,
                false,
                None,
                |page: &mut Self, _window, cx| page.reload(cx),
                cx,
            );
            blocks.push(PageBlock::plain(
                h_flex()
                    .w_full()
                    .gap(px(12.0))
                    .items_center()
                    .child(div().flex_1().min_w_0().child(static_note(&p, error, true)))
                    .child(retry),
            ));
        }
        if self.view.is_none() {
            if self.error.is_none() {
                blocks.push(PageBlock::plain(static_note(
                    &p,
                    "Reading the security log\u{2026}",
                    false,
                )));
            }
            return settings_page(&self.store, SettingsTabId::Security, &p, blocks, cx);
        }
        if let Some(section) = self.log_section(&p) {
            let mut block = PageBlock::section("log", section);
            block.margin_top += 20.0;
            blocks.push(block);
        }
        settings_page(&self.store, SettingsTabId::Security, &p, blocks, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_says_how_it_ended_and_who_asked() {
        let detail = entry_detail(&json!({
            "outcome": "refused",
            "reason": "Only you can change it.",
            "target": "remoteMachines",
            "actor": { "label": "An agent or the app (says session G1)" },
            "atMs": 0,
        }));
        assert!(detail.starts_with(
            "Refused \u{b7} asked by An agent or the app (says session G1) \u{b7} remoteMachines \u{b7} Only you can change it."
        ));
    }
}
