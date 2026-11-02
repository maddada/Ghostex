// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: terminal search bar/button rendering, workspace terminal/session runtime-action dispatch, focused-session helpers, and Agents/command ghostty surface focus handoff.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::KeyDownEvent;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::h_flex;
use gpui_component::input::Input;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// The GPUI search bar is a normal-layout chrome row above the terminal
    /// body (same discipline as the close-confirm banner) because GPUI cannot
    /// float elements over the mounted AppKit Ghostty view. Contents are
    /// right-aligned to echo the macOS floating bar's top-right placement.
    pub(crate) fn render_terminal_search_bar(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        search: &GpuiTerminalSearchState,
        element_id_suffix: String,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let count_label = terminal_search_count_label(search);
        let input = self
            .terminal_search_inputs
            .get(&runtime_session_id)
            .cloned();

        h_flex()
            .id(format!(
                "ghostex-gpui-terminal-search-row-{element_id_suffix}"
            ))
            .flex_shrink_0()
            .w_full()
            .h(px(FIND_BAR_HEIGHT))
            .items_center()
            .justify_end()
            .pl(px(8.0))
            .bg(terminal_search_bar_row_color())
            .border_b_1()
            .border_color(terminal_search_bar_divider_color())
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        cx.stop_propagation();
                        this.close_terminal_search(runtime_session_id, window, cx);
                    }
                    "up" => {
                        cx.stop_propagation();
                        let _ = this.perform_terminal_search_binding_action(
                            runtime_session_id,
                            "navigate_search:previous",
                            cx,
                        );
                    }
                    "down" => {
                        cx.stop_propagation();
                        let _ = this.perform_terminal_search_binding_action(
                            runtime_session_id,
                            "navigate_search:next",
                            cx,
                        );
                    }
                    _ => {}
                }
            }))
            .child(
                h_flex()
                    .id(format!(
                        "ghostex-gpui-terminal-search-bar-{element_id_suffix}"
                    ))
                    .w(px(300.0))
                    .max_w_full()
                    .h_full()
                    .items_center()
                    .gap(px(4.0))
                    .border_l_1()
                    .border_color(terminal_search_bar_border_color())
                    .bg(terminal_search_bar_background_color())
                    .pl(px(9.0))
                    .when_some(input, |this, input| {
                        this.child(
                            div().flex_1().min_w_0().overflow_hidden().child(
                                Input::new(&input)
                                    .with_size(ComponentSize::XSmall)
                                    .appearance(false)
                                    .bordered(false)
                                    .focus_bordered(false)
                                    .w_full()
                                    .px(px(0.0))
                                    .py(px(0.0))
                                    .text_size(px(13.0))
                                    .text_color(terminal_search_bar_text_color()),
                            ),
                        )
                    })
                    .when(!count_label.is_empty(), |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.0))
                                .text_color(terminal_search_bar_count_color())
                                .child(count_label),
                        )
                    })
                    .child(
                        h_flex()
                            .h_full()
                            .flex_shrink_0()
                            .child(self.render_terminal_search_button(
                                format!("ghostex-gpui-terminal-search-prev-{element_id_suffix}"),
                                "↑",
                                FIND_BAR_NAV_BUTTON_WIDTH,
                                move |this, _window, cx| {
                                    let _ = this.perform_terminal_search_binding_action(
                                        runtime_session_id,
                                        "navigate_search:previous",
                                        cx,
                                    );
                                },
                                cx,
                            ))
                            .child(self.render_terminal_search_button(
                                format!("ghostex-gpui-terminal-search-next-{element_id_suffix}"),
                                "↓",
                                FIND_BAR_NAV_BUTTON_WIDTH,
                                move |this, _window, cx| {
                                    let _ = this.perform_terminal_search_binding_action(
                                        runtime_session_id,
                                        "navigate_search:next",
                                        cx,
                                    );
                                },
                                cx,
                            ))
                            .child(self.render_terminal_search_button(
                                format!("ghostex-gpui-terminal-search-close-{element_id_suffix}"),
                                "✕",
                                FIND_BAR_CLOSE_BUTTON_WIDTH,
                                move |this, window, cx| {
                                    this.close_terminal_search(runtime_session_id, window, cx);
                                },
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_terminal_search_button(
        &self,
        id: String,
        glyph: &'static str,
        width: f32,
        on_click: impl Fn(&mut Self, &mut Window, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(width))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(terminal_search_bar_border_color())
            .rounded(px(0.0))
            .text_size(px(12.0))
            .line_height(px(20.0))
            .text_color(terminal_search_bar_button_color())
            .bg(terminal_search_bar_button_background_color())
            .cursor_default()
            .hover(|this| this.bg(terminal_search_bar_button_hover_color()))
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
                    on_click(this, window, cx);
                }),
            )
            .child(div().child(glyph))
            .into_any_element()
    }

    /// Fork/Reload for Agents terminals follow macOS ownership: Rust resolves the mapped gxserver
    /// identity and runs the same store action a sidebar row runs (`/api/forkSession` or the
    /// sleep then wake full reload). Unmapped local placeholder tabs have no gxserver identity and
    /// no-op.
    pub(crate) fn dispatch_gpui_workspace_terminal_runtime_action(
        &mut self,
        action: &str,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session) = self.workspace_terminal_session_key(shell_session_id) else {
            return false;
        };
        if !session.machine.is_local() {
            return self.dispatch_gpui_remote_session_runtime_action(action, &session, cx);
        }
        let key = GpuiLocalWorkspaceSessionKey {
            project_id: session.project_id,
            session_id: session.session_id,
        };
        self.dispatch_gpui_workspace_session_key_runtime_action(action, &key, cx)
    }

    /// The gxserver session behind an Agents tab, on this computer or on a remote machine.
    pub(crate) fn workspace_terminal_session_key(
        &self,
        shell_session_id: TerminalSessionId,
    ) -> Option<ghostex_gx_core::SessionKey> {
        Some(
            match self.workspace_terminal_key_for_shell_session(shell_session_id)? {
                GpuiWorkspaceTerminalSessionKey::Local(key) => {
                    ghostex_gx_core::SessionKey::local(key.project_id, key.session_id)
                }
                GpuiWorkspaceTerminalSessionKey::Remote(key) => {
                    ghostex_gx_core::SessionKey::remote(
                        key.remote_machine_id,
                        key.project_id,
                        key.session_id,
                    )
                }
            },
        )
    }

    /// CDXC:RemoteMachines 2026-10-25 WHY:
    /// The chat's "…" menu and the terminal bar only looked a session up among this computer's tabs, so on a remote session Handoff / Export, Fork, Full Reload and Note did nothing while the sidebar row's menu worked. A remote session's action is the sidebar row's own command, sent through the sidebar's dispatch, so both menus run one path (the machine's tunnel, gx_store/sidebar_remote.rs; the export dialog, gx_store/git/export_transcript.rs).
    fn dispatch_gpui_remote_session_runtime_action(
        &mut self,
        action: &str,
        session: &ghostex_gx_core::SessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let session_id = session.to_sidebar_session_id();
        let command = match action {
            "exportTranscript" => serde_json::json!({
                "type": "command",
                "message": {"type": "exportSessionTranscript", "sessionId": session_id},
            }),
            "forkSession" | "fullReloadSession" | "closeSession" => serde_json::json!({
                "type": "command",
                "message": {"type": action, "sessionId": session_id},
            }),
            "close" => serde_json::json!({
                "type": "command",
                "message": {"type": "closeSession", "sessionId": session_id},
            }),
            "sleepSession" => serde_json::json!({
                "type": "command",
                "message": {"type": "setSessionSleeping", "sessionId": session_id, "sleeping": true},
            }),
            "openSessionNote" => serde_json::json!({
                "type": "sessionAction",
                "sessionId": session_id,
                "action": "note",
            }),
            "rename" => serde_json::json!({
                "type": "sessionAction",
                "sessionId": session_id,
                "action": "rename",
            }),
            _ => return false,
        };
        self.dispatch_native_sidebar_ui(command, cx);
        true
    }

    /// Same store lifecycle route addressed by gxserver identity, for sessions that exist in the
    /// daemon presentation without a mounted pane in this window.
    pub(crate) fn dispatch_gpui_workspace_session_key_runtime_action(
        &mut self,
        action: &str,
        key: &GpuiLocalWorkspaceSessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.dispatch_gpui_workspace_session_key_runtime_action_with_agent(action, key, None, cx)
    }

    /// CDXC:AgentProviders 2026-09-03: Switch Account from the terminal action
    /// bar or the chat composer, addressed like Fork / Full reload but carrying
    /// the picked agent id, which goes to `/api/switchSessionAgent` before the
    /// Full reload.
    pub(crate) fn dispatch_gpui_workspace_terminal_switch_account(
        &mut self,
        shell_session_id: TerminalSessionId,
        agent_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == shell_session_id).then(|| key.clone()))
        else {
            return false;
        };
        self.dispatch_gpui_workspace_session_key_runtime_action_with_agent(
            "switchSessionAgent",
            &key,
            Some(agent_id),
            cx,
        )
    }

    /// The chat model picker chose another agent's model. It takes the Handoff / Export route, carrying the target so the dialog opens with that agent selected.
    /// SEE-ALSO: apps/desktop/src/app/gx_store/git/export_transcript.rs (`git_export_session_transcript`).
    pub(crate) fn dispatch_gpui_workspace_terminal_handoff_to_model(
        &mut self,
        shell_session_id: TerminalSessionId,
        provider: &str,
        model: &str,
        effort: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // The picked model must name a model family and a model (`normalizeGpuiWorkspaceTerminalRuntimeAction`).
        let provider_known = matches!(
            provider,
            "claude" | "codex" | "cursor" | "grok" | "antigravity" | "opencode"
        );
        if !provider_known || model.trim().is_empty() {
            return false;
        }
        let Some(session) = self.workspace_terminal_session_key(shell_session_id) else {
            return false;
        };
        self.git_export_session_transcript(
            &session.to_sidebar_session_id(),
            Some(crate::app::gx_store::git::HandoffTarget {
                provider: provider.to_string(),
                model: model.trim().to_string(),
                effort: (!effort.trim().is_empty()).then(|| effort.to_string()),
            }),
            cx,
        );
        true
    }

    fn dispatch_gpui_workspace_session_key_runtime_action_with_agent(
        &mut self,
        action: &str,
        key: &GpuiLocalWorkspaceSessionKey,
        agent_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // Close, Sleep, Fork, Full Reload, Note and Switch Account are the store's own session
        // actions (gx_store/terminal_lifecycle/runtime_actions.rs).
        if let Some(performed) =
            self.gx_store_run_workspace_runtime_action(action, key, agent_id, cx)
        {
            return performed;
        }
        // Handoff / Export opens its dialog in Rust (gx_store/git/export_transcript.rs). Nothing
        // else reaches the old runtime from here any more.
        if action != "exportTranscript" {
            return false;
        }
        let session =
            ghostex_gx_core::SessionKey::local(key.project_id.as_str(), key.session_id.as_str());
        self.git_export_session_transcript(&session.to_sidebar_session_id(), None, cx);
        true
    }

    pub(crate) fn focused_agents_workspace_shell_session_id(&self) -> Option<TerminalSessionId> {
        if !self.agents_workspace_visible() {
            return None;
        }
        let ShellFocusTarget::AgentsPane(pane_id) = self.shell_focus else {
            return None;
        };
        self.agents_workspace
            .find_leaf(pane_id)
            .map(|leaf| leaf.tab_group.active_tab)
    }

    /// The Agents workspace session behind the focused terminal. Session-scoped agent actions
    /// (Rename, Delayed Send, Close After Done, Prompts, the overlay cluster) all resolve through it.
    pub(crate) fn focused_agents_or_companion_shell_session_id(&self) -> Option<TerminalSessionId> {
        self.focused_agents_workspace_shell_session_id()
    }

    /// The sidebar projection is the title authority for Agents tabs. It owns
    /// placeholder, generated, user, and terminal-title trust rules; using a
    /// second live OSC source here lets WSL's `user@host: /path` shell title
    /// hide title generation that the sidebar has already projected.
    pub(crate) fn agents_workspace_tab_display_title(
        &self,
        session_id: TerminalSessionId,
    ) -> String {
        self.agents_workspace
            .session(session_id)
            .map(|session| session.title.clone())
            .unwrap_or_else(|| "Terminal Session".to_string())
    }
}
