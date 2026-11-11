//! Settings > Security: this computer's security log (`/api/securityLog`, server/src/audit/),
//! read when the page opens and every 10 seconds while it is the open page. The first row says
//! whether the log is intact; a broken chain is drawn as a warning with what broke.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User (Q1724, GitHub #495): an append-only, hash-chained log "in Settings > Security, and a notification for every sensitive action. Who (session, device), what, when; you notice in real time."
//! SEE-ALSO: server/src/audit/ (the log and its check), packages/settings-catalog/src/pages/security.rs (its search rows), apps/desktop/src/app/gx_store/notifications/commands.rs (a security row in the bell opens this page).
use super::super::fields::notice::{is_service_unreachable, service_unreachable_state};
use super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, SettingsPage, settings_button, settings_list_item,
    settings_list_item_wrapped, settings_note_row, settings_section, static_note, switch_control,
    toggle_from_row,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, store_gxserver_rpc};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Div, Entity, IntoElement,
    ParentElement as _, Render, Styled as _, Task, Window, div, px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::time::Duration;

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const REFRESH_EVERY: Duration = Duration::from_secs(10);
const SHOWN_ENTRIES: u64 = 200;
const LOG_DESCRIPTION: &str = "Newest first. The important ones also show in the notification bell and on your paired phones.";

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
    /// The Security logs switch (`securityLogsEnabled`) as last seen: the log is read and shown only
    /// while it is on.
    logs_on: bool,
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
        "allowed" => ("Allowed", ListItemStatus::Neutral),
        // How an approval request ended (server/security_http.rs `note_request_event`).
        "expired" => ("Expired", ListItemStatus::Neutral),
        "replaced" => ("Replaced", ListItemStatus::Neutral),
        "withdrawn" => ("Withdrawn", ListItemStatus::Neutral),
        _ => ("Recorded", ListItemStatus::Neutral),
    }
}

/// One line under an entry's title: how it ended, who asked, what it was about, when.
pub(crate) fn entry_detail(entry: &Value) -> String {
    let (outcome, _) = outcome_words(entry["outcome"].as_str().unwrap_or_default());
    let mut parts = vec![outcome.to_string()];
    // A request's own outcome line (`approval.*`) says how it ended in its reason; who asked is
    // on the request's own line.
    if !entry["action"]
        .as_str()
        .is_some_and(|action| action.starts_with("approval."))
    {
        parts.push(format!(
            "asked by {}",
            entry
                .pointer("/actor/label")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ));
    }
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
            let logs_on = store.read(cx).security_logs_on();
            if active && (!page.was_active || (logs_on && !page.logs_on)) {
                page.logs_on = logs_on;
                page.reload(cx);
            }
            page.logs_on = logs_on;
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
            logs_on: false,
            _refresh: Some(refresh),
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        // The log shows only while its switch (`securityLogsEnabled`, on this page) is on.
        if !self.store.read(cx).security_logs_on() {
            return;
        }
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

    /// "Require Touch ID or your password to approve" (`approvalsRequirePresence`, off by
    /// default; CDXC:Security 2026-11-10 in packages/settings-catalog/src/pages/security.rs). The
    /// approving device checks it before it sends the plain approval (`owner_security`).
    fn approvals_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let (label, description) = if cfg!(target_os = "macos") {
            (
                "Require Touch ID or your password to approve",
                "Off: Approve on a card (a new phone or computer, a guest's message in Collab, an agent turning on Easy Connect) goes through with one click. On: Ghostex asks for Touch ID or your Mac password first, and your phone for Face ID or your fingerprint. It applies to every device that approves for this computer.",
            )
        } else if cfg!(target_os = "windows") {
            (
                "Require Windows Hello or your password to approve",
                "Off: Approve on a card (a new phone or computer, a guest's message in Collab, an agent turning on Easy Connect) goes through with one click. On: Ghostex asks for Windows Hello or your Windows password first, and your phone for Face ID or your fingerprint. It applies to every device that approves for this computer.",
            )
        } else {
            (
                "Require your password to approve",
                "Off: Approve on a card (a new phone or computer, a guest's message in Collab, an agent turning on Easy Connect) goes through with one click. On: Ghostex asks for your password first, and your phone for Face ID or your fingerprint. It applies to every device that approves for this computer.",
            )
        };
        /*
        CDXC:Security 2026-11-10 DECISION:
        User, looking at this page: "please move the text we have under the tool tip for that toggle to be inside the actual card instead of in the tooltip so the page doesn't look so empty". This row shows its explanation as a line under its label, inside the card (the list-item style Secrets and Sync use), with no tooltip; other Settings rows keep the 9 Sep "description only in the tooltip" rule (fields/row.rs `setting_row`).
        */
        let key = "approvalsRequirePresence";
        let checked = self.store.read(cx).values().bool(key);
        let switch = switch_control(
            p,
            "approvalsRequirePresence-switch",
            label,
            checked,
            false,
            None,
            move |page: &mut Self, next, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| store.update_setting(key, json!(next), cx));
            },
            cx,
        );
        let press = switch.press();
        let row = toggle_from_row(
            p,
            "approvalsRequirePresence-row",
            settings_list_item_wrapped(
                p,
                label,
                div().child(description).into_any_element(),
                Some(switch.into_any_element()),
            ),
            press,
        );
        settings_section(p, "Approvals", None, None, vec![row])
    }

    /// The chain check, then every entry, newest first.
    fn log_section(&mut self, p: &SettingsPalette, switch_row: AnyElement) -> Option<Div> {
        let Some(view) = self.view.as_ref().filter(|_| self.logs_on) else {
            return settings_section(
                p,
                "Log",
                Some(LOG_DESCRIPTION.into()),
                None,
                vec![switch_row],
            );
        };
        let check = &view["check"];
        let intact = check["intact"].as_bool() == Some(true);
        let count = check["entries"].as_u64().unwrap_or(0);
        let mut rows = vec![
            switch_row,
            if intact {
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
            },
        ];
        let entries = view["entries"].as_array().cloned().unwrap_or_default();
        if entries.is_empty() {
            // In the row inset, as Secrets' notes are, not flush against the card's edges.
            rows.push(settings_note_row(p, "Nothing yet. Pairing a phone, Easy Connect changes, sharing a chat and the other sensitive actions show here."));
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
        settings_section(p, "Log", Some(LOG_DESCRIPTION.into()), None, rows)
    }

    /*
    CDXC:Security 2026-11-10 DECISION:
    User (Q2173): Settings > Security shows "What still asks": a short read-only list of what asks for an Approve under today's rule, one line each in the app's own words, and one closing line that everything else works without asking. The lines are `ghostex_gx_protocol::approval::STILL_ASKS`, the same list gxserver's rule reads (server/src/security/policy.rs), so the page can't drift from the code.
    */
    /// "What still asks": one row per line of `approval::STILL_ASKS`, then the closing line.
    fn still_asks_section(&self, p: &SettingsPalette) -> Option<Div> {
        use ghostex_gx_protocol::approval::{STILL_ASKS, STILL_ASKS_CLOSING};
        let mut rows: Vec<AnyElement> = STILL_ASKS
            .iter()
            .map(|(_, _, line)| settings_list_item(p, None, None, line.to_string(), None, None))
            .collect();
        rows.push(settings_note_row(p, STILL_ASKS_CLOSING));
        settings_section(p, "What still asks", None, None, rows)
    }

    /*
    CDXC:Security 2026-11-10 DECISION:
    User (Q2173): the Security logs switch moves here from Settings > Extensions > Experimental, with Open log beside it; the log shows on this page only while it is on. Same key (`securityLogsEnabled`) and behaviour. Open log turns it on when it is off and scrolls to the log.
    */
    /// The Security logs switch with Open log beside it.
    fn log_switch_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let key = "securityLogsEnabled";
        let on = self.logs_on;
        let switch = switch_control(
            p,
            "securityLogsEnabled-switch",
            "Show the security log",
            on,
            false,
            None,
            move |page: &mut Self, next, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| store.update_setting(key, json!(next), cx));
            },
            cx,
        );
        let press = switch.press();
        let open_log = settings_button(
            p,
            "security-open-log",
            "Open log",
            None,
            ButtonVariant::Outline,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    if !store.security_logs_on() {
                        store.update_setting(key, json!(true), cx);
                    }
                    store.scroll_to_section(SettingsTabId::Security, "log", cx);
                });
            },
            cx,
        );
        toggle_from_row(
            p,
            "securityLogsEnabled-row",
            settings_list_item_wrapped(
                p,
                "Show the security log",
                div()
                    .child("Every pairing, Easy Connect change, Collab share and approval on this computer, and who asked for it. The log is kept and its alerts arrive whether this is on or off.")
                    .into_any_element(),
                Some(
                    h_flex()
                        .gap(px(12.0))
                        .items_center()
                        .child(open_log)
                        .child(switch.into_any_element())
                        .into_any_element(),
                ),
            ),
            press,
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
        self.logs_on = self.store.read(cx).security_logs_on();
        if let Some(section) = self.approvals_section(&p, cx) {
            blocks.push(PageBlock::section("approvals", section));
        }
        if let Some(section) = self.still_asks_section(&p) {
            blocks.push(PageBlock::section("stillAsks", section));
        }
        let switch_row = self.log_switch_row(&p, cx);
        if self.logs_on {
            if self.error.as_deref().is_some_and(is_service_unreachable) {
                blocks.push(PageBlock::plain(service_unreachable_state(
                    &p,
                    "security-unreachable",
                    |page: &mut Self, _window, cx| page.reload(cx),
                    cx,
                )));
            }
            if let Some(error) = self
                .error
                .clone()
                .filter(|error| !is_service_unreachable(error))
            {
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
        }
        if let Some(section) = self.log_section(&p, switch_row) {
            blocks.push(PageBlock::section("log", section));
        }
        if self.logs_on && self.view.is_none() && self.error.is_none() {
            blocks.push(PageBlock::plain(static_note(
                &p,
                "Reading the security log\u{2026}",
                false,
            )));
        }
        settings_page(&self.store, SettingsTabId::Security, &p, blocks, cx)
    }
}
