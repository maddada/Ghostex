//! The published agent model catalog, as the option pills read it.
//!
//! Ported from the deleted `packages/shared/agent-model-catalog.ts`. The catalog is authored outside the app and
//! refreshed at runtime, so it arrives as JSON and is validated here: anything that is not a
//! complete, well-formed document of the supported schema version is rejected whole, and the last
//! good catalog stays in effect.
//!
//! This module names no model. It only says what shape a catalog has and how its labels read.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::jsnum::js_number_of;

/// The schema version this build understands.
pub const AGENT_MODEL_CATALOG_SCHEMA_VERSION: i64 = 1;

/// Longest model label a footer pill shows before it is cut.
pub const AGENT_MODEL_LABEL_MAX_CHARS: usize = 20;

/// One row of an agent's model picker.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatalogModel {
    /// Stable id the client dispatches and persists; never shown to the user.
    pub value: String,
    /// Display label, already shortened for the pills.
    pub label: String,
    /// The exact row text the CLI's own picker shows, when it differs from `label`.
    pub picker_label: Option<String>,
    pub description: Option<String>,
    /// Effort levels this model accepts; empty when the model has none.
    pub efforts: Vec<String>,
    pub default_effort: Option<String>,
    pub fast_mode: bool,
    pub default: bool,
    /// Id of the agent group this row is nested under.
    pub group: Option<String>,
    /// Other spellings the CLI prints for this model, beyond `label` and `picker_label`.
    pub terminal_labels: Vec<String>,
}

/// A submenu in an agent's model picker.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatalogGroup {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

/// Whether an agent offers a fast mode, and how it is toggled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatalogFastMode {
    pub available: bool,
    /// Slash command that toggles it, when the CLI has one.
    pub command: Option<String>,
    /// `model` when only some models offer it, `session` when it is global, `default` when the CLI
    /// also saves it as its default for new sessions ([`FAST_MODE_SCOPE_DEFAULT`]).
    pub scope: Option<String>,
}

/// The fast-mode scope of an agent whose toggle is saved as the CLI's default (Codex writes
/// `service_tier` to `~/.codex/config.toml`), which the picker's Fast button notes with an ⓘ.
pub const FAST_MODE_SCOPE_DEFAULT: &str = "default";

/// One agent's lineup.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatalogAgent {
    pub name: String,
    /// Every effort level the agent knows, in rank order (lowest first).
    pub efforts: Vec<String>,
    pub default_effort: Option<String>,
    pub fast_mode: CatalogFastMode,
    /// Present only when the document declares at least one group.
    pub groups: Vec<CatalogGroup>,
    /// One flat list in display order; grouping is layered on top of it.
    pub models: Vec<CatalogModel>,
}

impl CatalogAgent {
    /// `catalogModel(agent, modelValue)`.
    pub fn model(&self, model_value: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|model| model.value == model_value)
    }

    /// Effort levels offered under a model: the model's own list, or every level the agent knows
    /// while the model is unknown or not in the catalog.
    pub fn efforts_for_model(&self, model_value: &str) -> Vec<String> {
        match self.model(model_value) {
            Some(model) => model.efforts.clone(),
            None => self.efforts.clone(),
        }
    }
}

/// A validated catalog document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentModelCatalog {
    pub schema_version: i64,
    /// ISO date; the newer of two catalogs wins.
    pub updated_at: String,
    /// Effort id to the label shown in the selector.
    pub effort_labels: BTreeMap<String, String>,
    pub agents: BTreeMap<String, CatalogAgent>,
}

impl AgentModelCatalog {
    /// `agentModelCatalogEffortLabel`.
    ///
    /// CDXC:SessionChat 2026-09-13 DECISION:
    /// User: show xHigh throughout chat, including inside and outside the composer dropdown,
    /// replacing the previous Extra High spelling. Keep the underlying effort value unchanged.
    /// Other effort labels remain Low, Medium, High, Max, Ultracode (Claude Code), or Ultra
    /// (Codex), with normal-sized text.
    pub fn effort_label(&self, effort: &str) -> String {
        if effort == "xhigh" {
            return "xHigh".to_string();
        }
        let source = self
            .effort_labels
            .get(effort)
            .map_or(effort, String::as_str);
        source
            .split(' ')
            .map(sentence_case)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// This catalog without its agents: what [`Self::effort_label`] reads.
    pub fn effort_labels_only(&self) -> AgentModelCatalog {
        AgentModelCatalog {
            schema_version: self.schema_version,
            updated_at: self.updated_at.clone(),
            effort_labels: self.effort_labels.clone(),
            agents: BTreeMap::new(),
        }
    }

    /// `newerAgentModelCatalog`: the newer by `updatedAt`, ties to `candidate`.
    pub fn newer(self, candidate: AgentModelCatalog) -> AgentModelCatalog {
        if candidate.updated_at >= self.updated_at {
            candidate
        } else {
            self
        }
    }
}

/// `sentenceCase`: first character up, the rest untouched.
fn sentence_case(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        None => text.to_string(),
        // `String.prototype.charAt(0)` is one UTF-16 code unit; `toUpperCase` on a lone surrogate
        // is that surrogate, and on every other first character it is what `to_uppercase` gives.
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
    }
}

/// `truncateAgentModelLabel`: cuts a long model label for a pill, keeping whole words where it
/// can and closing with an ellipsis. The full label belongs in the pill's tooltip.
pub fn truncate_agent_model_label(label: &str, max_chars: usize) -> String {
    let trimmed = js_trim(label);
    let units: Vec<u16> = trimmed.encode_utf16().collect();
    if units.len() <= max_chars {
        return trimmed;
    }
    let budget = max_chars.saturating_sub(1).max(1);
    let head = &units[..budget.min(units.len())];
    let last_space = head.iter().rposition(|unit| *unit == u16::from(b' '));
    let cut = match last_space {
        // `lastSpace >= budget / 2` with JavaScript's fractional division.
        Some(index) if (index as f64) >= budget as f64 / 2.0 => &head[..index],
        _ => head,
    };
    let text = String::from_utf16_lossy(cut);
    format!("{}\u{2026}", trim_end_js(&text))
}

/// `String.prototype.trim`, which strips the JavaScript whitespace set.
fn js_trim(text: &str) -> String {
    text.trim_matches(is_js_whitespace).to_string()
}

fn trim_end_js(text: &str) -> &str {
    text.trim_end_matches(is_js_whitespace)
}

/// The characters `String.prototype.trim` removes: WhiteSpace plus LineTerminator.
fn is_js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{9}'..='\u{d}'
            | '\u{20}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

/// `parseAgentModelCatalog`: a complete, well-formed document, or `None`.
pub fn parse_agent_model_catalog(input: &Value) -> Option<AgentModelCatalog> {
    let object = input.as_object()?;
    // `input.schemaVersion !== AGENT_MODEL_CATALOG_SCHEMA_VERSION`, over a JSON number: a stored
    // catalog whose version was written `1.0` is the same document, and refusing it threw the
    // whole lineup away.
    if js_number_of(object.get("schemaVersion")) != Some(AGENT_MODEL_CATALOG_SCHEMA_VERSION as f64)
    {
        return None;
    }
    let updated_at = optional_string(object.get("updatedAt"))?;
    let agents_input = object.get("agents")?.as_object()?;
    let labels_input = object.get("effortLabels")?.as_object()?;
    let mut effort_labels = BTreeMap::new();
    for (effort, label) in labels_input {
        let label = label.as_str()?;
        if js_trim(label).is_empty() {
            return None;
        }
        effort_labels.insert(effort.clone(), label.to_string());
    }
    let mut agents = BTreeMap::new();
    for (agent_id, agent_input) in agents_input {
        agents.insert(agent_id.clone(), parse_agent(agent_input)?);
    }
    Some(AgentModelCatalog {
        schema_version: AGENT_MODEL_CATALOG_SCHEMA_VERSION,
        updated_at,
        effort_labels,
        agents,
    })
}

fn parse_agent(input: &Value) -> Option<CatalogAgent> {
    let object = input.as_object()?;
    let name = optional_string(object.get("name"))?;
    let efforts = string_list(object.get("efforts"))?;
    let groups = parse_groups(object.get("groups"))?;
    let models_input = object.get("models")?.as_array()?;
    let mut models = Vec::with_capacity(models_input.len());
    for entry in models_input {
        let model = parse_model(entry, &efforts)?;
        // A row pointing at a group the agent never declares is an authoring mistake, and
        // rendering it flat would hide it; reject the document so the last good catalog stays.
        if let Some(group) = &model.group {
            if !groups.iter().any(|entry| &entry.id == group) {
                return None;
            }
        }
        models.push(model);
    }
    let mut groups = groups;
    if object.get("legacyRule").and_then(Value::as_str) == Some(LEGACY_RULE_NEWEST_PER_FAMILY) {
        group_older_versions(&mut groups, &mut models);
    }
    let fast_mode_input = object.get("fastMode").and_then(Value::as_object);
    let fast_mode = CatalogFastMode {
        available: fast_mode_input
            .and_then(|input| input.get("available"))
            .and_then(Value::as_bool)
            == Some(true),
        command: fast_mode_input
            .and_then(|input| optional_string(input.get("command")))
            .filter(|command| !command.is_empty()),
        scope: fast_mode_input.and_then(|input| optional_string(input.get("scope"))),
    };
    Some(CatalogAgent {
        name,
        efforts,
        default_effort: optional_string(object.get("defaultEffort")),
        fast_mode,
        groups,
        models,
    })
}

/// The `legacyRule` gxserver sets on a lineup it read from the agent's CLI.
pub const LEGACY_RULE_NEWEST_PER_FAMILY: &str = "newestPerFamily";
/// The group older models move into; the model menu draws it as its "Legacy models" list.
pub const LEGACY_GROUP_ID: &str = "legacy";
pub const LEGACY_GROUP_LABEL: &str = "Legacy models";

/// CDXC:AgentProviders 2026-10-17 DECISION:
/// User: for the model lists read from the CLIs (Antigravity, Codex, Cursor, then Grok) "we should have 'legacy models' menu and just show the new ones at the top". No CLI flags a model as old (Codex's `visibility` only hides internal models, which gxserver drops), so the rule reads the names: a model's family is its name without the version number ("GPT 6.1 Sol" is GPT Sol, version 6.1), only the newest version of each family stays at the top, and the older ones move, in their order, into the Legacy models group after them. A name without a version number (Auto, GPT-OSS 120B, Muse Wonder) stays at the top.
/// SEE-ALSO: server/src/agent_model_lineups.rs (the lineups that carry the rule), packages/gx-chat-core/src/menus/picker/model_menu.rs (the menu's Legacy models list).
fn group_older_versions(groups: &mut Vec<CatalogGroup>, models: &mut Vec<CatalogModel>) {
    let keys: Vec<Option<(String, Vec<u32>)>> = models
        .iter()
        .map(|model| family_and_version(&model.label))
        .collect();
    let mut newest: BTreeMap<&str, &Vec<u32>> = BTreeMap::new();
    for (family, version) in keys.iter().flatten() {
        let entry = newest.entry(family.as_str()).or_insert(version);
        if version > *entry {
            *entry = version;
        }
    }
    let older: Vec<bool> = models
        .iter()
        .zip(&keys)
        .map(|(model, key)| {
            model.group.is_none()
                && key
                    .as_ref()
                    .is_some_and(|(family, version)| newest[family.as_str()] > version)
        })
        .collect();
    if !older.contains(&true) {
        return;
    }
    let (mut current, mut legacy): (Vec<_>, Vec<_>) = std::mem::take(models)
        .into_iter()
        .zip(older)
        .partition(|(_, older)| !older);
    models.extend(current.drain(..).map(|(model, _)| model));
    models.extend(legacy.drain(..).map(|(mut model, _)| {
        model.group = Some(LEGACY_GROUP_ID.to_string());
        model
    }));
    if !groups.iter().any(|group| group.id == LEGACY_GROUP_ID) {
        groups.push(CatalogGroup {
            id: LEGACY_GROUP_ID.to_string(),
            label: LEGACY_GROUP_LABEL.to_string(),
            description: None,
        });
    }
}

/// A model name's family (its other words, lower case) and its version number: the first word of
/// digits and dots. `None` for a name without one.
fn family_and_version(label: &str) -> Option<(String, Vec<u32>)> {
    let mut family = Vec::new();
    let mut version = None;
    for word in label.split([' ', '-']).filter(|word| !word.is_empty()) {
        if version.is_none()
            && word.starts_with(|ch: char| ch.is_ascii_digit())
            && word.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
        {
            version = word
                .split('.')
                .map(|part| part.parse::<u32>().ok())
                .collect::<Option<Vec<u32>>>();
            if version.is_some() {
                continue;
            }
        }
        family.push(word.to_lowercase());
    }
    Some((family.join(" "), version?))
}

fn parse_groups(input: Option<&Value>) -> Option<Vec<CatalogGroup>> {
    let Some(input) = input else {
        return Some(Vec::new());
    };
    if input.is_null() {
        // `undefined` is the only value the TypeScript short-circuits; a JSON `null` is not an
        // array and rejects the document.
        return None;
    }
    let entries = input.as_array()?;
    let mut groups: Vec<CatalogGroup> = Vec::with_capacity(entries.len());
    for entry in entries {
        let object = entry.as_object()?;
        let id = optional_string(object.get("id"))?;
        let label = optional_string(object.get("label"))?;
        if groups.iter().any(|group| group.id == id) {
            return None;
        }
        groups.push(CatalogGroup {
            id,
            label,
            description: optional_string(object.get("description")),
        });
    }
    Some(groups)
}

fn parse_model(input: &Value, agent_efforts: &[String]) -> Option<CatalogModel> {
    let object = input.as_object()?;
    let value = optional_string(object.get("value"))?;
    let label = optional_string(object.get("label"))?;
    let efforts = match object.get("efforts") {
        None => agent_efforts.to_vec(),
        Some(efforts) => string_list(Some(efforts))?,
    };
    let picker_label = optional_string(object.get("pickerLabel")).filter(|picker| picker != &label);
    let terminal_labels = match object.get("terminalLabels") {
        None => Vec::new(),
        Some(labels) => string_list(Some(labels))?,
    };
    Some(CatalogModel {
        value,
        label,
        picker_label,
        description: optional_string(object.get("description")),
        efforts,
        default_effort: optional_string(object.get("defaultEffort")),
        fast_mode: object.get("fastMode").and_then(Value::as_bool) == Some(true),
        default: object.get("default").and_then(Value::as_bool) == Some(true),
        group: optional_string(object.get("group")),
        terminal_labels,
    })
}

/// `optionalString`: a non-blank string, else absent.
fn optional_string(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?;
    if js_trim(text).is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// `stringList`: an array of non-blank strings, or a rejected document.
fn string_list(value: Option<&Value>) -> Option<Vec<String>> {
    let entries = value?.as_array()?;
    let mut list = Vec::with_capacity(entries.len());
    for entry in entries {
        let text = entry.as_str()?;
        if js_trim(text).is_empty() {
            return None;
        }
        list.push(text.to_string());
    }
    Some(list)
}

/// The `agent-model-catalog.json` snapshot bundled with the build, the lineup a host falls back to
/// before gxserver pushes a newer one.
pub fn bundled_agent_model_catalog() -> Option<AgentModelCatalog> {
    const BUNDLED: &str = include_str!("../../../../agent-model-catalog.json");
    parse_agent_model_catalog(&serde_json::from_str::<Value>(BUNDLED).ok()?)
}
