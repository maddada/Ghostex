//! Settings › Secrets: the 1Password accounts Ghostex reads keys and tokens from
//! (`/api/readSecretProviders`, `/api/saveSecretProvider`, `/api/removeSecretProvider`), a check
//! that a reference can be read (`/api/checkSecretReference`, which answers only whether it
//! worked), and a note on how agents ask for secrets privately. The rows are the catalog's
//! (`packages/settings-catalog/src/pages/secrets.rs`); gxserver keeps the list and checks every
//! write.
//!
//! CDXC:Secrets 2026-11-02 WHY:
//! Secrets is a Setup page beside Accounts, not an extension: agents' private secret requests work without it, and the keys other pages keep (the voice agent's) can point at it. A provider entry holds only a name and a 1Password account, so the page shows both; it never shows or asks for a secret's value.
//! SEE-ALSO: server/src/secret_providers/ (the store and the `op` reads), server/src/ghostex_cli/secret.rs (`ghostex secret providers`).
use super::super::super::native_modal_kit::*;
use super::super::catalog::settings_catalog;
use super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, RowSpec, SettingsPage, card_inset, setting_row,
    settings_button, settings_icon_button, settings_list_item, settings_section,
    settings_text_input, static_note,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, store_gxserver_rpc};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, px,
};
use gpui_component::input::InputState;
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

/// `.settings-page-width grid gap-6 px-5 py-5`: 20px above the first section on top of its own 12px.
const PAGE_TOP_PADDING: f32 = 20.0;
const READ_TIMEOUT: Duration = Duration::from_secs(15);
/// A check reads through 1Password, which may wait for the user to unlock it.
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);

const ICON_TRASH: &str = "modals/settings/trash.svg";
const ICON_PLUS: &str = "modals/settings/plus.svg";
const PROVIDERS: &str = "secretProviders";
const CHECK: &str = "secretReferenceCheck";
const REQUESTS: &str = "secretRequests";

pub(crate) fn secrets_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| SecretsTab::new(store.clone(), window, cx))
        .into()
}

/// One provider as `/api/readSecretProviders` lists it.
#[derive(Clone)]
struct ProviderRow {
    id: String,
    label: String,
    account: String,
    status: String,
}

pub(crate) struct SecretsTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// `None` until the first read answered.
    providers: Option<Vec<ProviderRow>>,
    cli_installed: bool,
    read_error: Option<String>,
    add_open: bool,
    name: Entity<InputState>,
    account: Entity<InputState>,
    add_busy: bool,
    add_error: Option<String>,
    reference: Entity<InputState>,
    check_busy: bool,
    /// The last check: whether it read the reference, and what to say.
    check_note: Option<(bool, String)>,
}

impl SettingsPage for SecretsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl super::HoldsUnsavedInput for SecretsTab {
    /// The Add form is open with something typed into it.
    fn holds_unsaved_input(&self, cx: &App) -> bool {
        self.add_open
            && (!self.name.read(cx).value().trim().is_empty()
                || !self.account.read(cx).value().trim().is_empty())
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl SecretsTab {
    fn new(store: Entity<SettingsStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let mut tab = Self {
            store,
            fields: FieldStates::default(),
            providers: None,
            cli_installed: true,
            read_error: None,
            add_open: false,
            name: input("Name, e.g. Personal", window, cx),
            account: input("1Password account, e.g. my.1password.com", window, cx),
            add_busy: false,
            add_error: None,
            reference: input("op://vault/item/field", window, cx),
            check_busy: false,
            check_note: None,
        };
        tab.read(cx);
        tab
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/readSecretProviders",
            json!({}),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    match result {
                        Ok(answer) => {
                            page.read_error = None;
                            page.cli_installed = answer["onePasswordCliInstalled"] != false;
                            page.providers = Some(
                                answer["providers"]
                                    .as_array()
                                    .map_or(&[][..], Vec::as_slice)
                                    .iter()
                                    .map(|row| ProviderRow {
                                        id: text(row, "id"),
                                        label: text(row, "label"),
                                        account: text(row, "account"),
                                        status: text(row, "status"),
                                    })
                                    .collect(),
                            );
                        }
                        Err(error) => page.read_error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label = self.name.read(cx).value().trim().to_string();
        let account = self.account.read(cx).value().trim().to_string();
        if label.is_empty() || account.is_empty() {
            self.add_error = Some("Enter a name and the 1Password account.".to_string());
            cx.notify();
            return;
        }
        self.add_busy = true;
        self.add_error = None;
        let page = cx.entity().downgrade();
        let window_handle = window.window_handle();
        store_gxserver_rpc(
            &self.store,
            "/api/saveSecretProvider",
            json!({ "kind": "onePassword", "label": label, "account": account }),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.add_busy = false;
                    match result {
                        Ok(_) => {
                            page.add_open = false;
                            let (name, account) = (page.name.clone(), page.account.clone());
                            let _ = window_handle.update(cx, |_, window, cx| {
                                name.update(cx, |input, cx| input.set_value("", window, cx));
                                account.update(cx, |input, cx| input.set_value("", window, cx));
                            });
                            page.read(cx);
                        }
                        Err(error) => page.add_error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    fn remove(&mut self, id: String, cx: &mut Context<Self>) {
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/removeSecretProvider",
            json!({ "id": id }),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    match result {
                        Ok(_) => page.read(cx),
                        Err(error) => page.read_error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn check(&mut self, provider: String, cx: &mut Context<Self>) {
        let reference = self.reference.read(cx).value().trim().to_string();
        if reference.is_empty() {
            self.check_note = Some((false, "Enter a reference first.".to_string()));
            cx.notify();
            return;
        }
        self.check_busy = true;
        self.check_note = None;
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/checkSecretReference",
            json!({ "provider": provider, "reference": reference }),
            CHECK_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.check_busy = false;
                    page.check_note = Some(match result {
                        Ok(answer) if answer["readable"] == true => (
                            true,
                            "1Password read it. Ghostex can use this reference.".to_string(),
                        ),
                        Ok(answer) => (false, text(&answer, "message")),
                        Err(error) => (false, error),
                    });
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    fn catalog_row(key: &str) -> Option<(String, String)> {
        settings_catalog()
            .extra_tab(SettingsTabId::Secrets.id())?
            .sections
            .iter()
            .flat_map(|section| section.settings.iter())
            .find(|row| row.key == key)
            .map(|row| (row.title.clone(), row.subtitle.clone()))
    }

    fn provider_rows(
        &mut self,
        p: &super::super::palette::SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        for provider in self.providers.clone().unwrap_or_default() {
            let (status, detail) = match provider.status.as_str() {
                "ready" => (ListItemStatus::Success, "Signed in on this computer"),
                "accountMissing" => (
                    ListItemStatus::Warning,
                    "The 1Password CLI isn't signed in to this account",
                ),
                "cliMissing" => (
                    ListItemStatus::Warning,
                    "The 1Password CLI (op) isn't installed",
                ),
                "unsupported" => (
                    ListItemStatus::Neutral,
                    "Added by a newer Ghostex; this version can't read it",
                ),
                _ => (ListItemStatus::Neutral, ""),
            };
            let detail = if detail.is_empty() {
                provider.account.clone()
            } else {
                format!("{} · {detail}", provider.account)
            };
            let id = provider.id.clone();
            let controls = settings_icon_button(
                p,
                SharedString::from(format!("secret-provider-remove-{}", provider.id)),
                ICON_TRASH,
                14.0,
                24.0,
                ButtonVariant::Ghost,
                Some("Remove".into()),
                false,
                move |page: &mut Self, _window, cx| page.remove(id.clone(), cx),
                cx,
            );
            rows.push(settings_list_item(
                p,
                Some(status),
                None,
                provider.label.clone(),
                Some(div().child(detail).into_any_element()),
                Some(controls),
            ));
        }
        if let Some(providers) = &self.providers
            && providers.is_empty()
        {
            rows.push(card_inset(static_note(
                p,
                "No 1Password account yet. Add one to paste op:// references where Ghostex asks for a key.",
                false,
            )));
        }
        if !self.cli_installed {
            rows.push(card_inset(static_note(
                p,
                "The 1Password CLI (op) isn't installed on this computer. Install it from 1Password's website or with Homebrew (brew install 1password-cli), then turn on \"Integrate with 1Password CLI\" in 1Password's Developer settings.",
                true,
            )));
        }
        let form = if self.add_open {
            v_flex()
                .w_full()
                .gap(px(10.0))
                .child(settings_text_input(p, &self.name, None, false, window, cx))
                .child(settings_text_input(p, &self.account, None, true, window, cx))
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(hsla(p.muted))
                        .child("The account is its sign-in address, email or ID; run op account list in a terminal to see them."),
                )
                .when_some(self.add_error.clone(), |form, error| {
                    form.child(
                        div()
                            .text_size(px(12.5))
                            .text_color(hsla(p.destructive))
                            .child(error),
                    )
                })
                .child(
                    h_flex()
                        .gap(px(8.0))
                        .justify_end()
                        .child(settings_button(
                            p,
                            "secret-provider-cancel",
                            "Cancel",
                            None,
                            ButtonVariant::Outline,
                            self.add_busy,
                            None,
                            |page: &mut Self, _window, cx| {
                                page.add_open = false;
                                page.add_error = None;
                                cx.notify();
                            },
                            cx,
                        ))
                        .child(settings_button(
                            p,
                            "secret-provider-save",
                            "Add",
                            None,
                            ButtonVariant::Default,
                            self.add_busy,
                            None,
                            |page: &mut Self, window, cx| page.add(window, cx),
                            cx,
                        )),
                )
                .into_any_element()
        } else {
            h_flex()
                .child(settings_button(
                    p,
                    "secret-provider-add",
                    "Add 1Password account",
                    Some(ICON_PLUS),
                    ButtonVariant::Outline,
                    false,
                    None,
                    |page: &mut Self, window, cx| {
                        page.add_open = true;
                        let name = page.name.clone();
                        name.update(cx, |input, cx| input.focus(window, cx));
                        cx.notify();
                    },
                    cx,
                ))
                .into_any_element()
        };
        rows.push(card_inset(form));
        if let Some(error) = &self.read_error {
            rows.push(card_inset(
                div()
                    .text_size(px(12.5))
                    .text_color(hsla(p.destructive))
                    .child(format!("Could not read the secret providers: {error}")),
            ));
        }
        rows
    }

    fn check_row(
        &mut self,
        p: &super::super::palette::SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let providers = self.providers.clone().unwrap_or_default();
        let readable: Vec<ProviderRow> = providers
            .into_iter()
            .filter(|provider| provider.status != "unsupported")
            .collect();
        if readable.is_empty() {
            return None;
        }
        let (title, subtitle) = Self::catalog_row(CHECK)?;
        let mut buttons = h_flex().gap(px(8.0)).items_center();
        for provider in &readable {
            let id = provider.id.clone();
            let label = if readable.len() == 1 {
                "Check".to_string()
            } else {
                format!("Check with {}", provider.label)
            };
            buttons = buttons.child(settings_button(
                p,
                SharedString::from(format!("secret-check-{}", provider.id)),
                label,
                None,
                ButtonVariant::Outline,
                self.check_busy,
                None,
                move |page: &mut Self, _window, cx| page.check(id.clone(), cx),
                cx,
            ));
        }
        let note = match (&self.check_note, self.check_busy) {
            (_, true) => Some((
                true,
                "Reading through 1Password; unlock it if it asks…".to_string(),
            )),
            (Some(note), false) => Some(note.clone()),
            (None, false) => None,
        };
        let control = v_flex()
            .w_full()
            .gap(px(6.0))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.0))
                    .items_center()
                    .child(settings_text_input(
                        p,
                        &self.reference,
                        None,
                        true,
                        window,
                        cx,
                    ))
                    .child(buttons),
            )
            .when_some(note, |this, (ok, note)| {
                this.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(hsla(if ok { p.muted } else { p.destructive }))
                        .child(note),
                )
            })
            .into_any_element();
        Some(setting_row(
            p,
            CHECK,
            RowSpec::new(title).description(subtitle).wide(),
            None,
            control,
            cx,
        ))
    }
}

impl Render for SecretsTab {
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
                store.tab_search(SettingsTabId::Secrets),
                matching,
            )
        };
        let mut blocks = Vec::new();
        if search.row_visible("providers", PROVIDERS)
            && let Some((_, subtitle)) = Self::catalog_row(PROVIDERS)
        {
            let rows = self.provider_rows(&p, window, cx);
            if let Some(section) =
                settings_section(&p, "1Password", Some(subtitle.into()), None, rows)
            {
                let mut block = PageBlock::section("providers", section);
                block.margin_top += PAGE_TOP_PADDING;
                blocks.push(block);
            }
        }
        if search.row_visible("check", CHECK)
            && let Some(row) = self.check_row(&p, window, cx)
            && let Some(section) = settings_section(&p, "Check a reference", None, None, vec![row])
        {
            let mut block = PageBlock::section("check", section);
            if blocks.is_empty() {
                block.margin_top += PAGE_TOP_PADDING;
            }
            blocks.push(block);
        }
        if search.row_visible("requests", REQUESTS)
            && let Some((title, subtitle)) = Self::catalog_row(REQUESTS)
            && let Some(section) = settings_section(
                &p,
                title,
                None,
                None,
                vec![card_inset(static_note(&p, subtitle, false))],
            )
        {
            let mut block = PageBlock::section("requests", section);
            if blocks.is_empty() {
                block.margin_top += PAGE_TOP_PADDING;
            }
            blocks.push(block);
        }
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Secrets,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        settings_page(&self.store, SettingsTabId::Secrets, &p, blocks, cx)
    }
}
