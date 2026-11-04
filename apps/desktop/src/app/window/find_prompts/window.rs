//! The Search by Prompt window's state and behaviour: the query line, the paged result window,
//! the filters, the preview, the fork picker, and every hotkey. A port of
//! the React page's use-find-prompts.ts and the key handling of find-prompts-view.tsx (deleted 2026-10-01).
//!
//! CDXC:PromptSearch 2026-09-27 WHY:
//! Results are a window, not the whole list, as in the React hook: gxserver holds ~30k prompts and
//! ranks them per query, so the window asks for 120 rows around the selection and pages when the
//! selection walks off the loaded edge. Each page is parsed and prepared (flattened first line,
//! match ranges, footer) on the background executor, and a reply that is not the newest request's
//! is dropped, so typing never waits on a stale search.
//!
//! CDXC:PromptSearch 2026-09-27 WHY:
//! Keys are read by a keystroke interceptor, before the query field's own bindings run, because
//! the React surface took every key on its scope first (`^k`, `^u`, `^Backspace`, Tab, Enter,
//! the arrows) and a text field binding would otherwise consume several of them.
use super::model::{
    FIND_PROMPT_AGENTS, FIND_PROMPT_FORK_AGENT_COUNT, FIND_PROMPTS_PAGE_SIZE, FindAction, FindMode,
    FindRow, ProjectFacet, SearchPage, ViewRow, build_view_rows, delete_word_backward,
    delete_word_forward, parse_search_page, resolve_find_action,
};
use super::palette::FindPalette;
use crate::app::gx_store::gx_rpc;
use gpui::{
    App, AppContext as _, Bounds, ClipboardItem, Context, Entity, FocusHandle, Focusable,
    Keystroke, ListAlignment, ListOffset, ListState, Pixels, Rgba, ScrollHandle, SharedString,
    Subscription, Window, WindowId, point, px,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use serde_json::json;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

/// Keystroke settle time before re-querying (`FIND_PROMPTS_QUERY_DEBOUNCE_MS`).
/// The query field's placeholder (the phone's Find says the same).
pub(super) const FIND_PLACEHOLDER: &str = "Search every prompt you have sent";
/// What the placeholder shortens to when the window is too narrow for the long one.
pub(super) const FIND_PLACEHOLDER_SHORT: &str = "Search your prompts";
const FIND_PROMPTS_QUERY_DEBOUNCE: Duration = Duration::from_millis(120);
/// Full prompt texts kept for the preview, so walking back over a long prompt does not fetch it again.
const FULL_TEXT_CACHE_LIMIT: usize = 64;

pub(crate) enum FindPromptsModalCommand {
    Close,
    /// A live Ghostex session already owns the conversation.
    FocusSession {
        project_id: String,
        session_id: String,
    },
    /// Nothing owns it: run `command` in `cwd` as a new session.
    LaunchSession {
        command: String,
        cwd: String,
        title: String,
    },
}

pub(crate) type FindPromptsModalHost = Rc<dyn Fn(FindPromptsModalCommand, &mut App)>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FindMenu {
    Agent,
    Project,
    /// The "…" button that holds Fav, View, Copy and Fork when the window is too narrow for them.
    Actions,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FindNoticeKind {
    Error,
    Info,
}

pub(crate) struct FindNotice {
    pub(crate) kind: FindNoticeKind,
    pub(crate) message: SharedString,
    pub(crate) detail: Option<SharedString>,
}

impl FindNotice {
    fn error(message: &str, detail: String) -> Self {
        Self {
            kind: FindNoticeKind::Error,
            message: SharedString::new(message),
            detail: Some(detail.into()),
        }
    }
}

pub(crate) struct GpuiFindPromptsModalWindow {
    host: FindPromptsModalHost,
    pub(super) p: FindPalette,
    pub(super) font_family: SharedString,
    pub(super) search: Entity<InputState>,
    query: String,
    debounced_query: String,
    query_generation: u64,
    pub(super) agents: [bool; FIND_PROMPT_AGENTS.len()],
    pub(super) project: Option<String>,
    pub(super) group_by_day: bool,
    pub(super) rows: Vec<FindRow>,
    pub(super) window_offset: usize,
    pub(super) matched: usize,
    pub(super) total: usize,
    pub(super) project_facets: Vec<ProjectFacet>,
    pub(super) agent_colors: [Option<Rgba>; FIND_PROMPT_AGENTS.len()],
    /// Absolute position of the selected row inside the matched list.
    pub(super) selection: usize,
    /// Full text of the selected prompt once fetched; the row text until then.
    pub(super) selected_text: Option<SharedString>,
    selected_text_key: Option<String>,
    text_generation: u64,
    full_texts: HashMap<String, SharedString>,
    pub(super) fork_open: bool,
    pub(super) open_menu: Option<FindMenu>,
    pub(super) menu_highlight: usize,
    pub(super) menu_search: Entity<InputState>,
    pub(super) menu_query: String,
    pub(super) agent_trigger: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(super) project_trigger: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(super) actions_trigger: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(super) preview_focused: bool,
    pub(super) wrap_preview: bool,
    pub(super) fullscreen_preview: bool,
    pub(super) expanded_prompt: bool,
    pub(super) loading: bool,
    pub(super) notice: Option<FindNotice>,
    /// Bumped on every copy; the Copy button shows a check until its timer for this count ends.
    pub(super) copied: u64,
    pub(super) copied_visible: bool,
    search_generation: u64,
    /// The (matched, rows, offset, selection) a page was last requested for, so a page that does
    /// not bring the selection in is not requested again (the React effect's dependency list).
    paged_for: Option<(usize, usize, usize, usize)>,
    pub(super) view_rows: Vec<ViewRow>,
    pub(super) list: ListState,
    pub(super) pending_reveal: bool,
    pub(super) preview_scroll: ScrollHandle,
    /// The bottom pane's prompt text as a read-only text area so it can be selected and copied,
    /// and the text and wrap mode it was last given (`sync_preview_input`).
    pub(super) preview_input: Entity<TextareaState>,
    pub(super) preview_input_text: SharedString,
    pub(super) preview_input_wrap: bool,
    /// Whether the query field currently shows the short placeholder (the window is too narrow for the long one).
    pub(super) short_placeholder: bool,
    /// The query row's measured text widths, taken once on the first render (the font is fixed for the window's life).
    pub(super) toolbar_metrics: Option<super::toolbar_steps::ToolbarMetrics>,
    pub(super) expanded_scroll: ScrollHandle,
    /// Unix seconds, refreshed every 30s so "6m ago" keeps moving.
    pub(super) now: i64,
    pub(super) reduce_motion: bool,
    _subscriptions: Vec<Subscription>,
}

fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

impl GpuiFindPromptsModalWindow {
    /// `light` is the Session Chat theme, `glass` window glass, and `font_family` the chat font
    /// setting, which is what the React page was opened with.
    pub(crate) fn new(
        host: FindPromptsModalHost,
        light: bool,
        glass: bool,
        font_family: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let p = FindPalette::resolve(light, glass);
        let muted = crate::app::window::native_modal_kit::hsla(p.muted);
        let search = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(FIND_PLACEHOLDER);
            input.set_placeholder_color(Some(muted));
            input
        });
        // CDXC:PromptSearch 2026-09-29 DECISION:
        // User: "also allow me to copy text from the prompt text shown at the bottom". The preview is a
        // read-only text area that grows to its text inside the pane's own scroller, so mouse selection
        // and Cmd+C work while the pane keeps its keyboard scrolling.
        let preview_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .soft_wrap(true)
                .auto_grow(1, 100_000)
        });
        let menu_search = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Filter projects...");
            input.set_placeholder_color(Some(muted));
            input
        });
        let change = cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    if value != this.query {
                        this.query = value;
                        this.schedule_query(cx);
                    }
                }
            },
        );
        let menu_change = cx.subscribe_in(
            &menu_search,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    if value != this.menu_query {
                        this.menu_query = value;
                        this.menu_highlight = 0;
                        cx.notify();
                    }
                }
            },
        );
        let window_id: WindowId = window.window_handle().window_id();
        let view = cx.weak_entity();
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle().window_id() != window_id {
                return;
            }
            let handled = view
                .update(cx, |this, cx| {
                    this.handle_keystroke(&event.keystroke, window, cx)
                })
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        });
        search.update(cx, |input, cx| input.focus(window, cx));
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                let alive = this.update(cx, |this, cx| {
                    this.now = unix_now();
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        let mut this = Self {
            host,
            p,
            font_family,
            search,
            query: String::new(),
            debounced_query: String::new(),
            query_generation: 0,
            agents: [false; FIND_PROMPT_AGENTS.len()],
            project: None,
            group_by_day: true,
            rows: Vec::new(),
            window_offset: 0,
            matched: 0,
            total: 0,
            project_facets: Vec::new(),
            agent_colors: [None; FIND_PROMPT_AGENTS.len()],
            selection: 0,
            selected_text: None,
            selected_text_key: None,
            text_generation: 0,
            full_texts: HashMap::new(),
            fork_open: false,
            open_menu: None,
            menu_highlight: 0,
            menu_search,
            menu_query: String::new(),
            agent_trigger: Rc::new(Cell::new(None)),
            project_trigger: Rc::new(Cell::new(None)),
            actions_trigger: Rc::new(Cell::new(None)),
            preview_focused: false,
            wrap_preview: true,
            fullscreen_preview: false,
            expanded_prompt: false,
            loading: true,
            notice: None,
            copied: 0,
            copied_visible: false,
            search_generation: 0,
            paged_for: None,
            view_rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(240.0)).measure_all(),
            pending_reveal: false,
            preview_scroll: ScrollHandle::new(),
            preview_input,
            preview_input_text: SharedString::default(),
            preview_input_wrap: true,
            short_placeholder: false,
            toolbar_metrics: None,
            expanded_scroll: ScrollHandle::new(),
            now: unix_now(),
            reduce_motion: crate::app::helpers::gpui_macos_reduce_motion_enabled(),
            _subscriptions: vec![change, menu_change, keys],
        };
        this._subscriptions
            .extend(super::super::popup_dismissal::close_app_modal_on_click_away(window, cx));
        this.run_search(0, cx);
        this
    }

    pub(super) fn post(&self, command: FindPromptsModalCommand, cx: &mut App) {
        (self.host)(command, cx);
    }

    pub(super) fn selected_row_index(&self) -> Option<usize> {
        let local = self.selection.checked_sub(self.window_offset)?;
        (local < self.rows.len()).then_some(local)
    }

    pub(super) fn selected_row(&self) -> Option<&FindRow> {
        self.selected_row_index().map(|index| &self.rows[index])
    }

    pub(super) fn agent_color(&self, index: usize) -> Option<Rgba> {
        self.agent_colors.get(index).copied().flatten()
    }

    fn schedule_query(&mut self, cx: &mut Context<Self>) {
        self.query_generation += 1;
        let generation = self.query_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(FIND_PROMPTS_QUERY_DEBOUNCE)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.query_generation != generation || this.debounced_query == this.query {
                    return;
                }
                this.debounced_query = this.query.clone();
                this.filter_changed(cx);
            });
        })
        .detach();
    }

    /// A new query or filter restarts at the top, like the terminal picker's recompute.
    fn filter_changed(&mut self, cx: &mut Context<Self>) {
        self.selection = 0;
        self.pending_reveal = true;
        self.paged_for = None;
        self.sync_selected_text(cx);
        self.run_search(0, cx);
    }

    fn run_search(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.search_generation += 1;
        let generation = self.search_generation;
        self.loading = true;
        cx.notify();
        let agents: Vec<&str> = FIND_PROMPT_AGENTS
            .iter()
            .zip(self.agents)
            .filter(|(_, on)| *on)
            .map(|(agent, _)| *agent)
            .collect();
        let mut params = json!({
            "agents": agents,
            "groupByDay": self.group_by_day,
            "includeFacets": true,
            "limit": FIND_PROMPTS_PAGE_SIZE,
            "offset": offset,
            "query": self.debounced_query,
            "refresh": false,
        });
        if let Some(project) = self.project.as_ref() {
            params["project"] = json!(project);
        }
        let task = cx.background_spawn(async move {
            let value = gx_rpc(None, "/api/searchAgentPrompts", params)
                .await
                .map_err(|error| error.message)?;
            parse_search_page(value)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| this.apply_search(generation, result, cx));
        })
        .detach();
    }

    fn apply_search(
        &mut self,
        generation: u64,
        result: Result<SearchPage, String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.search_generation {
            return;
        }
        self.loading = false;
        match result {
            Ok(page) => {
                self.rows = page.rows;
                self.window_offset = page.offset;
                self.matched = page.matched;
                self.total = page.total;
                if let Some(projects) = page.projects {
                    self.project_facets = projects;
                }
                if let Some(colors) = page.agent_colors {
                    self.agent_colors = colors;
                }
                self.notice = match (page.opencode_error, page.empryo_error) {
                    (None, None) => None,
                    (Some(detail), None) => Some(("opencode history could not be read.", detail)),
                    (None, Some(detail)) => Some(("Empryo history could not be read.", detail)),
                    (Some(opencode), Some(empryo)) => Some((
                        "opencode and Empryo history could not be read.",
                        format!("{opencode}\n{empryo}"),
                    )),
                }
                .map(|(message, detail)| FindNotice {
                    kind: FindNoticeKind::Info,
                    message: SharedString::new_static(message),
                    detail: Some(detail.into()),
                });
                self.rebuild_view_rows();
                self.pending_reveal = true;
                self.sync_selected_text(cx);
                self.page_toward_selection(cx);
            }
            Err(detail) => self.notice = Some(FindNotice::error("Search failed.", detail)),
        }
        cx.notify();
    }

    fn rebuild_view_rows(&mut self) {
        self.view_rows = build_view_rows(&self.rows, self.group_by_day);
        self.list.reset(self.view_rows.len());
    }

    /// Pages when the selection has walked off the loaded window.
    fn page_toward_selection(&mut self, cx: &mut Context<Self>) {
        if self.matched == 0 || self.selected_row_index().is_some() {
            return;
        }
        let key = (
            self.matched,
            self.rows.len(),
            self.window_offset,
            self.selection,
        );
        if self.paged_for == Some(key) {
            return;
        }
        self.paged_for = Some(key);
        let next = (self.matched - 1)
            .min(self.selection)
            .saturating_sub(FIND_PROMPTS_PAGE_SIZE / 2);
        self.run_search(next, cx);
    }

    /// The row payload is capped, so the selected prompt is pulled in full for the preview; short
    /// prompts arrived whole and skip the round trip.
    fn sync_selected_text(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            self.selected_text = None;
            self.selected_text_key = None;
            return;
        };
        if self.selected_text_key.as_deref() == Some(row.key.as_str()) {
            return;
        }
        let key = row.key.clone();
        let truncated = row.truncated;
        self.selected_text = Some(
            self.full_texts
                .get(&key)
                .cloned()
                .unwrap_or_else(|| row.text.clone()),
        );
        self.selected_text_key = Some(key.clone());
        if !truncated || self.full_texts.contains_key(&key) {
            return;
        }
        self.text_generation += 1;
        let generation = self.text_generation;
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(
                None,
                "/api/readAgentPromptText",
                json!({ "key": key.clone() }),
            )
            .await;
            // A failed top-up keeps the capped text showing rather than an error.
            let Some(text) = result
                .ok()
                .and_then(|value| value.get("text")?.as_str().map(SharedString::from))
            else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if this.full_texts.len() >= FULL_TEXT_CACHE_LIMIT {
                    this.full_texts.clear();
                }
                this.full_texts.insert(key.clone(), text.clone());
                if this.text_generation == generation
                    && this.selected_text_key.as_deref() == Some(key.as_str())
                {
                    this.selected_text = Some(text);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn select_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let next = index.min(self.matched.saturating_sub(1));
        if next == self.selection {
            return;
        }
        self.selection = next;
        self.pending_reveal = true;
        self.sync_selected_text(cx);
        self.page_toward_selection(cx);
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let last = self.matched as isize - 1;
        let next = (self.selection as isize + delta).min(last).max(0);
        self.select_row(next as usize, cx);
    }

    /// Day jumps scan the loaded window; past its edge the selection lands on the edge, which
    /// pages, and the next press continues from there.
    fn jump_day(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(local) = self.selected_row_index() else {
            self.move_selection(delta * FIND_PROMPTS_PAGE_SIZE as isize, cx);
            return;
        };
        let current_day = self.rows[local].day_key;
        if delta > 0 {
            let next = (local + 1..self.rows.len())
                .find(|index| self.rows[*index].day_key != current_day)
                .unwrap_or(self.rows.len());
            self.select_row(self.window_offset + next, cx);
            return;
        }
        if let Some(index) = (0..local)
            .rev()
            .find(|index| self.rows[*index].day_key != current_day)
        {
            let day = self.rows[index].day_key;
            let mut start = index;
            while start > 0 && self.rows[start - 1].day_key == day {
                start -= 1;
            }
            self.select_row(self.window_offset + start, cx);
            return;
        }
        self.select_row(self.window_offset.saturating_sub(1), cx);
    }

    fn toggle_favorite(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.selected_row_index() else {
            return;
        };
        let key = self.rows[index].key.clone();
        let favorite = !self.rows[index].favorite;
        // Paint at once; the list re-ranks on the next search (favorites rank above every score).
        self.rows[index].favorite = favorite;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(
                None,
                "/api/toggleAgentPromptFavorite",
                json!({ "favorite": favorite, "key": key.clone() }),
            )
            .await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    for row in this.rows.iter_mut().filter(|row| row.key == key) {
                        row.favorite = !favorite;
                    }
                    this.notice = Some(FindNotice::error(
                        "Could not update the favorite.",
                        error.message,
                    ));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Resume (`fork_agent` None) or fork the prompt at `key`, then focus or launch the session.
    fn launch(&mut self, key: String, fork_agent: Option<&'static str>, cx: &mut Context<Self>) {
        let mut params = json!({
            "action": if fork_agent.is_some() { "fork" } else { "resume" },
            "key": key,
        });
        if let Some(agent) = fork_agent {
            params["forkAgent"] = json!(agent);
        }
        let failure = if fork_agent.is_some() {
            "Could not fork."
        } else {
            "Could not resume."
        };
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/resolveAgentPromptLaunch", params).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(plan) => this.apply_launch_plan(&plan, cx),
                Err(error) => {
                    this.notice = Some(FindNotice::error(failure, error.message));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn apply_launch_plan(&mut self, plan: &serde_json::Value, cx: &mut Context<Self>) {
        let text = |key: &str| {
            plan.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let command = if plan.get("mode").and_then(serde_json::Value::as_str) == Some("focus") {
            FindPromptsModalCommand::FocusSession {
                project_id: text("projectId"),
                session_id: text("sessionId"),
            }
        } else {
            FindPromptsModalCommand::LaunchSession {
                command: text("commandLine"),
                cwd: text("cwd"),
                title: text("title"),
            }
        };
        self.post(command, cx);
    }

    pub(super) fn resume_row(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(key) = self.rows.get(index).map(|row| row.key.clone()) {
            self.launch(key, None, cx);
        }
    }

    pub(super) fn fork_selected(&mut self, agent: usize, cx: &mut Context<Self>) {
        let Some(key) = self.selected_row().map(|row| row.key.clone()) else {
            return;
        };
        self.fork_open = false;
        cx.notify();
        if let Some(agent) = FIND_PROMPT_AGENTS[..FIND_PROMPT_FORK_AGENT_COUNT].get(agent) {
            self.launch(key, Some(agent), cx);
        }
    }

    fn copy_selected(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self
            .selected_text
            .clone()
            .or_else(|| self.selected_row().map(|row| row.text.clone()))
            .filter(|text| !text.is_empty())
        else {
            return;
        };
        crate::app::helpers::gpui_copy_to_clipboard(
            ClipboardItem::new_string(text.to_string()),
            cx,
        );
        // CDXC:PromptSearch 2026-09-29 DECISION:
        // User: "when i click on copy and other actions please just show an indicator, don't show a whole
        // line bottom of the modal". Copy's own icon turns into a check for a moment; the notice line is
        // kept for failures only. The button's label follows the toolbar steps (`toolbar_steps.rs`), so
        // the check shows on the icon at every step.
        self.copied = self.copied.wrapping_add(1);
        self.copied_visible = true;
        let copied = self.copied;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1200))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.copied == copied {
                    this.copied_visible = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Rewrites the query line around its caret (`^k`, `^u`, the word deletes).
    fn edit_query(
        &mut self,
        transform: impl FnOnce(&str, usize) -> (String, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (value, caret) = {
            let input = self.search.read(cx);
            let value = input.value().to_string();
            let caret = input.cursor().min(value.len());
            transform(&value, caret)
        };
        self.search.update(cx, |input, cx| {
            input.set_value(value.clone(), window, cx);
            input.set_selected_range(caret..caret, cx);
        });
        if value != self.query {
            self.query = value;
            self.schedule_query(cx);
        }
    }

    pub(super) fn toggle_menu(
        &mut self,
        menu: FindMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.open_menu == Some(menu) {
            self.close_menu(window, cx);
            return;
        }
        self.open_menu = Some(menu);
        self.menu_highlight = 0;
        self.menu_query.clear();
        if menu == FindMenu::Project {
            self.menu_search.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
        }
        cx.notify();
    }

    /// Closing a filter menu hands the keyboard back to the query line.
    pub(super) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_menu.take().is_some() {
            self.search.update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        }
    }

    pub(super) fn toggle_agent(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(on) = self.agents.get_mut(index) {
            *on = !*on;
            self.filter_changed(cx);
        }
    }

    pub(super) fn clear_agents(&mut self, cx: &mut Context<Self>) {
        if self.agents.iter().any(|on| *on) {
            self.agents = [false; FIND_PROMPT_AGENTS.len()];
            self.filter_changed(cx);
        }
    }

    pub(super) fn set_project(
        &mut self,
        project: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_menu(window, cx);
        if self.project != project {
            self.project = project;
            self.filter_changed(cx);
        }
    }

    /// The project menu's rows for its search: "All projects" first, then the facets, by name or path.
    pub(super) fn project_menu_rows(&self) -> Vec<Option<usize>> {
        let query = self.menu_query.trim().to_lowercase();
        let mut rows: Vec<(u8, Option<usize>)> = Vec::new();
        let mut consider = |haystacks: &[&str], row: Option<usize>| {
            if query.is_empty() {
                rows.push((0, row));
                return;
            }
            if let Some(score) = haystacks
                .iter()
                .filter_map(|text| match_score(&text.to_lowercase(), &query))
                .max()
            {
                rows.push((score, row));
            }
        };
        consider(&["All projects"], None);
        for (index, facet) in self.project_facets.iter().enumerate() {
            consider(&[&facet.path, &facet.name], Some(index));
        }
        // cmdk ranks a search's matches; a stable sort keeps the list order within a rank.
        if !query.is_empty() {
            rows.sort_by(|a, b| b.0.cmp(&a.0));
        }
        rows.into_iter().map(|(_, row)| row).collect()
    }

    pub(super) fn choose_menu_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.open_menu {
            Some(FindMenu::Agent) => {
                self.menu_highlight = index;
                if index == 0 {
                    self.clear_agents(cx);
                } else {
                    self.toggle_agent(index - 1, cx);
                }
            }
            Some(FindMenu::Actions) => {
                let action = match index {
                    0 => FindAction::ToggleFavorite,
                    1 => FindAction::ViewPrompt,
                    2 => FindAction::CopyPrompt,
                    3 => FindAction::ForkPicker,
                    _ => return,
                };
                // Like the buttons they stand in for, the rows do nothing without a selected prompt.
                if self.selected_row().is_none() {
                    return;
                }
                self.close_menu(window, cx);
                self.run_action(action, window, cx);
            }
            Some(FindMenu::Project) => {
                let Some(row) = self.project_menu_rows().get(index).copied() else {
                    return;
                };
                let project = row.and_then(|index| {
                    self.project_facets
                        .get(index)
                        .map(|facet| facet.path.clone())
                });
                self.set_project(project, window, cx);
            }
            None => {}
        }
    }

    fn menu_row_count(&self) -> usize {
        match self.open_menu {
            Some(FindMenu::Agent) => FIND_PROMPT_AGENTS.len() + 1,
            Some(FindMenu::Project) => self.project_menu_rows().len(),
            Some(FindMenu::Actions) => 4,
            None => 0,
        }
    }

    /// An open filter menu owns the arrows, Enter and Escape; the agent menu has no text field,
    /// so it takes every other key too.
    fn handle_menu_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let control = keystroke.modifiers.control;
        let count = self.menu_row_count();
        match keystroke.key.as_str() {
            "escape" => self.close_menu(window, cx),
            "down" => {
                self.menu_highlight = (self.menu_highlight + 1).min(count.saturating_sub(1));
                cx.notify();
            }
            "n" if control => {
                self.menu_highlight = (self.menu_highlight + 1).min(count.saturating_sub(1));
                cx.notify();
            }
            "up" => {
                self.menu_highlight = self.menu_highlight.saturating_sub(1);
                cx.notify();
            }
            "p" if control => {
                self.menu_highlight = self.menu_highlight.saturating_sub(1);
                cx.notify();
            }
            "enter" => {
                if self.menu_highlight < count {
                    self.choose_menu_row(self.menu_highlight, window, cx);
                }
            }
            _ => {
                return matches!(
                    self.open_menu,
                    Some(FindMenu::Agent) | Some(FindMenu::Actions)
                );
            }
        }
        true
    }

    /// Every key the window receives, before the focused field's bindings. Returns whether it was used.
    fn handle_keystroke(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if matches!(
            keystroke.key.as_str(),
            "shift" | "control" | "alt" | "platform" | "function"
        ) {
            return false;
        }
        if self.expanded_prompt {
            if keystroke.key == "escape" {
                self.expanded_prompt = false;
                cx.notify();
                return true;
            }
            return false;
        }
        let mode = if self.fork_open {
            FindMode::ForkPicker
        } else if self.preview_focused {
            FindMode::Preview
        } else {
            FindMode::List
        };
        let action = resolve_find_action(keystroke, mode);
        if self.open_menu.is_some() {
            // Only a filter menu's own chord reaches the main map while one is open.
            if let Some(action @ (FindAction::OpenAgentPicker | FindAction::OpenProjectPicker)) =
                action
            {
                self.run_action(action, window, cx);
                return true;
            }
            return self.handle_menu_key(keystroke, window, cx);
        }
        let Some(action) = action else {
            return false;
        };
        self.run_action(action, window, cx);
        true
    }

    pub(super) fn run_action(
        &mut self,
        action: FindAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            FindAction::Move(delta) => self.move_selection(delta, cx),
            FindAction::JumpDay(delta) => self.jump_day(delta, cx),
            FindAction::ScrollPreview(delta) => {
                let viewport = self.preview_scroll.bounds().size.height;
                let offset = self.preview_scroll.offset();
                let max = self.preview_scroll.max_offset().y;
                let next = (offset.y - viewport * (0.9 * delta as f32)).clamp(-max, px(0.0));
                self.preview_scroll.set_offset(point(offset.x, next));
                cx.notify();
            }
            FindAction::ResumePrompt => {
                if let Some(index) = self.selected_row_index() {
                    self.resume_row(index, cx);
                }
            }
            FindAction::Close => self.post(FindPromptsModalCommand::Close, cx),
            FindAction::ToggleDayGrouping => {
                self.group_by_day = !self.group_by_day;
                // Headers follow at once; the re-sorted rows arrive with the search.
                self.rebuild_view_rows();
                self.filter_changed(cx);
            }
            FindAction::OpenAgentPicker => self.toggle_menu(FindMenu::Agent, window, cx),
            FindAction::OpenProjectPicker => self.toggle_menu(FindMenu::Project, window, cx),
            FindAction::ToggleFavorite => self.toggle_favorite(cx),
            FindAction::ViewPrompt => {
                self.expanded_prompt = !self.expanded_prompt;
                cx.notify();
            }
            FindAction::CopyPrompt => self.copy_selected(cx),
            FindAction::ForkPicker => {
                if self.fork_open {
                    self.fork_open = false;
                } else if self.selected_row().is_some() {
                    self.fork_open = true;
                }
                cx.notify();
            }
            FindAction::TogglePreviewFocus => {
                self.preview_focused = !self.preview_focused;
                cx.notify();
            }
            FindAction::ToggleWrap => {
                self.wrap_preview = !self.wrap_preview;
                cx.notify();
            }
            FindAction::ToggleFullscreenPreview => {
                self.fullscreen_preview = !self.fullscreen_preview;
                cx.notify();
            }
            FindAction::CancelOverlay => {
                self.fork_open = false;
                cx.notify();
            }
            FindAction::PickIndex(index) => {
                if self.fork_open {
                    self.fork_selected(index, cx);
                }
            }
            FindAction::KillToEnd => self.edit_query(
                |value, caret| (value[..caret].to_string(), caret),
                window,
                cx,
            ),
            FindAction::KillToStart => {
                self.edit_query(|value, caret| (value[caret..].to_string(), 0), window, cx)
            }
            FindAction::DeleteWordBackward => self.edit_query(
                |value, caret| {
                    let head = delete_word_backward(&value[..caret]);
                    (format!("{head}{}", &value[caret..]), head.len())
                },
                window,
                cx,
            ),
            FindAction::DeleteWordForward => self.edit_query(
                |value, caret| {
                    let tail = delete_word_forward(&value[caret..]);
                    (format!("{}{tail}", &value[..caret]), caret)
                },
                window,
                cx,
            ),
        }
    }

    /// Scrolls the list the least that shows the selected row (`scrollIntoView({ block: 'nearest' })`).
    /// Runs after the list has laid out (its rows are measured then); returns whether it scrolled.
    pub(super) fn reveal_selection(&mut self) -> bool {
        let Some(local) = self.selected_row_index() else {
            return false;
        };
        let Some(item) = self
            .view_rows
            .iter()
            .position(|row| *row == ViewRow::Row(local))
        else {
            return false;
        };
        let viewport = self.list.viewport_bounds();
        if viewport.size.height <= px(0.0) {
            return false;
        }
        match self.list.bounds_for_item(item) {
            Some(bounds) if bounds.top() < viewport.top() => {
                self.list.scroll_by(bounds.top() - viewport.top());
            }
            Some(bounds) if bounds.bottom() > viewport.bottom() => {
                self.list.scroll_by(bounds.bottom() - viewport.bottom());
            }
            Some(_) => return false,
            None => self.list.scroll_to(ListOffset {
                item_ix: item,
                offset_in_item: px(0.0),
            }),
        }
        true
    }
}

/// A rough cmdk score: a prefix beats a word start beats a substring beats a subsequence.
fn match_score(text: &str, query: &str) -> Option<u8> {
    if text.starts_with(query) {
        return Some(4);
    }
    if let Some(position) = text.find(query) {
        let at_word = text[..position]
            .chars()
            .last()
            .is_some_and(|previous| !previous.is_alphanumeric());
        return Some(if at_word { 3 } else { 2 });
    }
    let mut remaining = query.chars().filter(|character| !character.is_whitespace());
    let mut wanted = remaining.next();
    for character in text.chars() {
        if Some(character) == wanted {
            wanted = remaining.next();
        }
    }
    wanted.is_none().then_some(1)
}

impl Focusable for GpuiFindPromptsModalWindow {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search.read(cx).focus_handle(cx)
    }
}

impl crate::app::window::native_modal_kit::ModalCornerClose for GpuiFindPromptsModalWindow {
    fn close_from_corner(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.post(FindPromptsModalCommand::Close, cx);
    }
}
