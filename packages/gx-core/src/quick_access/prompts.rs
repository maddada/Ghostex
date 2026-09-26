//! The Saved Prompts tab: the Saved, Recovered and Sent views, the scope and tag filters, day
//! grouping, rows and editor of packages/core-ui/stashed-prompts-modal.tsx.
//!
//! Ported from `apps/desktop/sidebar/native-quick-access/prompts.ts` (deleted with QuickJS on
//! 2026-09-25; see git history).

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::data::{QuickAccessData, QuickAccessRecoveredDraft};
use super::icons::prompt_project_icon;
use super::text::{
    collapse_runs, day_label, js_lower, locale_compare, relative_time, QuickAccessClock,
};
use super::wire::{
    QuickAccessGroup, QuickAccessIcon, QuickAccessMenuItem, QuickAccessOption,
    QuickAccessPromptChip, QuickAccessPromptEditor, QuickAccessRow, QuickAccessSelect,
};
use crate::keys::{MachineId, SessionKey};
use crate::sidebar_actions::iso_string_from_ms;
use crate::sidebar_view::text::js_trim;

pub(crate) const ALL_PROJECTS_VALUE: &str = "scope:all";
pub(crate) const CURRENT_SESSION_VALUE: &str = "scope:session";
pub(crate) const NO_PROJECT_VALUE: &str = "project:none";
pub(crate) const ALL_TAGS_VALUE: &str = "tag:all";
pub(crate) const NO_TAG_VALUE: &str = "tag:none";
const TOOLTIP_LINE_COUNT: usize = 30;
pub(crate) const FAVORITE_TAG_ID: &str = "favorite";
const STASHED_TAG_ID: &str = "stashed";
pub(crate) const RECOVERED_PROMPT_ID_PREFIX: &str = "recovered:";

/// CDXC:SavedPrompts 2026-08-23 (ported):
/// New tags pick from these eight hues, which stay legible as a 7px dot, an 18px chip and a 3px
/// row stripe.
pub(crate) const STASHED_PROMPT_TAG_COLORS: [&str; 8] = [
    "#e3b341", "#7f9cf5", "#86d1a4", "#e3796b", "#c99bdd", "#7ec7f5", "#e0a3c8", "#9aa4b2",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PromptsView {
    #[default]
    Saved,
    Recovered,
    Sent,
}

impl PromptsView {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::Recovered => "recovered",
            Self::Sent => "sent",
        }
    }
    pub(crate) fn parse(value: &str) -> Self {
        match value {
            "recovered" => Self::Recovered,
            "sent" => Self::Sent,
            _ => Self::Saved,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PromptsScope {
    #[default]
    All,
    Project,
    Session,
}

impl PromptsScope {
    pub(crate) fn parse(value: Option<&str>) -> Option<Self> {
        match value? {
            "all" => Some(Self::All),
            "project" => Some(Self::Project),
            "session" => Some(Self::Session),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum TagFilter {
    #[default]
    All,
    Tag(String),
    Untagged,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Editing {
    pub(crate) prompt_id: Option<String>,
    pub(crate) content: String,
    pub(crate) project_value: String,
    pub(crate) tag_value: String,
    pub(crate) is_favorite: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Composer {
    pub(crate) name: String,
    pub(crate) color: String,
    pub(crate) anchor: String,
    pub(crate) prompt_id: Option<String>,
}

/// `PromptsTabState`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PromptsTabState {
    pub(crate) view: PromptsView,
    pub(crate) scope: PromptsScope,
    pub(crate) scope_project_id: Option<String>,
    pub(crate) tag_filter: TagFilter,
    pub(crate) prompts: Option<Vec<Value>>,
    pub(crate) tags: Vec<Value>,
    pub(crate) recovered: Vec<Value>,
    pub(crate) sent: Vec<Value>,
    pub(crate) project_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) raw_project_id: Option<String>,
    pub(crate) raw_session_id: Option<String>,
    pub(crate) editing: Option<Editing>,
    pub(crate) saving: bool,
    pub(crate) save_error: Option<String>,
    pub(crate) tag_error: Option<String>,
    pub(crate) composer: Option<Composer>,
    pub(crate) resolved_default_scope: bool,
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

fn opt<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value[key].as_str()
}

/// `applyPromptsLauncherContext`.
pub(crate) fn apply_launcher_context(
    state: &mut PromptsTabState,
    project_id: Option<String>,
    session_id: Option<String>,
    scope: Option<PromptsScope>,
) {
    // `parseGxserverPresentationProjectSessionId` reads only this computer's combined ids.
    let combined = session_id
        .as_deref()
        .and_then(SessionKey::parse_sidebar_session_id)
        .filter(|key| key.machine == MachineId::Local);
    state.raw_session_id = combined
        .as_ref()
        .map(|key| key.session_id.clone())
        .or_else(|| session_id.clone());
    state.raw_project_id = project_id
        .clone()
        .or_else(|| combined.as_ref().map(|key| key.project_id.clone()));
    state.project_id = project_id;
    state.session_id = session_id;
    state.scope = scope.unwrap_or_default();
    state.scope_project_id = state.raw_project_id.clone();
    state.resolved_default_scope = false;
}

fn tag_ids(prompt: &Value) -> Vec<&str> {
    prompt["tagIds"]
        .as_array()
        .map(|ids| ids.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn label_tag_ids(prompt: &Value) -> Vec<&str> {
    tag_ids(prompt)
        .into_iter()
        .filter(|id| *id != FAVORITE_TAG_ID)
        .collect()
}

/// `value.toLowerCase().replace(/\s+/g, ' ').trim()`.
fn normalize_text(value: &str) -> String {
    js_trim(&collapse_runs(&js_lower(value))).to_string()
}

fn search_text(prompt: &Value) -> String {
    normalize_text(&format!(
        "{} {}",
        text(prompt, "content"),
        opt(prompt, "projectName").unwrap_or("")
    ))
}

fn prompt_title(prompt: &Value) -> String {
    let title = js_trim(&collapse_runs(text(prompt, "content"))).to_string();
    if title.is_empty() {
        "Untitled saved prompt".to_string()
    } else {
        title
    }
}

fn parse_updated_at(prompt: &Value) -> i64 {
    crate::sidebar_view::text::parse_iso_ms(text(prompt, "updatedAt")).unwrap_or(0)
}

/// `promptBelongsToSession`.
pub(crate) fn belongs_to_session(
    prompt: &Value,
    state: &PromptsTabState,
    data: &QuickAccessData,
) -> bool {
    let agent_session_id = state
        .session_id
        .as_deref()
        .and_then(|id| data.session(id))
        .and_then(|session| session.agent_session_id.as_deref());
    if let Some(agent_session_id) = agent_session_id.filter(|id| !id.is_empty()) {
        if opt(prompt, "agentSessionId") == Some(agent_session_id) {
            return true;
        }
    }
    let Some(raw_session_id) = state.raw_session_id.as_deref().filter(|id| !id.is_empty()) else {
        return false;
    };
    if opt(prompt, "sessionId") != Some(raw_session_id) {
        return false;
    }
    state.raw_project_id.is_none() || opt(prompt, "projectId") == state.raw_project_id.as_deref()
}

/// `promptsProjectOptions`: live groups, then the prompts' own projects, then the launcher's.
pub(crate) fn project_options(
    state: &PromptsTabState,
    data: &QuickAccessData,
) -> Vec<(String, String)> {
    let mut options: Vec<(String, String)> = Vec::new();
    let add = |id: &str, name: String, options: &mut Vec<(String, String)>| {
        if !id.is_empty() && !options.iter().any(|(seen, _)| seen == id) {
            options.push((id.to_string(), name));
        }
    };
    for group in &data.groups {
        if let Some(project_id) = group.editor_project_id.as_deref() {
            add(project_id, group.title.clone(), &mut options);
        }
    }
    for prompt in state.prompts.iter().flatten() {
        if let Some(project_id) = opt(prompt, "projectId") {
            let name = js_trim(opt(prompt, "projectName").unwrap_or(""));
            let name = if name.is_empty() {
                "Unnamed project".to_string()
            } else {
                name.to_string()
            };
            add(project_id, name, &mut options);
        }
    }
    if let Some(raw) = state.raw_project_id.as_deref() {
        add(raw, "This project".to_string(), &mut options);
    }
    options.sort_by(|left, right| locale_compare(&left.1, &right.1));
    options
}

/// `hasSessionScope`.
pub(crate) fn has_session_scope(state: &PromptsTabState, data: &QuickAccessData) -> bool {
    let agent_session_id = state
        .session_id
        .as_deref()
        .and_then(|id| data.session(id))
        .and_then(|session| session.agent_session_id.as_deref())
        .filter(|id| !id.is_empty());
    state
        .raw_session_id
        .as_deref()
        .is_some_and(|id| !id.is_empty())
        || agent_session_id.is_some()
}

fn effective_scope(state: &PromptsTabState, data: &QuickAccessData) -> PromptsScope {
    if state.scope == PromptsScope::Session && !has_session_scope(state, data) {
        return PromptsScope::All;
    }
    if state.scope == PromptsScope::Project
        && state.scope_project_id.as_deref().is_none_or(str::is_empty)
    {
        return PromptsScope::All;
    }
    state.scope
}

/// `activePrompts`.
pub(crate) fn active_prompts(state: &PromptsTabState) -> Option<&Vec<Value>> {
    match state.view {
        PromptsView::Sent => Some(&state.sent),
        PromptsView::Recovered => Some(&state.recovered),
        PromptsView::Saved => state.prompts.as_ref(),
    }
}

/// Search, then scope, then tag: the order the React modal narrows in.
fn scoped_prompts<'a>(
    state: &'a PromptsTabState,
    query: &str,
    data: &QuickAccessData,
) -> Vec<&'a Value> {
    let all: &[Value] = active_prompts(state).map(Vec::as_slice).unwrap_or(&[]);
    let normalized = normalize_text(query);
    let searched = all
        .iter()
        .filter(|prompt| normalized.is_empty() || search_text(prompt).contains(&normalized));
    match effective_scope(state, data) {
        PromptsScope::All => searched.collect(),
        PromptsScope::Project => searched
            .filter(|prompt| {
                state.scope_project_id.is_some()
                    && opt(prompt, "projectId") == state.scope_project_id.as_deref()
            })
            .collect(),
        PromptsScope::Session => searched
            .filter(|prompt| belongs_to_session(prompt, state, data))
            .collect(),
    }
}

fn visible_prompts<'a>(
    state: &'a PromptsTabState,
    query: &str,
    data: &QuickAccessData,
) -> Vec<&'a Value> {
    let scoped = scoped_prompts(state, query, data);
    if state.view != PromptsView::Saved {
        return scoped;
    }
    match &state.tag_filter {
        TagFilter::All => scoped,
        TagFilter::Untagged => scoped
            .into_iter()
            .filter(|prompt| label_tag_ids(prompt).is_empty())
            .collect(),
        TagFilter::Tag(tag_id) => scoped
            .into_iter()
            .filter(|prompt| tag_ids(prompt).contains(&tag_id.as_str()))
            .collect(),
    }
}

/// `recoveredDraftAsPrompt`.
///
/// CDXC:Drafts 2026-08-28 (ported):
/// The Recovered view lists the composer's never-sent local drafts shaped as stash rows, so one
/// list, grouping, search and insert path renders both views.
pub(crate) fn recovered_draft_as_prompt(
    draft: &QuickAccessRecoveredDraft,
    names: &BTreeMap<String, String>,
) -> Value {
    let updated_at = iso_string_from_ms(draft.updated_at);
    let project_name = draft
        .project_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .and_then(|id| names.get(id))
        .filter(|name| !name.is_empty())
        .cloned();
    let id = match draft.recovery_id.as_deref() {
        Some(recovery_id) if !recovery_id.is_empty() => format!("history:{recovery_id}"),
        _ => draft.session_key.clone(),
    };
    json!({
        "content": draft.text,
        "createdAt": updated_at,
        "cwd": null,
        "projectId": draft.project_id,
        "projectName": project_name,
        "promptId": format!("{RECOVERED_PROMPT_ID_PREFIX}{id}"),
        "sessionId": draft.session_id,
        "updatedAt": updated_at,
    })
}

/// The sent history row with its project named, as `refreshSentPrompts` shapes it.
pub(crate) fn sent_as_prompt(message: &Value, names: &BTreeMap<String, String>) -> Value {
    let mut message = message.clone();
    let project_name = opt(&message, "projectId")
        .filter(|id| !id.is_empty())
        .and_then(|id| names.get(id))
        .filter(|name| !name.is_empty())
        .cloned();
    message["projectName"] = json!(project_name);
    message
}

pub(crate) fn recovered_session_key(prompt_id: &str) -> &str {
    prompt_id
        .strip_prefix(RECOVERED_PROMPT_ID_PREFIX)
        .unwrap_or(prompt_id)
}

pub(crate) fn prompt_names(
    state: &PromptsTabState,
    data: &QuickAccessData,
) -> BTreeMap<String, String> {
    project_options(state, data).into_iter().collect()
}

pub(crate) fn prompt_row_key(prompt_id: &str) -> String {
    format!("prompt:{prompt_id}")
}

/// `promptActions`.
pub(crate) fn prompt_actions(state: &PromptsTabState, prompt: &Value) -> Vec<&'static str> {
    let can_jump = opt(prompt, "agentSessionId").is_some_and(|id| !id.is_empty())
        || opt(prompt, "sessionId").is_some_and(|id| !id.is_empty());
    let mut actions: Vec<&'static str> = Vec::new();
    if can_jump {
        actions.push("open");
    }
    if state.view == PromptsView::Saved {
        actions.extend(["favorite", "tag", "copy", "edit", "delete"]);
    } else {
        actions.extend(["save", "copy", "delete"]);
    }
    actions
}

pub(crate) fn prompt_groups(
    state: &PromptsTabState,
    query: &str,
    data: &QuickAccessData,
    clock: &dyn QuickAccessClock,
) -> Vec<QuickAccessGroup> {
    let now_ms = clock.now_ms();
    let mut prompts = visible_prompts(state, query, data);
    prompts.sort_by(|left, right| {
        parse_updated_at(right)
            .cmp(&parse_updated_at(left))
            .then_with(|| locale_compare(text(left, "promptId"), text(right, "promptId")))
    });
    let mut groups: Vec<(String, Vec<QuickAccessRow>)> = Vec::new();
    for prompt in prompts {
        let stamp = parse_updated_at(prompt);
        let heading = if stamp == 0 {
            "Earlier".to_string()
        } else {
            day_label(stamp, clock)
        };
        let ids = tag_ids(prompt);
        let tags: Vec<QuickAccessPromptChip> = ids
            .iter()
            .filter(|id| **id != FAVORITE_TAG_ID)
            .filter_map(|id| state.tags.iter().find(|tag| text(tag, "tagId") == *id))
            .filter(|tag| text(tag, "tagId") != STASHED_TAG_ID)
            .map(|tag| QuickAccessPromptChip {
                label: text(tag, "name").to_string(),
                color: text(tag, "color").to_string(),
            })
            .collect();
        let content = js_trim(text(prompt, "content"));
        let lines: Vec<&str> = content.split('\n').collect();
        let mut tooltip = lines
            .iter()
            .take(TOOLTIP_LINE_COUNT)
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        if lines.len() > TOOLTIP_LINE_COUNT {
            tooltip.push_str("\n…");
        }
        let (time, suffix) = relative_time(text(prompt, "updatedAt"), true, now_ms);
        let row = QuickAccessRow::Prompt {
            key: prompt_row_key(text(prompt, "promptId")),
            title: prompt_title(prompt),
            tooltip,
            project_name: match opt(prompt, "projectName") {
                Some(name) => name.to_string(),
                None if state.view == PromptsView::Recovered => "Unknown project".to_string(),
                None => "No project".to_string(),
            },
            project_icon: prompt_project_icon(prompt),
            session_title: text(prompt, "sessionTitle").to_string(),
            tags,
            time: match suffix {
                Some(suffix) => format!("{time} {suffix}"),
                None => time,
            },
            is_favorite: ids.contains(&FAVORITE_TAG_ID),
        };
        match groups.iter_mut().find(|(existing, _)| *existing == heading) {
            Some((_, rows)) => rows.push(row),
            None => groups.push((heading, vec![row])),
        }
    }
    groups
        .into_iter()
        .map(|(heading, rows)| QuickAccessGroup {
            key: heading.clone(),
            heading,
            separated: false,
            rows,
        })
        .collect()
}

pub(crate) fn project_select(state: &PromptsTabState, data: &QuickAccessData) -> QuickAccessSelect {
    let projects = project_options(state, data);
    let scope = effective_scope(state, data);
    let value = match (scope, state.scope_project_id.as_deref()) {
        (PromptsScope::Session, _) => CURRENT_SESSION_VALUE.to_string(),
        (PromptsScope::Project, Some(id)) if !id.is_empty() => format!("project:{id}"),
        _ => ALL_PROJECTS_VALUE.to_string(),
    };
    let selected_name = if value == CURRENT_SESSION_VALUE {
        "This session".to_string()
    } else if value == ALL_PROJECTS_VALUE {
        "All projects".to_string()
    } else {
        projects
            .iter()
            .find(|(id, _)| format!("project:{id}") == value)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "All projects".to_string())
    };
    let mut options = vec![QuickAccessOption::plain(
        ALL_PROJECTS_VALUE,
        "All projects",
        value == ALL_PROJECTS_VALUE,
    )];
    if has_session_scope(state, data) {
        options.push(QuickAccessOption::plain(
            CURRENT_SESSION_VALUE,
            "This session",
            value == CURRENT_SESSION_VALUE,
        ));
    }
    options.extend(projects.iter().map(|(id, name)| {
        let option_value = format!("project:{id}");
        let selected = option_value == value;
        QuickAccessOption::plain(&option_value, name, selected)
    }));
    QuickAccessSelect {
        label: selected_name,
        detail: String::new(),
        color: String::new(),
        options,
        searchable: true,
        search_placeholder: "Filter projects...".to_string(),
    }
}

pub(crate) fn tag_select(
    state: &PromptsTabState,
    query: &str,
    data: &QuickAccessData,
) -> QuickAccessSelect {
    let scoped = scoped_prompts(state, query, data);
    let mut count_by_tag: BTreeMap<&str, usize> = BTreeMap::new();
    for prompt in &scoped {
        for id in tag_ids(prompt) {
            *count_by_tag.entry(id).or_default() += 1;
        }
    }
    let untagged = scoped
        .iter()
        .filter(|prompt| label_tag_ids(prompt).is_empty())
        .count();
    let has_tagged = state
        .prompts
        .iter()
        .flatten()
        .any(|prompt| !label_tag_ids(prompt).is_empty());
    let value = match &state.tag_filter {
        TagFilter::Tag(id) => format!("tag:{id}"),
        TagFilter::Untagged => NO_TAG_VALUE.to_string(),
        TagFilter::All => ALL_TAGS_VALUE.to_string(),
    };
    let mut options = vec![
        QuickAccessOption {
            icon: QuickAccessIcon::asset("plus", None),
            ..QuickAccessOption::plain("tag:new", "New tag…", false)
        },
        QuickAccessOption {
            color: "#ffffff".to_string(),
            separated: true,
            ..QuickAccessOption::plain(
                ALL_TAGS_VALUE,
                &format!("All tags ({})", scoped.len()),
                value == ALL_TAGS_VALUE,
            )
        },
    ];
    for tag in &state.tags {
        let tag_id = text(tag, "tagId");
        let option_value = format!("tag:{tag_id}");
        let selected = option_value == value;
        options.push(QuickAccessOption {
            color: text(tag, "color").to_string(),
            ..QuickAccessOption::plain(
                &option_value,
                &format!(
                    "{} ({})",
                    text(tag, "name"),
                    count_by_tag.get(tag_id).copied().unwrap_or(0)
                ),
                selected,
            )
        });
    }
    if has_tagged || state.tag_filter == TagFilter::Untagged {
        options.push(QuickAccessOption::plain(
            NO_TAG_VALUE,
            &format!("No tag ({untagged})"),
            value == NO_TAG_VALUE,
        ));
    }
    let selected = options.iter().find(|option| option.selected);
    QuickAccessSelect {
        label: selected
            .map(|option| option.label.clone())
            .unwrap_or_else(|| format!("All tags ({})", scoped.len())),
        detail: String::new(),
        color: selected
            .map(|option| option.color.clone())
            .unwrap_or_else(|| "#ffffff".to_string()),
        options,
        searchable: true,
        search_placeholder: "Filter tags...".to_string(),
    }
}

pub(crate) fn editor_state(
    state: &PromptsTabState,
    data: &QuickAccessData,
) -> Option<QuickAccessPromptEditor> {
    let editing = state.editing.as_ref()?;
    let projects = project_options(state, data);
    let selected_tag = state
        .tags
        .iter()
        .find(|tag| format!("tag:{}", text(tag, "tagId")) == editing.tag_value);
    let project_option = |value: &str, label: &str| {
        QuickAccessOption::plain(value, label, value == editing.project_value)
    };
    let tag_option = |value: &str, label: &str, color: &str| QuickAccessOption {
        color: color.to_string(),
        ..QuickAccessOption::plain(value, label, value == editing.tag_value)
    };
    let projects_select = if editing.prompt_id.is_some() {
        // Editing a prompt keeps its project; only a new prompt picks one.
        QuickAccessSelect::default()
    } else {
        let mut options = vec![project_option(NO_PROJECT_VALUE, "No project")];
        options.extend(
            projects
                .iter()
                .map(|(id, name)| project_option(&format!("project:{id}"), name)),
        );
        QuickAccessSelect {
            label: projects
                .iter()
                .find(|(id, _)| format!("project:{id}") == editing.project_value)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| "No project".to_string()),
            detail: String::new(),
            color: String::new(),
            options,
            searchable: true,
            search_placeholder: "Filter projects...".to_string(),
        }
    };
    let mut tag_options = vec![tag_option(NO_TAG_VALUE, "No tag", "")];
    tag_options.extend(
        state
            .tags
            .iter()
            .filter(|tag| text(tag, "tagId") != FAVORITE_TAG_ID)
            .map(|tag| {
                tag_option(
                    &format!("tag:{}", text(tag, "tagId")),
                    text(tag, "name"),
                    text(tag, "color"),
                )
            }),
    );
    Some(QuickAccessPromptEditor {
        heading: if editing.prompt_id.is_some() {
            "Edit Saved Prompt"
        } else {
            "Add Saved Prompt"
        }
        .to_string(),
        content: editing.content.clone(),
        projects: projects_select,
        tags: QuickAccessSelect {
            label: selected_tag
                .map(|tag| text(tag, "name").to_string())
                .unwrap_or_else(|| "No tag".to_string()),
            detail: String::new(),
            color: selected_tag
                .map(|tag| text(tag, "color").to_string())
                .unwrap_or_default(),
            options: tag_options,
            searchable: true,
            search_placeholder: "Filter tags...".to_string(),
        },
        is_favorite: editing.is_favorite,
        error: state.save_error.clone().unwrap_or_default(),
        saving: state.saving,
        submit_label: if state.saving {
            "Saving..."
        } else if editing.prompt_id.is_some() {
            "Save Changes"
        } else {
            "Add Prompt"
        }
        .to_string(),
    })
}

pub(crate) fn empty_copy(state: &PromptsTabState, data: &QuickAccessData) -> &'static str {
    if state.view == PromptsView::Sent {
        return "No sent messages match. The last 50 messages you send appear here.";
    }
    let scope = effective_scope(state, data);
    if state.view == PromptsView::Recovered {
        return match scope {
            PromptsScope::Session => "No recovered drafts came from this session.",
            PromptsScope::Project => "No recovered drafts came from this project.",
            PromptsScope::All => {
                "No recovered drafts. Unsent text and earlier draft versions show up here."
            }
        };
    }
    match (&state.tag_filter, scope) {
        (TagFilter::Tag(_), _) => "No saved prompts carry this tag yet.",
        (TagFilter::Untagged, _) => "Every saved prompt here already carries a tag.",
        (_, PromptsScope::Session) => "No saved prompts came from this session.",
        (_, PromptsScope::Project) => "No saved prompts came from this project.",
        _ => "No saved prompts match this search.",
    }
}

pub(crate) fn find_prompt<'a>(state: &'a PromptsTabState, key: &str) -> Option<&'a Value> {
    active_prompts(state)?
        .iter()
        .find(|prompt| prompt_row_key(text(prompt, "promptId")) == key)
}

/// `nextPromptTagIds`: Favorites and one label tag coexist; labels are exclusive.
pub(crate) fn next_tag_ids(prompt: &Value, tag_id: &str) -> Vec<String> {
    let current = tag_ids(prompt);
    let favorite: Vec<String> = if current.contains(&FAVORITE_TAG_ID) {
        vec![FAVORITE_TAG_ID.to_string()]
    } else {
        Vec::new()
    };
    if tag_id == FAVORITE_TAG_ID {
        let label = current.iter().find(|id| **id != FAVORITE_TAG_ID);
        let mut next: Vec<String> = if favorite.is_empty() {
            vec![FAVORITE_TAG_ID.to_string()]
        } else {
            Vec::new()
        };
        if let Some(label) = label {
            next.push((*label).to_string());
        }
        return next;
    }
    if current.contains(&tag_id) {
        return favorite;
    }
    let mut next = favorite;
    next.push(tag_id.to_string());
    next
}

/// The row tag menu: every non-Favorites tag as a toggle, then New tag….
pub(crate) fn tag_menu_items(state: &PromptsTabState, prompt: &Value) -> Vec<QuickAccessMenuItem> {
    let ids = tag_ids(prompt);
    let mut items: Vec<QuickAccessMenuItem> = state
        .tags
        .iter()
        .filter(|tag| text(tag, "tagId") != FAVORITE_TAG_ID)
        .map(|tag| {
            let tag_id = text(tag, "tagId");
            QuickAccessMenuItem {
                id: format!("tag:{tag_id}"),
                label: text(tag, "name").to_string(),
                icon: QuickAccessIcon::asset(
                    if ids.contains(&tag_id) {
                        "circle-check-filled"
                    } else {
                        "tag"
                    },
                    opt(tag, "color"),
                ),
                hotkey: String::new(),
                danger: false,
                disabled: false,
                separator: false,
            }
        })
        .collect();
    if !items.is_empty() {
        items.push(super::row_actions::separator());
    }
    items.push(QuickAccessMenuItem {
        id: "tag:new".to_string(),
        label: "New tag…".to_string(),
        icon: QuickAccessIcon::asset("plus", None),
        hotkey: String::new(),
        danger: false,
        disabled: false,
        separator: false,
    });
    items
}

/// The tag ids of a prompt, for the controller.
pub(crate) fn prompt_tag_ids(prompt: &Value) -> Vec<String> {
    tag_ids(prompt).into_iter().map(str::to_string).collect()
}
