//! Settings > Sync > Export and import: saves the settings to a JSON file (native save dialog) and
//! applies one from another computer (native open dialog, then a confirm that says how many
//! settings change, with the list behind Show, and Cancel as its default). What the file carries
//! and what an import skips are gx-core's rules (CDXC:SettingsSync 2026-11-14 in
//! packages/gx-core/src/settings_transfer.rs); the import saves through the Settings window's own
//! save path, so it applies live like any change here.
use super::super::super::fields::{
    ButtonVariant, RowSpec, SettingsDialogSpec, dialog_footer, setting_row, settings_button,
    settings_dialog, settings_section, static_note,
};
use super::super::super::palette::SettingsPalette;
use super::SyncTab;
use ghostex_gx_core::settings_transfer::{
    ExportSource, ImportPlan, describe_transfer_value, export_file_name, export_settings_document,
    plan_settings_import,
};
use gpui::{
    AnyElement, Context, Div, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::v_flex;
use serde_json::Value;

/// The app version the file names, as the About page shows it.
const APP_VERSION: &str = env!("GHOSTEX_BUILD_MARKETING_VERSION");

/// A file read and planned, waiting on the confirm.
pub(super) struct PendingImport {
    plan: ImportPlan,
    file_name: String,
    show_list: bool,
}

pub(super) struct TransferState {
    /// The last export's or import's outcome under the rows: (is an error, text).
    note: Option<(bool, String)>,
    pending: Option<PendingImport>,
    focus: FocusHandle,
    preview_applied: bool,
}

impl TransferState {
    pub(super) fn new(focus: FocusHandle) -> Self {
        Self {
            note: None,
            pending: None,
            focus,
            preview_applied: false,
        }
    }
}

fn downloads_folder() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join("Downloads"))
        .filter(|path| path.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

fn count_settings(count: usize) -> String {
    if count == 1 {
        "1 setting".to_string()
    } else {
        format!("{count} settings")
    }
}

impl SyncTab {
    /// This computer's name as Settings > Sync lists it, else its host name.
    fn this_computer_name(&self) -> String {
        let listed = self.status.as_ref().and_then(|status| {
            status
                .get("computers")
                .and_then(Value::as_array)?
                .iter()
                .find(|computer| {
                    computer.get("thisComputer").and_then(Value::as_bool) == Some(true)
                })
                .and_then(|computer| computer.get("name").and_then(Value::as_str))
                .map(str::to_string)
        });
        listed
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(crate::host_name::local_computer_name)
    }

    fn export_settings(&mut self, cx: &mut Context<Self>) {
        let computer = self.this_computer_name();
        let now = chrono::Local::now();
        let exported_at = now
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let document = export_settings_document(
            self.store.read(cx).settings(),
            &ExportSource {
                ghostex_version: APP_VERSION,
                computer_name: &computer,
                exported_at: &exported_at,
            },
        );
        let count = document["settings"]
            .as_object()
            .map_or(0, |values| values.len());
        let suggested = export_file_name(&computer, &now.format("%Y-%m-%d").to_string());
        let chosen = cx.prompt_for_new_path(&downloads_folder(), Some(&suggested));
        cx.spawn(async move |page, cx| {
            let Ok(Ok(Some(target))) = chosen.await else {
                return;
            };
            let written = serde_json::to_string_pretty(&document)
                .map_err(|error| error.to_string())
                .and_then(|text| {
                    std::fs::write(&target, format!("{text}\n")).map_err(|error| error.to_string())
                });
            let _ = page.update(cx, |page, cx| {
                page.transfer.note = Some(match written {
                    Ok(()) => (
                        false,
                        format!("Saved {} to {}.", count_settings(count), target.display()),
                    ),
                    Err(error) => (true, format!("Couldn't save the file: {error}")),
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn import_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn_in(window, async move |page, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let text = std::fs::read_to_string(&path);
            let _ = page.update_in(cx, |page, window, cx| {
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match text {
                    Ok(text) => {
                        page.open_import(&text, file_name, cx);
                        if page.transfer.pending.is_some() {
                            page.transfer.focus.focus(window, cx);
                        }
                    }
                    Err(error) => {
                        page.transfer.note =
                            Some((true, format!("Couldn't read {file_name}: {error}")));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Plans `text` against this computer's settings: a confirm when something changes, else a
    /// note under the rows.
    fn open_import(&mut self, text: &str, file_name: String, cx: &mut Context<Self>) {
        match plan_settings_import(text, self.store.read(cx).settings()) {
            Err(error) => self.transfer.note = Some((true, error.message().to_string())),
            Ok(plan) if plan.changes.is_empty() => {
                let mut note =
                    "Nothing to change: this computer already has these settings.".to_string();
                for skipped in plan.skipped_notes() {
                    note.push(' ');
                    note.push_str(&skipped);
                }
                self.transfer.note = Some((false, note));
            }
            Ok(plan) => {
                self.transfer.note = None;
                self.transfer.pending = Some(PendingImport {
                    plan,
                    file_name,
                    show_list: false,
                });
            }
        }
    }

    fn close_import(&mut self, cx: &mut Context<Self>) {
        self.transfer.pending = None;
        cx.notify();
    }

    fn apply_import(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.transfer.pending.take() else {
            return;
        };
        let patch = pending.plan.patch();
        let count = patch.len();
        self.store.update(cx, |store, cx| {
            store.apply_patch(patch, "settings:bulk", cx)
        });
        self.transfer.note = Some((
            false,
            format!(
                "Imported {} from {}.",
                count_settings(count),
                pending.file_name
            ),
        ));
        cx.notify();
    }

    /// `sync-import-confirm` (and `-list`, with the list shown) in the preview binary: the confirm
    /// for a file that changes a few of this computer's settings.
    pub(super) fn apply_transfer_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.transfer.preview_applied {
            return;
        }
        self.transfer.preview_applied = true;
        let state = self.store.read(cx).request().preview_state.clone();
        let Some(state) = state.filter(|state| state.starts_with("sync-import-confirm")) else {
            return;
        };
        let mut settings = self.store.read(cx).settings().clone();
        for (key, value) in [
            ("sidebarTheme", serde_json::json!("plain-light")),
            ("terminalFontSize", serde_json::json!(15)),
            (
                "copySound",
                serde_json::json!(!self.store.read(cx).values().bool("copySound")),
            ),
            ("sessionChatTheme", serde_json::json!("github")),
        ] {
            settings.insert(key.to_string(), value);
        }
        let mut document = export_settings_document(
            &settings,
            &ExportSource {
                ghostex_version: APP_VERSION,
                computer_name: "Studio",
                exported_at: "2026-11-14T10:00:00Z",
            },
        );
        document["settings"]["aSettingFromANewerGhostex"] = serde_json::json!(true);
        self.open_import(
            &document.to_string(),
            "ghostex-settings-studio-2026-11-14.json".to_string(),
            cx,
        );
        if let Some(pending) = self.transfer.pending.as_mut() {
            pending.show_list = state.ends_with("-list");
            self.transfer.focus.focus(window, cx);
        }
    }

    pub(super) fn transfer_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let export = settings_button(
            p,
            "settings-export",
            "Export…",
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| page.export_settings(cx),
            cx,
        );
        let import = settings_button(
            p,
            "settings-import",
            "Import…",
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, window, cx| page.import_settings(window, cx),
            cx,
        );
        let mut rows = vec![
            setting_row(
                p,
                "settingsExport",
                RowSpec::new("Export settings").description(
                    "Save your settings to a file to bring them to another computer. Secrets, sign-ins and settings that belong to this computer stay out.",
                ),
                None,
                export,
                cx,
            ),
            setting_row(
                p,
                "settingsImport",
                RowSpec::new("Import settings").description(
                    "Apply a file exported on another computer. You see what changes before anything is saved.",
                ),
                None,
                import,
                cx,
            ),
        ];
        if let Some((error, note)) = self.transfer.note.clone() {
            rows.push(static_note(p, note, error));
        }
        settings_section(p, "Export and import", None, None, rows)
    }

    /// The Import settings? confirm: how many settings change and from which file, what is
    /// skipped, and the list of changes behind Show.
    pub(super) fn render_import_dialog(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pending = self.transfer.pending.as_ref()?;
        let p = &p.nested_dialog();
        let count = pending.plan.changes.len();
        let from = pending
            .plan
            .from_computer
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(|name| format!(" from {name}"))
            .unwrap_or_default();
        let description = format!(
            "{} will change to the values in {}{from}. Your other settings stay as they are.",
            count_settings(count),
            pending.file_name
        );
        let mut body: Vec<AnyElement> = Vec::new();
        for skipped in pending.plan.skipped_notes() {
            body.push(
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(gpui::Hsla::from(p.muted))
                    .child(skipped)
                    .into_any_element(),
            );
        }
        let show_list = pending.show_list;
        body.push(
            div()
                .flex()
                .child(settings_button(
                    p,
                    "settings-import-show",
                    if show_list {
                        "Hide the list".to_string()
                    } else {
                        format!("Show the {}", count_settings(count))
                    },
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    |page: &mut Self, _window, cx| {
                        if let Some(pending) = page.transfer.pending.as_mut() {
                            pending.show_list = !pending.show_list;
                        }
                        cx.notify();
                    },
                    cx,
                ))
                .into_any_element(),
        );
        if show_list {
            let rows = pending.plan.changes.iter().map(|change| {
                v_flex()
                    .w_full()
                    .py(px(4.0))
                    .child(
                        div()
                            .text_color(gpui::Hsla::from(p.foreground))
                            .child(change.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(gpui::Hsla::from(p.muted))
                            .child(format!(
                                "{} → {}",
                                describe_transfer_value(&change.key, change.from.as_ref()),
                                describe_transfer_value(&change.key, Some(&change.to))
                            )),
                    )
            });
            body.push(
                div()
                    .id("settings-import-list")
                    .w_full()
                    .max_h(px(240.0))
                    .overflow_y_scroll()
                    .children(rows)
                    .into_any_element(),
            );
        }
        let cancel = settings_button(
            p,
            "settings-import-cancel",
            "Cancel",
            None,
            ButtonVariant::Primary,
            false,
            None,
            |page: &mut Self, _window, cx| page.close_import(cx),
            cx,
        );
        let confirm = settings_button(
            p,
            "settings-import-confirm",
            SharedString::from(format!("Import {}", count_settings(count))),
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| page.apply_import(cx),
            cx,
        );
        let focus = self.transfer.focus.clone();
        Some(settings_dialog(
            p,
            SettingsDialogSpec::new("settings-import-dialog", "Import settings?")
                .width(440.0)
                .spacing(20.0, 16.0)
                .enter_cancels()
                .description(description.into_any_element()),
            body,
            Some(dialog_footer(vec![confirm, cancel])),
            Some(&focus),
            |page: &mut Self, _window, cx| page.close_import(cx),
            window,
            cx,
        ))
    }
}
