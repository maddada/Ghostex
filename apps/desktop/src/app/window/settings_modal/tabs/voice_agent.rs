//! Settings › Voice: Dictate's engine, models and clean-up (`voice_dictate.rs`), then the voice
//! agent's provider and its voice (catalog rows, `packages/settings-catalog/src/pages/voice_agent.rs`),
//! and the API keys gxserver keeps, which both features share. A key is
//! pasted, sent once to `/api/setVoiceAgentKey` (which checks it with the provider before sealing
//! it) and never shown again; the page only learns whether each provider has one
//! (`/api/readVoiceAgent`).
//!
//! The Memory row shows the remembered last voice conversation and forgets it
//! (`/api/forgetVoiceAgentMemory`).
//!
//! CDXC:VoiceMode 2026-10-20 DECISION: User: the API keys live on the computer, kept by gxserver. Its "a plain Settings page, not an extension card" is superseded by CDXC:VoiceMode 2026-10-25 (packages/settings-catalog/src/pages/voice_agent.rs): the voice agent is a built-in extension, and while it is off this page keeps the xAI and OpenAI keys Dictate uses and the Dictation history (both keys since CDXC:VoiceMode 2026-10-30 in official_extensions.rs).
//! SEE-ALSO: server/src/voice_agent/http.rs, server/src/voice_agent/keys.rs, server/src/voice_agent/memory.rs.
use super::super::super::native_modal_kit::*;
use super::super::catalog::{SettingOption, settings_catalog};
use super::super::fields::{
    ButtonVariant, FieldStates, RowSpec, SettingsPage, reset_key, select_field, setting_row,
    settings_button, settings_section, settings_text_input, static_note,
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
use serde_json::json;
use std::time::Duration;

/// `.settings-page-width grid gap-6 px-5 py-5`: 20px above the first section on top of its own 12px.
const PAGE_TOP_PADDING: f32 = 20.0;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Saving checks the key with the provider first, after 1Password unlocks for a reference.
const SAVE_TIMEOUT: Duration = Duration::from_secs(90);

const PROVIDER: &str = "voiceAgentProvider";
const XAI_VOICE: &str = "voiceAgentXaiVoice";
const OPENAI_VOICE: &str = "voiceAgentOpenaiVoice";
const SPEED: &str = "voiceAgentSpeed";
const CALL_MODE: &str = "voiceAgentCallMode";
const XAI_KEY: &str = "voiceAgentXaiKey";
const OPENAI_KEY: &str = "voiceAgentOpenaiKey";
const MEMORY: &str = "voiceAgentMemory";
const DICTATION_HISTORY: &str = "voiceAgentDictationHistory";

fn store_request_key(store: &Entity<SettingsStore>, cx: &App) -> Option<String> {
    store.read(cx).request().initial_voice_agent_key.clone()
}

pub(crate) fn voice_agent_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| VoiceAgentTab::new(store.clone(), window, cx))
        .into()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Provider {
    Xai,
    OpenAi,
}

impl Provider {
    fn id(self) -> &'static str {
        match self {
            Provider::Xai => "xai",
            Provider::OpenAi => "openai",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Provider::Xai => "xAI",
            Provider::OpenAi => "OpenAI",
        }
    }

    fn row_key(self) -> &'static str {
        match self {
            Provider::Xai => XAI_KEY,
            Provider::OpenAi => OPENAI_KEY,
        }
    }
}

/// One provider's key field and what the page knows about its saved key.
struct KeyField {
    input: Entity<InputState>,
    /// `None` until `/api/readVoiceAgent` answered.
    configured: Option<bool>,
    /// A key is saved but can't be read anymore, so it has to be pasted again.
    unreadable: bool,
    /// The 1Password reference the key is read from, when it is kept there (Settings > Secrets).
    reference: Option<String>,
    busy: bool,
    /// The last save or remove: whether it worked and what to say.
    note: Option<(bool, String)>,
}

pub(crate) struct VoiceAgentTab {
    pub(super) store: Entity<SettingsStore>,
    /// Dictate's half of the page (`voice_dictate.rs`).
    pub(super) dictate: super::voice_dictate::DictateState,
    fields: FieldStates,
    xai: KeyField,
    openai: KeyField,
    read_error: Option<String>,
    /// The remembered last conversation (`memory` of `/api/readVoiceAgent`); `None` until read.
    memory: Option<serde_json::Value>,
    memory_pending: bool,
    memory_busy: bool,
    memory_error: Option<String>,
    /// Dictation history, the last section (`voice_agent_history.rs`).
    history: Entity<super::voice_agent_history::DictationHistory>,
}

impl SettingsPage for VoiceAgentTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl VoiceAgentTab {
    fn new(store: Entity<SettingsStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let key_field =
            |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| KeyField {
                input: cx.new(|cx| {
                    InputState::new(window, cx)
                        .masked(true)
                        .placeholder(placeholder)
                }),
                configured: None,
                unreadable: false,
                reference: None,
                busy: false,
                note: None,
            };
        let history = {
            let store = store.clone();
            cx.new(|cx| super::voice_agent_history::DictationHistory::new(store, window, cx))
        };
        let dictate = super::voice_dictate::DictateState::new(window, cx);
        let mut tab = Self {
            history,
            dictate,
            store,
            fields: FieldStates::default(),
            xai: key_field("Paste your xAI API key or an op:// reference", window, cx),
            openai: key_field(
                "Paste your OpenAI API key or an op:// reference",
                window,
                cx,
            ),
            read_error: None,
            memory: None,
            memory_pending: false,
            memory_busy: false,
            memory_error: None,
        };
        tab.read_status(cx);
        tab.read_dictate(cx);
        super::machine_picker::request_machine_states(&tab.store, cx);
        // Dictate's Add key / Check key lands with the caret in that key's field.
        let focus = match store_request_key(&tab.store, cx).as_deref() {
            Some("openai") => Some(tab.openai.input.clone()),
            Some("xai") => Some(tab.xai.input.clone()),
            _ => None,
        };
        if let Some(input) = focus {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
        tab
    }

    fn key(&mut self, provider: Provider) -> &mut KeyField {
        match provider {
            Provider::Xai => &mut self.xai,
            Provider::OpenAi => &mut self.openai,
        }
    }

    /// Whether each provider has a key (never the key itself).
    fn read_status(&mut self, cx: &mut Context<Self>) {
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/readVoiceAgent",
            json!({}),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    match result {
                        Ok(status) => {
                            page.read_error = None;
                            for provider in [Provider::Xai, Provider::OpenAi] {
                                let key = &status["keys"][provider.id()];
                                let field = page.key(provider);
                                field.configured = key["configured"].as_bool();
                                field.unreadable = key["unreadable"].as_bool() == Some(true);
                                field.reference = key["reference"].as_str().map(str::to_string);
                            }
                            page.memory = Some(status["memory"].clone());
                            page.memory_pending =
                                status["memorySummaryPending"].as_bool().unwrap_or(false);
                        }
                        Err(error) => page.read_error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Sends the pasted key once (or an empty one to remove it). gxserver checks it with the
    /// provider and answers whether it is saved.
    fn save_key(
        &mut self,
        provider: Provider,
        remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.key(provider).input.clone();
        let key = if remove {
            String::new()
        } else {
            input.read(cx).value().trim().to_string()
        };
        if !remove && key.is_empty() {
            self.key(provider).note = Some((false, "Paste a key first.".to_string()));
            cx.notify();
            return;
        }
        let field = self.key(provider);
        field.busy = true;
        field.note = None;
        // The key leaves the field as soon as it is sent, so it is never on screen again.
        input.update(cx, |input, cx| input.set_value("", window, cx));
        // Only a reference (not a secret) is kept to show where the key is read from.
        let typed_reference = key.starts_with("op://").then(|| key.clone());
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/setVoiceAgentKey",
            json!({ "provider": provider.id(), "apiKey": key }),
            SAVE_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    let field = page.key(provider);
                    field.busy = false;
                    match result {
                        Ok(answer) => {
                            let configured = answer["configured"].as_bool().unwrap_or(!remove);
                            let from_reference = answer["reference"] == true;
                            field.configured = Some(configured);
                            field.unreadable = false;
                            field.reference = typed_reference.clone().filter(|_| from_reference);
                            field.note = Some((
                                true,
                                if from_reference {
                                    format!(
                                        "Saved. {} accepted the key 1Password gave; it is read from 1Password each time.",
                                        provider.label()
                                    )
                                } else if configured {
                                    format!("Saved. {} accepted the key.", provider.label())
                                } else {
                                    "Removed from this computer.".to_string()
                                },
                            ));
                        }
                        Err(error) => field.note = Some((false, error)),
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    fn forget_memory(&mut self, cx: &mut Context<Self>) {
        self.memory_busy = true;
        self.memory_error = None;
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/forgetVoiceAgentMemory",
            json!({}),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.memory_busy = false;
                    match result {
                        Ok(_) => {
                            page.memory = Some(serde_json::Value::Null);
                            page.memory_pending = false;
                        }
                        Err(error) => page.memory_error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    fn memory_row(
        &mut self,
        p: &super::super::palette::SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (title, subtitle, _) = Self::catalog_row(MEMORY)?;
        let remembered = self.memory.as_ref().filter(|memory| memory.is_object());
        let status: String = match (&self.memory_error, remembered) {
            (Some(error), _) => error.clone(),
            (None, Some(memory)) => {
                let when = memory["endedAt"]
                    .as_str()
                    .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                    .map(|at| {
                        at.with_timezone(&chrono::Local)
                            .format("%-d %b, %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default();
                format!("{when}: {}", memory["gist"].as_str().unwrap_or_default())
            }
            (None, None) if self.memory_pending => {
                "Saving a summary of the last conversation…".to_string()
            }
            (None, None) if self.memory.is_some() => "Nothing remembered yet.".to_string(),
            (None, None) => String::new(),
        };
        let can_forget = remembered.is_some() || self.memory_pending;
        let failed = self.memory_error.is_some();
        let control = h_flex()
            .w_full()
            .gap(px(8.0))
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.5))
                    .text_color(hsla(if failed { p.destructive } else { p.muted }))
                    .child(status),
            )
            .when(can_forget, |this| {
                this.child(settings_button(
                    p,
                    SharedString::from(format!("{MEMORY}-forget")),
                    "Forget",
                    None,
                    ButtonVariant::Outline,
                    self.memory_busy,
                    None,
                    move |page: &mut Self, _window, cx| page.forget_memory(cx),
                    cx,
                ))
            })
            .into_any_element();
        Some(setting_row(
            p,
            MEMORY,
            RowSpec::new(title).description(subtitle).wide(),
            None,
            control,
            cx,
        ))
    }

    pub(super) fn catalog_row(key: &str) -> Option<(String, String, Vec<SettingOption>)> {
        settings_catalog()
            .extra_tab(SettingsTabId::VoiceAgent.id())?
            .sections
            .iter()
            .flat_map(|section| section.settings.iter())
            .find(|row| row.key == key)
            .map(|row| (row.title.clone(), row.subtitle.clone(), row.options.clone()))
    }

    fn select_row(
        &mut self,
        p: &super::super::palette::SettingsPalette,
        key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (title, subtitle, options) = Self::catalog_row(key)?;
        let values = self.store.read(cx).values();
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = values.choice(key, &allowed);
        Some(select_field(
            self,
            p,
            key,
            RowSpec::new(title)
                .description(subtitle)
                .keyed(&values, key),
            Some(reset_key::<Self>(key)),
            &options,
            &value,
            None,
            move |page: &mut Self, next, _window, cx| {
                page.store
                    .update(cx, |store, cx| store.update_setting(key, json!(next), cx));
            },
            window,
            cx,
        ))
    }

    fn key_row(
        &mut self,
        p: &super::super::palette::SettingsPalette,
        provider: Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (title, subtitle, _) = Self::catalog_row(provider.row_key())?;
        let field = self.key(provider);
        let input = field.input.clone();
        let configured = field.configured;
        let unreadable = field.unreadable;
        let reference = field.reference.clone();
        let busy = field.busy;
        let note = field.note.clone();
        let status: SharedString = match (busy, configured) {
            (true, _) => format!("Checking the key with {}…", provider.label()).into(),
            (false, _) if unreadable => {
                "This saved key can't be read anymore. Paste it again.".into()
            }
            (false, Some(true)) => match &reference {
                Some(reference) => format!("Read from 1Password: {reference}").into(),
                None => "A key is saved on this computer.".into(),
            },
            (false, Some(false)) => "No key yet.".into(),
            (false, None) => "".into(),
        };
        let save_id = SharedString::from(format!("{}-save", provider.row_key()));
        let remove_id = SharedString::from(format!("{}-remove", provider.row_key()));
        let control = v_flex()
            .w_full()
            .gap(px(6.0))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.0))
                    .items_center()
                    .child(settings_text_input(p, &input, None, true, window, cx))
                    .child(settings_button(
                        p,
                        save_id,
                        if configured == Some(true) {
                            "Replace"
                        } else {
                            "Save"
                        },
                        None,
                        ButtonVariant::Outline,
                        busy,
                        None,
                        move |page: &mut Self, window, cx| {
                            page.save_key(provider, false, window, cx)
                        },
                        cx,
                    ))
                    .when(configured == Some(true) || unreadable, |this| {
                        this.child(settings_button(
                            p,
                            remove_id,
                            "Remove",
                            None,
                            ButtonVariant::Outline,
                            busy,
                            None,
                            move |page: &mut Self, window, cx| {
                                page.save_key(provider, true, window, cx)
                            },
                            cx,
                        ))
                    }),
            )
            .when(!status.is_empty() || note.is_some(), |this| {
                this.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(hsla(match &note {
                            Some((false, _)) => p.destructive,
                            None if unreadable && !busy => p.destructive,
                            _ => p.muted,
                        }))
                        .child(match note {
                            Some((_, text)) => SharedString::from(text),
                            None => status,
                        }),
                )
            })
            .into_any_element();
        Some(setting_row(
            p,
            provider.row_key(),
            RowSpec::new(title).description(subtitle).wide(),
            None,
            control,
            cx,
        ))
    }
}

impl VoiceAgentTab {
    fn dictate_machine(&self) -> Option<&str> {
        self.dictate.machine()
    }
}

impl Render for VoiceAgentTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, values, search, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.values(),
                store.tab_search(SettingsTabId::VoiceAgent),
                matching,
            )
        };
        // A machine picked at the top (or one that went away): Dictate follows it.
        let _ = super::machine_picker::picked_machine_choice(&self.store, cx);
        self.sync_dictate_machine(cx);
        let remote = self.dictate_machine().is_some();
        // The voice agent turned off on Settings > Extensions leaves its call rows out; Dictate
        // keeps its xAI and OpenAI keys and history here (CDXC:VoiceMode 2026-10-30 in official_extensions.rs).
        let calls_on = ghostex_settings_catalog::built_in_extensions::on_with(
            ghostex_settings_catalog::built_in_extensions::VOICE_AGENT,
            |key| Some(values.bool(key)),
        );
        let provider = values.string(PROVIDER);
        let mut provider_rows = Vec::new();
        for key in [PROVIDER, XAI_VOICE, OPENAI_VOICE, SPEED, CALL_MODE] {
            // Only the chosen provider's voice is offered.
            let shown = match key {
                XAI_VOICE => provider != "openai",
                OPENAI_VOICE => provider == "openai",
                _ => true,
            };
            if shown
                && calls_on
                && !remote
                && search.row_visible("provider", key)
                && let Some(row) = self.select_row(&p, key, window, cx)
            {
                provider_rows.push(row);
            }
        }
        let mut memory_rows = Vec::new();
        if calls_on
            && !remote
            && search.row_visible("memory", MEMORY)
            && let Some(row) = self.memory_row(&p, cx)
        {
            memory_rows.push(row);
        }
        let mut key_rows = Vec::new();
        for provider in [Provider::Xai, Provider::OpenAi] {
            if !remote
                && search.row_visible("keys", provider.row_key())
                && let Some(row) = self.key_row(&p, provider, window, cx)
            {
                key_rows.push(row);
            }
        }
        if !key_rows.is_empty() {
            let note = if calls_on {
                "Get an xAI key at console.x.ai under API Keys, and an OpenAI key at platform.openai.com under API keys. Dictate uses one only when its engine is that provider; the local model needs none. Each provider bills your account while a conversation is open."
            } else {
                "The voice agent is turned off in Settings > Extensions. Dictate still uses these keys when its engine is xAI or OpenAI; the local model needs none. Get an xAI key at console.x.ai under API Keys, or an OpenAI key at platform.openai.com under API keys."
            };
            key_rows.push(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .child(static_note(&p, note, false))
                    .into_any_element(),
            );
            if let Some(error) = &self.read_error {
                key_rows.push(
                    div()
                        .px(px(16.0))
                        .pb(px(12.0))
                        .text_size(px(12.5))
                        .text_color(hsla(p.destructive))
                        .child(format!("Could not read the saved keys: {error}"))
                        .into_any_element(),
                );
            }
        }
        let mut blocks = Vec::new();
        // CDXC:VoiceMode 2026-11-05 DECISION: User (Q1275): Dictate's settings belong to each
        // computer, so the page carries the machine picker Agents and Integrations use; a remote
        // machine shows its Dictate, while the voice agent and the keys stay this computer's.
        if let Some(picker) = super::machine_picker::render_machine_picker_described(
            self,
            &p,
            &self.store.clone(),
            |shown| match shown {
                Some(name) => format!(
                    "Showing Dictate on {name}: its engine, models and clean-up are saved there and transcribe its sessions. The voice agent and the API keys are changed on {name} itself."
                ),
                None => "Pick a connected remote machine to change its Dictate engine, models and clean-up.".to_string(),
            },
            window,
            cx,
        ) {
            let mut block = PageBlock::section("machinePicker", picker);
            block.margin_top += PAGE_TOP_PADDING;
            blocks.push(block);
        }
        let dictate_visible = |key: &str| search.row_visible("dictate", key);
        let dictate_rows = self.dictate_rows(&p, &dictate_visible, window, cx);
        if let Some(section) = settings_section(
            &p,
            "Dictate",
            Some("Speech to text in the chat box. It types what you said and never sends it.".into()),
            None,
            dictate_rows,
        ) {
            let mut block = PageBlock::section("dictate", section);
            if blocks.is_empty() {
                block.margin_top += PAGE_TOP_PADDING;
            }
            blocks.push(block);
        }
        let cleanup_visible = |key: &str| search.row_visible("cleanup", key);
        let cleanup_rows = self.cleanup_rows(&p, &cleanup_visible, window, cx);
        if let Some(section) = settings_section(
            &p,
            "Clean-up",
            Some("Applied after you stop, before the text is typed.".into()),
            None,
            cleanup_rows,
        ) {
            blocks.push(PageBlock::section("cleanup", section));
        }
        if let Some(section) = settings_section(
            &p,
            "Voice agent",
            Some("A spoken conversation about your sessions, with its own provider: it doesn't follow Dictate's engine.".into()),
            None,
            provider_rows,
        ) {
            let mut block = PageBlock::section("provider", section);
            if blocks.is_empty() {
                block.margin_top += PAGE_TOP_PADDING;
            }
            blocks.push(block);
        }
        if let Some(section) = settings_section(&p, "Memory", None, None, memory_rows) {
            let mut block = PageBlock::section("memory", section);
            if blocks.is_empty() {
                block.margin_top += PAGE_TOP_PADDING;
            }
            blocks.push(block);
        }
        if let Some(section) = settings_section(&p, "API keys", None, None, key_rows) {
            let mut block = PageBlock::section("keys", section);
            if blocks.is_empty() {
                block.margin_top += PAGE_TOP_PADDING;
            }
            blocks.push(block);
        }
        if !remote
            && search.row_visible("dictations", DICTATION_HISTORY)
            && let Some(section) = settings_section(
                &p,
                "Dictation history",
                None,
                None,
                vec![self.history.clone().into_any_element()],
            )
        {
            let mut block = PageBlock::section("dictations", section);
            if blocks.is_empty() {
                block.margin_top += PAGE_TOP_PADDING;
            }
            blocks.push(block);
        }
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::VoiceAgent,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        settings_page(&self.store, SettingsTabId::VoiceAgent, &p, blocks, cx)
    }
}
