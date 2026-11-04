//! The shapes of Settings search rows: a row per control, sections of rows, and the pages that hold them.

use crate::json::{js_number_string, NumOpt, Opt, J};

/// One `{ label, value }` choice of a row, with the value as the string the control stores (`String(value)`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingOption {
    pub label: String,
    pub value: String,
}

/// One searchable Settings control. `key` is the settings key, or a UI id for rows that edit no single key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingRow {
    pub key: &'static str,
    pub title: String,
    /// Empty when the row has no subtitle.
    pub subtitle: String,
    pub options: Vec<SettingOption>,
    /// Folded into the page's Advanced section (search still finds it in place).
    pub advanced: bool,
    /// The row is a select whose choices only the running app knows; `options` is then empty.
    pub runtime_options: Option<RuntimeOptions>,
}

/// Where a select's choices come from when the catalog cannot list them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeOptions {
    /// This device's installed speech voices, after a first "System voice" choice stored as `""`.
    /// Each app lists its own (`readAloudVoice`: macOS `AVSpeechSynthesisVoice`, Windows
    /// `SpeechSynthesizer::AllVoices`, the browser's `speechSynthesis`).
    SystemVoices,
}

impl RuntimeOptions {
    /// The id the catalog JSON carries (`runtimeOptions`).
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeOptions::SystemVoices => "systemVoices",
        }
    }
}

/// A titled list of rows: a General section, or one section of another page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub settings: Vec<SettingRow>,
}

/// A searchable page other than General, Theme and Hotkeys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub id: &'static str,
    pub title: &'static str,
    pub sections: Vec<Section>,
}

pub(crate) fn section(id: &'static str, title: &'static str, settings: Vec<SettingRow>) -> Section {
    Section {
        id,
        title,
        settings,
    }
}

pub(crate) fn row(
    key: &'static str,
    title: impl Into<String>,
    subtitle: impl Into<String>,
) -> SettingRow {
    SettingRow {
        key,
        title: title.into(),
        subtitle: subtitle.into(),
        options: Vec::new(),
        advanced: false,
        runtime_options: None,
    }
}

impl SettingRow {
    pub(crate) fn options(mut self, options: &[Opt]) -> Self {
        self.options
            .extend(options.iter().map(|option| SettingOption {
                label: option.label.to_string(),
                value: option.value.to_string(),
            }));
        self
    }

    /// Numeric option tables, with each value as `String(value)`.
    pub(crate) fn num_options(mut self, options: &[NumOpt]) -> Self {
        self.options
            .extend(options.iter().map(|option| SettingOption {
                label: option.label.to_string(),
                value: js_number_string(option.value),
            }));
        self
    }

    /// `table.map((item) => ({ label: item[label_key], value: item[value_key] }))` over a `J` array.
    pub(crate) fn options_of(mut self, table: J, label_key: &str, value_key: &str) -> Self {
        self.options.extend(table.as_array().iter().map(|item| {
            SettingOption {
                label: item
                    .get(label_key)
                    .and_then(J::as_str)
                    .unwrap_or_default()
                    .to_string(),
                value: item
                    .get(value_key)
                    .and_then(J::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }
        }));
        self
    }

    pub(crate) fn runtime_options(mut self, source: RuntimeOptions) -> Self {
        self.runtime_options = Some(source);
        self
    }

    pub(crate) fn option_list(mut self, options: impl IntoIterator<Item = SettingOption>) -> Self {
        self.options.extend(options);
        self
    }
}

pub(crate) fn option(label: impl Into<String>, value: impl Into<String>) -> SettingOption {
    SettingOption {
        label: label.into(),
        value: value.into(),
    }
}
