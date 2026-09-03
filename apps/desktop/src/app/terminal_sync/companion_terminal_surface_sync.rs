// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: Agents terminal runtime clipboard drain plus project-editor-companion terminal ghostty surface sync, drain, and focus handoff.

use std::collections::HashSet;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    #[cfg(target_os = "macos")]
    pub(crate) fn drain_agents_terminal_runtime_clipboard_requests(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Clipboard 2026-06-23-19:07:
        Agents runtime clipboard handoff is authorized by exact mounted surface ownership, not focus. The drain snapshots current Agents mount keys, re-gets the still-mounted owner on the app thread, enables only standard clipboard for owner-local queued Ghostty requests, reads only explicit string entries, and writes only runtime-provided text without logging, persistence, selection clipboard support, or fallback requester inference.

        CDXC:Clipboard 2026-06-27-10:28:
        Runtime Ghostty clipboard reads share the direct-paste previewable-image normalization so menu/Cmd paste and runtime clipboard requests convert the same validated image inputs while preserving the disabled setting as explicit-string-only.
        */
        let paste_previewable_images_enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        let slot_ids = self
            .agents_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();

        let mut runtime_osc_state_changed = false;
        let mut bell_shell_session_ids = Vec::new();
        let mut terminal_link_requests = Vec::new();
        for slot_id in slot_ids {
            let Some(surface) = self.agents_terminal_ghostty_surfaces.get(&slot_id) else {
                continue;
            };
            surface.drain_runtime_clipboard_requests(
                true,
                || {
                    terminal_runtime_clipboard_read_text(
                        || cx.read_from_clipboard(),
                        paste_previewable_images_enabled,
                    )
                },
                |text| {
                    terminal_runtime_clipboard_write_standard_text(text, |item| {
                        cx.write_to_clipboard(item);
                    });
                },
            );
            let action_events = surface.drain_runtime_action_events();
            if !action_events.is_empty() {
                let runtime_session_id = surface.runtime_session_id();
                terminal_link_requests.extend(action_events.iter().filter_map(|event| {
                    let terminal_ghostty_surface::GhosttyRuntimeActionEvent::OpenUrl { url } =
                        event
                    else {
                        return None;
                    };
                    Some((runtime_session_id, url.clone()))
                }));
                if action_events.iter().any(|event| {
                    matches!(
                        event,
                        terminal_ghostty_surface::GhosttyRuntimeActionEvent::RingBell
                    )
                }) {
                    bell_shell_session_ids.push(slot_id.session_id);
                }
                if action_events.iter().any(|event| {
                    matches!(
                        event,
                        terminal_ghostty_surface::GhosttyRuntimeActionEvent::StartSearch { .. }
                    )
                }) {
                    self.terminal_search_focus_pending = Some(surface.runtime_session_id());
                }
                runtime_osc_state_changed |= apply_gpui_terminal_runtime_action_events(
                    &mut self.agents_terminal_runtime_osc_states,
                    runtime_session_id,
                    action_events,
                );
            }
        }
        for (runtime_session_id, url) in terminal_link_requests {
            let working_directory = self
                .agents_terminal_runtime_osc_states
                .get(&runtime_session_id)
                .and_then(|state| state.pwd.clone());
            self.open_gpui_engine_terminal_action_url(&url, working_directory.as_deref(), cx);
        }
        for shell_session_id in bell_shell_session_ids {
            self.dispatch_gpui_workspace_terminal_bell(shell_session_id, cx);
        }
        if !self.agents_terminal_runtime_osc_states.is_empty() {
            let live_runtime_session_ids = self
                .agents_terminal_ghostty_surfaces
                .values()
                .map(|surface| surface.runtime_session_id())
                .chain(
                    self.project_editor_companion_terminal_ghostty_surfaces
                        .values()
                        .map(|surface| surface.runtime_session_id()),
                )
                .chain(
                    self.agents_gpui_engine_terminals
                        .values()
                        .map(|record| record.runtime_session_id),
                )
                .collect::<HashSet<_>>();
            self.agents_terminal_runtime_osc_states
                .retain(|runtime_session_id, _| {
                    live_runtime_session_ids.contains(runtime_session_id)
                });
        }
        if runtime_osc_state_changed {
            cx.notify();
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_project_editor_companion_terminal_ghostty_surfaces(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let host_slot_ids = self
            .project_editor_companion_terminal_host_native_views
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let request_slot_ids = self
            .project_editor_companion_terminal_ghostty_surface_config_requests
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let stale_surface_slot_ids = self
            .project_editor_companion_terminal_ghostty_surfaces
            .keys()
            .copied()
            .filter(|slot_id| {
                !host_slot_ids.contains(slot_id) || !request_slot_ids.contains(slot_id)
            })
            .collect::<Vec<_>>();

        for slot_id in stale_surface_slot_ids {
            self.project_editor_companion_terminal_ghostty_surfaces
                .remove(&slot_id);
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.project_editor_companion_terminal_host_native_views
                    .get(&slot_id),
                false,
            );
        }

        self.sync_project_editor_companion_terminal_ghostty_surface_focus();

        if self
            .project_editor_companion_terminal_host_native_views
            .is_empty()
        {
            return;
        }

        if self.agents_terminal_ghostty_app.is_none() {
            let Ok(app) = terminal_ghostty_surface::GhosttyAppOwner::new() else {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .clear();
                for host_view in self
                    .project_editor_companion_terminal_host_native_views
                    .values()
                {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        Some(host_view),
                        false,
                    );
                }
                return;
            };
            self.agents_terminal_ghostty_app = Some(app);
        }

        let host_plans = self
            .project_editor_companion_terminal_host_native_views
            .iter()
            .map(|(slot_id, host_view)| (*slot_id, host_view.attachment_plan()))
            .collect::<Vec<_>>();

        for (slot_id, plan) in host_plans {
            let Some(runtime_session_id) = self
                .agents_terminal_runtime_sessions
                .runtime_session_id_for_shell_session(slot_id.session_id)
            else {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.project_editor_companion_terminal_host_native_views
                        .get(&slot_id),
                    false,
                );
                continue;
            };
            let Some(request) = self
                .project_editor_companion_terminal_ghostty_surface_config_requests
                .get(&slot_id)
                .cloned()
            else {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.project_editor_companion_terminal_host_native_views
                        .get(&slot_id),
                    false,
                );
                continue;
            };

            if self
                .project_editor_companion_terminal_ghostty_surfaces
                .get(&slot_id)
                .is_some_and(|surface| {
                    surface.mount_slot_id() != slot_id
                        || surface.runtime_session_id() != runtime_session_id
                })
            {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .remove(&slot_id);
            }

            if !self
                .project_editor_companion_terminal_ghostty_surfaces
                .contains_key(&slot_id)
            {
                let Some(app) = self.agents_terminal_ghostty_app.as_ref() else {
                    return;
                };
                let Ok(surface) = terminal_ghostty_surface::GhosttySurfaceOwner::new(
                    app,
                    slot_id,
                    runtime_session_id,
                    &request,
                ) else {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        self.project_editor_companion_terminal_host_native_views
                            .get(&slot_id),
                        false,
                    );
                    continue;
                };
                self.project_editor_companion_terminal_ghostty_surfaces
                    .insert(slot_id, surface);
            }

            let update_failed = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get_mut(&slot_id)
                .is_some_and(|surface| {
                    surface
                        .update_content_scale_and_size(plan.bounds, request.scale_factor())
                        .is_err()
                });
            if update_failed {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.project_editor_companion_terminal_host_native_views
                        .get(&slot_id),
                    false,
                );
                continue;
            }

            let surface_mounted = self
                .project_editor_companion_terminal_ghostty_surfaces
                .contains_key(&slot_id);
            if surface_mounted
                && let (Some(host_view), Some(surface)) = (
                    self.project_editor_companion_terminal_host_native_views
                        .get(&slot_id),
                    self.project_editor_companion_terminal_ghostty_surfaces
                        .get(&slot_id),
                )
            {
                terminal_ghostty_surface::register_native_key_target(
                    host_view.native_view_handle(),
                    surface,
                );
            }
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.project_editor_companion_terminal_host_native_views
                    .get(&slot_id),
                surface_mounted,
            );
        }

        self.sync_project_editor_companion_terminal_ghostty_surface_focus();

        if let Some(app) = self.agents_terminal_ghostty_app.as_ref()
            && !self
                .project_editor_companion_terminal_ghostty_surfaces
                .is_empty()
        {
            app.tick_if_woken();
            app.tick();
            self.drain_project_editor_companion_terminal_runtime_requests(cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn drain_project_editor_companion_terminal_runtime_requests(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let paste_previewable_images_enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        let slot_ids = self
            .project_editor_companion_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut runtime_osc_state_changed = false;
        let mut terminal_link_requests = Vec::new();
        for slot_id in slot_ids {
            let Some(surface) = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get(&slot_id)
            else {
                continue;
            };
            surface.drain_runtime_clipboard_requests(
                true,
                || {
                    terminal_runtime_clipboard_read_text(
                        || cx.read_from_clipboard(),
                        paste_previewable_images_enabled,
                    )
                },
                |text| {
                    terminal_runtime_clipboard_write_standard_text(text, |item| {
                        cx.write_to_clipboard(item);
                    });
                },
            );
            let action_events = surface.drain_runtime_action_events();
            if action_events.is_empty() {
                continue;
            }
            let runtime_session_id = surface.runtime_session_id();
            terminal_link_requests.extend(action_events.iter().filter_map(|event| {
                let terminal_ghostty_surface::GhosttyRuntimeActionEvent::OpenUrl { url } = event
                else {
                    return None;
                };
                Some((runtime_session_id, url.clone()))
            }));
            if action_events.iter().any(|event| {
                matches!(
                    event,
                    terminal_ghostty_surface::GhosttyRuntimeActionEvent::StartSearch { .. }
                )
            }) {
                self.terminal_search_focus_pending = Some(surface.runtime_session_id());
            }
            runtime_osc_state_changed |= apply_gpui_terminal_runtime_action_events(
                &mut self.agents_terminal_runtime_osc_states,
                runtime_session_id,
                action_events,
            );
        }
        for (runtime_session_id, url) in terminal_link_requests {
            let working_directory = self
                .agents_terminal_runtime_osc_states
                .get(&runtime_session_id)
                .and_then(|state| state.pwd.clone());
            self.open_gpui_engine_terminal_action_url(&url, working_directory.as_deref(), cx);
        }
        if runtime_osc_state_changed {
            cx.notify();
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_project_editor_companion_terminal_ghostty_surface_focus(&mut self) {
        self.sync_project_editor_companion_terminal_ghostty_surface_focus_with_appkit_handoff(
            false,
        );
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_project_editor_companion_terminal_ghostty_surface_focus_with_appkit_handoff(
        &mut self,
        force_terminal_appkit_focus_handoff: bool,
    ) {
        let mounted_slot_ids = self
            .project_editor_companion_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let focused_slot_id = focused_project_editor_companion_terminal_surface_mount_slot(
            self.active_mode,
            self.shell_focus,
            self.project_editor_companion_focused_terminal_session_id(),
        )
        .filter(|slot_id| mounted_slot_ids.contains(slot_id));
        let app_has_focused_terminal_surface = focused_slot_id.is_some();

        for slot_id in mounted_slot_ids {
            if let Some(surface) = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get_mut(&slot_id)
            {
                surface.set_focus(Some(slot_id) == focused_slot_id);
            }
        }

        if let Some(app) = self.agents_terminal_ghostty_app.as_mut() {
            app.set_focus(app_has_focused_terminal_surface);
        }

        let next_appkit_focus_identity = focused_slot_id.and_then(|slot_id| {
            if !self
                .project_editor_companion_terminal_ghostty_surfaces
                .contains_key(&slot_id)
            {
                return None;
            }
            terminal_native_view::app_owned_terminal_host_focus_identity(
                self.project_editor_companion_terminal_host_native_views
                    .get(&slot_id),
            )
        });
        if terminal_native_view::app_owned_terminal_host_focus_should_execute(
            self.project_editor_companion_terminal_appkit_focused_host,
            next_appkit_focus_identity,
            force_terminal_appkit_focus_handoff,
        ) {
            self.project_editor_companion_terminal_appkit_focused_host = next_appkit_focus_identity
                .and_then(|focus_identity| {
                    terminal_native_view::focus_app_owned_terminal_host_native_view(
                        self.project_editor_companion_terminal_host_native_views
                            .get(&focus_identity.slot_id()),
                    )
                });
        } else {
            self.project_editor_companion_terminal_appkit_focused_host = next_appkit_focus_identity;
        }
    }
}
