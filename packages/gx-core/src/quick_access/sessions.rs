//! The Sessions tab: the open and closed merge, the scope, tag and project filters, paging, day
//! grouping and rows of packages/core-ui/previous-sessions-modal.tsx.
//!
//! Ported from `apps/desktop/sidebar/native-quick-access/sessions.ts` (deleted; see git history).
//!
//! SEE-ALSO: packages/core-ui/previous-session-search.ts (the search rules ported below).

use std::collections::BTreeMap;

use serde_json::Value;

use super::data::{QuickAccessData, QuickAccessSession, QuickAccessStoreGroup};
use super::session_titles::is_default_session_search_title;
use super::text::{
    day_label, format_file_size, is_letter_or_number, locale_compare, parse_timestamp,
    relative_time, QuickAccessClock,
};
use super::wire::{
    QuickAccessGroup, QuickAccessIcon, QuickAccessOption, QuickAccessRow, QuickAccessSelect,
};
use crate::sidebar_menu::colored_agent_logo;
use crate::sidebar_view::tags::{
    effective_tag, enabled_visible_tag_filters, matches_tag_filters, normalize_tag_list_items,
    tag_label, tag_list_item_filter, tag_presentation, TagCatalog,
};
use crate::sidebar_view::text::{is_js_whitespace, js_trim};
use crate::sidebar_view::TagListItemKind;

pub(crate) const SESSIONS_PAGE_SIZE: u64 = 80;
pub(crate) const SESSIONS_VISIBLE_WINDOW_MS: i64 = 14 * 24 * 60 * 60 * 1_000;
pub(crate) const SESSION_TRANSCRIPT_SIZE_BATCH_SIZE: usize = 24;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SessionScope {
    #[default]
    All,
    Closed,
    External,
}

impl SessionScope {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Closed => "closed",
            Self::External => "external",
        }
    }

    /// `command.scope as SessionScope`: a value the window does not send reads as no match, which
    /// every comparison below treats like `all`.
    pub(crate) fn parse(value: &str) -> Self {
        match value {
            "closed" => Self::Closed,
            "external" => Self::External,
            _ => Self::All,
        }
    }
}

/// `{ projectId, name }` from a page answer or a live group.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectOption {
    pub(crate) project_id: String,
    pub(crate) name: String,
}

/// `SessionsTabState`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SessionsTabState {
    pub(crate) scope: SessionScope,
    pub(crate) project_id: String,
    pub(crate) tag_filters: Vec<String>,
    pub(crate) remote_sessions: Option<Vec<Value>>,
    pub(crate) cursor: Option<String>,
    pub(crate) project_options: Vec<ProjectOption>,
    pub(crate) resolved_query_key: Option<String>,
    pub(crate) history_window_count: i64,
    pub(crate) history_anchor_ms: i64,
    /// `fileSizesByKey`: absent while loading, `None` once the answer said "no size".
    pub(crate) file_sizes_by_key: BTreeMap<String, Option<i64>>,
}

impl SessionsTabState {
    /// `createSessionsTabState()`.
    pub(crate) fn new(now_ms: i64) -> Self {
        Self {
            scope: SessionScope::All,
            project_id: String::new(),
            tag_filters: Vec::new(),
            remote_sessions: None,
            cursor: None,
            project_options: Vec::new(),
            resolved_query_key: None,
            history_window_count: 1,
            history_anchor_ms: now_ms,
            file_sizes_by_key: BTreeMap::new(),
        }
    }

    pub(crate) fn has_filters_or_query(&self, query: &str) -> bool {
        !js_trim(query).is_empty()
            || !self.tag_filters.is_empty()
            || !self.project_id.is_empty()
            || self.scope == SessionScope::External
    }
}

/// `sessionsQueryKey`: the answer a page must match to count as this query's.
pub(crate) fn sessions_query_key(state: &SessionsTabState, query: &str) -> String {
    let mut tags = state.tag_filters.clone();
    tags.sort_by(|left, right| left.encode_utf16().cmp(right.encode_utf16()));
    serde_json::json!([
        js_trim(query),
        tags,
        state.project_id,
        state.scope == SessionScope::External
    ])
    .to_string()
}

/// A session either kind of row reads, open or closed.
pub(crate) trait SessionFields {
    fn field(&self, key: &str) -> Option<&str>;
    fn is_favorite(&self) -> bool;
}

impl SessionFields for QuickAccessSession {
    fn field(&self, key: &str) -> Option<&str> {
        match key {
            "alias" => Some(self.alias.as_str()),
            "displayTitle" => self.display_title.as_deref(),
            "primaryTitle" => self.primary_title.as_deref(),
            "terminalTitle" => self.terminal_title.as_deref(),
            "detail" => self.detail.as_deref(),
            "sessionNumber" => self.session_number.as_deref(),
            "sessionTag" => self.session_tag.as_deref(),
            "faviconDataUrl" => self.favicon_data_url.as_deref(),
            "agentIcon" => self.agent_icon.as_deref(),
            "sessionKind" => self.session_kind.as_deref(),
            _ => None,
        }
    }
    fn is_favorite(&self) -> bool {
        self.is_favorite
    }
}

impl SessionFields for Value {
    fn field(&self, key: &str) -> Option<&str> {
        self[key].as_str()
    }
    fn is_favorite(&self) -> bool {
        self["isFavorite"] == Value::Bool(true)
    }
}

/// `getSessionHistoryCardTitle`.
pub(crate) fn card_title(session: &dyn SessionFields) -> String {
    for key in ["displayTitle", "primaryTitle", "terminalTitle"] {
        let value = js_trim(session.field(key).unwrap_or(""));
        if !value.is_empty() {
            return value.to_string();
        }
    }
    session.field("alias").unwrap_or("").to_string()
}

fn session_effective_tag(session: &dyn SessionFields) -> Option<String> {
    effective_tag(session.field("sessionTag"), session.is_favorite())
}

/// `normalizeSessionSearchValue`: camelCase split, every run of non-letters one space, lowercase.
pub(crate) fn normalize_search_value(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let mut out = String::with_capacity(value.len());
    let mut previous: Option<char> = None;
    let mut pending_space = false;
    for character in value.chars() {
        let splits_camel = previous
            .is_some_and(|previous| previous.is_ascii_lowercase() || previous.is_ascii_digit())
            && character.is_ascii_uppercase();
        if is_letter_or_number(character) {
            if (pending_space || splits_camel) && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(character);
        } else {
            pending_space = true;
        }
        previous = Some(character);
    }
    out.to_lowercase()
}

fn search_text(session: &dyn SessionFields, catalog: &TagCatalog) -> String {
    let tag = session_effective_tag(session);
    let label = tag_label(tag.as_deref(), catalog);
    [
        "alias",
        "displayTitle",
        "primaryTitle",
        "terminalTitle",
        "detail",
        "sessionNumber",
    ]
    .iter()
    .map(|key| normalize_search_value(session.field(key)))
    .chain(std::iter::once(normalize_search_value(label.as_deref())))
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

/// `filterSidebarSessionItems`: the indices of the sessions the query keeps, in order.
pub(crate) fn filter_sessions(
    sessions: &[&dyn SessionFields],
    query: &str,
    catalog: &TagCatalog,
) -> Vec<usize> {
    let normalized_query = normalize_search_value(Some(query));
    let searchable: Vec<usize> = (0..sessions.len())
        .filter(|index| !is_default_session_search_title(&card_title(sessions[*index])))
        .collect();
    if normalized_query.is_empty() {
        return searchable;
    }
    let tokens: Vec<&str> = normalized_query
        .split(is_js_whitespace)
        .filter(|token| !token.is_empty())
        .collect();
    searchable
        .into_iter()
        .filter(|index| {
            let text = search_text(sessions[*index], catalog);
            tokens.iter().all(|token| matches_token(&text, token))
        })
        .collect()
}

fn utf16(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

/// `matchesNormalizedQueryToken`.
fn matches_token(search_text: &str, query: &str) -> bool {
    let query_units = utf16(query);
    if query_units.len() <= 3 {
        return fuzzy_includes(search_text, &query_units);
    }
    if search_text.contains(query) {
        return true;
    }
    let compact: String = search_text
        .chars()
        .filter(|c| !is_js_whitespace(*c))
        .collect();
    if (query_units.len() >= 5 && compact.contains(query))
        || single_edit_distance(&utf16(&compact), &query_units)
    {
        return true;
    }
    let words: Vec<&str> = search_text
        .split(is_js_whitespace)
        .filter(|word| !word.is_empty())
        .collect();
    words
        .iter()
        .any(|word| long_query_typo_candidate(&utf16(word), &query_units))
        || adjacent_words_single_edit(&words, &query_units)
}

/// `fuzzyIncludes`: the query's code units in order among the text's code points.
fn fuzzy_includes(text: &str, query: &[u16]) -> bool {
    let mut index = 0;
    for character in text.chars() {
        let mut buffer = [0u16; 2];
        let encoded = character.encode_utf16(&mut buffer);
        if encoded.len() != 1 || query.get(index) != Some(&encoded[0]) {
            continue;
        }
        index += 1;
        if index >= query.len() {
            return true;
        }
    }
    query.is_empty()
}

/// `hasSingleEditDistance`.
fn single_edit_distance(candidate: &[u16], query: &[u16]) -> bool {
    if candidate == query {
        return true;
    }
    if candidate.len().abs_diff(query.len()) > 1 {
        return false;
    }
    let (mut c, mut q, mut edits) = (0, 0, 0);
    while c < candidate.len() && q < query.len() {
        if candidate[c] == query[q] {
            c += 1;
            q += 1;
            continue;
        }
        edits += 1;
        if edits > 1 {
            return false;
        }
        match candidate.len().cmp(&query.len()) {
            std::cmp::Ordering::Greater => c += 1,
            std::cmp::Ordering::Less => q += 1,
            std::cmp::Ordering::Equal => {
                c += 1;
                q += 1;
            }
        }
    }
    true
}

/// `isLongQueryTypoCandidate`.
fn long_query_typo_candidate(candidate: &[u16], query: &[u16]) -> bool {
    candidate.len() >= 4.max(query.len().saturating_sub(1))
        && single_edit_distance(candidate, query)
}

/// `hasAdjacentWordSingleEditDistance`.
fn adjacent_words_single_edit(words: &[&str], query: &[u16]) -> bool {
    for start in 0..words.len() {
        let mut joined: Vec<u16> = Vec::new();
        for word in &words[start..] {
            joined.extend(word.encode_utf16());
            if joined.len() > query.len() + 1 {
                break;
            }
            if long_query_typo_candidate(&joined, query) {
                return true;
            }
        }
    }
    false
}

/// `filterPreviousSessionsModalItems`: no web pages.
fn is_web_page(session: &Value) -> bool {
    session["sessionKind"] == "browser"
        || session["sessionRecord"]["kind"] == "browser"
        || session["agentIcon"] == "browser"
}

fn timestamp_of(value: &Value) -> i64 {
    parse_timestamp(value.as_str()).unwrap_or(0)
}

/// `dedupePreviousSessionsByProjectAndTitle`: one row per project and title, the newest closed.
fn dedupe_previous(sessions: Vec<&Value>) -> Vec<&Value> {
    let mut kept: Vec<(String, usize, i64)> = Vec::new();
    for (index, session) in sessions.iter().enumerate() {
        let key = dedupe_key(session);
        let closed = timestamp_of(&session["closedAt"]);
        let stamp = if closed != 0 {
            closed
        } else {
            timestamp_of(&session["lastInteractionAt"])
        };
        match kept.iter_mut().find(|(existing, _, _)| *existing == key) {
            Some(entry) if entry.2 >= stamp => {}
            Some(entry) => {
                entry.1 = index;
                entry.2 = stamp;
            }
            None => kept.push((key, index, stamp)),
        }
    }
    let mut indices: Vec<usize> = kept.into_iter().map(|(_, index, _)| index).collect();
    indices.sort_unstable();
    indices.into_iter().map(|index| sessions[index]).collect()
}

fn dedupe_key(session: &Value) -> String {
    let history_id = session["historyId"].as_str().unwrap_or("");
    if session["externalSession"].as_bool().unwrap_or(false) {
        return history_id.to_string();
    }
    let project = ["projectPath", "projectId", "projectName"]
        .iter()
        .find_map(|key| session[*key].as_str().filter(|value| !value.is_empty()))
        .unwrap_or("");
    let project_key = normalize_search_value(Some(project));
    let scoped = if project_key.is_empty() {
        format!("history:{history_id}")
    } else {
        project_key
    };
    let title_key = normalize_search_value(Some(&card_title(session)));
    let branch_count = session["forkBranchCount"].as_f64().unwrap_or(0.0);
    let branch = if branch_count != 0.0 {
        let id = session["sessionId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(history_id);
        format!("\u{0}branch:{id}")
    } else {
        String::new()
    };
    format!("{scoped}\u{0}{title_key}{branch}")
}

/// `filterPreviousSessions(sessions, query, { sessionTags })`.
fn filter_previous<'a>(
    sessions: Vec<&'a Value>,
    query: &str,
    tag_filters: &[String],
    catalog: &TagCatalog,
) -> Vec<&'a Value> {
    let filtered: Vec<&Value> = if tag_filters.is_empty() {
        sessions
    } else {
        sessions
            .into_iter()
            .filter(|session| {
                matches_tag_filters(session_effective_tag(*session).as_deref(), tag_filters)
            })
            .collect()
    };
    let deduped = dedupe_previous(filtered);
    if js_trim(query).is_empty() {
        return deduped;
    }
    let fields: Vec<&dyn SessionFields> =
        deduped.iter().map(|s| *s as &dyn SessionFields).collect();
    filter_sessions(&fields, query, catalog)
        .into_iter()
        .map(|index| deduped[index])
        .collect()
}

/// One row of the tab before it is drawn.
pub(crate) enum SessionItem<'a> {
    Open {
        key: String,
        project_label: Option<String>,
        session: &'a QuickAccessSession,
        timestamp: i64,
    },
    Closed {
        key: String,
        session: &'a Value,
        timestamp: i64,
    },
}

impl SessionItem<'_> {
    pub(crate) fn key(&self) -> &str {
        match self {
            Self::Open { key, .. } | Self::Closed { key, .. } => key,
        }
    }
    fn timestamp(&self) -> i64 {
        match self {
            Self::Open { timestamp, .. } | Self::Closed { timestamp, .. } => *timestamp,
        }
    }
}

/// `getQuickAccessSessionProjectId`.
pub(crate) fn group_project_id(group: &QuickAccessStoreGroup) -> Option<String> {
    match (&group.remote_machine_id, &group.remote_project_id) {
        (Some(machine), Some(project)) if !project.is_empty() => {
            Some(format!("remote:{machine}:project:{project}"))
        }
        _ => group.editor_project_id.clone(),
    }
}

/// The merged catalog every label and tag icon resolves against (`getSessionTagCatalogs`).
pub(crate) fn merged_catalog(data: &QuickAccessData) -> TagCatalog {
    TagCatalog::merged(
        std::iter::once(data.local_custom_tags.as_ref())
            .chain(data.remote_custom_tags.iter().map(Some)),
    )
}

/// `visibleSessionItems`: open and closed rows, newest first.
pub(crate) fn visible_items<'a>(
    state: &'a SessionsTabState,
    query: &str,
    data: &'a QuickAccessData,
) -> Vec<SessionItem<'a>> {
    let catalog = merged_catalog(data);
    let has_tag_filters = !state.tag_filters.is_empty();
    let external_only = state.scope == SessionScope::External;
    let closed_only = state.scope == SessionScope::Closed;
    let mut items: Vec<SessionItem<'a>> = Vec::new();
    if !closed_only && !external_only {
        let open: Vec<(&QuickAccessStoreGroup, &QuickAccessSession)> = data
            .groups
            .iter()
            .flat_map(|group| group.sessions.iter().map(move |session| (group, session)))
            .filter(|(_, session)| {
                !has_tag_filters
                    || matches_tag_filters(
                        session_effective_tag(*session).as_deref(),
                        &state.tag_filters,
                    )
            })
            .collect();
        let fields: Vec<&dyn SessionFields> = open
            .iter()
            .map(|(_, session)| *session as &dyn SessionFields)
            .collect();
        for index in filter_sessions(&fields, query, &catalog) {
            let (group, session) = open[index];
            if !state.project_id.is_empty()
                && group_project_id(group).as_deref() != Some(state.project_id.as_str())
            {
                continue;
            }
            let title = js_trim(&group.title);
            items.push(SessionItem::Open {
                key: format!("open:{}", session.session_id),
                project_label: (!title.is_empty()).then(|| title.to_string()),
                session,
                timestamp: parse_timestamp(session.last_interaction_at.as_deref()).unwrap_or(0),
            });
        }
    }
    let previous: Vec<&Value> = state
        .remote_sessions
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter(|session| !is_web_page(session))
        .filter(|session| {
            (state.project_id.is_empty()
                || session["projectId"].as_str() == Some(state.project_id.as_str()))
                && (!external_only || session["externalSession"].as_bool().unwrap_or(false))
        })
        .collect();
    let mut closed = filter_previous(previous, query, &state.tag_filters, &catalog);
    closed.sort_by(|left, right| {
        timestamp_of(&right["closedAt"])
            .cmp(&timestamp_of(&left["closedAt"]))
            .then_with(|| {
                locale_compare(
                    left["historyId"].as_str().unwrap_or(""),
                    right["historyId"].as_str().unwrap_or(""),
                )
            })
    });
    let has_filters = has_tag_filters || !state.project_id.is_empty() || external_only;
    let cutoff = state.history_anchor_ms - state.history_window_count * SESSIONS_VISIBLE_WINDOW_MS;
    for session in closed {
        let closed_at = timestamp_of(&session["closedAt"]);
        if js_trim(query).is_empty() && !has_filters && closed_at < cutoff {
            continue;
        }
        items.push(SessionItem::Closed {
            key: format!("closed:{}", session["historyId"].as_str().unwrap_or("")),
            session,
            timestamp: closed_at,
        });
    }
    items.sort_by(|left, right| {
        right
            .timestamp()
            .cmp(&left.timestamp())
            .then_with(|| locale_compare(left.key(), right.key()))
    });
    items
}

/// `sessionProjectOptions`: the page answer's projects plus the live groups', by name.
pub(crate) fn project_options(
    state: &SessionsTabState,
    data: &QuickAccessData,
) -> Vec<ProjectOption> {
    let mut options: Vec<ProjectOption> = Vec::new();
    for option in &state.project_options {
        match options
            .iter_mut()
            .find(|seen| seen.project_id == option.project_id)
        {
            Some(seen) => *seen = option.clone(),
            None => options.push(option.clone()),
        }
    }
    for group in &data.groups {
        if let Some(project_id) = group_project_id(group).filter(|id| !id.is_empty()) {
            if !options.iter().any(|seen| seen.project_id == project_id) {
                options.push(ProjectOption {
                    project_id,
                    name: group.title.clone(),
                });
            }
        }
    }
    options.sort_by(|left, right| {
        locale_compare(&left.name, &right.name)
            .then_with(|| locale_compare(&left.project_id, &right.project_id))
    });
    options
}

fn closed_project_label(session: &Value) -> String {
    let name = js_trim(session["projectName"].as_str().unwrap_or(""));
    if !name.is_empty() {
        return name.to_string();
    }
    let path = js_trim(session["projectPath"].as_str().unwrap_or(""));
    if path.is_empty() {
        return String::new();
    }
    path.split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .next_back()
        .unwrap_or(path)
        .to_string()
}

/// The leading identity glyph, in `SessionFloatingAgentIcon`'s order: the session tag at rest,
/// then the browser favicon, the colored agent logo, and the terminal glyph for an agentless
/// terminal row.
fn session_icon(session: &dyn SessionFields, catalog: &TagCatalog) -> QuickAccessIcon {
    if let Some(tag) = session_effective_tag(session).filter(|tag| !tag.is_empty()) {
        return match tag_presentation(Some(&tag), catalog) {
            Some(presentation) => {
                QuickAccessIcon::asset(&presentation.icon, Some(&presentation.icon_color))
            }
            None => QuickAccessIcon::asset("tag", None),
        };
    }
    if let Some(favicon) = QuickAccessIcon::image(session.field("faviconDataUrl")) {
        return favicon;
    }
    if let Some(agent_icon) = session.field("agentIcon").filter(|icon| !icon.is_empty()) {
        if let Some(logo) = QuickAccessIcon::image(colored_agent_logo(agent_icon)) {
            return logo;
        }
    }
    // `shouldShowTerminalSessionIcon`.
    let agentless = session.field("agentIcon").is_none_or(str::is_empty);
    let terminal_kind = session
        .field("sessionKind")
        .is_none_or(|kind| kind == "terminal");
    if agentless && terminal_kind {
        return QuickAccessIcon::asset("terminal-2", None);
    }
    QuickAccessIcon::None
}

/// `buildSessionGroups`.
pub(crate) fn session_groups(
    state: &SessionsTabState,
    query: &str,
    data: &QuickAccessData,
    clock: &dyn QuickAccessClock,
) -> Vec<QuickAccessGroup> {
    let catalog = merged_catalog(data);
    let now_ms = clock.now_ms();
    let mut groups: Vec<(String, Vec<QuickAccessRow>)> = Vec::new();
    for item in visible_items(state, query, data) {
        let heading = if item.timestamp() == 0 {
            "Unknown day".to_string()
        } else {
            day_label(item.timestamp(), clock)
        };
        let size = state.file_sizes_by_key.get(item.key());
        let (session, stamp, in_sidebar, project_label, sleeping, can_activate): (
            &dyn SessionFields,
            Option<&str>,
            bool,
            String,
            bool,
            bool,
        ) = match &item {
            SessionItem::Open {
                session,
                project_label,
                ..
            } => (
                *session,
                session.last_interaction_at.as_deref(),
                true,
                project_label.clone().unwrap_or_default(),
                session.lifecycle_state.as_deref() == Some("sleeping"),
                true,
            ),
            SessionItem::Closed { session, .. } => (
                *session,
                session["closedAt"].as_str(),
                false,
                closed_project_label(session),
                false,
                session["isRestorable"] == Value::Bool(true),
            ),
        };
        let row = QuickAccessRow::Session {
            key: item.key().to_string(),
            title: card_title(session),
            icon: session_icon(session, &catalog),
            project_label,
            file_size: match size {
                None => "…".to_string(),
                Some(None) => "-".to_string(),
                Some(Some(bytes)) => format_file_size(*bytes),
            },
            file_size_loading: size.is_none(),
            time: match stamp.filter(|stamp| !stamp.is_empty()) {
                Some(stamp) => relative_time(stamp, false, now_ms).0,
                None => String::new(),
            },
            in_sidebar,
            sleeping,
            can_activate,
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

/// The tag filter menu, built from the Settings-managed sidebar tag row set.
pub(crate) fn tag_select(state: &SessionsTabState, data: &QuickAccessData) -> QuickAccessSelect {
    let local = TagCatalog::from_state(data.local_custom_tags.as_ref());
    let settings_items = &data.settings()["sidebarSessionTagListItems"];
    let items = normalize_tag_list_items(settings_items, Some(&local));
    let enabled = enabled_visible_tag_filters(settings_items, &local);
    let catalog = merged_catalog(data);
    let mut options: Vec<QuickAccessOption> = Vec::new();
    let mut pending_separator = false;
    for item in &items {
        if !item.visible {
            continue;
        }
        if item.kind == TagListItemKind::Separator {
            pending_separator = item.enabled && !options.is_empty();
            continue;
        }
        let Some(filter) = tag_list_item_filter(item) else {
            continue;
        };
        let separated = pending_separator;
        pending_separator = false;
        options.push(QuickAccessOption {
            value: filter.to_string(),
            label: tag_label(Some(filter), &catalog).unwrap_or_else(|| filter.to_string()),
            detail: String::new(),
            color: String::new(),
            icon: QuickAccessIcon::asset(
                if filter == "favorite" {
                    "star-filled"
                } else {
                    "tag"
                },
                None,
            ),
            disabled: !item.enabled || !enabled.iter().any(|tag| tag == filter),
            selected: state.tag_filters.iter().any(|tag| tag == filter),
            separated,
        });
    }
    QuickAccessSelect {
        label: if state.tag_filters.is_empty() {
            "All tags".to_string()
        } else {
            format!("{} tags", state.tag_filters.len())
        },
        detail: String::new(),
        color: String::new(),
        options,
        searchable: true,
        search_placeholder: "Filter tags...".to_string(),
    }
}

pub(crate) fn project_select(
    state: &SessionsTabState,
    data: &QuickAccessData,
) -> QuickAccessSelect {
    let projects = project_options(state, data);
    let label = projects
        .iter()
        .find(|project| project.project_id == state.project_id)
        .map(|project| project.name.clone())
        .unwrap_or_else(|| "All projects".to_string());
    let mut options = vec![QuickAccessOption::plain(
        "",
        "All projects",
        state.project_id.is_empty(),
    )];
    options.extend(projects.iter().map(|project| {
        QuickAccessOption::plain(
            &project.project_id,
            &project.name,
            project.project_id == state.project_id,
        )
    }));
    QuickAccessSelect {
        label,
        detail: String::new(),
        color: String::new(),
        options,
        searchable: true,
        search_placeholder: "Filter projects...".to_string(),
    }
}

/// The empty copy the React modal shows for the active scope and filters.
pub(crate) fn empty_copy(state: &SessionsTabState, query: &str) -> String {
    let has_tags = !state.tag_filters.is_empty();
    let query_empty = js_trim(query).is_empty();
    if state.scope == SessionScope::External
        && query_empty
        && !has_tags
        && state.project_id.is_empty()
    {
        return "No Claude or Codex conversations found outside Ghostex.".to_string();
    }
    let prefix = if state.scope == SessionScope::Closed {
        "closed "
    } else {
        ""
    };
    if !query_empty {
        return if has_tags {
            format!("No tagged {prefix}sessions match that search.")
        } else {
            format!("No {prefix}sessions match that search.")
        };
    }
    if has_tags {
        format!("No {prefix}sessions match those tags.")
    } else {
        format!("No {prefix}sessions yet.")
    }
}

pub(crate) fn loading_copy(state: &SessionsTabState) -> &'static str {
    match state.scope {
        SessionScope::External => "Loading external sessions...",
        SessionScope::Closed => "Loading closed sessions...",
        SessionScope::All => "Loading sessions...",
    }
}
