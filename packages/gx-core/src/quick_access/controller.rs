//! The Quick Access controller: data requests, filters, ranking, grouping, selection and every
//! command, publishing one resolved snapshot per frame to the native window.
//!
//! CDXC:AppModal 2026-09-25 WHY:
//! This is apps/desktop/sidebar/native-quick-access/controller.ts (since deleted) moved out of the app runtime, one
//! branch for one branch, so the window, the hotkeys and every request the host answers see exactly
//! what they saw from QuickJS. The core performs nothing: every post, modal, clipboard write and
//! timer comes back as a [`QuickAccessEffect`] and the host performs it, and the client storage the
//! TypeScript touched in place goes through [`QuickAccessStorage`].
//!
//! SEE-ALSO: apps/desktop/src/app/quick_access/ (the host), apps/desktop/src/app/window/quick_access/
//! (the window).

use std::collections::BTreeSet;

use serde_json::{json, Value};

use super::data::{QuickAccessData, QuickAccessStorage};
use super::projects::RecentProjectsData;
use super::prompts::{
    self, apply_launcher_context, belongs_to_session, has_session_scope, Composer, PromptsScope,
    PromptsTabState, PromptsView, TagFilter, ALL_PROJECTS_VALUE, ALL_TAGS_VALUE,
    CURRENT_SESSION_VALUE, FAVORITE_TAG_ID, NO_TAG_VALUE, STASHED_PROMPT_TAG_COLORS,
};
use super::sessions::{
    self, sessions_query_key, visible_items, ProjectOption, SessionItem, SessionScope,
    SessionsTabState, SESSIONS_PAGE_SIZE, SESSIONS_VISIBLE_WINDOW_MS,
    SESSION_TRANSCRIPT_SIZE_BATCH_SIZE,
};
use super::text::{collapse_runs, js_lower, QuickAccessClock};
use super::wire::QuickAccessUpdate;
use crate::sidebar_view::text::js_trim;

const SESSIONS_QUERY_DEBOUNCE_MS: u64 = 200;

/// The four tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuickAccessTab {
    #[default]
    Commands,
    RecentProjects,
    RecentSessions,
    SavedPrompts,
}

impl QuickAccessTab {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Commands => "commands",
            Self::RecentProjects => "recentProjects",
            Self::RecentSessions => "recentSessions",
            Self::SavedPrompts => "savedPrompts",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "commands" => Self::Commands,
            "recentProjects" => Self::RecentProjects,
            "recentSessions" => Self::RecentSessions,
            "savedPrompts" => Self::SavedPrompts,
            _ => return None,
        })
    }
}

/// What the host must do. Performed in order.
#[derive(Clone, Debug, PartialEq)]
pub enum QuickAccessEffect {
    /// Hand this to the window (`postNativeQuickAccessSnapshot`).
    Update(QuickAccessUpdate),
    /// A modal-host `sidebarCommand` carrying this message.
    Post(Value),
    /// A modal-host `open` message (`openAppModal`).
    OpenModal(Value),
    /// A modal-host `copySessionDetails` with this text.
    CopyText(String),
    /// Call [`QuickAccessController::flush_publish`] on the next frame.
    SchedulePublish,
    /// Call [`QuickAccessController::sessions_timer_fired`] with this token after the delay. A newer
    /// token replaces an older one; a stale one does nothing.
    ScheduleSessionsRequest { token: u64, delay_ms: u64 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PromptRequestIds {
    list: Option<String>,
    recovered: Option<String>,
    sent: Option<String>,
    save: Option<String>,
}

/// The controller's whole state.
#[derive(Clone, Debug)]
pub struct QuickAccessController {
    pub(crate) open: bool,
    pub(crate) tab: QuickAccessTab,
    pub(crate) query: String,
    pub(crate) query_revision: u64,
    pub(crate) selected_key: String,
    pub(crate) selection_seq: u64,
    pub(crate) machine_id: Option<String>,
    pub(crate) projects: Option<RecentProjectsData>,
    pub(crate) sessions: SessionsTabState,
    pub(crate) prompts: PromptsTabState,
    menu_row_key: Option<String>,
    menu_prompt_key: Option<String>,
    opened_prompt_scope: Option<PromptsScope>,
    pending_publish: bool,
    sessions_timer: Option<u64>,
    next_token: u64,
    sessions_request_id: Option<String>,
    sessions_request_append: bool,
    sessions_request_query_key: String,
    sessions_loading_more: bool,
    external_refresh_pending: bool,
    requested_file_size_keys: BTreeSet<String>,
    file_size_request_ids: BTreeSet<String>,
    prompt_request_ids: PromptRequestIds,
    pending_tag_application: Option<(String, Option<String>)>,
    request_counter: u64,
}

/// The borrowed context every entry point is given.
pub struct QuickAccessContext<'a> {
    pub data: &'a QuickAccessData,
    pub storage: &'a mut dyn QuickAccessStorage,
    pub clock: &'a dyn QuickAccessClock,
}

impl QuickAccessController {
    pub fn new(now_ms: i64) -> Self {
        Self {
            open: false,
            tab: QuickAccessTab::Commands,
            query: String::new(),
            query_revision: 0,
            selected_key: String::new(),
            selection_seq: 0,
            machine_id: None,
            projects: None,
            sessions: SessionsTabState::new(now_ms),
            prompts: PromptsTabState::default(),
            menu_row_key: None,
            menu_prompt_key: None,
            opened_prompt_scope: None,
            pending_publish: false,
            sessions_timer: None,
            next_token: 0,
            sessions_request_id: None,
            sessions_request_append: false,
            sessions_request_query_key: String::new(),
            sessions_loading_more: false,
            external_refresh_pending: false,
            requested_file_size_keys: BTreeSet::new(),
            file_size_request_ids: BTreeSet::new(),
            prompt_request_ids: PromptRequestIds::default(),
            pending_tag_application: None,
            request_counter: 0,
        }
    }

    /// Whether the window is showing, so the host knows whether a store change must republish.
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn tab(&self) -> QuickAccessTab {
        self.tab
    }

    pub(crate) fn next_request_id(&mut self, kind: &str, clock: &dyn QuickAccessClock) -> String {
        self.request_counter += 1;
        format!("{kind}-{}-{}", clock.now_ms(), self.request_counter)
    }

    pub(crate) fn close(&mut self, effects: &mut Vec<QuickAccessEffect>) {
        self.open = false;
        effects.push(QuickAccessEffect::Update(QuickAccessUpdate::Close));
    }

    /// `publish`: at most one snapshot per frame, and none while the window is gone.
    pub(crate) fn publish(&mut self, effects: &mut Vec<QuickAccessEffect>) {
        if !self.open || self.pending_publish {
            return;
        }
        self.pending_publish = true;
        effects.push(QuickAccessEffect::SchedulePublish);
    }

    /// The sidebar store moved while the window may be showing (`sidebarStore.subscribe(publish)`).
    pub fn store_changed(&mut self) -> Vec<QuickAccessEffect> {
        let mut effects = Vec::new();
        self.publish(&mut effects);
        effects
    }

    /// The frame callback: the snapshot, then the transcript sizes of the rows it shows.
    pub fn flush_publish(
        &mut self,
        context: &mut QuickAccessContext<'_>,
    ) -> Vec<QuickAccessEffect> {
        let mut effects = Vec::new();
        self.pending_publish = false;
        if !self.open {
            return effects;
        }
        let snapshot = self.snapshot(context);
        effects.push(QuickAccessEffect::Update(QuickAccessUpdate::Snapshot(
            Box::new(snapshot),
        )));
        self.request_visible_session_sizes(context, &mut effects);
        effects
    }

    // ------------------------------------------------------------------------------ requests

    fn request_recent_projects(&mut self, effects: &mut Vec<QuickAccessEffect>) {
        let mut message = json!({ "type": "requestRecentProjects" });
        if let Some(machine_id) = &self.machine_id {
            message["machineId"] = json!(machine_id);
        }
        effects.push(QuickAccessEffect::Post(message));
    }

    pub(crate) fn request_sessions_page(
        &mut self,
        append: bool,
        cursor: Option<String>,
        context: &QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        if append && (cursor.is_none() || self.sessions_loading_more) {
            return;
        }
        let request_id = self.next_request_id("previous-sessions", context.clock);
        let refresh_external = !append && self.external_refresh_pending;
        if refresh_external {
            self.external_refresh_pending = false;
            self.sessions.resolved_query_key = None;
        }
        self.sessions_request_id = Some(request_id.clone());
        self.sessions_request_append = append;
        self.sessions_request_query_key = sessions_query_key(&self.sessions, &self.query);
        if append {
            self.sessions_loading_more = true;
        } else {
            self.sessions_loading_more = false;
            self.sessions.cursor = None;
        }
        let mut message = json!({
            "limit": SESSIONS_PAGE_SIZE,
            "requestId": request_id,
            "sessionTags": self.sessions.tag_filters,
            "externalOnly": self.sessions.scope == SessionScope::External,
            "refreshExternalSessions": refresh_external,
            "type": "requestPreviousSessions",
        });
        if let Some(cursor) = cursor {
            message["cursor"] = json!(cursor);
        }
        let query = js_trim(&self.query);
        if !query.is_empty() {
            message["query"] = json!(query);
        }
        if !self.sessions.project_id.is_empty() {
            message["projectId"] = json!(self.sessions.project_id);
        }
        effects.push(QuickAccessEffect::Post(message));
    }

    /// CDXC:AppModal 2026-09-20 WHY:
    /// The unfiltered list shows a rolling 14-day window, so the controller keeps paging gxserver
    /// until the oldest loaded session predates that cutoff. Without it the first page can end
    /// inside the window and the list looks like it stops early.
    fn fill_visible_history_window(
        &mut self,
        context: &QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        if self.tab != QuickAccessTab::RecentSessions || self.sessions_loading_more {
            return;
        }
        let Some(cursor) = self.sessions.cursor.clone() else {
            return;
        };
        if self.sessions.has_filters_or_query(&self.query) {
            return;
        }
        let cutoff = self.sessions.history_anchor_ms
            - self.sessions.history_window_count * SESSIONS_VISIBLE_WINDOW_MS;
        let oldest = self
            .sessions
            .remote_sessions
            .iter()
            .flatten()
            .filter_map(|session| {
                super::text::parse_timestamp(session["closedAt"].as_str()).filter(|at| *at > 0)
            })
            .min();
        if oldest.is_some_and(|oldest| oldest <= cutoff) {
            return;
        }
        self.request_sessions_page(true, Some(cursor), context, effects);
    }

    pub(crate) fn schedule_sessions_request(
        &mut self,
        immediate: bool,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        self.next_token += 1;
        self.sessions_timer = Some(self.next_token);
        effects.push(QuickAccessEffect::ScheduleSessionsRequest {
            token: self.next_token,
            delay_ms: if immediate {
                0
            } else {
                SESSIONS_QUERY_DEBOUNCE_MS
            },
        });
    }

    /// The debounced sessions request is due.
    pub fn sessions_timer_fired(
        &mut self,
        token: u64,
        context: &mut QuickAccessContext<'_>,
    ) -> Vec<QuickAccessEffect> {
        let mut effects = Vec::new();
        if self.sessions_timer != Some(token) {
            return effects;
        }
        self.sessions_timer = None;
        self.request_sessions_page(false, None, context, &mut effects);
        effects
    }

    /// CDXC:AppModal 2026-09-20 WHY:
    /// The React list used an IntersectionObserver to defer transcript sizes until a row
    /// approached the viewport. The native list has no observer, so the sizes of the rows being
    /// published are requested in the same bounded batches.
    fn request_visible_session_sizes(
        &mut self,
        context: &QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        if self.tab != QuickAccessTab::RecentSessions {
            return;
        }
        let mut targets: Vec<Value> = Vec::new();
        for item in visible_items(&self.sessions, &self.query, context.data) {
            if self.requested_file_size_keys.contains(item.key()) {
                continue;
            }
            let target = match &item {
                SessionItem::Closed { session, key, .. } => {
                    Some(json!({ "historyId": session["historyId"], "key": key }))
                }
                SessionItem::Open { session, key, .. } => session
                    .session_routing_id
                    .as_ref()
                    .filter(|id| !id.is_empty())
                    .map(|routing_id| json!({ "key": key, "routingId": routing_id })),
            };
            if let Some(target) = target {
                self.requested_file_size_keys.insert(item.key().to_string());
                targets.push(target);
            }
        }
        for batch in targets.chunks(SESSION_TRANSCRIPT_SIZE_BATCH_SIZE) {
            let request_id = self.next_request_id("session-transcript-sizes", context.clock);
            self.file_size_request_ids.insert(request_id.clone());
            effects.push(QuickAccessEffect::Post(json!({
                "requestId": request_id,
                "sessions": batch,
                "type": "requestSessionTranscriptSizes",
            })));
        }
    }

    fn request_prompt_list(
        &mut self,
        clock: &dyn QuickAccessClock,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        let request_id = self.next_request_id("stashed-prompts", clock);
        self.prompt_request_ids.list = Some(request_id.clone());
        self.prompts.resolved_default_scope = false;
        effects.push(QuickAccessEffect::Post(json!({
            "requestId": request_id,
            "includeRecovery": false,
            "includeDelivered": false,
            "type": "requestStashedPrompts",
        })));
    }

    pub(crate) fn request_prompt_history(
        &mut self,
        view: PromptsView,
        clock: &dyn QuickAccessClock,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        let slot = match view {
            PromptsView::Recovered => &self.prompt_request_ids.recovered,
            PromptsView::Sent => &self.prompt_request_ids.sent,
            PromptsView::Saved => return,
        };
        if slot.is_some() {
            return;
        }
        let request_id = self.next_request_id(&format!("stashed-prompts-{}", view.wire()), clock);
        match view {
            PromptsView::Recovered => self.prompt_request_ids.recovered = Some(request_id.clone()),
            _ => self.prompt_request_ids.sent = Some(request_id.clone()),
        }
        effects.push(QuickAccessEffect::Post(json!({
            "requestId": request_id,
            "includeRecovery": view == PromptsView::Recovered,
            "includeDelivered": view == PromptsView::Sent,
            "type": "requestStashedPrompts",
        })));
    }

    pub(crate) fn refresh_recovered(&mut self, context: &mut QuickAccessContext<'_>) {
        let names = prompts::prompt_names(&self.prompts, context.data);
        self.prompts.recovered = context
            .storage
            .recovered_drafts()
            .iter()
            .map(|draft| prompts::recovered_draft_as_prompt(draft, &names))
            .collect();
    }

    pub(crate) fn refresh_sent(&mut self, context: &mut QuickAccessContext<'_>) {
        let names = prompts::prompt_names(&self.prompts, context.data);
        self.prompts.sent = context
            .storage
            .sent_messages()
            .iter()
            .map(|message| prompts::sent_as_prompt(message, &names))
            .collect();
    }

    fn refresh_tab_data(
        &mut self,
        context: &QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        match self.tab {
            QuickAccessTab::RecentProjects => self.request_recent_projects(effects),
            QuickAccessTab::RecentSessions => self.schedule_sessions_request(true, effects),
            QuickAccessTab::SavedPrompts => {
                self.request_prompt_list(context.clock, effects);
                if self.prompts.view != PromptsView::Saved {
                    let view = self.prompts.view;
                    self.request_prompt_history(view, context.clock, effects);
                }
            }
            QuickAccessTab::Commands => {}
        }
    }

    fn cycle_sessions_scope(&mut self, effects: &mut Vec<QuickAccessEffect>) {
        self.sessions.scope = match self.sessions.scope {
            SessionScope::All => SessionScope::Closed,
            SessionScope::Closed => SessionScope::External,
            SessionScope::External => SessionScope::All,
        };
        if self.sessions.scope == SessionScope::External {
            self.external_refresh_pending = true;
        }
        self.schedule_sessions_request(true, effects);
    }

    fn new_composer(&self, anchor: String, prompt_id: Option<String>) -> Composer {
        Composer {
            name: String::new(),
            color: STASHED_PROMPT_TAG_COLORS
                [self.prompts.tags.len() % STASHED_PROMPT_TAG_COLORS.len()]
            .to_string(),
            anchor,
            prompt_id,
        }
    }

    // ------------------------------------------------------------------------------ commands

    /// `onNativeQuickAccessCommand`: one interaction from the window.
    pub fn command(
        &mut self,
        command: &Value,
        context: &mut QuickAccessContext<'_>,
    ) -> Vec<QuickAccessEffect> {
        let mut effects = Vec::new();
        let str_field = |key: &str| command[key].as_str().unwrap_or("").to_string();
        match command["type"].as_str().unwrap_or("") {
            "open" => {
                let Some(tab) = command["tab"].as_str().and_then(QuickAccessTab::parse) else {
                    return effects;
                };
                self.open = true;
                self.tab = tab;
                self.query = command["query"].as_str().unwrap_or("").to_string();
                self.query_revision += 1;
                self.selected_key.clear();
                match tab {
                    QuickAccessTab::RecentProjects => {
                        self.machine_id = command["machineId"].as_str().map(str::to_string);
                        self.projects = None;
                    }
                    QuickAccessTab::RecentSessions => {
                        self.sessions = SessionsTabState::new(context.clock.now_ms());
                        self.sessions.project_id =
                            command["projectId"].as_str().unwrap_or("").to_string();
                        self.sessions.scope = command["scope"]
                            .as_str()
                            .map(SessionScope::parse)
                            .unwrap_or_default();
                        self.external_refresh_pending =
                            self.sessions.scope == SessionScope::External;
                        self.requested_file_size_keys.clear();
                    }
                    QuickAccessTab::SavedPrompts => {
                        /*
                        CDXC:SavedPrompts 2026-09-11 (ported) WHY:
                        The saved library is retained across closes so reopening paints it before the
                        refresh lands; everything else about the tab resets.
                        */
                        let kept_prompts = self.prompts.prompts.take();
                        let kept_tags = std::mem::take(&mut self.prompts.tags);
                        self.prompts = PromptsTabState {
                            prompts: kept_prompts,
                            tags: kept_tags,
                            ..PromptsTabState::default()
                        };
                        self.prompt_request_ids.recovered = None;
                        self.prompt_request_ids.sent = None;
                        let scope = PromptsScope::parse(command["promptScope"].as_str());
                        self.opened_prompt_scope = scope;
                        apply_launcher_context(
                            &mut self.prompts,
                            command["promptProjectId"].as_str().map(str::to_string),
                            command["promptSessionId"].as_str().map(str::to_string),
                            scope,
                        );
                    }
                    QuickAccessTab::Commands => {}
                }
                self.refresh_tab_data(context, &mut effects);
            }
            "closed" => {
                self.open = false;
                self.menu_row_key = None;
                self.menu_prompt_key = None;
                return effects;
            }
            "close" => {
                self.close(&mut effects);
                return effects;
            }
            "tab" => {
                let Some(tab) = command["tab"].as_str().and_then(QuickAccessTab::parse) else {
                    return effects;
                };
                if self.tab != tab {
                    self.tab = tab;
                    self.query.clear();
                    self.query_revision += 1;
                    self.selected_key.clear();
                    self.refresh_tab_data(context, &mut effects);
                }
            }
            "query" => {
                self.query = str_field("query");
                self.selected_key.clear();
                if self.tab == QuickAccessTab::RecentSessions {
                    self.schedule_sessions_request(false, &mut effects);
                }
            }
            "select" => {
                self.selected_key = str_field("key");
                self.selection_seq = command["seq"].as_u64().unwrap_or(0);
            }
            "activate" => {
                let key = str_field("key");
                self.activate_row(&key, context, &mut effects);
            }
            "secondary" => {
                self.menu_prompt_key = None;
                let key = str_field("key");
                let items = self.row_action_items(&key, context);
                self.menu_row_key = Some(key);
                effects.push(QuickAccessEffect::Update(QuickAccessUpdate::Menu(items)));
                return effects;
            }
            "actionHotkey" => {
                let key = str_field("key");
                let items = self.row_action_items(&key, context);
                let id = super::row_actions::action_for_hotkey(
                    &items,
                    command["hotkey"].as_str().unwrap_or(""),
                )
                .map(|item| item.id.clone());
                if let Some(id) = id {
                    if self.run_row_action(&key, &id, context, &mut effects) {
                        return effects;
                    }
                }
            }
            "menuItem" => {
                let id = str_field("id");
                if let Some(key) = self.menu_prompt_key.take() {
                    if id == "tag:new" {
                        let prompt_id = prompts::find_prompt(&self.prompts, &key)
                            .and_then(|prompt| prompt["promptId"].as_str())
                            .map(str::to_string);
                        self.prompts.composer =
                            Some(self.new_composer(format!("row:{key}"), prompt_id));
                        self.prompts.tag_error = None;
                    } else if let Some(tag_id) = id.strip_prefix("tag:") {
                        if let Some(prompt) = prompts::find_prompt(&self.prompts, &key) {
                            let prompt_id = prompt["promptId"].as_str().unwrap_or("").to_string();
                            let next = prompts::next_tag_ids(prompt, tag_id);
                            self.set_prompt_tags(&prompt_id, next, context.clock, &mut effects);
                        }
                    }
                } else {
                    let key = self.menu_row_key.take().unwrap_or_default();
                    if self.run_row_action(&key, &id, context, &mut effects) {
                        return effects;
                    }
                }
            }
            "scope" => {
                let value = str_field("value");
                if value == "cycle" {
                    self.cycle_sessions_scope(&mut effects);
                } else {
                    self.sessions.scope = SessionScope::parse(&value);
                    if self.sessions.scope == SessionScope::External {
                        self.external_refresh_pending = true;
                    }
                    self.schedule_sessions_request(true, &mut effects);
                }
            }
            "view" => {
                self.prompts.view = PromptsView::parse(&str_field("value"));
                match self.prompts.view {
                    PromptsView::Recovered => {
                        self.refresh_recovered(context);
                        self.request_prompt_history(
                            PromptsView::Recovered,
                            context.clock,
                            &mut effects,
                        );
                    }
                    PromptsView::Sent => {
                        self.refresh_sent(context);
                        self.request_prompt_history(PromptsView::Sent, context.clock, &mut effects);
                    }
                    PromptsView::Saved => {}
                }
            }
            "project" => {
                let value = str_field("value");
                if self.tab == QuickAccessTab::RecentSessions {
                    self.sessions.project_id = value;
                    self.schedule_sessions_request(true, &mut effects);
                } else if self.tab == QuickAccessTab::SavedPrompts {
                    if value == ALL_PROJECTS_VALUE {
                        self.prompts.scope = PromptsScope::All;
                    } else if value == CURRENT_SESSION_VALUE {
                        self.prompts.scope = PromptsScope::Session;
                    } else {
                        self.prompts.scope = PromptsScope::Project;
                        self.prompts.scope_project_id =
                            Some(value.get("project:".len()..).unwrap_or("").to_string());
                    }
                }
            }
            "tagFilter" => {
                let value = str_field("value");
                if self.tab == QuickAccessTab::RecentSessions {
                    if let Some(index) = self
                        .sessions
                        .tag_filters
                        .iter()
                        .position(|tag| *tag == value)
                    {
                        self.sessions.tag_filters.remove(index);
                    } else {
                        self.sessions.tag_filters.push(value);
                    }
                    self.schedule_sessions_request(true, &mut effects);
                } else if self.tab == QuickAccessTab::SavedPrompts {
                    if value == "tag:new" {
                        self.prompts.composer =
                            Some(self.new_composer("toolbar".to_string(), None));
                        self.prompts.tag_error = None;
                    } else if value == ALL_TAGS_VALUE {
                        self.prompts.tag_filter = TagFilter::All;
                    } else if value == NO_TAG_VALUE {
                        self.prompts.tag_filter = TagFilter::Untagged;
                    } else {
                        self.prompts.tag_filter =
                            TagFilter::Tag(value.get("tag:".len()..).unwrap_or("").to_string());
                    }
                }
            }
            "loadMore" => {
                if self.tab == QuickAccessTab::RecentSessions {
                    if !self.sessions.has_filters_or_query(&self.query) {
                        self.sessions.history_window_count += 1;
                    } else if let Some(cursor) = self.sessions.cursor.clone() {
                        if !self.sessions_loading_more {
                            self.request_sessions_page(true, Some(cursor), context, &mut effects);
                        }
                    }
                }
            }
            "editorField" => {
                let value = str_field("value");
                if let Some(editing) = self.prompts.editing.as_mut() {
                    match command["field"].as_str() {
                        Some("content") => editing.content = value,
                        Some("project") => editing.project_value = value,
                        Some("tag") => editing.tag_value = value,
                        _ => {}
                    }
                }
            }
            "editorFavorite" => {
                if let Some(editing) = self.prompts.editing.as_mut() {
                    editing.is_favorite = !editing.is_favorite;
                }
            }
            "editorSubmit" => self.submit_editor(context.clock, &mut effects),
            "editorCancel" => {
                if !self.prompts.saving {
                    self.prompts.editing = None;
                    self.prompts.save_error = None;
                }
            }
            "tagComposerOpen" => {
                let anchor = str_field("anchor");
                let prompt_id = anchor
                    .strip_prefix("row:")
                    .and_then(|key| prompts::find_prompt(&self.prompts, key))
                    .and_then(|prompt| prompt["promptId"].as_str())
                    .map(str::to_string);
                self.prompts.composer = Some(self.new_composer(anchor, prompt_id));
                self.prompts.tag_error = None;
            }
            "tagComposerField" => {
                let value = str_field("value");
                if let Some(composer) = self.prompts.composer.as_mut() {
                    match command["field"].as_str() {
                        Some("name") => composer.name = value,
                        Some("color") => composer.color = value,
                        _ => {}
                    }
                }
            }
            "tagComposerSubmit" => {
                if let Some(composer) = self.prompts.composer.clone() {
                    let name = js_trim(&collapse_runs(js_trim(&composer.name))).to_string();
                    if !name.is_empty() {
                        self.pending_tag_application = Some((js_lower(&name), composer.prompt_id));
                        let request_id =
                            self.next_request_id("save-stashed-prompt-tag", context.clock);
                        effects.push(QuickAccessEffect::Post(json!({
                            "color": composer.color,
                            "name": name,
                            "requestId": request_id,
                            "type": "saveStashedPromptTag",
                        })));
                        self.prompts.composer = None;
                    }
                }
            }
            "tagComposerCancel" => self.prompts.composer = None,
            _ => {}
        }
        self.publish(&mut effects);
        effects
    }

    // ------------------------------------------------------------------------------ messages

    /// One answer the host produced for a request this controller posted.
    pub fn receive(
        &mut self,
        message: &Value,
        context: &mut QuickAccessContext<'_>,
    ) -> Vec<QuickAccessEffect> {
        let mut effects = Vec::new();
        let request_id = message["requestId"].as_str();
        match message["type"].as_str().unwrap_or("") {
            "recentProjectsResult" => {
                if message["machineId"].as_str() != self.machine_id.as_deref() {
                    return effects;
                }
                self.projects = Some(RecentProjectsData {
                    machine_id: self.machine_id.clone(),
                    projects: message["recentProjects"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default(),
                });
            }
            "previousSessionsResult" => {
                if request_id != self.sessions_request_id.as_deref() {
                    return effects;
                }
                let page = message["previousSessions"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if self.sessions_request_append {
                    let existing = self.sessions.remote_sessions.get_or_insert_with(Vec::new);
                    let seen: BTreeSet<String> = existing
                        .iter()
                        .map(|session| session["historyId"].as_str().unwrap_or("").to_string())
                        .collect();
                    existing.extend(page.into_iter().filter(|session| {
                        !seen.contains(session["historyId"].as_str().unwrap_or(""))
                    }));
                } else {
                    self.sessions.remote_sessions = Some(page);
                    self.sessions.resolved_query_key =
                        Some(self.sessions_request_query_key.clone());
                    let default_key = sessions_query_key(&SessionsTabState::new(0), "");
                    if self.sessions_request_query_key == default_key {
                        self.sessions.history_anchor_ms = context.clock.now_ms();
                        self.sessions.history_window_count = 1;
                    }
                }
                if let Some(projects) = message["projects"].as_array() {
                    self.sessions.project_options = projects
                        .iter()
                        .map(|project| ProjectOption {
                            project_id: project["projectId"].as_str().unwrap_or("").to_string(),
                            name: project["name"].as_str().unwrap_or("").to_string(),
                        })
                        .collect();
                }
                // An empty cursor is falsy everywhere the TypeScript read it.
                self.sessions.cursor = message["cursor"]
                    .as_str()
                    .filter(|cursor| !cursor.is_empty())
                    .map(str::to_string);
                self.sessions_loading_more = false;
                self.fill_visible_history_window(context, &mut effects);
            }
            "sessionTranscriptSizesResult" => {
                let Some(request_id) = request_id else {
                    return effects;
                };
                if !self.file_size_request_ids.remove(request_id) {
                    return effects;
                }
                for result in message["sizes"].as_array().into_iter().flatten() {
                    let key = result["key"].as_str().unwrap_or("").to_string();
                    let size = result["sizeBytes"]
                        .as_f64()
                        .filter(|size| size.is_finite() && *size >= 0.0)
                        .map(|size| size as i64);
                    self.sessions.file_sizes_by_key.insert(key, size);
                }
            }
            "stashedPromptsResult" => {
                if request_id.is_some()
                    && request_id == self.prompt_request_ids.recovered.as_deref()
                {
                    /*
                    The daemon answer seeds local recovery storage; the visible rows are always read
                    back from that storage so a local-only draft is listed too.
                    */
                    context
                        .storage
                        .import_draft_recovery(&default_array(&message["recoveryDrafts"]));
                    context
                        .storage
                        .reconcile_drafts_from_server(&default_array(&message["drafts"]));
                    self.refresh_recovered(context);
                } else if request_id.is_some()
                    && request_id == self.prompt_request_ids.sent.as_deref()
                {
                    context
                        .storage
                        .record_delivered_drafts(&default_array(&message["deliveredDrafts"]));
                    self.refresh_sent(context);
                } else {
                    if request_id.is_none() || request_id != self.prompt_request_ids.list.as_deref()
                    {
                        return effects;
                    }
                    self.prompts.prompts =
                        Some(message["prompts"].as_array().cloned().unwrap_or_default());
                    self.prompts.tags = message["tags"].as_array().cloned().unwrap_or_default();
                    /*
                    CDXC:SavedPrompts 2026-08-24 (ported):
                    Without a launcher-pinned scope the tab opens on this session when it has session
                    context and that scope is not empty; the decision runs once per open so a later
                    refresh cannot yank the scope back.
                    */
                    if !self.prompts.resolved_default_scope {
                        self.prompts.resolved_default_scope = true;
                        if self.opened_prompt_scope.is_none()
                            && has_session_scope(&self.prompts, context.data)
                        {
                            let belongs = self.prompts.prompts.iter().flatten().any(|prompt| {
                                belongs_to_session(prompt, &self.prompts, context.data)
                            });
                            if belongs {
                                self.prompts.scope = PromptsScope::Session;
                            }
                        }
                    }
                }
            }
            "saveStashedPromptResult" => {
                if request_id.is_none() || request_id != self.prompt_request_ids.save.as_deref() {
                    return effects;
                }
                self.prompts.saving = false;
                self.prompt_request_ids.save = None;
                let ok = message["ok"] == Value::Bool(true);
                if !ok || !message["prompt"].is_object() {
                    self.prompts.save_error = Some(
                        message["error"]
                            .as_str()
                            .unwrap_or("Could not save this prompt.")
                            .to_string(),
                    );
                } else {
                    let saved = message["prompt"].clone();
                    let saved_id = saved["promptId"].clone();
                    let mut next = vec![saved];
                    next.extend(
                        self.prompts
                            .prompts
                            .take()
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|prompt| prompt["promptId"] != saved_id),
                    );
                    self.prompts.prompts = Some(next);
                    self.prompts.editing = None;
                    self.prompts.save_error = None;
                }
            }
            "stashedPromptTagsResult" => {
                if message["ok"] != Value::Bool(true) {
                    self.prompts.tag_error = Some(
                        message["error"]
                            .as_str()
                            .unwrap_or("Could not update tags.")
                            .to_string(),
                    );
                } else {
                    self.prompts.tag_error = None;
                    self.prompts.tags = message["tags"].as_array().cloned().unwrap_or_default();
                    if let Some(deleted) =
                        message["deletedTagId"].as_str().filter(|id| !id.is_empty())
                    {
                        if let Some(prompts) = self.prompts.prompts.as_mut() {
                            for prompt in prompts.iter_mut() {
                                let ids = prompts::prompt_tag_ids(prompt);
                                if ids.iter().any(|id| id == deleted) {
                                    prompt["tagIds"] = json!(ids
                                        .into_iter()
                                        .filter(|id| id != deleted)
                                        .collect::<Vec<_>>());
                                }
                            }
                        }
                        if self.prompts.tag_filter == TagFilter::Tag(deleted.to_string()) {
                            self.prompts.tag_filter = TagFilter::All;
                        }
                    }
                    /*
                    The daemon owns tag ids, so a tag created from a row's menu can only be applied
                    once the refreshed catalogue comes back naming it.
                    */
                    if let Some((name, prompt_id)) = self.pending_tag_application.clone() {
                        let created = self
                            .prompts
                            .tags
                            .iter()
                            .find(|tag| js_lower(tag["name"].as_str().unwrap_or("")) == name)
                            .and_then(|tag| tag["tagId"].as_str())
                            .map(str::to_string);
                        if let Some(created) = created {
                            self.pending_tag_application = None;
                            match prompt_id {
                                None => self.prompts.tag_filter = TagFilter::Tag(created),
                                Some(prompt_id) => {
                                    let prompt = self
                                        .prompts
                                        .prompts
                                        .iter()
                                        .flatten()
                                        .find(|prompt| {
                                            prompt["promptId"].as_str() == Some(prompt_id.as_str())
                                        })
                                        .cloned();
                                    if let Some(prompt) = prompt {
                                        let ids = prompts::prompt_tag_ids(&prompt);
                                        if !ids.contains(&created) {
                                            let mut next = Vec::new();
                                            if ids.iter().any(|id| id == FAVORITE_TAG_ID) {
                                                next.push(FAVORITE_TAG_ID.to_string());
                                            }
                                            next.push(created);
                                            self.set_prompt_tags(
                                                &prompt_id,
                                                next,
                                                context.clock,
                                                &mut effects,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "setStashedPromptTagsResult" => {
                if message["ok"] != Value::Bool(true) || !message["prompt"].is_object() {
                    self.prompts.tag_error = Some(
                        message["error"]
                            .as_str()
                            .unwrap_or("Could not update this prompt's tags.")
                            .to_string(),
                    );
                } else {
                    let tagged = message["prompt"].clone();
                    self.prompts.tag_error = None;
                    if let Some(prompts) = self.prompts.prompts.as_mut() {
                        for prompt in prompts.iter_mut() {
                            if prompt["promptId"] == tagged["promptId"] {
                                *prompt = tagged.clone();
                            }
                        }
                    }
                }
            }
            _ => return effects,
        }
        self.publish(&mut effects);
        effects
    }

    pub(crate) fn set_prompt_tags(
        &mut self,
        prompt_id: &str,
        tag_ids: Vec<String>,
        clock: &dyn QuickAccessClock,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        if let Some(prompts) = self.prompts.prompts.as_mut() {
            for prompt in prompts.iter_mut() {
                if prompt["promptId"].as_str() == Some(prompt_id) {
                    prompt["tagIds"] = json!(tag_ids);
                }
            }
        }
        let request_id = self.next_request_id("set-stashed-prompt-tags", clock);
        effects.push(QuickAccessEffect::Post(json!({
            "promptId": prompt_id,
            "requestId": request_id,
            "tagIds": tag_ids,
            "type": "setStashedPromptTags",
        })));
    }

    pub(crate) fn set_save_request(&mut self, request_id: String) {
        self.prompt_request_ids.save = Some(request_id);
    }

    pub(crate) fn set_menu_prompt_key(&mut self, key: String) {
        self.menu_prompt_key = Some(key);
    }

    pub(crate) fn sessions_tab_empty_copy(&self) -> String {
        sessions::empty_copy(&self.sessions, &self.query)
    }
}

/// `message.x ?? []`.
fn default_array(value: &Value) -> Value {
    if value.is_null() {
        json!([])
    } else {
        value.clone()
    }
}
