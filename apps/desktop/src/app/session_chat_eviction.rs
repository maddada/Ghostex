use crate::app::consts::*;
use crate::app::model::*;
use crate::*;
use futures::channel::oneshot;
use futures::future::{Either, select};

#[derive(Clone)]
struct ChatEvictionCandidate {
    project_id: Option<String>,
    session_id: TerminalSessionId,
    generation: u64,
    hidden_since: Instant,
}

impl GhostexGpuiApp {
    pub(crate) fn cancel_session_chat_eviction_probe(&mut self, session_id: TerminalSessionId) {
        if let Some(state) = self.agents_chat_page_states.get_mut(&session_id) {
            state.pending_probe = None;
        }
    }

    fn chat_eviction_drag_active(&self) -> bool {
        self.workspace_tab_drag_active
            || self.browser_tab_drag_active
            || self.command_tab_drag_active
    }

    fn hidden_chat_page_count(&self) -> usize {
        self.agents_chat_surface_hidden_since
            .keys()
            .filter(|id| self.agents_chat_surfaces.contains_key(id))
            .count()
            + self
                .parked_agents_chat_runtimes_by_project
                .values()
                .map(|parked| {
                    parked
                        .surface_hidden_since
                        .keys()
                        .filter(|id| parked.surfaces.contains_key(id))
                        .count()
                })
                .sum::<usize>()
    }

    fn chat_eviction_candidates(&self) -> Vec<ChatEvictionCandidate> {
        let mut candidates = Vec::new();
        for (session_id, hidden_since) in &self.agents_chat_surface_hidden_since {
            if let Some(state) = self.agents_chat_page_states.get(session_id) {
                candidates.push(ChatEvictionCandidate {
                    project_id: self.agents_workspace_project_id.clone(),
                    session_id: *session_id,
                    generation: state.generation,
                    hidden_since: *hidden_since,
                });
            }
        }
        for (project_id, parked) in &self.parked_agents_chat_runtimes_by_project {
            for (session_id, hidden_since) in &parked.surface_hidden_since {
                if let Some(state) = parked.page_states.get(session_id) {
                    candidates.push(ChatEvictionCandidate {
                        project_id: Some(project_id.clone()),
                        session_id: *session_id,
                        generation: state.generation,
                        hidden_since: *hidden_since,
                    });
                }
            }
        }
        candidates.sort_by_key(|candidate| (candidate.hidden_since, candidate.generation));
        candidates
    }

    fn chat_eviction_candidate_allowed(
        &self,
        candidate: &ChatEvictionCandidate,
        require_empty: bool,
    ) -> bool {
        if self.chat_eviction_drag_active()
            || (candidate.hidden_since.elapsed() < GPUI_AGENTS_CHAT_SURFACE_HIDDEN_EVICT_AFTER
                && self.hidden_chat_page_count() <= GPUI_AGENTS_CHAT_SURFACE_HIDDEN_MAX)
        {
            return false;
        }
        if candidate.project_id == self.agents_workspace_project_id {
            self.agents_chat_surfaces
                .contains_key(&candidate.session_id)
                && self
                    .agents_chat_surface_hidden_since
                    .get(&candidate.session_id)
                    == Some(&candidate.hidden_since)
                && self
                    .agents_chat_page_states
                    .get(&candidate.session_id)
                    .is_some_and(|state| state.generation == candidate.generation)
                && self.agents_chat_surface_evictable(candidate.session_id, require_empty)
        } else {
            candidate
                .project_id
                .as_ref()
                .and_then(|project_id| self.parked_agents_chat_runtimes_by_project.get(project_id))
                .is_some_and(|parked| {
                    parked.surfaces.contains_key(&candidate.session_id)
                        && parked.surface_hidden_since.get(&candidate.session_id)
                            == Some(&candidate.hidden_since)
                        && parked
                            .page_states
                            .get(&candidate.session_id)
                            .is_some_and(|state| state.generation == candidate.generation)
                        && parked.surface_evictable(candidate.session_id, require_empty)
                })
        }
    }

    fn request_chat_eviction_probe(
        &mut self,
        candidate: &ChatEvictionCandidate,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(u64, oneshot::Receiver<bool>)> {
        if !self.chat_eviction_candidate_allowed(candidate, false) {
            return None;
        }
        let (state, surface) = if candidate.project_id == self.agents_workspace_project_id {
            (
                self.agents_chat_page_states
                    .get_mut(&candidate.session_id)?,
                self.agents_chat_surfaces
                    .get(&candidate.session_id)?
                    .clone(),
            )
        } else {
            let parked = self
                .parked_agents_chat_runtimes_by_project
                .get_mut(candidate.project_id.as_ref()?)?;
            (
                parked.page_states.get_mut(&candidate.session_id)?,
                parked.surfaces.get(&candidate.session_id)?.clone(),
            )
        };
        let nonce = SessionChatPageState::next_identity();
        let (sender, receiver) = oneshot::channel();
        state.pending_probe = Some((nonce, Some(sender)));
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&format!(
                "(function(){{var ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatEvictionProbeRequested==='function'){{ns.onSessionChatEvictionProbeRequested('{nonce}');}}}})(); undefined;"
            ));
        });
        Some((nonce, receiver))
    }

    fn finish_chat_eviction_probe(
        &mut self,
        candidate: &ChatEvictionCandidate,
        nonce: u64,
        allowed: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let eligible = allowed && self.chat_eviction_candidate_allowed(candidate, true);
        let active = candidate.project_id == self.agents_workspace_project_id;
        let state = if active {
            self.agents_chat_page_states.get_mut(&candidate.session_id)
        } else {
            candidate
                .project_id
                .as_ref()
                .and_then(|id| self.parked_agents_chat_runtimes_by_project.get_mut(id))
                .and_then(|parked| parked.page_states.get_mut(&candidate.session_id))
        };
        let Some(state) = state else {
            return;
        };
        if state.generation != candidate.generation
            || state.pending_probe.as_ref().map(|(id, _)| *id) != Some(nonce)
        {
            return;
        }
        state.pending_probe = None;
        if !eligible {
            return;
        }
        let surface = if active {
            self.agents_chat_page_states.remove(&candidate.session_id);
            self.agents_chat_surface_hidden_since
                .remove(&candidate.session_id);
            self.session_chat_composer_ready_sessions
                .remove(&candidate.session_id);
            self.session_chat_composer_empty_reports
                .remove(&candidate.session_id);
            self.agents_chat_surfaces.remove(&candidate.session_id)
        } else {
            let parked = self
                .parked_agents_chat_runtimes_by_project
                .get_mut(candidate.project_id.as_ref().unwrap())
                .unwrap();
            parked.page_states.remove(&candidate.session_id);
            parked.surface_hidden_since.remove(&candidate.session_id);
            parked.composer_ready_sessions.remove(&candidate.session_id);
            parked.composer_empty_reports.remove(&candidate.session_id);
            parked.protected_sessions.remove(&candidate.session_id);
            parked.surfaces.remove(&candidate.session_id)
        };
        if let Some(surface) = surface {
            surface.update(cx, |surface, _| surface.set_visible(false));
            drop(surface);
            if active {
                cx.notify();
            }
        }
    }

    /// CDXC:SessionChat 2026-09-05 WHY:
    /// A prior empty report can precede the final keystroke or blur, so elapsed time is not a safe substitute for a fresh reply.
    /// Probe the exact hidden page after a fresh provider activity read, then recheck native guards and its hidden epoch before dropping it.
    /// Serial probes preserve oldest-first eviction without simultaneous snapshot reads for every cached page; refusal, timeout and unknown state protect the page.
    pub(crate) fn evict_expired_hidden_agents_chat_surfaces(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.chat_eviction_drag_active() {
            return;
        }
        if self.agents_chat_eviction_running {
            self.agents_chat_eviction_requested = true;
            return;
        }
        let candidates = self.chat_eviction_candidates();
        if !candidates
            .iter()
            .any(|candidate| self.chat_eviction_candidate_allowed(candidate, false))
        {
            return;
        }
        self.agents_chat_eviction_running = true;
        self.agents_chat_eviction_requested = false;
        cx.spawn(async move |this, cx| {
            for candidate in candidates {
                let probe = this.update(cx, |this, cx| {
                    this.request_chat_eviction_probe(&candidate, cx)
                });
                let Ok(Some((nonce, receiver))) = probe else {
                    continue;
                };
                let timeout = cx.background_executor().timer(Duration::from_secs(3));
                let allowed = match select(receiver, Box::pin(timeout)).await {
                    Either::Left((Ok(allowed), _)) => allowed,
                    _ => false,
                };
                let _ = this.update(cx, |this, cx| {
                    this.finish_chat_eviction_probe(&candidate, nonce, allowed, cx)
                });
            }
            let _ = this.update(cx, |this, cx| {
                this.agents_chat_eviction_running = false;
                if std::mem::take(&mut this.agents_chat_eviction_requested) {
                    this.evict_expired_hidden_agents_chat_surfaces(cx);
                }
            });
        })
        .detach();
    }

    /// Reports from a parked or obsolete browser must never target a colliding active-project session ID.
    pub(crate) fn receive_owned_session_chat_host_action(
        &mut self,
        session_id: TerminalSessionId,
        generation: u64,
        payload: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let active = self
            .agents_chat_page_states
            .get(&session_id)
            .is_some_and(|state| state.generation == generation);
        // A workspace can acquire its project ID without parking its pages.
        // Resolve ownership from the unique page generation instead of capturing that provisional ID.
        let project_id = if active {
            self.agents_workspace_project_id.clone()
        } else {
            self.parked_agents_chat_runtimes_by_project
                .iter()
                .find_map(|(project_id, parked)| {
                    parked
                        .page_states
                        .get(&session_id)
                        .filter(|state| state.generation == generation)
                        .map(|_| project_id.clone())
                })
        };
        let (was_ready, old_empty) = if active {
            (
                self.session_chat_composer_ready_sessions
                    .contains(&session_id),
                self.session_chat_composer_empty_reports
                    .get(&session_id)
                    .copied(),
            )
        } else {
            project_id
                .as_ref()
                .and_then(|id| self.parked_agents_chat_runtimes_by_project.get(id))
                .map(|parked| {
                    (
                        parked.composer_ready_sessions.contains(&session_id),
                        parked.composer_empty_reports.get(&session_id).copied(),
                    )
                })
                .unwrap_or((false, None))
        };
        let state = if active {
            self.agents_chat_page_states.get_mut(&session_id)
        } else {
            project_id
                .as_ref()
                .and_then(|id| self.parked_agents_chat_runtimes_by_project.get_mut(id))
                .and_then(|parked| parked.page_states.get_mut(&session_id))
        };
        let Some(state) = state.filter(|state| state.generation == generation) else {
            return;
        };
        let Ok(message) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        let host_action = message.get("type").and_then(serde_json::Value::as_str)
            == Some("sessionChatHostAction");
        let action = host_action
            .then(|| message.get("action").and_then(serde_json::Value::as_str))
            .flatten();
        if action == Some("composerEvictionState") {
            let nonce = message
                .get("nonce")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| value.parse::<u64>().ok());
            let Some((expected, sender)) = state.pending_probe.as_mut() else {
                return;
            };
            if nonce != Some(*expected) {
                return;
            }
            let allowed = message.get("allowed").and_then(serde_json::Value::as_bool) == Some(true);
            let sender = sender.take();
            if active {
                self.session_chat_composer_empty_reports
                    .insert(session_id, allowed);
            } else if let Some(parked) = project_id
                .as_ref()
                .and_then(|id| self.parked_agents_chat_runtimes_by_project.get_mut(id))
            {
                parked.composer_empty_reports.insert(session_id, allowed);
            }
            if let Some(sender) = sender {
                let _ = sender.send(allowed);
            }
            return;
        }
        if action == Some("composerDraftState")
            && message.get("empty").and_then(serde_json::Value::as_bool) != Some(true)
        {
            state.pending_probe = None;
        }
        if !matches!(
            action,
            Some("composerReady" | "composerDraftState" | "diagnosticLog")
        ) {
            state.pending_probe = None;
        }
        if active {
            self.receive_session_chat_host_action(session_id, payload, window, cx);
        } else if let Some(parked) = project_id
            .as_ref()
            .and_then(|id| self.parked_agents_chat_runtimes_by_project.get_mut(id))
        {
            match action {
                Some("composerReady") => {
                    parked.composer_ready_sessions.insert(session_id);
                }
                Some("composerDraftState") => {
                    let empty =
                        message.get("empty").and_then(serde_json::Value::as_bool) == Some(true);
                    parked.composer_empty_reports.insert(session_id, empty);
                }
                _ => return,
            }
        }
        let readiness_changed = action == Some("composerReady") && !was_ready;
        let empty_changed = action == Some("composerDraftState")
            && old_empty
                != Some(message.get("empty").and_then(serde_json::Value::as_bool) == Some(true));
        if readiness_changed || empty_changed {
            self.evict_expired_hidden_agents_chat_surfaces(cx);
        }
    }
}
