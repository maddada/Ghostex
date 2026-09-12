use crate::*;

impl GhostexGpuiApp {
    /// CDXC:SessionChat 2026-09-12 DECISION:
    /// User: reuse persistent CEF chat pages across sessions and projects, preserving independent state and native actions.
    /// This supersedes per-session browser ownership; the three-page, five-minute unused budget and protected draft/work rules remain.
    pub(crate) fn expire_reusable_chat_renderers(&mut self) {
        self.reusable_chat_renderers
            .retain(|(_, _, _, hidden_since)| {
                hidden_since.elapsed() < GPUI_AGENTS_CHAT_SURFACE_HIDDEN_EVICT_AFTER
            });
        let excess = self
            .reusable_chat_renderers
            .len()
            .saturating_sub(GPUI_AGENTS_CHAT_SURFACE_HIDDEN_MAX);
        self.reusable_chat_renderers.drain(..excess);
    }

    /// CDXC:SessionChat 2026-09-12 WHY:
    /// A reused page that fails before composerReady stays hidden and cannot pass the release probe.
    /// Retire only that binding after ten seconds, preserve the requested chat mode and pending drafts, and rebuild once through fresh page initialization instead of cycling pooled pages indefinitely.
    pub(crate) fn watch_session_chat_activation(
        &self,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(10))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.recover_session_chat_activation(generation, cx)
            });
        })
        .detach();
    }

    fn recover_session_chat_activation(&mut self, generation: u64, cx: &mut gpui::Context<Self>) {
        let active_id = self
            .agents_chat_page_states
            .iter()
            .find(|(_, state)| state.generation == generation && state.awaiting_activation)
            .map(|(id, _)| *id);
        let surface = if let Some(id) = active_id {
            self.record_session_chat_lifecycle(
                id,
                "sessionChat.nativeActivationTimedOut",
                "freshRendererRetry",
            );
            let state = self.agents_chat_page_states.get_mut(&id).unwrap();
            *state = SessionChatPageState::new();
            state.force_fresh_renderer = true;
            self.session_chat_composer_ready_sessions.remove(&id);
            self.session_chat_composer_empty_reports.remove(&id);
            self.agents_chat_surface_hidden_since.remove(&id);
            self.agents_chat_surfaces.remove(&id)
        } else {
            self.parked_agents_chat_runtimes_by_project
                .values_mut()
                .find_map(|parked| {
                    let id = parked
                        .page_states
                        .iter()
                        .find(|(_, state)| {
                            state.generation == generation && state.awaiting_activation
                        })
                        .map(|(id, _)| *id)?;
                    let state = parked.page_states.get_mut(&id).unwrap();
                    *state = SessionChatPageState::new();
                    state.force_fresh_renderer = true;
                    parked.composer_ready_sessions.remove(&id);
                    parked.composer_empty_reports.remove(&id);
                    parked.surface_hidden_since.remove(&id);
                    parked.surfaces.remove(&id)
                })
        };
        if let Some(surface) = surface {
            surface.update(cx, |surface, _| surface.set_visible(false));
            drop(surface);
            if active_id.is_some() {
                self.reconcile_agents_chat_surfaces(cx);
                cx.notify();
            }
        }
    }

    pub(crate) fn begin_session_chat_native_request(
        &mut self,
        session_id: TerminalSessionId,
    ) -> Option<u64> {
        let state = self.agents_chat_page_states.get_mut(&session_id)?;
        state.pending_probe = None;
        state.pending_native_requests += 1;
        Some(state.generation)
    }

    pub(crate) fn dispatch_session_chat_generation_response(
        &mut self,
        generation: u64,
        callback: &str,
        payload: &serde_json::Value,
        finish_request: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_id = self
            .agents_chat_page_states
            .iter()
            .find(|(_, state)| state.generation == generation)
            .map(|(id, _)| *id);
        let surface = if let Some(id) = active_id {
            if finish_request && let Some(state) = self.agents_chat_page_states.get_mut(&id) {
                state.pending_native_requests = state.pending_native_requests.saturating_sub(1);
            }
            self.agents_chat_surfaces.get(&id).cloned()
        } else {
            self.parked_agents_chat_runtimes_by_project
                .values_mut()
                .find_map(|parked| {
                    let id = parked
                        .page_states
                        .iter()
                        .find(|(_, state)| state.generation == generation)
                        .map(|(id, _)| *id)?;
                    if finish_request && let Some(state) = parked.page_states.get_mut(&id) {
                        state.pending_native_requests =
                            state.pending_native_requests.saturating_sub(1);
                    }
                    parked.surfaces.get(&id).cloned()
                })
        };
        if let Some(surface) = surface {
            let literal = payload
                .to_string()
                .replace('\u{2028}', "\\u2028")
                .replace('\u{2029}', "\\u2029");
            surface.update(cx, |surface, _| {
                surface.execute_app_owned_script(&format!(
                    "(() => {{ const ns = window.ghostexGpui; if (ns?.sessionChatActivation?.generation === '{generation}') ns.{callback}?.({literal}); }})(); undefined;"
                ));
            });
        }
    }

    /// CDXC:SessionChat 2026-09-12 WHY:
    /// A renderer outlives its session binding and numeric shell IDs collide across projects.
    /// Match both its immutable renderer identity and the echoed binding generation before resolving any session action.
    pub(crate) fn receive_session_chat_renderer_action(
        &mut self,
        renderer_id: u64,
        payload: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        // CDXC:SessionChat 2026-09-12 WHY:
        // Shared toast and modal launchers carry captured content but no session identity; stamping their delayed callbacks with the current binding would invent an owner.
        // Keep only these existing global UI commands outside the generation requirement, and never reinterpret an explicitly stale generation.
        if message.get("pageGeneration").is_none() {
            let renderer_is_bound = self
                .agents_chat_page_states
                .values()
                .chain(
                    self.parked_agents_chat_runtimes_by_project
                        .values()
                        .flat_map(|parked| parked.page_states.values()),
                )
                .any(|state| state.renderer_id == renderer_id);
            if !renderer_is_bound {
                return;
            }
            if message["type"] == "toast" {
                self.receive_gpui_app_toast_bridge_message(&message, cx);
            } else if message["type"] == "open"
                && matches!(
                    message["modal"].as_str(),
                    Some("settings" | "mermaidDiagram" | "markdownTable")
                )
            {
                self.receive_app_modal_host_bridge_event(
                    cef::AppModalHostBridgeEvent::Message(payload.to_string()),
                    window,
                    cx,
                );
            }
            return;
        }
        let Some(generation) = message
            .get("pageGeneration")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return;
        };
        let binding = self
            .agents_chat_page_states
            .iter()
            .chain(
                self.parked_agents_chat_runtimes_by_project
                    .values()
                    .flat_map(|parked| parked.page_states.iter()),
            )
            .find(|(_, state)| state.renderer_id == renderer_id && state.generation == generation)
            .map(|(session_id, state)| (*session_id, state.account_key.clone()));
        let Some((session_id, account_key)) = binding else {
            return;
        };
        if message["type"] == "sessionChatHostAction"
            && message["action"] == "accountSwitchProgress"
        {
            if let Some(key) = account_key {
                self.set_session_account_switch_progress(
                    key,
                    &message["progress"],
                    Some(generation),
                    cx,
                );
            }
            return;
        }
        self.receive_owned_session_chat_host_action(session_id, generation, payload, window, cx);
    }
}
