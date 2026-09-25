//! The native Ghostex Quick Access window: one borderless child window whose four tabs
//! (Commands, Projects, Sessions, Saved Prompts) replace the React pages that used to render in
//! the CEF modal host.
//!
//! CDXC:AppModal 2026-09-21 DECISION:
//! User: make the GPUI Quick Access look like Raycast instead of the React one: one large search line with the
//! tab's filters at its right edge, clean single-line rows, and a footer that holds the four tabs on the left and
//! the Return action plus the Actions panel (Cmd+K) on the right. Every tab and every filter stays. Row buttons
//! moved into the row's actions (right-click, Cmd+K, or the action's own hotkey). Supersedes the 2026-09-20
//! instruction to look exactly like the React one; the React twins keep their look on web and mobile.
//! The sidebar runtime keeps owning data and commands and publishes one resolved snapshot per frame; this
//! window paints it, owns the search field, hover, scroll and keyboard, and posts interactions back.
//! SEE-ALSO: packages/shared/native-quick-access.ts (the contract), apps/desktop/sidebar/native-quick-access/ (the controller),
//! apps/desktop/src/app/quick_access_modal_lifecycle.rs (open, snapshot routing, close),
//! packages/core-ui/command-palette.tsx, recent-projects-modal.tsx, previous-sessions-modal.tsx, stashed-prompts-modal.tsx (the retained React twins).
use super::actions_menu::{QuickAccessMenuRequest, QuickAccessOpenMenu};
use super::chrome::{
    QuickAccessMenuState, capture_bounds, quick_access_filter_trigger, quick_access_footer,
    quick_access_search_bar, quick_access_select_menu, quick_access_tooltip, segments_as_select,
    visible_options,
};
use super::editor::{quick_access_prompt_editor, quick_access_tag_composer};
use super::model::{QuickAccessSnapshot, QuickAccessTabId, QuickAccessToolbar};
use super::palette::{
    QUICK_ACCESS_GROUP_HEADING_HEIGHT, QUICK_ACCESS_ITEM_FONT_SIZE, QUICK_ACCESS_LIST_PADDING,
    QUICK_ACCESS_ROW_PADDING_X, QuickAccessPalette, hsla,
};
use super::rows::{RowCallbacks, quick_access_row};
use crate::app::window::native_modal_kit::MODAL_UI_FONT;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Point, Render, Rgba,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    div, point, px, svg,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::v_flex;
use serde_json::json;
use std::rc::Rc;

pub(crate) type QuickAccessHost = Rc<dyn Fn(serde_json::Value, &mut App)>;

pub(crate) struct GpuiQuickAccessWindow {
    host: QuickAccessHost,
    pub(crate) snapshot: Option<QuickAccessSnapshot>,
    search: Entity<InputState>,
    query: String,
    applied_query_revision: u64,
    hovered: Option<String>,
    /// Counts the `select` commands this window sent, so a snapshot built before the newest one is recognisable.
    selection_seq: u64,
    scroll: ScrollHandle,
    pending_scroll: Option<usize>,
    /// Saved / Recovered / Sent on Saved Prompts, All / Closed / External on Sessions.
    view_menu: QuickAccessMenuState,
    project_menu: QuickAccessMenuState,
    tag_menu: QuickAccessMenuState,
    editor_project_menu: QuickAccessMenuState,
    editor_tag_menu: QuickAccessMenuState,
    pub(crate) editor_input: Option<Entity<InputState>>,
    /// Where the next menu the runtime sends belongs. Kept after a choice because
    /// an item may answer with a submenu (Tag…) that opens in the same place.
    pub(super) menu_request: Option<QuickAccessMenuRequest>,
    pub(super) context_menu: Option<QuickAccessOpenMenu>,
    tag_name_input: Option<Entity<InputState>>,
    /// Where the row's actions menu stood when it opened the create-tag popover,
    /// since a row has no captured trigger bounds of its own.
    pub(super) tag_composer_anchor: Option<Point<gpui::Pixels>>,
    /// A selection the pointer made must not scroll the list under it; only
    /// keyboard moves and re-ranked queries reveal their row.
    suppress_scroll: bool,
    last_load_more: Option<web_time::Instant>,
    was_active: bool,
    focus_handle: FocusHandle,
    subscriptions: Vec<Subscription>,
}

impl GpuiQuickAccessWindow {
    pub(crate) fn new(host: QuickAccessHost, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx));
        let change = cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    if value == this.query {
                        return;
                    }
                    this.query = value.clone();
                    this.post(json!({ "type": "query", "query": value }), cx);
                    cx.notify();
                }
            },
        );
        let activation = cx.observe_window_activation(window, |this: &mut Self, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active {
                this.close(cx);
            }
        });
        search.update(cx, |input, cx| input.focus(window, cx));
        Self {
            host,
            snapshot: None,
            search,
            query: String::new(),
            applied_query_revision: 0,
            hovered: None,
            selection_seq: 0,
            scroll: ScrollHandle::new(),
            pending_scroll: None,
            view_menu: QuickAccessMenuState::default(),
            project_menu: QuickAccessMenuState::default(),
            tag_menu: QuickAccessMenuState::default(),
            editor_project_menu: QuickAccessMenuState::default(),
            editor_tag_menu: QuickAccessMenuState::default(),
            editor_input: None,
            menu_request: None,
            context_menu: None,
            tag_name_input: None,
            tag_composer_anchor: None,
            suppress_scroll: false,
            last_load_more: None,
            was_active: window.is_window_active(),
            focus_handle: cx.focus_handle(),
            subscriptions: vec![change, activation],
        }
    }

    /// Quick Access follows the live app appearance and sidebar theme, the way
    /// the React surface followed `<body data-sidebar-theme>` and the child
    /// window's own fill.
    pub(crate) fn palette(&self) -> QuickAccessPalette {
        let light = crate::CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        QuickAccessPalette::resolve(
            light,
            settings
                .object()
                .get("sidebarTheme")
                .and_then(serde_json::Value::as_str),
            self.window_background(),
        )
    }

    fn window_background(&self) -> Rgba {
        if crate::CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
            gpui::rgb(0xffffff)
        } else {
            Rgba::from(crate::app::helpers::titlebar_background())
        }
    }

    pub(crate) fn post(&self, command: serde_json::Value, cx: &mut App) {
        (self.host)(command, cx);
    }

    pub(crate) fn tab(&self) -> QuickAccessTabId {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.tab)
            .unwrap_or_default()
    }

    pub(crate) fn apply_snapshot(
        &mut self,
        snapshot: QuickAccessSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if snapshot.query_revision != self.applied_query_revision {
            self.applied_query_revision = snapshot.query_revision;
            self.query = snapshot.query.clone();
            let query = snapshot.query.clone();
            self.search.update(cx, |input, cx| {
                input.set_value(query, window, cx);
                input.focus(window, cx);
            });
        }
        let placeholder = snapshot.placeholder.clone();
        if self
            .snapshot
            .as_ref()
            .map(|previous| previous.placeholder != placeholder)
            .unwrap_or(true)
        {
            self.search.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx);
            });
        }
        // The editor is a separate text surface; create and release its state with the panel.
        match (&snapshot.editor, self.editor_input.is_some()) {
            (Some(editor), false) => {
                let content = editor.content.clone();
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .multi_line(true)
                        .placeholder("Write a prompt you want to save...")
                        .default_value(content)
                });
                let change = cx.subscribe_in(
                    &input,
                    window,
                    |this: &mut Self, input, event: &InputEvent, _window, cx| {
                        if matches!(event, InputEvent::Change) {
                            let value = input.read(cx).value().to_string();
                            this.post(
                                json!({ "type": "editorField", "field": "content", "value": value }),
                                cx,
                            );
                        }
                    },
                );
                input.update(cx, |input, cx| input.focus(window, cx));
                self.subscriptions.push(change);
                self.editor_input = Some(input);
            }
            (None, true) => {
                self.editor_input = None;
                self.search.update(cx, |input, cx| input.focus(window, cx));
            }
            _ => {}
        }
        match (&snapshot.tag_composer, self.tag_name_input.is_some()) {
            (Some(composer), false) => {
                let name = composer.name.clone();
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("Tag name")
                        .default_value(name)
                });
                let change = cx.subscribe_in(
                    &input,
                    window,
                    |this: &mut Self, input, event: &InputEvent, _window, cx| {
                        if matches!(event, InputEvent::Change) {
                            let value = input.read(cx).value().to_string();
                            this.post(
                                json!({ "type": "tagComposerField", "field": "name", "value": value }),
                                cx,
                            );
                        }
                    },
                );
                input.update(cx, |input, cx| input.focus(window, cx));
                self.subscriptions.push(change);
                self.tag_name_input = Some(input);
            }
            (None, true) => {
                self.tag_name_input = None;
                self.search.update(cx, |input, cx| input.focus(window, cx));
            }
            _ => {}
        }
        let mut snapshot = snapshot;
        self.keep_newer_local_selection(&mut snapshot);
        let selection_changed = self
            .snapshot
            .as_ref()
            .map(|previous| previous.selected_key != snapshot.selected_key)
            .unwrap_or(true);
        self.snapshot = Some(snapshot);
        if selection_changed && !std::mem::take(&mut self.suppress_scroll) {
            self.pending_scroll = self.selected_flat_index();
        }
        cx.notify();
    }

    /// The scroll index of the selected row, counting headings as rows because
    /// the list renders them as siblings so the handle can reach either.
    fn selected_flat_index(&self) -> Option<usize> {
        let snapshot = self.snapshot.as_ref()?;
        if snapshot.selected_key.is_empty() {
            return None;
        }
        let mut index = 0usize;
        for group in &snapshot.groups {
            if !group.heading.is_empty() {
                index += 1;
            }
            for row in &group.rows {
                if row.key() == snapshot.selected_key {
                    return Some(index);
                }
                index += 1;
            }
        }
        None
    }

    fn row_callbacks(&self) -> RowCallbacks<Self> {
        RowCallbacks {
            on_activate: Rc::new(|this: &mut Self, key, _window, cx| {
                this.post(json!({ "type": "activate", "key": key }), cx);
            }),
            on_hover: Rc::new(|this: &mut Self, key, _window, cx| {
                if this.hovered.as_deref() == Some(key.as_str()) {
                    return;
                }
                this.hovered = Some(key.clone());
                this.suppress_scroll = true;
                this.post_selection(&key, cx);
                cx.notify();
            }),
            on_secondary: Rc::new(|this: &mut Self, key, position, _window, cx| {
                this.request_row_menu(&key, QuickAccessMenuRequest::at_pointer(position), cx);
            }),
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.post(json!({ "type": "close" }), cx);
    }

    fn move_selection(&mut self, direction: i32, cx: &mut Context<Self>) -> bool {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return false;
        };
        let keys = snapshot
            .rows()
            .map(|row| row.key().to_string())
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return false;
        }
        let current = keys
            .iter()
            .position(|key| key == &snapshot.selected_key)
            .map(|index| index as i32);
        let next = match current {
            Some(index) => (index + direction).rem_euclid(keys.len() as i32),
            None if direction > 0 => 0,
            None => keys.len() as i32 - 1,
        } as usize;
        self.hovered = None;
        self.suppress_scroll = false;
        let key = keys[next].clone();
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.selected_key = key.clone();
        }
        self.pending_scroll = self.selected_flat_index();
        self.post_selection(&key, cx);
        true
    }

    fn post_selection(&mut self, key: &str, cx: &mut App) {
        self.selection_seq += 1;
        self.post(
            json!({ "type": "select", "key": key, "seq": self.selection_seq }),
            cx,
        );
    }

    /// CDXC:AppModal 2026-09-21 WHY:
    /// The runtime publishes a snapshot a frame after it hears a command, so while Down is held a snapshot carrying an older selection arrives after the window has already moved on. Applying it moved the highlight back and the next key press continued from there, which made the list jump. A snapshot older than the window's newest selection keeps the window's row, as long as that row is still listed.
    fn keep_newer_local_selection(&self, snapshot: &mut QuickAccessSnapshot) {
        if snapshot.selection_seq >= self.selection_seq {
            return;
        }
        let Some(local) = self.snapshot.as_ref().map(|current| &current.selected_key) else {
            return;
        };
        if snapshot.rows().any(|row| row.key() == local) {
            snapshot.selected_key = local.clone();
        }
    }

    /// The open picker owns Up/Down/Enter/Escape and, when it is searchable,
    /// the typed filter, exactly like the React `Command` inside each popover.
    fn menu_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let which = if self.view_menu.open {
            "view"
        } else if self.project_menu.open {
            "project"
        } else if self.tag_menu.open {
            "tag"
        } else if self.editor_project_menu.open {
            "editorProject"
        } else if self.editor_tag_menu.open {
            "editorTag"
        } else {
            return false;
        };
        let Some(select) = self.snapshot.as_ref().and_then(|snapshot| match which {
            "view" => view_select(snapshot),
            "project" => project_select(snapshot),
            "tag" => tag_select(snapshot),
            "editorProject" => snapshot
                .editor
                .as_ref()
                .map(|editor| editor.projects.clone()),
            _ => snapshot.editor.as_ref().map(|editor| editor.tags.clone()),
        }) else {
            return false;
        };
        let state = match which {
            "view" => &mut self.view_menu,
            "project" => &mut self.project_menu,
            "tag" => &mut self.tag_menu,
            "editorProject" => &mut self.editor_project_menu,
            _ => &mut self.editor_tag_menu,
        };
        let count = visible_options(&select, &state.query).len();
        match key {
            "escape" => {
                state.close();
                cx.notify();
                return true;
            }
            "up" | "down" if count > 0 => {
                let delta = if key == "down" { 1 } else { count - 1 };
                state.highlight = (state.highlight.min(count - 1) + delta) % count;
                state.scroll.scroll_to_item(state.highlight);
                cx.notify();
                return true;
            }
            "backspace" if select.searchable && !state.query.is_empty() => {
                state.query.pop();
                state.highlight = 0;
                cx.notify();
                return true;
            }
            "enter" if count > 0 => {
                let highlight = state.highlight.min(count - 1);
                let value = visible_options(&select, &state.query)[highlight]
                    .value
                    .clone();
                self.choose_menu_value(which, value, cx);
                cx.notify();
                return true;
            }
            _ => {}
        }
        if select.searchable
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
            && let Some(text) = event.keystroke.key_char.as_deref()
            && text.chars().all(|character| !character.is_control())
        {
            state.query.push_str(text);
            state.highlight = 0;
            cx.notify();
            return true;
        }
        false
    }

    fn choose_menu_value(&mut self, which: &str, value: String, cx: &mut Context<Self>) {
        match which {
            "view" => {
                self.view_menu.close();
                let kind = if self.tab() == QuickAccessTabId::RecentSessions {
                    "scope"
                } else {
                    "view"
                };
                self.post(json!({ "type": kind, "value": value }), cx);
            }
            "project" => {
                self.project_menu.close();
                self.post(json!({ "type": "project", "value": value }), cx);
            }
            "tag" => {
                self.post(json!({ "type": "tagFilter", "value": value }), cx);
            }
            "editorProject" => {
                self.editor_project_menu.close();
                self.post(
                    json!({ "type": "editorField", "field": "project", "value": value }),
                    cx,
                );
            }
            _ => {
                self.editor_tag_menu.close();
                self.post(
                    json!({ "type": "editorField", "field": "tag", "value": value }),
                    cx,
                );
            }
        }
    }

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if self.actions_menu_key(event, cx) {
            cx.stop_propagation();
            return;
        }
        if self.menu_key(event, cx) {
            cx.stop_propagation();
            return;
        }
        // `⌘1`..`⌘4` reach every tab from every tab, matching QuickAccessHeader.
        if modifiers.platform && !modifiers.shift && !modifiers.alt {
            let tab = match key {
                "1" => Some(QuickAccessTabId::Commands),
                "2" => Some(QuickAccessTabId::RecentProjects),
                "3" => Some(QuickAccessTabId::RecentSessions),
                "4" => Some(QuickAccessTabId::SavedPrompts),
                _ => None,
            };
            if let Some(tab) = tab {
                cx.stop_propagation();
                self.post(json!({ "type": "tab", "tab": tab.wire_name() }), cx);
                return;
            }
        }
        /* CDXC:AppModal 2026-09-20 WHY:
        Option+C cycles All / Closed / External and takes precedence over other hotkeys while
        Sessions is open, the same reservation `isReservedQuickAccessSessionScopeHotkey` makes in
        the React host. */
        if modifiers.alt
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.shift
            && key == "c"
            && self.tab() == QuickAccessTabId::RecentSessions
        {
            cx.stop_propagation();
            self.post(json!({ "type": "scope", "value": "cycle" }), cx);
            return;
        }
        if self.editor_input.is_some() {
            // The editor form owns its own keys: Escape cancels, Cmd+Enter saves.
            if key == "escape" {
                cx.stop_propagation();
                self.post(json!({ "type": "editorCancel" }), cx);
            } else if key == "enter" && (modifiers.platform || modifiers.control) {
                cx.stop_propagation();
                self.post(json!({ "type": "editorSubmit" }), cx);
            }
            return;
        }
        if self.tag_name_input.is_some() {
            if key == "escape" {
                cx.stop_propagation();
                self.post(json!({ "type": "tagComposerCancel" }), cx);
            } else if key == "enter" {
                cx.stop_propagation();
                self.post(json!({ "type": "tagComposerSubmit" }), cx);
            }
            return;
        }
        if modifiers.secondary() && !modifiers.shift && !modifiers.alt && key == "k" {
            cx.stop_propagation();
            self.open_actions_panel(window, cx);
            return;
        }
        if let Some(hotkey) = wire_hotkey(event)
            && self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.action_hotkeys.contains(&hotkey))
        {
            cx.stop_propagation();
            let key = self.selected_key();
            // The action may answer with a submenu (Tag…), which opens where the Actions panel does.
            self.menu_request = Some(QuickAccessMenuRequest::actions_panel(window));
            self.post(
                json!({ "type": "actionHotkey", "key": key, "hotkey": hotkey }),
                cx,
            );
            return;
        }
        match key {
            "escape" => {
                cx.stop_propagation();
                self.close(cx);
            }
            "up" | "down" => {
                if self.move_selection(if key == "down" { 1 } else { -1 }, cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }
            "enter" if !modifiers.alt && !modifiers.control && !modifiers.shift => {
                if self.activate_selected(cx) {
                    cx.stop_propagation();
                }
            }
            _ => {
                if self.view_menu.open
                    || self.project_menu.open
                    || self.tag_menu.open
                    || self.editor_project_menu.open
                    || self.editor_tag_menu.open
                {
                    return;
                }
                // Everything else is search text; keep the field focused so a key
                // pressed while a row or button had focus still lands in the query.
                let search = self.search.clone();
                if !search.read(cx).focus_handle(cx).is_focused(window) {
                    search.update(cx, |input, cx| input.focus(window, cx));
                }
            }
        }
    }
}

/// Return's target: the selected row, when it can be activated.
impl GpuiQuickAccessWindow {
    pub(super) fn selected_key(&self) -> String {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.selected_key.clone())
            .unwrap_or_default()
    }

    fn activate_selected(&mut self, cx: &mut Context<Self>) -> bool {
        let selected = self
            .snapshot
            .as_ref()
            .filter(|snapshot| !snapshot.selected_key.is_empty())
            .and_then(|snapshot| {
                snapshot
                    .rows()
                    .find(|row| row.key() == snapshot.selected_key)
                    .filter(|row| row.is_activatable())
                    .map(|row| row.key().to_string())
            });
        let Some(key) = selected else {
            return false;
        };
        self.post(json!({ "type": "activate", "key": key }), cx);
        true
    }

    pub(super) fn close_filter_menus(&mut self) {
        self.view_menu.close();
        self.project_menu.close();
        self.tag_menu.close();
    }
}

/// The pressed chord in the runtime's wire form (`cmd+shift+c`), when it carries a command modifier.
fn wire_hotkey(event: &KeyDownEvent) -> Option<String> {
    let modifiers = event.keystroke.modifiers;
    if !modifiers.secondary() {
        return None;
    }
    let mut parts = vec!["cmd"];
    if modifiers.control && modifiers.platform {
        parts.push("ctrl");
    }
    if modifiers.alt {
        parts.push("alt");
    }
    if modifiers.shift {
        parts.push("shift");
    }
    parts.push(event.keystroke.key.as_str());
    Some(parts.join("+"))
}

fn view_select(snapshot: &QuickAccessSnapshot) -> Option<super::model::QuickAccessSelect> {
    match &snapshot.toolbar {
        QuickAccessToolbar::Sessions { scope, scopes, .. } => {
            Some(segments_as_select(scopes, scope))
        }
        QuickAccessToolbar::Prompts { view, views, .. } => Some(segments_as_select(views, view)),
        QuickAccessToolbar::None => None,
    }
}

fn project_select(snapshot: &QuickAccessSnapshot) -> Option<super::model::QuickAccessSelect> {
    match &snapshot.toolbar {
        QuickAccessToolbar::Sessions { projects, .. }
        | QuickAccessToolbar::Prompts { projects, .. } => Some(projects.clone()),
        QuickAccessToolbar::None => None,
    }
}

fn tag_select(snapshot: &QuickAccessSnapshot) -> Option<super::model::QuickAccessSelect> {
    match &snapshot.toolbar {
        QuickAccessToolbar::Sessions { tags, .. } | QuickAccessToolbar::Prompts { tags, .. } => {
            Some(tags.clone())
        }
        QuickAccessToolbar::None => None,
    }
}

impl Render for GpuiQuickAccessWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette();
        let snapshot = self.snapshot.clone();
        let tabs = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .tabs
                    .iter()
                    .map(|tab| {
                        (
                            SharedString::from(tab.label.clone()),
                            SharedString::from(tab.hotkey.clone()),
                            tab.id == snapshot.tab,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let tab_ids = snapshot
            .as_ref()
            .map(|snapshot| snapshot.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>())
            .unwrap_or_default();
        if let Some(index) = self.pending_scroll.take() {
            self.scroll.scroll_to_item(index);
        }
        let mut overlays: Vec<AnyElement> = Vec::new();
        if let Some(snapshot) = snapshot.as_ref() {
            if let Some(menu) = quick_access_select_menu(
                &p,
                &view_select(snapshot).unwrap_or_default(),
                &self.view_menu,
                "quick-access-view-menu",
                160.0,
                |this: &mut Self, value, _window, cx| {
                    this.choose_menu_value("view", value, cx);
                    cx.notify();
                },
                |this: &mut Self, _window, cx| {
                    this.view_menu.close();
                    cx.notify();
                },
                window,
                cx,
            ) {
                overlays.push(menu);
            }
            if let Some(menu) = quick_access_select_menu(
                &p,
                &project_select(snapshot).unwrap_or_default(),
                &self.project_menu,
                "quick-access-project-menu",
                240.0,
                |this: &mut Self, value, _window, cx| {
                    this.project_menu.close();
                    this.post(json!({ "type": "project", "value": value }), cx);
                    cx.notify();
                },
                |this: &mut Self, _window, cx| {
                    this.project_menu.close();
                    cx.notify();
                },
                window,
                cx,
            ) {
                overlays.push(menu);
            }
            if let Some(menu) = quick_access_select_menu(
                &p,
                &tag_select(snapshot).unwrap_or_default(),
                &self.tag_menu,
                "quick-access-tag-menu",
                224.0,
                |this: &mut Self, value, _window, cx| {
                    this.post(json!({ "type": "tagFilter", "value": value }), cx);
                    cx.notify();
                },
                |this: &mut Self, _window, cx| {
                    this.tag_menu.close();
                    cx.notify();
                },
                window,
                cx,
            ) {
                overlays.push(menu);
            }
            if let Some(editor) = snapshot.editor.as_ref() {
                if let Some(menu) = quick_access_select_menu(
                    &p,
                    &editor.projects,
                    &self.editor_project_menu,
                    "quick-access-editor-project-menu",
                    240.0,
                    |this: &mut Self, value, _window, cx| {
                        this.editor_project_menu.close();
                        this.post(
                            json!({ "type": "editorField", "field": "project", "value": value }),
                            cx,
                        );
                        cx.notify();
                    },
                    |this: &mut Self, _window, cx| {
                        this.editor_project_menu.close();
                        cx.notify();
                    },
                    window,
                    cx,
                ) {
                    overlays.push(menu);
                }
                if let Some(menu) = quick_access_select_menu(
                    &p,
                    &editor.tags,
                    &self.editor_tag_menu,
                    "quick-access-editor-tag-menu",
                    224.0,
                    |this: &mut Self, value, _window, cx| {
                        this.editor_tag_menu.close();
                        this.post(
                            json!({ "type": "editorField", "field": "tag", "value": value }),
                            cx,
                        );
                        cx.notify();
                    },
                    |this: &mut Self, _window, cx| {
                        this.editor_tag_menu.close();
                        cx.notify();
                    },
                    window,
                    cx,
                ) {
                    overlays.push(menu);
                }
            }
            if let Some(composer) = snapshot.tag_composer.as_ref() {
                let anchor = if composer.anchor.starts_with("row:") {
                    self.tag_composer_anchor
                } else {
                    self.tag_menu.trigger_bounds.get().map(anchor_below)
                };
                overlays.push(quick_access_tag_composer(
                    &p,
                    composer,
                    self.tag_name_input.as_ref(),
                    anchor,
                    cx,
                ));
            }
            if let Some(menu) = self.context_menu.as_ref() {
                overlays.push(self.render_context_menu(&p, menu, cx));
            }
        }
        let primary_action = snapshot
            .as_ref()
            .filter(|snapshot| snapshot.editor.is_none())
            .map(|snapshot| snapshot.primary_action.clone())
            .unwrap_or_default();
        let actions_open = self.actions_panel_open();
        div()
            .id("quick-access-window")
            .size_full()
            .overflow_hidden()
            .bg(hsla(p.window))
            .font_family(MODAL_UI_FONT)
            .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
            .line_height(px(18.0))
            .text_color(hsla(p.item))
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key(event, window, cx);
            }))
            /* CDXC:AppModal 2026-09-21 DECISION:
            User: while Quick Access is showing, the hotkeys that open its tabs (Saved Prompts, Commands, Sessions) must switch to those tabs. App hotkeys are answered by the main window's root, which this window is not under, so they are answered here as the same tab switch the rail and Cmd+1..4 make. Re-running the app route instead would leave Saved Prompts unreachable whenever no agent session has focus. */
            .on_action(cx.listener(
                |this, action: &crate::app::hotkeys::RunConfiguredGhostexHotkey, _window, cx| {
                    let tab = match action.action_id.as_str() {
                        "openCommandPalette" => QuickAccessTabId::Commands,
                        "openSessionSearchPalette" => QuickAccessTabId::RecentSessions,
                        "stashedPrompts" => QuickAccessTabId::SavedPrompts,
                        _ => {
                            cx.propagate();
                            return;
                        }
                    };
                    this.post(json!({ "type": "tab", "tab": tab.wire_name() }), cx);
                },
            ))
            .child(
                v_flex()
                    .size_full()
                    .min_h_0()
                    .children(
                        snapshot
                            .as_ref()
                            .map(|snapshot| self.render_body(&p, snapshot, cx)),
                    )
                    .child(quick_access_footer(
                        &p,
                        &tabs,
                        &primary_action,
                        actions_open,
                        move |this: &mut Self, index, _window, cx| {
                            if let Some(tab) = tab_ids.get(index).copied() {
                                this.post(json!({ "type": "tab", "tab": tab.wire_name() }), cx);
                            }
                        },
                        |this: &mut Self, _window, cx| {
                            this.activate_selected(cx);
                        },
                        |this: &mut Self, window, cx| {
                            this.open_actions_panel(window, cx);
                        },
                        cx,
                    )),
            )
            .children(overlays)
    }
}

impl GpuiQuickAccessWindow {
    fn render_body(
        &mut self,
        p: &QuickAccessPalette,
        snapshot: &QuickAccessSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(editor) = snapshot.editor.as_ref() {
            return quick_access_prompt_editor(
                p,
                editor,
                self.editor_input.as_ref(),
                &self.editor_project_menu,
                &self.editor_tag_menu,
                cx,
            );
        }
        let has_query = !self.query.is_empty();
        let filters = self.render_filters(p, snapshot, cx);
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(quick_access_search_bar(
                p,
                &self.search,
                has_query,
                filters,
                |this: &mut Self, window, cx| {
                    this.query.clear();
                    this.search.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                        input.focus(window, cx);
                    });
                    this.post(json!({ "type": "query", "query": "" }), cx);
                    cx.notify();
                },
                cx,
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .child(self.render_list(p, snapshot, cx))
                    .children(
                        (!snapshot.hint.is_empty())
                            .then(|| self.render_stash_hint(p, &snapshot.hint, cx)),
                    ),
            )
            .into_any_element()
    }

    /// The tab's filters, in the search line's order: the single-choice view first, then project, then tags.
    fn render_filters(
        &mut self,
        p: &QuickAccessPalette,
        snapshot: &QuickAccessSnapshot,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (view, view_tooltip, projects, tags, tags_active) = match &snapshot.toolbar {
            QuickAccessToolbar::None => return Vec::new(),
            QuickAccessToolbar::Sessions {
                scope,
                scopes,
                scope_hotkey,
                tag_filter_active,
                tags,
                projects,
                ..
            } => (
                segments_as_select(scopes, scope),
                (!scope_hotkey.is_empty()).then(|| format!("Cycle with {scope_hotkey}")),
                projects,
                tags,
                *tag_filter_active,
            ),
            QuickAccessToolbar::Prompts {
                view,
                views,
                projects,
                tags,
                ..
            } => (segments_as_select(views, view), None, projects, tags, false),
        };
        let mut tags = tags.clone();
        if tags.label.is_empty() {
            tags.label = "Tags".to_string();
        }
        vec![
            div()
                .flex_shrink_0()
                .on_children_prepainted(capture_bounds(self.view_menu.trigger_bounds.clone(), 0))
                .child(quick_access_filter_trigger(
                    p,
                    &view,
                    &self.view_menu,
                    "quick-access-view-filter",
                    false,
                    view_tooltip,
                    |this: &mut Self, _window, cx| {
                        this.project_menu.close();
                        this.tag_menu.close();
                        this.view_menu.toggle();
                        cx.notify();
                    },
                    cx,
                ))
                .into_any_element(),
            div()
                .flex_shrink_0()
                .on_children_prepainted(capture_bounds(self.project_menu.trigger_bounds.clone(), 0))
                .child(quick_access_filter_trigger(
                    p,
                    projects,
                    &self.project_menu,
                    "quick-access-project-filter",
                    false,
                    None,
                    |this: &mut Self, _window, cx| {
                        this.view_menu.close();
                        this.tag_menu.close();
                        this.project_menu.toggle();
                        cx.notify();
                    },
                    cx,
                ))
                .into_any_element(),
            div()
                .flex_shrink_0()
                .on_children_prepainted(capture_bounds(self.tag_menu.trigger_bounds.clone(), 0))
                .child(quick_access_filter_trigger(
                    p,
                    &tags,
                    &self.tag_menu,
                    "quick-access-tag-filter",
                    tags_active,
                    None,
                    |this: &mut Self, _window, cx| {
                        this.view_menu.close();
                        this.project_menu.close();
                        this.tag_menu.toggle();
                        cx.notify();
                    },
                    cx,
                ))
                .into_any_element(),
        ]
    }

    fn render_list(
        &mut self,
        p: &QuickAccessPalette,
        snapshot: &QuickAccessSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *p;
        let callbacks = self.row_callbacks();
        let hovered = self.hovered.clone();
        let selected = snapshot.selected_key.clone();
        let mut children: Vec<AnyElement> = Vec::new();
        let mut row_index = 0usize;
        for group in &snapshot.groups {
            if group.separated && !children.is_empty() {
                children.push(
                    div()
                        .my(px(4.0))
                        .mx(px(QUICK_ACCESS_ROW_PADDING_X))
                        .h(px(1.0))
                        .bg(hsla(p.separator()))
                        .into_any_element(),
                );
            }
            if !group.heading.is_empty() {
                children.push(
                    div()
                        .w_full()
                        .flex_shrink_0()
                        .min_h(px(QUICK_ACCESS_GROUP_HEADING_HEIGHT))
                        .px(px(QUICK_ACCESS_ROW_PADDING_X))
                        .py(px(5.0))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .line_height(px(16.0))
                        .text_color(hsla(p.muted))
                        .child(SharedString::from(group.heading.clone()))
                        .into_any_element(),
                );
            }
            for row in &group.rows {
                let is_selected = row.key() == selected;
                let is_hovered = hovered.as_deref() == Some(row.key());
                children.push(quick_access_row(
                    &p,
                    row,
                    row_index,
                    is_selected,
                    is_hovered,
                    &callbacks,
                    cx,
                ));
                row_index += 1;
            }
        }
        if children.is_empty() {
            children.push(
                div()
                    .w_full()
                    .py(px(24.0))
                    .px(px(12.0))
                    .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
                    .text_color(hsla(p.muted))
                    .text_center()
                    .child(SharedString::from(if snapshot.loading {
                        snapshot.loading_label.clone()
                    } else {
                        snapshot.empty.clone()
                    }))
                    .into_any_element(),
            );
        }
        v_flex()
            .id("quick-access-list")
            .flex_1()
            .min_h_0()
            .w_full()
            .p(px(QUICK_ACCESS_LIST_PADDING))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(|this, _, _window, cx| {
                /* The runtime widens the visible history window and pages gxserver; the
                window only has to say that the reader reached the end of what it has. */
                let offset = this.scroll.offset().y;
                let max = this.scroll.max_offset().y;
                if max <= gpui::px(0.0) || (max + offset) > gpui::px(96.0) {
                    return;
                }
                // The React list throttled its reveal to one step per 150ms.
                let now = web_time::Instant::now();
                if this
                    .last_load_more
                    .is_some_and(|last| now.duration_since(last).as_millis() < 150)
                {
                    return;
                }
                this.last_load_more = Some(now);
                this.post(json!({ "type": "loadMore" }), cx);
            }))
            .children(children)
            .into_any_element()
    }

    /// `.ghostex-stashed-prompts-stash-hint`: the 28px info pill in the corner.
    fn render_stash_hint(
        &self,
        p: &QuickAccessPalette,
        hint: &str,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *p;
        let hint = hint.to_string();
        div()
            .id("quick-access-stash-hint")
            .absolute()
            .right(px(10.0))
            .bottom(px(10.0))
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(hsla(gpui::Rgba {
                a: 0.18,
                ..p.foreground
            }))
            .bg(hsla(p.raised))
            .text_color(hsla(p.muted))
            .hover(move |this| this.bg(hsla(p.raised_hover)))
            .tooltip(move |window, cx| quick_access_tooltip(hint.clone(), window, cx))
            .child(
                svg()
                    .path(super::chrome::asset_icon_path("info-circle"))
                    .size(px(16.0))
                    .text_color(hsla(p.muted)),
            )
            .into_any_element()
    }
}

impl Focusable for GpuiQuickAccessWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Anchors a popover to a control whose bounds were captured during prepaint.
pub(crate) fn anchor_below(bounds: gpui::Bounds<gpui::Pixels>) -> Point<gpui::Pixels> {
    point(
        bounds.origin.x,
        bounds.origin.y + bounds.size.height + px(6.0),
    )
}

impl super::editor::EditorHost for GpuiQuickAccessWindow {
    fn post_editor(&mut self, command: serde_json::Value, cx: &mut Context<Self>) {
        self.post(command, cx);
        cx.notify();
    }

    fn toggle_editor_project_menu(&mut self, cx: &mut Context<Self>) {
        self.editor_tag_menu.close();
        self.editor_project_menu.toggle();
        cx.notify();
    }

    fn toggle_editor_tag_menu(&mut self, cx: &mut Context<Self>) {
        self.editor_project_menu.close();
        self.editor_tag_menu.toggle();
        cx.notify();
    }
}
