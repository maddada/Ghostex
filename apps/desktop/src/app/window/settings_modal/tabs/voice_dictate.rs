//! Settings › Dictation's Speech to text section: the engine (Hex, OpenAI, xAI) with the key a
//! cloud engine needs right under it, the language, the hotkey and its mode, the microphone and
//! Insert with spacing.
//!
//! The engine, models and language belong to the computer whose
//! gxserver transcribes, so the page reads and writes them through `/api/readDictateSettings` and
//! `/api/setDictateSettings` on the machine the picker at the top shows (this computer, or a
//! connected remote one: `store_machine_gxserver_rpc`). The hotkey mode, the microphone and Insert
//! with spacing are this app's own settings.
//!
//! CDXC:VoiceMode 2026-11-05 DECISION: User (Q1257, Q1269 and the settings mockup): Dictate and the
//! voice agent each pick their own engine, shown as two separate sections of one Voice page under
//! App; the engine picker sits first under Dictate with the hotkey, microphone and clean-up after
//! it. Q1272: nothing switches engine behind the user's back. Since 2026-11-08 the engines are Hex,
//! OpenAI and xAI (CDXC:Dictate in server/src/voice_agent/speech/retired.rs) and the page is
//! Settings › Dictation (CDXC:VoiceMode 2026-11-08 in packages/settings-catalog/src/pages/dictation.rs). Q1275: settings are per
//! computer, so the page follows the machine picker like Agents and Integrations.
//! SEE-ALSO: server/src/voice_agent/speech/http.rs (the routes), voice_agent.rs (the page),
//! apps/desktop/src/app/voice_agent/dictate_hotkey.rs, microphones.rs.
use super::super::super::native_modal_kit::*;
use super::super::catalog::SettingOption;
use super::super::fields::{
    ButtonVariant, RowSpec, format_hotkey_label, setting_row, settings_button, settings_segmented,
    settings_select, toggle_field_with,
};
use super::super::model::SettingsTabId;
use super::super::palette::SettingsPalette;
use super::super::store::{SettingsStore, post_store_message, store_machine_gxserver_rpc};
use super::voice_agent::VoiceAgentTab;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Task, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

mod hex;
pub(crate) mod hex_install_plan;
mod notice;

const READ_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) const ENGINE: &str = "dictateEngine";
pub(super) const OPENAI_KEY: &str = "dictateOpenaiKey";
pub(super) const XAI_KEY: &str = "dictateXaiKey";
pub(super) const LANGUAGE: &str = "dictateLanguage";
pub(super) const HOTKEY: &str = "dictateHotkey";
pub(super) const HOTKEY_MODE: &str = "dictateHotkeyMode";
pub(super) const MICROPHONE: &str = "dictateMicrophone";
pub(super) const SPACING: &str = "dictateInsertSpacing";

/// The hotkey Dictate ships with, and the one offered when macOS takes it.
const DEFAULT_CHORD: &str = "ctrl+alt+space";
const MAC_FALLBACK_CHORD: &str = "ctrl+alt+d";
const DICTATE_HOTKEY_ID: &str = "toggleDictation";

/// What the page knows about the shown computer's Dictate.
pub(crate) struct DictateState {
    /// `/api/readDictateSettings`' answer, `None` until it arrived.
    answer: Option<Value>,
    /// The machine `answer` is from (`None`: this computer).
    machine: Option<String>,
    read_error: Option<String>,
    /// The last change's failure, shown under the section.
    action_error: Option<String>,
    devices_requested: bool,
    /// Install Hex is running on the shown computer (`hex.rs`).
    hex_install: Option<Task<()>>,
    /// The Install Hex confirm dialog is open (this computer's install, run by the app).
    hex_install_confirm: bool,
    hex_confirm_focus: gpui::FocusHandle,
    /// This computer's install as the app reports it (`hexInstallState`).
    hex_install_run: Option<hex::HexInstallRun>,
    /// Open Hex was pressed and the row is checking again until Hex runs.
    hex_opening: Option<Task<()>>,
}

impl DictateState {
    pub(crate) fn new(cx: &mut Context<VoiceAgentTab>) -> Self {
        Self {
            answer: None,
            machine: None,
            read_error: None,
            action_error: None,
            devices_requested: false,
            hex_install: None,
            hex_install_confirm: false,
            hex_confirm_focus: cx.focus_handle(),
            hex_install_run: None,
            hex_opening: None,
        }
    }
}

impl DictateState {
    /// The machine shown (`None`: this computer).
    pub(crate) fn machine(&self) -> Option<&str> {
        self.machine.as_deref()
    }
}

impl VoiceAgentTab {
    fn picked_machine(&self, cx: &gpui::App) -> Option<String> {
        self.store.read(cx).picked_machine().map(str::to_string)
    }

    /// Reads the shown computer's Dictate settings (again when the picker moved).
    pub(super) fn read_dictate(&mut self, cx: &mut Context<Self>) {
        let machine = self.picked_machine(cx);
        let page = cx.entity().downgrade();
        let asked = machine.clone();
        store_machine_gxserver_rpc(
            &self.store,
            machine.as_deref(),
            "/api/readDictateSettings",
            json!({}),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    if page.picked_machine(cx) != asked {
                        return;
                    }
                    match result {
                        Ok(answer) => page.take_dictate_answer(answer, cx),
                        Err(error) => page.dictate.read_error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
        self.dictate.machine = machine;
        self.dictate.answer = None;
        self.dictate.read_error = None;
        if !self.dictate.devices_requested {
            self.dictate.devices_requested = true;
            post_store_message(&self.store, json!({ "type": "requestDictateDevices" }), cx);
        }
    }

    /// The page follows the machine picker.
    pub(super) fn sync_dictate_machine(&mut self, cx: &mut Context<Self>) {
        if self.picked_machine(cx) != self.dictate.machine {
            self.read_dictate(cx);
        }
    }

    fn take_dictate_answer(&mut self, answer: Value, cx: &mut Context<Self>) {
        let _ = cx;
        self.dictate.read_error = None;
        self.dictate.answer = Some(answer);
    }

    /// Saves part of the settings (`/api/setDictateSettings`), which answers the whole object.
    fn set_dictate(&mut self, patch: Value, cx: &mut Context<Self>) {
        // Shown at once; the answer brings gxserver's own copy.
        if let (Some(answer), Some(fields)) = (self.dictate.answer.as_mut(), patch.as_object()) {
            for (key, value) in fields {
                answer["settings"][key] = value.clone();
            }
        }
        self.dictate.action_error = None;
        let machine = self.picked_machine(cx);
        let page = cx.entity().downgrade();
        store_machine_gxserver_rpc(
            &self.store,
            machine.as_deref(),
            "/api/setDictateSettings",
            json!({ "patch": patch }),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    match result {
                        Ok(answer) => page.take_dictate_answer(answer, cx),
                        Err(error) => {
                            page.dictate.action_error = Some(error);
                            page.read_dictate(cx);
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    /// `/api/readDictateSettings` again without clearing what the page shows.
    fn refresh_dictate_quietly(&mut self, cx: &mut Context<Self>) {
        let machine = self.picked_machine(cx);
        let page = cx.entity().downgrade();
        let asked = machine.clone();
        store_machine_gxserver_rpc(
            &self.store,
            machine.as_deref(),
            "/api/readDictateSettings",
            json!({}),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    if page.picked_machine(cx) == asked
                        && let Ok(answer) = result
                    {
                        page.take_dictate_answer(answer, cx);
                        cx.notify();
                    }
                });
            },
            cx,
        );
    }

    fn dictate_settings(&self) -> Option<&Value> {
        self.dictate
            .answer
            .as_ref()
            .map(|answer| &answer["settings"])
    }

    /// The Dictate section's rows, in the mockup's order.
    pub(super) fn dictate_rows(
        &mut self,
        p: &SettingsPalette,
        visible: &dyn Fn(&str) -> bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        let ready = self.dictate.answer.is_some();
        if !ready {
            let (text, failed) = match &self.dictate.read_error {
                Some(error) => (format!("Could not read Dictate's settings: {error}"), true),
                None => ("Reading Dictate's settings…".to_string(), false),
            };
            if visible(ENGINE) {
                rows.push(
                    div()
                        .px(px(16.0))
                        .py(px(12.0))
                        .text_size(px(12.5))
                        .text_color(hsla(if failed { p.destructive } else { p.muted }))
                        .child(text)
                        .into_any_element(),
                );
            }
        } else {
            if visible(ENGINE) {
                rows.push(self.engine_row(p, cx));
                if let Some(row) = self.hex_row(p, window, cx) {
                    rows.push(row);
                }
            }
            if let Some(row) = self.engine_key_row(p, visible, window, cx) {
                rows.push(row);
            }
            if visible(ENGINE)
                && let Some(row) = self.remote_key_missing_row(p, cx)
            {
                rows.push(row);
            }
            if visible(LANGUAGE)
                && let Some(row) = self.language_row(p, window, cx)
            {
                rows.push(row);
            }
        }
        if visible(HOTKEY) {
            rows.push(self.hotkey_row(p, cx));
        }
        if visible(HOTKEY_MODE) {
            rows.push(self.hotkey_mode_row(p, cx));
        }
        if visible(MICROPHONE) {
            rows.push(self.microphone_row(p, window, cx));
        }
        if visible(SPACING) {
            rows.push(self.spacing_row(p, cx));
        }
        if let Some(error) = &self.dictate.action_error {
            rows.push(
                div()
                    .px(px(16.0))
                    .py(px(10.0))
                    .text_size(px(12.5))
                    .text_color(hsla(p.destructive))
                    .child(error.clone())
                    .into_any_element(),
            );
        }
        rows
    }

    /// Insert with spacing, this app's own setting.
    fn spacing_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let (title, subtitle, _) = Self::catalog_row(SPACING).unwrap_or_default();
        let on = self.store.read(cx).values().bool(SPACING);
        toggle_field_with(
            p,
            SPACING,
            RowSpec::new(title).description(subtitle),
            on,
            None,
            |page: &mut Self, on, _window, cx| {
                page.store
                    .update(cx, |store, cx| store.update_setting(SPACING, json!(on), cx));
            },
            cx,
        )
    }

    fn engine_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let (title, subtitle, _) = Self::catalog_row(ENGINE).unwrap_or_default();
        let answer = self.dictate.answer.clone().unwrap_or_default();
        let engine = answer["settings"]["engine"]
            .as_str()
            .unwrap_or("hex")
            .to_string();
        // Hex first, as the recommended engine, where Hex 3 ships for the shown computer
        // (CDXC:Dictate 2026-11-07); a computer already set to it keeps the choice in view.
        let hex = (Self::hex_offered(&answer) || engine == "hex").then_some(("Hex", "hex"));
        let options = hex
            .into_iter()
            .chain([("OpenAI", "openai"), ("xAI", "xai")])
            .map(|(label, value)| SettingOption {
                label: label.to_string(),
                value: value.to_string(),
            })
            .collect::<Vec<_>>();
        let control = settings_segmented(
            p,
            "dictate-engine",
            &options,
            Some(engine.as_str()),
            |page: &mut Self, value, _window, cx| page.set_dictate(json!({ "engine": value }), cx),
            cx,
        );
        // What will transcribe, or what is missing (no key, no model yet).
        let status = match answer["engine"]["problem"]["message"].as_str() {
            // The Hex row under this one says what is missing.
            Some(_) if engine == "hex" => String::new(),
            Some(message) => match answer["engine"]["problem"]["code"].as_str() {
                // The key row right under this one takes it.
                Some("noKey" | "keyUnreadable") if self.dictate.machine.is_none() => String::new(),
                Some("noKey" | "keyUnreadable") => {
                    format!("{message} Add it in Settings › Dictation on that computer.")
                }
                _ => message.to_string(),
            },
            None => match answer["engine"]["label"].as_str() {
                Some(label) => format!("Transcribes with {label}."),
                None => String::new(),
            },
        };
        let description = if status.is_empty() {
            subtitle
        } else {
            format!("{subtitle}\n{status}")
        };
        setting_row(
            p,
            ENGINE,
            RowSpec::new(title).description(description),
            None,
            control,
            cx,
        )
    }

    fn language_row(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (title, subtitle, _) = Self::catalog_row(LANGUAGE)?;
        let answer = self.dictate.answer.clone()?;
        let options: Vec<SettingOption> = answer["languages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|language| {
                Some(SettingOption {
                    label: language["label"].as_str()?.to_string(),
                    value: language["code"].as_str()?.to_string(),
                })
            })
            .collect();
        let value = answer["settings"]["language"]
            .as_str()
            .unwrap_or("auto")
            .to_string();
        let control = settings_select(
            self,
            p,
            "dictate-language",
            &options,
            &value,
            None,
            false,
            None,
            |page: &mut Self, value, _window, cx| {
                page.set_dictate(json!({ "language": value }), cx)
            },
            window,
            cx,
        );
        Some(setting_row(
            p,
            LANGUAGE,
            RowSpec::new(title).description(subtitle),
            None,
            control,
            cx,
        ))
    }

    /// The saved Dictate chord (`hotkeys.toggleDictation`), the default when none is saved.
    fn dictate_chord(&self, cx: &gpui::App) -> String {
        self.store
            .read(cx)
            .value("hotkeys")
            .get(DICTATE_HOTKEY_ID)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| DEFAULT_CHORD.to_string())
    }

    fn hotkey_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let (title, subtitle, _) = Self::catalog_row(HOTKEY).unwrap_or_default();
        let chord = self.dictate_chord(cx);
        let label = if chord.trim().is_empty() {
            "Not set".to_string()
        } else {
            format_hotkey_label(&chord)
        };
        let control = h_flex()
            .gap(px(8.0))
            .items_center()
            .child(
                div()
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.foreground_alpha(0.05)))
                    .text_size(px(13.0))
                    .text_color(hsla(p.foreground))
                    .child(label),
            )
            .child(settings_button(
                p,
                "dictate-hotkey-change",
                "Change…",
                None,
                ButtonVariant::Outline,
                false,
                None,
                |page: &mut Self, _window, cx| {
                    page.store.update(cx, |store, cx| {
                        store.set_active_tab(SettingsTabId::Hotkeys, cx)
                    });
                },
                cx,
            ))
            .into_any_element();
        let conflict = input_source_conflict(&self.store, cx)
            && chord.trim().eq_ignore_ascii_case(DEFAULT_CHORD);
        let description = if conflict {
            format!(
                "{subtitle}\nmacOS uses {} to switch input sources, so it never reaches Ghostex. Pick {} in Hotkeys, or turn the input source shortcut off in System Settings > Keyboard > Keyboard Shortcuts.",
                format_hotkey_label(DEFAULT_CHORD),
                format_hotkey_label(MAC_FALLBACK_CHORD)
            )
        } else {
            subtitle
        };
        setting_row(
            p,
            HOTKEY,
            RowSpec::new(title).description(description),
            None,
            control,
            cx,
        )
    }

    fn hotkey_mode_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let (title, subtitle, options) = Self::catalog_row(HOTKEY_MODE).unwrap_or_default();
        let values = self.store.read(cx).values();
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = values.choice(HOTKEY_MODE, &allowed);
        let control = settings_segmented(
            p,
            "dictate-hotkey-mode",
            &options,
            Some(value.as_str()),
            |page: &mut Self, value, _window, cx| {
                page.store.update(cx, |store, cx| {
                    store.update_setting(HOTKEY_MODE, json!(value), cx)
                });
            },
            cx,
        );
        setting_row(
            p,
            HOTKEY_MODE,
            RowSpec::new(title).description(subtitle),
            None,
            control,
            cx,
        )
    }

    fn microphone_row(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (title, subtitle, _) = Self::catalog_row(MICROPHONE).unwrap_or_default();
        let saved = self.store.read(cx).values().string(MICROPHONE);
        let devices = self.store.read(cx).host_payload("dictateDevices").cloned();
        let default_name = devices
            .as_ref()
            .and_then(|devices| devices["defaultMicrophone"].as_str())
            .map(str::to_string);
        let mut options = vec![SettingOption {
            label: match &default_name {
                Some(name) => format!("System default ({name})"),
                None => "System default".to_string(),
            },
            value: String::new(),
        }];
        let names: Vec<String> = devices
            .as_ref()
            .and_then(|devices| devices["microphones"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        for name in &names {
            options.push(SettingOption {
                label: name.clone(),
                value: name.clone(),
            });
        }
        if !saved.is_empty() && devices.is_some() && !names.contains(&saved) {
            options.push(SettingOption {
                label: format!("{saved} (not connected: the default is used)"),
                value: saved.clone(),
            });
        }
        let control = settings_select(
            self,
            p,
            "dictate-microphone",
            &options,
            &saved,
            None,
            false,
            None,
            |page: &mut Self, value, _window, cx| {
                page.store.update(cx, |store, cx| {
                    store.update_setting(MICROPHONE, json!(value), cx)
                });
            },
            window,
            cx,
        );
        let description = if self.dictate.machine.is_some() {
            format!("{subtitle} It is this computer's microphone, whichever computer transcribes.")
        } else {
            subtitle
        };
        setting_row(
            p,
            MICROPHONE,
            RowSpec::new(title).description(description),
            None,
            control,
            cx,
        )
    }
}

/// Whether the app found that macOS takes Ctrl+Alt+Space for input sources (`dictateDevices`).
pub(crate) fn input_source_conflict(store: &Entity<SettingsStore>, cx: &gpui::App) -> bool {
    store
        .read(cx)
        .host_payload("dictateDevices")
        .and_then(|devices| devices["inputSourceConflict"].as_bool())
        .unwrap_or(false)
}
