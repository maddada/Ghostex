//! Settings > Extensions > Install from folder…: the owner picks a folder on this computer, sees
//! the extension's name and what it asks for, ticks the permissions to give it, and gxserver
//! installs it with that consent (`/api/installExtension` `{ localPath, approval }`). A permission
//! left unticked is not granted (server/src/extensions/store.rs `check_install_approval`).
//!
//! CDXC:Extensions 2026-11-08 DECISION:
//! User (#495 follow-up, Q1892): "yes" to an Install from folder… button in Settings > Extensions, owner only: it opens a folder picker, shows the extension's name and the permissions it asks for, and installs with the consent the owner ticks. Installing is the owner's devices' alone (server/src/server/security_owner_only.rs), so this is the only way a folder extension gets in; agents and the CLI are refused.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    SizedButtonSize, SizedButtonVariant, checkbox_control, settings_icon, settings_sized_button,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_gxserver_rpc;
use super::ExtensionsTab;
use super::data::{
    CatalogEntry, InstalledExtension, capitalize, permission_description, replace_installed,
};
use super::sections::error_banner;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

const MANIFEST_FILE: &str = "ghostex-extension.json";
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

/// The folder waiting for the owner's consent.
pub(crate) struct FolderInstall {
    pub(crate) path: String,
    pub(crate) entry: CatalogEntry,
    /// The permissions the owner ticked; nothing is ticked at first.
    pub(crate) ticked: BTreeSet<String>,
    pub(crate) installing: bool,
    pub(crate) error: Option<String>,
}

/// The manifest of the extension in `folder`, or why it can't be installed.
fn read_folder_manifest(folder: &Path) -> Result<Value, String> {
    let file = folder.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&file).map_err(|_| {
        format!(
            "{} has no {MANIFEST_FILE}, so it isn't a Ghostex extension.",
            folder.display()
        )
    })?;
    let manifest: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{MANIFEST_FILE} in that folder can't be read: {error}"))?;
    let named = |key: &str| {
        manifest
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty())
    };
    if !named("name") || !named("version") {
        return Err(format!(
            "{MANIFEST_FILE} in that folder has no name or version."
        ));
    }
    Ok(manifest)
}

impl ExtensionsTab {
    /// The Install from folder… button of the Store section's header.
    pub(crate) fn install_from_folder_button(
        &self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let unavailable = !self.has_transport(cx);
        settings_sized_button(
            p,
            "extensions-install-from-folder",
            "Install from folder…",
            Some("modals/settings/folder-open.svg"),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Default,
            unavailable,
            unavailable.then(|| "Ghostex's server isn't reachable.".into()),
            |page: &mut Self, _window, cx| page.pick_extension_folder(cx),
            cx,
        )
    }

    /// Opens the system folder picker and reads the extension in the folder picked.
    pub(crate) fn pick_extension_folder(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        cx.spawn(async move |page, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(folder) = paths.into_iter().next() else {
                return;
            };
            let _ = page.update(cx, |page, cx| match read_folder_manifest(&folder) {
                Ok(manifest) => {
                    page.show_folder_install(folder.to_string_lossy().into_owned(), manifest, cx)
                }
                Err(error) => {
                    page.browser.error = Some(error);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Shows the consent for the extension in `path`, with nothing ticked.
    pub(crate) fn show_folder_install(
        &mut self,
        path: String,
        manifest: Value,
        cx: &mut Context<Self>,
    ) {
        self.browser.folder_install = Some(FolderInstall {
            path,
            entry: CatalogEntry { raw: manifest },
            ticked: BTreeSet::new(),
            installing: false,
            error: None,
        });
        cx.notify();
    }

    fn close_folder_install(&mut self, cx: &mut Context<Self>) {
        if self
            .browser
            .folder_install
            .as_ref()
            .is_some_and(|install| !install.installing)
        {
            self.browser.folder_install = None;
            cx.notify();
        }
    }

    /// Installs the folder's extension with the permissions ticked.
    fn install_folder_extension(&mut self, cx: &mut Context<Self>) {
        let Some(install) = self.browser.folder_install.as_mut() else {
            return;
        };
        install.installing = true;
        install.error = None;
        let entry = &install.entry;
        let params = json!({
            "localPath": install.path,
            "approval": {
                "version": entry.version(),
                "permissions": install.ticked.iter().collect::<Vec<_>>(),
                "backgroundProcess": entry.runs_background_process(),
                "remoteUrl": entry.remote_url(),
            },
        });
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store,
            "/api/installExtension",
            params,
            INSTALL_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    match result {
                        Ok(value) if value["extension"].is_object() => {
                            let extension = InstalledExtension {
                                raw: value["extension"].clone(),
                            };
                            page.browser.icons.remove(&extension.id());
                            replace_installed(&mut page.browser.installed, extension);
                            page.browser.folder_install = None;
                        }
                        Ok(_) => {
                            if let Some(install) = page.browser.folder_install.as_mut() {
                                install.installing = false;
                                install.error =
                                    Some("Ghostex didn't say the extension was installed.".into());
                            }
                        }
                        Err(error) => {
                            if let Some(install) = page.browser.folder_install.as_mut() {
                                install.installing = false;
                                install.error = Some(error);
                            }
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// The consent dialog for a folder's extension.
    pub(crate) fn render_folder_install(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let install = self.browser.folder_install.as_ref()?;
        let entry = install.entry.clone();
        let path = install.path.clone();
        let ticked = install.ticked.clone();
        let installing = install.installing;
        let error = install.error.clone();
        let m = p.modal;
        let title = {
            let title = entry.title();
            if title.trim().is_empty() {
                entry.name()
            } else {
                title
            }
        };
        let notice = |glyph: &'static str, heading: String, text: String| -> AnyElement {
            h_flex()
                .w_full()
                .items_start()
                .gap(px(12.0))
                .p(px(12.0))
                .rounded(px(MODAL_RADIUS_SECTION))
                .border_1()
                .border_color(hsla(m.hairline))
                .bg(hsla(m.panel))
                .child(
                    div()
                        .mt(px(2.0))
                        .flex_shrink_0()
                        .child(settings_icon(glyph, 20.0, m.muted)),
                )
                .child(
                    v_flex()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(20.0))
                                .text_color(hsla(m.foreground))
                                .child(heading),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .text_color(hsla(m.muted))
                                .child(text),
                        ),
                )
                .into_any_element()
        };
        let mut items: Vec<AnyElement> = Vec::new();
        items.push(
            div()
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(hsla(m.muted))
                .child(format!("{} v{} · {path}", entry.name(), entry.version()))
                .into_any_element(),
        );
        let permissions = entry.permissions();
        if permissions.is_empty() {
            items.push(
                div()
                    .text_size(px(13.0))
                    .text_color(hsla(m.muted))
                    .child("It asks for no permissions.")
                    .into_any_element(),
            );
        }
        for permission in permissions {
            let checked = ticked.contains(&permission);
            let label = v_flex()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(m.foreground))
                        .child(capitalize(&permission)),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(20.0))
                        .text_color(hsla(m.muted))
                        .child(permission_description(&permission).to_string()),
                )
                .into_any_element();
            let key = permission.clone();
            items.push(
                div()
                    .w_full()
                    .p(px(12.0))
                    .rounded(px(MODAL_RADIUS_SECTION))
                    .border_1()
                    .border_color(hsla(m.hairline))
                    .bg(hsla(m.panel))
                    .child(checkbox_control(
                        p,
                        gpui::SharedString::from(format!(
                            "extension-folder-permission-{permission}"
                        )),
                        checked,
                        Some(label),
                        12.0,
                        move |page: &mut Self, checked, _window, cx| {
                            if let Some(install) = page.browser.folder_install.as_mut() {
                                if checked {
                                    install.ticked.insert(key.clone());
                                } else {
                                    install.ticked.remove(&key);
                                }
                                cx.notify();
                            }
                        },
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        if entry.runs_background_process() {
            items.push(notice(
                super::super::super::fields::icon::ALERT_TRIANGLE,
                "Runs a background process".into(),
                "It runs a program on this computer, outside any sandbox, while it is on.".into(),
            ));
        }
        if let Some(url) = entry.remote_url() {
            items.push(notice(
                super::super::super::fields::icon::ALERT_TRIANGLE,
                "Loads a remote website".into(),
                format!("It opens {url}, which sees whatever you type into it."),
            ));
        }
        if let Some(error) = error {
            items.push(error_banner(p, error));
        }
        let cancel = modal_action_button(
            &m,
            "extension-folder-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            installing,
            |page: &mut Self, _window, cx| page.close_folder_install(cx),
            cx,
        );
        let confirm = modal_action_button(
            &m,
            "extension-folder-install",
            if installing {
                "Installing…"
            } else {
                "Install"
            },
            None,
            ModalButtonTone::Neutral,
            installing,
            |page: &mut Self, _window, cx| page.install_folder_extension(cx),
            cx,
        );
        let sheet = v_flex()
            .id("extension-folder-consent")
            .occlude()
            .w(px(520.0))
            .max_w(window.viewport_size().width - px(32.0))
            .p(px(MODAL_WINDOW_PADDING))
            .gap(px(MODAL_SECTION_GAP))
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(m.hairline))
            .bg(hsla(p.dialog_surface()))
            .shadow_xl()
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(m.foreground))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(modal_header(
                &m,
                format!("Install {title} from a folder?"),
                Some("It isn't from the Ghostex store, so nobody has reviewed it. Tick only what you trust it with; it gets nothing you leave unticked."),
            ))
            .child(v_flex().w_full().gap(px(8.0)).children(items))
            .child(modal_footer(vec![cancel, confirm]))
            .into_any_element();
        Some(super::super::super::fields::settings_dialog_layer(
            "extension-folder-consent",
            sheet,
            0.0,
            |page: &mut Self, _window, cx| page.close_folder_install(cx),
            window,
            cx,
        ))
    }
}
