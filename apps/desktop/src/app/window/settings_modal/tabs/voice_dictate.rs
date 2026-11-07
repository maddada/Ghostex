//! Settings › Voice › Dictate and Clean-up: the engine (Local, OpenAI, xAI, Hex), the local models with
//! their downloads, the language, the hotkey and its mode, the microphone, the dictionary, filler
//! removal, Insert with spacing and AI polish.
//!
//! The engine, models, language, dictionary, filler removal and polish belong to the computer whose
//! gxserver transcribes, so the page reads and writes them through `/api/readDictateSettings` and
//! `/api/setDictateSettings` on the machine the picker at the top shows (this computer, or a
//! connected remote one: `store_machine_gxserver_rpc`). The hotkey mode, the microphone and Insert
//! with spacing are this app's own settings.
//!
//! CDXC:VoiceMode 2026-11-05 DECISION: User (Q1257, Q1269 and the settings mockup): Dictate and the
//! voice agent each pick their own engine, shown as two separate sections of one Voice page under
//! App; the engine picker sits first under Dictate with the models, hotkey, microphone and clean-up
//! after it. Q1270: models live in gxserver's data folder, with Show in Finder (Explorer, the file
//! manager). Q1272: nothing switches engine behind the user's back. Q1275: settings are per
//! computer, so the page follows the machine picker like Agents and Integrations.
//! SEE-ALSO: server/src/voice_agent/speech/http.rs (the routes), voice_agent.rs (the page),
//! apps/desktop/src/app/voice_agent/dictate_hotkey.rs, microphones.rs.
use super::super::super::native_modal_kit::*;
use super::super::catalog::SettingOption;
use super::super::fields::{
    ButtonVariant, RowSpec, format_hotkey_label, setting_row, settings_button, settings_segmented,
    settings_select, settings_text_input, settings_textarea, toggle_field_with,
};
use super::super::model::SettingsTabId;
use super::super::palette::SettingsPalette;
use super::super::store::{SettingsStore, post_store_message, store_machine_gxserver_rpc};
use super::voice_agent::VoiceAgentTab;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Task, Window,
    div, px,
};
use gpui_component::input::InputState;
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

mod hex;

const READ_TIMEOUT: Duration = Duration::from_secs(15);
/// The contract's pace for download progress.
const POLL: Duration = Duration::from_millis(500);

pub(super) const ENGINE: &str = "dictateEngine";
pub(super) const MODELS: &str = "dictateModels";
pub(super) const LANGUAGE: &str = "dictateLanguage";
pub(super) const HOTKEY: &str = "dictateHotkey";
pub(super) const HOTKEY_MODE: &str = "dictateHotkeyMode";
pub(super) const MICROPHONE: &str = "dictateMicrophone";
pub(super) const DICTIONARY: &str = "dictateDictionary";
pub(super) const FILLERS: &str = "dictateRemoveFillers";
pub(super) const SPACING: &str = "dictateInsertSpacing";
pub(super) const POLISH: &str = "dictatePolish";

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
    /// Model ids with a request in flight.
    model_busy: Vec<String>,
    poll: Option<Task<()>>,
    word_input: Entity<InputState>,
    alias_input: Entity<InputState>,
    dictionary_note: Option<String>,
    /// The polish prompt editor is open, with the text typed so far.
    prompt_draft: Option<String>,
    devices_requested: bool,
    /// Install Hex is running on the shown computer (`hex.rs`).
    hex_install: Option<Task<()>>,
}

impl DictateState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<VoiceAgentTab>) -> Self {
        Self {
            answer: None,
            machine: None,
            read_error: None,
            action_error: None,
            model_busy: Vec::new(),
            poll: None,
            word_input: cx.new(|cx| InputState::new(window, cx).placeholder("Word, e.g. gxserver")),
            alias_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Heard as (optional), e.g. GX server")
            }),
            dictionary_note: None,
            prompt_draft: None,
            devices_requested: false,
            hex_install: None,
        }
    }
}

impl DictateState {
    /// The machine shown (`None`: this computer).
    pub(crate) fn machine(&self) -> Option<&str> {
        self.machine.as_deref()
    }
}

/// A local model's row, from the answer's `models`.
#[derive(Clone, Debug, PartialEq)]
struct Model {
    id: String,
    title: String,
    languages: String,
    speed: String,
    download_bytes: u64,
    disk_bytes: u64,
    state: String,
    received: u64,
    total: u64,
    error: Option<String>,
    in_use: bool,
    attribution: String,
}

fn models_of(answer: &Value) -> Vec<Model> {
    answer["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| {
            Some(Model {
                id: model["id"].as_str()?.to_string(),
                title: model["title"].as_str().unwrap_or_default().to_string(),
                languages: model["languagesLabel"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                speed: model["speedLabel"].as_str().unwrap_or_default().to_string(),
                download_bytes: model["downloadBytes"].as_u64().unwrap_or(0),
                disk_bytes: model["diskBytes"].as_u64().unwrap_or(0),
                state: model["state"]
                    .as_str()
                    .unwrap_or("notDownloaded")
                    .to_string(),
                received: model["download"]["receivedBytes"].as_u64().unwrap_or(0),
                total: model["download"]["totalBytes"].as_u64().unwrap_or(0),
                error: model["download"]["error"]
                    .as_str()
                    .filter(|error| !error.is_empty())
                    .map(str::to_string),
                in_use: model["inUse"].as_bool().unwrap_or(false),
                attribution: model["attribution"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect()
}

/// Whether any model is downloading or being prepared (the page polls while so).
fn any_moving(answer: &Value) -> bool {
    models_of(answer)
        .iter()
        .any(|model| matches!(model.state.as_str(), "downloading" | "preparing"))
}

/// "0.72 GB", "150 MB".
pub(super) fn size_label(bytes: u64) -> String {
    if bytes < 200_000_000 {
        return format!("{} MB", (bytes as f64 / 1_000_000.0).round() as u64);
    }
    let gb = format!("{:.2}", bytes as f64 / 1_000_000_000.0);
    let gb = gb.trim_end_matches('0').trim_end_matches('.');
    format!("{gb} GB")
}

/// The progress line of a download: "42% · 0.3 of 0.72 GB", or "Preparing…" after it.
fn progress_label(model: &Model) -> String {
    if model.state == "preparing" {
        return "Preparing…".to_string();
    }
    let total = if model.total > 0 {
        model.total
    } else {
        model.download_bytes
    };
    let percent = (model.received * 100)
        .checked_div(total)
        .unwrap_or(0)
        .min(100);
    format!(
        "{percent}% · {:.1} of {}",
        model.received as f64 / 1_000_000_000.0,
        size_label(total)
    )
}

/// What the "Show in Finder" button reads on this system.
fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Show in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in Explorer"
    } else {
        "Open folder"
    }
}

/// A dictionary entry from the add form: the word and its aliases (comma separated), checked
/// against gxserver's limits (word 1 to 64 characters; each alias 3 to 64, at most 8).
pub(super) fn dictionary_entry(word: &str, aliases: &str) -> Result<Value, String> {
    let word = word.trim();
    if word.is_empty() {
        return Err("Type a word first.".to_string());
    }
    if word.chars().count() > 64 {
        return Err("A word can be at most 64 characters.".to_string());
    }
    let aliases: Vec<String> = aliases
        .split(',')
        .map(str::trim)
        .filter(|alias| !alias.is_empty())
        .map(str::to_string)
        .collect();
    if aliases.len() > 8 {
        return Err("A word can have at most 8 aliases.".to_string());
    }
    if let Some(alias) = aliases
        .iter()
        .find(|alias| !(3..=64).contains(&alias.chars().count()))
    {
        return Err(format!(
            "\"{alias}\" is too short or too long: an alias is 3 to 64 characters."
        ));
    }
    Ok(json!({ "word": word, "aliases": aliases }))
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
        self.dictate.prompt_draft = None;
        if !self.dictate.devices_requested {
            self.dictate.devices_requested = true;
            post_store_message(&self.store, json!({ "type": "requestDictateDevices" }), cx);
        }
    }

    /// The page follows the machine picker.
    pub(super) fn sync_dictate_machine(&mut self, cx: &mut Context<Self>) {
        if self.picked_machine(cx) != self.dictate.machine {
            self.dictate.poll = None;
            self.read_dictate(cx);
        }
    }

    fn take_dictate_answer(&mut self, answer: Value, cx: &mut Context<Self>) {
        self.dictate.read_error = None;
        let moving = any_moving(&answer);
        self.dictate.answer = Some(answer);
        if moving {
            self.start_model_poll(cx);
        }
    }

    /// Saves part of the settings (`/api/setDictateSettings`), which answers the whole object.
    fn set_dictate(&mut self, patch: Value, cx: &mut Context<Self>) {
        // Shown at once; the answer brings gxserver's own copy.
        if let (Some(answer), Some(fields)) = (self.dictate.answer.as_mut(), patch.as_object()) {
            for (key, value) in fields {
                if key == "polish" {
                    if let (Some(polish), Some(changes)) = (
                        answer["settings"]["polish"].as_object_mut(),
                        value.as_object(),
                    ) {
                        for (key, value) in changes {
                            polish.insert(key.clone(), value.clone());
                        }
                    }
                } else {
                    answer["settings"][key] = value.clone();
                }
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

    /// Download, Cancel or Remove for model `id`.
    fn model_action(&mut self, path: &'static str, id: String, cx: &mut Context<Self>) {
        if self.dictate.model_busy.contains(&id) {
            return;
        }
        self.dictate.model_busy.push(id.clone());
        self.dictate.action_error = None;
        let machine = self.picked_machine(cx);
        let page = cx.entity().downgrade();
        let done = id.clone();
        store_machine_gxserver_rpc(
            &self.store,
            machine.as_deref(),
            path,
            json!({ "id": id }),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.dictate.model_busy.retain(|busy| *busy != done);
                    match result {
                        Ok(answer) => {
                            page.merge_model(&answer["model"]);
                            page.start_model_poll(cx);
                        }
                        Err(error) => page.dictate.action_error = Some(error),
                    }
                    // The engine's readiness changed with the model.
                    page.refresh_dictate_quietly(cx);
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    fn merge_model(&mut self, model: &Value) {
        let (Some(answer), Some(id)) = (self.dictate.answer.as_mut(), model["id"].as_str()) else {
            return;
        };
        if let Some(slot) = answer["models"]
            .as_array_mut()
            .and_then(|models| models.iter_mut().find(|slot| slot["id"] == id))
        {
            *slot = model.clone();
        }
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

    /// Polls `/api/listSpeechModels` every 500 ms while a model downloads or is prepared.
    fn start_model_poll(&mut self, cx: &mut Context<Self>) {
        if self.dictate.poll.is_some() {
            return;
        }
        self.dictate.poll = Some(cx.spawn(async move |page, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let asked = page.update(cx, |page, cx| {
                    let machine = page.picked_machine(cx);
                    let (sender, receiver) = futures::channel::oneshot::channel();
                    store_machine_gxserver_rpc(
                        &page.store,
                        machine.as_deref(),
                        "/api/listSpeechModels",
                        json!({}),
                        READ_TIMEOUT,
                        move |result, _| {
                            let _ = sender.send(result);
                        },
                        cx,
                    );
                    receiver
                });
                let Ok(receiver) = asked else {
                    break;
                };
                let Ok(result) = receiver.await else {
                    break;
                };
                let going = page.update(cx, |page, cx| {
                    let Ok(list) = result else {
                        // A dropped answer is tried again on the next tick.
                        return true;
                    };
                    if let Some(answer) = page.dictate.answer.as_mut() {
                        answer["models"] = list["models"].clone();
                        answer["modelsDiskBytes"] = list["modelsDiskBytes"].clone();
                    }
                    let moving = page.dictate.answer.as_ref().is_some_and(any_moving);
                    if !moving {
                        page.dictate.poll = None;
                        page.refresh_dictate_quietly(cx);
                    }
                    cx.notify();
                    moving
                });
                if !matches!(going, Ok(true)) {
                    break;
                }
            }
        }));
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
            if visible(MODELS) {
                rows.push(self.models_row(p, cx));
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

    /// The Clean-up section's rows.
    pub(super) fn cleanup_rows(
        &mut self,
        p: &SettingsPalette,
        visible: &dyn Fn(&str) -> bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        let ready = self.dictate.answer.is_some();
        if ready && visible(DICTIONARY) {
            rows.push(self.dictionary_row(p, window, cx));
        }
        if ready && visible(FILLERS) {
            let (title, subtitle, _) = Self::catalog_row(FILLERS).unwrap_or_default();
            let on = self
                .dictate_settings()
                .and_then(|settings| settings["removeFillers"].as_bool())
                .unwrap_or(true);
            rows.push(toggle_field_with(
                p,
                FILLERS,
                RowSpec::new(title).description(subtitle),
                on,
                None,
                |page: &mut Self, on, _window, cx| {
                    page.set_dictate(json!({ "removeFillers": on }), cx)
                },
                cx,
            ));
        }
        if visible(SPACING) {
            let (title, subtitle, _) = Self::catalog_row(SPACING).unwrap_or_default();
            let on = self.store.read(cx).values().bool(SPACING);
            rows.push(toggle_field_with(
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
            ));
        }
        if ready && visible(POLISH) {
            rows.extend(self.polish_rows(p, window, cx));
        }
        rows
    }

    fn engine_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let (title, subtitle, _) = Self::catalog_row(ENGINE).unwrap_or_default();
        let answer = self.dictate.answer.clone().unwrap_or_default();
        let engine = answer["settings"]["engine"]
            .as_str()
            .unwrap_or("local")
            .to_string();
        // Hex only where Hex 3 ships for the shown computer (CDXC:Dictate 2026-11-07).
        let hex = Self::hex_offered(&answer).then_some(("Hex", "hex"));
        let options = [
            ("Local model", "local"),
            ("OpenAI", "openai"),
            ("xAI", "xai"),
        ]
        .into_iter()
        .chain(hex)
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
            |page: &mut Self, value, _window, cx| page.pick_engine(value, cx),
            cx,
        );
        // What will transcribe, or what is missing (no key, no model yet).
        let status = match answer["engine"]["problem"]["message"].as_str() {
            // The Hex row under this one says what is missing.
            Some(_) if engine == "hex" => String::new(),
            Some(message) => match answer["engine"]["problem"]["code"].as_str() {
                Some("noKey" | "keyUnreadable") => {
                    format!("{message} Add it under API keys below.")
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

    /// Picking Local with no model downloaded also starts the picked model's download, right in
    /// the Engine row (the mockup's note); a cloud engine with no key says so in the row.
    fn pick_engine(&mut self, engine: String, cx: &mut Context<Self>) {
        let start_download = (engine == "local")
            .then(|| self.dictate.answer.as_ref())
            .flatten()
            .and_then(|answer| {
                let wanted = answer["settings"]["localModel"].as_str()?.to_string();
                let models = models_of(answer);
                let any_ready = models.iter().any(|model| model.state == "ready");
                let wanted_model = models.into_iter().find(|model| model.id == wanted)?;
                (!any_ready && wanted_model.state == "notDownloaded").then_some(wanted)
            });
        self.set_dictate(json!({ "engine": engine }), cx);
        if let Some(id) = start_download {
            self.model_action("/api/downloadSpeechModel", id, cx);
        }
    }

    fn models_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let (title, subtitle, _) = Self::catalog_row(MODELS).unwrap_or_default();
        let answer = self.dictate.answer.clone().unwrap_or_default();
        let models = models_of(&answer);
        let remote = self.dictate.machine.is_some();
        let mut list = v_flex().w_full().gap(px(8.0));
        for model in models {
            list = list.child(self.model_card(p, &model, cx));
        }
        let folder = answer["modelsFolder"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let disk = answer["modelsDiskBytes"].as_u64().unwrap_or(0);
        let place = if remote {
            "that computer"
        } else {
            "this computer"
        };
        let footer = h_flex()
            .w_full()
            .gap(px(8.0))
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.0))
                    .text_color(hsla(p.muted))
                    .child(if disk > 0 {
                        format!("Stored on {place} in {folder} ({} used)", size_label(disk))
                    } else {
                        format!("Stored on {place} in {folder}")
                    }),
            )
            .when(!remote && !folder.is_empty(), |footer| {
                footer.child(settings_button(
                    p,
                    "dictate-models-reveal",
                    reveal_label(),
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    |page: &mut Self, _window, cx| {
                        post_store_message(
                            &page.store,
                            json!({ "type": "revealDictateModelsFolder" }),
                            cx,
                        );
                    },
                    cx,
                ))
            });
        let control = v_flex()
            .w_full()
            .gap(px(10.0))
            .child(list)
            .child(footer)
            .into_any_element();
        setting_row(
            p,
            MODELS,
            RowSpec::new(title).description(subtitle).wide(),
            None,
            control,
            cx,
        )
    }

    fn model_card(
        &mut self,
        p: &SettingsPalette,
        model: &Model,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.dictate.model_busy.contains(&model.id);
        let id = model.id.clone();
        let size = if model.state == "ready" {
            size_label(model.disk_bytes)
        } else {
            size_label(model.download_bytes)
        };
        let details = [
            model.languages.as_str(),
            size.as_str(),
            model.speed.as_str(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
        let button = |page_id: String,
                      label: String,
                      variant: ButtonVariant,
                      action: &'static str,
                      cx: &mut Context<Self>| {
            let id = id.clone();
            settings_button(
                p,
                SharedString::from(format!("dictate-model-{page_id}")),
                label,
                None,
                variant,
                busy,
                None,
                move |page: &mut Self, _window, cx| match action {
                    "use" => page.set_dictate(json!({ "localModel": id.clone() }), cx),
                    path => page.model_action(path, id.clone(), cx),
                },
                cx,
            )
        };
        let mut actions = h_flex().gap(px(8.0)).items_center().flex_shrink_0();
        let mut progress: Option<AnyElement> = None;
        match model.state.as_str() {
            "downloading" | "preparing" => {
                let total = if model.total > 0 {
                    model.total
                } else {
                    model.download_bytes
                };
                let fraction = if model.state == "preparing" || total == 0 {
                    1.0
                } else {
                    (model.received as f32 / total as f32).clamp(0.0, 1.0)
                };
                progress = Some(
                    v_flex()
                        .w_full()
                        .gap(px(4.0))
                        .child(
                            div()
                                .w_full()
                                .h(px(4.0))
                                .rounded_full()
                                .bg(hsla(p.foreground_alpha(0.1)))
                                .child(
                                    div()
                                        .h_full()
                                        .rounded_full()
                                        .bg(hsla(p.primary))
                                        .w(gpui::relative(fraction)),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(hsla(p.muted))
                                .child(progress_label(model)),
                        )
                        .into_any_element(),
                );
                actions = actions.child(button(
                    format!("{}-cancel", model.id),
                    "Cancel".to_string(),
                    ButtonVariant::Outline,
                    "/api/cancelSpeechModelDownload",
                    cx,
                ));
            }
            "ready" => {
                if !model.in_use {
                    actions = actions.child(button(
                        format!("{}-use", model.id),
                        "Use".to_string(),
                        ButtonVariant::Outline,
                        "use",
                        cx,
                    ));
                }
                actions = actions.child(button(
                    format!("{}-remove", model.id),
                    "Remove".to_string(),
                    ButtonVariant::Outline,
                    "/api/deleteSpeechModel",
                    cx,
                ));
            }
            "failed" => {
                actions = actions.child(button(
                    format!("{}-retry", model.id),
                    "Download again".to_string(),
                    ButtonVariant::Outline,
                    "/api/downloadSpeechModel",
                    cx,
                ));
            }
            _ => {
                if !model.in_use {
                    actions = actions.child(button(
                        format!("{}-use", model.id),
                        "Use".to_string(),
                        ButtonVariant::Outline,
                        "use",
                        cx,
                    ));
                }
                actions = actions.child(button(
                    format!("{}-download", model.id),
                    format!("Download ({})", size_label(model.download_bytes)),
                    ButtonVariant::Outline,
                    "/api/downloadSpeechModel",
                    cx,
                ));
            }
        }
        let status = match model.state.as_str() {
            "ready" if model.in_use => Some("In use"),
            "ready" => Some("Downloaded"),
            _ if model.in_use => Some("Picked"),
            _ => None,
        };
        v_flex()
            .id(SharedString::from(format!(
                "dictate-model-card-{}",
                model.id
            )))
            .w_full()
            .gap(px(6.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(if model.in_use {
                p.foreground_alpha(0.22)
            } else {
                p.hairline
            }))
            .bg(hsla(p.foreground_alpha(if model.in_use {
                0.05
            } else {
                0.025
            })))
            .when(!model.attribution.is_empty(), |card| {
                let attribution = model.attribution.clone();
                card.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(attribution.clone()).build(window, cx)
                })
            })
            .child(
                h_flex()
                    .w_full()
                    .gap(px(10.0))
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(2.0))
                            .child(
                                h_flex()
                                    .gap(px(8.0))
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(13.5))
                                            .text_color(hsla(p.foreground))
                                            .child(model.title.clone()),
                                    )
                                    .when_some(status, |line, status| {
                                        line.child(
                                            div()
                                                .px(px(7.0))
                                                .py(px(1.0))
                                                .rounded_full()
                                                .text_size(px(11.0))
                                                .text_color(hsla(p.foreground))
                                                .bg(hsla(p.foreground_alpha(0.1)))
                                                .child(status),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(hsla(p.muted))
                                    .child(details),
                            ),
                    )
                    .child(actions),
            )
            .when_some(progress, |card, progress| card.child(progress))
            .when_some(model.error.clone(), |card, error| {
                card.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(hsla(p.destructive))
                        .child(error),
                )
            })
            .into_any_element()
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

    fn dictionary_row(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (title, subtitle, _) = Self::catalog_row(DICTIONARY).unwrap_or_default();
        let answer = self.dictate.answer.clone().unwrap_or_default();
        let words: Vec<Value> = answer["settings"]["dictionary"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let add_projects = answer["settings"]["addProjectNames"]
            .as_bool()
            .unwrap_or(true);
        let project_words: Vec<String> = answer["projectWords"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let chip = |text: String| {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(9.0))
                .py(px(3.0))
                .rounded_full()
                .text_size(px(12.5))
                .text_color(hsla(p.foreground))
                .child(text)
        };
        let mut chips = div().flex().flex_wrap().gap(px(6.0)).w_full();
        for (index, entry) in words.iter().enumerate() {
            let word = entry["word"].as_str().unwrap_or_default().to_string();
            let aliases: Vec<String> = entry["aliases"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|alias| format!("\"{alias}\""))
                .collect();
            let text = if aliases.is_empty() {
                word.clone()
            } else {
                format!("{word} ← {}", aliases.join(", "))
            };
            chips = chips.child(
                chip(text)
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.foreground_alpha(0.06)))
                    .child(
                        div()
                            .id(SharedString::from(format!("dictate-word-remove-{index}")))
                            .cursor_pointer()
                            .px(px(2.0))
                            .text_color(hsla(p.muted))
                            .hover(|style| style.text_color(hsla(p.foreground)))
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new("Remove").build(window, cx)
                            })
                            .child("✕")
                            .on_click(cx.listener(move |page, _, _, cx| {
                                page.remove_word(index, cx);
                            })),
                    ),
            );
        }
        if add_projects {
            for word in project_words.iter().take(24) {
                chips = chips.child(
                    chip(word.clone())
                        .border_1()
                        .border_dashed()
                        .border_color(hsla(p.foreground_alpha(0.25)))
                        .text_color(hsla(p.muted)),
                );
            }
        }
        let form = h_flex()
            .w_full()
            .gap(px(8.0))
            .items_center()
            .child(
                div()
                    .w(px(170.0))
                    .flex_shrink_0()
                    .child(settings_text_input(
                        p,
                        &self.dictate.word_input,
                        None,
                        false,
                        window,
                        cx,
                    )),
            )
            .child(settings_text_input(
                p,
                &self.dictate.alias_input,
                None,
                false,
                window,
                cx,
            ))
            .child(settings_button(
                p,
                "dictate-word-add",
                "Add word",
                None,
                ButtonVariant::Outline,
                false,
                None,
                |page: &mut Self, window, cx| page.add_word(window, cx),
                cx,
            ));
        let project_words_switch = super::super::fields::switch_control(
            p,
            "dictate-project-words-switch",
            "Add project names",
            add_projects,
            false,
            None,
            |page: &mut Self, on, _window, cx| {
                page.set_dictate(json!({ "addProjectNames": on }), cx)
            },
            cx,
        );
        let project_words_press = project_words_switch.press();
        let projects_switch = h_flex()
            .id("dictate-project-words-row")
            .gap(px(8.0))
            .items_center()
            .when_some(project_words_press, |row, press| {
                row.switch_row_press(switch_row_wash(p.foreground), false, press)
            })
            .child(project_words_switch)
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(hsla(p.muted))
                    .child(format!(
                        "Add project names ({}), drawn dashed",
                        project_words.len()
                    )),
            );
        let control = v_flex()
            .w_full()
            .gap(px(10.0))
            .when(
                !words.is_empty() || (add_projects && !project_words.is_empty()),
                |this| this.child(chips),
            )
            .child(form)
            .when_some(self.dictate.dictionary_note.clone(), |this, note| {
                this.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(hsla(p.destructive))
                        .child(note),
                )
            })
            .child(projects_switch)
            .into_any_element();
        setting_row(
            p,
            DICTIONARY,
            RowSpec::new(title).description(subtitle).wide(),
            None,
            control,
            cx,
        )
    }

    fn add_word(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let word = self.dictate.word_input.read(cx).value().to_string();
        let aliases = self.dictate.alias_input.read(cx).value().to_string();
        let entry = match dictionary_entry(&word, &aliases) {
            Ok(entry) => entry,
            Err(note) => {
                self.dictate.dictionary_note = Some(note);
                cx.notify();
                return;
            }
        };
        let mut words = self
            .dictate_settings()
            .and_then(|settings| settings["dictionary"].as_array().cloned())
            .unwrap_or_default();
        // The same word again replaces its aliases.
        words.retain(|existing| {
            !existing["word"].as_str().is_some_and(|existing| {
                existing.eq_ignore_ascii_case(entry["word"].as_str().unwrap_or_default())
            })
        });
        if words.len() >= 200 {
            self.dictate.dictionary_note =
                Some("The dictionary holds at most 200 words.".to_string());
            cx.notify();
            return;
        }
        words.push(entry);
        self.dictate.dictionary_note = None;
        self.dictate
            .word_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.dictate
            .alias_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.set_dictate(json!({ "dictionary": words }), cx);
    }

    fn remove_word(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut words = self
            .dictate_settings()
            .and_then(|settings| settings["dictionary"].as_array().cloned())
            .unwrap_or_default();
        if index < words.len() {
            words.remove(index);
            self.set_dictate(json!({ "dictionary": words }), cx);
        }
    }

    fn polish_rows(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (title, subtitle, _) = Self::catalog_row(POLISH).unwrap_or_default();
        let answer = self.dictate.answer.clone().unwrap_or_default();
        let polish = &answer["settings"]["polish"];
        let on = polish["enabled"].as_bool().unwrap_or(true);
        let available = answer["polishAvailable"].as_bool().unwrap_or(true);
        let description = if available {
            subtitle
        } else {
            format!(
                "{subtitle}\nClaude Code isn't set up on {}, so the text goes in unpolished until it is.",
                if self.dictate.machine.is_some() {
                    "that computer"
                } else {
                    "this computer"
                }
            )
        };
        let mut rows = vec![toggle_field_with(
            p,
            POLISH,
            RowSpec::new(title).description(description),
            on,
            None,
            |page: &mut Self, on, _window, cx| {
                page.set_dictate(json!({ "polish": { "enabled": on } }), cx)
            },
            cx,
        )];
        if !on {
            return rows;
        }
        let custom = polish["prompt"].as_str().map(str::to_string);
        let editing = self.dictate.prompt_draft.is_some();
        let control = h_flex()
            .gap(px(8.0))
            .items_center()
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(hsla(p.foreground))
                    .child("Claude Sonnet 5.5"),
            )
            .when(!editing, |this| {
                let custom = custom.clone();
                this.child(settings_button(
                    p,
                    "dictate-polish-prompt-edit",
                    if custom.is_some() {
                        "Edit prompt (custom)"
                    } else {
                        "Edit prompt"
                    },
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.dictate.prompt_draft = Some(custom.clone().unwrap_or_default());
                        cx.notify();
                    },
                    cx,
                ))
            })
            .into_any_element();
        let mut spec = RowSpec::new("Polish model").description(
            "The model that polishes, through Claude Code on the computer that transcribes. Edit prompt replaces the built-in instructions; leave it empty to use them.",
        );
        spec = spec.dependent();
        rows.push(setting_row(
            p,
            "dictatePolishModel",
            spec,
            None,
            control,
            cx,
        ));
        if let Some(draft) = self.dictate.prompt_draft.clone() {
            let id = SharedString::from("dictate-polish-prompt");
            let input = super::super::fields::FieldStates::textarea_state(
                self,
                &id,
                &draft,
                Some("Leave empty to use the built-in prompt"),
                (4, 12),
                |page: &mut Self, text, _window, cx| {
                    if page.dictate.prompt_draft.is_some() {
                        page.dictate.prompt_draft = Some(text);
                    }
                    let _ = cx;
                },
                window,
                cx,
            );
            let editor = v_flex()
                .w_full()
                .gap(px(8.0))
                .px(px(16.0))
                .pb(px(12.0))
                .child(settings_textarea(p, &input, 96.0, false, false, window, cx))
                .child(
                    h_flex()
                        .gap(px(8.0))
                        .justify_end()
                        .child(settings_button(
                            p,
                            "dictate-polish-prompt-cancel",
                            "Cancel",
                            None,
                            ButtonVariant::Outline,
                            false,
                            None,
                            |page: &mut Self, _window, cx| {
                                page.dictate.prompt_draft = None;
                                cx.notify();
                            },
                            cx,
                        ))
                        .child(settings_button(
                            p,
                            "dictate-polish-prompt-reset",
                            "Use built-in",
                            None,
                            ButtonVariant::Outline,
                            false,
                            None,
                            |page: &mut Self, _window, cx| {
                                page.dictate.prompt_draft = None;
                                page.set_dictate(json!({ "polish": { "prompt": null } }), cx);
                            },
                            cx,
                        ))
                        .child(settings_button(
                            p,
                            "dictate-polish-prompt-save",
                            "Save prompt",
                            None,
                            ButtonVariant::Default,
                            false,
                            None,
                            |page: &mut Self, _window, cx| {
                                let text = page.dictate.prompt_draft.take().unwrap_or_default();
                                let prompt = text.trim();
                                let value = if prompt.is_empty() {
                                    Value::Null
                                } else {
                                    Value::String(prompt.to_string())
                                };
                                page.set_dictate(json!({ "polish": { "prompt": value } }), cx);
                            },
                            cx,
                        )),
                )
                .into_any_element();
            rows.push(editor);
        }
        rows
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_the_mockup() {
        assert_eq!(size_label(715_135_416), "0.72 GB");
        assert_eq!(size_label(1_600_000_000), "1.6 GB");
        assert_eq!(size_label(147_951_465), "148 MB");
    }

    #[test]
    fn a_download_shows_its_percentage_then_preparing() {
        let mut model = models_of(&json!({"models": [{
            "id": "parakeet-tdt-0.6b-v3", "state": "downloading", "downloadBytes": 715_135_416u64,
            "download": {"receivedBytes": 301_989_888u64, "totalBytes": 715_135_416u64}
        }]}))
        .remove(0);
        assert_eq!(progress_label(&model), "42% · 0.3 of 0.72 GB");
        model.state = "preparing".to_string();
        assert_eq!(progress_label(&model), "Preparing…");
    }

    #[test]
    fn the_dictionary_form_keeps_to_gxservers_limits() {
        assert_eq!(
            dictionary_entry(" gxserver ", "GX server, g x server").unwrap(),
            json!({"word": "gxserver", "aliases": ["GX server", "g x server"]})
        );
        assert_eq!(
            dictionary_entry("Ghostex", "").unwrap(),
            json!({"word": "Ghostex", "aliases": []})
        );
        assert!(dictionary_entry("  ", "").is_err());
        assert!(dictionary_entry("word", "ab").is_err());
        assert!(dictionary_entry(&"x".repeat(65), "").is_err());
        assert!(dictionary_entry("word", "aaa,bbb,ccc,ddd,eee,fff,ggg,hhh,iii").is_err());
    }

    #[test]
    fn polling_runs_only_while_a_model_moves() {
        assert!(any_moving(
            &json!({"models": [{"id": "a", "state": "downloading"}]})
        ));
        assert!(any_moving(
            &json!({"models": [{"id": "a", "state": "preparing"}]})
        ));
        assert!(!any_moving(
            &json!({"models": [{"id": "a", "state": "ready"}, {"id": "b", "state": "failed"}]})
        ));
    }
}
