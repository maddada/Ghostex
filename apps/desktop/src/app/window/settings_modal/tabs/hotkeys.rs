//! The Hotkeys page (packages/core-ui/settings-modal/tabs/hotkeys.tsx (deleted 2026-10-01)): the six sections of
//! `HOTKEY_SETTINGS_SECTIONS`, one recorder per action (duplicates marked), the Jump to Project
//! and Skip sleeping sessions toggles, and Reset Hotkeys. The same top search filters the rows
//! (`rail::hotkey_section_searches`), and the rail jumps to a section by its id.
use super::super::super::native_modal_kit::*;
use super::super::catalog::settings_catalog;
use super::super::fields::{
    ButtonVariant, CONTROL_LANE_WIDTH, FieldStates, HotkeyRecorder, HotkeyRecorderHost, RowSpec,
    SettingsPage, hotkey_recorder_field, is_reserved_hotkey, normalize_hotkey_text, setting_row,
    settings_button, settings_section, toggle_field,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::hotkey_section_searches;
use super::super::search::{should_show_section, should_show_setting};
use super::super::store::SettingsStore;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, Styled as _, Window, div, px,
};
use gpui_component::h_flex;
use serde_json::{Map, Value};
use std::collections::HashMap;

const HOTKEYS_MODULE: &str = super::super::catalog::module::HOTKEYS;

/// One `GHOSTEX_HOTKEY_DEFINITIONS` entry.
struct HotkeyDefinition {
    id: String,
    title: String,
    description: String,
    default_key: String,
    windows_linux_default_key: Option<String>,
    retired_default_keys: Vec<String>,
}

impl HotkeyDefinition {
    /// The default on this platform (`platformDefaultKey`).
    fn platform_default(&self) -> String {
        if cfg!(target_os = "macos") {
            self.default_key.clone()
        } else {
            self.windows_linux_default_key
                .clone()
                .unwrap_or_else(|| self.default_key.clone())
        }
    }
}

fn definitions() -> Vec<HotkeyDefinition> {
    settings_catalog()
        .module_value(HOTKEYS_MODULE, "GHOSTEX_HOTKEY_DEFINITIONS")
        .and_then(Value::as_array)
        .map(|definitions| {
            definitions
                .iter()
                .filter_map(|definition| {
                    let text = |key: &str| definition.get(key)?.as_str().map(str::to_string);
                    Some(HotkeyDefinition {
                        id: text("id")?,
                        title: text("title").unwrap_or_default(),
                        description: text("description").unwrap_or_default(),
                        default_key: text("defaultKey").unwrap_or_default(),
                        windows_linux_default_key: text("windowsLinuxDefaultKey"),
                        retired_default_keys: definition
                            .get("retiredDefaultKeys")
                            .and_then(Value::as_array)
                            .map(|keys| {
                                keys.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `readLegacyHotkey`: Toggle View Panel keeps a saved Companion Pane chord, and Jump to Project
/// 1-5 keep the old Focus Group 1-5 chords.
fn legacy_hotkey<'a>(source: &'a Map<String, Value>, action_id: &str) -> Option<&'a Value> {
    if action_id == "toggleViewPanel" {
        return source.get("toggleCompanionPane");
    }
    let slot = action_id.strip_prefix("jumpToProject")?;
    matches!(slot, "1" | "2" | "3" | "4" | "5")
        .then(|| source.get(&format!("focusGroup{slot}")))
        .flatten()
}

/// `normalizeghostexHotkeySettings`: every action's chord, the platform default where nothing
/// (or a retired default, the macOS default on another platform, or a reserved chord) is saved,
/// and an explicit blank kept as unassigned.
fn normalize_hotkeys(candidate: &Value, definitions: &[HotkeyDefinition]) -> Vec<(String, String)> {
    let empty = Map::new();
    let source = candidate.as_object().unwrap_or(&empty);
    definitions
        .iter()
        .map(|definition| {
            let platform_default = definition.platform_default();
            let value = source
                .get(&definition.id)
                .or_else(|| legacy_hotkey(source, &definition.id));
            let chord = match value.and_then(Value::as_str) {
                Some(value) => {
                    let text = if value.trim().is_empty() {
                        String::new()
                    } else {
                        normalize_hotkey_text(value)
                    };
                    let retired = definition.retired_default_keys.contains(&text);
                    let mac_default_elsewhere = !cfg!(target_os = "macos")
                        && definition
                            .windows_linux_default_key
                            .as_deref()
                            .is_some_and(|key| !key.is_empty())
                        && text == definition.default_key;
                    if retired || mac_default_elsewhere || is_reserved_hotkey(&text) {
                        platform_default
                    } else {
                        text
                    }
                }
                None => platform_default,
            };
            (definition.id.clone(), chord)
        })
        .collect()
}

/// The id prefix of a Terminal focused row's recorder, so one action's two rows record apart.
const TERMINAL_ROW_PREFIX: &str = "terminal:";

/// Every Terminal focused binding (CDXC:Hotkeys 2026-10-27 DECISION in
/// packages/settings-catalog/src/hotkey_definitions.rs): what `candidate` (the saved
/// `terminalHotkeys`) holds, or else the normal chord the user customized (`customized`), or else
/// the platform's Terminal focused default (`ghostex_settings_catalog::terminal_hotkey`).
fn terminal_hotkeys(
    candidate: &Value,
    customized: &HashMap<String, String>,
    definitions: &[HotkeyDefinition],
) -> Vec<(String, String)> {
    let platform = ghostex_settings_catalog::Platform::current();
    definitions
        .iter()
        .filter_map(|definition| {
            let saved = candidate
                .get(&definition.id)
                .and_then(Value::as_str)
                .map(normalize_hotkey_text);
            ghostex_settings_catalog::terminal_hotkey(
                &definition.id,
                saved.as_deref(),
                customized.get(&definition.id).map(String::as_str),
                platform,
            )
            .map(|chord| (definition.id.clone(), chord))
        })
        .collect()
}

/// `getDuplicateHotkeyIds`.
fn duplicate_ids(hotkeys: &[(String, String)]) -> Vec<String> {
    let mut by_chord: HashMap<String, Vec<String>> = HashMap::new();
    for (id, chord) in hotkeys {
        let chord = normalize_hotkey_text(chord);
        // A view-scoped hotkey (Search in Files) may share an app-wide chord on purpose.
        if chord.is_empty()
            || ghostex_settings_catalog::VIEW_SCOPED_HOTKEY_IDS.contains(&id.as_str())
        {
            continue;
        }
        by_chord.entry(chord).or_default().push(id.clone());
    }
    by_chord
        .into_values()
        .filter(|ids| ids.len() > 1)
        .flatten()
        .collect()
}

pub(crate) fn hotkeys_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| HotkeysTab::new(store.clone(), cx)).into()
}

pub(crate) struct HotkeysTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    recorder: HotkeyRecorder,
    definitions: Vec<HotkeyDefinition>,
    /// The preview binary's `hotkeys-recording` state arms a recorder on the first frame.
    preview_recording: bool,
}

impl HotkeysTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let preview_recording =
            store.read(cx).request().preview_state.as_deref() == Some("hotkeys-recording");
        Self {
            store,
            fields: FieldStates::default(),
            recorder: HotkeyRecorder::default(),
            definitions: definitions(),
            preview_recording,
        }
    }

    /// The normal bindings: every hotkey but the terminal-only ones.
    fn current_hotkeys(&self, cx: &App) -> Vec<(String, String)> {
        normalize_hotkeys(&self.store.read(cx).value("hotkeys"), &self.definitions)
            .into_iter()
            .filter(|(id, _)| ghostex_settings_catalog::hotkey_has_normal_binding(id))
            .collect()
    }

    /// The normal defaults on this platform (`normalizeghostexHotkeySettings(DEFAULT_GHOSTEX_HOTKEYS)`):
    /// the macOS defaults, moved to the Windows/Linux chord where an action has one.
    fn normal_defaults(&self) -> Vec<(String, String)> {
        let default_table = settings_catalog()
            .module_value(HOTKEYS_MODULE, "DEFAULT_GHOSTEX_HOTKEYS")
            .cloned()
            .unwrap_or_default();
        normalize_hotkeys(&default_table, &self.definitions)
            .into_iter()
            .filter(|(id, _)| ghostex_settings_catalog::hotkey_has_normal_binding(id))
            .collect()
    }

    /// The normal chords the user changed from their default (`""` for a cleared one). `hotkeys`
    /// stores the whole table, so only a chord that differs from its default counts.
    fn customized_normal_hotkeys(&self, cx: &App) -> HashMap<String, String> {
        let defaults: HashMap<String, String> = self.normal_defaults().into_iter().collect();
        self.current_hotkeys(cx)
            .into_iter()
            .filter(|(id, chord)| defaults.get(id) != Some(chord))
            .collect()
    }

    fn saved_terminal_hotkeys(&self, cx: &App) -> Value {
        self.store
            .read(cx)
            .value(ghostex_settings_catalog::TERMINAL_HOTKEYS_SETTING_KEY)
    }

    /// The Terminal focused bindings, resolved the way the app binds them.
    fn current_terminal_hotkeys(&self, cx: &App) -> Vec<(String, String)> {
        terminal_hotkeys(
            &self.saved_terminal_hotkeys(cx),
            &self.customized_normal_hotkeys(cx),
            &self.definitions,
        )
    }

    /// What each Terminal focused row shows as its default: the customized normal chord it
    /// follows, or the Terminal focused default.
    fn terminal_fallbacks(&self, cx: &App) -> Vec<(String, String)> {
        terminal_hotkeys(
            &Value::Null,
            &self.customized_normal_hotkeys(cx),
            &self.definitions,
        )
    }

    fn save_terminal_hotkeys(&mut self, map: Map<String, Value>, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.update_setting(
                ghostex_settings_catalog::TERMINAL_HOTKEYS_SETTING_KEY,
                Value::Object(map),
                cx,
            )
        });
    }

    fn save_hotkeys(&mut self, hotkeys: Vec<(String, String)>, cx: &mut Context<Self>) {
        let mut map = Map::new();
        for (id, chord) in hotkeys {
            map.insert(id, Value::String(chord));
        }
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.update_setting("hotkeys", Value::Object(map), cx)
        });
    }
}

impl super::HoldsUnsavedInput for HotkeysTab {
    /// A hotkey is being recorded.
    fn holds_unsaved_input(&self, _cx: &gpui::App) -> bool {
        self.recorder.recording().is_some()
    }
}

impl SettingsPage for HotkeysTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl HotkeyRecorderHost for HotkeysTab {
    fn hotkey_recorder(&mut self) -> &mut HotkeyRecorder {
        &mut self.recorder
    }

    /// `updateHotkey(id, value)`: the whole normalized table with this action's chord replaced.
    fn hotkey_recorded(&mut self, id: &str, hotkey: String, cx: &mut Context<Self>) {
        if let Some(id) = id.strip_prefix(TERMINAL_ROW_PREFIX) {
            // Only this row's own value is stored, so every other Terminal focused row keeps
            // following its normal key; a value equal to what the row would follow anyway is
            // dropped instead of pinned.
            let mut saved = self
                .saved_terminal_hotkeys(cx)
                .as_object()
                .cloned()
                .unwrap_or_default();
            let recorded = normalize_hotkey_text(&hotkey);
            let fallback = self
                .terminal_fallbacks(cx)
                .into_iter()
                .find(|(fallback_id, _)| fallback_id == id)
                .map(|(_, chord)| chord);
            if fallback.as_deref() == Some(recorded.as_str()) {
                saved.remove(id);
            } else {
                saved.insert(id.to_string(), Value::String(recorded));
            }
            self.save_terminal_hotkeys(saved, cx);
            return;
        }
        let mut hotkeys: Map<String, Value> = self
            .current_hotkeys(cx)
            .into_iter()
            .map(|(id, chord)| (id, Value::String(chord)))
            .collect();
        hotkeys.insert(
            id.to_string(),
            Value::String(normalize_hotkey_text(&hotkey)),
        );
        let normalized = normalize_hotkeys(&Value::Object(hotkeys), &self.definitions);
        self.save_hotkeys(normalized, cx);
    }
}

impl HotkeysTab {
    /// A toggle row of the Projects or Navigation section.
    fn toggle(
        &mut self,
        key: &'static str,
        label: &str,
        description: &str,
        dependent: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (p, values) = {
            let store = self.store.read(cx);
            (store.palette(), store.values())
        };
        let mut spec = RowSpec::new(label.to_string())
            .description(description.to_string())
            .keyed(&values, key);
        if dependent {
            spec = spec.dependent();
        }
        toggle_field(self, &p, key, spec, values.bool(key), cx)
    }
}

impl HotkeysTab {
    /// CDXC:Hotkeys 2026-09-19 DECISION:
    /// User: the Skip sleeping sessions option lives on the Hotkeys page right below the Previous/Next Session hotkeys it changes.
    fn skip_sleeping_row(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.toggle(
            "sidebarSessionCycleSkipsSleeping",
            "Skip sleeping sessions",
            "Next Session and Previous Session jump over sleeping sessions in the sidebar.",
            true,
            cx,
        )
    }
}

impl Render for HotkeysTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.preview_recording) {
            super::super::fields::start_recording(self, "focusChatComposer".into(), window, cx);
        }
        let (p, values, query) = {
            let store = self.store.read(cx);
            (
                store.palette(),
                store.values(),
                store.search_query().to_string(),
            )
        };
        let searches = hotkey_section_searches(&query, &|id| {
            ghostex_settings_catalog::built_in_extensions::hotkey_shown_with(id, |key| {
                Some(values.bool(key))
            })
        });
        let hotkeys = self.current_hotkeys(cx);
        let defaults = self.normal_defaults();
        let duplicates = duplicate_ids(&hotkeys);
        // CDXC:Hotkeys 2026-10-27 WHY:
        // The Terminal focused rows are a second table: conflicts are found within each section only, because the same keys may be set in both (one applies in a terminal, the other everywhere else).
        let terminal_hotkeys_now = self.current_terminal_hotkeys(cx);
        let terminal_defaults = self.terminal_fallbacks(cx);
        let terminal_duplicates = duplicate_ids(&terminal_hotkeys_now);
        let sections_json = settings_catalog()
            .module_value(
                super::super::catalog::module::SETTINGS_TYPES,
                "HOTKEY_SETTINGS_SECTIONS",
            )
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let terminal_section_ids: Vec<String> = sections_json
            .iter()
            .filter(|section| section.get("terminal").and_then(Value::as_bool) == Some(true))
            .filter_map(|section| section.get("id")?.as_str().map(str::to_string))
            .collect();
        let section_ids: HashMap<String, Vec<String>> = sections_json
            .iter()
            .filter_map(|section| {
                Some((
                    section.get("id")?.as_str()?.to_string(),
                    section
                        .get("ids")?
                        .as_array()?
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect(),
                ))
            })
            .collect();
        let mut blocks: Vec<PageBlock> = Vec::new();
        let mut any_visible = false;
        for (section_id, title, result) in &searches {
            if !should_show_section(result, None) {
                continue;
            }
            any_visible = true;
            let mut rows: Vec<AnyElement> = Vec::new();
            if section_id == "projects"
                && should_show_setting(result, "expandCollapsedProjectsOnJump", None)
            {
                rows.push(self.toggle(
                        "expandCollapsedProjectsOnJump",
                        "Expand Collapsed Projects on Jump",
                        "Reveal a collapsed project row before focusing it from Jump to Project hotkeys.",
                        false,
                        cx,
                    ));
            }
            let terminal_section = terminal_section_ids.contains(section_id);
            let (section_hotkeys, section_defaults, section_duplicates) = if terminal_section {
                (
                    &terminal_hotkeys_now,
                    &terminal_defaults,
                    &terminal_duplicates,
                )
            } else {
                (&hotkeys, &defaults, &duplicates)
            };
            for id in section_ids.get(section_id).into_iter().flatten() {
                let Some(definition) = self
                    .definitions
                    .iter()
                    .find(|definition| definition.id == *id)
                else {
                    continue;
                };
                if !should_show_setting(result, &definition.id, None) {
                    continue;
                }
                let value = section_hotkeys
                    .iter()
                    .find(|(hotkey_id, _)| hotkey_id == id)
                    .map(|(_, chord)| chord.clone())
                    .unwrap_or_else(|| definition.default_key.clone());
                let original = section_defaults
                    .iter()
                    .find(|(hotkey_id, _)| hotkey_id == id)
                    .map(|(_, chord)| chord.clone())
                    .unwrap_or_default();
                let (title, description) =
                    (definition.title.clone(), definition.description.clone());
                let row_id = if terminal_section {
                    format!("{TERMINAL_ROW_PREFIX}{id}")
                } else {
                    id.clone()
                };
                let recorder = hotkey_recorder_field(
                    self,
                    &p,
                    row_id.clone(),
                    &value,
                    &original,
                    section_duplicates.contains(id),
                    cx,
                );
                let control = div()
                    .w(px(CONTROL_LANE_WIDTH))
                    .max_w_full()
                    .child(recorder)
                    .into_any_element();
                rows.push(setting_row(
                    &p,
                    format!("hotkey-row-{row_id}"),
                    RowSpec::new(title).description(description),
                    None,
                    control,
                    cx,
                ));
                if id == "focusNextSession"
                    && !terminal_section
                    && searches
                        .iter()
                        .find(|(id, _, _)| id == "navigation")
                        .is_some_and(|(_, _, navigation)| {
                            should_show_setting(
                                navigation,
                                "sidebarSessionCycleSkipsSleeping",
                                None,
                            )
                        })
                {
                    rows.push(self.skip_sleeping_row(cx));
                }
            }
            blocks.extend(
                settings_section(&p, title.clone(), None, None, rows)
                    .map(|section| PageBlock::section(section_id.clone(), section)),
            );
        }
        if !any_visible {
            blocks.push(PageBlock::plain(
                div()
                    .w_full()
                    .px(px(16.0))
                    .py(px(24.0))
                    .flex()
                    .justify_center()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.muted))
                    .child("No hotkeys match your search."),
            ));
        }
        let defaults_for_reset = defaults.clone();

        blocks.push(PageBlock::plain(h_flex().w_full().justify_end().child(
            settings_button(
                &p,
                "hotkeys-reset",
                "Reset Hotkeys",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.recorder.stop();
                    page.save_hotkeys(defaults_for_reset.clone(), cx);
                    // Every Terminal focused row goes back to following its (now default) key.
                    page.save_terminal_hotkeys(Map::new(), cx);
                },
                cx,
            ),
        )));
        settings_page(&self.store, SettingsTabId::Hotkeys, &p, blocks, cx)
    }
}
