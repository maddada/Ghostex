//! Settings > Sharing (under Connections, beside Remote, Sync and Collab): on/off toggle,
//! tunnel provider + optional self-hosted OpenTunnel relay URL (`OPENTUNNEL_API`), default link
//! expiry and passcode policy, and the list of active share links with Copy, Stop, and Stop all.
//!
//! CDXC:Sharing 2026-11-24 DECISION:
//! User chose built-in Sharing in Settings > Sharing (under Connections): OpenTunnel by default
//! with a custom relay URL setting (and a pluggable provider seam for cloudflared / Tailscale
//! Funnel later), 24h default expiry, passcode off by default, and active shares with Stop /
//! Stop all. Turning Sharing off hides dependent rows/sections (Settings dependent-row rule) and
//! stops every active share immediately.

use super::super::catalog::SettingOption;
use super::super::fields::notice::{is_service_unreachable, service_unreachable_state};
use super::super::fields::{
    ButtonVariant, CONTROL_LANE_WIDTH, FieldStates, ListItemStatus, RowSpec, SettingsPage,
    segmented_field, setting_row, settings_button, settings_list_item, settings_note_row,
    settings_section, settings_text_input, static_note, switch_control,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, store_gxserver_rpc};
use gpui::{
    AnyView, App, AppContext as _, ClipboardItem, Context, Div, Entity, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Task, Window, div, px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::time::Duration;

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const REFRESH_EVERY: Duration = Duration::from_secs(5);

pub(crate) fn sharing_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| SharingTab::new(store.clone(), cx)).into()
}

pub(crate) struct SharingTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    view: Option<Value>,
    error: Option<String>,
    saving: usize,
    relay_edit: u64,
    copied_share: Option<String>,
    was_active: bool,
    _refresh: Option<Task<()>>,
}

impl SettingsPage for SharingTab {
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

fn format_remaining(expires_at_ms: Option<i64>) -> String {
    let Some(at) = expires_at_ms else {
        return "until stopped".to_string();
    };
    let mins = (at - chrono::Utc::now().timestamp_millis()).max(0) / 60_000;
    match mins {
        0 => "ends in <1 min".to_string(),
        1..=59 => format!("ends in {mins} min"),
        60..=1439 => format!("ends in {} h", (mins + 30) / 60),
        _ => format!("ends in {} d", (mins + 720) / 1440),
    }
}

impl SharingTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |page: &mut Self, store, cx| {
            let active = store.read(cx).active_tab() == SettingsTabId::Sharing;
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
                    if page.store.read(cx).active_tab() == SettingsTabId::Sharing {
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
            relay_edit: 0,
            copied_share: None,
            was_active: true,
            _refresh: Some(refresh),
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.call(json!({ "op": "read" }), cx);
    }

    fn relay_changed(&mut self, url: String, cx: &mut Context<Self>) {
        self.relay_edit += 1;
        let edit = self.relay_edit;
        cx.spawn(async move |page, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(700))
                .await;
            let _ = page.update(cx, |page, cx| {
                if page.relay_edit == edit {
                    page.set(json!({ "relayUrl": url }), cx);
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
            "/api/sharing",
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
        self.call(json!({ "op": "setConfig", "patch": patch }), cx);
    }

    fn config(&self) -> Value {
        self.view
            .as_ref()
            .and_then(|view| view.get("config"))
            .cloned()
            .unwrap_or(Value::Null)
    }

    fn copy_share_url(&mut self, share_id: String, url: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(url));
        self.copied_share = Some(share_id.clone());
        cx.notify();
        cx.spawn(async move |page, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1600))
                .await;
            let _ = page.update(cx, |page, cx| {
                if page.copied_share.as_deref() == Some(share_id.as_str()) {
                    page.copied_share = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn switch_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let enabled = self.config()["enabled"].as_bool().unwrap_or(true);
        let control = switch_control(
            p,
            SharedString::from("sharing-enabled-switch"),
            "Enable sharing",
            enabled,
            false,
            None,
            |page: &mut Self, next, _window, cx| {
                page.set(json!({ "enabled": next }), cx);
            },
            cx,
        );
        let row = setting_row(
            p,
            "sharingEnabled",
            RowSpec::new("Enable sharing").description(
                "Share individual Live pages or local dev server ports over temporary encrypted links. Turning this off stops every active share immediately.",
            ),
            None,
            control,
            cx,
        );
        settings_section(p, "", None, None, vec![row])
    }

    fn provider_section(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let config = self.config();
        let relay_url = config["relayUrl"].as_str().unwrap_or_default().to_string();
        let provider_badge = div()
            .px(px(10.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(gpui::Hsla::from(p.modal.hairline))
            .bg(p.raised)
            .text_size(px(12.5))
            .text_color(p.foreground)
            .child("OpenTunnel (blind E2E TLS)")
            .into_any_element();
        let relay_input = FieldStates::text_state(
            self,
            &SharedString::from("sharingRelayUrl"),
            &relay_url,
            Some("https://opentunnel.xyz"),
            |page: &mut Self, text, _window, cx| page.relay_changed(text, cx),
            window,
            cx,
        );
        let relay_control = div()
            .w(px(CONTROL_LANE_WIDTH))
            .max_w_full()
            .flex()
            .child(settings_text_input(p, &relay_input, None, false, window, cx))
            .into_any_element();
        let rows = vec![
            setting_row(
                p,
                "sharingProvider",
                RowSpec::new("Provider").description(
                    "Blind end-to-end TLS: your machine terminates HTTPS so the relay never reads page HTML, answers, or headers.",
                ),
                None,
                provider_badge,
                cx,
            ),
            setting_row(
                p,
                "sharingRelayUrl",
                RowSpec::new("Relay URL").description(
                    "Optional self-hosted OpenTunnel API URL (OPENTUNNEL_API). Leave blank for the default https://opentunnel.xyz relay.",
                ),
                None,
                relay_control,
                cx,
            ),
        ];
        settings_section(p, "Tunnel provider", None, None, rows)
    }

    fn defaults_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let config = self.config();
        let expiry = config["defaultExpiry"]
            .as_str()
            .unwrap_or("24h")
            .to_string();
        let default_passcode = config["defaultPasscode"].as_bool().unwrap_or(false);
        let passcode_switch = switch_control(
            p,
            SharedString::from("sharing-default-passcode-switch"),
            "Require passcode by default",
            default_passcode,
            false,
            None,
            |page: &mut Self, next, _window, cx| {
                page.set(json!({ "defaultPasscode": next }), cx);
            },
            cx,
        );
        let rows = vec![
            segmented_field(
                p,
                "sharingDefaultExpiry",
                RowSpec::new("Default link expiry").description(
                    "Links stop automatically when time runs out or when Ghostex quits. You can pick a different length per share.",
                ),
                None,
                &[
                    option("1 hour", "1h"),
                    option("24 hours", "24h"),
                    option("7 days", "7d"),
                    option("Until stopped", "untilStopped"),
                ],
                Some(expiry.as_str()),
                None,
                |page: &mut Self, value, _, cx| {
                    page.set(json!({ "defaultExpiry": value }), cx);
                },
                cx,
            ),
            setting_row(
                p,
                "sharingDefaultPasscode",
                RowSpec::new("Require passcode by default").description(
                    "Protect newly shared Live pages and local ports with a 6-digit code. Off by default; you can turn it on for any single share.",
                ),
                None,
                passcode_switch,
                cx,
            ),
        ];
        settings_section(p, "New shares start with", None, None, rows)
    }

    fn active_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let shares = self
            .view
            .as_ref()
            .and_then(|view| view["shares"].as_array())
            .cloned()
            .unwrap_or_default();
        let mut rows = Vec::new();
        if shares.is_empty() {
            rows.push(settings_note_row(
                p,
                "Nothing is shared right now. Share a Live page from its \u{22ef} menu or share a local dev server from Dev servers.",
            ));
        }
        for share in &shares {
            let id = share["shareId"].as_str().unwrap_or_default().to_string();
            let kind = share["kind"].as_str().unwrap_or("page");
            let raw_title = share["title"].as_str().unwrap_or("Shared item");
            let title = if kind == "port" {
                let port = share["port"].as_u64().unwrap_or_default();
                format!("Port: {raw_title} (localhost:{port})")
            } else {
                format!("Live page: {raw_title}")
            };
            let url = share["url"].as_str().unwrap_or_default().to_string();
            let passcode_on = share["passcodeRequired"].as_bool().unwrap_or(false);
            let passcode_note = if passcode_on {
                match share["passcode"].as_str() {
                    Some(code) => format!("Passcode {code} \u{b7} "),
                    None => "Passcode on \u{b7} ".to_string(),
                }
            } else {
                String::new()
            };
            let detail = format!(
                "{passcode_note}OpenTunnel \u{b7} {}",
                format_remaining(share["expiresAtMs"].as_i64())
            );
            let copied = self.copied_share.as_deref() == Some(id.as_str());
            let copy_id = id.clone();
            let copy_url = url.clone();
            let copy_btn = settings_button(
                p,
                row_id(format!("sharing-copy-{id}")),
                if copied { "Copied" } else { "Copy" },
                Some(if copied {
                    "titlebar/check.svg"
                } else {
                    "titlebar/copy.svg"
                }),
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.copy_share_url(copy_id.clone(), copy_url.clone(), cx);
                },
                cx,
            );
            let stop_id = id.clone();
            let stop_btn = settings_button(
                p,
                row_id(format!("sharing-stop-{id}")),
                "Stop",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.call(json!({ "op": "stop", "shareId": stop_id }), cx);
                },
                cx,
            );
            let actions = h_flex()
                .items_center()
                .gap(px(6.0))
                .child(copy_btn)
                .child(stop_btn)
                .into_any_element();
            rows.push(
                settings_list_item(
                    p,
                    Some(ListItemStatus::Success),
                    None,
                    title,
                    Some(div().child(detail).into_any_element()),
                    Some(actions),
                )
                .into_any_element(),
            );
        }
        let stop_all = (!shares.is_empty()).then(|| {
            settings_button(
                p,
                "sharing-stop-all",
                "Stop all",
                None,
                ButtonVariant::Outline,
                false,
                None,
                |page: &mut Self, _window, cx| page.call(json!({ "op": "stopAll" }), cx),
                cx,
            )
        });
        let heading = if shares.is_empty() {
            "Active shares".to_string()
        } else {
            format!("Active shares ({})", shares.len())
        };
        settings_section(p, heading, None, stop_all, rows)
    }
}

impl Render for SharingTab {
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
                store.tab_search(SettingsTabId::Sharing),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Sharing,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
            return settings_page(&self.store, SettingsTabId::Sharing, &p, blocks, cx);
        }
        if self.error.as_deref().is_some_and(is_service_unreachable) {
            blocks.push(PageBlock::plain(service_unreachable_state(
                &p,
                "sharing-unreachable",
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
                "sharing-retry",
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
                    "Reading Sharing\u{2026}",
                    false,
                )));
            }
            return settings_page(&self.store, SettingsTabId::Sharing, &p, blocks, cx);
        }
        if self
            .view
            .as_ref()
            .is_some_and(|view| view["supported"] == false)
        {
            blocks.push(PageBlock::plain(static_note(
                &p,
                "Sharing links isn't available on Windows yet.",
                false,
            )));
            return settings_page(&self.store, SettingsTabId::Sharing, &p, blocks, cx);
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
        let on = self.config()["enabled"].as_bool().unwrap_or(true);
        if on {
            if shown("provider")
                && let Some(section) = self.provider_section(&p, window, cx)
            {
                blocks.push(PageBlock::section("provider", section));
            }
            if shown("defaults")
                && let Some(section) = self.defaults_section(&p, cx)
            {
                blocks.push(PageBlock::section("defaults", section));
            }
            if shown("active")
                && let Some(section) = self.active_section(&p, cx)
            {
                blocks.push(PageBlock::section("active", section));
            }
        }
        settings_page(&self.store, SettingsTabId::Sharing, &p, blocks, cx)
    }
}
