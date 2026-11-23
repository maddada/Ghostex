//! The `/`, `$`, `@` and `+` popup: which list is open, what it shows, and what a pick writes.
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
    filter_skills, is_command_entry, linked_skill_mention, skill_detail, ComposerTrigger, Skill,
    TriggerKind,
};
use crate::transcript::session_mentions::{
    choice_detail, choice_mention, filter_session_choices, SessionChoice, HERE_SECTION,
};

/// The heading above the composer's `@` file list.
pub const FILE_SUGGESTION_HEADING: &str = "Project files";

/// The heading above the composer's `+` session list when it starts with this project's agents;
/// the other sections are headed by their computer's name (`session_choices`).
pub const SESSION_SUGGESTION_HEADING: &str = HERE_SECTION;

/// The heading above the custom and plugin commands (`commands/<name>.md`) in the `/` and `$` lists.
pub const CUSTOM_COMMANDS_HEADING: &str = "Custom commands";

/// Which of the three lists is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionKind {
    Slash,
    Skill,
    File,
    /// The `+` list of agent sessions: this project's, then every other one by computer.
    Session,
    /// The `#M` list of the user's earlier messages in this chat (`message_numbers.rs`).
    Message,
    /// The `*` list of Boosts (`extras/boosts.rs`).
    Boost,
}

/// The heading above the composer's `*` Boost list.
pub const BOOST_SUGGESTION_HEADING: &str = "Boosts";

/// The heading above the composer's `#M` message list.
pub const MESSAGE_SUGGESTION_HEADING: &str = "Your messages";

/// One pickable row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionRow {
    pub label: String,
    pub detail: String,
    pub round_top: bool,
    pub round_bottom: bool,
    /// A `+` row's agent artwork id, drawn before the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_icon: Option<String>,
    /// A `+` row's session is a coordinator: the coordinator's crown instead of the logo.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub coordinator: bool,
    /// A `+` row's coordinator is its project's main one: its crown is gold
    /// (CDXC:Coordinators 2026-11-12 in transcript/session_mentions.rs `choice_detail`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub main_coordinator: bool,
    /// A `+` row's session is working: its state reads in the working colour.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub working: bool,
    /// A section heading drawn above this row, on the first row of every section after the first
    /// (whose heading is the popup's own `heading`): the `/` list's skills, custom commands and
    /// built-in commands; the `+` list's computers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
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
    /// The `+` list: this project's other agent sessions, then every other reachable one, by
    /// computer, newest first (`session_choices`).
    pub sessions: Vec<SessionChoice>,
    /// The `#M` list: this chat's numbered messages that are loaded, newest first.
    pub messages: Vec<crate::transcript::message_numbers::MessageChoice>,
    /// The Boosts the `*` list offers: `(id, label, description)` (`extras/boosts.rs`).
    pub boosts: Vec<(String, String, String)>,
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
            sessions: Vec::new(),
            messages: Vec::new(),
            boosts: Vec::new(),
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
    pub session: bool,
    pub message: bool,
    pub boost: bool,
}

/// What the three filters produced for one draft and caret.
#[derive(Clone, Debug, PartialEq)]
pub struct SuggestionMatches {
    pub slash_query: Option<String>,
    /// The skills and custom commands the `/` list offers above the built-in commands, skills
    /// first.
    pub slash_skills: Vec<Skill>,
    pub slash_matches: Vec<&'static SlashCommand>,
    /// The built-in commands come first: the query names one exactly and no skill.
    pub slash_commands_first: bool,
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
    pub session_matches: Vec<SessionChoice>,
    pub session_open: bool,
    pub message_matches: Vec<crate::transcript::message_numbers::MessageChoice>,
    pub message_open: bool,
    pub boost_matches: Vec<(String, String, String)>,
    pub boost_open: bool,
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
    let trigger = detect_composer_trigger(draft, Some(caret));
    let kind = trigger.as_ref().map(|trigger| trigger.kind);
    // A `/` token asks for the skills too: the `/` list shows them above the commands.
    let slash_skill = trigger.as_ref().is_some_and(|trigger| trigger.slash);
    (
        kind == Some(TriggerKind::Skill) && !dismissed.skill && (slash_skill || !slash_open),
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
    let skills = sources.skills.clone().unwrap_or_default();
    let (slash_matches, slash_skills) = match &slash_query {
        Some(query) if !dismissed.slash => (
            filter_slash_commands(commands, query),
            ranked_skill_matches(&skills, query),
        ),
        _ => (Vec::new(), Vec::new()),
    };
    let slash_commands_first = slash_query.as_deref().is_some_and(|query| {
        slash_matches.iter().any(|command| command.name == query)
            && !slash_skills.iter().any(|skill| skill.name == query)
    });
    let slash_open = !slash_matches.is_empty() || !slash_skills.is_empty();
    let trigger = detect_composer_trigger(draft, Some(caret));
    let skill_query = trigger
        .as_ref()
        .filter(|trigger| trigger.kind == TriggerKind::Skill)
        .map(|trigger| trigger.query.clone());
    let slash_skill = trigger.as_ref().is_some_and(|trigger| trigger.slash);
    let skill_matches = match &skill_query {
        Some(query) if !dismissed.skill => ranked_skill_matches(&skills, query),
        _ => Vec::new(),
    };
    // A `/` that starts the draft belongs to the `/` list, which Escape closes for good.
    let skill_picker_active = skill_query.is_some()
        && !dismissed.skill
        && !slash_open
        && !(slash_skill && slash_query.is_some());
    // A `/` later in the message opens the skills only while one matches, so a word such as
    // `/tmp` in prose stays text; `$` also shows the loading, empty and error lines.
    let skill_open = skill_picker_active
        && (!skill_matches.is_empty()
            || (!slash_skill
                && (sources.skills_loading
                    || sources
                        .skills_error
                        .as_deref()
                        .is_some_and(|error| !error.is_empty())
                    || (sources.skills.as_ref().is_some_and(Vec::is_empty)
                        && can_request_skills))));
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
    // The `+` list comes from the host's session list, already here: it opens only with a match,
    // so a `+word` that names no session stays plain typing.
    let session_matches = match &trigger {
        Some(trigger)
            if trigger.kind == TriggerKind::Session && !dismissed.session && !slash_open =>
        {
            filter_session_choices(&sources.sessions, &trigger.query)
        }
        _ => Vec::new(),
    };
    let session_open = !session_matches.is_empty();
    // The `#M` list comes from the loaded transcript and, like `+`, opens only with a match.
    let message_matches = match &trigger {
        Some(trigger)
            if trigger.kind == TriggerKind::Message && !dismissed.message && !slash_open =>
        {
            crate::transcript::message_numbers::filter_message_choices(
                &sources.messages,
                &trigger.query,
            )
        }
        _ => Vec::new(),
    };
    let message_open = !message_matches.is_empty();
    // The `*` list opens only while a Boost matches, so `*italics*` typed as text stays text.
    let boost_matches: Vec<(String, String, String)> = match &trigger {
        Some(trigger) if trigger.kind == TriggerKind::Boost && !dismissed.boost && !slash_open => {
            crate::extras::boosts::filter_choices(&sources.boosts, &trigger.query)
                .into_iter()
                .cloned()
                .collect()
        }
        _ => Vec::new(),
    };
    let boost_open = !boost_matches.is_empty();
    SuggestionMatches {
        slash_query,
        slash_skills,
        slash_matches,
        slash_commands_first,
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
        session_matches,
        session_open,
        message_matches,
        message_open,
        boost_matches,
        boost_open,
    }
}

/// The skills and custom commands whose name contains the query: skills before command files,
/// then names that start with the query before the rest, each in catalog order.
fn ranked_skill_matches(skills: &[Skill], query: &str) -> Vec<Skill> {
    let lower = query.trim().to_lowercase();
    let mut matches: Vec<Skill> = filter_skills(skills, query).into_iter().cloned().collect();
    matches.sort_by_key(|skill| {
        (
            is_command_entry(skill),
            !skill.name.to_lowercase().starts_with(&lower),
        )
    });
    matches
}

/// One row of the `/` list.
#[derive(Clone, Copy, Debug)]
pub enum SlashRow<'a> {
    Skill(&'a Skill),
    Command(&'static SlashCommand),
}

/// The `/` list's rows in order: skills, custom and plugin commands, then the agent's built-in
/// commands, or the built-ins first when the query names one exactly.
pub fn slash_rows(matches: &SuggestionMatches) -> Vec<SlashRow<'_>> {
    let skills = matches.slash_skills.iter().map(SlashRow::Skill);
    let commands = matches
        .slash_matches
        .iter()
        .map(|command| SlashRow::Command(command));
    if matches.slash_commands_first {
        commands.chain(skills).collect()
    } else {
        skills.chain(commands).collect()
    }
}

/// The section a skill or command row belongs to, as its heading.
fn row_section(row: &SlashRow, sources: &SuggestionSources) -> String {
    match row {
        SlashRow::Skill(skill) if is_command_entry(skill) => CUSTOM_COMMANDS_HEADING.to_string(),
        SlashRow::Skill(_) => skills_heading(sources),
        SlashRow::Command(_) => slash_heading_for_agent(sources.agent.as_deref()).to_string(),
    }
}

/// Each row's section heading when it starts a new section after the first, and the first
/// section's heading, which is the popup's own.
fn sections(
    rows: &[SlashRow],
    sources: &SuggestionSources,
) -> (Option<String>, Vec<Option<String>>) {
    let mut first = None;
    let mut previous: Option<String> = None;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let section = row_section(row, sources);
        if previous.as_deref() == Some(section.as_str()) {
            out.push(None);
            continue;
        }
        if first.is_none() {
            first = Some(section.clone());
            out.push(None);
        } else {
            out.push(Some(section.clone()));
        }
        previous = Some(section);
    }
    (first, out)
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

/// Replaces the `+` mention under the caret with a session's `replacement`, kept apart from the
/// words beside it by a real space on each side: one before it when the character before is not
/// whitespace, one after it unless the text after already starts with one, the caret past that
/// space (CDXC:SessionChat 2026-11-16 on `mention_card` in transcript/session_mentions.rs).
pub fn complete_spaced_mention(
    draft: &str,
    caret: usize,
    replacement: &str,
) -> Option<(String, usize)> {
    let trigger = detect_composer_trigger(draft, Some(caret))?;
    let indexed = Utf16Text::new(draft);
    let start = indexed.index_of_offset(trigger.start);
    let end = indexed.index_of_offset(trigger.end);
    let before = indexed.slice(0, start);
    let after = indexed.slice(end, indexed.len());
    let lead = if before.chars().last().is_some_and(|c| !c.is_whitespace()) {
        " "
    } else {
        ""
    };
    let trail = if after.starts_with(char::is_whitespace) {
        ""
    } else {
        " "
    };
    let inserted = format!("{lead}{replacement}{trail}");
    let caret = trigger.start + inserted.encode_utf16().count() + usize::from(trail.is_empty());
    Some((format!("{before}{inserted}{after}"), caret))
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
    ("freebuff", "Freebuff"),
    ("empryo", "Empryo"),
    ("dsh", "DeepSeek Harness"),
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
    } else if matches.session_open {
        SuggestionKind::Session
    } else if matches.message_open {
        SuggestionKind::Message
    } else if matches.boost_open {
        SuggestionKind::Boost
    } else {
        return None;
    };
    let slash = slash_rows(matches);
    let skill_rows: Vec<SlashRow> = matches.skill_matches.iter().map(SlashRow::Skill).collect();
    let (section_heading, row_sections) = match kind {
        SuggestionKind::Slash => sections(&slash, sources),
        SuggestionKind::Skill => sections(&skill_rows, sources),
        SuggestionKind::Session => session_sections(&matches.session_matches),
        SuggestionKind::Boost => boost_sections(&matches.boost_matches),
        SuggestionKind::File | SuggestionKind::Message => (None, Vec::new()),
    };
    let row_label = |row: &SlashRow| match row {
        SlashRow::Skill(skill) => (format!("/{}", skill.name), skill_detail(skill)),
        SlashRow::Command(command) => (
            format!("/{}", command.name),
            command.description.to_string(),
        ),
    };
    let rows: Vec<(String, String)> = match kind {
        SuggestionKind::Slash => slash.iter().map(row_label).collect(),
        SuggestionKind::Skill => skill_rows.iter().map(row_label).collect(),
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
        SuggestionKind::Session => matches
            .session_matches
            .iter()
            .map(|choice| (choice.title.clone(), choice_detail(choice)))
            .collect(),
        SuggestionKind::Message => matches
            .message_matches
            .iter()
            .map(|choice| (format!("M{}", choice.number), choice.preview.clone()))
            .collect(),
        SuggestionKind::Boost => matches
            .boost_matches
            .iter()
            .map(|(_, label, description)| (label.clone(), description.clone()))
            .collect(),
    };
    let choice = |row_index: usize| {
        matches
            .session_matches
            .get(row_index)
            .filter(|_| kind == SuggestionKind::Session)
    };
    let count = rows.len();
    let selected = index.min(count.saturating_sub(1));
    let send_on_enter = kind == SuggestionKind::Slash
        && matches!(
            slash.get(selected),
            Some(SlashRow::Command(command)) if command.insert_text.is_none()
        )
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
                    agent_icon: choice(row_index).and_then(|choice| choice.agent_icon.clone()),
                    coordinator: choice(row_index).is_some_and(|choice| choice.coordinator),
                    main_coordinator: choice(row_index)
                        .is_some_and(|choice| choice.coordinator && choice.main_coordinator),
                    working: choice(row_index).is_some_and(|choice| choice.working),
                    section: row_sections.get(row_index).cloned().flatten(),
                }
            })
            .collect(),
        selected,
        send_on_enter,
        heading: match kind {
            SuggestionKind::Slash | SuggestionKind::Skill => {
                section_heading.unwrap_or_else(|| skills_heading(sources))
            }
            SuggestionKind::File => FILE_SUGGESTION_HEADING.to_string(),
            SuggestionKind::Session => {
                section_heading.unwrap_or_else(|| SESSION_SUGGESTION_HEADING.to_string())
            }
            SuggestionKind::Message => MESSAGE_SUGGESTION_HEADING.to_string(),
            SuggestionKind::Boost => section_heading
                .map(|tab| format!("{BOOST_SUGGESTION_HEADING} \u{b7} {tab}"))
                .unwrap_or_else(|| BOOST_SUGGESTION_HEADING.to_string()),
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
            SuggestionKind::Slash
            | SuggestionKind::Session
            | SuggestionKind::Message
            | SuggestionKind::Boost => false,
        },
    })
}

/// The `*` list's first tab (named in its heading, "Boosts · Show") and the tab label each later
/// tab's first row carries (Actions, Work, Custom), the list being in the menu's order.
/// CDXC:Boosts 2026-11-20 DECISION: User: "in the * picker show the Boosts grouped with small
/// section labels between the types (Show, Do, Work, Custom)", the same labels as the menu's tabs.
fn boost_sections(choices: &[(String, String, String)]) -> (Option<String>, Vec<Option<String>>) {
    let mut first = None;
    let mut previous: Option<&str> = None;
    let mut out = Vec::with_capacity(choices.len());
    for (id, _, _) in choices {
        let tab = crate::extras::boosts::tab_label_of(id);
        if previous == Some(tab) {
            out.push(None);
            continue;
        }
        if first.is_none() {
            first = Some(tab.to_string());
            out.push(None);
        } else {
            out.push(Some(tab.to_string()));
        }
        previous = Some(tab);
    }
    (first, out)
}

/// The `+` list's heading (its first row's section) and the heading each later section's first row
/// carries.
fn session_sections(choices: &[SessionChoice]) -> (Option<String>, Vec<Option<String>>) {
    let heading = choices
        .first()
        .map(|choice| choice.section.clone())
        .filter(|section| !section.is_empty());
    let rows = choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            (index > 0
                && choices[index - 1].section != choice.section
                && !choice.section.is_empty())
            .then(|| choice.section.clone())
        })
        .collect();
    (heading, rows)
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
        SuggestionKind::Session => matches.session_matches.get(index).map(choice_mention),
        SuggestionKind::Message => matches.message_matches.get(index).map(|choice| {
            crate::transcript::message_numbers::message_link(
                &crate::transcript::message_numbers::MessageRef {
                    session_id: None,
                    number: choice.number,
                },
            )
        }),
        SuggestionKind::Boost => matches
            .boost_matches
            .get(index)
            .map(|(id, label, _)| crate::extras::boosts::boost_link(id, label)),
    }
}

/// The compact mention menu for a reply box (`*` Boosts, `+` agents, `/` or `$` skills): same
/// trigger detection, filtering, sections and row format as the main chat composer, without `@`
/// files, `#M` messages or built-in slash commands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyMentionPopup {
    pub kind: SuggestionKind,
    /// Draft offset of the trigger sigil, in UTF-16 code units, so the caller can track Escape
    /// dismissal per trigger.
    pub trigger_start: usize,
    pub heading: String,
    pub rows: Vec<SuggestionRow>,
    pub status: Option<String>,
    pub loading: bool,
}

/// The text and UTF-16 caret offset after picking a row from [`ReplyMentionPopup`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyMentionCompletion {
    pub text: String,
    pub caret: usize,
}

/// Evaluates the `*`, `+`, `/` or `$` mention menu for a reply box at `caret` (UTF-16 code units).
pub fn reply_mention_popup(
    text: &str,
    caret: usize,
    sources: &SuggestionSources,
) -> Option<ReplyMentionPopup> {
    let trigger = detect_composer_trigger(text, Some(caret))?;
    match trigger.kind {
        TriggerKind::Boost => {
            let boost_matches: Vec<(String, String, String)> =
                crate::extras::boosts::filter_choices(&sources.boosts, &trigger.query)
                    .into_iter()
                    .cloned()
                    .collect();
            if boost_matches.is_empty() {
                return None;
            }
            let (section_heading, row_sections) = boost_sections(&boost_matches);
            let count = boost_matches.len();
            let rows = boost_matches
                .into_iter()
                .enumerate()
                .map(|(index, (_, label, detail))| {
                    let (round_top, round_bottom) = suggestion_row_corners(index, count);
                    SuggestionRow {
                        label,
                        detail,
                        round_top,
                        round_bottom,
                        agent_icon: None,
                        coordinator: false,
                        main_coordinator: false,
                        working: false,
                        section: row_sections.get(index).cloned().flatten(),
                    }
                })
                .collect();
            Some(ReplyMentionPopup {
                kind: SuggestionKind::Boost,
                trigger_start: trigger.start,
                heading: section_heading
                    .map(|tab| format!("{BOOST_SUGGESTION_HEADING} \u{b7} {tab}"))
                    .unwrap_or_else(|| BOOST_SUGGESTION_HEADING.to_string()),
                rows,
                status: None,
                loading: false,
            })
        }
        TriggerKind::Session => {
            let session_matches = filter_session_choices(&sources.sessions, &trigger.query);
            if session_matches.is_empty() {
                return None;
            }
            let (section_heading, row_sections) = session_sections(&session_matches);
            let count = session_matches.len();
            let rows = session_matches
                .into_iter()
                .enumerate()
                .map(|(index, choice)| {
                    let (round_top, round_bottom) = suggestion_row_corners(index, count);
                    let detail = choice_detail(&choice);
                    SuggestionRow {
                        label: choice.title,
                        detail,
                        round_top,
                        round_bottom,
                        agent_icon: choice.agent_icon,
                        coordinator: choice.coordinator,
                        main_coordinator: choice.coordinator && choice.main_coordinator,
                        working: choice.working,
                        section: row_sections.get(index).cloned().flatten(),
                    }
                })
                .collect();
            Some(ReplyMentionPopup {
                kind: SuggestionKind::Session,
                trigger_start: trigger.start,
                heading: section_heading
                    .unwrap_or_else(|| SESSION_SUGGESTION_HEADING.to_string()),
                rows,
                status: None,
                loading: false,
            })
        }
        TriggerKind::Skill => {
            let skills = sources.skills.clone().unwrap_or_default();
            let skill_matches = ranked_skill_matches(&skills, &trigger.query);
            let status = if sources.skills_loading {
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
            };
            // A `/` opens only while a skill matches, so `/tmp` in prose stays text; `$` also shows
            // loading, empty and error states, matching the main composer.
            if skill_matches.is_empty() && (trigger.slash || status.is_none()) {
                return None;
            }
            let slash_rows: Vec<SlashRow> = skill_matches.iter().map(SlashRow::Skill).collect();
            let (section_heading, row_sections) = sections(&slash_rows, sources);
            let count = skill_matches.len();
            let rows = skill_matches
                .iter()
                .enumerate()
                .map(|(index, skill)| {
                    let (round_top, round_bottom) = suggestion_row_corners(index, count);
                    SuggestionRow {
                        label: format!("/{}", skill.name),
                        detail: skill_detail(skill),
                        round_top,
                        round_bottom,
                        agent_icon: None,
                        coordinator: false,
                        main_coordinator: false,
                        working: false,
                        section: row_sections.get(index).cloned().flatten(),
                    }
                })
                .collect();
            Some(ReplyMentionPopup {
                kind: SuggestionKind::Skill,
                trigger_start: trigger.start,
                heading: section_heading.unwrap_or_else(|| skills_heading(sources)),
                rows,
                status,
                loading: sources.skills_loading,
            })
        }
        TriggerKind::Path | TriggerKind::Message => None,
    }
}

/// Applies a pick from [`reply_mention_popup`], inserting the exact same token and spacing the
/// main chat composer inserts.
pub fn complete_reply_mention(
    text: &str,
    caret: usize,
    sources: &SuggestionSources,
    index: usize,
) -> Option<ReplyMentionCompletion> {
    let trigger = detect_composer_trigger(text, Some(caret))?;
    let (content, caret) = match trigger.kind {
        TriggerKind::Boost => {
            let matches = crate::extras::boosts::filter_choices(&sources.boosts, &trigger.query);
            let (id, label, _) = matches.get(index)?;
            let replacement = crate::extras::boosts::boost_link(id, label);
            complete_composer_mention(text, caret, &format!("{replacement} "))?
        }
        TriggerKind::Session => {
            let matches = filter_session_choices(&sources.sessions, &trigger.query);
            let choice = matches.get(index)?;
            let replacement = choice_mention(choice);
            complete_spaced_mention(text, caret, &replacement)?
        }
        TriggerKind::Skill => {
            let skills = sources.skills.clone().unwrap_or_default();
            let matches = ranked_skill_matches(&skills, &trigger.query);
            let skill = matches.get(index)?;
            let replacement = linked_skill_mention(skill);
            complete_composer_mention(text, caret, &format!("{replacement} "))?
        }
        TriggerKind::Path | TriggerKind::Message => return None,
    };
    Some(ReplyMentionCompletion {
        text: content,
        caret,
    })
}

#[cfg(test)]
mod session_picker_tests {
    use super::*;
    use crate::event::KnownSession;
    use crate::transcript::session_mentions::session_choices;

    fn sources() -> SuggestionSources {
        let session = |id: &str, title: &str, at: &str| KnownSession {
            global_ref: format!("S90:P3lv0:{id}"),
            project_id: "P3lv0".to_string(),
            session_id: id.to_string(),
            title: title.to_string(),
            agent_id: "claude".to_string(),
            agent_name: "Claude".to_string(),
            activity: "working".to_string(),
            lifecycle_state: "running".to_string(),
            active_at: at.to_string(),
            ..KnownSession::default()
        };
        SuggestionSources {
            sessions: session_choices(
                &[
                    session("G1", "Fix login", "2026-10-28T08:00:00Z"),
                    session("G2", "C++ build", "2026-10-28T07:00:00Z"),
                    session("Gme", "This chat", "2026-10-28T09:00:00Z"),
                ],
                "P3lv0",
                "Gme",
            ),
            ..SuggestionSources::default()
        }
    }

    fn popup(draft: &str) -> Option<SuggestionPopup> {
        let sources = sources();
        let caret = draft.encode_utf16().count();
        let matches = composer_suggestions(
            draft,
            caret,
            &sources,
            SuggestionDismissals::default(),
            true,
        );
        suggestion_popup(&matches, &sources, draft, 0)
    }

    #[test]
    fn plus_opens_the_session_list_at_the_start_or_after_a_space() {
        let opened = popup("+").expect("the list");
        assert_eq!(opened.kind, SuggestionKind::Session);
        assert_eq!(opened.heading, SESSION_SUGGESTION_HEADING);
        let labels: Vec<&str> = opened.rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(labels, vec!["Fix login", "C++ build"]);
        assert_eq!(opened.rows[0].detail, "Claude \u{b7} Working");
        assert_eq!(opened.rows[0].agent_icon.as_deref(), Some("claude"));
        assert!(opened.rows[0].working);

        let filtered = popup("ask +fix").expect("filtered");
        assert_eq!(filtered.rows.len(), 1);
        assert_eq!(filtered.rows[0].label, "Fix login");

        // Titles have spaces, so the query keeps going past one while a title matches.
        let words = popup("ask +fix log").expect("words");
        assert_eq!(words.rows[0].label, "Fix login");
        assert!(popup("ask +fix it later please").is_none());
        assert!(popup("a + fix").is_none());
        assert!(popup("+fix\nlog").is_none());
    }

    #[test]
    fn the_session_list_heads_each_computer_after_this_project() {
        let row = |title: &str, section: &str| SessionChoice {
            title: title.to_string(),
            global_ref: format!("S1:P1:{title}"),
            section: section.to_string(),
            ..SessionChoice::default()
        };
        let sources = SuggestionSources {
            sessions: vec![
                row("Fix login", SESSION_SUGGESTION_HEADING),
                row("Big Chungus", "Studio"),
                row("Arch Chungus", "Omarchy"),
                row("Win Chungus", "Omarchy"),
            ],
            ..SuggestionSources::default()
        };
        let open = |draft: &str| {
            let matches = composer_suggestions(
                draft,
                draft.encode_utf16().count(),
                &sources,
                SuggestionDismissals::default(),
                true,
            );
            suggestion_popup(&matches, &sources, draft, 0).expect("the list")
        };
        let sections = |popup: &SuggestionPopup| -> Vec<Option<String>> {
            popup.rows.iter().map(|row| row.section.clone()).collect()
        };
        let all = open("+");
        assert_eq!(all.heading, SESSION_SUGGESTION_HEADING);
        assert_eq!(
            sections(&all),
            vec![None, Some("Studio".into()), Some("Omarchy".into()), None]
        );
        // A query whose matches are all elsewhere heads the list with their computer.
        let chungus = open("+chungus");
        assert_eq!(chungus.heading, "Studio");
        assert_eq!(sections(&chungus), vec![None, Some("Omarchy".into()), None]);
    }

    #[test]
    fn plus_inside_a_word_or_before_a_number_stays_text() {
        for draft in ["1+1", "C++", "a+b", "+1", "+5%", "++", "+-", "x +nomatch"] {
            assert!(popup(draft).is_none(), "{draft} opened the + list");
        }
        // A `+` after a space with nothing typed yet is the start of a mention.
        assert_eq!(
            popup("use +").map(|popup| popup.kind),
            Some(SuggestionKind::Session)
        );
    }

    #[test]
    fn a_pick_writes_the_mention_and_a_space() {
        let sources = sources();
        let draft = "ask +fix lo";
        let caret = draft.encode_utf16().count();
        let matches = composer_suggestions(
            draft,
            caret,
            &sources,
            SuggestionDismissals::default(),
            true,
        );
        let replacement =
            suggestion_replacement(&matches, SuggestionKind::Session, 0, draft).unwrap();
        let (text, caret) =
            complete_composer_mention(draft, caret, &format!("{replacement} ")).unwrap();
        assert_eq!(text, "ask [Fix login](S90:P3lv0:G1) ");
        assert_eq!(caret, text.encode_utf16().count());
    }
}
