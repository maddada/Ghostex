//! The local value of every option pill: where it came from, when, and what it reads as.
//!
//! Port of the value half of `packages/core-ui/chat/session-chat-session-options.ts`.
//!
//! CDXC:AgentScreenDetection 2026-08-01 WHY:
//! gxserver reads the agent's structured transcript and terminal statusline and reports what it is
//! REALLY running (`selectedOptions` on read results and snapshot, replaced and state frames).
//! That outranks this surface's local truth, with one exception: a value the user just dispatched
//! keeps the pill for a short grace window, because the TUI may not have repainted yet. A
//! detection that AGREES with a pending dispatch confirms it. Nothing detected means nothing here
//! runs and no current value is claimed.

use std::collections::BTreeMap;

use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::menus::catalog::AgentModelCatalog;
use crate::menus::option_catalog::{
    OptionChoice, OptionChoiceGroup, OptionDescriptor, OptionDispatch, SessionOptionCatalog,
};
use crate::menus::time::{iso_from_millis, parse_iso_millis};

/// How long a just-typed option command outranks a DISAGREEING detection: the TUI needs a moment
/// to repaint, and a probe that catches the old statusline must not flip the pill back. A
/// detection that AGREES confirms immediately, and after the window a disagreement wins (the agent
/// did something else).
pub const SESSION_CHAT_DISPATCH_GRACE_MS: i64 = 10_000;

/// Where a pill's current value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionSource {
    Default,
    Dispatched,
    Detected,
}

impl OptionSource {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Dispatched => "dispatched",
            Self::Detected => "detected",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "default" => Some(Self::Default),
            "dispatched" => Some(Self::Dispatched),
            "detected" => Some(Self::Detected),
            _ => None,
        }
    }
}

impl Serialize for OptionSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.wire())
    }
}

/// One option's local value. The field order is the order the TypeScript wrote it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionValue {
    pub value: String,
    pub source: OptionSource,
    /// The raw text the agent reported (`Fable 5`, an unknown codex id). Only set by a detection;
    /// preferred over the catalog label so the pill shows the real model string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// ISO time this surface typed the option command (source `dispatched`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dispatched_at: Option<String>,
    /// Agent-owned evidence used for a detected value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_source: Option<String>,
    /// ISO time gxserver read the value (source `detected`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_at: Option<String>,
}

impl OptionValue {
    /// A value with nothing but its source.
    pub fn new(value: impl Into<String>, source: OptionSource) -> Self {
        Self {
            value: value.into(),
            source,
            label: None,
            dispatched_at: None,
            detected_source: None,
            detected_at: None,
        }
    }
}

/// Descriptor id to local value, including pending Fast and Plan toggles.
///
/// Insertion order is not observable: the document serializes it as a JSON object, and JSON object
/// key order is not semantic. A sorted map keeps two runs identical.
pub type OptionState = BTreeMap<String, OptionValue>;

/// One run of top-level rows, or one submenu, of a choice list.
#[derive(Clone, Debug, PartialEq)]
pub enum ChoiceSection {
    Choices {
        key: String,
        choices: Vec<OptionChoice>,
    },
    Group {
        key: String,
        group: OptionChoiceGroup,
        choices: Vec<OptionChoice>,
    },
}

impl ChoiceSection {
    pub fn key(&self) -> &str {
        match self {
            Self::Choices { key, .. } | Self::Group { key, .. } => key,
        }
    }

    pub fn choices(&self) -> &[OptionChoice] {
        match self {
            Self::Choices { choices, .. } | Self::Group { choices, .. } => choices,
        }
    }
}

/// `sessionChatOptionChoiceSections`.
///
/// CDXC:AgentProviders 2026-09-05 DECISION:
/// User: the model list can nest rows under a named group, and the order of the published catalog
/// is the order the menu shows.
///
/// So the flat `choices` order is the only ordering input: rows render in it, and a group's
/// submenu takes the place of its FIRST member, carrying every member in that same order. A group
/// is moved by moving its rows, and a descriptor without `choiceGroups` yields exactly one plain
/// run.
pub fn option_choice_sections(descriptor: &OptionDescriptor) -> Vec<ChoiceSection> {
    let groups = descriptor.choice_groups.as_deref().unwrap_or(&[]);
    let mut sections: Vec<ChoiceSection> = Vec::new();
    let mut grouped: BTreeMap<String, usize> = BTreeMap::new();
    for choice in descriptor.choice_list() {
        let group = choice
            .group
            .as_ref()
            .and_then(|id| groups.iter().find(|entry| &entry.id == id));
        let Some(group) = group else {
            match sections.last_mut() {
                Some(ChoiceSection::Choices { choices, .. }) => choices.push(choice.clone()),
                _ => sections.push(ChoiceSection::Choices {
                    key: format!("choices:{}", choice.value),
                    choices: vec![choice.clone()],
                }),
            }
            continue;
        };
        let index = match grouped.get(&group.id) {
            Some(index) => *index,
            None => {
                sections.push(ChoiceSection::Group {
                    key: format!("group:{}", group.id),
                    group: group.clone(),
                    choices: Vec::new(),
                });
                let index = sections.len() - 1;
                grouped.insert(group.id.clone(), index);
                index
            }
        };
        if let ChoiceSection::Group { choices, .. } = &mut sections[index] {
            choices.push(choice.clone());
        }
    }
    sections
}

/// `isTrackedValue`.
fn is_tracked_value(descriptor: &OptionDescriptor, value: &str) -> bool {
    if let OptionDispatch::ToggleCommand { .. } = descriptor.dispatch {
        return if descriptor.id == "fastMode" {
            value == "on" || value == "off"
        } else {
            descriptor.id == "mode" && (value == "plan" || value == "default")
        };
    }
    descriptor
        .choice_list()
        .iter()
        .any(|choice| choice.value == value)
}

/// `sessionChatOptionTracksValue`: a select the pills can label from.
pub fn option_tracks_value(descriptor: &OptionDescriptor) -> bool {
    matches!(
        descriptor.dispatch,
        OptionDispatch::Command(_)
            | OptionDispatch::CommandConfirmPicker(_)
            | OptionDispatch::ModelPicker
            | OptionDispatch::BoundedKeySteps { .. }
            | OptionDispatch::CyclicKeySteps { .. }
    ) && !descriptor.choice_list().is_empty()
}

/// `sessionChatCyclicKeySteps`: exact forward-only key sequence for a cyclic ordered setting.
pub fn cyclic_key_steps(
    choices: &[OptionChoice],
    current_value: Option<&str>,
    target_value: &str,
    key: &str,
) -> Vec<String> {
    let index_of = |wanted: Option<&str>| {
        wanted.and_then(|wanted| {
            choices
                .iter()
                .position(|choice| choice.value.as_str() == wanted)
        })
    };
    let (Some(current), Some(target)) = (index_of(current_value), index_of(Some(target_value)))
    else {
        return Vec::new();
    };
    if choices.len() < 2 {
        return Vec::new();
    }
    let count = (target + choices.len() - current) % choices.len();
    vec![key.to_string(); count]
}

/// `sessionChatBoundedKeySteps`: exact key sequence for a bounded ordered setting.
///
/// With a known current value, send only the delta. Without one, first saturate at the nearer edge
/// and then step inward, so the requested value is deterministic.
pub fn bounded_key_steps(
    choices: &[OptionChoice],
    current_value: Option<&str>,
    target_value: &str,
    decrease_key: &str,
    increase_key: &str,
) -> Vec<String> {
    let Some(target) = choices
        .iter()
        .position(|choice| choice.value == target_value)
    else {
        return Vec::new();
    };
    if choices.len() < 2 {
        return Vec::new();
    }
    let current = current_value.and_then(|wanted| {
        choices
            .iter()
            .position(|choice| choice.value.as_str() == wanted)
    });
    if let Some(current) = current {
        let delta = target as i64 - current as i64;
        let key = if delta > 0 {
            increase_key
        } else {
            decrease_key
        };
        return vec![key.to_string(); delta.unsigned_abs() as usize];
    }
    let last = choices.len() - 1;
    let from_lower_edge = last + target;
    let from_upper_edge = last + (last - target);
    if from_lower_edge <= from_upper_edge {
        let mut keys = vec![decrease_key.to_string(); last];
        keys.extend(vec![increase_key.to_string(); target]);
        keys
    } else {
        let mut keys = vec![increase_key.to_string(); last];
        keys.extend(vec![decrease_key.to_string(); last - target]);
        keys
    }
}

/// `seedSessionChatOptionState`.
pub fn seed_option_state(
    catalog: &SessionOptionCatalog,
    stored: &OptionState,
    now_ms: i64,
) -> OptionState {
    let mut next: OptionState = BTreeMap::new();
    let seed = |next: &mut OptionState, descriptor: &OptionDescriptor| {
        if next.contains_key(&descriptor.id) {
            return;
        }
        if !option_tracks_value(descriptor)
            && !matches!(descriptor.dispatch, OptionDispatch::ToggleCommand { .. })
        {
            return;
        }
        let stored_value = stored.get(&descriptor.id);
        // A persisted detection can be stale after the user changes the agent in the terminal
        // while Chat is unmounted. gxserver will re-confirm it on the seed read; only still
        // pending local intent survives this synchronous reseed.
        if let Some(stored_value) = stored_value {
            let dispatched = stored_value
                .dispatched_at
                .as_deref()
                .and_then(parse_iso_millis);
            if stored_value.source == OptionSource::Dispatched
                && is_tracked_value(descriptor, &stored_value.value)
                && dispatched.is_some_and(|at| at + SESSION_CHAT_DISPATCH_GRACE_MS > now_ms)
            {
                next.insert(descriptor.id.clone(), stored_value.clone());
                return;
            }
        }
        if let Some(default_value) = &descriptor.default_value {
            next.insert(
                descriptor.id.clone(),
                OptionValue::new(default_value.clone(), OptionSource::Default),
            );
        }
    };
    seed(&mut next, &catalog.model);
    let model_value = current_model_value(catalog, &next);
    for descriptor in catalog.options_for_model(&model_value) {
        seed(&mut next, &descriptor);
    }
    next
}

/// `state[catalog.model.id]?.value ?? catalog.model.defaultValue ?? ''`.
pub fn current_model_value(catalog: &SessionOptionCatalog, state: &OptionState) -> String {
    state
        .get(&catalog.model.id)
        .map(|entry| entry.value.clone())
        .or_else(|| catalog.model.default_value.clone())
        .unwrap_or_default()
}

/// `setSessionChatOptionValue`. Returns `None` when nothing changed, which is the TypeScript's
/// identity return.
pub fn set_option_value(
    state: &OptionState,
    descriptor_id: &str,
    value: &str,
    source: OptionSource,
    now_ms: i64,
) -> Option<OptionState> {
    if let Some(current) = state.get(descriptor_id) {
        if current.value == value && current.source == source {
            return None;
        }
    }
    let mut next = OptionValue::new(value, source);
    if source == OptionSource::Dispatched {
        // Stamped so a detection can tell "the user just sent this" from "the agent has been
        // running this for a while".
        next.dispatched_at = Some(iso_from_millis(now_ms));
    }
    let mut updated = state.clone();
    updated.insert(descriptor_id.to_string(), next);
    Some(updated)
}

/// `reconcileSessionChatOptionsFromCommand`.
///
/// A command the USER typed reconciles the pills: `/model opus` makes the model pill read Opus
/// without a second dispatch. Exact match against the catalog's own builders, so an unrelated
/// `/model` argument is ignored.
pub fn reconcile_options_from_command(
    catalog: &SessionOptionCatalog,
    state: &OptionState,
    text: &str,
    now_ms: i64,
) -> OptionState {
    let normalized = collapse_whitespace(text.trim());
    if !normalized.starts_with('/') {
        return state.clone();
    }
    let model_value = current_model_value(catalog, state);
    let mut descriptors = vec![catalog.model.clone()];
    descriptors.extend(catalog.options_for_model(&model_value));
    let mut next = state.clone();
    for descriptor in &descriptors {
        let OptionDispatch::Command(build) = &descriptor.dispatch else {
            continue;
        };
        for choice in descriptor.choice_list() {
            if build.build(&choice.value) == normalized {
                if let Some(updated) = set_option_value(
                    &next,
                    &descriptor.id,
                    &choice.value,
                    OptionSource::Dispatched,
                    now_ms,
                ) {
                    next = updated;
                }
            }
        }
    }
    next
}

/// `text.trim().replace(/\s+/g, ' ')`.
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(character);
            in_space = false;
        }
    }
    out
}

/// One choice gxserver detected on the agent's screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetectedChoice {
    pub value: String,
    pub label: String,
    pub source: Option<String>,
}

/// `SessionChatDetectedOptionInput`: what a `selectedOptions` payload carries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetectedOptions {
    pub model: Option<DetectedChoice>,
    pub effort: Option<DetectedChoice>,
    pub mode: Option<DetectedChoice>,
    pub fast: Option<bool>,
    pub detected_at: String,
}

impl DetectedOptions {
    /// Reads the `selectedOptions` value family a folded, or `None` when there is none.
    pub fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let choice = |key: &str| -> Option<DetectedChoice> {
            let entry = object.get(key)?.as_object()?;
            Some(DetectedChoice {
                value: entry.get("value")?.as_str()?.to_string(),
                label: entry.get("label")?.as_str()?.to_string(),
                source: entry
                    .get("source")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        };
        Some(Self {
            model: choice("model"),
            effort: choice("effort"),
            mode: choice("mode"),
            fast: object.get("fast").and_then(Value::as_bool),
            detected_at: object
                .get("detectedAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
    }
}

/// `sessionChatOptionEvidencePriority`.
///
/// CDXC:AgentScreenDetection 2026-09-08 SEE-ALSO:
/// server/src/session_chat_options.rs owns the evidence precedence; the read path must also admit
/// stronger evidence before filtering older replies.
pub fn evidence_priority(source: Option<&str>) -> i32 {
    match source {
        Some("terminal") => 3,
        Some("statusline") => 2,
        Some("transcript") => 1,
        _ => 0,
    }
}

/// `applyDetectedChoice`.
fn apply_detected_choice(
    state: &OptionState,
    descriptor_id: &str,
    detected: &DetectedChoice,
    detected_at: &str,
) -> Option<OptionState> {
    let current = state.get(descriptor_id);
    let detected_at_ms = parse_iso_millis(detected_at);
    let dispatched_at_ms = current
        .and_then(|entry| entry.dispatched_at.as_deref())
        .and_then(parse_iso_millis);
    let agrees = current.is_some_and(|entry| entry.value == detected.value);
    let current_priority =
        evidence_priority(current.and_then(|entry| entry.detected_source.as_deref()));
    let incoming_priority = evidence_priority(detected.source.as_deref());
    // Reading an old transcript again does not make it stronger evidence than the terminal.
    if current.is_some_and(|entry| entry.source == OptionSource::Detected)
        && current_priority > incoming_priority
    {
        return None;
    }
    if let Some(current_detected_at) = current.and_then(|entry| entry.detected_at.as_deref()) {
        let current_at_ms = parse_iso_millis(current_detected_at);
        if current_priority >= incoming_priority && js_less_than(detected_at_ms, current_at_ms) {
            return None;
        }
    }
    if current.is_some_and(|entry| entry.source == OptionSource::Dispatched) {
        if let (Some(dispatched), Some(detected_ms)) = (dispatched_at_ms, detected_at_ms) {
            // A read taken BEFORE the dispatch is stale by construction; a read taken just after
            // it may have caught the pre-repaint screen.
            if detected_ms < dispatched
                || (!agrees && detected_ms < dispatched + SESSION_CHAT_DISPATCH_GRACE_MS)
            {
                return None;
            }
        }
    }
    if let Some(current) = current {
        if current.source == OptionSource::Detected
            && agrees
            && current.label.as_deref() == Some(detected.label.as_str())
            && current.detected_source.as_deref() == detected.source.as_deref()
            && current.detected_at.as_deref() == Some(detected_at)
        {
            return None;
        }
    }
    let mut next = state.clone();
    next.insert(
        descriptor_id.to_string(),
        OptionValue {
            value: detected.value.clone(),
            source: OptionSource::Detected,
            label: Some(detected.label.clone()),
            dispatched_at: None,
            detected_source: detected.source.clone(),
            detected_at: Some(detected_at.to_string()),
        },
    );
    Some(next)
}

/// `left < right` with JavaScript's `NaN` comparison: an unparsable stamp never compares less.
fn js_less_than(left: Option<i64>, right: Option<i64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left < right,
        _ => false,
    }
}

/// `applySessionChatDetectedOptions`: folds a detection onto the local state.
pub fn apply_detected_options(
    catalog: &SessionOptionCatalog,
    state: &OptionState,
    detected: Option<&DetectedOptions>,
) -> OptionState {
    let Some(detected) = detected else {
        return state.clone();
    };
    let mut next = state.clone();
    let apply = |next: &mut OptionState, id: &str, choice: &DetectedChoice| {
        if let Some(updated) = apply_detected_choice(next, id, choice, &detected.detected_at) {
            *next = updated;
        }
    };
    if let Some(model) = &detected.model {
        let id = catalog.model.id.clone();
        apply(&mut next, &id, model);
    }
    if let Some(effort) = &detected.effort {
        let model_value = current_model_value(catalog, &next);
        // Only when the current model actually has an effort option (Haiku has none).
        let has_effort = catalog
            .options_for_model(&model_value)
            .iter()
            .any(|descriptor| descriptor.id == "effort");
        if has_effort {
            apply(&mut next, "effort", effort);
        }
    }
    let codex_terminal_model = catalog.model_icon == "codex"
        && detected
            .model
            .as_ref()
            .and_then(|model| model.source.as_deref())
            == Some("terminal");
    if let Some(mode) = &detected.mode {
        let model_value = current_model_value(catalog, &next);
        let has_mode = catalog
            .options_for_model(&model_value)
            .iter()
            .any(|descriptor| descriptor.id == "mode");
        if has_mode {
            apply(&mut next, "mode", mode);
        }
    } else if codex_terminal_model {
        // A recognized Codex footer with no Plan marker explicitly means default mode.
        apply(
            &mut next,
            "mode",
            &DetectedChoice {
                value: "default".to_string(),
                label: "Default".to_string(),
                source: Some("terminal".to_string()),
            },
        );
    }
    if detected.fast.is_some() || codex_terminal_model {
        let on = detected.fast == Some(true);
        apply(
            &mut next,
            "fastMode",
            &DetectedChoice {
                value: if on { "on" } else { "off" }.to_string(),
                label: if on { "Fast enabled" } else { "Fast disabled" }.to_string(),
                source: None,
            },
        );
    }
    next
}

/// `sessionChatSentenceCaseLabel`: first letter up, the rest left as the agent reported it.
fn sentence_case_label(label: &str) -> String {
    let mut characters = label.chars();
    match characters.next() {
        None => label.to_string(),
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
    }
}

/// `choiceNamesLabel`: the detected text names this choice by its id, its label, or the CLI's own
/// row text.
fn choice_names_label(choice: &OptionChoice, detected_label: &str) -> bool {
    let needle = detected_label.to_lowercase();
    [
        choice.value.as_str(),
        choice.label.as_str(),
        choice.picker_label.as_deref().unwrap_or(""),
    ]
    .iter()
    .any(|name| name.to_lowercase() == needle)
}

/// `sessionChatOptionValueLabel`: the value's label, or `None` when nothing is known.
///
/// When the detected text names a catalog choice (its id, its label, or the CLI's own row text
/// such as `Claude Opus 5`), the catalog's display label wins, so the published catalog decides
/// how a model is shown. Anything else the agent reports still renders verbatim; lowercase
/// statusline tokens are shown in sentence case.
pub fn option_value_label(
    catalog: &AgentModelCatalog,
    descriptor: &OptionDescriptor,
    state: &OptionState,
) -> Option<String> {
    let current = state.get(&descriptor.id)?;
    if descriptor.id == "effort" {
        return Some(catalog.effort_label(&current.value));
    }
    let choice = descriptor
        .choice_list()
        .iter()
        .find(|entry| entry.value == current.value);
    let detected_label = current.label.as_deref().map(str::trim).unwrap_or("");
    if !detected_label.is_empty() {
        if let Some(choice) = choice {
            if choice_names_label(choice, detected_label) {
                return Some(choice.label.clone());
            }
        }
        return Some(sentence_case_label(detected_label));
    }
    choice.map(|choice| choice.label.clone())
}

/// `sessionChatOptionsPillLabel`: known non-model values joined by " · ".
pub fn options_pill_label(
    catalog: &AgentModelCatalog,
    descriptors: &[OptionDescriptor],
    state: &OptionState,
) -> Option<String> {
    let labels: Vec<String> = descriptors
        .iter()
        .filter(|descriptor| !matches!(descriptor.dispatch, OptionDispatch::ToggleCommand { .. }))
        .filter_map(|descriptor| option_value_label(catalog, descriptor, state))
        .collect();
    if labels.is_empty() {
        None
    } else {
        Some(labels.join(" · "))
    }
}

/// Reads a stored option state, the way `readStoredSessionChatOptions` validates one.
pub fn option_state_from_value(value: &Value) -> OptionState {
    let mut next = OptionState::new();
    let Some(object) = value.as_object() else {
        return next;
    };
    for (id, entry) in object {
        let Some(entry) = entry.as_object() else {
            continue;
        };
        let Some(value) = entry.get("value").and_then(Value::as_str) else {
            continue;
        };
        let Some(source) = entry
            .get("source")
            .and_then(Value::as_str)
            .and_then(OptionSource::from_wire)
        else {
            continue;
        };
        let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_string);
        next.insert(
            id.clone(),
            OptionValue {
                value: value.to_string(),
                source,
                label: text("label").filter(|label| !label.is_empty()),
                dispatched_at: text("dispatchedAt"),
                detected_source: text("detectedSource").filter(|source| {
                    matches!(source.as_str(), "terminal" | "transcript" | "statusline")
                }),
                detected_at: text("detectedAt"),
            },
        );
    }
    next
}
