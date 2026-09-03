// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: hover-link checks, GPUI-engine and native terminal search actions, and the queued-prompts chip plus per-surface search bar renderers.

use std::collections::HashMap;

use gpui::AnyElement;
use gpui::App;
use gpui::AppContext as _;
use gpui::Entity;
use gpui::Focusable as _;
use gpui::InteractiveElement as _;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::px;
use gpui_component::h_flex;
use gpui_component::input::InputEvent;
use gpui_component::input::InputState;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    #[cfg(target_os = "macos")]
    pub(crate) fn agents_terminal_slot_hovers_link(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.agents_terminal_ghostty_surfaces
            .get(&slot_id)
            .map(|surface| surface.runtime_session_id())
            .and_then(|runtime_session_id| {
                self.agents_terminal_runtime_osc_states
                    .get(&runtime_session_id)
            })
            .is_some_and(|state| state.hovered_link_url.is_some())
    }

    // Hover state rides the native Ghostty surface map, which only exists on
    // macOS; without native surfaces no slot can report a hovered link (the
    // GPUI engine draws its own hover underline inside the element).
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn agents_terminal_slot_hovers_link(
        &self,
        _slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        false
    }

    pub(crate) fn command_terminal_slot_hovers_link(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        self.command_terminal_runtime_osc_states
            .get(&command_terminal_runtime_session_id(slot_id))
            .is_some_and(|state| state.hovered_link_url.is_some())
    }

    /// Cmd+F on a focused terminal surface triggers Ghostty's own
    /// `start_search` keybind action, matching the macOS surface-level key
    /// equivalent. The search bar itself opens when Ghostty answers with a
    /// START_SEARCH runtime action.
    /// Cmd+F on a focused GPUI-engine terminal opens the same search bar the
    /// native path uses, driving the element's viewport find instead of
    /// Ghostty binding actions.
    pub(crate) fn start_search_in_focused_gpui_engine_terminal(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let target = match focused_terminal_text_target(self.active_mode, self.shell_focus) {
            Some(target) => target,
            None => return false,
        };
        let (record, osc_states) = match target {
            FocusedTerminalTextTarget::Agents => {
                let Some(slot_id) = focused_agents_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    &self.agents_workspace,
                ) else {
                    return false;
                };
                let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id)
                else {
                    return false;
                };
                (record, &mut self.agents_terminal_runtime_osc_states)
            }
            FocusedTerminalTextTarget::Command => {
                let Some(slot_id) = focused_command_terminal_surface_mount_slot(
                    self.shell_focus,
                    &self.command_pane,
                ) else {
                    return false;
                };
                let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id)
                else {
                    return false;
                };
                (record, &mut self.command_terminal_runtime_osc_states)
            }
            FocusedTerminalTextTarget::ProjectEditorCompanion => {
                let Some(slot_id) = focused_project_editor_companion_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    self.project_editor_companion_focused_terminal_session_id(),
                ) else {
                    return false;
                };
                let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id)
                else {
                    return false;
                };
                (record, &mut self.agents_terminal_runtime_osc_states)
            }
        };
        let runtime_session_id = record.runtime_session_id;
        let view = record.view.clone();
        let state = osc_states.entry(runtime_session_id).or_default();
        if state.search.is_none() {
            state.search = Some(GpuiTerminalSearchState::default());
        }
        view.update(cx, |view, cx| view.set_search_needle("", cx));
        self.terminal_search_focus_pending = Some(runtime_session_id);
        cx.notify();
        true
    }

    /// Drive a GPUI-engine terminal's find from the shared search-bar action
    /// vocabulary (`search:<needle>`, `navigate_search:*`, `end_search`).
    pub(crate) fn perform_gpui_engine_terminal_search_action(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        action: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<bool> {
        let record = self.gpui_engine_record_for_runtime_session_id(runtime_session_id)?;
        let view = record.view.clone();
        Some(match action {
            "navigate_search:next" => {
                view.update(cx, |view, cx| view.navigate_search(true, cx));
                true
            }
            "navigate_search:previous" => {
                view.update(cx, |view, cx| view.navigate_search(false, cx));
                true
            }
            "end_search" => {
                view.update(cx, |view, cx| view.clear_search(cx));
                true
            }
            _ => {
                if let Some(needle) = action.strip_prefix("search:") {
                    let needle = needle.to_string();
                    view.update(cx, |view, cx| view.set_search_needle(&needle, cx));
                    true
                } else {
                    false
                }
            }
        })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn start_search_in_focused_terminal_surface(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.start_search_in_focused_gpui_engine_terminal(cx) {
            return true;
        }
        let started = match focused_terminal_text_target(self.active_mode, self.shell_focus) {
            Some(FocusedTerminalTextTarget::Agents) => focused_agents_terminal_surface_mount_slot(
                self.active_mode,
                self.shell_focus,
                &self.agents_workspace,
            )
            .and_then(|slot_id| self.agents_terminal_ghostty_surfaces.get(&slot_id))
            .is_some_and(|surface| surface.perform_binding_action("start_search")),
            Some(FocusedTerminalTextTarget::Command) => {
                focused_command_terminal_surface_mount_slot(self.shell_focus, &self.command_pane)
                    .and_then(|slot_id| self.command_terminal_ghostty_surfaces.get(&slot_id))
                    .is_some_and(|surface| surface.perform_binding_action("start_search"))
            }
            Some(FocusedTerminalTextTarget::ProjectEditorCompanion) => {
                focused_project_editor_companion_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    self.project_editor_companion_focused_terminal_session_id(),
                )
                .and_then(|slot_id| {
                    self.project_editor_companion_terminal_ghostty_surfaces
                        .get(&slot_id)
                })
                .is_some_and(|surface| surface.perform_binding_action("start_search"))
            }
            None => false,
        };
        if started {
            cx.notify();
        }
        started
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn start_search_in_focused_terminal_surface(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.start_search_in_focused_gpui_engine_terminal(cx)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn perform_terminal_search_binding_action(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        action: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if let Some(handled) =
            self.perform_gpui_engine_terminal_search_action(runtime_session_id, action, cx)
        {
            return handled;
        }
        if let Some(surface) = self
            .agents_terminal_ghostty_surfaces
            .values()
            .find(|surface| surface.runtime_session_id() == runtime_session_id)
        {
            return surface.perform_binding_action(action);
        }
        if let Some(surface) = self
            .command_terminal_ghostty_surfaces
            .values()
            .find(|surface| surface.runtime_session_id() == runtime_session_id)
        {
            return surface.perform_binding_action(action);
        }
        if let Some(surface) = self
            .project_editor_companion_terminal_ghostty_surfaces
            .values()
            .find(|surface| surface.runtime_session_id() == runtime_session_id)
        {
            return surface.perform_binding_action(action);
        }
        false
    }

    /// Typing in the GPUI search bar mirrors macOS ownership: the local search
    /// state's needle is the source of truth updated from the field, then the
    /// needle is pushed into Ghostty via the `search:<needle>` keybind action.
    pub(crate) fn update_terminal_search_needle(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        needle: &str,
    ) -> bool {
        for osc_states in [
            &mut self.agents_terminal_runtime_osc_states,
            &mut self.command_terminal_runtime_osc_states,
        ] {
            if let Some(search) = osc_states
                .get_mut(&runtime_session_id)
                .and_then(|state| state.search.as_mut())
            {
                search.needle = needle.to_string();
                return true;
            }
        }
        false
    }

    pub(crate) fn handle_terminal_search_input_event(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        input: &Entity<InputState>,
        event: &InputEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        match event {
            InputEvent::Change => {
                let needle = input.read(cx).value().to_string();
                if self.update_terminal_search_needle(runtime_session_id, &needle) {
                    let _ = self.perform_terminal_search_binding_action(
                        runtime_session_id,
                        &format!("search:{needle}"),
                        cx,
                    );
                }
            }
            InputEvent::PressEnter { shift, .. } => {
                let action = if *shift {
                    "navigate_search:previous"
                } else {
                    "navigate_search:next"
                };
                let _ = self.perform_terminal_search_binding_action(runtime_session_id, action, cx);
            }
            InputEvent::Focus | InputEvent::Blur => {}
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (runtime_session_id, input, event, cx);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn close_terminal_search(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = self.perform_terminal_search_binding_action(runtime_session_id, "end_search", cx);
        let mut closed = false;
        for osc_states in [
            &mut self.agents_terminal_runtime_osc_states,
            &mut self.command_terminal_runtime_osc_states,
        ] {
            if let Some(state) = osc_states.get_mut(&runtime_session_id) {
                closed |= state.search.take().is_some();
            }
        }
        if !closed {
            return;
        }
        let companion_slot_id = self
            .project_editor_companion_terminal_ghostty_surfaces
            .iter()
            .find_map(|(slot_id, surface)| {
                (surface.runtime_session_id() == runtime_session_id).then_some(*slot_id)
            })
            .or_else(|| {
                self.current_project_editor_companion_terminal_body_mount_slots()
                    .into_iter()
                    .find(|slot_id| {
                        self.agents_gpui_engine_terminals
                            .get(&slot_id.session_id)
                            .is_some_and(|record| record.runtime_session_id == runtime_session_id)
                    })
            });
        if let Some(slot_id) = companion_slot_id {
            self.focus_project_editor_companion_terminal_session(
                slot_id.mode,
                slot_id.session_id,
                window,
                cx,
            );
        } else if let Some(slot_id) = self
            .agents_terminal_ghostty_surfaces
            .iter()
            .find_map(|(slot_id, surface)| {
                (surface.runtime_session_id() == runtime_session_id).then_some(*slot_id)
            })
            .or_else(|| {
                self.agents_gpui_engine_terminals
                    .iter()
                    .find(|(_, record)| record.runtime_session_id == runtime_session_id)
                    .and_then(|(session_id, _)| {
                        let pane_id = self.agents_workspace.pane_id_for_session(*session_id)?;
                        Some(AgentsTerminalBodyMountSlotId {
                            pane_id,
                            session_id: *session_id,
                        })
                    })
            })
        {
            self.focus_agents_terminal_mount_slot(slot_id, window, cx);
        } else if let Some(slot_id) = self
            .command_terminal_ghostty_surfaces
            .iter()
            .find_map(|(slot_id, surface)| {
                (surface.runtime_session_id() == runtime_session_id).then_some(*slot_id)
            })
            .or_else(|| {
                self.command_gpui_engine_terminals
                    .iter()
                    .find(|(_, record)| record.runtime_session_id == runtime_session_id)
                    .and_then(|(session_id, _)| {
                        self.command_pane
                            .flat_tab_ids()
                            .into_iter()
                            .find(|(_, tab_session_id)| tab_session_id == session_id)
                            .map(|(group_id, session_id)| CommandTerminalBodyMountSlotId {
                                group_id,
                                session_id,
                            })
                    })
            })
        {
            self.focus_command_terminal_mount_slot(slot_id, window, cx);
        }
        cx.notify();
    }

    /// Whether any live terminal search input currently holds GPUI keyboard
    /// focus. Shell focus stays on the terminal pane while the search bar is
    /// open, so terminal key routing that is derived from shell focus must
    /// consult this before treating a keystroke as terminal input.
    pub(crate) fn terminal_search_input_owns_keyboard_focus(
        &self,
        window: &Window,
        cx: &App,
    ) -> bool {
        self.terminal_search_inputs
            .values()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Keeps one live search input per terminal with an active Ghostty search
    /// state, mirrors Ghostty-provided needles into the field, and applies a
    /// pending open-focus so Cmd+F immediately types into the bar like macOS.
    pub(crate) fn sync_terminal_search_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut active_needles: HashMap<AgentsTerminalRuntimeSessionId, String> = HashMap::new();
        for (runtime_session_id, state) in self
            .agents_terminal_runtime_osc_states
            .iter()
            .chain(self.command_terminal_runtime_osc_states.iter())
        {
            if let Some(search) = &state.search {
                active_needles.insert(*runtime_session_id, search.needle.clone());
            }
        }
        self.terminal_search_inputs
            .retain(|runtime_session_id, _| active_needles.contains_key(runtime_session_id));
        self.terminal_search_input_subscriptions
            .retain(|runtime_session_id, _| active_needles.contains_key(runtime_session_id));
        for (runtime_session_id, needle) in active_needles {
            let input = match self.terminal_search_inputs.get(&runtime_session_id) {
                Some(input) => input.clone(),
                None => {
                    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
                    let subscription = cx.subscribe(
                        &input,
                        move |this: &mut Self, input, event: &InputEvent, cx| {
                            this.handle_terminal_search_input_event(
                                runtime_session_id,
                                &input,
                                event,
                                cx,
                            );
                        },
                    );
                    self.terminal_search_inputs
                        .insert(runtime_session_id, input.clone());
                    self.terminal_search_input_subscriptions
                        .insert(runtime_session_id, subscription);
                    input
                }
            };
            if input.read(cx).value().as_ref() != needle {
                input.update(cx, |input, cx| input.set_value(needle, window, cx));
            }
        }
        if let Some(runtime_session_id) = self.terminal_search_focus_pending.take() {
            if let Some(input) = self
                .terminal_search_inputs
                .get(&runtime_session_id)
                .cloned()
            {
                #[cfg(target_os = "macos")]
                self.begin_programmatic_focus();
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                cef::focus_gpui_root_view(self.parent_ns_view);
                input.update(cx, |input, cx| input.focus(window, cx));
                #[cfg(target_os = "macos")]
                self.end_programmatic_focus();
            }
        }
    }

    /*
    CDXC:SessionChat 2026-08-21:
    The terminal view's "Queued: N" control is the leading item of the pane's
    own tab bar — existing chrome, a normal sibling frame, drawn beside the tab
    strip rather than over anything.

    It deliberately does NOT float over the terminal the way the web host's does:
    GPUI cannot paint above a mounted Ghostty/CEF body, and solving that with a
    transparent overlay or hit-test routing is exactly what this repo's native
    layout discipline forbids. A chrome ROW between the tab bar and the body (the
    search bar's slot) was the other candidate and was rejected: it would resize
    the Ghostty surface — reflowing the user's scrollback — every time a queue
    filled or drained. The tab bar has a fixed height, so appearing and
    disappearing here cannot touch terminal geometry at all.
    */
    pub(crate) fn render_agents_terminal_queued_prompts_chip(
        &self,
        leaf: &WorkspaceLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let session_id = leaf.tab_group.active_session_id()?;
        // Chat owns this pane's body when it is on and renders the queue rows
        // itself, so the terminal chip has nothing to add.
        if self.agents_chat_mode_sessions.contains(&session_id) {
            return None;
        }
        let counts = self
            .session_chat_queued_counts
            .get(&session_id)
            .copied()
            .unwrap_or_default();
        if counts.total == 0 {
            return None;
        }
        let count = counts.total;
        /*
        CDXC:SessionChat 2026-08-21-b:
        A `failed` row holds the whole queue until the user retries or deletes
        it, so the chip's dot turns the sidebar's error red instead of the
        waiting yellow. Only the dot's colour changes — every box property below
        is identical either way, so the chip cannot resize the tab bar and
        therefore cannot touch the Ghostty surface's geometry.
        */
        let dot_color = if counts.failed > 0 {
            terminal_queued_prompts_failed_dot_color()
        } else {
            terminal_queued_prompts_dot_color()
        };
        let element_id_suffix = format!("agents-{}-{}", leaf.pane_id.0, session_id.0);
        Some(
            h_flex()
                .id(format!(
                    "ghostex-gpui-terminal-queued-prompts-{element_id_suffix}"
                ))
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .ml(px(6.0))
                .mr(px(2.0))
                .h(px(TERMINAL_QUEUED_PROMPTS_CHIP_HEIGHT))
                .px(px(7.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(terminal_queued_prompts_border_color())
                .bg(terminal_queued_prompts_background_color())
                .text_size(px(11.0))
                .text_color(terminal_queued_prompts_text_color())
                .cursor_default()
                .hover(|this| this.bg(terminal_queued_prompts_hover_color()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.handoff_agents_session_chat_mode(session_id, cx);
                    }),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(dot_color),
                )
                .child(div().child(format!("Queued: {count}")))
                .into_any_element(),
        )
    }

    pub(crate) fn render_agents_terminal_search_bar(
        &self,
        leaf: &WorkspaceLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let slot_id = self
            .agents_workspace
            .terminal_body_mount_candidate(leaf)
            .mount_slot_id()?;
        let runtime_session_id = self.agents_terminal_search_bar_runtime_session_id(slot_id)?;
        let search = self
            .agents_terminal_runtime_osc_states
            .get(&runtime_session_id)?
            .search
            .clone()?;
        Some(self.render_terminal_search_bar(
            runtime_session_id,
            &search,
            format!("agents-{}-{}", slot_id.pane_id.0, slot_id.session_id.0),
            cx,
        ))
    }

    pub(crate) fn agents_terminal_search_bar_runtime_session_id(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        if let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id) {
            return Some(record.runtime_session_id);
        }
        #[cfg(target_os = "macos")]
        {
            let surface = self.agents_terminal_ghostty_surfaces.get(&slot_id)?;
            return (surface.mount_slot_id() == slot_id).then(|| surface.runtime_session_id());
        }
        #[cfg(not(target_os = "macos"))]
        None
    }

    pub(crate) fn render_command_terminal_search_bar(
        &self,
        leaf: &CommandPaneLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let session_id = leaf.tab_group.active_session_id()?;
        let slot_id = CommandTerminalBodyMountSlotId {
            group_id: leaf.group_id,
            session_id,
        };
        let runtime_session_id = self.command_terminal_search_bar_runtime_session_id(slot_id)?;
        let search = self
            .command_terminal_runtime_osc_states
            .get(&runtime_session_id)?
            .search
            .clone()?;
        Some(self.render_terminal_search_bar(
            runtime_session_id,
            &search,
            format!("command-{}-{}", slot_id.group_id.0, slot_id.session_id.0),
            cx,
        ))
    }

    pub(crate) fn command_terminal_search_bar_runtime_session_id(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        if let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id) {
            return Some(record.runtime_session_id);
        }
        #[cfg(target_os = "macos")]
        {
            let surface = self.command_terminal_ghostty_surfaces.get(&slot_id)?;
            return (surface.mount_slot_id() == slot_id).then(|| surface.runtime_session_id());
        }
        #[cfg(not(target_os = "macos"))]
        None
    }

    pub(crate) fn render_project_editor_companion_terminal_search_bar(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let runtime_session_id =
            self.project_editor_companion_terminal_search_bar_runtime_session_id(slot_id)?;
        let search = self
            .agents_terminal_runtime_osc_states
            .get(&runtime_session_id)?
            .search
            .clone()?;
        Some(self.render_terminal_search_bar(
            runtime_session_id,
            &search,
            format!(
                "companion-{}-{}",
                slot_id.mode.element_slug(),
                slot_id.session_id.0
            ),
            cx,
        ))
    }

    pub(crate) fn project_editor_companion_terminal_search_bar_runtime_session_id(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        if let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id) {
            return Some(record.runtime_session_id);
        }
        #[cfg(target_os = "macos")]
        {
            let surface = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get(&slot_id)?;
            return (surface.mount_slot_id() == slot_id).then(|| surface.runtime_session_id());
        }
        #[cfg(not(target_os = "macos"))]
        None
    }
}
