//! Settings > Collab (Ghostex Collab): Your name, what a new share starts with, and the chats
//! shared right now with Stop sharing. Everything here is the Collab component's (`/api/collab`,
//! server/collab/src/owner.rs, through gxserver), read when the page opens, every 5 seconds while
//! it is the open page, and after each change. The page shows only while Collab is installed
//! (Settings > Extensions installs it, with the owner's approval); Uninstall is here too.
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1610): "we need a page in settings called "Collab" and we need to call this feature Ghostex Collab": the global switch (off by default), the defaults a new share starts with (24-hour expiry, a code the guest types, approve each message) and the list of active shares.
//!
//! CDXC:Collab 2026-11-08 DECISION:
//! User (#495): a link lasts an hour by default and a day at most, a code is always asked, and Send is never a default (it is turned on per chat, with approval).
//!
//! CDXC:Collab 2026-11-08 DECISION:
//! User (#495, Q1730): Collab is an extension component. Its on/off switch is gone: it is on while installed, installing asks for the owner's approval (Settings > Extensions), and uninstalling (here or there) never asks.
//! SEE-ALSO: server/collab/src/config.rs (the choices and their defaults), packages/settings-catalog/src/pages/collab.rs (its search rows), apps/desktop/src/app/window/collab_share.rs (a chat's Share popover).
use super::super::catalog::SettingOption;
use super::super::fields::notice::{is_service_unreachable, service_unreachable_state};
use super::super::fields::{
    ButtonVariant, CONTROL_LANE_WIDTH, FieldStates, ListItemStatus, RowSpec, SettingsPage,
    segmented_field, setting_row, settings_button, settings_list_item, settings_note_row,
    settings_section, settings_text_input, static_note,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, refresh_collab_component, store_gxserver_rpc};
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
    /// The guest messages log, newest first (`op: "audit"`).
    log: Vec<Value>,
    /// The Your name field's typing, saved once it pauses.
    name_edit: u64,
    /// Uninstalling now.
    removing: bool,
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
    let at = expires_at_ms.unwrap_or_default();
    let left = (at - chrono::Utc::now().timestamp_millis()).max(0) / 60_000;
    match left {
        0..=59 => format!("ends in {left} min"),
        _ => format!("ends in {} h", (left + 30) / 60),
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
            log: Vec::new(),
            name_edit: 0,
            removing: false,
            was_active: true,
            _refresh: Some(refresh),
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.call(json!({ "op": "read" }), cx);
        if !self.store.read(cx).request().gxserver_rpc_available {
            return;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/collab",
            json!({ "op": "audit", "limit": 30 }),
            CALL_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Ok(answer) = result {
                        page.log = answer["entries"].as_array().cloned().unwrap_or_default();
                        cx.notify();
                    }
                });
            },
            cx,
        );
    }

    /// Your name: saved once typing pauses.
    fn name_changed(&mut self, name: String, cx: &mut Context<Self>) {
        self.name_edit += 1;
        let edit = self.name_edit;
        cx.spawn(async move |page, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(700))
                .await;
            let _ = page.update(cx, |page, cx| {
                if page.name_edit == edit {
                    page.set(json!({ "ownerName": name }), cx);
                }
            });
        })
        .detach();
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
                            if answer["installed"] == false {
                                // Uninstalled meanwhile: the rail drops this page.
                                refresh_collab_component(&page.store, cx);
                            }
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

    /// Uninstalls Collab: every share ends and the component is removed. Never asks.
    fn uninstall(&mut self, cx: &mut Context<Self>) {
        if self.removing {
            return;
        }
        self.removing = true;
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/extensionComponents",
            json!({ "op": "uninstall", "name": "collab" }),
            Duration::from_secs(60),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.removing = false;
                    if let Err(error) = result {
                        page.error = Some(error);
                    }
                    refresh_collab_component(&page.store, cx);
                    page.reload(cx);
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

    fn switch_section(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let view = self.view.clone().unwrap_or(Value::Null);
        let version = view["version"].as_str().unwrap_or_default().to_string();
        let description = if self.removing {
            "Uninstalling Collab\u{2026}".to_string()
        } else {
            format!(
                "Installed{}. Share is in each chat's header: it makes a link that opens the chat in any browser. Each share needs your approval. Uninstalling ends every share at once and removes Collab from this computer.",
                if version.is_empty() {
                    String::new()
                } else {
                    format!(" ({version})")
                }
            )
        };
        let uninstall = settings_button(
            p,
            "collab-uninstall",
            "Uninstall",
            None,
            ButtonVariant::Outline,
            self.removing,
            None,
            |page: &mut Self, _window, cx| page.uninstall(cx),
            cx,
        );
        let mut rows = vec![setting_row(
            p,
            "collabEnabled",
            RowSpec::new("Ghostex Collab").description(description),
            None,
            uninstall,
            cx,
        )];
        // CDXC:Collab 2026-11-07 DECISION: the coordinator, for the user: a "Your name" field in Settings > Collab; empty, guests see this computer's user name.
        let config = self.config();
        let name = config["ownerName"].as_str().unwrap_or_default().to_string();
        let placeholder = config["ownerDisplayName"]
            .as_str()
            .unwrap_or("Your name")
            .to_string();
        let input = FieldStates::text_state(
            self,
            &gpui::SharedString::from("collabOwnerName"),
            &name,
            Some(&placeholder),
            |page: &mut Self, text, _window, cx| page.name_changed(text, cx),
            window,
            cx,
        );
        let control = div()
            .w(px(CONTROL_LANE_WIDTH))
            .max_w_full()
            .flex()
            .child(settings_text_input(p, &input, None, false, window, cx))
            .into_any_element();
        rows.push(setting_row(
            p,
            "collabOwnerName",
            RowSpec::new("Your name")
                .description("What guests see for you: \"Shared by …\" and \"Waiting for …\". Empty uses this computer's user name."),
            None,
            control,
            cx,
        ));
        settings_section(p, "", None, None, rows)
    }

    fn new_shares_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let config = self.config();
        let mode = config["mode"].as_str().unwrap_or("approve").to_string();
        let expiry = config["expiry"].as_str().unwrap_or("hour").to_string();
        let rows = vec![
            segmented_field(
                p,
                "collabMode",
                RowSpec::new("Guest messages").description(match mode.as_str() {
                    "view" => "Guests watch; their chat box is off.",
                    _ => "A guest's message waits for you as a card: Send (with your approval), Edit or Decline. Send straight to the agent is turned on per chat, in its Share popover.",
                }),
                None,
                &[option("Ask me", "approve"), option("View only", "view")],
                Some(mode.as_str()),
                None,
                |page: &mut Self, value, _, cx| page.set(json!({ "mode": value }), cx),
                cx,
            ),
            segmented_field(
                p,
                "collabExpiry",
                RowSpec::new("Link ends after").description(
                    "Until I stop it keeps a link live until you stop sharing. Stop sharing, or deleting the chat, ends a link at once. Guests always type the eight-digit code that comes with the link, and every link asks for Touch ID.",
                ),
                None,
                &[
                    option("1 hour", "hour"),
                    option("4 hours", "fourHours"),
                    option("24 hours", "day"),
                    // CDXC:Collab 2026-11-08 DECISION: User (Q1785) "add \"until i stop it\" as an option please here and in settings".
                    option("Until I stop it", "untilStopped"),
                ],
                Some(expiry.as_str()),
                None,
                |page: &mut Self, value, _, cx| page.set(json!({ "expiry": value }), cx),
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
        // The relay's guest page failed this release's check (server/collab/src/page_pin.rs):
        // sharing waits until it passes.
        if let Some(message) = self
            .view
            .as_ref()
            .and_then(|view| view["guestPage"]["message"].as_str())
        {
            rows.push(settings_note_row(p, message.to_string()));
        }
        if shares.is_empty() {
            rows.push(settings_note_row(p, "No chat is shared right now."));
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

fn ago(at_ms: i64) -> String {
    let minutes = (chrono::Utc::now().timestamp_millis() - at_ms).max(0) / 60_000;
    match minutes {
        0 => "just now".to_string(),
        1..=59 => format!("{minutes} min ago"),
        60..=1439 => format!("{} h ago", minutes / 60),
        _ => format!("{} days ago", minutes / 1440),
    }
}

impl CollabTab {
    /// Every guest message and who joined, left or was removed, newest first.
    fn log_section(&mut self, p: &SettingsPalette) -> Option<Div> {
        let mut rows = Vec::new();
        if self.log.is_empty() {
            rows.push(settings_note_row(
                p,
                "Nothing yet. Every guest message and what you did with it shows here.",
            ));
        }
        for entry in &self.log {
            let guest = entry["guest"].as_str().unwrap_or("A guest");
            let text = entry["text"].as_str().unwrap_or_default();
            let short: String = text.chars().take(90).collect();
            let images = entry["images"].as_u64().unwrap_or(0);
            let (title, status, what) = match (entry["event"].as_str(), entry["outcome"].as_str()) {
                (Some("message"), outcome) => (
                    if short.is_empty() {
                        format!("{guest}: {images} picture(s)")
                    } else {
                        format!("{guest}: {short}")
                    },
                    match outcome {
                        Some("sent" | "approved") => ListItemStatus::Success,
                        Some("held") => ListItemStatus::Warning,
                        _ => ListItemStatus::Neutral,
                    },
                    match outcome {
                        Some("sent") => "Went straight to the agent",
                        Some("held") => "Waited for you",
                        Some("approved") => "You sent it",
                        Some("edited") => "You edited it",
                        Some("declined") => "You declined it",
                        _ => "Not accepted",
                    },
                ),
                (Some("joined"), _) => {
                    (format!("{guest} joined"), ListItemStatus::Neutral, "Joined")
                }
                (Some("left"), _) => (format!("{guest} left"), ListItemStatus::Neutral, "Left"),
                (Some("removed"), _) => (
                    format!("{guest} was removed"),
                    ListItemStatus::Neutral,
                    "Removed and blocked",
                ),
                _ => continue,
            };
            let detail = format!(
                "{what} \u{b7} {} \u{b7} {}",
                entry["title"].as_str().unwrap_or("A chat"),
                ago(entry["atMs"].as_i64().unwrap_or_default())
            );
            rows.push(settings_list_item(
                p,
                Some(status),
                None,
                title,
                Some(div().child(detail).into_any_element()),
                None,
            ));
        }
        settings_section(p, "Guest messages log", None, None, rows)
    }
}

impl Render for CollabTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        if self.error.as_deref().is_some_and(is_service_unreachable) {
            blocks.push(PageBlock::plain(service_unreachable_state(
                &p,
                "collab-unreachable",
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
            && let Some(section) = self.switch_section(&p, window, cx)
        {
            let mut block = PageBlock::section("switch", section);
            block.margin_top += 20.0;
            blocks.push(block);
        }
        if self
            .view
            .as_ref()
            .is_some_and(|view| view["installed"] == false)
        {
            blocks.push(PageBlock::plain(static_note(
                &p,
                "Collab isn't installed. Install it in Settings > Extensions.",
                false,
            )));
            return settings_page(&self.store, SettingsTabId::Collab, &p, blocks, cx);
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
        if shown("log")
            && let Some(section) = self.log_section(&p)
        {
            blocks.push(PageBlock::section("log", section));
        }
        settings_page(&self.store, SettingsTabId::Collab, &p, blocks, cx)
    }
}
