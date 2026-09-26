//! What a Quick Access row does (activate, remove, the actions menu and its hotkeys, the prompt
//! editor) and the snapshot the window paints.
//!
//! Ported from the deleted `apps/desktop/sidebar/native-quick-access/controller.ts` (see git
//! history), these halves of it:
//! `activateRow`, `removeRow`, `runPromptAction`, `submitEditor`, `startAddPrompt`,
//! `rowActionItems`, `primaryActionLabel`, `runRowAction` and `snapshot`.

use serde_json::{json, Value};

use super::commands::{command_groups, run_command_row, CommandRun};
use super::controller::{
    QuickAccessContext, QuickAccessController, QuickAccessEffect, QuickAccessTab,
};
use super::hotkeys::normalize_hotkey_settings;
use super::projects::{self, activate_message, find_project, project_menu_items, remove_message};
use super::prompts::{
    self, editor_state, find_prompt, next_tag_ids, prompt_actions, Editing, PromptsScope,
    PromptsView, TagFilter, FAVORITE_TAG_ID, NO_PROJECT_VALUE, NO_TAG_VALUE,
    STASHED_PROMPT_TAG_COLORS,
};
use super::row_actions::{action_hotkeys, action_item, item, separator, tidy, ACTIVATE_ACTION_ID};
use super::sessions::{self, sessions_query_key, visible_items, SessionItem};
use super::text::{format_hotkey_label, QuickAccessClock};
use super::wire::{
    QuickAccessGroup, QuickAccessMenuItem, QuickAccessSegment, QuickAccessSnapshot,
    QuickAccessTab as WireTab, QuickAccessTagComposer, QuickAccessToolbar,
};
use crate::sidebar_view::text::js_trim;

const TABS: [(QuickAccessTab, &str, &str); 4] = [
    (QuickAccessTab::Commands, "Commands", "cmd+1"),
    (QuickAccessTab::RecentProjects, "Projects", "cmd+2"),
    (QuickAccessTab::RecentSessions, "Sessions", "cmd+3"),
    (QuickAccessTab::SavedPrompts, "Saved Prompts", "cmd+4"),
];

fn placeholder(tab: QuickAccessTab) -> &'static str {
    match tab {
        QuickAccessTab::Commands => "Search commands...",
        QuickAccessTab::RecentProjects => "Search projects...",
        QuickAccessTab::RecentSessions => "Search sessions...",
        QuickAccessTab::SavedPrompts => "Search saved prompts...",
    }
}

const SESSIONS_SCOPE_TOGGLE_HOTKEY: &str = "alt+c";

/// `trimPromptEditorTrailingSpaces`: spaces and tabs at line ends go, everything else stays.
fn trim_trailing_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending = String::new();
    for character in text.chars() {
        match character {
            ' ' | '\t' => pending.push(character),
            '\n' | '\r' => {
                pending.clear();
                out.push(character);
            }
            _ => {
                out.push_str(&pending);
                pending.clear();
                out.push(character);
            }
        }
    }
    out
}

impl QuickAccessController {
    pub(crate) fn activate_row(
        &mut self,
        key: &str,
        context: &mut QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        match self.tab {
            QuickAccessTab::Commands => {
                for step in run_command_row(key, context.data) {
                    match step {
                        CommandRun::Close => self.close(effects),
                        CommandRun::Post(message) => effects.push(QuickAccessEffect::Post(message)),
                        CommandRun::OpenModal(message) => {
                            effects.push(QuickAccessEffect::OpenModal(message))
                        }
                    }
                }
            }
            QuickAccessTab::RecentProjects => {
                let Some(project) = find_project(self.projects.as_ref(), key) else {
                    return;
                };
                effects.push(QuickAccessEffect::Post(activate_message(project)));
                self.close(effects);
            }
            QuickAccessTab::RecentSessions => {
                let message = {
                    let items = visible_items(&self.sessions, &self.query, context.data);
                    let Some(item) = items.iter().find(|item| item.key() == key) else {
                        return;
                    };
                    match item {
                        SessionItem::Open { session, .. } => {
                            json!({ "sessionId": session.session_id, "type": "focusSession" })
                        }
                        SessionItem::Closed { session, .. } => {
                            if session["isRestorable"] != Value::Bool(true) {
                                return;
                            }
                            json!({ "historyId": session["historyId"], "type": "restorePreviousSession" })
                        }
                    }
                };
                effects.push(QuickAccessEffect::Post(message));
                self.close(effects);
            }
            QuickAccessTab::SavedPrompts => {
                let Some(prompt) = find_prompt(&self.prompts, key) else {
                    return;
                };
                let mut message = json!({
                    "content": prompt["content"],
                    "promptId": prompt["promptId"],
                    "type": "insertStashedPrompt",
                });
                if let Some(session_id) = self
                    .prompts
                    .session_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                {
                    message["sessionId"] = json!(session_id);
                }
                effects.push(QuickAccessEffect::Post(message));
                self.close(effects);
            }
        }
    }

    fn remove_row(
        &mut self,
        key: &str,
        context: &QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        match self.tab {
            QuickAccessTab::RecentProjects => {
                if let Some(project) = find_project(self.projects.as_ref(), key) {
                    effects.push(QuickAccessEffect::Post(remove_message(project)));
                }
            }
            QuickAccessTab::RecentSessions => {
                let history_id = {
                    let items = visible_items(&self.sessions, &self.query, context.data);
                    match items.iter().find(|item| item.key() == key) {
                        Some(SessionItem::Closed { session, .. }) => session["historyId"].clone(),
                        _ => return,
                    }
                };
                // `removePreviousSessionByHistoryId(remoteSessions ?? store.previousSessions, id)`;
                // the store's list is always empty on this client.
                let remaining: Vec<Value> = self
                    .sessions
                    .remote_sessions
                    .take()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|session| session["historyId"] != history_id)
                    .collect();
                self.sessions.remote_sessions = Some(remaining);
                effects.push(QuickAccessEffect::Post(json!({
                    "historyId": history_id,
                    "type": "deletePreviousSession",
                })));
            }
            _ => {}
        }
    }

    fn run_prompt_action(
        &mut self,
        key: &str,
        action: &str,
        context: &mut QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        let Some(prompt) = find_prompt(&self.prompts, key).cloned() else {
            return;
        };
        let text = |field: &str| prompt[field].as_str().filter(|value| !value.is_empty());
        let prompt_id = prompt["promptId"].as_str().unwrap_or("").to_string();
        match action {
            "open" => {
                let mut message = json!({ "type": "jumpToStashedPromptSession" });
                for field in ["agentSessionId", "projectId", "sessionId"] {
                    if let Some(value) = text(field) {
                        message[field] = json!(value);
                    }
                }
                effects.push(QuickAccessEffect::Post(message));
                self.close(effects);
            }
            "favorite" => {
                let next = next_tag_ids(&prompt, FAVORITE_TAG_ID);
                self.set_prompt_tags(&prompt_id, next, context.clock, effects);
            }
            "copy" => {
                /*
                The app has no clipboard of its own here, so the copy goes through the host message
                the rest of the app already uses; it writes the clipboard and plays the copy sound.
                */
                effects.push(QuickAccessEffect::CopyText(
                    prompt["content"].as_str().unwrap_or("").to_string(),
                ));
            }
            "edit" => {
                let tag_ids = prompts::prompt_tag_ids(&prompt);
                self.prompts.editing = Some(Editing {
                    prompt_id: Some(prompt_id),
                    content: prompt["content"].as_str().unwrap_or("").to_string(),
                    project_value: match text("projectId") {
                        Some(project_id) => format!("project:{project_id}"),
                        None => NO_PROJECT_VALUE.to_string(),
                    },
                    tag_value: match tag_ids.iter().find(|id| *id != FAVORITE_TAG_ID) {
                        Some(tag_id) => format!("tag:{tag_id}"),
                        None => NO_TAG_VALUE.to_string(),
                    },
                    is_favorite: tag_ids.iter().any(|id| id == FAVORITE_TAG_ID),
                });
                self.prompts.save_error = None;
            }
            "save" => {
                if self.prompts.saving {
                    return;
                }
                let request_id = self.next_request_id("save-stashed-prompt", context.clock);
                self.set_save_request(request_id.clone());
                self.prompts.saving = true;
                self.prompts.tag_error = None;
                let mut message = json!({
                    "content": prompt["content"],
                    "requestId": request_id,
                    "tagIds": [],
                    "type": "saveStashedPrompt",
                });
                for field in ["projectId", "sessionId"] {
                    if let Some(value) = text(field) {
                        message[field] = json!(value);
                    }
                }
                effects.push(QuickAccessEffect::Post(message));
            }
            "delete" | "dismiss" => match self.prompts.view {
                PromptsView::Saved => {
                    effects.push(QuickAccessEffect::Post(json!({
                        "promptId": prompt_id,
                        "type": "deleteStashedPrompt",
                    })));
                    if let Some(prompts) = self.prompts.prompts.as_mut() {
                        prompts.retain(|candidate| {
                            candidate["promptId"].as_str() != Some(prompt_id.as_str())
                        });
                    }
                }
                PromptsView::Sent => {
                    context.storage.delete_sent_message(&prompt_id);
                    self.refresh_sent(context);
                }
                PromptsView::Recovered => {
                    let session_key = prompts::recovered_session_key(&prompt_id);
                    match session_key.strip_prefix("history:") {
                        Some(recovery_id) => context.storage.dismiss_draft_recovery(recovery_id),
                        None => context.storage.delete_stored_draft(session_key),
                    }
                    self.refresh_recovered(context);
                }
            },
            _ => {}
        }
    }

    pub(crate) fn submit_editor(
        &mut self,
        clock: &dyn QuickAccessClock,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        let Some(editing) = self.prompts.editing.clone() else {
            return;
        };
        if self.prompts.saving {
            return;
        }
        let content = trim_trailing_spaces(&editing.content);
        if js_trim(&content).is_empty() {
            return;
        }
        let request_id = self.next_request_id("save-stashed-prompt", clock);
        self.set_save_request(request_id.clone());
        self.prompts.saving = true;
        self.prompts.save_error = None;
        let selected_project_id = if editing.project_value == NO_PROJECT_VALUE {
            None
        } else {
            Some(
                editing
                    .project_value
                    .get("project:".len()..)
                    .unwrap_or("")
                    .to_string(),
            )
        };
        let mut tag_ids: Vec<String> = Vec::new();
        if editing.is_favorite {
            tag_ids.push(FAVORITE_TAG_ID.to_string());
        }
        if editing.tag_value != NO_TAG_VALUE {
            tag_ids.push(
                editing
                    .tag_value
                    .get("tag:".len()..)
                    .unwrap_or("")
                    .to_string(),
            );
        }
        let mut message = json!({
            "content": content,
            "requestId": request_id,
            "tagIds": tag_ids,
            "type": "saveStashedPrompt",
        });
        let editing_id = editing.prompt_id.as_deref().filter(|id| !id.is_empty());
        if let Some(prompt_id) = editing_id {
            message["promptId"] = json!(prompt_id);
        }
        if editing_id.is_none() {
            if let Some(project_id) = selected_project_id.as_deref().filter(|id| !id.is_empty()) {
                message["projectId"] = json!(project_id);
            }
            let raw_session = self
                .prompts
                .raw_session_id
                .as_deref()
                .filter(|id| !id.is_empty());
            if selected_project_id == self.prompts.raw_project_id {
                if let Some(session_id) = raw_session {
                    message["sessionId"] = json!(session_id);
                }
            }
        }
        effects.push(QuickAccessEffect::Post(message));
    }

    fn start_add_prompt(&mut self) {
        let default_project = if self.prompts.scope == PromptsScope::Project {
            self.prompts.scope_project_id.clone()
        } else {
            self.prompts.raw_project_id.clone()
        };
        let (tag_value, is_favorite) = match &self.prompts.tag_filter {
            TagFilter::Tag(tag_id) if tag_id != FAVORITE_TAG_ID => (format!("tag:{tag_id}"), false),
            TagFilter::Tag(_) => (NO_TAG_VALUE.to_string(), true),
            _ => (NO_TAG_VALUE.to_string(), false),
        };
        self.prompts.editing = Some(Editing {
            prompt_id: None,
            content: String::new(),
            project_value: match default_project.as_deref().filter(|id| !id.is_empty()) {
                Some(project_id) => format!("project:{project_id}"),
                None => NO_PROJECT_VALUE.to_string(),
            },
            tag_value,
            is_favorite,
        });
        self.prompts.save_error = None;
    }

    fn find_prompts_hotkey(&self, context: &QuickAccessContext<'_>) -> String {
        // `normalizeghostexHotkeySettings(settings.hotkeys)`.
        let hotkeys =
            normalize_hotkey_settings(&context.data.settings()["hotkeys"], context.data.platform());
        match hotkeys.get("openFindPrompts").filter(|key| !key.is_empty()) {
            Some(key) => format_hotkey_label(key, context.data.platform()),
            None => String::new(),
        }
    }

    /// Everything the row at `key` can do, Return's action first. An empty key lists what the tab
    /// offers without a row.
    pub(crate) fn row_action_items(
        &self,
        key: &str,
        context: &QuickAccessContext<'_>,
    ) -> Vec<QuickAccessMenuItem> {
        let platform = context.data.platform();
        match self.tab {
            QuickAccessTab::Commands => {
                if key.is_empty() {
                    Vec::new()
                } else {
                    vec![item(
                        ACTIVATE_ACTION_ID,
                        "Run Command",
                        "player-play",
                        platform,
                    )]
                }
            }
            QuickAccessTab::RecentProjects => match find_project(self.projects.as_ref(), key) {
                Some(project) => project_menu_items(project, self.machine_id.as_deref(), platform),
                None => Vec::new(),
            },
            QuickAccessTab::RecentSessions => {
                let items = visible_items(&self.sessions, &self.query, context.data);
                let found = items.iter().find(|item| item.key() == key);
                let mut menu: Vec<QuickAccessMenuItem> = Vec::new();
                match found {
                    Some(SessionItem::Open { .. }) => {
                        menu.push(item(
                            ACTIVATE_ACTION_ID,
                            "Focus Session",
                            "focus-2",
                            platform,
                        ));
                    }
                    Some(SessionItem::Closed { session, .. }) => menu.push(action_item(
                        ACTIVATE_ACTION_ID,
                        "Resume Session",
                        "player-play",
                        None,
                        false,
                        session["isRestorable"] != Value::Bool(true),
                        platform,
                    )),
                    None => {}
                }
                menu.push(separator());
                menu.push(action_item(
                    "findPrompts",
                    "Search by Prompt",
                    "file-search",
                    Some(self.find_prompts_hotkey(context)),
                    false,
                    false,
                    platform,
                ));
                menu.push(separator());
                if matches!(found, Some(SessionItem::Closed { .. })) {
                    menu.push(action_item(
                        "remove",
                        "Delete Session",
                        "trash",
                        None,
                        true,
                        false,
                        platform,
                    ));
                }
                tidy(menu)
            }
            QuickAccessTab::SavedPrompts => {
                let prompt = find_prompt(&self.prompts, key);
                let offered = prompt
                    .map(|prompt| prompt_actions(&self.prompts, prompt))
                    .unwrap_or_default();
                let is_favorite = prompt.is_some_and(|prompt| {
                    prompts::prompt_tag_ids(prompt)
                        .iter()
                        .any(|id| id == FAVORITE_TAG_ID)
                });
                let mut menu: Vec<QuickAccessMenuItem> = Vec::new();
                if prompt.is_some() {
                    menu.push(item(
                        ACTIVATE_ACTION_ID,
                        "Insert into Chat",
                        "arrow-back-up",
                        platform,
                    ));
                }
                if offered.contains(&"open") {
                    menu.push(item(
                        "prompt:open",
                        "Open Source Session",
                        "arrow-up-right",
                        platform,
                    ));
                }
                menu.push(separator());
                if offered.contains(&"favorite") {
                    menu.push(item(
                        "prompt:favorite",
                        if is_favorite { "Remove Star" } else { "Star" },
                        if is_favorite { "star-filled" } else { "star" },
                        platform,
                    ));
                }
                if offered.contains(&"save") {
                    menu.push(item(
                        "prompt:save",
                        "Save Prompt",
                        "device-floppy",
                        platform,
                    ));
                }
                if offered.contains(&"tag") {
                    menu.push(item("prompt:tag", "Tag…", "tag", platform));
                }
                if offered.contains(&"copy") {
                    menu.push(item("prompt:copy", "Copy Text", "copy", platform));
                }
                if offered.contains(&"edit") {
                    menu.push(item("prompt:edit", "Edit", "pencil", platform));
                }
                if self.prompts.view == PromptsView::Saved {
                    menu.push(item("addPrompt", "New Prompt", "plus", platform));
                }
                menu.push(separator());
                if offered.contains(&"delete") {
                    menu.push(action_item(
                        "prompt:delete",
                        "Delete",
                        "trash",
                        None,
                        true,
                        false,
                        platform,
                    ));
                }
                tidy(menu)
            }
        }
    }

    /// The footer's name for Return: one word, because the footer also carries the four tabs.
    fn primary_action_label(&self, context: &QuickAccessContext<'_>) -> String {
        let items = self.row_action_items(&self.selected_key, context);
        let Some(first) = items.first() else {
            return String::new();
        };
        if first.id != ACTIVATE_ACTION_ID || first.disabled {
            return String::new();
        }
        match self.tab {
            QuickAccessTab::Commands => "Run".to_string(),
            QuickAccessTab::SavedPrompts => "Insert".to_string(),
            QuickAccessTab::RecentSessions => if first.label == "Focus Session" {
                "Focus"
            } else {
                "Resume"
            }
            .to_string(),
            QuickAccessTab::RecentProjects => first.label.clone(),
        }
    }

    /// Runs one actions-menu item. Returns true when it already answered the window, so the
    /// caller skips its publish.
    pub(crate) fn run_row_action(
        &mut self,
        key: &str,
        id: &str,
        context: &mut QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) -> bool {
        if id == ACTIVATE_ACTION_ID {
            self.activate_row(key, context, effects);
            return false;
        }
        if id == "remove" {
            self.remove_row(key, context, effects);
            return false;
        }
        if id == "addPrompt" {
            self.start_add_prompt();
            return false;
        }
        if id == "findPrompts" {
            // Close first: the desktop close removes whichever app-modal window is open, so opening
            // Find before closing would take down its own window.
            self.close(effects);
            effects.push(QuickAccessEffect::Post(json!({
                "actionId": "openFindPrompts",
                "type": "runGhostexHotkeyAction",
            })));
            return true;
        }
        if id == "prompt:tag" {
            let Some(prompt) = find_prompt(&self.prompts, key) else {
                return false;
            };
            let items = prompts::tag_menu_items(&self.prompts, prompt);
            self.set_menu_prompt_key(key.to_string());
            effects.push(QuickAccessEffect::Update(
                super::wire::QuickAccessUpdate::Menu(items),
            ));
            return true;
        }
        if let Some(action) = id.strip_prefix("prompt:") {
            self.run_prompt_action(key, action, context, effects);
            return false;
        }
        let Some(project) = find_project(self.projects.as_ref(), key).cloned() else {
            return false;
        };
        if id == "copyPath" {
            effects.push(QuickAccessEffect::Post(json!({
                "projectId": project["projectId"],
                "type": "copyRecentProjectPath",
            })));
        } else if id == "openLocation" || id == "openTerminal" {
            let remote = self.machine_id.is_some();
            effects.push(QuickAccessEffect::Post(json!({
                "projectId": project["projectId"],
                "type": if remote { "openRecentProjectTerminal" } else { "openRecentProjectInFinder" },
            })));
            if remote {
                self.close(effects);
                return true;
            }
        }
        false
    }

    /// The snapshot the window paints now. Also moves the selection to the first row when the
    /// selected one is gone, which is what the TypeScript did while building it.
    pub(crate) fn snapshot(&mut self, context: &mut QuickAccessContext<'_>) -> QuickAccessSnapshot {
        let data = context.data;
        let clock = context.clock;
        let platform = data.platform();
        let groups: Vec<QuickAccessGroup> = match self.tab {
            QuickAccessTab::Commands => command_groups(&self.query, data),
            QuickAccessTab::RecentProjects => {
                let hidden = context.storage.hidden_items();
                let collections = context.storage.project_collections();
                projects::project_groups(
                    self.projects.as_ref(),
                    &self.query,
                    &hidden,
                    &collections,
                    clock,
                )
            }
            QuickAccessTab::RecentSessions => {
                sessions::session_groups(&self.sessions, &self.query, data, clock)
            }
            QuickAccessTab::SavedPrompts => {
                prompts::prompt_groups(&self.prompts, &self.query, data, clock)
            }
        };
        let has_selected = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .any(|row| row.key() == self.selected_key);
        if !has_selected {
            self.selected_key = groups
                .iter()
                .flat_map(|group| group.rows.iter())
                .next()
                .map(|row| row.key().to_string())
                .unwrap_or_default();
        }
        let loading = match self.tab {
            QuickAccessTab::RecentProjects => self.projects.is_none(),
            QuickAccessTab::RecentSessions => {
                self.sessions.resolved_query_key.as_deref()
                    != Some(sessions_query_key(&self.sessions, &self.query).as_str())
            }
            QuickAccessTab::SavedPrompts => {
                self.prompts.view == PromptsView::Saved && self.prompts.prompts.is_none()
            }
            QuickAccessTab::Commands => false,
        };
        let loading_label = match self.tab {
            QuickAccessTab::RecentProjects => "Loading projects...",
            QuickAccessTab::RecentSessions => sessions::loading_copy(&self.sessions),
            QuickAccessTab::SavedPrompts => "Loading saved prompts...",
            QuickAccessTab::Commands => "Loading commands...",
        };
        let empty = match self.tab {
            QuickAccessTab::Commands => "No commands found.".to_string(),
            QuickAccessTab::RecentProjects => {
                if js_trim(&self.query).is_empty() {
                    "No projects yet.".to_string()
                } else {
                    "No projects match that search.".to_string()
                }
            }
            QuickAccessTab::RecentSessions => self.sessions_tab_empty_copy(),
            QuickAccessTab::SavedPrompts => prompts::empty_copy(&self.prompts, data).to_string(),
        };
        let toolbar = match self.tab {
            QuickAccessTab::RecentSessions => QuickAccessToolbar::Sessions {
                scope: self.sessions.scope.wire().to_string(),
                scopes: vec![
                    QuickAccessSegment {
                        value: "all",
                        label: "All",
                    },
                    QuickAccessSegment {
                        value: "closed",
                        label: "Closed",
                    },
                    QuickAccessSegment {
                        value: "external",
                        label: "External",
                    },
                ],
                scope_hotkey: format_hotkey_label(SESSIONS_SCOPE_TOGGLE_HOTKEY, platform),
                tag_filter_active: !self.sessions.tag_filters.is_empty(),
                tags: sessions::tag_select(&self.sessions, data),
                projects: sessions::project_select(&self.sessions, data),
            },
            QuickAccessTab::SavedPrompts => QuickAccessToolbar::Prompts {
                view: self.prompts.view.wire().to_string(),
                views: vec![
                    QuickAccessSegment {
                        value: "saved",
                        label: "Saved",
                    },
                    QuickAccessSegment {
                        value: "recovered",
                        label: "Recovered",
                    },
                    QuickAccessSegment {
                        value: "sent",
                        label: "Sent",
                    },
                ],
                projects: prompts::project_select(&self.prompts, data),
                tags: prompts::tag_select(&self.prompts, &self.query, data),
            },
            _ => QuickAccessToolbar::None,
        };
        let primary_action = self.primary_action_label(context);
        let prompts_tab = self.tab == QuickAccessTab::SavedPrompts;
        QuickAccessSnapshot {
            kind: "snapshot",
            version: 1,
            revision: self.query_revision,
            tab: self.tab.wire(),
            tabs: TABS
                .iter()
                .map(|(tab, label, hotkey)| WireTab {
                    id: tab.wire(),
                    label,
                    hotkey: format_hotkey_label(hotkey, platform),
                })
                .collect(),
            placeholder: placeholder(self.tab),
            query: self.query.clone(),
            query_revision: self.query_revision,
            loading,
            loading_label: loading_label.to_string(),
            empty,
            groups,
            selected_key: self.selected_key.clone(),
            selection_seq: self.selection_seq,
            toolbar,
            primary_action,
            action_hotkeys: action_hotkeys(),
            hint: if prompts_tab && self.prompts.editing.is_none() {
                format!(
                    "Press {} while you're using an agent to stash your prompt (Local only for now)",
                    format_hotkey_label("alt+s", platform)
                )
            } else {
                String::new()
            },
            editor: if prompts_tab {
                editor_state(&self.prompts, data)
            } else {
                None
            },
            tag_composer: if prompts_tab {
                self.prompts
                    .composer
                    .as_ref()
                    .map(|composer| QuickAccessTagComposer {
                        name: composer.name.clone(),
                        color: composer.color.clone(),
                        colors: STASHED_PROMPT_TAG_COLORS
                            .iter()
                            .map(|color| (*color).to_string())
                            .collect(),
                        error: self.prompts.tag_error.clone().unwrap_or_default(),
                        anchor: composer.anchor.clone(),
                    })
            } else {
                None
            },
        }
    }
}
