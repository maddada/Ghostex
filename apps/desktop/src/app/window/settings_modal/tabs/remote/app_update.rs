//! A saved machine's Ghostex version, its Update button and the "Keep this machine up to date" switch, drawn from the row the app sends (`remoteAppUpdateState`, apps/desktop/src/app/remote_conn/app_update.rs).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, icon, settings_button_sized, settings_icon_button,
    small_switch_control,
};
use super::RemoteTab;
use super::style::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

impl RemoteTab {
    /// `remoteAppUpdateState` from the app: one machine's row.
    pub(super) fn receive_app_update_state(&mut self, message: &Value, cx: &mut Context<Self>) {
        let Some(machine_id) = message
            .get("remoteMachineId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            return;
        };
        let view = message.get("view").cloned().unwrap_or(Value::Null);
        if view.is_null() {
            self.app_updates.remove(machine_id);
        } else {
            self.app_updates.insert(machine_id.to_string(), view);
        }
        cx.notify();
    }

    /// The page became visible: every connected machine is read again if its answer is old.
    pub(super) fn request_app_update_rows(&self, cx: &mut App) {
        self.post(json!({ "type": "remoteAppUpdate", "op": "refreshAll" }), cx);
    }

    fn post_app_update(&self, machine_id: &str, op: &str, enabled: Option<bool>, cx: &mut App) {
        let mut message =
            json!({ "type": "remoteAppUpdate", "op": op, "remoteMachineId": machine_id });
        if let Some(enabled) = enabled {
            message["enabled"] = json!(enabled);
        }
        self.post(message, cx);
    }
}

fn view_text<'a>(view: &'a Value, key: &str) -> Option<&'a str> {
    view.get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

/// The tile's second line: the version, then what is happening or available.
pub(super) fn tile_update_line(tab: &RemoteTab, machine_id: &str) -> Option<String> {
    let view = tab.app_updates.get(machine_id)?;
    // A background service older than the app is said beside the version, where the old number would otherwise pass for the computer's own.
    let version = match (
        view_text(view, "versionText"),
        view_text(view, "serviceVersionText"),
    ) {
        (Some(version), Some(service)) => Some(format!("{version} · {service}")),
        (version, _) => version.map(str::to_string),
    };
    let version = version.as_deref();
    let state = if view.get("updateAvailable").and_then(Value::as_bool) == Some(true)
        || view.get("busy").and_then(Value::as_bool) == Some(true)
    {
        view_text(view, "detail")
    } else {
        None
    };
    match (version, state) {
        (Some(version), Some(state)) => Some(format!("{version} · {state}")),
        (Some(version), None) => Some(version.to_string()),
        (None, Some(state)) => Some(state.to_string()),
        (None, None) => None,
    }
}

/// The tile's Update button while an update is available, or a spinner while one runs.
pub(super) fn tile_update_button(
    tab: &RemoteTab,
    t: &RemoteTokens,
    machine_id: &str,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    let view = tab.app_updates.get(machine_id)?;
    if view.get("busy").and_then(Value::as_bool) == Some(true) {
        return Some(spinner(
            SharedString::from(format!("remote-machine-update-spin-{machine_id}")),
            14.0,
            t.muted,
            900,
        ));
    }
    if view.get("updateAvailable").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let label = view
        .get("action")
        .and_then(|action| view_text(action, "label"))
        .unwrap_or("Update")
        .to_string();
    let id = machine_id.to_string();
    Some(settings_icon_button(
        &t.p,
        SharedString::from(format!("remote-machine-update-{machine_id}")),
        icon::DOWNLOAD,
        14.0,
        24.0,
        ButtonVariant::Ghost,
        Some(label.into()),
        false,
        move |tab: &mut RemoteTab, _window, cx| {
            cx.stop_propagation();
            tab.post_app_update(&id, "install", None, cx);
        },
        cx,
    ))
}

/// The machine dialog's Ghostex row: the version and state, the action, and the switch.
pub(super) fn dialog_update_section(
    tab: &RemoteTab,
    t: &RemoteTokens,
    machine_id: &str,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    let view = tab.app_updates.get(machine_id)?.clone();
    let p = t.p;
    let tone = match view_text(&view, "tone") {
        Some("warning") => t.warn,
        Some("success") => t.success,
        _ if view_text(&view, "serviceNote").is_some() => t.warn,
        _ => t.muted,
    };
    let busy = view.get("busy").and_then(Value::as_bool) == Some(true);
    let text = [
        view_text(&view, "versionText"),
        view_text(&view, "serviceNote"),
        view_text(&view, "detail"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let action = view
        .get("action")
        .filter(|action| !action.is_null())
        .map(|action| {
            let label = view_text(action, "label").unwrap_or("Update").to_string();
            let op = view_text(action, "op").unwrap_or("refresh").to_string();
            let enabled = action.get("enabled").and_then(Value::as_bool) != Some(false);
            let id = machine_id.to_string();
            settings_button_sized(
                &p,
                "remote-machine-app-update",
                label,
                Some(if op == "install" {
                    icon::DOWNLOAD
                } else {
                    ICON_REFRESH
                }),
                ButtonVariant::Secondary,
                ButtonSize::Default,
                !enabled,
                None,
                move |tab: &mut RemoteTab, _window, cx| tab.post_app_update(&id, &op, None, cx),
                cx,
            )
        });
    let auto = view
        .get("autoUpdate")
        .and_then(Value::as_bool)
        .map(|checked| {
            let id = machine_id.to_string();
            small_switch_control(
                &p,
                "remote-machine-auto-update",
                "Keep this machine up to date",
                checked,
                false,
                None,
                move |tab: &mut RemoteTab, enabled, _window, cx| {
                    tab.post_app_update(&id, "setAuto", Some(enabled), cx)
                },
                cx,
            )
        });
    Some(
        v_flex()
            .w_full()
            .mt(px(8.0))
            .gap(px(8.0))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(px(8.0))
                    .when(busy, |row| {
                        row.child(spinner("remote-machine-app-update-spin", 14.0, t.muted, 900))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.0))
                            .line_height(px(18.2))
                            .text_color(hsla(tone))
                            .child(text),
                    )
                    .children(action),
            )
            .children(auto.map(|switch| {
                v_flex()
                    .gap(px(2.0))
                    .child(switch)
                    .child(detail(
                        t,
                        "Installs Ghostex updates on that computer by itself while none of its agents is working. Sessions keep running through the restart.",
                        true,
                    ))
                    .into_any_element()
            }))
            .into_any_element(),
    )
}
