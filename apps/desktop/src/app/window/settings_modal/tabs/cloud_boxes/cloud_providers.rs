//! "Providers": Instabox and Boat (and the debug-only Local provider), each with its connection
//! and plan, Connect in the browser, Paste Key, and Disconnect (Q998 "browser then key").
//! Instabox's browser sign-in finishes on its own; Boat's asks for the account's email first and
//! then shows a 6-digit code to type on Boat's page.
use super::super::super::fields::{
    ButtonVariant, ListItemStatus, settings_button, settings_section, settings_text_input,
};
use super::super::super::palette::SettingsPalette;
use super::cloud_state::ConnectForm;
use super::{CloudBoxesTab, info_row};
use ghostex_gx_protocol::cloud_box::{
    CloudConnectMethod, CloudProvider, CloudProviderConnectionState, ConnectResult, ProvidersResult,
};
use gpui::{
    AnyElement, AppContext as _, Context, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use gpui_component::input::InputState;
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

const ICON_CLOUD: &str = "modals/settings/cloud.svg";
const ICON_BOX: &str = "modals/settings/box.svg";
const ICON_KEY: &str = "modals/settings/key.svg";
const ICON_EXTERNAL: &str = "modals/settings/external-link.svg";
const ICON_PLUG: &str = "modals/settings/plug-connected.svg";

/// Validating a key or starting a sign-in reaches the provider.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(45);

/// The providers this page always lists, in order; Local joins when gxserver lists it.
const LISTED: [(&str, &str); 2] = [("instabox", "Instabox"), ("boat", "Boat")];

fn provider_tooltip(id: &str) -> &'static str {
    match id {
        "instabox" => {
            "Instabox.sh runs each box as a Linux machine on your own Instabox account, which it bills. Connect signs in in your browser; Ghostex then makes its own API key. Instabox pauses an idle box and keeps its programs running."
        }
        "boat" => {
            "Boat.dev runs each box on your own Boat account, which it bills. Connect asks for your Boat email, opens Boat's page and shows a 6-digit code to type there. A stopped Boat box starts again on a new machine."
        }
        _ => {
            "For development: a box is a Docker container on this computer running the same Ghostex server a cloud box runs. Free, and shown only with Show debug UI controls on."
        }
    }
}

fn plan_text(provider: &CloudProvider) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(account) = provider
        .account_label
        .as_ref()
        .or(provider.account_email.as_ref())
    {
        parts.push(account.clone());
    }
    if let Some(plan) = provider.plan.as_ref().filter(|plan| !plan.is_empty()) {
        let mut plan = plan.clone();
        if let Some(first) = plan.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        parts.push(format!("{plan} plan"));
    }
    if let Some(note) = provider
        .limits
        .as_ref()
        .and_then(|limits| limits.note.clone())
    {
        parts.push(note);
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

impl CloudBoxesTab {
    pub(super) fn cloud_providers_section(
        &mut self,
        p: &SettingsPalette,
        show: impl Fn(&str) -> bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut rows = Vec::new();
        for (id, label) in LISTED {
            let key = if id == "instabox" {
                "cloudBoxesInstabox"
            } else {
                "cloudBoxesBoat"
            };
            if !show(key) {
                continue;
            }
            rows.push(self.cloud_provider_row(p, id, label, cx));
            if let Some(form) = self.connect_form(p, id, window, cx) {
                rows.push(form);
            }
        }
        if show("cloudBoxesInstabox") && self.cloud.provider("local").is_some() {
            rows.push(self.cloud_provider_row(p, "local", "Local (Docker)", cx));
        }
        if let Some(error) = &self.cloud.providers_error {
            rows.push(info_row(
                p,
                "cloud-providers-error",
                Some(ListItemStatus::Warning),
                ICON_CLOUD,
                "Providers",
                format!("Ghostex could not read the providers: {error}"),
                Some(format!("Ghostex could not read the providers: {error}")),
                Vec::new(),
            ));
        }
        settings_section(
            p,
            "Providers",
            Some(
                "A Cloud Box runs a session on a Linux machine in the cloud: pick Cloud Box in a new chat. Boxes run on your own provider account, which bills them."
                    .into(),
            ),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    fn cloud_provider_row(
        &mut self,
        p: &SettingsPalette,
        id: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connected_server = self.rpc_available(cx);
        let provider = self.cloud.provider(id).cloned();
        let busy = self.cloud.busy.contains(id);
        let waiting = matches!(
            &self.cloud.connect,
            Some((open, ConnectForm::Waiting { .. })) if open == id
        );
        let form_open = matches!(&self.cloud.connect, Some((open, _)) if open == id);
        let (dot, state) = match provider.as_ref().map(|provider| &provider.state) {
            _ if !connected_server => (ListItemStatus::Warning, "Server not reachable".to_string()),
            None => (ListItemStatus::Neutral, "Checking…".to_string()),
            _ if waiting => (
                ListItemStatus::Neutral,
                "Waiting for you to finish in the browser…".to_string(),
            ),
            Some(CloudProviderConnectionState::Connected) => {
                let provider = provider.as_ref().expect("matched Some");
                let mut state = plan_text(provider).unwrap_or_else(|| "Connected".to_string());
                if !provider.available {
                    if let Some(reason) = &provider.unavailable_reason {
                        state = format!("{state} · {reason}");
                    }
                }
                (
                    if provider.available {
                        ListItemStatus::Success
                    } else {
                        ListItemStatus::Warning
                    },
                    state,
                )
            }
            Some(CloudProviderConnectionState::Expired) => (
                ListItemStatus::Warning,
                "Sign-in expired: connect again".to_string(),
            ),
            Some(CloudProviderConnectionState::Error) => (
                ListItemStatus::Warning,
                provider
                    .as_ref()
                    .and_then(|provider| provider.error.clone())
                    .unwrap_or_else(|| "Connection failed".to_string()),
            ),
            Some(_) => (ListItemStatus::Neutral, "Not connected".to_string()),
        };
        let connected = provider
            .as_ref()
            .is_some_and(|provider| provider.state == CloudProviderConnectionState::Connected);
        let methods: Vec<CloudConnectMethod> = provider
            .as_ref()
            .map(|provider| provider.connect_methods.clone())
            .unwrap_or_default();
        let mut controls: Vec<AnyElement> = Vec::new();
        if connected_server && provider.is_some() && id != "local" && !form_open {
            if connected {
                controls.push(settings_button(
                    p,
                    SharedString::from(format!("cloud-boxes-disconnect-{id}")),
                    "Disconnect",
                    None,
                    ButtonVariant::Ghost,
                    busy,
                    Some("Disconnecting…".into()),
                    move |page: &mut Self, _window, cx| page.disconnect_provider(id, cx),
                    cx,
                ));
            } else {
                if methods.contains(&CloudConnectMethod::Key) {
                    controls.push(settings_button(
                        p,
                        SharedString::from(format!("cloud-boxes-paste-key-{id}")),
                        "Paste Key",
                        Some(ICON_KEY),
                        ButtonVariant::Ghost,
                        busy,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.open_connect_form(id, ConnectForm::Key, cx)
                        },
                        cx,
                    ));
                }
                if methods.contains(&CloudConnectMethod::Browser) {
                    controls.push(settings_button(
                        p,
                        SharedString::from(format!("cloud-boxes-connect-{id}")),
                        "Connect…",
                        Some(ICON_PLUG),
                        ButtonVariant::Outline,
                        busy,
                        Some("Starting the sign-in…".into()),
                        move |page: &mut Self, _window, cx| {
                            if id == "boat" {
                                page.open_connect_form(id, ConnectForm::Email, cx);
                            } else {
                                page.connect_provider(id, json!({ "method": "browser" }), cx);
                            }
                        },
                        cx,
                    ));
                }
            }
        }
        info_row(
            p,
            &format!("cloud-provider-{id}"),
            Some(dot),
            if id == "local" { ICON_BOX } else { ICON_CLOUD },
            label,
            provider_tooltip(id),
            Some(state),
            controls,
        )
    }

    fn open_connect_form(&mut self, id: &str, form: ConnectForm, cx: &mut Context<Self>) {
        self.cloud.connect = Some((id.to_string(), form));
        self.cloud.connect_error = None;
        cx.notify();
    }

    fn close_connect_form(&mut self, cx: &mut Context<Self>) {
        self.cloud.connect = None;
        self.cloud.connect_error = None;
        self.cloud.key_input = None;
        self.cloud.email_input = None;
        cx.notify();
    }

    /// `connect` with `extra` (`method`, `key` or `email`); a waiting browser sign-in keeps the
    /// form open with its code, and the page polls `connectStatus`.
    fn connect_provider(&mut self, id: &'static str, extra: Value, cx: &mut Context<Self>) {
        let mut params = json!({ "provider": id });
        if let (Some(params), Value::Object(extra)) = (params.as_object_mut(), extra) {
            params.extend(extra);
        }
        self.cloud.busy.insert(id.to_string());
        self.cloud.connect_error = None;
        cx.notify();
        self.cloud_rpc(
            "connect",
            params,
            CONNECT_TIMEOUT,
            move |page, result, cx| {
                page.cloud.busy.remove(id);
                page.apply_connect_result(id, result, cx);
            },
            cx,
        );
    }

    fn apply_connect_result(
        &mut self,
        id: &'static str,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        let answer = result.and_then(|value| {
            serde_json::from_value::<ConnectResult>(value)
                .map_err(|error| format!("Unexpected answer: {error}"))
        });
        match answer {
            Ok(answer) if answer.state == CloudProviderConnectionState::Connected => {
                self.close_connect_form(cx);
                self.toast("success", &format!("{} connected", label_of(id)), "", cx);
                self.load_cloud_providers(cx);
                self.load_cloud_spend(true, cx);
            }
            Ok(answer) if answer.state == CloudProviderConnectionState::Connecting => {
                if let Some(connect_id) = answer.connect_id {
                    self.cloud.connect = Some((
                        id.to_string(),
                        ConnectForm::Waiting {
                            connect_id,
                            auth_url: answer.auth_url,
                            user_code: answer.user_code,
                        },
                    ));
                }
            }
            Ok(answer) => {
                self.cloud.connect_error = Some(
                    answer
                        .error
                        .unwrap_or_else(|| "The sign-in didn't finish.".to_string()),
                );
                if matches!(
                    &self.cloud.connect,
                    Some((open, ConnectForm::Waiting { .. })) if open == id
                ) {
                    self.cloud.connect = Some((id.to_string(), ConnectForm::Key));
                }
            }
            Err(error) => {
                if self.cloud.connect.is_none() {
                    self.toast(
                        "error",
                        &format!("Couldn't connect {}", label_of(id)),
                        &error,
                        cx,
                    );
                } else {
                    self.cloud.connect_error = Some(error);
                }
            }
        }
    }

    /// One `connectStatus` read for a waiting browser sign-in (the page's poll calls it).
    pub(super) fn poll_connect(&mut self, cx: &mut Context<Self>) {
        let Some((id, ConnectForm::Waiting { connect_id, .. })) = self.cloud.connect.clone() else {
            return;
        };
        let Some(id) = LISTED
            .iter()
            .map(|(known, _)| *known)
            .find(|known| *known == id)
        else {
            return;
        };
        if !self.cloud.loading.insert("connectStatus") {
            return;
        }
        self.cloud_rpc(
            "connectStatus",
            json!({ "provider": id, "connectId": connect_id }),
            CONNECT_TIMEOUT,
            move |page, result, cx| {
                page.cloud.loading.remove("connectStatus");
                // The user may have cancelled while the read was out.
                if matches!(&page.cloud.connect, Some((open, ConnectForm::Waiting { .. })) if open == id)
                {
                    page.apply_connect_result(id, result, cx);
                }
            },
            cx,
        );
    }

    fn disconnect_provider(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.cloud.busy.insert(id.to_string());
        cx.notify();
        self.cloud_rpc(
            "disconnect",
            json!({ "provider": id }),
            CONNECT_TIMEOUT,
            move |page, result, cx| {
                page.cloud.busy.remove(id);
                match result.and_then(|value| {
                    serde_json::from_value::<ProvidersResult>(value)
                        .map_err(|error| format!("Unexpected answer: {error}"))
                }) {
                    Ok(answer) => page.cloud.providers = Some(answer.providers),
                    Err(error) => page.toast(
                        "error",
                        &format!("Couldn't disconnect {}", label_of(id)),
                        &error,
                        cx,
                    ),
                }
            },
            cx,
        );
    }

    /// The form under a provider row: Paste Key, Boat's email, or the waiting browser sign-in.
    fn connect_form(
        &mut self,
        p: &SettingsPalette,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (open, form) = self.cloud.connect.clone()?;
        if open != id {
            return None;
        }
        let busy = self.cloud.busy.contains(id);
        let cancel = settings_button(
            p,
            SharedString::from(format!("cloud-boxes-connect-cancel-{id}")),
            "Cancel",
            None,
            ButtonVariant::Ghost,
            false,
            None,
            |page: &mut Self, _window, cx| page.close_connect_form(cx),
            cx,
        );
        let mut body = v_flex().w_full().gap(px(10.0));
        let mut buttons: Vec<AnyElement> = vec![cancel];
        let muted = |text: String| {
            div()
                .min_w_0()
                .whitespace_normal()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(gpui::Hsla::from(p.muted))
                .child(text)
        };
        match form {
            ConnectForm::Key => {
                let input = self
                    .cloud
                    .key_input
                    .get_or_insert_with(|| {
                        cx.new(|cx| {
                            InputState::new(window, cx)
                                .masked(true)
                                .placeholder(if id == "boat" { "boat_…" } else { "ibk_…" })
                        })
                    })
                    .clone();
                body = body
                    .child(muted(format!(
                        "Paste an API key from your {} account settings.",
                        label_of(id)
                    )))
                    .child(settings_text_input(p, &input, None, true, window, cx));
                buttons.push(settings_button(
                    p,
                    SharedString::from(format!("cloud-boxes-save-key-{id}")),
                    "Save Key",
                    Some(ICON_KEY),
                    ButtonVariant::Outline,
                    busy,
                    Some("Checking the key…".into()),
                    move |page: &mut Self, _window, cx| {
                        let key = page
                            .cloud
                            .key_input
                            .as_ref()
                            .map(|input| input.read(cx).value().trim().to_string())
                            .unwrap_or_default();
                        if key.is_empty() {
                            page.cloud.connect_error = Some("Paste a key first.".to_string());
                            cx.notify();
                            return;
                        }
                        page.connect_provider(id, json!({ "method": "key", "key": key }), cx);
                    },
                    cx,
                ));
            }
            ConnectForm::Email => {
                let input = self
                    .cloud
                    .email_input
                    .get_or_insert_with(|| {
                        cx.new(|cx| InputState::new(window, cx).placeholder("you@example.com"))
                    })
                    .clone();
                body = body
                    .child(muted(
                        "Your Boat account's email. Boat's page opens next and asks for a code Ghostex shows here."
                            .to_string(),
                    ))
                    .child(settings_text_input(p, &input, None, false, window, cx));
                buttons.push(settings_button(
                    p,
                    "cloud-boxes-boat-paste-key",
                    "Paste a Key Instead",
                    None,
                    ButtonVariant::Ghost,
                    false,
                    None,
                    |page: &mut Self, _window, cx| {
                        page.open_connect_form("boat", ConnectForm::Key, cx)
                    },
                    cx,
                ));
                buttons.push(settings_button(
                    p,
                    "cloud-boxes-boat-continue",
                    "Continue",
                    Some(ICON_EXTERNAL),
                    ButtonVariant::Outline,
                    busy,
                    Some("Starting the sign-in…".into()),
                    move |page: &mut Self, _window, cx| {
                        let email = page
                            .cloud
                            .email_input
                            .as_ref()
                            .map(|input| input.read(cx).value().trim().to_string())
                            .unwrap_or_default();
                        if !email.contains('@') {
                            page.cloud.connect_error =
                                Some("Type the email of your Boat account.".to_string());
                            cx.notify();
                            return;
                        }
                        page.connect_provider(
                            id,
                            json!({ "method": "browser", "email": email }),
                            cx,
                        );
                    },
                    cx,
                ));
            }
            ConnectForm::Waiting {
                auth_url,
                user_code,
                ..
            } => {
                body = body.child(muted(match &user_code {
                    Some(_) => format!(
                        "Type this code on {}'s page in your browser. This page updates by itself.",
                        label_of(id)
                    ),
                    None => "Finish signing in in your browser. This page updates by itself."
                        .to_string(),
                }));
                if let Some(code) = user_code {
                    body = body.child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(gpui::Hsla::from(p.foreground))
                            .child(code),
                    );
                }
                if let Some(url) = auth_url {
                    buttons.push(settings_button(
                        p,
                        SharedString::from(format!("cloud-boxes-open-sign-in-{id}")),
                        "Open the Page Again",
                        Some(ICON_EXTERNAL),
                        ButtonVariant::Outline,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| page.open_url(&url, cx),
                        cx,
                    ));
                }
            }
        }
        let error = self.cloud.connect_error.clone();
        Some(
            div()
                .w_full()
                .px(px(20.0))
                .py(px(14.0))
                .child(
                    body.child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .whitespace_normal()
                                    .text_size(px(13.0))
                                    .text_color(gpui::Hsla::from(p.destructive))
                                    .children(error),
                            )
                            .child(
                                h_flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap(px(8.0))
                                    .children(buttons),
                            ),
                    ),
                )
                .into_any_element(),
        )
    }
}

fn label_of(id: &str) -> &'static str {
    match id {
        "instabox" => "Instabox",
        "boat" => "Boat",
        _ => "Local (Docker)",
    }
}
