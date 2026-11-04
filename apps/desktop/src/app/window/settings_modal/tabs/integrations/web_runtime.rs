//! The Chromium runtime (CEF) row of the Integrations page: status, Install / Reinstall /
//! Uninstall, and the undo while a change waits for the next launch.
//!
//! CDXC:CefRuntime 2026-10-29 DECISION:
//! User: "A small row in Settings > Integrations". The web runtime left the Extensions store, so
//! its management lives here as one compact row beside the Tools, in the Embedded browser section (tool_sections.rs); it installs from the first view
//! that needs it too (CDXC:CefRuntime 2026-09-28 in app/helpers/web_runtime.rs). The status comes
//! from plugins_modal.rs (`pluginSettingsStatus`); the actions are its `reinstallPlugin`,
//! `uninstallPlugin` and `cancelPluginChange` handlers (delayed_send/app_modal_sidebar_command/
//! settings_tools.rs).
use super::super::super::fields::{
    ListItemStatus, SizedButtonSize, SizedButtonVariant, settings_icon, settings_list_item,
    settings_sized_button, settings_square_button,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::post_store_message;
use super::{ICON_DEVICE_DESKTOP, ICON_REFRESH, ICON_TRASH, IntegrationsTab};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};

const TITLE: &str = "Chromium runtime (CEF)";
pub(super) const DESCRIPTION: &str = "The optional web runtime for the Browser, the VS Code view, website and extension views, and HTML files in Files. It also installs from the first view that needs it.";

/// The row's detail line: the status label, the version, a change waiting for the next launch, and
/// the last error.
fn detail(runtime: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(label) = runtime["statusLabel"]
        .as_str()
        .filter(|label| !label.is_empty())
    {
        parts.push(label.to_string());
    }
    if let Some(version) = runtime["version"]
        .as_str()
        .filter(|version| !version.is_empty())
    {
        parts.push(format!("v{version}"));
    }
    match runtime["pendingChange"].as_str() {
        Some("uninstall") => parts.push("Removed when Ghostex next starts".to_string()),
        Some("reinstall") => parts.push("Reinstalled when Ghostex next starts".to_string()),
        _ => {}
    }
    if let Some(error) = runtime["errorMessage"]
        .as_str()
        .filter(|error| !error.is_empty())
    {
        parts.push(error.to_string());
    }
    parts.join(" · ")
}

impl IntegrationsTab {
    /// The component status is read when Integrations opens without one.
    pub(super) fn request_web_runtime_status_if_missing(&mut self, cx: &mut Context<Self>) {
        if self
            .store
            .read(cx)
            .host_payload("pluginSettingsStatus")
            .is_none()
        {
            self.post_web_runtime_message(json!({ "type": "requestPluginSettingsStatus" }), cx);
        }
    }

    fn post_web_runtime_message(&mut self, message: Value, cx: &mut Context<Self>) {
        post_store_message(&self.store, message, cx);
    }

    /// The web runtime lives on this computer, so another machine's page has no row for it.
    pub(super) fn web_runtime_status_row(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let runtime = self
            .store
            .read(cx)
            .host_payload("pluginSettingsStatus")
            .and_then(|status| status["plugins"].as_array())
            .and_then(|plugins| plugins.iter().find(|plugin| plugin["id"] == "cef"))
            .cloned();
        match runtime {
            None => settings_list_item(
                p,
                None,
                Some(settings_icon(ICON_DEVICE_DESKTOP, 17.0, p.muted).into_any_element()),
                TITLE,
                Some(
                    div()
                        .child("Checking what is installed…")
                        .into_any_element(),
                ),
                None,
            ),
            Some(runtime) => self.web_runtime_row(p, &runtime, cx),
        }
    }

    fn web_runtime_row(
        &mut self,
        p: &SettingsPalette,
        runtime: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = runtime["status"].as_str();
        let busy =
            status.is_some_and(|status| !matches!(status, "installed" | "notInstalled" | "failed"));
        let available = runtime["canReinstall"].as_bool() == Some(true);
        let unavailable: SharedString = if busy {
            format!("{TITLE} is being installed.").into()
        } else {
            "This build does not provide a reinstallable remote component.".into()
        };
        let mut controls: Vec<AnyElement> = Vec::new();
        // A change waiting for the next launch offers only its undo (app/helpers/web_runtime_pending.rs).
        if let Some(change) = runtime["pendingChange"].as_str() {
            controls.push(settings_sized_button(
                p,
                "integrations-web-runtime-cancel-change",
                if change == "reinstall" {
                    "Cancel reinstall"
                } else {
                    "Keep installed"
                },
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Xs,
                false,
                None,
                |page: &mut Self, _window, cx| {
                    page.post_web_runtime_message(
                        json!({ "pluginId": "cef", "type": "cancelPluginChange" }),
                        cx,
                    )
                },
                cx,
            ));
        } else if status == Some("notInstalled") {
            controls.push(settings_sized_button(
                p,
                "integrations-web-runtime-install",
                "Install",
                Some("modals/settings/download.svg"),
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Xs,
                busy || !available,
                Some(unavailable),
                |page: &mut Self, _window, cx| {
                    page.post_web_runtime_message(
                        json!({ "pluginId": "cef", "type": "reinstallPlugin" }),
                        cx,
                    )
                },
                cx,
            ));
        } else {
            controls.push(settings_square_button(
                p,
                "integrations-web-runtime-reinstall",
                ICON_REFRESH,
                None,
                SizedButtonVariant::Ghost,
                32.0,
                Some(format!("Reinstall {TITLE}").into()),
                busy || !available,
                Some(unavailable),
                |page: &mut Self, _window, cx| {
                    page.post_web_runtime_message(
                        json!({ "pluginId": "cef", "type": "reinstallPlugin" }),
                        cx,
                    )
                },
                cx,
            ));
            if runtime["canUninstall"].as_bool() == Some(true) {
                controls.push(settings_square_button(
                    p,
                    "integrations-web-runtime-uninstall",
                    ICON_TRASH,
                    None,
                    SizedButtonVariant::Ghost,
                    32.0,
                    Some(format!("Uninstall {TITLE}").into()),
                    busy,
                    Some(format!("{TITLE} is being installed.").into()),
                    |page: &mut Self, _window, cx| {
                        page.post_web_runtime_message(
                            json!({ "pluginId": "cef", "type": "uninstallPlugin" }),
                            cx,
                        )
                    },
                    cx,
                ));
            }
        }
        let list_status = match status {
            Some("installed") => ListItemStatus::Success,
            Some("failed") => ListItemStatus::Warning,
            _ => ListItemStatus::Neutral,
        };
        settings_list_item(
            p,
            Some(list_status),
            Some(settings_icon(ICON_DEVICE_DESKTOP, 17.0, p.muted).into_any_element()),
            TITLE,
            Some(div().child(detail(runtime)).into_any_element()),
            Some(
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(controls)
                    .into_any_element(),
            ),
        )
    }
}
