//! CLIProxyAPI hubs in Settings > Accounts (the CLIProxyAPI built-in extension): the hubs and the
//! accounts each one pools in the list (`render_hub_list`), Add a hub (`render_add_hub_pane`), a
//! hub's pane (its address, keys, label, Remove) and a hub account's read-only usage.
//!
//! gxserver sends `cliproxyHubs` only while the extension is on (server/src/accounts/hub/), so
//! nothing here shows while it is off, on this computer or another one the page shows. Keys are
//! only ever typed here: gxserver never sends them back, and a saved key shows as an empty field
//! that replaces it when filled.
//!
//! CDXC:AgentProviders 2026-11-18 SEE-ALSO: the user's decision on the `cliProxyApi` entry in packages/settings-catalog/src/data/official_extensions.rs and in server/src/accounts/hub/mod.rs.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    FieldStates, RowSpec, SizedButtonSize, SizedButtonVariant, setting_row, settings_icon,
    settings_sized_button, settings_text_input, switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{store_copy_to_clipboard, store_host_message};
use super::client::AccountsClient;
use super::data::{Account, AccountsState, provider_label};
use super::editor::{DETAIL_CONTROL_WIDTH, DETAIL_INSET};
use super::list::{list_item, list_item_with_title};
use super::menus::MenuRow;
use super::widgets::{TagTone, account_logo, account_text, status_tag, usage_bar, usage_lines};
use super::{AccountsTab, Pane};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Task, Window,
    div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

const HUB_OAUTH_POLL: Duration = Duration::from_secs(2);

/// An active v8 Management API OAuth sign-in on a CLIProxyAPI hub (Claude or Codex only).
pub(crate) struct HubOAuthFlow {
    pub(crate) id: u64,
    pub(crate) hub_id: String,
    pub(crate) provider: &'static str,
    pub(crate) url: String,
    pub(crate) state: String,
    pub(crate) is_local: bool,
    /// `"starting"`, `"waiting"`, `"ok"`, or `"error"`.
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) input: String,
    pub(crate) submitting: bool,
    pub(crate) poll: Option<Task<()>>,
}

/// The draft of the Add a hub form (`new`) or of a saved hub's fields (its id).
#[derive(Clone, Debug, Default)]
pub(crate) struct HubDraft {
    pub(crate) url: String,
    pub(crate) client_key: String,
    pub(crate) management_key: String,
    pub(crate) label: String,
    /// The Remove confirmation is open.
    pub(crate) remove: bool,
    /// The inline Add account (Claude / Codex) picker is open.
    pub(crate) pick_add_account: bool,
}

impl HubDraft {
    fn typed(&self) -> bool {
        !(self.url.is_empty()
            && self.client_key.is_empty()
            && self.management_key.is_empty()
            && self.label.is_empty())
    }
}

/// One hub of `cliproxyHubs`.
#[derive(Clone, Debug)]
pub(crate) struct Hub {
    raw: Value,
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

impl Hub {
    pub(crate) fn id(&self) -> String {
        text(&self.raw, "id")
    }
    pub(crate) fn label(&self) -> String {
        text(&self.raw, "label")
    }
    fn custom_label(&self) -> String {
        text(&self.raw, "customLabel")
    }
    fn url(&self) -> String {
        text(&self.raw, "url")
    }
    fn host(&self) -> String {
        text(&self.raw, "host")
    }
    fn status(&self) -> String {
        text(&self.raw, "status")
    }
    fn error(&self) -> Option<String> {
        self.raw["error"]
            .as_str()
            .filter(|error| !error.is_empty())
            .map(str::to_string)
    }
    fn has_management_key(&self) -> bool {
        self.raw["hasManagementKey"].as_bool() == Some(true)
    }
    fn session_affinity(&self) -> Option<bool> {
        self.raw["sessionAffinity"].as_bool()
    }
    fn routing_strategy(&self) -> Option<String> {
        self.raw["routingStrategy"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }
    /// The hub's accounts, shaped like any account row (provider, name, usage, usageError,
    /// status, plan) so the usage helpers draw them the same way.
    pub(crate) fn accounts(&self) -> Vec<Account> {
        self.raw["accounts"]
            .as_array()
            .map(|accounts| {
                accounts
                    .iter()
                    .map(|raw| Account { raw: raw.clone() })
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn hub_account_can_manage(account: &Account) -> bool {
    account.raw["canManage"].as_bool() == Some(true)
}

fn hub_account_retry_text(account: &Account) -> Option<String> {
    let raw = account.raw["nextRetryAfter"].as_str()?;
    let at = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
    let seconds = at.timestamp() - chrono::Utc::now().timestamp();
    if seconds <= 0 {
        return Some("ready soon".to_string());
    }
    let minutes = seconds / 60;
    Some(if minutes < 60 {
        format!("resets in {}m", minutes.max(1))
    } else if minutes < 24 * 60 {
        format!("resets in {}h {}m", minutes / 60, minutes % 60)
    } else {
        let local = at.with_timezone(&chrono::Local);
        format!("resets {}", local.format("%a %-H:%M"))
    })
}

impl AccountsState {
    /// The CLIProxyAPI hubs; `None` while the extension is off (gxserver leaves the key out).
    pub(crate) fn hubs(&self) -> Option<Vec<Hub>> {
        self.raw
            .get("cliproxyHubs")?
            .as_array()
            .map(|hubs| hubs.iter().map(|raw| Hub { raw: raw.clone() }).collect())
    }

    pub(crate) fn hub(&self, id: &str) -> Option<Hub> {
        self.hubs()?.into_iter().find(|hub| hub.id() == id)
    }

    /// A hub account by its row id (`<hub id>:<index>`), with its hub.
    pub(crate) fn hub_account(&self, id: &str) -> Option<(Hub, Account)> {
        self.hubs()?.into_iter().find_map(|hub| {
            let account = hub.accounts().into_iter().find(|a| a.id() == id)?;
            Some((hub, account))
        })
    }
}

/// What a hub account's state says, for its list row and pane.
fn account_state(account: &Account) -> Option<(String, TagTone)> {
    match account.status().as_str() {
        "disabled" => Some(("Disabled".into(), TagTone::Off)),
        "error" => Some((
            if account.raw["requiresSignIn"].as_bool() == Some(true) {
                "Sign in needed".into()
            } else {
                "Not working".into()
            },
            TagTone::Warning,
        )),
        "coolingDown" => Some((
            match hub_account_retry_text(account) {
                Some(reset) => format!("Cooling down · {reset}"),
                None => "Cooling down".into(),
            },
            TagTone::Warning,
        )),
        _ => None,
    }
}

fn muted(p: &SettingsPalette, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(px(12.5))
        .line_height(px(18.0))
        .text_color(hsla(p.muted))
        .child(text.into())
        .into_any_element()
}

fn server_mark(p: &SettingsPalette, size: f32) -> AnyElement {
    div()
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .child(settings_icon("modals/settings/server.svg", size, p.muted))
        .into_any_element()
}

impl AccountsTab {
    fn hub_draft(&mut self, key: &str) -> HubDraft {
        self.hub_drafts.get(key).cloned().unwrap_or_default()
    }

    fn update_hub_draft(
        &mut self,
        key: &str,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut HubDraft),
    ) {
        change(self.hub_drafts.entry(key.to_string()).or_default());
        cx.notify();
    }

    /// The list's hub section: a header, each hub with the accounts it pools under it, or Add a
    /// hub when there is none. Nothing while the extension is off.
    pub(crate) fn render_hub_list(
        &mut self,
        p: &SettingsPalette,
        data: &AccountsState,
        pane: &Pane,
        hide: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(hubs) = data.hubs() else {
            return Vec::new();
        };
        let mut children = vec![
            h_flex()
                .w_full()
                .px(px(8.0))
                .pt(px(4.0))
                .pb(px(4.0))
                .gap(px(8.0))
                .items_center()
                .child(settings_icon("modals/settings/server.svg", 14.0, p.muted))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(p.foreground))
                        .child("CLIProxyAPI"),
                )
                .into_any_element(),
        ];
        if hubs.is_empty() {
            children.push(list_item(
                p,
                "accounts-hub-add-row".into(),
                *pane == Pane::AddHub,
                server_mark(p, 15.0),
                "Add a hub".into(),
                Some(muted(p, "A CLIProxyAPI you run")),
                |page, cx| page.select_pane(Pane::AddHub, cx),
                cx,
            ));
        }
        for hub in hubs {
            let id = hub.id();
            let accounts = hub.accounts();
            let detail = match hub.status().as_str() {
                "unavailable" => "Can't be read right now".to_string(),
                "loading" => "Reading its accounts…".to_string(),
                "noManagementKey" => format!("{} · picks its own account", hub.host()),
                _ if accounts.len() == 1 => "1 account".to_string(),
                _ => format!("{} accounts", accounts.len()),
            };
            let select_id = id.clone();
            children.push(list_item(
                p,
                SharedString::from(format!("accounts-hub-{id}")),
                *pane == Pane::Hub(id.clone()),
                server_mark(p, 15.0),
                hub.label().into(),
                Some(muted(p, detail)),
                move |page, cx| page.select_pane(Pane::Hub(select_id.clone()), cx),
                cx,
            ));
            for account in accounts {
                children.push(self.render_hub_list_account(p, &account, pane, hide, cx));
            }
        }
        children
    }

    /// One hub account under its hub: logo, name and its tightest usage bar, read only.
    fn render_hub_list_account(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        pane: &Pane,
        hide: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = account.id();
        let provider = account.provider();
        let mark = div()
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .when(account.status() == "disabled", |this| this.opacity(0.35))
            .child(account_logo(p, &provider, 15.0))
            .into_any_element();
        let detail = match (account_state(account), account.tightest_percent()) {
            (Some((state, _)), _) => muted(p, state),
            (None, Some(used)) => h_flex()
                .w_full()
                .gap(px(6.0))
                .items_center()
                .child(usage_bar(p, used, 4.0))
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(MODAL_MONO_FONT)
                        .text_size(px(10.5))
                        .text_color(hsla(p.muted))
                        .child(format!("{}%", used.round() as i64)),
                )
                .into_any_element(),
            (None, None) => muted(
                p,
                if account.usage_error().is_some() {
                    "Usage can't be read"
                } else {
                    "No usage yet"
                },
            ),
        };
        let title = div()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(account_text(&account.name(), hide))
            .into_any_element();
        let select_id = id.clone();
        div()
            .pl(px(14.0))
            .child(list_item_with_title(
                p,
                SharedString::from(format!("accounts-hub-account-{id}")),
                *pane == Pane::HubAccount(id.clone()),
                mark,
                title,
                Some(detail),
                move |page, cx| page.select_pane(Pane::HubAccount(select_id.clone()), cx),
                cx,
            ))
            .into_any_element()
    }

    /// A key, address or label field of the hub form `key`.
    #[allow(clippy::too_many_arguments)]
    fn hub_field(
        &mut self,
        p: &SettingsPalette,
        key: &str,
        field: &'static str,
        title: &'static str,
        description: &'static str,
        placeholder: &'static str,
        secret: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let draft = self.hub_draft(key);
        let value = match field {
            "url" => draft.url,
            "clientKey" => draft.client_key,
            "managementKey" => draft.management_key,
            _ => draft.label,
        };
        let input_id = SharedString::from(format!("accounts-hub-{key}-{field}"));
        let draft_key = key.to_string();
        let input = FieldStates::text_state(
            self,
            &input_id,
            &value,
            Some(placeholder),
            move |page: &mut Self, text, _window, cx| {
                let limit = if field == "label" { 119 } else { 2048 };
                let text: String = text.trim().chars().take(limit).collect();
                page.update_hub_draft(&draft_key, cx, |draft| match field {
                    "url" => draft.url = text,
                    "clientKey" => draft.client_key = text,
                    "managementKey" => draft.management_key = text,
                    _ => draft.label = text,
                })
            },
            window,
            cx,
        );
        if secret {
            super::widgets::sync_masked(
                &mut self.masked_inputs,
                &input_id,
                &input,
                true,
                window,
                cx,
            );
        }
        setting_row(
            p,
            SharedString::from(format!("accounts-hub-{key}-{field}-row")),
            RowSpec::new(title).description(description).inset_x(0.0),
            None,
            settings_text_input(p, &input, Some(DETAIL_CONTROL_WIDTH), secret, window, cx),
            cx,
        )
    }

    /// Add a hub: address, client key, management key (optional) and label (optional).
    pub(crate) fn render_add_hub_pane(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.client.read(cx).busy;
        let draft = self.hub_draft("new");
        let rows = vec![
            self.hub_field(p, "new", "url", "Address", "Where the hub answers, as you'd open it in a browser.", "http://127.0.0.1:8317", false, window, cx),
            self.hub_field(p, "new", "clientKey", "Client key", "One of the keys under api-keys in the hub's config. Sessions on the hub use it.", "Required", true, window, cx),
            self.hub_field(p, "new", "managementKey", "Management key", "Optional. Lets Ghostex list the hub's accounts and their usage. It can read every login the hub holds, so Ghostex keeps it sealed on this computer and never syncs it.", "Optional", true, window, cx),
            self.hub_field(p, "new", "label", "Name", "Optional. Shown in the account list and when a session starts.", "The hub's address", false, window, cx),
        ];
        let ready = !draft.url.is_empty() && !draft.client_key.is_empty();
        let add = settings_sized_button(
            p,
            "accounts-hub-add",
            "Add hub",
            None,
            None,
            SizedButtonVariant::Primary,
            SizedButtonSize::Default,
            busy || !ready,
            None,
            |page: &mut Self, _window, cx| page.add_hub(cx),
            cx,
        );
        let guide_btn = settings_sized_button(
            p,
            "accounts-hub-add-guide",
            "Setup instructions",
            Some("modals/settings/info-circle.svg"),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Sm,
            false,
            None,
            |page: &mut Self, window, cx| page.open_cliproxy_guide(window, cx),
            cx,
        );
        v_flex()
            .id("accounts-hub-add-pane")
            .w_full()
            .p(px(DETAIL_INSET))
            .gap(px(12.0))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(10.0))
                    .items_center()
                    .child(settings_icon("modals/settings/server.svg", 20.0, p.foreground))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(16.0))
                            .line_height(px(22.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("Add a CLIProxyAPI hub"),
                    )
                    .child(guide_btn),
            )
            .child(muted(p, "Point Ghostex at a CLIProxyAPI instance running on your always-on machine. With a management key, Ghostex enables session affinity by default (for prompt caching) and lets you sign in, check status/cooldowns, and remove Claude and Codex accounts here."))
            .child(muted(p, "Only add a hub you run yourself. Do not add Antigravity to CLIProxyAPI (account bans), and using 2 accounts of the same provider on the same PC is not recommended."))
            .child(v_flex().w_full().children(rows))
            .child(h_flex().child(add))
            .into_any_element()
    }

    fn add_hub(&mut self, cx: &mut Context<Self>) {
        let draft = self.hub_draft("new");
        let this = cx.weak_entity();
        let url = draft.url.clone();
        let mut params = json!({
            "operation": "hubAdd",
            "url": draft.url,
            "clientKey": draft.client_key,
            "label": draft.label,
        });
        if !draft.management_key.is_empty() {
            params["managementKey"] = json!(draft.management_key);
        }
        self.account_request(
            params,
            Some(Box::new(move |ok, cx| {
                if !ok {
                    return;
                }
                let _ = this.update(cx, |page, cx| {
                    page.hub_drafts.remove("new");
                    page.clear_hub_inputs("new", cx);
                    // The hub just added: the last one with this address.
                    let added = page.client.read(cx).data.as_ref().and_then(|data| {
                        data.hubs()?
                            .into_iter()
                            .rev()
                            .find(|hub| url.contains(hub.host().as_str()))
                            .map(|hub| hub.id())
                    });
                    page.pane = added.map(Pane::Hub);
                    cx.notify();
                });
            })),
            cx,
        );
    }

    /// Forgets the inputs of the hub form `key`, so a key typed there never stays in a field.
    fn clear_hub_inputs(&mut self, key: &str, _cx: &mut Context<Self>) {
        let prefix = format!("accounts-hub-{key}-");
        self.fields
            .texts
            .retain(|id, _| !id.as_ref().starts_with(prefix.as_str()));
        self.masked_inputs
            .retain(|id, _| !id.as_ref().starts_with(prefix.as_str()));
    }

    /// A saved hub: its state, Sessions on the hub, its fields, and Remove in the ⋯ menu.
    pub(crate) fn render_hub_pane(
        &mut self,
        p: &SettingsPalette,
        hub: &Hub,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = hub.id();
        let busy = self.client.read(cx).busy;
        if !self.hub_drafts.contains_key(&id) {
            self.hub_drafts.insert(
                id.clone(),
                HubDraft {
                    url: hub.url(),
                    label: hub.custom_label(),
                    ..HubDraft::default()
                },
            );
        }
        let draft = self.hub_draft(&id);
        let mut menu = vec![
            MenuRow::action("Refresh hub", |page, cx| {
                page.account_request(json!({ "operation": "list", "refresh": true }), None, cx);
            }),
            MenuRow::action("Setup instructions", |page, cx| {
                page.cliproxy_guide_open = true;
                page.cliproxy_guide_needs_focus = true;
                cx.notify();
            }),
        ];
        if hub.has_management_key() {
            let forget_id = id.clone();
            menu.push(
                MenuRow::action("Forget management key", move |page, cx| {
                    page.account_request(
                        json!({ "operation": "hubUpdate", "hubId": forget_id, "clearManagementKey": true }),
                        None,
                        cx,
                    )
                })
                .detail("Stop listing or managing this hub's accounts.")
                .group(),
            );
        }
        let remove_id = id.clone();
        menu.push(
            MenuRow::action("Remove…", move |page, cx| {
                page.update_hub_draft(&remove_id, cx, |draft| draft.remove = true)
            })
            .destructive()
            .group(),
        );
        let more = self.render_more_menu(p, &format!("hub-{id}"), "More hub actions", menu, cx);
        let tag = match hub.status().as_str() {
            "unavailable" => Some(status_tag(p, "Can't be read", TagTone::Warning)),
            "noManagementKey" => Some(status_tag(p, "No management key", TagTone::Plain)),
            _ => None,
        };
        let mut blocks: Vec<AnyElement> = vec![
            h_flex()
                .px(px(DETAIL_INSET))
                .gap(px(12.0))
                .items_center()
                .child(
                    div()
                        .size(px(42.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(hsla(p.foreground_alpha(0.03)))
                        .child(settings_icon(
                            "modals/settings/server.svg",
                            20.0,
                            p.foreground,
                        )),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(16.0))
                                .line_height(px(22.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(p.foreground))
                                .child(hub.label()),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(hsla(p.muted))
                                .child(format!("CLIProxyAPI · {}", hub.host())),
                        ),
                )
                .children(tag)
                .child(more)
                .into_any_element(),
        ];
        if draft.remove {
            blocks.push(self.render_hub_remove_confirmation(p, &id, busy, cx));
        }
        if let Some(error) = hub.error() {
            blocks.push(
                div()
                    .mx(px(DETAIL_INSET))
                    .p(px(12.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(modal_rgba(0xfbbf24, 0.3)))
                    .bg(hsla(modal_rgba(0xfbbf24, 0.05)))
                    .child(muted(p, error))
                    .into_any_element(),
            );
        }
        if hub.has_management_key() {
            blocks.push(self.render_hub_accounts_management(p, hub, busy, window, cx));
            blocks.push(self.render_hub_routing_section(p, hub, busy, cx));
        } else {
            blocks.push(
                div()
                    .px(px(DETAIL_INSET))
                    .child(muted(p, "Add this hub's management key below to sign in, inspect status and cooldowns, toggle session affinity, or remove Claude and Codex accounts from Ghostex."))
                    .into_any_element(),
            );
        }
        blocks.push(
            div()
                .px(px(DETAIL_INSET))
                .child(muted(p, "Start a session On the hub from the New Thread picker (or pin it under Claude / Codex rules). Claude hub sessions share ~/.claude/CLAUDE.md, skills/, projects/, and history.jsonl."))
                .into_any_element(),
        );
        let rows = vec![
            self.hub_field(p, &id, "label", "Name", "Shown in the account list and when a session starts.", "The hub's address", false, window, cx),
            self.hub_field(p, &id, "url", "Address", "Where the hub answers.", "http://127.0.0.1:8317", false, window, cx),
            self.hub_field(p, &id, "clientKey", "Client key", "Saved. Type a new one to replace it.", "Saved", true, window, cx),
            self.hub_field(p, &id, "managementKey", "Management key", if hub.has_management_key() { "Saved. Type a new one to replace it." } else { "Optional. Lets Ghostex list the hub's accounts and their usage; kept sealed on this computer." }, if hub.has_management_key() { "Saved" } else { "Optional" }, true, window, cx),
        ];
        let changed = draft.label != hub.custom_label()
            || draft.url != hub.url()
            || !draft.client_key.is_empty()
            || !draft.management_key.is_empty();
        let save_id = id.clone();
        let save = settings_sized_button(
            p,
            SharedString::from(format!("accounts-hub-{id}-save")),
            "Save",
            None,
            None,
            SizedButtonVariant::Primary,
            SizedButtonSize::Sm,
            busy || !changed || draft.url.is_empty(),
            None,
            move |page: &mut Self, _window, cx| page.save_hub(&save_id, cx),
            cx,
        );
        blocks.push(
            v_flex()
                .px(px(DETAIL_INSET))
                .w_full()
                .children(rows)
                .child(h_flex().pt(px(8.0)).child(save))
                .into_any_element(),
        );
        v_flex()
            .id(SharedString::from(format!("accounts-hub-{id}-pane")))
            .w_full()
            .py(px(DETAIL_INSET))
            .gap(px(14.0))
            .children(blocks)
            .into_any_element()
    }

    /// Accounts management card inside a hub pane: Claude and Codex OAuth sign-in, account list
    /// with status/cooldown/usage, and inline cooldown clear.
    fn render_hub_accounts_management(
        &mut self,
        p: &SettingsPalette,
        hub: &Hub,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        let hub_id = hub.id();
        let accounts = hub.accounts();
        let ready_count = accounts.iter().filter(|a| a.status() == "ready").count();
        let cooling_count = accounts
            .iter()
            .filter(|a| a.status() == "coolingDown")
            .count();
        let summary = if accounts.is_empty() {
            "No Claude or Codex accounts signed in yet".to_string()
        } else if cooling_count > 0 {
            format!("{ready_count} ready · {cooling_count} cooling down")
        } else if ready_count == 1 {
            "1 account ready".to_string()
        } else {
            format!("{ready_count} accounts ready")
        };

        let oauth_busy = self.hub_oauth.as_ref().is_some_and(|flow| {
            flow.hub_id == hub_id && matches!(flow.status.as_str(), "starting" | "waiting")
        });
        let local_block_reason = self.hub_add_account_unavailable_reason(cx);
        let picker_open = self.hub_draft(&hub_id).pick_add_account && !oauth_busy;

        let mut header_row = h_flex()
            .w_full()
            .px(px(12.0))
            .py(px(10.0))
            .gap(px(8.0))
            .items_center()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("Accounts on this hub"),
                    )
                    .child(muted(p, summary)),
            );

        if local_block_reason.is_none() {
            let toggle_hub_id = hub_id.clone();
            header_row = header_row.child(settings_sized_button(
                p,
                SharedString::from(format!("accounts-hub-{hub_id}-add-account")),
                "Add account",
                Some("modals/settings/plus.svg"),
                None,
                if picker_open {
                    SizedButtonVariant::Outline
                } else {
                    SizedButtonVariant::Primary
                },
                SizedButtonSize::Sm,
                busy || oauth_busy,
                None,
                move |page: &mut Self, _window, cx| {
                    page.update_hub_draft(&toggle_hub_id, cx, |draft| {
                        draft.pick_add_account = !draft.pick_add_account;
                    });
                },
                cx,
            ));
        }

        let mut rows: Vec<AnyElement> = vec![header_row.into_any_element()];

        if let Some(reason) = local_block_reason {
            rows.push(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(9.0))
                    .border_t_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.foreground_alpha(0.02)))
                    .child(muted(p, reason))
                    .into_any_element(),
            );
        } else if picker_open {
            let claude_hub_id = hub_id.clone();
            let codex_hub_id = hub_id.clone();
            let cancel_hub_id = hub_id.clone();
            rows.push(
                v_flex()
                    .w_full()
                    .px(px(12.0))
                    .py(px(10.0))
                    .gap(px(8.0))
                    .border_t_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.foreground_alpha(0.03)))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("Choose a provider to sign in to this hub"),
                    )
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .flex_wrap()
                            .child(settings_sized_button(
                                p,
                                SharedString::from(format!("accounts-hub-{hub_id}-oauth-claude")),
                                "Claude",
                                Some("modals/settings/plus.svg"),
                                None,
                                SizedButtonVariant::Primary,
                                SizedButtonSize::Sm,
                                busy || oauth_busy,
                                None,
                                move |page: &mut Self, _window, cx| {
                                    page.update_hub_draft(&claude_hub_id, cx, |draft| {
                                        draft.pick_add_account = false;
                                    });
                                    page.start_hub_oauth(claude_hub_id.clone(), "claude", cx);
                                },
                                cx,
                            ))
                            .child(settings_sized_button(
                                p,
                                SharedString::from(format!("accounts-hub-{hub_id}-oauth-codex")),
                                "Codex",
                                Some("modals/settings/plus.svg"),
                                None,
                                SizedButtonVariant::Outline,
                                SizedButtonSize::Sm,
                                busy || oauth_busy,
                                None,
                                move |page: &mut Self, _window, cx| {
                                    page.update_hub_draft(&codex_hub_id, cx, |draft| {
                                        draft.pick_add_account = false;
                                    });
                                    page.start_hub_oauth(codex_hub_id.clone(), "codex", cx);
                                },
                                cx,
                            ))
                            .child(settings_sized_button(
                                p,
                                SharedString::from(format!(
                                    "accounts-hub-{hub_id}-add-account-cancel"
                                )),
                                "Cancel",
                                None,
                                None,
                                SizedButtonVariant::Ghost,
                                SizedButtonSize::Sm,
                                false,
                                None,
                                move |page: &mut Self, _window, cx| {
                                    page.update_hub_draft(&cancel_hub_id, cx, |draft| {
                                        draft.pick_add_account = false;
                                    });
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .p(px(10.0))
                            .rounded(px(MODAL_RADIUS_CONTROL))
                            .border_1()
                            .border_color(hsla(modal_rgba(0xfbbf24, 0.3)))
                            .bg(hsla(modal_rgba(0xfbbf24, 0.05)))
                            .child(muted(
                                p,
                                "Don't also use the same account directly in Claude Swap, Codex Swap, or another CLI login at the same time. Refreshing a token in one invalidates the other (invalid_grant).",
                            )),
                    )
                    .into_any_element(),
            );
        }

        if let Some(oauth_card) = self.render_hub_oauth_card(p, &hub_id, window, cx) {
            rows.push(oauth_card);
        } else if local_block_reason.is_none() && !picker_open {
            rows.push(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(8.0))
                    .border_t_1()
                    .border_color(hsla(p.hairline))
                    .child(muted(
                        p,
                        "Avoid using the same account both on this hub and directly in Claude Swap, Codex Swap, or another CLI login at the same time (refresh tokens invalidate each other: invalid_grant).",
                    ))
                    .into_any_element(),
            );
        }

        for account in accounts {
            let account_id = account.id();
            let provider = account.provider();
            let state = account_state(&account);
            let can_manage = hub_account_can_manage(&account);
            let cooling = account.status() == "coolingDown";
            let open_id = account_id.clone();
            let clear_id = account_id.clone();
            let meta = match (&state, account.tightest_percent()) {
                (Some((label, tone)), _) => status_tag(p, label.clone(), *tone),
                (None, Some(used)) => h_flex()
                    .w(px(96.0))
                    .gap(px(6.0))
                    .items_center()
                    .child(usage_bar(p, used, 4.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(11.0))
                            .text_color(hsla(p.muted))
                            .child(format!("{}%", used.round() as i64)),
                    )
                    .into_any_element(),
                (None, None) => status_tag(p, "Ready", TagTone::Plain),
            };
            let mut row = h_flex()
                .id(SharedString::from(format!(
                    "accounts-hub-inline-{account_id}"
                )))
                .w_full()
                .px(px(12.0))
                .py(px(9.0))
                .gap(px(10.0))
                .items_center()
                .border_t_1()
                .border_color(hsla(p.hairline))
                .cursor_pointer()
                .hover(|this| this.bg(hsla(p.foreground_alpha(0.03))))
                .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                    page.select_pane(Pane::HubAccount(open_id.clone()), cx);
                }))
                .child(account_logo(p, &provider, 16.0))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(13.0))
                                .text_color(hsla(p.foreground))
                                .child(account_text(&account.name(), hide)),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(hsla(p.muted))
                                .child(provider_label(&provider)),
                        ),
                )
                .child(meta);
            if cooling && can_manage {
                row = row.child(settings_sized_button(
                    p,
                    SharedString::from(format!("accounts-hub-inline-reset-{account_id}")),
                    "Clear cooldown",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    busy,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.account_request(
                            json!({
                                "operation": "hubAccountResetCooldown",
                                "accountId": clear_id.clone(),
                            }),
                            None,
                            cx,
                        );
                    },
                    cx,
                ));
            }
            rows.push(row.into_any_element());
        }

        v_flex()
            .mx(px(DETAIL_INSET))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.foreground_alpha(0.02)))
            .overflow_hidden()
            .children(rows)
            .into_any_element()
    }

    /// Session affinity toggle row (on by default when connecting a hub with a management key).
    fn render_hub_routing_section(
        &mut self,
        p: &SettingsPalette,
        hub: &Hub,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hub_id = hub.id();
        let strategy = hub.routing_strategy();
        let control = match hub.session_affinity() {
            Some(affinity) => switch_control(
                p,
                SharedString::from(format!("accounts-hub-{hub_id}-session-affinity-switch")),
                "Session affinity (prompt caching)",
                affinity,
                busy,
                Some("Saving hub routing settings…".into()),
                move |page: &mut Self, next, _window, cx| {
                    page.account_request(
                        json!({
                            "operation": "hubSetSessionAffinity",
                            "hubId": hub_id.clone(),
                            "enabled": next,
                        }),
                        None,
                        cx,
                    );
                },
                cx,
            )
            .into_any_element(),
            None => status_tag(
                p,
                if hub.status() == "loading" {
                    "Checking…"
                } else {
                    "Requires v8"
                },
                TagTone::Off,
            ),
        };
        let description = match (hub.session_affinity(), strategy) {
            (Some(_), Some(strategy)) => format!(
                "Keeps each conversation on one account until it hits a limit so prompt caching stays warm (hub strategy: {strategy}). On by default."
            ),
            (Some(_), None) => "Keeps each conversation on one account until it hits a limit so prompt caching stays warm. On by default.".to_string(),
            (None, _) if hub.status() == "loading" => "Reading routing settings from this hub…".to_string(),
            (None, _) => "Upgrade CLIProxyAPI to v8 to read and toggle session affinity from Ghostex.".to_string(),
        };
        div()
            .px(px(DETAIL_INSET))
            .child(setting_row(
                p,
                SharedString::from(format!("accounts-hub-{}-session-affinity", hub.id())),
                RowSpec::new("Session affinity (prompt caching)")
                    .description(description)
                    .inset_x(0.0),
                None,
                control,
                cx,
            ))
            .into_any_element()
    }

    /// Why Add account on a hub is unavailable here, or `None` when the browser runs on the same
    /// computer as gxserver (where Ghostex can catch the localhost OAuth callback).
    fn hub_add_account_unavailable_reason(&self, cx: &gpui::App) -> Option<&'static str> {
        if self.shown_machine_name(cx).is_some() {
            return Some(
                "Sign an account into a hub from Ghostex on this computer (switch the Machine picker above back to This computer). OAuth sign-in returns to localhost on the computer running your browser.",
            );
        }
        #[cfg(target_arch = "wasm32")]
        {
            let is_loopback = web_sys::window()
                .and_then(|w| w.location().hostname().ok())
                .is_some_and(|host| {
                    matches!(
                        host.to_ascii_lowercase().as_str(),
                        "localhost" | "127.0.0.1" | "::1" | "[::1]"
                    )
                });
            if !is_loopback {
                return Some(
                    "Open Ghostex on the same computer as gxserver (localhost) to sign an account into a hub. OAuth sign-in returns to localhost on the browser's computer.",
                );
            }
        }
        None
    }

    fn start_hub_oauth(&mut self, hub_id: String, provider: &'static str, cx: &mut Context<Self>) {
        if !matches!(provider, "claude" | "codex") {
            return;
        }
        self.hub_oauth_seq = self.hub_oauth_seq.wrapping_add(1);
        let flow_id = self.hub_oauth_seq;
        let local_callback = self.client.read(cx).machine().is_none();
        self.hub_oauth = Some(HubOAuthFlow {
            id: flow_id,
            hub_id: hub_id.clone(),
            provider,
            url: String::new(),
            state: String::new(),
            is_local: false,
            status: "starting".to_string(),
            error: None,
            input: String::new(),
            submitting: false,
            poll: None,
        });
        self.fields.texts.remove(&SharedString::from(format!(
            "accounts-hub-oauth-input-{hub_id}"
        )));
        cx.notify();

        let this = cx.weak_entity();
        AccountsClient::call(
            &self.client.clone(),
            json!({
                "operation": "hubOAuthStart",
                "hubId": hub_id,
                "provider": provider,
                "localCallback": local_callback,
            }),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let Some(flow) = page.hub_oauth.as_mut().filter(|f| f.id == flow_id) else {
                        return;
                    };
                    match result {
                        Ok(answer) => {
                            let url = answer["url"].as_str().unwrap_or_default().to_string();
                            let state = answer["state"].as_str().unwrap_or_default().to_string();
                            let is_local = answer["isLocal"].as_bool() == Some(true);
                            flow.url = url.clone();
                            flow.state = state;
                            flow.is_local = is_local;
                            flow.status = "waiting".to_string();
                            flow.error = None;
                            if !url.is_empty() {
                                store_host_message(
                                    &page.store,
                                    json!({ "type": "openExternalUrl", "url": url }),
                                    cx,
                                );
                            }
                            page.begin_hub_oauth_poll(cx);
                        }
                        Err(error) => {
                            flow.status = "error".to_string();
                            flow.error = Some(error);
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn begin_hub_oauth_poll(&mut self, cx: &mut Context<Self>) {
        let Some((flow_id, hub_id, state)) = self
            .hub_oauth
            .as_ref()
            .filter(|f| f.status == "waiting" && !f.state.is_empty())
            .map(|f| (f.id, f.hub_id.clone(), f.state.clone()))
        else {
            return;
        };
        let task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(HUB_OAUTH_POLL).await;
                let keep_going = this
                    .update(cx, |page, cx| {
                        let still_this = page.hub_oauth.as_ref().is_some_and(|flow| {
                            flow.id == flow_id
                                && flow.hub_id == hub_id
                                && flow.state == state
                                && flow.status == "waiting"
                        });
                        if !still_this {
                            return false;
                        }
                        let weak = cx.weak_entity();
                        let check_state = state.clone();
                        AccountsClient::call(
                            &page.client.clone(),
                            json!({
                                "operation": "hubOAuthStatus",
                                "hubId": hub_id.clone(),
                                "state": state.clone(),
                            }),
                            move |result, cx| {
                                let _ = weak.update(cx, |page, cx| {
                                    let Some(flow) = page
                                        .hub_oauth
                                        .as_mut()
                                        .filter(|f| f.id == flow_id && f.state == check_state)
                                    else {
                                        return;
                                    };
                                    match result {
                                        Ok(answer) => {
                                            let status =
                                                answer["status"].as_str().unwrap_or("wait");
                                            if status == "ok" {
                                                flow.status = "ok".to_string();
                                                flow.error = None;
                                                flow.poll = None;
                                                page.account_request(
                                                    json!({ "operation": "list", "refresh": true }),
                                                    None,
                                                    cx,
                                                );
                                            } else if status == "error" {
                                                flow.status = "error".to_string();
                                                flow.error = answer["error"]
                                                    .as_str()
                                                    .map(str::to_string)
                                                    .or_else(|| {
                                                        Some("The hub sign-in failed.".to_string())
                                                    });
                                                flow.poll = None;
                                            }
                                        }
                                        Err(error) => {
                                            flow.error = Some(error);
                                        }
                                    }
                                    cx.notify();
                                });
                            },
                            cx,
                        );
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        });
        if let Some(flow) = self.hub_oauth.as_mut().filter(|f| f.id == flow_id) {
            flow.poll = Some(task);
        }
    }

    fn submit_hub_oauth_callback(&mut self, cx: &mut Context<Self>) {
        let Some((flow_id, hub_id, provider, state, input)) = self.hub_oauth.as_ref().map(|f| {
            (
                f.id,
                f.hub_id.clone(),
                f.provider,
                f.state.clone(),
                f.input.trim().to_string(),
            )
        }) else {
            return;
        };
        if input.is_empty() || state.is_empty() {
            return;
        }
        if let Some(flow) = self.hub_oauth.as_mut().filter(|f| f.id == flow_id) {
            flow.submitting = true;
            flow.error = None;
        }
        cx.notify();
        let this = cx.weak_entity();
        let expected_state = state.clone();
        AccountsClient::call(
            &self.client.clone(),
            json!({
                "operation": "hubOAuthCallback",
                "hubId": hub_id,
                "provider": provider,
                "state": state,
                "input": input,
            }),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Some(flow) = page
                        .hub_oauth
                        .as_mut()
                        .filter(|f| f.id == flow_id && f.state == expected_state)
                    {
                        flow.submitting = false;
                        match result {
                            Ok(_) => {
                                flow.input.clear();
                                flow.error = None;
                            }
                            Err(error) => {
                                flow.error = Some(error);
                            }
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn cancel_hub_oauth(&mut self, cx: &mut Context<Self>) {
        let current = self.hub_oauth.take();
        if let Some(flow) = current {
            self.fields.texts.remove(&SharedString::from(format!(
                "accounts-hub-oauth-input-{}",
                flow.hub_id
            )));
            if flow.status == "waiting" && !flow.state.is_empty() {
                AccountsClient::call(
                    &self.client.clone(),
                    json!({
                        "operation": "hubOAuthCancel",
                        "hubId": flow.hub_id,
                        "state": flow.state,
                    }),
                    |_, _| {},
                    cx,
                );
            }
        }
        cx.notify();
    }

    fn render_hub_oauth_card(
        &mut self,
        p: &SettingsPalette,
        hub_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let flow = self.hub_oauth.as_ref().filter(|f| f.hub_id == hub_id)?;
        let provider = flow.provider;
        let label = provider_label(provider);
        let status = flow.status.clone();
        let url = flow.url.clone();
        let error = flow.error.clone();
        let input_val = flow.input.clone();
        let submitting = flow.submitting;
        let is_local = flow.is_local;

        let mut parts: Vec<AnyElement> = vec![
            h_flex()
                .gap(px(8.0))
                .items_center()
                .child(account_logo(p, provider, 16.0))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(p.foreground))
                        .child(match status.as_str() {
                            "starting" => format!("Starting {label} sign-in on hub…"),
                            "ok" => format!("{label} account added to hub"),
                            "error" => format!("{label} sign-in failed"),
                            _ => format!("Sign in to {label} in your browser"),
                        }),
                )
                .into_any_element(),
        ];

        if status == "waiting" {
            let port = if provider == "claude" { 54545 } else { 1455 };
            let note = if is_local {
                format!(
                    "Finish signing in to {label} in your browser. Ghostex is listening on 127.0.0.1:{port} for up to 5 minutes and will forward the sign-in to the hub automatically."
                )
            } else {
                "Finish signing in in your browser. When the browser redirects to localhost and shows an error page, copy the full URL from your browser address bar and paste it below so Ghostex can send it to your remote hub.".to_string()
            };
            parts.push(muted(p, note));

            let open_url = url.clone();
            let copy_url = url.clone();
            let mut url_actions: Vec<AnyElement> = Vec::new();
            if !url.is_empty() {
                url_actions.push(settings_sized_button(
                    p,
                    "accounts-hub-oauth-open",
                    "Open sign-in page",
                    Some("modals/settings/external-link.svg"),
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        store_host_message(
                            &page.store,
                            json!({ "type": "openExternalUrl", "url": open_url.clone() }),
                            cx,
                        );
                    },
                    cx,
                ));
                url_actions.push(settings_sized_button(
                    p,
                    "accounts-hub-oauth-copy",
                    "Copy sign-in URL",
                    Some("modals/settings/copy.svg"),
                    None,
                    SizedButtonVariant::Ghost,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        store_copy_to_clipboard(&page.store, copy_url.clone(), cx);
                    },
                    cx,
                ));
            }
            url_actions.push(settings_sized_button(
                p,
                "accounts-hub-oauth-cancel",
                "Cancel",
                None,
                None,
                SizedButtonVariant::Ghost,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| page.cancel_hub_oauth(cx),
                cx,
            ));
            parts.push(
                h_flex()
                    .gap(px(8.0))
                    .flex_wrap()
                    .children(url_actions)
                    .into_any_element(),
            );

            if !is_local {
                let input_id = SharedString::from(format!("accounts-hub-oauth-input-{hub_id}"));
                let input_state = FieldStates::text_state(
                    self,
                    &input_id,
                    &input_val,
                    Some("Paste redirect URL (http://localhost:...) or code"),
                    |page: &mut Self, text, _window, cx| {
                        if let Some(flow) = page.hub_oauth.as_mut() {
                            flow.input = text.chars().take(2048).collect();
                            cx.notify();
                        }
                    },
                    window,
                    cx,
                );
                let submit_btn = settings_sized_button(
                    p,
                    "accounts-hub-oauth-submit",
                    if submitting {
                        "Sending…"
                    } else {
                        "Submit callback"
                    },
                    None,
                    None,
                    SizedButtonVariant::Primary,
                    SizedButtonSize::Sm,
                    submitting || input_val.trim().is_empty(),
                    None,
                    |page: &mut Self, _window, cx| page.submit_hub_oauth_callback(cx),
                    cx,
                );
                parts.push(
                    h_flex()
                        .w_full()
                        .gap(px(8.0))
                        .items_center()
                        .child(div().flex_1().min_w_0().child(settings_text_input(
                            p,
                            &input_state,
                            None,
                            false,
                            window,
                            cx,
                        )))
                        .child(submit_btn)
                        .into_any_element(),
                );
            }
        } else if status == "ok" || status == "error" {
            if let Some(err) = error.clone() {
                parts.push(
                    div()
                        .text_size(px(12.5))
                        .line_height(px(18.0))
                        .text_color(hsla(p.destructive))
                        .child(err)
                        .into_any_element(),
                );
            } else if status == "ok" {
                parts.push(muted(
                    p,
                    "Signed in and forwarded to the hub. Its account list has been refreshed.",
                ));
            }
            let mut end_actions: Vec<AnyElement> = Vec::new();
            if status == "error" {
                let retry_hub_id = hub_id.to_string();
                end_actions.push(settings_sized_button(
                    p,
                    "accounts-hub-oauth-retry",
                    "Try again",
                    None,
                    None,
                    SizedButtonVariant::Primary,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.start_hub_oauth(retry_hub_id.clone(), provider, cx)
                    },
                    cx,
                ));
            }
            end_actions.push(settings_sized_button(
                p,
                "accounts-hub-oauth-close",
                if status == "ok" { "Done" } else { "Close" },
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| page.cancel_hub_oauth(cx),
                cx,
            ));
            parts.push(h_flex().gap(px(8.0)).children(end_actions).into_any_element());
        }

        if status == "waiting" {
            if let Some(err) = error {
                parts.push(
                    div()
                        .text_size(px(12.0))
                        .text_color(hsla(p.destructive))
                        .child(err)
                        .into_any_element(),
                );
            }
        }

        Some(
            v_flex()
                .w_full()
                .px(px(12.0))
                .py(px(10.0))
                .gap(px(8.0))
                .border_t_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.foreground_alpha(0.03)))
                .children(parts)
                .into_any_element(),
        )
    }

    fn save_hub(&mut self, id: &str, cx: &mut Context<Self>) {
        let draft = self.hub_draft(id);
        let mut params = json!({
            "operation": "hubUpdate",
            "hubId": id,
            "label": draft.label,
            "url": draft.url,
        });
        if !draft.client_key.is_empty() {
            params["clientKey"] = json!(draft.client_key);
        }
        if !draft.management_key.is_empty() {
            params["managementKey"] = json!(draft.management_key);
        }
        let this = cx.weak_entity();
        let id = id.to_string();
        self.account_request(
            params,
            Some(Box::new(move |ok, cx| {
                if ok {
                    let _ = this.update(cx, |page, cx| {
                        page.hub_drafts.remove(&id);
                        page.clear_hub_inputs(&id, cx);
                        cx.notify();
                    });
                }
            })),
            cx,
        );
    }

    fn render_hub_remove_confirmation(
        &mut self,
        p: &SettingsPalette,
        id: &str,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let remove_id = id.to_string();
        let confirm = settings_sized_button(
            p,
            SharedString::from(format!("accounts-hub-{id}-remove-confirm")),
            "Remove hub",
            None,
            None,
            SizedButtonVariant::Destructive,
            SizedButtonSize::Sm,
            busy,
            None,
            move |page: &mut Self, _window, cx| {
                let this = cx.weak_entity();
                let close_id = remove_id.clone();
                page.account_request(
                    json!({ "operation": "hubRemove", "hubId": remove_id }),
                    Some(Box::new(move |ok, cx| {
                        if ok {
                            let _ = this.update(cx, |page, cx| {
                                page.hub_drafts.remove(&close_id);
                                page.clear_hub_inputs(&close_id, cx);
                                page.pane = None;
                                cx.notify();
                            });
                        }
                    })),
                    cx,
                );
            },
            cx,
        );
        let cancel_id = id.to_string();
        let cancel = settings_sized_button(
            p,
            SharedString::from(format!("accounts-hub-{id}-remove-cancel")),
            "Cancel",
            None,
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Sm,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.update_hub_draft(&cancel_id, cx, |draft| draft.remove = false)
            },
            cx,
        );
        v_flex()
            .mx(px(DETAIL_INSET))
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(css_fade(p.destructive, 0.35)))
            .bg(hsla(css_fade(p.destructive, 0.06)))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child("Remove this hub?"),
            )
            .child(muted(p, "Sessions already on it keep their folder and conversations, but can't reach the hub once its key is gone. The hub and its accounts are not touched."))
            .child(h_flex().gap(px(8.0)).child(confirm).child(cancel))
            .into_any_element()
    }

    /// A hub account: its usage, read through the hub, and what the hub says about it.
    pub(crate) fn render_hub_account_pane(
        &mut self,
        p: &SettingsPalette,
        hub: &Hub,
        account: &Account,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        let provider = account.provider();
        let tag = account_state(account)
            .map(|(state, tone)| status_tag(p, state, tone))
            .or_else(|| {
                account
                    .plan()
                    .map(|plan| status_tag(p, plan, TagTone::Plain))
            });
        let provider_name = match provider.as_str() {
            "claude" | "codex" => provider_label(&provider).to_string(),
            other => other.to_string(),
        };
        let can_manage = hub_account_can_manage(account);
        let cooling = account.status() == "coolingDown";
        let subtitle = if can_manage {
            format!("{provider_name} · on {}", hub.label())
        } else {
            format!("{provider_name} · on {} · read only", hub.label())
        };
        let mut blocks: Vec<AnyElement> = vec![
            h_flex()
                .px(px(DETAIL_INSET))
                .gap(px(12.0))
                .items_center()
                .child(
                    div()
                        .size(px(42.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(hsla(p.foreground_alpha(0.03)))
                        .child(account_logo(p, &provider, 22.0)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(16.0))
                                .line_height(px(22.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(p.foreground))
                                .child(account_text(&account.name(), hide)),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(hsla(p.muted))
                                .child(subtitle),
                        ),
                )
                .children(tag)
                .into_any_element(),
        ];
        let bars = account.usage_bars();
        let note = account.raw["statusMessage"]
            .as_str()
            .filter(|message| !message.is_empty() && account.status() != "ready")
            .map(str::to_string);
        let busy = self.client.read(cx).busy;
        let account_id = account.id();
        if !bars.is_empty() || account.usage_error().is_some() || note.is_some() || cooling {
            let clear_id = account_id.clone();
            let mut card = v_flex()
                .mx(px(DETAIL_INSET))
                .gap(px(8.0))
                .p(px(12.0))
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.foreground_alpha(0.025)))
                .children(usage_lines(p, &bars, window))
                .children(account.usage_error().map(|error| muted(p, error)))
                .children(note.map(|note| muted(p, format!("The hub says: {note}"))));
            if cooling && can_manage {
                card = card.child(h_flex().child(settings_sized_button(
                    p,
                    SharedString::from(format!("accounts-hub-account-{account_id}-reset-cooldown")),
                    "Clear cooldown",
                    Some("modals/settings/refresh.svg"),
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    busy,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.account_request(
                            json!({
                                "operation": "hubAccountResetCooldown",
                                "accountId": clear_id.clone(),
                            }),
                            None,
                            cx,
                        );
                    },
                    cx,
                )));
            }
            blocks.push(card.into_any_element());
        }
        // Show usage in the sidebar (CDXC:AgentProviders 2026-11-19 in
        // server/src/accounts/hub/sidebar.rs), like a Claude Swap or Codex Swap account's.
        let shown = account.raw["showInSidebar"].as_bool() != Some(false);
        let sidebar_id = account_id.clone();
        blocks.push(
            div()
                .px(px(DETAIL_INSET))
                .child(setting_row(
                    p,
                    SharedString::from(format!("accounts-hub-account-{account_id}-sidebar")),
                    RowSpec::new("Show usage in the sidebar")
                        .description("Keeps this account's usage at the bottom of the sidebar.")
                        .inset_x(0.0),
                    None,
                    switch_control(
                        p,
                        SharedString::from(format!(
                            "accounts-hub-account-{account_id}-sidebar-switch"
                        )),
                        "Show usage in the sidebar",
                        shown,
                        busy,
                        Some("Accounts are being updated.".into()),
                        move |page: &mut Self, next, _window, cx| {
                            page.account_request(
                                json!({
                                    "operation": "hubAccountSidebar",
                                    "accountId": sidebar_id.clone(),
                                    "shown": next,
                                }),
                                None,
                                cx,
                            );
                        },
                        cx,
                    ),
                    cx,
                ))
                .into_any_element(),
        );
        if can_manage {
            let enabled_on_hub = account.status() != "disabled";
            let toggle_id = account_id.clone();
            blocks.push(
                div()
                    .px(px(DETAIL_INSET))
                    .child(setting_row(
                        p,
                        SharedString::from(format!("accounts-hub-account-{account_id}-enabled")),
                        RowSpec::new("Enabled on hub")
                            .description(
                                "Turns this login on or off in CLIProxyAPI's routing pool.",
                            )
                            .inset_x(0.0),
                        None,
                        switch_control(
                            p,
                            SharedString::from(format!(
                                "accounts-hub-account-{account_id}-enabled-switch"
                            )),
                            "Enabled on hub",
                            enabled_on_hub,
                            busy,
                            Some("Updating account on hub…".into()),
                            move |page: &mut Self, next, _window, cx| {
                                page.account_request(
                                    json!({
                                        "operation": "hubAccountSetEnabled",
                                        "accountId": toggle_id.clone(),
                                        "enabled": next,
                                    }),
                                    None,
                                    cx,
                                );
                            },
                            cx,
                        ),
                        cx,
                    ))
                    .into_any_element(),
            );
            let confirming_remove = self.hub_account_remove.as_deref() == Some(account_id.as_str());
            if confirming_remove {
                let confirm_id = account_id.clone();
                let hub_back = hub.id();
                blocks.push(
                    v_flex()
                        .mx(px(DETAIL_INSET))
                        .gap(px(8.0))
                        .p(px(12.0))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(css_fade(p.destructive, 0.35)))
                        .bg(hsla(css_fade(p.destructive, 0.06)))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(p.foreground))
                                .child("Remove this account from the hub?"),
                        )
                        .child(muted(p, "Deletes the saved credential file from CLIProxyAPI. You can sign it in again later from the hub pane."))
                        .child(
                            h_flex()
                                .gap(px(8.0))
                                .child(settings_sized_button(
                                    p,
                                    SharedString::from(format!(
                                        "accounts-hub-account-{account_id}-remove-confirm"
                                    )),
                                    "Remove account",
                                    None,
                                    None,
                                    SizedButtonVariant::Destructive,
                                    SizedButtonSize::Sm,
                                    busy,
                                    None,
                                    move |page: &mut Self, _window, cx| {
                                        let this = cx.weak_entity();
                                        let return_hub = hub_back.clone();
                                        page.account_request(
                                            json!({
                                                "operation": "hubAccountRemove",
                                                "accountId": confirm_id.clone(),
                                            }),
                                            Some(Box::new(move |ok, cx| {
                                                if ok {
                                                    let _ = this.update(cx, |page, cx| {
                                                        page.hub_account_remove = None;
                                                        page.pane = Some(Pane::Hub(return_hub));
                                                        cx.notify();
                                                    });
                                                }
                                            })),
                                            cx,
                                        );
                                    },
                                    cx,
                                ))
                                .child(settings_sized_button(
                                    p,
                                    SharedString::from(format!(
                                        "accounts-hub-account-{account_id}-remove-cancel"
                                    )),
                                    "Cancel",
                                    None,
                                    None,
                                    SizedButtonVariant::Outline,
                                    SizedButtonSize::Sm,
                                    false,
                                    None,
                                    |page: &mut Self, _window, cx| {
                                        page.hub_account_remove = None;
                                        cx.notify();
                                    },
                                    cx,
                                )),
                        )
                        .into_any_element(),
                );
            } else {
                let ask_id = account_id.clone();
                blocks.push(
                    h_flex()
                        .px(px(DETAIL_INSET))
                        .child(settings_sized_button(
                            p,
                            SharedString::from(format!("accounts-hub-account-{account_id}-remove")),
                            "Remove from hub…",
                            Some("modals/settings/trash.svg"),
                            None,
                            SizedButtonVariant::Outline,
                            SizedButtonSize::Sm,
                            busy,
                            None,
                            move |page: &mut Self, _window, cx| {
                                page.hub_account_remove = Some(ask_id.clone());
                                cx.notify();
                            },
                            cx,
                        ))
                        .into_any_element(),
                );
            }
        } else {
            blocks.push(
                div()
                    .px(px(DETAIL_INSET))
                    .child(muted(p, "The hub keeps this account's login and decides when to use it. Add a management key on the hub to enable, disable, or remove it from Ghostex."))
                    .into_any_element(),
            );
        }
        v_flex()
            .id(SharedString::from(format!(
                "accounts-hub-account-{}-pane",
                account.id()
            )))
            .w_full()
            .py(px(DETAIL_INSET))
            .gap(px(14.0))
            .children(blocks)
            .into_any_element()
    }
}
