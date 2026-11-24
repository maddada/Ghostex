//! The published agent model catalog, as the option pills read it.
//!
//! Ported from the deleted `packages/shared/agent-model-catalog.ts`. The catalog is authored outside the app and
//! refreshed at runtime, so it arrives as JSON and is validated here: anything that is not a
//! complete, well-formed document of the supported schema version is rejected whole, and the last
//! good catalog stays in effect.
//!
//! This module names no model, only Claude's model families, whose order every list keeps
//! ([`order_claude_models`]). Otherwise it only says what shape a catalog has and how its labels read.

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

    /// Resolves a model value, display label, raw API slug (`claude-opus-5-5`), or flattened
    /// `<model>-<effort>` slug (`gemini-4-neon-high`) to its catalog row and any embedded effort.
    pub fn resolve_model(&self, raw: &str) -> Option<(&CatalogModel, Option<String>)> {
        resolve_model_in_list(
            &self.models,
            raw,
            |m| m.value.as_str(),
            |m| m.label.as_str(),
            |m| m.picker_label.as_deref(),
            |m| m.terminal_labels.as_slice(),
        )
    }

    /// Effort levels offered under a model: the model's own list, or every level the agent knows
    /// while the model is unknown or not in the catalog.
    pub fn efforts_for_model(&self, model_value: &str) -> Vec<String> {
        match self.resolve_model(model_value) {
            Some((model, _)) => model.efforts.clone(),
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
        let mut agent = parse_agent(agent_input)?;
        if agent_id == CLAUDE_AGENT_ID {
            group_older_versions(&mut agent.groups, &mut agent.models);
            order_claude_models(
                &mut agent.models,
                |model| &model.label,
                |model| model.group.as_deref(),
            );
        }
        agents.insert(agent_id.clone(), agent);
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

/// The catalog key of Claude Code's lineup.
pub const CLAUDE_AGENT_ID: &str = "claude";
/// Claude's model families, in the order every list of Claude's models shows them.
pub const CLAUDE_FAMILY_ORDER: [&str; 4] = ["fable", "opus", "sonnet", "haiku"];

/// CDXC:AgentModelCatalog 2026-11-11 DECISION:
/// User: "on desktorp and mobile switch order so fable is always top then opus then sonnet then haiku". Claude's rows are ordered by family (Fable, Opus, Sonnet, Haiku, then any other in the catalog's order), then newest version first, so a future Fable or Opus keeps its family's place without an edit; the catalog's own order no longer decides. Ungrouped rows come first and each group (Legacy models) keeps its rows together after them, in the same order. Only Claude Code's lineup is sorted: other agents that offer Claude models keep their catalog order.
/// SEE-ALSO: server/src/agent_model_catalog.rs (`rebuild_effective` sorts the document gxserver serves the same way), apps/mobile/app/src/screens/sessions-screen/NewCoordinatorSheet.tsx (the phone's New Coordinator form, which reads gxserver's copy).
pub fn order_claude_models<T>(
    models: &mut Vec<T>,
    label: impl Fn(&T) -> &str,
    group: impl Fn(&T) -> Option<&str>,
) {
    let mut groups: Vec<String> = Vec::new();
    let mut keyed: Vec<((usize, usize, std::cmp::Reverse<Vec<u32>>), T)> = std::mem::take(models)
        .into_iter()
        .map(|model| {
            let slot = match group(&model) {
                None => 0,
                Some(id) => {
                    1 + groups
                        .iter()
                        .position(|seen| seen == id)
                        .unwrap_or_else(|| {
                            groups.push(id.to_string());
                            groups.len() - 1
                        })
                }
            };
            let (rank, version) = claude_family_and_version(label(&model));
            ((slot, rank, std::cmp::Reverse(version)), model)
        })
        .collect();
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    models.extend(keyed.into_iter().map(|(_, model)| model));
}

/// A Claude model name's place in [`CLAUDE_FAMILY_ORDER`] (past its end for another family) and
/// its version number (none for another family, so those keep the catalog's order).
fn claude_family_and_version(label: &str) -> (usize, Vec<u32>) {
    let words: Vec<String> = label.split([' ', '-']).map(str::to_lowercase).collect();
    let Some(rank) = CLAUDE_FAMILY_ORDER
        .iter()
        .position(|family| words.iter().any(|word| word == family))
    else {
        return (CLAUDE_FAMILY_ORDER.len(), Vec::new());
    };
    let version = family_and_version(label)
        .map(|(_, version)| version)
        .unwrap_or_default();
    (rank, version)
}

/// The `legacyRule` gxserver sets on a lineup it read from the agent's CLI.
pub const LEGACY_RULE_NEWEST_PER_FAMILY: &str = "newestPerFamily";
/// The group older models move into; the model menu draws it as its "Legacy models" list.
pub const LEGACY_GROUP_ID: &str = "legacy";
pub const LEGACY_GROUP_LABEL: &str = "Legacy models";

/// CDXC:AgentProviders 2026-10-17 DECISION:
/// User: for the model lists read from the CLIs (Antigravity, Codex, Cursor, then Grok) "we should have 'legacy models' menu and just show the new ones at the top". No CLI flags a model as old (Codex's `visibility` only hides internal models, which gxserver drops), so the rule reads the names: a model's family is its name without the version number ("GPT 6.1 Sol" is GPT Sol, version 6.1), only the newest version of each family stays at the top, and the older ones move, in their order, into the Legacy models group after them. A name without a version number (Auto, GPT-OSS 120B, Muse Wonder) stays at the top.
/// SEE-ALSO: server/src/agent_model_lineups.rs (the lineups that carry the rule), packages/gx-chat-core/src/menus/picker/model_menu.rs (the menu's Legacy models list).
///
/// CDXC:AgentModelCatalog 2026-11-12 DECISION:
/// User: "i said to make Fable 5.2 top and hide 5.1 under legacy like twice already PLEASE FIX". Claude Code's lineup always gets this rule too (the core when it parses the catalog, gxserver in the copy it serves), whatever the file says: every older version of a family is a Legacy model, so a new Fable 6 row moves Fable 5.2 into Legacy models with no edit to its row.
fn group_older_versions(groups: &mut Vec<CatalogGroup>, models: &mut Vec<CatalogModel>) {
    let older: Vec<bool> = older_versions(models.iter().map(|model| model.label.as_str()))
        .into_iter()
        .zip(models.iter())
        .map(|(older, model)| older && model.group.is_none())
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

/// For each label, whether a newer version of its family is in the list (by
/// [`family_and_version`]); a label without a version number is never older.
pub fn older_versions<'a>(labels: impl Iterator<Item = &'a str>) -> Vec<bool> {
    let keys: Vec<Option<(String, Vec<u32>)>> = labels.map(family_and_version).collect();
    let mut newest: BTreeMap<&str, &Vec<u32>> = BTreeMap::new();
    for (family, version) in keys.iter().flatten() {
        let entry = newest.entry(family.as_str()).or_insert(version);
        if version > *entry {
            *entry = version;
        }
    }
    keys.iter()
        .map(|key| {
            key.as_ref()
                .is_some_and(|(family, version)| newest[family.as_str()] > version)
        })
        .collect()
}

/// A model name's family (its other words, lower case) and its version number: the first word of
/// digits and dots. `None` for a name without one. gxserver picks Chief of Staff's newest model
/// with it (server/src/chief/model.rs).
pub fn family_and_version(label: &str) -> Option<(String, Vec<u32>)> {
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

/// Canonical model-catalog family key for an agent id or icon (`"antigravity-cli"` -> `"antigravity"`).
pub fn canonical_model_family(agent_id: &str) -> &str {
    match agent_id.trim() {
        "claude" | "claude-code" | "openclaude" => "claude",
        "codex" | "codex-cli" => "codex",
        "antigravity" | "antigravity-cli" | "agy" => "antigravity",
        "cursor" | "cursor-cli" | "cursor-agent" => "cursor",
        "grok" | "grok-build" => "grok",
        "hermes" | "hermes-agent" => "hermes",
        "dsh" | "dsh-tui" => "dsh",
        other => other,
    }
}

/// CDXC:SessionChat 2026-11-24 DECISION:
/// User: "never ever ever show 'Default' or some crap like that in the model selector. It must
/// mention the model." Every model selector and label treats these values as unresolved aliases
/// that must resolve to a concrete model instead of rendering verbatim.
pub fn is_placeholder_model_value(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized.is_empty()
        || matches!(
            normalized.as_str(),
            "default" | "auto" | "recommended" | "model" | "agent default"
        )
        || normalized.starts_with("default (")
        || normalized.starts_with("default ")
}

/// True when `label` is a generic placeholder ("Default", "Default (recommended)", "Auto",
/// "Agent default", "OpenCode's default", "Model", or blank) rather than a concrete model name.
pub fn is_placeholder_model_label(label: &str) -> bool {
    let normalized = label.trim().to_ascii_lowercase();
    normalized.is_empty()
        || matches!(
            normalized.as_str(),
            "default" | "auto" | "recommended" | "model" | "agent default"
        )
        || normalized.starts_with("default (")
        || normalized.starts_with("default ")
        || normalized.ends_with("'s default")
        || normalized.ends_with("\u{2019}s default")
}

/// Preferred family words for agents with an explicit default-model rule:
/// Claude Code -> newest Opus; Codex -> newest Sol; Antigravity -> newest Neon.
fn preferred_default_family_words(family: &str) -> Option<&'static [&'static str]> {
    match canonical_model_family(family) {
        "claude" => Some(&["opus"]),
        "codex" => Some(&["gpt", "sol"]),
        "antigravity" => Some(&["gemini", "neon"]),
        _ => None,
    }
}

fn newest_catalog_model_matching<'a>(
    models: &[&'a CatalogModel],
    words: &[&str],
    exact: bool,
) -> Option<&'a CatalogModel> {
    let mut best: Option<(&'a CatalogModel, Vec<u32>)> = None;
    for model in models {
        if is_placeholder_model_value(&model.value) || is_placeholder_model_label(&model.label) {
            continue;
        }
        let Some((family, version)) =
            family_and_version(&model.label).or_else(|| family_and_version(&model.value))
        else {
            continue;
        };
        let family_words: Vec<&str> = family.split_whitespace().collect();
        if !words.iter().all(|word| family_words.contains(word))
            || (exact && family_words.len() != words.len())
        {
            continue;
        }
        if best.as_ref().is_none_or(|(_, newest)| version > *newest) {
            best = Some((model, version));
        }
    }
    best.map(|(model, _)| model)
}

fn newest_catalog_model_of<'a>(
    models: &[&'a CatalogModel],
    words: &[&str],
) -> Option<&'a CatalogModel> {
    newest_catalog_model_matching(models, words, true)
        .or_else(|| newest_catalog_model_matching(models, words, false))
        .or_else(|| {
            // If only a single keyword matches (e.g. a future "Sol" or "Neon" without "GPT"/"Gemini"),
            // pick the newest row containing the distinguishing family word.
            words
                .last()
                .and_then(|tail| newest_catalog_model_matching(models, &[*tail], false))
        })
}

/// The concrete default model for `family` from its reported model lineup:
/// - Claude Code: newest Opus available (today Opus 5.5)
/// - Codex: newest Sol model available (today GPT 6.2 Sol)
/// - Antigravity: newest Neon model available (today Gemini 4 Neon)
/// - Other agents: the lineup's non-placeholder `default: true` row, else its first non-legacy
///   concrete row, else its first concrete row.
pub fn preferred_default_model<'a>(
    family: &str,
    agent: &'a CatalogAgent,
) -> Option<&'a CatalogModel> {
    let all: Vec<&CatalogModel> = agent
        .models
        .iter()
        .filter(|m| !is_placeholder_model_value(&m.value) && !is_placeholder_model_label(&m.label))
        .collect();
    if all.is_empty() {
        return None;
    }
    let current: Vec<&CatalogModel> = all
        .iter()
        .copied()
        .filter(|m| m.group.as_deref() != Some(LEGACY_GROUP_ID))
        .collect();
    let pool = if current.is_empty() { &all } else { &current };
    preferred_default_family_words(family)
        .and_then(|words| {
            newest_catalog_model_of(pool, words).or_else(|| newest_catalog_model_of(&all, words))
        })
        .or_else(|| pool.iter().copied().find(|m| m.default))
        .or_else(|| pool.first().copied())
        .or_else(|| all.first().copied())
}

/// Hard-coded fallback `(value, label, default_effort)` used ONLY when an agent has no reported
/// model list at all.
pub fn preferred_default_model_fallback(
    family: &str,
) -> Option<(&'static str, &'static str, Option<&'static str>)> {
    match canonical_model_family(family) {
        "claude" => Some(("opus[1m]", "Opus 5.5", Some("high"))),
        "codex" => Some(("gpt-6.2-sol", "GPT 6.2 Sol", Some("medium"))),
        "antigravity" => Some(("gemini-4-neon", "Gemini 4 Neon", Some("high"))),
        _ => None,
    }
}

/// Default reasoning effort for `model` on `agent` when the user has not chosen one explicitly.
pub fn preferred_default_effort(
    family: &str,
    agent: &CatalogAgent,
    model: &CatalogModel,
) -> Option<String> {
    let offered = |effort: &str| model.efforts.iter().any(|candidate| candidate == effort);
    if let Some(default_effort) = model.default_effort.as_deref().filter(|e| offered(e)) {
        return Some(default_effort.to_string());
    }
    if let Some(default_effort) = agent.default_effort.as_deref().filter(|e| offered(e)) {
        return Some(default_effort.to_string());
    }
    if matches!(
        canonical_model_family(family),
        "claude" | "codex" | "antigravity"
    ) && offered("high")
    {
        return Some("high".to_string());
    }
    model.efforts.first().cloned()
}

/// Resolves the concrete default `(value, label, effort)` for `family` from `catalog`, falling
/// back to [`preferred_default_model_fallback`] only when the catalog has no models for it.
pub fn resolved_default_model(
    catalog: &AgentModelCatalog,
    family: &str,
) -> Option<(String, String, Option<String>)> {
    let canonical = canonical_model_family(family);
    if let Some(agent) = catalog.agents.get(canonical) {
        if let Some(model) = preferred_default_model(canonical, agent) {
            let effort = preferred_default_effort(canonical, agent, model);
            return Some((model.value.clone(), model.label.clone(), effort));
        }
    }
    preferred_default_model_fallback(canonical).map(|(value, label, effort)| {
        (
            value.to_string(),
            label.to_string(),
            effort.map(str::to_string),
        )
    })
}

/// Normalizes a reasoning effort token to its canonical lowercase wire id.
pub fn normalize_effort_token(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "minimal" => Some("minimal"),
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        "xhigh" | "extra high" | "extra-high" => Some("xhigh"),
        "max" => Some("max"),
        "ultra" => Some("ultra"),
        "ultracode" => Some("ultracode"),
        _ => None,
    }
}

/// If `label` ends with a parenthesized reasoning effort such as `"Opus 5.5 (High)"` or
/// `"Gemini 4 Neon (Medium)"`, splits it into `("Opus 5.5", Some("high"))` (canonical lowercase
/// effort id) so effort always renders as separate dimmed text, never in brackets.
pub fn split_parenthesized_effort(label: &str) -> (&str, Option<String>) {
    let trimmed = label.trim();
    let Some(without_close) = trimmed.strip_suffix(')') else {
        return (trimmed, None);
    };
    let Some((base, inside)) = without_close.rsplit_once(" (") else {
        return (trimmed, None);
    };
    let base = base.trim();
    if base.is_empty() {
        return (trimmed, None);
    }
    match normalize_effort_token(inside) {
        Some(effort) => (base, Some(effort.to_string())),
        None => (trimmed, None),
    }
}

/// Parses a Claude API model slug such as `"claude-opus-5-5"`, `"claude-opus-5-5[1m]"`,
/// `"claude-opus-5-5-20260514"`, or `"Claude-sonnet-5-5"` into its friendly display label
/// (`"Opus 5.5"`) and optional bracket variant (`Some("1m")`).
pub fn parse_claude_api_slug(raw: &str) -> Option<(String, Option<String>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains(' ') {
        return None;
    }
    let normalized = trimmed.to_ascii_lowercase();
    let variant = normalized
        .split_once('[')
        .and_then(|(_, tail)| tail.strip_suffix(']'))
        .map(str::to_string);
    let head = normalized
        .split_once('[')
        .map_or(normalized.as_str(), |(h, _)| h);
    let tokens: Vec<&str> = head.split('-').filter(|t| !t.is_empty()).collect();
    if tokens.first().copied() != Some("claude") {
        return None;
    }
    let (family_index, family) =
        tokens
            .iter()
            .enumerate()
            .find_map(|(index, token)| match *token {
                "fable" | "opus" | "sonnet" | "haiku" => Some((index, *token)),
                _ => None,
            })?;
    let title = match family {
        "fable" => "Fable",
        "opus" => "Opus",
        "sonnet" => "Sonnet",
        "haiku" => "Haiku",
        _ => return None,
    };
    let following_version: Vec<&str> = tokens
        .iter()
        .skip(family_index + 1)
        .copied()
        .take_while(|token| {
            token.len() <= 2 && !token.is_empty() && token.chars().all(|ch| ch.is_ascii_digit())
        })
        .take(2)
        .collect();
    let preceding_version: Vec<&str> = tokens
        .iter()
        .take(family_index)
        .rev()
        .copied()
        .take_while(|token| {
            token.len() <= 2 && !token.is_empty() && token.chars().all(|ch| ch.is_ascii_digit())
        })
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let version = if following_version.is_empty() {
        preceding_version
    } else {
        following_version
    };
    let label = if version.is_empty() {
        title.to_string()
    } else {
        format!("{title} {}", version.join("."))
    };
    Some((label, variant))
}

/// Converts a raw hyphenated API/CLI model slug (`"claude-opus-5-5"`, `"Claude-opus-5-5"`,
/// `"gpt-6.2-sol"`, `"gemini-4-neon"`) into a clean display name (`"Opus 5.5"`, `"GPT 6.2 Sol"`,
/// `"Gemini 4 Neon"`). Returns `None` for already-human labels or unrecognized ids.
pub fn humanize_raw_model_slug(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains(' ') || !trimmed.contains('-') {
        return None;
    }
    if let Some((label, _)) = parse_claude_api_slug(trimmed) {
        return Some(label);
    }
    let lower = trimmed.to_ascii_lowercase();
    let head = lower.split_once('[').map_or(lower.as_str(), |(h, _)| h);
    let tokens: Vec<&str> = head.split('-').filter(|t| !t.is_empty()).collect();
    if tokens.len() < 2 {
        return None;
    }
    let has_version = tokens
        .iter()
        .any(|t| t.starts_with(|c: char| c.is_ascii_digit()));
    if !has_version {
        return None;
    }
    match tokens[0] {
        "gpt" => {
            let mut out = vec!["GPT".to_string()];
            for token in &tokens[1..] {
                out.push(format_slug_token(token));
            }
            Some(out.join(" "))
        }
        "gemini" => {
            let mut end = tokens.len();
            if end > 2 && normalize_effort_token(tokens[end - 1]).is_some() {
                end -= 1;
            }
            let mut out = vec!["Gemini".to_string()];
            for token in &tokens[1..end] {
                out.push(format_slug_token(token));
            }
            Some(out.join(" "))
        }
        _ => None,
    }
}

fn format_slug_token(token: &str) -> String {
    match token {
        "codex" => "Codex".to_string(),
        "oss" => "OSS".to_string(),
        "pro" => "Pro".to_string(),
        "flash" => "Flash".to_string(),
        "mini" => "Mini".to_string(),
        other if other.starts_with(|c: char| c.is_ascii_digit()) => other.to_string(),
        other => sentence_case(other),
    }
}

fn normalized_model_tokens(text: &str) -> Vec<String> {
    text.split([' ', '-', '_'])
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Resolves `raw` (exact id, label, raw API slug like `claude-opus-5-5`, bracketed context like
/// `Opus 5.5 (1M)`, parenthesized effort like `Opus 5.5 (High)`, or flattened effort slug like
/// `gemini-4-neon-high`) against an arbitrary slice of model rows.
pub fn resolve_model_in_list<'a, T>(
    items: &'a [T],
    raw: &str,
    value_of: impl Fn(&T) -> &str,
    label_of: impl Fn(&T) -> &str,
    picker_label_of: impl Fn(&T) -> Option<&str>,
    terminal_labels_of: impl Fn(&T) -> &[String],
) -> Option<(&'a T, Option<String>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(found) = items.iter().find(|item| value_of(item) == trimmed) {
        return Some((found, None));
    }
    // Strip trailing parenthesized effort ("Opus 5.5 (High)", "Gemini 4 Neon (Medium)") up front.
    let (candidate, paren_effort) = split_parenthesized_effort(trimmed);
    let find_exact_ci = |needle: &str| -> Option<&'a T> {
        items.iter().find(|item| {
            value_of(item).eq_ignore_ascii_case(needle)
                || label_of(item).eq_ignore_ascii_case(needle)
                || picker_label_of(item).is_some_and(|p| p.eq_ignore_ascii_case(needle))
                || terminal_labels_of(item)
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(needle))
        })
    };
    if let Some(found) = find_exact_ci(candidate) {
        return Some((found, paren_effort));
    }
    // Strip trailing context marker ("Opus 5.5 (1M)" / "Opus 5.5 (1M context)").
    for suffix in [" (1M context)", " (1m context)", " (1M)", " (1m)"] {
        if let Some(base) = candidate.strip_suffix(suffix) {
            if let Some(found) = items
                .iter()
                .find(|item| {
                    value_of(item).ends_with("[1m]") && label_of(item).eq_ignore_ascii_case(base)
                })
                .or_else(|| find_exact_ci(base))
            {
                return Some((found, paren_effort));
            }
        }
    }
    // Strip trailing flattened effort suffix ("gemini-4-neon-high").
    if let Some((base, suffix)) = candidate.rsplit_once('-') {
        if let Some(effort) = normalize_effort_token(suffix) {
            if let Some(found) = find_exact_ci(base) {
                return Some((found, paren_effort.or_else(|| Some(effort.to_string()))));
            }
        }
    }
    // Claude API slug ("claude-opus-5-5", "claude-opus-5-5[1m]", "claude-sonnet-5-5-2026...").
    if let Some((friendly_label, variant)) = parse_claude_api_slug(candidate) {
        let prefer_long = variant.as_deref() == Some("1m");
        if prefer_long {
            if let Some(found) = items.iter().find(|item| {
                value_of(item).ends_with("[1m]")
                    && label_of(item).eq_ignore_ascii_case(&friendly_label)
            }) {
                return Some((found, paren_effort));
            }
        }
        if let Some(found) = items
            .iter()
            .find(|item| label_of(item).eq_ignore_ascii_case(&friendly_label))
        {
            return Some((found, paren_effort));
        }
    }
    // Bare alias whose catalog row only exists with `[1m]` (e.g. `opus` -> `opus[1m]`).
    if !candidate.to_ascii_lowercase().ends_with("[1m]") {
        let long = format!("{candidate}[1m]");
        if let Some(found) = items
            .iter()
            .find(|item| value_of(item).eq_ignore_ascii_case(&long))
        {
            return Some((found, paren_effort));
        }
    }
    // Normalized token match ("gpt-6.2-sol" -> "GPT 6.2 Sol", "gemini-4-neon" -> "Gemini 4 Neon").
    let needle_tokens = normalized_model_tokens(candidate);
    if needle_tokens.len() >= 2 {
        if let Some(found) = items.iter().find(|item| {
            normalized_model_tokens(label_of(item)) == needle_tokens
                || normalized_model_tokens(value_of(item)) == needle_tokens
        }) {
            return Some((found, paren_effort));
        }
    }
    None
}
