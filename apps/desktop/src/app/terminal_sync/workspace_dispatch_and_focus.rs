// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: terminal search bar/button rendering, workspace terminal/session runtime-action dispatch, focused-session helpers, and Agents/command ghostty surface focus handoff.

use std::collections::HashSet;

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
                #[cfg(target_os = "macos")]
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
                #[cfg(not(target_os = "macos"))]
                let _ = (this, event, window, cx);
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
                                    #[cfg(target_os = "macos")]
                                    let _ = this.perform_terminal_search_binding_action(
                                        runtime_session_id,
                                        "navigate_search:previous",
                                        cx,
                                    );
                                    #[cfg(not(target_os = "macos"))]
                                    let _ = (this, cx);
                                },
                                cx,
                            ))
                            .child(self.render_terminal_search_button(
                                format!("ghostex-gpui-terminal-search-next-{element_id_suffix}"),
                                "↓",
                                FIND_BAR_NAV_BUTTON_WIDTH,
                                move |this, _window, cx| {
                                    #[cfg(target_os = "macos")]
                                    let _ = this.perform_terminal_search_binding_action(
                                        runtime_session_id,
                                        "navigate_search:next",
                                        cx,
                                    );
                                    #[cfg(not(target_os = "macos"))]
                                    let _ = (this, cx);
                                },
                                cx,
                            ))
                            .child(self.render_terminal_search_button(
                                format!("ghostex-gpui-terminal-search-close-{element_id_suffix}"),
                                "✕",
                                FIND_BAR_CLOSE_BUTTON_WIDTH,
                                move |this, window, cx| {
                                    #[cfg(target_os = "macos")]
                                    this.close_terminal_search(runtime_session_id, window, cx);
                                    #[cfg(not(target_os = "macos"))]
                                    let _ = (this, window, cx);
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

    /// Fork/Reload for Agents terminals follow macOS ownership: Rust resolves
    /// the mapped gxserver identity and the sidebar runtime commits the
    /// gxserver mutation (`/api/forkSession` or the sleep→wake full reload).
    /// Unmapped local placeholder tabs have no gxserver identity and no-op.
    pub(crate) fn dispatch_gpui_workspace_terminal_runtime_action(
        &mut self,
        action: &str,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == shell_session_id).then(|| key.clone()))
        else {
            return false;
        };
        self.dispatch_gpui_workspace_session_key_runtime_action(action, &key, cx)
    }

    /// Same sidebar-runtime lifecycle route addressed by gxserver identity, for
    /// sessions that exist in the daemon presentation without a mounted pane in
    /// this window.
    pub(crate) fn dispatch_gpui_workspace_session_key_runtime_action(
        &mut self,
        action: &str,
        key: &GpuiLocalWorkspaceSessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "action": action,
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_runtime_action_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn focused_agents_workspace_shell_session_id(&self) -> Option<TerminalSessionId> {
        if self.active_mode != TitlebarMode::Agents {
            return None;
        }
        let ShellFocusTarget::AgentsPane(pane_id) = self.shell_focus else {
            return None;
        };
        self.agents_workspace
            .find_leaf(pane_id)
            .map(|leaf| leaf.tab_group.active_tab)
    }

    /// The Agents workspace session behind the focused terminal, whether it is
    /// focused in the Agents view or in a project-editor companion pane (top
    /// or bottom split). Companion panes display Agents workspace sessions, so
    /// both resolve into the same shell-session id space; session-scoped
    /// agent actions (Rename, Delayed Send, Close After Done, Prompts, the
    /// overlay cluster) treat the two focus surfaces identically.
    pub(crate) fn focused_agents_or_companion_shell_session_id(&self) -> Option<TerminalSessionId> {
        if let Some(session_id) = self.focused_agents_workspace_shell_session_id() {
            return Some(session_id);
        }
        match self.focused_terminal_text_mount_target() {
            Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id)) => {
                Some(slot_id.session_id)
            }
            _ => None,
        }
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

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_agents_terminal_ghostty_surface_focus(&mut self) {
        self.sync_agents_terminal_ghostty_surface_focus_with_appkit_handoff(false);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_agents_terminal_ghostty_surface_focus_with_appkit_handoff(
        &mut self,
        force_terminal_appkit_focus_handoff: bool,
    ) {
        /*
        CDXC:GPUTerminalGhosttySurfaceFocus 2026-06-22-22:59:
        Apply focus only to already-mounted real Agents Ghostty surfaces. The focused surface is the current shell-focused visible Running Agents mount slot; all other mounted slots receive unfocused state. The shared Ghostty app focus means only "GPUI currently has a focused terminal surface" and deliberately does not infer NSApp activation or route keyboard/mouse input.

        CDXC:GPUTerminalAppKitFocus 2026-06-22-23:11:
        The same focused mounted Agents slot must also hand first responder to the exact App-owned host NSView that backs its real Ghostty surface. Store only runtime slot plus host identity to avoid repeated `makeFirstResponder` calls during render sync, but allow terminal-body clicks to force one handoff even when shell focus already points at the same slot.
        */
        let mounted_slot_ids = self
            .agents_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let focus_states = agents_terminal_surface_focus_states_for_slots(
            self.active_mode,
            self.shell_focus,
            &self.agents_workspace,
            &mounted_slot_ids,
        );
        let app_has_focused_terminal_surface =
            focus_states.iter().any(|(_slot_id, focused)| *focused);
        let focused_mounted_slot_id = focus_states
            .iter()
            .find_map(|(slot_id, focused)| (*focused).then_some(*slot_id));

        for (slot_id, focused) in focus_states {
            if let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) {
                surface.set_focus(focused);
            }
        }

        if let Some(app) = self.agents_terminal_ghostty_app.as_mut() {
            app.set_focus(app_has_focused_terminal_surface);
        }

        let next_appkit_focus_identity = focused_mounted_slot_id.and_then(|slot_id| {
            if !self.agents_terminal_ghostty_surfaces.contains_key(&slot_id) {
                return None;
            }
            terminal_native_view::app_owned_terminal_host_focus_identity(
                self.agents_terminal_host_native_views.get(&slot_id),
            )
        });
        if terminal_native_view::app_owned_terminal_host_focus_should_execute(
            self.agents_terminal_appkit_focused_host,
            next_appkit_focus_identity,
            force_terminal_appkit_focus_handoff,
        ) {
            self.agents_terminal_appkit_focused_host =
                next_appkit_focus_identity.and_then(|focus_identity| {
                    terminal_native_view::focus_app_owned_terminal_host_native_view(
                        self.agents_terminal_host_native_views
                            .get(&focus_identity.slot_id()),
                    )
                });
        } else {
            self.agents_terminal_appkit_focused_host = next_appkit_focus_identity;
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn drop_command_terminal_ghostty_surface_before_host_detach(
        &mut self,
        commands: &[terminal_surface_host::NativeTerminalSurfaceHostCommand<
            CommandTerminalBodyMountSlotId,
        >],
    ) {
        for command in commands {
            let terminal_surface_host::NativeTerminalSurfaceHostCommand::HideAndDetach { plan } =
                *command
            else {
                continue;
            };

            if park_command_terminal_runtime_owner_before_host_detach(
                &self.command_pane,
                &mut self.command_terminal_parked_runtime_owners,
                &mut self.command_terminal_host_native_views,
                &mut self.command_terminal_ghostty_surfaces,
                plan,
            ) {
                continue;
            }
            self.command_terminal_ghostty_surfaces.remove(&plan.slot_id);
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.command_terminal_host_native_views.get(&plan.slot_id),
                false,
            );
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_command_terminal_ghostty_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        let host_slot_ids = self
            .command_terminal_host_native_views
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let request_slot_ids = self
            .command_terminal_ghostty_surface_config_requests
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let stale_surface_slot_ids = self
            .command_terminal_ghostty_surfaces
            .keys()
            .copied()
            .filter(|slot_id| {
                !host_slot_ids.contains(slot_id) || !request_slot_ids.contains(slot_id)
            })
            .collect::<Vec<_>>();

        for slot_id in stale_surface_slot_ids {
            self.command_terminal_ghostty_surfaces.remove(&slot_id);
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.command_terminal_host_native_views.get(&slot_id),
                false,
            );
        }

        self.sync_command_terminal_ghostty_surface_focus();

        if self.command_terminal_host_native_views.is_empty() {
            return;
        }

        if self.command_terminal_ghostty_app.is_none() {
            let Ok(app) = terminal_ghostty_surface::GhosttyAppOwner::new() else {
                self.command_terminal_ghostty_surfaces.clear();
                for host_view in self.command_terminal_host_native_views.values() {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        Some(host_view),
                        false,
                    );
                }
                return;
            };
            self.command_terminal_ghostty_app = Some(app);
        }

        let host_plans = self
            .command_terminal_host_native_views
            .iter()
            .map(|(slot_id, host_view)| (*slot_id, host_view.attachment_plan()))
            .collect::<Vec<_>>();

        for (slot_id, plan) in host_plans {
            let runtime_session_id = command_terminal_runtime_session_id(slot_id);
            let Some(request) = self
                .command_terminal_ghostty_surface_config_requests
                .get(&slot_id)
                .cloned()
            else {
                self.command_terminal_ghostty_surfaces.remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.command_terminal_host_native_views.get(&slot_id),
                    false,
                );
                continue;
            };

            if self
                .command_terminal_ghostty_surfaces
                .get(&slot_id)
                .is_some_and(|surface| {
                    surface.mount_slot_id() != slot_id
                        || surface.runtime_session_id() != runtime_session_id
                })
            {
                self.command_terminal_ghostty_surfaces.remove(&slot_id);
            }

            if !self
                .command_terminal_ghostty_surfaces
                .contains_key(&slot_id)
            {
                let Some(app) = self.command_terminal_ghostty_app.as_ref() else {
                    return;
                };
                let Ok(surface) = terminal_ghostty_surface::GhosttySurfaceOwner::new(
                    app,
                    slot_id,
                    runtime_session_id,
                    &request,
                ) else {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        self.command_terminal_host_native_views.get(&slot_id),
                        false,
                    );
                    continue;
                };
                self.command_terminal_ghostty_surfaces
                    .insert(slot_id, surface);
            }

            let update_failed = self
                .command_terminal_ghostty_surfaces
                .get_mut(&slot_id)
                .is_some_and(|surface| {
                    surface
                        .update_content_scale_and_size(plan.bounds, request.scale_factor())
                        .is_err()
                });
            if update_failed {
                self.command_terminal_ghostty_surfaces.remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.command_terminal_host_native_views.get(&slot_id),
                    false,
                );
                continue;
            }

            let surface_mounted = self
                .command_terminal_ghostty_surfaces
                .contains_key(&slot_id);
            if surface_mounted {
                if let (Some(host_view), Some(surface)) = (
                    self.command_terminal_host_native_views.get(&slot_id),
                    self.command_terminal_ghostty_surfaces.get(&slot_id),
                ) {
                    /*
                    CDXC:GPUITerminalNativeKeyBridge 2026-06-24-20:58:
                    Command-pane terminals use the same exact host-view key bridge as Agents terminals, but registration is scoped to command mount slots and their own Ghostty app/surface map so command keys cannot fall through to Agents surfaces or shell placeholders.
                    */
                    terminal_ghostty_surface::register_native_key_target(
                        host_view.native_view_handle(),
                        surface,
                    );
                }
            }
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.command_terminal_host_native_views.get(&slot_id),
                surface_mounted,
            );
        }

        self.sync_command_terminal_ghostty_surface_focus();

        if let Some(app) = self.command_terminal_ghostty_app.as_ref() {
            if !self.command_terminal_ghostty_surfaces.is_empty() {
                app.tick_if_woken();
                app.tick();
                self.drain_command_terminal_runtime_clipboard_requests(cx);
            };
        }
    }
}
