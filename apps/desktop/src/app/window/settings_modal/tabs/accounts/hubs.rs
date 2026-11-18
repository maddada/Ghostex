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
    settings_sized_button, settings_text_input,
};
use super::super::super::palette::SettingsPalette;
use super::data::{Account, AccountsState, provider_label};
use super::editor::{DETAIL_CONTROL_WIDTH, DETAIL_INSET};
use super::list::{list_item, list_item_with_title};
use super::menus::MenuRow;
use super::widgets::{TagTone, account_logo, account_text, status_tag, usage_bar, usage_lines};
use super::{AccountsTab, Pane};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// The draft of the Add a hub form (`new`) or of a saved hub's fields (its id).
#[derive(Clone, Debug, Default)]
pub(crate) struct HubDraft {
    pub(crate) url: String,
    pub(crate) client_key: String,
    pub(crate) management_key: String,
    pub(crate) label: String,
    /// The Remove confirmation is open.
    pub(crate) remove: bool,
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
    fn label(&self) -> String {
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
        "disabled" => Some(("Off on the hub".into(), TagTone::Off)),
        "error" => Some(("Not working".into(), TagTone::Warning)),
        "coolingDown" => Some(("Cooling down".into(), TagTone::Warning)),
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
                let text: String = text.trim().chars().take(4000).collect();
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
        v_flex()
            .id("accounts-hub-add-pane")
            .w_full()
            .p(px(DETAIL_INSET))
            .gap(px(12.0))
            .child(
                h_flex()
                    .gap(px(10.0))
                    .items_center()
                    .child(settings_icon("modals/settings/server.svg", 20.0, p.foreground))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .line_height(px(22.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("Add a CLIProxyAPI hub"),
                    ),
            )
            .child(muted(p, "New Claude and Codex sessions can then start on the hub, which picks one of its accounts for each request. Its accounts show here with their usage."))
            .child(muted(p, "Only add a hub you run yourself, with your own accounts. Sharing or pooling subscription logins can break a provider's terms."))
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
        let mut menu = Vec::new();
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
                .detail("Stop listing the hub's accounts and their usage."),
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
            "noManagementKey" => Some(status_tag(p, "No account list", TagTone::Plain)),
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
        blocks.push(
            div()
                .px(px(DETAIL_INSET))
                .child(muted(p, "Start a session On the hub from the New Thread picker. It runs in a profile folder of its own, and the hub picks the account for each request, so Ghostex never switches it to another account."))
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
        let mut blocks: Vec<AnyElement> =
            vec![
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
                            .child(div().text_size(px(12.5)).text_color(hsla(p.muted)).child(
                                format!("{provider_name} · on {} · read only", hub.label()),
                            )),
                    )
                    .children(tag)
                    .into_any_element(),
            ];
        let bars = account.usage_bars();
        let note = account.raw["statusMessage"]
            .as_str()
            .filter(|message| !message.is_empty() && account.status() != "ready")
            .map(str::to_string);
        if !bars.is_empty() || account.usage_error().is_some() || note.is_some() {
            blocks.push(
                v_flex()
                    .mx(px(DETAIL_INSET))
                    .gap(px(8.0))
                    .p(px(12.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.foreground_alpha(0.025)))
                    .children(usage_lines(p, &bars, window))
                    .children(account.usage_error().map(|error| muted(p, error)))
                    .children(note.map(|note| muted(p, format!("The hub says: {note}"))))
                    .into_any_element(),
            );
        }
        blocks.push(
            div()
                .px(px(DETAIL_INSET))
                .child(muted(p, "The hub keeps this account's login and decides when to use it. Sign it in, out or off on the hub itself."))
                .into_any_element(),
        );
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
