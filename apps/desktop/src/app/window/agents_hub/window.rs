//! The Agents Hub window entity: its state, the commands it hands the app, the answers the app
//! delivers back, and the keyboard contract.
use super::super::native_modal_kit::{ModalPalette, ModalRailItem, hsla, modal_raised_tab_rail};
use super::editor::new_editor_state;
use super::model::*;
use super::palette::HubPalette;
use super::sync_model::*;
use gpui::{
    App, AppContext as _, ClipboardItem, Context, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, Keystroke, ParentElement as _, Render, ScrollHandle, SharedString,
    Styled as _, Subscription, Window, WindowId, div, px,
};
use gpui_component::input::{EditorState, InputEvent, InputState};
use gpui_component::v_flex;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;
use web_time::Instant;

/// `APP_MODAL_HOST_WINDOW_WIDTH` x `APP_MODAL_HOST_WINDOW_HEIGHT`: the React Hub filled the
/// shared Settings-sized child window.
pub(crate) const AGENTS_HUB_MODAL_WIDTH: f32 = 1080.0;
pub(crate) const AGENTS_HUB_MODAL_HEIGHT: f32 = 760.0;

/// How long the copy button shows its check.
const COPIED_FEEDBACK: Duration = Duration::from_millis(1600);
/// How long "Copy as shell script" reads "Copied".
const SCRIPT_COPIED_FEEDBACK: Duration = Duration::from_millis(1500);

/// What the Hub asks the app to do. Every file command names a path from the catalog; the app
/// validates it against the catalog again before touching disk.
#[derive(Clone, Debug)]
pub(crate) enum AgentsHubModalCommand {
    RequestCatalog,
    RequestFileContent {
        file_path: String,
        request_id: String,
    },
    SaveFile {
        file_path: String,
        content: String,
    },
    /// Reveal a file or open a folder in the OS file manager.
    OpenPath {
        path: String,
    },
    OpenInBuiltInEditor {
        file_path: String,
    },
    /// A path went to the clipboard; the app plays the copy sound.
    Copied,
    /// The wrap toggle changed: the app remembers it for the Files view's editors too.
    SetWordWrap {
        wrap: bool,
    },
    RequestSyncReport,
    RequestSyncPlan {
        scope: String,
    },
    ApplySyncPlan {
        scope: String,
        groups: Vec<String>,
    },
    Close,
}

pub(crate) type AgentsHubModalHost = Rc<dyn Fn(AgentsHubModalCommand, &mut App)>;

pub(crate) struct AgentsHubModalConfig {
    pub(crate) palette: ModalPalette,
    pub(crate) initial_tab: AgentsHubTab,
    /// A skill to select once the catalog arrives (the Session details panel's skill rows).
    pub(crate) initial_skill: Option<String>,
    /// That skill's SKILL.md, which finds its row even when the row's name is the folder's and
    /// the skill calls itself something else.
    pub(crate) initial_skill_path: Option<String>,
    /// The remembered wrap choice the Files view shares (`ghostex.manage.wordWrap`).
    pub(crate) word_wrap: bool,
}

/// The Agent Sync filter over the agent list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncFilter {
    Fix,
    Synced,
    All,
}

/// The open plan sheet: its scope, the groups a problem row chose before the plan arrived, and
/// the groups the user switched since.
#[derive(Clone, Debug)]
pub(crate) struct SyncSheet {
    pub(crate) scope: String,
    pub(crate) preset: Option<Vec<String>>,
    pub(crate) enabled: Option<Vec<String>>,
}

/// The Agent Sync tab's own state. The React tab unmounted when another tab was chosen, so
/// everything but the answers from the app resets when the tab is left.
pub(crate) struct SyncTabState {
    pub(crate) report: Option<SyncReport>,
    pub(crate) plan: Option<SyncPlan>,
    pub(crate) apply_result: Option<SyncApplyResult>,
    pub(crate) requested: bool,
    pub(crate) selected_id: String,
    pub(crate) query: String,
    pub(crate) show_hidden: bool,
    pub(crate) filter: Option<SyncFilter>,
    pub(crate) open_profiles: HashSet<String>,
    pub(crate) part: Option<SyncPart>,
    pub(crate) tidy_lock: bool,
    pub(crate) open_fix: Option<String>,
    pub(crate) fix_all_agents: HashSet<String>,
    pub(crate) fix_all_paths: HashSet<String>,
    pub(crate) show_skills: bool,
    pub(crate) expanded_buckets: HashSet<&'static str>,
    pub(crate) sheet: Option<SyncSheet>,
    pub(crate) applying: bool,
    pub(crate) script_copied_at: Option<Instant>,
    pub(crate) expanded_plan_groups: HashSet<String>,
    pub(crate) list_scroll: ScrollHandle,
    pub(crate) detail_scroll: ScrollHandle,
    pub(crate) sheet_scroll: ScrollHandle,
}

impl SyncTabState {
    fn new() -> Self {
        Self {
            report: None,
            plan: None,
            apply_result: None,
            requested: false,
            selected_id: "all".to_string(),
            query: String::new(),
            show_hidden: false,
            filter: None,
            open_profiles: HashSet::new(),
            part: None,
            tidy_lock: false,
            open_fix: None,
            fix_all_agents: HashSet::new(),
            fix_all_paths: HashSet::new(),
            show_skills: false,
            expanded_buckets: HashSet::new(),
            sheet: None,
            applying: false,
            script_copied_at: None,
            expanded_plan_groups: HashSet::new(),
            list_scroll: ScrollHandle::new(),
            detail_scroll: ScrollHandle::new(),
            sheet_scroll: ScrollHandle::new(),
        }
    }

    /// What the tab forgets when it is left: everything but the reports.
    fn reset_view(&mut self) {
        let report = self.report.take();
        let plan = self.plan.take();
        let apply_result = self.apply_result.take();
        *self = Self::new();
        self.report = report;
        self.plan = plan;
        self.apply_result = apply_result;
    }

    /// The groups the sheet applies: the user's switches, else the row's preset, else the plan's defaults.
    pub(crate) fn enabled_groups(&self) -> Vec<String> {
        let Some(sheet) = self.sheet.as_ref() else {
            return Vec::new();
        };
        if let Some(enabled) = sheet.enabled.as_ref() {
            return enabled.clone();
        }
        if let Some(preset) = sheet.preset.as_ref() {
            return preset.clone();
        }
        default_plan_groups(self.sheet_plan())
    }

    /// The plan, when it answers the open sheet's scope.
    pub(crate) fn sheet_plan(&self) -> Option<&SyncPlan> {
        let sheet = self.sheet.as_ref()?;
        self.plan.as_ref().filter(|plan| plan.scope == sheet.scope)
    }

    pub(crate) fn sheet_result(&self) -> Option<&SyncApplyResult> {
        let sheet = self.sheet.as_ref()?;
        self.apply_result
            .as_ref()
            .filter(|result| result.scope == sheet.scope)
    }
}

/// The editor showing the active file, and the text it was last loaded with or saved as.
pub(crate) struct HubEditor {
    pub(crate) state: gpui::Entity<EditorState>,
    pub(crate) path: String,
    pub(crate) loaded: String,
    pub(crate) saved: String,
    pub(crate) dirty: bool,
    _change: Subscription,
}

pub(crate) struct GpuiAgentsHubModalWindow {
    pub(crate) host: AgentsHubModalHost,
    pub(crate) hp: HubPalette,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) active_tab: AgentsHubTab,
    pub(crate) catalog: Option<AgentsHubCatalog>,
    /// Saved editor texts, valid for the catalog generation they were saved against.
    pub(crate) saved_overlay: Option<(String, HashMap<String, String>)>,
    pub(crate) contents: HashMap<String, String>,
    pub(crate) content_errors: HashMap<String, String>,
    pub(crate) pending_requests: HashMap<String, String>,
    pub(crate) selected_file_ids: [String; 4],
    /// The skill to select when the next catalog arrives (`AgentsHubModalConfig::initial_skill`)
    /// and its SKILL.md (`initial_skill_path`).
    pending_skill: Option<(String, Option<String>)>,
    pub(crate) expanded_ids: HashSet<String>,
    pub(crate) search: gpui::Entity<InputState>,
    pub(crate) query: String,
    pub(crate) list_scroll: ScrollHandle,
    pub(crate) editor: Option<HubEditor>,
    pub(crate) word_wrap: bool,
    pub(crate) copied_at: Option<Instant>,
    request_counter: u64,
    pub(crate) sync: SyncTabState,
    pub(crate) sync_search: gpui::Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl GpuiAgentsHubModalWindow {
    pub(crate) fn new(
        config: AgentsHubModalConfig,
        host: AgentsHubModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hp = HubPalette::resolve(config.palette);
        let muted = hsla(hp.muted);
        let search = cx.new(|cx| {
            let mut input = InputState::new(window, cx)
                .placeholder(format!("Search {}", AgentsHubTab::Mds.label()));
            input.set_placeholder_color(Some(muted));
            input
        });
        let sync_search = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Search agents");
            input.set_placeholder_color(Some(muted));
            input
        });
        let search_change = cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.query = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        );
        let sync_search_change = cx.subscribe_in(
            &sync_search,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.sync.query = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        );
        /*
        CDXC:AgentLauncher 2026-08-24 (round 2):
        Cmd+1..Cmd+5 select a tab from anywhere in the Hub window, matching Quick Access, and
        Cmd+S saves the open file. They are taken before the focused field or the editor sees
        them, the way the React listener ran in the capture phase so Monaco could not swallow them.
        */
        let window_id: WindowId = window.window_handle().window_id();
        let view = cx.weak_entity();
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle().window_id() != window_id {
                return;
            }
            let handled = view
                .update(cx, |this, cx| {
                    this.handle_shortcut(&event.keystroke, window, cx)
                })
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        });
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let mut this = Self {
            host,
            hp,
            focus_handle,
            active_tab: config.initial_tab,
            catalog: None,
            saved_overlay: None,
            contents: HashMap::new(),
            content_errors: HashMap::new(),
            pending_requests: HashMap::new(),
            selected_file_ids: Default::default(),
            pending_skill: config
                .initial_skill
                .map(|skill| (skill, config.initial_skill_path)),
            expanded_ids: HashSet::new(),
            search,
            query: String::new(),
            list_scroll: ScrollHandle::new(),
            editor: None,
            word_wrap: config.word_wrap,
            copied_at: None,
            request_counter: 0,
            sync: SyncTabState::new(),
            sync_search,
            _subscriptions: vec![search_change, sync_search_change, keys],
        };
        this._subscriptions
            .extend(super::super::popup_dismissal::close_app_modal_on_click_away(window, cx));
        this.update_search_placeholder(window, cx);
        /*
        CDXC:AgentLauncher 2026-05-14-08:29:
        The Hub catalog is filesystem-owned data. Request it on each open so profile-specific files, installed skills, and config files reflect the current machine without baking private file contents into the modal.
        */
        this.send(AgentsHubModalCommand::RequestCatalog, cx);
        this.request_sync_report_if_needed(cx);
        this
    }

    pub(crate) fn send(&self, command: AgentsHubModalCommand, cx: &mut App) {
        (self.host)(command, cx);
    }

    // ---- answers from the app -------------------------------------------------------------

    /// A new catalog generation. File bodies are fetched per file, so the per-file caches
    /// belong to the generation they were read in and are dropped with it.
    pub(crate) fn receive_catalog(
        &mut self,
        catalog: AgentsHubCatalog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        /*
        CDXC:AgentLauncher 2026-06-12-02:53:
        The Hub catalog is metadata-only so opening the modal does not bridge every local agent file buffer. Clear the per-file caches whenever a new catalog generation arrives, then fetch only the selected file's content through a separate small request.
        */
        let generation_changed = self
            .catalog
            .as_ref()
            .is_none_or(|current| current.generated_at != catalog.generated_at);
        self.catalog = Some(catalog);
        if generation_changed {
            self.contents.clear();
            self.content_errors.clear();
            self.pending_requests.clear();
        }
        self.after_catalog_change(window, cx);
    }

    pub(crate) fn receive_file_content(
        &mut self,
        answer: AgentsHubFileContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_requests.get(&answer.file_path) == Some(&answer.request_id) {
            self.pending_requests.remove(&answer.file_path);
        }
        match answer.content {
            Some(content) => {
                self.content_errors.remove(&answer.file_path);
                self.contents.insert(answer.file_path, content);
            }
            None => {
                self.content_errors.insert(
                    answer.file_path,
                    answer
                        .error_message
                        .unwrap_or_else(|| "Unable to load file contents.".to_string()),
                );
            }
        }
        self.sync_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn receive_sync_report(&mut self, report: SyncReport, cx: &mut Context<Self>) {
        self.sync.report = Some(report);
        self.sync.plan = None;
        self.ensure_sync_filter();
        cx.notify();
    }

    /// The agent list starts on "To fix" when anything needs fixing and on "All" otherwise, decided
    /// once when the list first shows a report, like the React list's initial state.
    fn ensure_sync_filter(&mut self) {
        if self.sync.filter.is_some() {
            return;
        }
        let Some(report) = self.sync.report.as_ref() else {
            return;
        };
        let needs_fixing = report
            .agents
            .iter()
            .any(|agent| agent.status == "attention");
        self.sync.filter = Some(if needs_fixing {
            SyncFilter::Fix
        } else {
            SyncFilter::All
        });
    }

    pub(crate) fn receive_sync_plan(&mut self, plan: SyncPlan, cx: &mut Context<Self>) {
        self.sync.plan = Some(plan);
        self.sync.apply_result = None;
        cx.notify();
    }

    pub(crate) fn receive_sync_apply_result(
        &mut self,
        result: SyncApplyResult,
        cx: &mut Context<Self>,
    ) {
        self.sync.apply_result = Some(result);
        self.sync.applying = false;
        cx.notify();
    }

    // ---- files -------------------------------------------------------------------------------

    /// One file tab's groups in the current catalog.
    pub(crate) fn groups(&self, slot: usize) -> &[AgentsHubGroup] {
        match self.catalog.as_ref() {
            Some(catalog) => catalog.groups_by_tab.slot(slot),
            None => &[],
        }
    }

    /// The active file, carrying the text it was saved with in this catalog generation
    /// (`applySavedAgentsHubContents`).
    pub(crate) fn active_file(&self) -> Option<AgentsHubFile> {
        let slot = self.active_tab.file_slot()?;
        let mut file = find_file(self.groups(slot), &self.selected_file_ids[slot])?.clone();
        let catalog = self.catalog.as_ref()?;
        if let Some((generation, saved)) = self.saved_overlay.as_ref()
            && *generation == catalog.generated_at
            && let Some(content) = saved.get(&file.path)
        {
            file.content = Some(content.clone());
        }
        Some(file)
    }

    /// The active file's text: the saved or inlined body, else what was read for it.
    pub(crate) fn active_file_content(&self, file: &AgentsHubFile) -> Option<String> {
        file.content
            .clone()
            .or_else(|| self.contents.get(&file.path).cloned())
    }

    fn after_catalog_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A Hub opened on one skill lands on that skill: its first file selected, its row open
        // and scrolled to the top of the list, so a skill far down a long list is not left
        // folded out of sight.
        if let Some((skill, path)) = self.pending_skill.take()
            && let Some(slot) = AgentsHubTab::Skills.file_slot()
            && let Some((index, group_id, file_id)) = skill_row(self.groups(slot), &skill, path)
        {
            self.selected_file_ids[slot] = file_id;
            self.expanded_ids.insert(group_id);
            self.active_tab = AgentsHubTab::Skills;
            self.list_scroll.scroll_to_top_of_item(index);
        }
        for slot in 0..4 {
            let groups = self.groups(slot);
            if find_file(groups, &self.selected_file_ids[slot]).is_none() {
                self.selected_file_ids[slot] = first_file_id(groups);
            }
        }
        self.request_active_file(cx);
        self.sync_editor(window, cx);
        cx.notify();
    }

    /// Asks for the active file's body when nothing holds it, nothing is on the way, and the
    /// last read did not fail.
    fn request_active_file(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.active_file() else {
            return;
        };
        if self.active_file_content(&file).is_some()
            || self.pending_requests.contains_key(&file.path)
            || self.content_errors.contains_key(&file.path)
        {
            return;
        }
        self.request_counter += 1;
        let request_id = format!("agents-hub-file-{}", self.request_counter);
        self.pending_requests
            .insert(file.path.clone(), request_id.clone());
        self.send(
            AgentsHubModalCommand::RequestFileContent {
                file_path: file.path,
                request_id,
            },
            cx,
        );
    }

    /// Points the editor at the active file once its text is known. A different file, or new
    /// text for the same file, replaces the editor's text and makes it the saved text.
    pub(crate) fn sync_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self.active_file() else {
            return;
        };
        let Some(content) = self.active_file_content(&file) else {
            return;
        };
        let language = editor_language(&file.language);
        if let Some(editor) = self.editor.as_mut() {
            if editor.path == file.path && editor.loaded == content {
                return;
            }
            let same_text = editor.state.read(cx).value().as_ref() == content;
            if editor.path != file.path {
                editor.state.update(cx, |state, cx| {
                    state.set_highlighter(language, cx);
                });
            }
            if !same_text {
                let text = content.clone();
                editor
                    .state
                    .update(cx, |state, cx| state.set_value(text, window, cx));
            }
            editor.path = file.path;
            editor.loaded = content.clone();
            editor.saved = content;
            editor.dirty = false;
            return;
        }
        let state = new_editor_state(language, &content, self.word_wrap, window, cx);
        let change = cx.subscribe_in(
            &state,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(editor) = this.editor.as_mut()
                {
                    let dirty = input.read(cx).value().as_ref() != editor.saved;
                    if dirty != editor.dirty {
                        editor.dirty = dirty;
                        cx.notify();
                    }
                }
            },
        );
        self.editor = Some(HubEditor {
            state,
            path: file.path,
            loaded: content.clone(),
            saved: content,
            dirty: false,
            _change: change,
        });
    }

    pub(crate) fn select_tab(
        &mut self,
        tab: AgentsHubTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if tab == self.active_tab {
            return;
        }
        if self.active_tab == AgentsHubTab::Sync {
            self.sync.reset_view();
            self.sync_search
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.active_tab = tab;
        // Each React tab mounted its own editor, so a tab switch starts from the file's text.
        if let Some(editor) = self.editor.as_mut() {
            editor.loaded.clear();
            editor.path.clear();
        }
        self.copied_at = None;
        self.update_search_placeholder(window, cx);
        self.request_active_file(cx);
        self.sync_editor(window, cx);
        self.request_sync_report_if_needed(cx);
        self.ensure_sync_filter();
        cx.notify();
    }

    fn update_search_placeholder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_tab == AgentsHubTab::Sync {
            return;
        }
        let placeholder = format!("Search {}", self.active_tab.label());
        self.search.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
        });
    }

    pub(crate) fn select_file(
        &mut self,
        file_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self.active_tab.file_slot() else {
            return;
        };
        if self.selected_file_ids[slot] == file_id {
            return;
        }
        self.selected_file_ids[slot] = file_id;
        self.copied_at = None;
        self.request_active_file(cx);
        self.sync_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_group(
        &mut self,
        group_id: String,
        primary_file_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.expanded_ids.remove(&group_id) {
            self.expanded_ids.insert(group_id);
        }
        self.select_file(primary_file_id, window, cx);
        cx.notify();
    }

    /// CDXC:AgentLauncher 2026-06-04-20:08:
    /// External file edits need a user-triggered refresh inside the open editor because the Hub catalogs file contents on demand instead of watching every local profile folder. The saved-content overlay is cleared before the new scan so stale in-modal buffers cannot mask the latest disk contents.
    pub(crate) fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        self.saved_overlay = None;
        self.send(AgentsHubModalCommand::RequestCatalog, cx);
        cx.notify();
    }

    /// CDXC:AgentLauncher 2026-05-14-08:27:
    /// Users edit agent instruction and config files directly in the Hub, so Save stays disabled until the editor text differs from the last saved contents. Saving hands the path and text to the app, which validates the path against the catalog before it writes.
    ///
    /// CDXC:AgentLauncher 2026-05-16-07:19:
    /// A saved file shows its saved text right away, for the catalog generation it was saved from, so selecting another file and coming back cannot bring the pre-save buffer back, and the next scan stays authoritative.
    pub(crate) fn save_active_file(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if self.active_tab.file_slot().is_none() || editor.path.is_empty() {
            return;
        }
        let value = editor.state.read(cx).value().to_string();
        if value == editor.saved {
            return;
        }
        let file_path = editor.path.clone();
        editor.saved = value.clone();
        editor.loaded = value.clone();
        editor.dirty = false;
        let generation = self
            .catalog
            .as_ref()
            .map(|catalog| catalog.generated_at.clone())
            .unwrap_or_default();
        let overlay = match self.saved_overlay.take() {
            Some((current, saved)) if current == generation => saved,
            _ => HashMap::new(),
        };
        let mut overlay = overlay;
        overlay.insert(file_path.clone(), value.clone());
        self.saved_overlay = Some((generation, overlay));
        self.send(
            AgentsHubModalCommand::SaveFile {
                file_path,
                content: value,
            },
            cx,
        );
        cx.notify();
    }

    /// CDXC:AgentLauncher 2026-08-24 (round 3):
    /// The open file's full path is what users most often want out of the Hub (to paste into a prompt or a terminal), so the toolbar copies it and confirms with a check for a moment.
    pub(crate) fn copy_active_path(&mut self, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path));
        self.send(AgentsHubModalCommand::Copied, cx);
        self.copied_at = Some(Instant::now());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FEEDBACK).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
        cx.notify();
    }

    /// The toolbar's wrap toggle: the open editor follows it and the app remembers it.
    pub(crate) fn toggle_word_wrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wrap = !self.word_wrap;
        self.word_wrap = wrap;
        if let Some(editor) = self.editor.as_ref() {
            editor
                .state
                .update(cx, |state, cx| state.set_soft_wrap(wrap, window, cx));
        }
        self.send(AgentsHubModalCommand::SetWordWrap { wrap }, cx);
        cx.notify();
    }

    pub(crate) fn path_copied(&self) -> bool {
        self.copied_at
            .is_some_and(|at| at.elapsed() < COPIED_FEEDBACK)
    }

    // ---- Agent Sync ---------------------------------------------------------------------

    fn request_sync_report_if_needed(&mut self, cx: &mut Context<Self>) {
        if self.active_tab != AgentsHubTab::Sync
            || self.sync.report.is_some()
            || self.sync.requested
        {
            return;
        }
        self.sync.requested = true;
        self.send(AgentsHubModalCommand::RequestSyncReport, cx);
    }

    pub(crate) fn refresh_sync_report(&mut self, cx: &mut Context<Self>) {
        self.send(AgentsHubModalCommand::RequestSyncReport, cx);
    }

    pub(crate) fn select_sync_agent(&mut self, id: String, cx: &mut Context<Self>) {
        if id == "all" {
            // The per-agent pane unmounts on the overview, which forgets its disclosures.
            self.sync.show_skills = false;
            self.sync.expanded_buckets.clear();
        }
        self.sync.selected_id = id;
        cx.notify();
    }

    pub(crate) fn open_sync_plan(
        &mut self,
        scope: String,
        preset: Option<Vec<String>>,
        cx: &mut Context<Self>,
    ) {
        self.sync.sheet = Some(SyncSheet {
            scope: scope.clone(),
            preset,
            enabled: None,
        });
        self.sync.expanded_plan_groups.clear();
        self.sync.script_copied_at = None;
        // A sheet opened again must not show the last run's result while its plan loads.
        self.sync.apply_result = None;
        self.send(AgentsHubModalCommand::RequestSyncPlan { scope }, cx);
        cx.notify();
    }

    pub(crate) fn close_sync_sheet(&mut self, cx: &mut Context<Self>) {
        self.sync.sheet = None;
        self.sync.applying = false;
        cx.notify();
    }

    pub(crate) fn finish_sync_sheet(&mut self, cx: &mut Context<Self>) {
        self.close_sync_sheet(cx);
        self.refresh_sync_report(cx);
    }

    pub(crate) fn toggle_sync_plan_group(&mut self, kind: String, cx: &mut Context<Self>) {
        let mut enabled = self.sync.enabled_groups();
        if let Some(index) = enabled.iter().position(|group| *group == kind) {
            enabled.remove(index);
        } else {
            enabled.push(kind);
        }
        if let Some(sheet) = self.sync.sheet.as_mut() {
            sheet.enabled = Some(enabled);
        }
        cx.notify();
    }

    pub(crate) fn apply_sync_plan(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.sync.sheet.as_ref().map(|sheet| sheet.scope.clone()) else {
            return;
        };
        let groups = self.sync.enabled_groups();
        self.sync.applying = true;
        self.send(AgentsHubModalCommand::ApplySyncPlan { scope, groups }, cx);
        cx.notify();
    }

    pub(crate) fn copy_sync_script(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self.sync.sheet_plan() else {
            return;
        };
        let script = plan_to_shell_script(plan, &self.sync.enabled_groups());
        cx.write_to_clipboard(ClipboardItem::new_string(script));
        self.send(AgentsHubModalCommand::Copied, cx);
        self.sync.script_copied_at = Some(Instant::now());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SCRIPT_COPIED_FEEDBACK).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn sync_script_copied(&self) -> bool {
        self.sync
            .script_copied_at
            .is_some_and(|at| at.elapsed() < SCRIPT_COPIED_FEEDBACK)
    }

    // ---- keyboard -----------------------------------------------------------------------

    fn handle_shortcut(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = keystroke.modifiers;
        if !modifiers.secondary(cx) || modifiers.shift || modifiers.alt {
            return false;
        }
        let key = keystroke.key.as_str();
        if let Some(tab) = AgentsHubTab::ALL
            .iter()
            .find(|tab| tab.hotkey().ends_with(&format!("+{key}")))
        {
            self.select_tab(*tab, window, cx);
            return true;
        }
        if key == "s" && self.active_tab.file_slot().is_some() {
            self.save_active_file(cx);
            return true;
        }
        false
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            cx.stop_propagation();
            self.send(AgentsHubModalCommand::Close, cx);
        }
    }

    // ---- preview hooks ------------------------------------------------------------------
    // Used by src/bin/native_modal_demo/agents_hub.rs only.

    /// Preview-only: pretend the user chose a sync agent, a filter, a fix row, a group, a search
    /// or an edit.
    #[allow(dead_code)]
    pub(crate) fn preview_select_sync_agent(&mut self, id: &str, cx: &mut Context<Self>) {
        self.select_sync_agent(id.to_string(), cx);
    }

    #[allow(dead_code)]
    pub(crate) fn preview_sync_filter_all_with_profiles(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        self.sync.filter = Some(SyncFilter::All);
        self.sync.open_profiles.insert(id.to_string());
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn preview_open_fix(&mut self, kind: &str, cx: &mut Context<Self>) {
        self.sync.open_fix = Some(kind.to_string());
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn preview_expand_group(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self.active_tab.file_slot() else {
            return;
        };
        let Some(group) = self.groups(slot).get(index) else {
            return;
        };
        let group_id = group.id.clone();
        let primary = group
            .files
            .first()
            .map(|file| file.id.clone())
            .unwrap_or_default();
        self.toggle_group(group_id, primary, window, cx);
    }

    #[allow(dead_code)]
    pub(crate) fn preview_type_in_editor(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.editor.as_ref() {
            let value = format!("{}{text}", editor.state.read(cx).value());
            editor
                .state
                .update(cx, |state, cx| state.set_value(value, window, cx));
            let dirty = editor.state.read(cx).value().as_ref() != editor.saved;
            if let Some(editor) = self.editor.as_mut() {
                editor.dirty = dirty;
            }
            cx.notify();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn preview_search(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = text.to_string();
        if self.active_tab == AgentsHubTab::Sync {
            self.sync_search
                .update(cx, |input, cx| input.set_value(value, window, cx));
            self.sync.query = text.to_string();
        } else {
            self.search
                .update(cx, |input, cx| input.set_value(value, window, cx));
            self.query = text.to_string();
        }
        cx.notify();
    }
}

impl gpui::Focusable for GpuiAgentsHubModalWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GpuiAgentsHubModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hp = self.hp;
        let items: Vec<ModalRailItem> = AgentsHubTab::ALL
            .iter()
            .map(|tab| ModalRailItem {
                label: SharedString::from(tab.label()),
                trailing: Some(SharedString::from(
                    crate::hotkey_label::terminal_overlay_hotkey_chord_label(tab.hotkey()),
                )),
                dot: None,
            })
            .collect();
        let rail = modal_raised_tab_rail(
            &hp.modal,
            "agents-hub-tabs",
            &items,
            self.active_tab.index(),
            hp.tab_hotkey,
            hp.tab_hotkey_active,
            |this: &mut Self, index, window, cx| {
                this.select_tab(AgentsHubTab::ALL[index], window, cx);
            },
            cx,
        );
        let content = if self.active_tab == AgentsHubTab::Sync {
            self.render_sync_tab(window, cx)
        } else {
            self.render_files_tab(window, cx)
        };
        div()
            .id("agents-hub-root")
            .key_context("AgentsHub")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(hsla(hp.page))
            .font_family(super::super::native_modal_kit::MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(hp.foreground))
            .on_key_down(cx.listener(Self::on_key_down))
            .p(px(4.0))
            .child(
                // `.agents-hub-dialog`: the dialog 4px inside the window, a hairline edge,
                // 12px corners and 8px of padding around the tabs. Its fill is the window's own,
                // so it paints none (under glass a second coat of the frosted fill would darken it).
                v_flex()
                    .size_full()
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(hsla(hp.line))
                    .p(px(8.0))
                    .gap(px(8.0))
                    .overflow_hidden()
                    .child(div().w_full().px(px(2.0)).pb(px(6.0)).child(rail))
                    .child(div().flex_1().min_h_0().w_full().flex().child(content)),
            )
    }
}

impl super::super::native_modal_kit::ModalCornerClose for GpuiAgentsHubModalWindow {
    fn close_from_corner(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.send(AgentsHubModalCommand::Close, cx);
    }

    /// An edited file not saved yet, a sync plan waiting for its answer or a sync being applied
    /// keep the Hub open on a click away.
    fn keeps_open_on_click_away(&self, _window: &Window, _cx: &App) -> bool {
        self.editor.as_ref().is_some_and(|editor| editor.dirty)
            || self.sync.sheet.is_some()
            || self.sync.applying
    }
}

/// The Skills row holding `skill`: the one that lists its SKILL.md (catalog paths are real
/// paths, so a symlinked skill folder still matches), else the one with its name. Its index, id
/// and the file to select: that SKILL.md, else the row's first file.
fn skill_row(
    groups: &[AgentsHubGroup],
    skill: &str,
    path: Option<String>,
) -> Option<(usize, String, String)> {
    let path = path.map(|path| {
        std::fs::canonicalize(&path)
            .map(|real| real.to_string_lossy().into_owned())
            .unwrap_or(path)
    });
    let by_path = path.as_deref().and_then(|path| {
        groups.iter().enumerate().find_map(|(index, group)| {
            let file = group.files.iter().find(|file| file.path == path)?;
            Some((index, group.id.clone(), file.id.clone()))
        })
    });
    by_path.or_else(|| {
        groups.iter().enumerate().find_map(|(index, group)| {
            (group.name == skill)
                .then(|| Some((index, group.id.clone(), group.files.first()?.id.clone())))
                .flatten()
        })
    })
}
