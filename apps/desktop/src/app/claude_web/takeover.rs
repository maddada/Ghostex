//! Turning Claude on and off: Claude takes the sessions column's place and the panels around it
//! step aside, then come back.
//!
//! CDXC:Extensions 2026-10-28 DECISION:
//! User: "when i toggle claude on then please make it take over the whole area (so the sidebar is hidden basically in this case). no need to show a tab for it on the right sidepanel. i should still be able to open stuff on the right sidebar like browser etc while claude is open (but by default it hides right sidepanel and left sidebar and bottom commands pane when i toggle it on pls". Claude is never a side panel tab: its page is drawn where the sessions column is (claude_web/column.rs), turning it on hides the sidebar, the side panel and the Commands pane, and the side panel, the sidebar and the Commands pane still open beside it with their usual toggles and shortcuts. Opening the Claude view any other way (the Open a view picker, a saved tab from an older build) turns Claude on. This supersedes the 2026-10-25 rule that Claude's page kept the sidebar shown and opened as an expanded side panel view (claude_web/sidebar_button.rs, packages/settings-catalog/src/data/official_extensions.rs).
//! User (Q727): the Commands pane is hidden to its thin collapsed strip, as built. User (Q728): "Yes, any opened session turns Claude off": opening a session any way while Claude is on (Quick Access, a notification, a new agent, a hotkey, a link, the phone or the CLI switching to it) turns Claude off the way a sidebar click does, with the layout put back by `ClaudeLayout::after_claude` (`OpenedSession`).
//! User (2026-10-29): "please when i click on claude and see claude, then i click on the button to show sidebar then we should disable claude and go back to how the app was before i clicked on claude basically. consider claude a new space+project for example so it's like switching away from the space+project im in and then switching back to it if u get me". While Claude is on, showing the sidebar (the header's Toggle sidebar button, Cmd+B, the palette and `ghostex` toggle-sidebar) turns Claude off and puts back the layout from before Claude and the same Space, project and session in front (`ClaudeTakeover::leave_by_showing_sidebar`, `ClaudePlace`). The side panel and Commands toggles still work beside Claude (the 2026-10-28 request to use the side panel beside it), and the left-edge hover reveal stays a peek that leaves Claude on.
//! User (2026-10-29, the header): "when we're on claude (when it's enabled) then we need to change this header (shouldn't say the name of the project and session i was in before) and i wanna show the buttons that are there above the sidebar as they are so i can click on the claude button in the same place it is to toggle claude (so these buttons should stay exactly in same spot even tho sidebar goes away)". While Claude is on the header names no project or session and keeps only the app's own controls (`HeaderControl::shown_with_claude`), and with the sidebar hidden it draws the sidebar's top row (Toggle sidebar, the Agents Panel toggle, Claude shown as on, Search, the bell, the menu) at exactly the sidebar's positions (claude_web/header.rs). Turning Claude off brings the header back as it was.
//! SEE-ALSO: render/workarea_header/shell.rs and render/workarea_header/action_buttons.rs (the header), native_sidebar/navigation.rs (`render_native_sidebar_row`), render/root.rs (the sidebar menu while the sidebar is hidden), render/workarea_split.rs and render/command_pane_structure.rs (the column), workarea/runtime_surfaces.rs, keyboard_owner.rs, focus/shell_focus.rs and tab_actions/focused_surface.rs (`view_page_on_screen`), workspace_reconcile/active_mode.rs (opening the view turns Claude on), gx_store/focus_perform.rs and gx_store/create/focus_created.rs (opened sessions), render/workarea_header/toggles.rs, render/root.rs, delayed_send/app_modal_sidebar_command/hotkey_action.rs and gx_store/renderer_commands/perform.rs (Toggle sidebar).

use gpui::Window;
use serde_json::{Value, json};

use super::place::WebPlace;
use crate::*;

/// The side panel as Claude found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClaudeSidePanel {
    /// The view it showed; `Agents` is the Open a view picker.
    pub(crate) mode: TitlebarMode,
    pub(crate) maximized: bool,
}

/// Which of the panels Claude hides are on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClaudeLayout {
    pub(crate) sidebar: bool,
    pub(crate) side_panel: Option<ClaudeSidePanel>,
    pub(crate) commands: bool,
}

impl ClaudeLayout {
    /// What turning Claude on leaves: Claude alone in the window.
    pub(crate) const CLAUDE_ALONE: Self = Self {
        sidebar: false,
        side_panel: None,
        commands: false,
    };

    /// What turning Claude off puts on screen, from the layout Claude found (`before`) and the one
    /// on screen now.
    ///
    /// CDXC:Extensions 2026-10-28 WHY:
    /// Every panel comes back as it was before Claude, except one that is on screen when Claude turns off, which stays as it is (a side panel keeps the view it shows now). The user opened it on purpose while Claude was on, so taking it away would undo what they just did; a panel that is hidden is indistinguishable from Claude's own hiding, which was only ever meant to last while Claude is on, so it goes back.
    pub(crate) fn after_claude(before: Self, now: Self) -> Self {
        Self {
            sidebar: before.sidebar || now.sidebar,
            side_panel: now.side_panel.or(before.side_panel),
            commands: before.commands || now.commands,
        }
    }

    fn to_shell_state_json(self) -> Value {
        json!({
            "sidebar": self.sidebar,
            "sidePanel": self.side_panel.map(|panel| json!({
                "mode": panel.mode.element_slug(),
                "maximized": panel.maximized,
            })),
            "commands": self.commands,
        })
    }

    fn from_shell_state_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let side_panel = match object.get("sidePanel") {
            Some(Value::Object(panel)) => Some(ClaudeSidePanel {
                mode: panel
                    .get("mode")
                    .and_then(Value::as_str)
                    .and_then(TitlebarMode::from_slug)
                    .filter(|mode| !mode.is_claude_web())?,
                maximized: panel
                    .get("maximized")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
            _ => None,
        };
        Some(Self {
            sidebar: object.get("sidebar")?.as_bool()?,
            side_panel,
            commands: object.get("commands")?.as_bool()?,
        })
    }
}

/// Where the user was when Claude came on: the sidebar's machine section and Space, and the
/// session (or, with none, the project) in front. Showing the sidebar goes back there, the way
/// switching back to a Space and project does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClaudePlace {
    /// The machine section the Space belongs to (`local`, `remote:<id>`).
    pub(crate) section: String,
    pub(crate) space: Option<String>,
    /// The sidebar session id of the session in front.
    pub(crate) session: Option<String>,
    /// The workspace project id in front.
    pub(crate) project: Option<String>,
}

impl ClaudePlace {
    /// The Space to select to get back here from `now`, when the sidebar still shows the same
    /// machine and another Space is selected.
    pub(crate) fn space_to_select(&self, now: &Self) -> Option<String> {
        self.space
            .clone()
            .filter(|space| self.section == now.section && now.space.as_ref() != Some(space))
    }

    /// The session to focus to get back here from `now` (read after the Space switch, which
    /// focuses a session of its own).
    pub(crate) fn session_to_focus(&self, now: &Self) -> Option<String> {
        self.session
            .clone()
            .filter(|session| now.session.as_ref() != Some(session))
    }

    /// With no session in front, the project to show.
    pub(crate) fn project_to_focus(&self, now: &Self) -> Option<String> {
        if self.session.is_some() {
            return None;
        }
        self.project
            .clone()
            .filter(|project| now.project.as_ref() != Some(project))
    }

    fn to_shell_state_json(&self) -> Value {
        json!({
            "section": self.section,
            "space": self.space,
            "session": self.session,
            "project": self.project,
        })
    }

    fn from_shell_state_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Self {
            section: text("section")?,
            space: text("space"),
            session: text("session"),
            project: text("project"),
        })
    }
}

/// Claude (or Grok, which shares it: claude_web/place.rs) is on: the window's layout and place from
/// before, to put back when it goes off, and which of them is on. Saved with the window's shell
/// state, so a relaunch comes back with it on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClaudeTakeover {
    before: ClaudeLayout,
    place: Option<ClaudePlace>,
    view: WebPlace,
}

impl ClaudeTakeover {
    /// Turning Claude on over `current` at `place`: the takeover to keep, and the layout to show.
    pub(crate) fn turn_on(current: ClaudeLayout, place: ClaudePlace) -> (Self, ClaudeLayout) {
        (
            Self {
                before: current,
                place: Some(place),
                view: WebPlace::Claude,
            },
            ClaudeLayout::CLAUDE_ALONE,
        )
    }

    /// Turning Claude off over `current` (the Claude button, an opened session): the layout to
    /// show.
    pub(crate) fn turn_off(&self, current: ClaudeLayout) -> ClaudeLayout {
        ClaudeLayout::after_claude(self.before, current)
    }

    /// Leaving Claude by showing the sidebar: the layout from before Claude exactly, as switching
    /// back to a Space finds it, with the sidebar the user just asked for. Where to go back to is
    /// `place`.
    pub(crate) fn leave_by_showing_sidebar(&self) -> ClaudeLayout {
        ClaudeLayout {
            sidebar: true,
            ..self.before
        }
    }

    pub(crate) fn place(&self) -> Option<&ClaudePlace> {
        self.place.as_ref()
    }

    /// Which page is on: Claude or Grok.
    pub(crate) fn view(&self) -> WebPlace {
        self.view
    }

    /// Switching from one page to the other keeps the layout and place from before either.
    pub(crate) fn set_view(&mut self, view: WebPlace) {
        self.view = view;
    }

    pub(crate) fn to_shell_state_json(&self) -> Value {
        let mut value = self.before.to_shell_state_json();
        value["place"] = self
            .place
            .as_ref()
            .map_or(Value::Null, ClaudePlace::to_shell_state_json);
        value["view"] = json!(self.view.slug());
        value
    }

    pub(crate) fn from_shell_state_json(value: &Value) -> Option<Self> {
        Some(Self {
            before: ClaudeLayout::from_shell_state_json(value)?,
            place: value
                .get("place")
                .and_then(ClaudePlace::from_shell_state_json),
            // Saved before Grok existed: Claude.
            view: value
                .get("view")
                .and_then(Value::as_str)
                .and_then(WebPlace::from_slug)
                .unwrap_or(WebPlace::Claude),
        })
    }
}

/// How a session came to the front of this window, for whether it turns Claude off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenedSession {
    /// The store focused an existing session (`gx_store_focus_session_row`): a sidebar click, a
    /// notification, Quick Access or the palette, the menu bar, a session hotkey, Back and
    /// Forward, `ghostex focus` from the CLI or the phone, the voice agent, a project activation.
    /// `keeps_view` is set by the Space, workspace window and machine switches that put back the
    /// session they had, and by a sidebar click into another project, which turns Claude off
    /// itself (native_sidebar/sessions.rs).
    Focused { keeps_view: bool },
    /// This window created a session and opened it (`gx_store_focus_created_session`): a new agent
    /// or terminal, Find's resume and fork, a board start, a Linear
    /// ticket or feedback agent.
    Created,
}

impl OpenedSession {
    /// CDXC:Extensions 2026-10-28 WHY:
    /// A switch of Space, workspace window or machine reselects the session it had in the background so the place comes back as it was; it is not the user opening a session, so Claude stays on through it.
    pub(crate) fn turns_claude_off(self) -> bool {
        match self {
            Self::Focused { keeps_view } => !keeps_view,
            Self::Created => true,
        }
    }
}

impl GhostexGpuiApp {
    /// A session came to the front: Claude goes off when this kind of opening turns it off
    /// (Q728 above), before the session is shown, so it lands in the layout Claude put back.
    pub(crate) fn note_opened_session(
        &mut self,
        opened: OpenedSession,
        cx: &mut gpui::Context<Self>,
    ) {
        if opened.turns_claude_off() {
            self.turn_claude_off(cx);
        }
    }

    /// Claude or Grok is on (claude_web/place.rs): the takeover below holds for both.
    pub(crate) fn claude_on(&self) -> bool {
        self.claude_takeover.is_some()
    }

    /// Which of Claude and Grok is on.
    pub(crate) fn web_place_on(&self) -> Option<WebPlace> {
        self.claude_takeover.as_ref().map(ClaudeTakeover::view)
    }

    /// Claude's (or Grok's) page is on screen: it is on and no expanded side panel covers its
    /// column.
    pub(crate) fn claude_page_on_screen(&self) -> bool {
        self.claude_on() && !self.view_panel_maximized()
    }

    /// The page on screen in the sessions column's place, if any.
    pub(crate) fn web_place_page_on_screen(&self) -> Option<WebPlace> {
        self.web_place_on().filter(|_| self.claude_page_on_screen())
    }

    /// Whether a view's page is on screen: the side panel's view, or Claude or Grok in the
    /// sessions column's place. Visibility, keyboard and focus ask this instead of comparing
    /// `active_mode`, which only ever names the side panel's view.
    pub(crate) fn view_page_on_screen(&self, mode: TitlebarMode) -> bool {
        self.active_mode == mode
            || WebPlace::from_mode(mode)
                .is_some_and(|place| self.web_place_page_on_screen() == Some(place))
    }

    fn claude_place_now(&self) -> ClaudePlace {
        let focus = self.gx_store.core.focus();
        ClaudePlace {
            section: self.gx_store.sidebar_ui.state().section_key(),
            space: self
                .gx_store
                .sidebar_list
                .view()
                .spaces
                .iter()
                .find(|space| space.selected)
                .map(|space| space.id.clone()),
            session: focus
                .focused_session
                .as_ref()
                .map(|session| session.to_sidebar_session_id()),
            project: focus
                .active_project
                .as_ref()
                .map(|project| project.to_workspace_project_id()),
        }
    }

    fn claude_layout_now(&self) -> ClaudeLayout {
        ClaudeLayout {
            sidebar: !self.sidebar_collapsed,
            side_panel: self.view_panel_open().then_some(ClaudeSidePanel {
                mode: self.active_mode,
                maximized: self.view_panel_maximized,
            }),
            commands: self.command_pane.is_expanded(),
        }
    }

    /// Shows `layout`'s panels. A side panel view that is no longer open or available stays closed.
    fn apply_claude_layout(&mut self, layout: ClaudeLayout, cx: &mut gpui::Context<Self>) {
        match layout.side_panel {
            None if self.view_panel_open() => self.close_view_panel(cx),
            None => {}
            Some(panel) => {
                if !self.view_panel_open() {
                    // Set first, so the panel opens expanded instead of expanding after.
                    self.view_panel_maximized = panel.maximized;
                    if panel.mode == TitlebarMode::Agents {
                        self.open_view_picker(cx);
                    } else if self.open_views.contains(&panel.mode) {
                        self.activate_mode(panel.mode, cx);
                    }
                    if !self.view_panel_open() {
                        self.view_panel_maximized = false;
                    }
                }
                if self.view_panel_open() && self.view_panel_maximized != panel.maximized {
                    self.toggle_view_panel_maximized(cx);
                }
            }
        }
        // After the side panel: a sidebar remembered per view moves with the view.
        if self.sidebar_collapsed == layout.sidebar {
            self.toggle_gpui_sidebar_collapsed(cx);
        }
        if self.command_pane.is_expanded() != layout.commands {
            self.command_pane_auto_minimize.idle_since = None;
            if layout.commands {
                self.command_pane.expand();
            } else {
                self.clear_command_pane_keep_open();
                self.command_pane.collapse();
                self.clear_command_resize_hover_state();
            }
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        }
    }

    /// The Claude (or Grok) button: turns it on, or off when it is on. With the other one on, it
    /// switches to this one in place.
    pub(crate) fn toggle_web_place(
        &mut self,
        place: WebPlace,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.web_place_on() == Some(place) {
            self.turn_claude_off(cx);
            self.sync_browser_address_input_if_shown(window, cx);
        } else if !self.show_web_place(place, cx) {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                &format!("{} could not open", place.title()),
                "Open a project in this window first.",
                cx,
            );
        }
    }

    /// Turns `place` on, or with it on wakes and focuses its page. With the other page on, switches
    /// to this one and keeps the layout from before either. Returns whether it is on.
    pub(crate) fn show_web_place(&mut self, place: WebPlace, cx: &mut gpui::Context<Self>) -> bool {
        let id = place.view_id();
        let mode = TitlebarMode::Extension(id);
        if !place.enabled() || !self.titlebar_mode_available(mode) {
            return false;
        }
        if let Some(takeover) = self
            .claude_takeover
            .as_mut()
            .filter(|takeover| takeover.view() != place)
        {
            takeover.set_view(place);
            self.open_views.retain(|tab| *tab != mode);
        }
        if self.claude_takeover.is_none() {
            // A Claude tab saved by an older build leaves the side panel, and so does the Open a
            // view picker, which Claude may just have been picked from: neither comes back after.
            if self.active_mode == mode || self.view_picker_open() {
                self.close_view_panel(cx);
            }
            self.open_views.retain(|tab| *tab != mode);
            let (mut takeover, alone) =
                ClaudeTakeover::turn_on(self.claude_layout_now(), self.claude_place_now());
            takeover.set_view(place);
            self.claude_takeover = Some(takeover);
            self.apply_claude_layout(alone, cx);
        } else if self.view_picker_open() {
            // Picked in the side panel's Open a view picker: the panel steps aside for Claude.
            self.close_view_panel(cx);
        } else if self.view_panel_maximized() {
            // An expanded side panel covers Claude's column.
            self.toggle_view_panel_maximized(cx);
        }
        self.mark_project_editor_mode_awake(mode, cx);
        self.ensure_extension_view_runtime_for_current_context(id, cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        self.focus_shell_target(ShellFocusTarget::ProjectEditorSurface(mode), cx);
        // The sessions column just left the screen, so its terminals and chats hide.
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
        true
    }

    /// Turns Claude off and puts the layout back (`ClaudeLayout::after_claude`). Its page stays
    /// loaded until the awake cap puts it to sleep, so turning Claude on again shows it at once.
    /// Returns whether Claude was on.
    pub(crate) fn turn_claude_off(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let Some(takeover) = self.claude_takeover.take() else {
            return false;
        };
        let after = takeover.turn_off(self.claude_layout_now());
        self.apply_claude_layout(after, cx);
        self.finish_turning_claude_off(cx);
        true
    }

    fn finish_turning_claude_off(&mut self, cx: &mut gpui::Context<Self>) {
        self.focus_default_surface_for_active_mode(cx);
        self.reconcile_agents_pane_surfaces(cx);
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// Toggle sidebar as the user asks for it (the header button, Cmd+B, the palette and
    /// `ghostex` toggle-sidebar): with Claude on and the sidebar hidden, showing it leaves Claude
    /// for where the user was before (the 2026-10-29 decision above). The left-edge peek never comes here.
    pub(crate) fn toggle_sidebar_from_user(&mut self, cx: &mut gpui::Context<Self>) {
        if self.claude_on() && self.sidebar_collapsed {
            self.leave_claude_by_showing_sidebar(cx);
        } else {
            self.toggle_gpui_sidebar_collapsed(cx);
        }
    }

    /// Turns Claude off and goes back to the layout, Space, project and session from before it.
    fn leave_claude_by_showing_sidebar(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(takeover) = self.claude_takeover.take() else {
            return;
        };
        self.apply_claude_layout(takeover.leave_by_showing_sidebar(), cx);
        if let Some(place) = takeover.place() {
            if let Some(space) = place.space_to_select(&self.claude_place_now()) {
                // The plain Space switch, without the row's slide: it reopens the Space's own
                // remembered session at once, which the session below then replaces.
                self.dispatch_native_sidebar_ui(
                    json!({"type": "selectSpace", "spaceId": space}),
                    cx,
                );
            }
            let now = self.claude_place_now();
            if let Some(session) = place.session_to_focus(&now) {
                self.gx_store_focus_activated_session(&session, cx);
            } else if let Some(project) = place.project_to_focus(&now) {
                self.gx_store_focus_project(&project, cx);
            }
        }
        self.finish_turning_claude_off(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROWSER: ClaudeSidePanel = ClaudeSidePanel {
        mode: TitlebarMode::Browser,
        maximized: false,
    };
    const KANBAN: ClaudeSidePanel = ClaudeSidePanel {
        mode: TitlebarMode::Kanban,
        maximized: true,
    };
    const EVERYTHING: ClaudeLayout = ClaudeLayout {
        sidebar: true,
        side_panel: Some(KANBAN),
        commands: true,
    };

    fn place() -> ClaudePlace {
        ClaudePlace {
            section: "local".into(),
            space: Some("space-work".into()),
            session: Some("P1:S7".into()),
            project: Some("P1".into()),
        }
    }

    #[test]
    fn showing_the_sidebar_brings_back_the_layout_from_before_claude_exactly() {
        let (takeover, _) = ClaudeTakeover::turn_on(EVERYTHING, place());
        // The Browser opened beside Claude does not stay: the layout is the one from before, as
        // switching back to a Space finds it.
        assert_eq!(takeover.leave_by_showing_sidebar(), EVERYTHING);
        let hidden_sidebar = ClaudeLayout {
            sidebar: false,
            side_panel: None,
            commands: false,
        };
        let (takeover, _) = ClaudeTakeover::turn_on(hidden_sidebar, place());
        // The sidebar the user just asked for shows even if it was hidden before Claude.
        assert_eq!(
            takeover.leave_by_showing_sidebar(),
            ClaudeLayout {
                sidebar: true,
                ..hidden_sidebar
            }
        );
    }

    #[test]
    fn showing_the_sidebar_goes_back_to_the_space_and_session_from_before() {
        let before = place();
        // While Claude was on, the peek switched to another Space and its session.
        let now = ClaudePlace {
            space: Some("space-home".into()),
            session: Some("P2:S3".into()),
            project: Some("P2".into()),
            ..place()
        };
        assert_eq!(before.space_to_select(&now), Some("space-work".into()));
        assert_eq!(before.session_to_focus(&now), Some("P1:S7".into()));
        assert_eq!(before.project_to_focus(&now), None);
    }

    #[test]
    fn showing_the_sidebar_where_nothing_moved_switches_nothing() {
        assert_eq!(place().space_to_select(&place()), None);
        assert_eq!(place().session_to_focus(&place()), None);
        assert_eq!(place().project_to_focus(&place()), None);
    }

    #[test]
    fn a_project_with_no_session_in_front_comes_back_as_the_project() {
        let before = ClaudePlace {
            session: None,
            ..place()
        };
        let now = ClaudePlace {
            session: Some("P2:S3".into()),
            project: Some("P2".into()),
            ..place()
        };
        assert_eq!(before.session_to_focus(&now), None);
        assert_eq!(before.project_to_focus(&now), Some("P1".into()));
    }

    #[test]
    fn another_machine_in_the_sidebar_keeps_its_space() {
        let now = ClaudePlace {
            section: "remote:box".into(),
            space: Some("space-remote".into()),
            ..place()
        };
        assert_eq!(place().space_to_select(&now), None);
    }

    #[test]
    fn turning_claude_on_hides_the_sidebar_the_side_panel_and_the_commands_pane() {
        let (_, shown) = ClaudeTakeover::turn_on(EVERYTHING, place());
        assert_eq!(shown, ClaudeLayout::CLAUDE_ALONE);
        assert!(!shown.sidebar && shown.side_panel.is_none() && !shown.commands);
    }

    #[test]
    fn turning_claude_off_brings_back_the_layout_from_before() {
        let (takeover, shown) = ClaudeTakeover::turn_on(EVERYTHING, place());
        assert_eq!(takeover.turn_off(shown), EVERYTHING);

        let bare = ClaudeLayout {
            sidebar: false,
            side_panel: None,
            commands: false,
        };
        let (takeover, shown) = ClaudeTakeover::turn_on(bare, place());
        assert_eq!(takeover.turn_off(shown), bare);
    }

    #[test]
    fn a_panel_opened_while_claude_is_on_stays_open_when_it_turns_off() {
        let before = ClaudeLayout {
            sidebar: true,
            side_panel: None,
            commands: false,
        };
        let (takeover, shown) = ClaudeTakeover::turn_on(before, place());
        // The user opens the Browser in the side panel and the Commands pane beside Claude.
        let now = ClaudeLayout {
            side_panel: Some(BROWSER),
            commands: true,
            ..shown
        };
        assert_eq!(
            takeover.turn_off(now),
            ClaudeLayout {
                sidebar: true,
                side_panel: Some(BROWSER),
                commands: true,
            }
        );
    }

    #[test]
    fn the_side_panel_opened_while_claude_is_on_keeps_its_view_over_the_one_from_before() {
        let (takeover, shown) = ClaudeTakeover::turn_on(EVERYTHING, place());
        let now = ClaudeLayout {
            side_panel: Some(BROWSER),
            ..shown
        };
        assert_eq!(takeover.turn_off(now).side_panel, Some(BROWSER));
    }

    #[test]
    fn a_panel_opened_and_closed_again_while_claude_is_on_goes_back_to_how_it_was() {
        let (takeover, shown) = ClaudeTakeover::turn_on(EVERYTHING, place());
        // Opened and closed again: hidden, as Claude left it.
        assert_eq!(takeover.turn_off(shown), EVERYTHING);
    }

    #[test]
    fn turning_claude_on_and_off_again_starts_from_the_layout_the_last_turn_left() {
        let before = ClaudeLayout {
            sidebar: true,
            side_panel: None,
            commands: false,
        };
        let (takeover, shown) = ClaudeTakeover::turn_on(before, place());
        let after_first = takeover.turn_off(ClaudeLayout {
            side_panel: Some(BROWSER),
            ..shown
        });

        let (takeover, shown) = ClaudeTakeover::turn_on(after_first, place());
        assert_eq!(shown, ClaudeLayout::CLAUDE_ALONE);
        assert_eq!(takeover.turn_off(shown), after_first);
        assert_eq!(after_first.side_panel, Some(BROWSER));
    }

    #[test]
    fn a_session_this_window_creates_turns_claude_off() {
        // A new agent or terminal, Find's resume and fork, a board start.
        assert!(OpenedSession::Created.turns_claude_off());
    }

    #[test]
    fn a_session_opened_while_claude_is_on_lands_in_the_layout_from_before() {
        let (takeover, shown) = ClaudeTakeover::turn_on(EVERYTHING, place());
        assert!(OpenedSession::Focused { keeps_view: false }.turns_claude_off());
        assert_eq!(takeover.turn_off(shown), EVERYTHING);
    }

    #[test]
    fn the_layout_from_before_survives_a_relaunch() {
        let (takeover, _) = ClaudeTakeover::turn_on(EVERYTHING, place());
        let saved = takeover.to_shell_state_json();
        assert_eq!(
            ClaudeTakeover::from_shell_state_json(&saved).and_then(|t| t.place().cloned()),
            Some(place())
        );
        assert_eq!(
            ClaudeTakeover::from_shell_state_json(&saved),
            Some(takeover)
        );

        let picker = ClaudeLayout {
            sidebar: false,
            side_panel: Some(ClaudeSidePanel {
                mode: TitlebarMode::Agents,
                maximized: false,
            }),
            commands: true,
        };
        let (takeover, _) = ClaudeTakeover::turn_on(picker, place());
        assert_eq!(
            ClaudeTakeover::from_shell_state_json(&takeover.to_shell_state_json()),
            Some(takeover)
        );
        assert_eq!(ClaudeTakeover::from_shell_state_json(&Value::Null), None);
    }
}
