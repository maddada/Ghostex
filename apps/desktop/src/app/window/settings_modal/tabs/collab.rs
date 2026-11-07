//! Settings > Collab (Ghostex Collab): the switch, what a new share starts with, and the chats
//! shared right now with Stop sharing. Everything here is gxserver's (`/api/collab`,
//! server/src/server/collab_http.rs), read when the page opens, every 5 seconds while it is the
//! open page, and after each change.
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1610): "we need a page in settings called "Collab" and we need to call this feature Ghostex Collab": the global switch (off by default), the defaults a new share starts with (24-hour expiry, a code the guest types, approve each message) and the list of active shares.
//! SEE-ALSO: server/src/collab/config.rs (the choices and their defaults), packages/settings-catalog/src/pages/collab.rs (its search rows), apps/desktop/src/app/window/collab_share.rs (a chat's Share popover).
use super::super::catalog::SettingOption;
use super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, RowSpec, SettingsPage, segmented_field,
    settings_button, settings_list_item, settings_section, static_note, toggle_field_with,
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
const REFRESH_EVERY: Duration = Duration::from_secs(5);

pub(crate) fn collab_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| CollabTab::new(store.clone(), cx)).into()
}

pub(crate) struct CollabTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    view: Option<Value>,
    error: Option<String>,
    saving: usize,
    was_active: bool,
    _refresh: Option<Task<()>>,
}

impl SettingsPage for CollabTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

fn option(label: &str, value: &str) -> SettingOption {
    SettingOption {
        label: label.to_string(),
        value: value.to_string(),
    }
}

fn row_id(id: String) -> &'static str {
    use std::collections::HashSet;
    use std::sync::Mutex;
    static IDS: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
    let mut ids = IDS.lock().unwrap_or_else(|error| error.into_inner());
    let ids = ids.get_or_insert_with(HashSet::new);
    if let Some(known) = ids.get(id.as_str()) {
        return known;
    }
    let leaked: &'static str = Box::leak(id.into_boxed_str());
    ids.insert(leaked);
    leaked
}

fn ends(expires_at_ms: Option<i64>) -> String {
    let Some(at) = expires_at_ms else {
        return "never ends".to_string();
    };
    let left = (at - chrono::Utc::now().timestamp_millis()).max(0) / 60_000;
    match left {
        0..=59 => format!("ends in {left} min"),
        60..=2879 => format!("ends in {} h", (left + 30) / 60),
        _ => format!("ends in {} days", (left + 720) / 1440),
    }
}

impl CollabTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |page: &mut Self, store, cx| {
            let active = store.read(cx).active_tab() == SettingsTabId::Collab;
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
                    if page.store.read(cx).active_tab() == SettingsTabId::Collab {
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
            saving: 0,
            was_active: true,
            _refresh: Some(refresh),
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.call(json!({ "op": "read" }), cx);
    }

    fn call(&mut self, params: Value, cx: &mut Context<Self>) {
        if !self.store.read(cx).request().gxserver_rpc_available {
            self.error = Some("Ghostex's background service is not reachable.".to_string());
            cx.notify();
            return;
        }
        let read = params["op"] == "read";
        if !read {
            self.saving += 1;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/collab",
            params,
            CALL_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if !read {
                        page.saving = page.saving.saturating_sub(1);
                    }
                    match result {
                        Ok(_) if read && page.saving > 0 => {}
                        Ok(answer) => {
                            page.view = Some(answer);
                            page.error = None;
                        }
                        Err(error) => {
                            page.error = Some(error);
                            if !read {
                                page.reload(cx);
                            }
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Changes some choices: on screen at once, then saved.
    fn set(&mut self, patch: Value, cx: &mut Context<Self>) {
        if let (Some(config), Some(fields)) = (
            self.view
                .as_mut()
                .and_then(|view| view.get_mut("config"))
                .and_then(Value::as_object_mut),
            patch.as_object(),
        ) {
            for (key, value) in fields {
                config.insert(key.clone(), value.clone());
            }
        }
        cx.notify();
        self.call(json!({ "op": "set", "patch": patch }), cx);
    }

    fn config(&self) -> Value {
        self.view
            .as_ref()
            .and_then(|view| view.get("config"))
            .cloned()
            .unwrap_or(Value::Null)
    }

    fn switch_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let on = self.config()["enabled"] == true;
        let description = if on {
            "On. Share is in each chat's header: it makes a link that opens the chat in any browser. Turning this off ends every share at once."
        } else {
            "Share a chat through a link that opens it in any browser, with no app or account: the person you send it to can follow the chat live and write in it. Their messages wait for you unless you choose otherwise."
        };
        let rows = vec![toggle_field_with(
            p,
            "collabEnabled",
            RowSpec::new("Ghostex Collab").description(description),
            on,
            None,
            |page: &mut Self, checked, _, cx| page.set(json!({ "enabled": checked }), cx),
            cx,
        )];
        settings_section(p, "Ghostex Collab", None, None, rows)
    }

    fn new_shares_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let config = self.config();
        let mode = config["mode"].as_str().unwrap_or("approve").to_string();
        let expiry = config["expiry"].as_str().unwrap_or("day").to_string();
        let rows = vec![
            segmented_field(
                p,
                "collabMode",
                RowSpec::new("Guest messages").description(match mode.as_str() {
                    "send" => "A guest's message goes straight to the agent, labelled with their name.",
                    "view" => "Guests watch; their chat box is off.",
                    _ => "A guest's message waits for you as a card: Send, Edit or Decline.",
                }),
                None,
                &[
                    option("Send", "send"),
                    option("Ask me", "approve"),
                    option("View only", "view"),
                ],
                Some(mode.as_str()),
                None,
                |page: &mut Self, value, _, cx| page.set(json!({ "mode": value }), cx),
                cx,
            ),
            segmented_field(
                p,
                "collabExpiry",
                RowSpec::new("Link ends after")
                    .description("Stop sharing ends a link at once, whatever this says."),
                None,
                &[
                    option("1 hour", "hour"),
                    option("24 hours", "day"),
                    option("7 days", "week"),
                    option("Never", "never"),
                ],
                Some(expiry.as_str()),
                None,
                |page: &mut Self, value, _, cx| page.set(json!({ "expiry": value }), cx),
                cx,
            ),
            toggle_field_with(
                p,
                "collabRequireCode",
                RowSpec::new("Ask for a code").description(
                    "Guests also type the six-digit code shown in the Share popover, so a link alone is not enough.",
                ),
                config["requireCode"] != false,
                None,
                |page: &mut Self, checked, _, cx| page.set(json!({ "requireCode": checked }), cx),
                cx,
            ),
        ];
        settings_section(
            p,
            "New shares start with",
            Some("Each chat's Share popover changes its own guest messages.".into()),
            None,
            rows,
        )
    }

    fn shared_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let shares = self
            .view
            .as_ref()
            .and_then(|view| view["shares"].as_array())
            .cloned()
            .unwrap_or_default();
        let mut rows = Vec::new();
        if shares.is_empty() {
            rows.push(static_note(p, "No chat is shared right now.", false).into_any_element());
        }
        for share in &shares {
            let id = share["shareId"].as_str().unwrap_or_default().to_string();
            let watching = share["viewers"].as_array().map(Vec::len).unwrap_or(0);
            let detail = format!(
                "{} watching \u{b7} link {}",
                watching,
                ends(share["expiresAtMs"].as_i64())
            );
            let stop = settings_button(
                p,
                row_id(format!("collab-stop-{id}")),
                "Stop sharing",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.call(json!({ "op": "stop", "shareId": id }), cx)
                },
                cx,
            );
            rows.push(
                settings_list_item(
                    p,
                    Some(ListItemStatus::Success),
                    None,
                    share["title"].as_str().unwrap_or("A chat").to_string(),
                    Some(div().child(detail).into_any_element()),
                    Some(stop),
                )
                .into_any_element(),
            );
        }
        let stop_all = (shares.len() > 1).then(|| {
            settings_button(
                p,
                "collab-stop-all",
                "Stop all",
                None,
                ButtonVariant::Outline,
                false,
                None,
                |page: &mut Self, _window, cx| page.call(json!({ "op": "stopAll" }), cx),
                cx,
            )
        });
        settings_section(p, "Shared now", None, stop_all, rows)
    }
}

impl Render for CollabTab {
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
                store.tab_search(SettingsTabId::Collab),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Collab,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
            return settings_page(&self.store, SettingsTabId::Collab, &p, blocks, cx);
        }
        if let Some(error) = self.error.clone() {
            let retry = settings_button(
                &p,
                "collab-retry",
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
                    "Reading Ghostex Collab\u{2026}",
                    false,
                )));
            }
            return settings_page(&self.store, SettingsTabId::Collab, &p, blocks, cx);
        }
        let shown = |section: &str| {
            let result = search.section(section);
            !result.is_searching || result.has_visible()
        };
        if shown("switch")
            && let Some(section) = self.switch_section(&p, cx)
        {
            let mut block = PageBlock::section("switch", section);
            block.margin_top += 20.0;
            blocks.push(block);
        }
        let on = self.config()["enabled"] == true;
        if shown("newShares")
            && let Some(section) = self.new_shares_section(&p, cx)
        {
            blocks.push(PageBlock::section("newShares", section));
        }
        if on
            && shown("shared")
            && let Some(section) = self.shared_section(&p, cx)
        {
            blocks.push(PageBlock::section("shared", section));
        }
        settings_page(&self.store, SettingsTabId::Collab, &p, blocks, cx)
    }
}
