//! `grokWebLogins`: the Grok extension's logins as rows with a name box and Remove, and Add login
//! (Settings > Files & Links > Browser, while Grok is on).
//!
//! CDXC:Extensions 2026-11-09 DECISION:
//! The bar above the Grok page switches and adds logins (apps/desktop/src/app/claude_web/grok.rs); naming them needs a text box, so renaming and removing are here. An empty setting is one login named Grok (id `default`), shown as that row; editing it saves it. The last login cannot be removed. Removing one only drops it from the list: the Grok page clears its browser profile the next time it opens, as this window cannot reach the browser engine.
use super::super::palette::SettingsPalette;
use super::controls::{ButtonVariant, settings_button, settings_icon_button};
use super::row::{PageAction, RowSpec, setting_row};
use super::text::settings_text_input;
use super::{FieldStates, SettingsPage, icon};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

const KEY: &str = "grokWebLogins";
const DEFAULT_LOGIN_ID: &str = "default";
const DEFAULT_LOGIN_NAME: &str = "Grok";

/// `(id, name)` of each login, as the Grok page reads them: unique ids, and the one default login
/// when the setting lists none.
fn read_logins(value: &Value) -> Vec<(String, String)> {
    let mut logins: Vec<(String, String)> = Vec::new();
    for entry in value.as_array().into_iter().flatten() {
        let Some(id) = entry["id"]
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        if logins.iter().any(|(other, _)| other == id) {
            continue;
        }
        let name = entry["name"].as_str().unwrap_or("").to_string();
        logins.push((id.to_string(), name));
    }
    if logins.is_empty() {
        logins.push((DEFAULT_LOGIN_ID.into(), DEFAULT_LOGIN_NAME.into()));
    }
    logins
}

fn save_logins<V: SettingsPage>(page: &mut V, logins: &[(String, String)], cx: &mut Context<V>) {
    let value = Value::Array(
        logins
            .iter()
            .map(|(id, name)| json!({ "id": id, "name": name }))
            .collect(),
    );
    let store = page.settings_store().clone();
    store.update(cx, |store, cx| {
        if store.value(KEY) != value {
            store.update_setting(KEY, value, cx);
        }
    });
}

fn current_logins<V: SettingsPage>(page: &V, cx: &Context<V>) -> Vec<(String, String)> {
    read_logins(&page.settings_store().read(cx).value(KEY))
}

fn add_login<V: SettingsPage>(page: &mut V, cx: &mut Context<V>) {
    let mut logins = current_logins(page, cx);
    let name = (logins.len() + 1..)
        .map(|number| format!("{DEFAULT_LOGIN_NAME} {number}"))
        .find(|name| logins.iter().all(|(_, other)| other.trim() != name))
        .unwrap_or_default();
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    logins.push((id, name));
    save_logins(page, &logins, cx);
}

/// The Grok logins field (a wide row).
pub(crate) fn grok_logins_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    value: &Value,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let logins = read_logins(value);
    let only_one = logins.len() == 1;
    let mut rows: Vec<AnyElement> = Vec::new();
    for (id, name) in &logins {
        let input = FieldStates::text_state(
            page,
            &SharedString::from(format!("{KEY}-name-{id}")),
            name,
            Some("Login name"),
            {
                let id = id.clone();
                move |page: &mut V, text, _window, cx| {
                    let mut logins = current_logins(page, cx);
                    if let Some(login) = logins.iter_mut().find(|(other, _)| *other == id) {
                        login.1 = text;
                    }
                    save_logins(page, &logins, cx);
                }
            },
            window,
            cx,
        );
        let remove_id = id.clone();
        rows.push(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .child(settings_text_input(p, &input, None, false, window, cx))
                .child(settings_icon_button(
                    p,
                    SharedString::from(format!("{KEY}-remove-{id}")),
                    icon::TRASH,
                    14.0,
                    32.0,
                    ButtonVariant::Ghost,
                    Some(if only_one {
                        "The last login cannot be removed.".into()
                    } else {
                        "Remove this login and sign it out".into()
                    }),
                    only_one,
                    move |page: &mut V, _window, cx| {
                        let logins: Vec<(String, String)> = current_logins(page, cx)
                            .into_iter()
                            .filter(|(other, _)| *other != remove_id)
                            .collect();
                        if !logins.is_empty() {
                            save_logins(page, &logins, cx);
                        }
                    },
                    cx,
                ))
                .into_any_element(),
        );
    }
    let control = v_flex()
        .w_full()
        .gap(px(8.0))
        .children(rows)
        .child(div().child(settings_button(
            p,
            "grok-web-login-add",
            "Add login",
            Some(icon::PLUS),
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut V, _window, cx| add_login(page, cx),
            cx,
        )))
        .into_any_element();
    setting_row(p, KEY, spec.wide(), on_reset, control, cx)
}
