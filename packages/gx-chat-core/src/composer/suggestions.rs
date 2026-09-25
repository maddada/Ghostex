//! The `/`, `$` and `@` popup: which list is open, what it shows, and what a pick writes.
//!
//! Port of `packages/shared/session-chat-presentation/composer-suggestions.ts` and
//! `packages/shared/session-chat-controller/native-suggestions.ts`. The popup's own geometry stays
//! in `packages/gx-chat-core/visual/composer-suggestions.json`, which both renderers
//! read directly.

use serde::{Deserialize, Serialize};

use crate::composer::references::next_file_reference_index;
use crate::composer::slash_commands::{
    filter_slash_commands, slash_commands_for_agent, slash_heading_for_agent, slash_query,
    SlashCommand,
};
use crate::composer::text::Utf16Text;
use crate::composer::trigger::{
    detect_composer_trigger, file_basename, file_directory, file_mention, filter_files,
    filter_skills, linked_skill_mention, skill_detail, ComposerTrigger, Skill, TriggerKind,
};

/// The heading above the composer's `@` file list.
pub const FILE_SUGGESTION_HEADING: &str = "Project files";

/// Which of the three lists is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionKind {
    Slash,
    Skill,
    File,
}

/// One pickable row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionRow {
    pub label: String,
    pub detail: String,
    pub round_top: bool,
    pub round_bottom: bool,
}

/// The popup as the renderer draws it, or absent when no list is open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionPopup {
    pub kind: SuggestionKind,
    pub rows: Vec<SuggestionRow>,
    pub selected: usize,
    pub send_on_enter: bool,
    pub heading: String,
    /// The loading, empty or error line, or `null`.
    pub status: Option<String>,
    pub retry: bool,
    pub loading: bool,
}

/// Which corners of a pickable row are rounded.
///
/// CDXC:SessionChat 2026-09-19 DECISION:
/// User: "the rounding on each skill and mention row isn't nice here, please fix. maybe just round
/// the very first skill in the list and the very last one from the bottom (but only round the
/// corners not touching another row)". The rows read as one block: only the first row's top corners
/// and the last row's bottom corners are rounded (a lone row keeps all four), for the row outline
/// and the hover and selected fill alike. The count is the whole list, not the rows scrolled into
/// view; the heading and the loading, empty or error line are not rows.
pub fn suggestion_row_corners(index: usize, count: usize) -> (bool, bool) {
    (index == 0, count > 0 && index == count - 1)
}

/// What the composer knows about the two catalogs it can offer.
#[derive(Clone, Debug, PartialEq)]
pub struct SuggestionSources {
    pub agent: Option<String>,
    pub session_agent_id: Option<String>,
    /// The draft session's agent rows, for the `$` heading.
    pub available_agents: Vec<AvailableAgent>,
    /// `None` until a read has answered.
    pub skills: Option<Vec<Skill>>,
    pub skills_loading: bool,
    pub skills_error: Option<String>,
    /// `None` until a read has answered.
    pub files: Option<Vec<String>>,
    pub files_loading: bool,
    /// The agent the skills list was read for, so a switch re-reads it.
    pub skills_agent: Option<String>,
    /// The skills read in flight, so a late answer to a retired one is dropped.
    pub skills_request: Option<u64>,
    /// The files read in flight. Asked for once per chat, when the `@` list first opens.
    pub files_request: Option<u64>,
    pub files_asked: bool,
    /// The skills read has gone out for this agent at least once.
    pub skills_asked: bool,
}

/// CDXC:AgentSkills 2026-09-22 WHY:
/// `skillsLoading` starts TRUE. `useSessionChatSkills` publishes
/// `current?.loading ?? Boolean(transport.readSkills)` (`skills.ts:62`) and every transport this
/// crate serves has a `readSkills`, so a `$` typed before the first read has answered draws
/// "Loading skills…" rather than "No skills". It was false here until 2026-09-22, which is the
/// whole window between mount and the boot read.
impl Default for SuggestionSources {
    fn default() -> Self {
        Self {
            agent: None,
            session_agent_id: None,
            available_agents: Vec::new(),
            skills: None,
            skills_loading: true,
            skills_error: None,
            files: None,
            files_loading: false,
            skills_agent: None,
            skills_request: None,
            files_request: None,
            files_asked: false,
            skills_asked: false,
        }
    }
}

/// One row of the draft session's agent list, as far as the `$` heading needs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableAgent {
    pub agent_id: String,
    pub name: String,
}

/// Which lists were dismissed with Escape, per trigger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SuggestionDismissals {
    pub slash: bool,
    pub skill: bool,
    pub file: bool,
}

/// What the three filters produced for one draft and caret.
#[derive(Clone, Debug, PartialEq)]
pub struct SuggestionMatches {
    pub slash_query: Option<String>,
    pub slash_matches: Vec<&'static SlashCommand>,
    pub slash_open: bool,
    pub trigger: Option<ComposerTrigger>,
    pub skill_query: Option<String>,
    pub skill_matches: Vec<Skill>,
    pub skill_picker_active: bool,
    pub skill_open: bool,
    pub file_query: Option<String>,
    pub file_matches: Vec<String>,
    pub file_picker_active: bool,
    pub file_open: bool,
}

/// Which of the two mention pickers the draft has open, without filtering either catalog.
///
/// `skillPickerActive` and `filePickerActive` are computed from the slash list and the trigger
/// alone (`composer-suggestions.ts:83`, `:92`), never from the skills or the files, so the edge
/// that decides to ASK for a catalog can be measured without cloning the catalog it is about to
/// ask for. That is what lets `request_catalogs` run this on every event the way
/// `NativeComposerSuggestions.projection` runs on every publish.
pub fn picker_edges(
    draft: &str,
    caret: usize,
    agent: Option<&str>,
    dismissed: SuggestionDismissals,
) -> (bool, bool) {
    let slash_open = match slash_query(draft) {
        Some(query) if !dismissed.slash => {
            !filter_slash_commands(slash_commands_for_agent(agent), query).is_empty()
        }
        _ => false,
    };
    let kind = detect_composer_trigger(draft, Some(caret)).map(|trigger| trigger.kind);
    (
        kind == Some(TriggerKind::Skill) && !dismissed.skill && !slash_open,
        kind == Some(TriggerKind::Path) && !dismissed.file && !slash_open,
    )
}

/// Runs the three filters for one draft and caret.
pub fn composer_suggestions(
    draft: &str,
    caret: usize,
    sources: &SuggestionSources,
    dismissed: SuggestionDismissals,
    can_request_skills: bool,
) -> SuggestionMatches {
    let commands = slash_commands_for_agent(sources.agent.as_deref());
    let slash_query = slash_query(draft).map(str::to_string);
    let slash_matches = match &slash_query {
        Some(query) if !dismissed.slash => filter_slash_commands(commands, query),
        _ => Vec::new(),
    };
    let slash_open = !slash_matches.is_empty();
    let trigger = detect_composer_trigger(draft, Some(caret));
    let skill_query = trigger
        .as_ref()
        .filter(|trigger| trigger.kind == TriggerKind::Skill)
        .map(|trigger| trigger.query.clone());
    let skills = sources.skills.clone().unwrap_or_default();
    let skill_matches = match &skill_query {
        Some(query) if !dismissed.skill => {
            filter_skills(&skills, query).into_iter().cloned().collect()
        }
        _ => Vec::new(),
    };
    let skill_picker_active = skill_query.is_some() && !dismissed.skill && !slash_open;
    let skill_open = skill_picker_active
        && (!skill_matches.is_empty()
            || sources.skills_loading
            || sources
                .skills_error
                .as_deref()
                .is_some_and(|error| !error.is_empty())
            || (sources.skills.as_ref().is_some_and(Vec::is_empty) && can_request_skills));
    let file_query = trigger
        .as_ref()
        .filter(|trigger| trigger.kind == TriggerKind::Path)
        .map(|trigger| trigger.query.clone());
    let files = sources.files.clone().unwrap_or_default();
    let file_matches: Vec<String> = match &file_query {
        Some(query) if !dismissed.file => filter_files(&files, query)
            .into_iter()
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    let file_picker_active = file_query.is_some() && !dismissed.file && !slash_open;
    let file_open = file_picker_active
        && (!file_matches.is_empty() || (sources.files_loading && sources.files.is_none()));
    SuggestionMatches {
        slash_query,
        slash_matches,
        slash_open,
        trigger,
        skill_query,
        skill_matches,
        skill_picker_active,
        skill_open,
        file_query,
        file_matches,
        file_picker_active,
        file_open,
    }
}

/// Replaces the mention under the caret with `replacement`, answering the new text and caret.
pub fn complete_composer_mention(
    draft: &str,
    caret: usize,
    replacement: &str,
) -> Option<(String, usize)> {
    let trigger = detect_composer_trigger(draft, Some(caret))?;
    let indexed = Utf16Text::new(draft);
    let start = indexed.index_of_offset(trigger.start);
    let end = indexed.index_of_offset(trigger.end);
    Some((
        format!(
            "{}{replacement}{}",
            indexed.slice(0, start),
            indexed.slice(end, indexed.len())
        ),
        trigger.start + replacement.encode_utf16().count(),
    ))
}

/// The composer-native completion for a typed `/name`, when the catalog has one.
pub fn composer_native_command(agent: Option<&str>, text: &str) -> Option<&'static str> {
    slash_commands_for_agent(agent)
        .iter()
        .find(|command| {
            command.insert_text.is_some() && text.trim() == format!("/{}", command.name)
        })
        .and_then(|command| command.insert_text)
}

/// The `$` list's heading.
///
/// A project custom agent's row name wins, because its own id has no entry in the shared agent
/// catalog. The `/` list keeps the catalog's product heading instead.
fn skills_heading(sources: &SuggestionSources) -> String {
    let row = sources
        .available_agents
        .iter()
        .find(|agent| Some(&agent.agent_id) == sources.session_agent_id.as_ref());
    let name = row
        .map(|row| row.name.clone())
        .or_else(|| agent_display_name(sources.agent.as_deref()))
        .unwrap_or_else(|| "Agent".to_string());
    format!("{name} skills")
}

/// The agent's display name, `claude-code` to `Claude Code`.
///
/// **To fold into family f.** `sessionChatWelcomeAgentName` in
/// `packages/shared/session-chat-presentation/new-session-welcome.ts` is family f's, and the
/// default-agent table it reads belongs in `packages/gx-core` beside the sidebar's copy. The `$`
/// list heading is the only caller family d has, so it lives here until family f lands.
pub fn agent_display_name(agent: Option<&str>) -> Option<String> {
    let normalized = agent?.trim();
    if normalized.is_empty() {
        return None;
    }
    if let Some(name) = default_agent_name(&normalized.to_lowercase()) {
        return Some(name.to_string());
    }
    // `.replace(/[-_]+/g, ' ')` then upper-case the first letter of every word.
    let spaced: String = normalized
        .chars()
        .map(|character| {
            if character == '-' || character == '_' {
                ' '
            } else {
                character
            }
        })
        .collect();
    let collapsed = collapse_separator_runs(&spaced, normalized);
    let mut out = String::with_capacity(collapsed.len());
    let mut at_word_start = true;
    for character in collapsed.chars() {
        if at_word_start && character.is_alphabetic() {
            out.extend(character.to_uppercase());
        } else {
            out.push(character);
        }
        at_word_start = !character.is_alphabetic() && !character.is_numeric();
    }
    Some(out)
}

/// `[-_]+` collapses to one space, so `a__b` becomes `a b`.
fn collapse_separator_runs(spaced: &str, source: &str) -> String {
    let source: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(spaced.len());
    let mut index = 0;
    while index < source.len() {
        if source[index] == '-' || source[index] == '_' {
            out.push(' ');
            while index < source.len() && (source[index] == '-' || source[index] == '_') {
                index += 1;
            }
            continue;
        }
        out.push(source[index]);
        index += 1;
    }
    out
}

const DEFAULT_AGENT_NAMES: &[(&str, &str)] = &[
    ("codex", "Codex"),
    ("claude", "Claude"),
    ("cursor", "Cursor CLI"),
    ("pi", "Pi Agent"),
    ("opencode", "OpenCode"),
    ("gemini", "Gemini"),
    ("copilot", "Copilot"),
    ("droid", "Factory Droid"),
    ("grok", "Grok Build"),
    ("antigravity", "Antigravity CLI"),
    ("amp", "Amp CLI"),
    ("hermes-agent", "Hermes Agent"),
    ("rovodev", "Rovo Dev"),
    ("codebuddy", "CodeBuddy"),
    ("qoder", "Qoder"),
    ("kiro", "Kiro CLI"),
    ("omp", "OMP"),
    ("kimi", "Kimi Code"),
    ("openclaude", "OpenClaude"),
    ("command-code", "Command Code"),
    ("devin", "Devin"),
    ("mastra", "Mastra Code"),
    ("zcode", "ZCode"),
];

fn default_agent_name(agent_id: &str) -> Option<&'static str> {
    DEFAULT_AGENT_NAMES
        .iter()
        .find(|(id, _)| *id == agent_id)
        .map(|(_, name)| *name)
}

/// The popup for one draft, caret and catalog state, or `None` when no list is open.
pub fn suggestion_popup(
    matches: &SuggestionMatches,
    sources: &SuggestionSources,
    text: &str,
    index: usize,
) -> Option<SuggestionPopup> {
    let kind = if matches.slash_open {
        SuggestionKind::Slash
    } else if matches.skill_open {
        SuggestionKind::Skill
    } else if matches.file_open {
        SuggestionKind::File
    } else {
        return None;
    };
    let rows: Vec<(String, String)> = match kind {
        SuggestionKind::Slash => matches
            .slash_matches
            .iter()
            .map(|command| {
                (
                    format!("/{}", command.name),
                    command.description.to_string(),
                )
            })
            .collect(),
        SuggestionKind::Skill => matches
            .skill_matches
            .iter()
            .map(|skill| (format!("${}", skill.name), skill_detail(skill)))
            .collect(),
        SuggestionKind::File => matches
            .file_matches
            .iter()
            .map(|path| {
                (
                    file_basename(path).to_string(),
                    file_directory(path).to_string(),
                )
            })
            .collect(),
    };
    let count = rows.len();
    let selected = index.min(count.saturating_sub(1));
    let send_on_enter = kind == SuggestionKind::Slash
        && matches
            .slash_matches
            .get(selected)
            .is_none_or(|command| command.insert_text.is_none())
        && rows
            .get(selected)
            .is_some_and(|(label, _)| text == label.as_str());
    Some(SuggestionPopup {
        kind,
        rows: rows
            .into_iter()
            .enumerate()
            .map(|(row_index, (label, detail))| {
                let (round_top, round_bottom) = suggestion_row_corners(row_index, count);
                SuggestionRow {
                    label,
                    detail,
                    round_top,
                    round_bottom,
                }
            })
            .collect(),
        selected,
        send_on_enter,
        heading: match kind {
            SuggestionKind::Slash => slash_heading_for_agent(sources.agent.as_deref()).to_string(),
            SuggestionKind::Skill => skills_heading(sources),
            SuggestionKind::File => FILE_SUGGESTION_HEADING.to_string(),
        },
        status: match kind {
            SuggestionKind::Skill => {
                if sources.skills_loading {
                    Some("Loading skills\u{2026}".to_string())
                } else if let Some(error) = sources
                    .skills_error
                    .as_deref()
                    .filter(|error| !error.is_empty())
                {
                    Some(error.to_string())
                } else if sources.skills.as_ref().is_some_and(Vec::is_empty) {
                    Some("No skills available.".to_string())
                } else {
                    None
                }
            }
            SuggestionKind::File if count == 0 => Some("Listing project files\u{2026}".to_string()),
            _ => None,
        },
        retry: kind == SuggestionKind::Skill
            && sources
                .skills_error
                .as_deref()
                .is_some_and(|error| !error.is_empty()),
        // React turned a loader beside "Loading skills…" and "Listing project files…", and showed
        // no spinner beside the error row or "No skills available.".
        loading: match kind {
            SuggestionKind::Skill => sources.skills_loading,
            SuggestionKind::File => count == 0,
            SuggestionKind::Slash => false,
        },
    })
}

/// The replacement a pick writes, or `None` when the row cannot complete.
pub fn suggestion_replacement(
    matches: &SuggestionMatches,
    kind: SuggestionKind,
    index: usize,
    text: &str,
) -> Option<String> {
    match kind {
        SuggestionKind::Slash => None,
        SuggestionKind::Skill => matches.skill_matches.get(index).map(linked_skill_mention),
        SuggestionKind::File => matches
            .file_matches
            .get(index)
            .map(|path| file_mention(path, next_file_reference_index(text))),
    }
}
