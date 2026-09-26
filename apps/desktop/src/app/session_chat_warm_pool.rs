use crate::app::native_chat::state::NativeChatView;
use crate::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::app::session_chat_prewarm::NATIVE_CHAT_WARM_VIEWS_TOTAL;
/// How often hidden views are paused and the pool trimmed.
const NATIVE_CHAT_POOL_PASS_INTERVAL: Duration = Duration::from_secs(3);
/// A view pauses its subscription only after being off screen this long, so flipping between a few sessions keeps them all live.
const NATIVE_CHAT_PAUSE_AFTER_HIDDEN: Duration = Duration::from_secs(20);

thread_local! {
    /// When each runtime generation was last seen hidden by the pool pass.
    static HIDDEN_SINCE: RefCell<HashMap<u64, Instant>> = RefCell::new(HashMap::new());
}

impl GhostexGpuiApp {
    /// Runs the pool pass on a timer; a no-op when it is already running.
    pub(crate) fn ensure_native_chat_pool_pass(&mut self, cx: &mut gpui::Context<Self>) {
        if self.native_chat_pool_pass_scheduled {
            return;
        }
        self.native_chat_pool_pass_scheduled = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(NATIVE_CHAT_POOL_PASS_INTERVAL)
                    .await;
                if this
                    .update(cx, |this, cx| this.native_chat_pool_pass(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// CDXC:SessionChat 2026-09-19 WHY:
    /// Every native chat view kept its own runtime alive and subscribed for as long as its session existed: the ones the user opened, the prewarmed ones, and every view of every project switched away from. Thirty of them were live at once, and with eighteen sessions streaming the service thread spent most of its time relaying frames nobody was looking at, which is what made the app burn CPU.
    /// A view that is not in a visible pane has its document paused (`pause_session_chat_runtime`): its chat stays retained and followed in the Rust chat host, and the view gets everything it missed in one drain the moment it is shown again, so switching stays fast. Beyond a small app-wide total the least recently painted hidden views are pooled out entirely, never one holding unsent text or an unfinished request; a session whose view was pooled out is recreated by the ordinary reconcile when it is shown.
    fn native_chat_pool_pass(&mut self, cx: &mut gpui::Context<Self>) {
        let visible = self.native_chat_visible_sessions.clone();
        let mut hidden: Vec<(
            Option<Instant>,
            Option<String>,
            TerminalSessionId,
            u64,
            bool,
        )> = Vec::new();
        for (session_id, view) in &self.native_chat_views {
            if visible.contains(session_id) {
                continue;
            }
            let Some(state) = self.agents_chat_page_states.get(session_id) else {
                continue;
            };
            let (last_render, evictable) = Self::native_chat_pool_facts(view, state, cx);
            hidden.push((last_render, None, *session_id, state.generation, evictable));
        }
        for (project_id, parked) in &self.parked_agents_chat_runtimes_by_project {
            for (session_id, view) in &parked.native_views {
                let Some(state) = parked.page_states.get(session_id) else {
                    continue;
                };
                let (last_render, evictable) = Self::native_chat_pool_facts(view, state, cx);
                hidden.push((
                    last_render,
                    Some(project_id.clone()),
                    *session_id,
                    state.generation,
                    evictable,
                ));
            }
        }
        let now = Instant::now();
        let due = HIDDEN_SINCE.with_borrow_mut(|since| {
            let hidden_generations = hidden
                .iter()
                .map(|(_, _, _, generation, _)| *generation)
                .collect::<std::collections::HashSet<_>>();
            since.retain(|generation, _| hidden_generations.contains(generation));
            hidden_generations
                .into_iter()
                .filter(|generation| {
                    now.duration_since(*since.entry(*generation).or_insert(now))
                        >= NATIVE_CHAT_PAUSE_AFTER_HIDDEN
                })
                .collect::<Vec<_>>()
        });
        for generation in due {
            self.pause_session_chat_runtime(generation, cx);
        }
        let total = self.native_chat_views.len()
            + self
                .parked_agents_chat_runtimes_by_project
                .values()
                .map(|parked| parked.native_views.len())
                .sum::<usize>();
        let mut excess = total.saturating_sub(NATIVE_CHAT_WARM_VIEWS_TOTAL);
        if excess == 0 {
            return;
        }
        hidden.sort_by_key(|(last_render, ..)| *last_render);
        for (_, project_id, session_id, _, evictable) in hidden {
            if excess == 0 {
                break;
            }
            if !evictable {
                continue;
            }
            match project_id {
                None => self.evict_native_chat_view(session_id, cx),
                Some(project_id) => self.evict_parked_native_chat_view(&project_id, session_id),
            }
            excess -= 1;
        }
    }

    fn native_chat_pool_facts(
        view: &Entity<NativeChatView>,
        state: &SessionChatPageState,
        cx: &gpui::App,
    ) -> (Option<Instant>, bool) {
        let view = view.read(cx);
        let evictable = view.draft.trim().is_empty() && state.pending_native_requests == 0;
        (view.last_render, evictable)
    }

    /// Stops a hidden view's document: the chat stays retained and followed, and the view is sent
    /// only the requests it must perform until it is shown again.
    pub(crate) fn pause_session_chat_runtime(
        &mut self,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.session_chat_paused_generations.contains(&generation) {
            return;
        }
        let Some(view) = self.native_chat_for_generation(generation) else {
            return;
        };
        self.session_chat_paused_generations.insert(generation);
        if let Some(runtime) = &view.read(cx).runtime {
            runtime.set_paused(true);
        }
    }

    /// Restarts a paused view's document, which drains everything it missed in one frame.
    pub(crate) fn resume_session_chat_runtime(
        &mut self,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.session_chat_paused_generations.remove(&generation) {
            return;
        }
        if let Some(view) = self.native_chat_for_generation(generation)
            && let Some(runtime) = &view.read(cx).runtime
        {
            runtime.set_paused(false);
        }
    }

    /// Resumes every runtime whose view is in a visible pane; called after the reconcile computes that set and when a view paints.
    pub(crate) fn resume_visible_native_chat_runtimes(&mut self, cx: &mut gpui::Context<Self>) {
        if self.session_chat_paused_generations.is_empty() {
            return;
        }
        let generations = self
            .native_chat_visible_sessions
            .iter()
            .filter_map(|session_id| self.agents_chat_page_states.get(session_id))
            .map(|state| state.generation)
            .collect::<Vec<_>>();
        for generation in generations {
            self.resume_session_chat_runtime(generation, cx);
        }
    }

    /// Takes down the child windows of every view whose pane is not on screen, which is also what
    /// keeps a warmed-up view that was never shown from opening one.
    pub(crate) fn dismiss_native_chat_windows_leaving_view(
        &mut self,
        visible: &HashSet<TerminalSessionId>,
        cx: &mut gpui::Context<Self>,
    ) {
        let hidden = self
            .native_chat_views
            .iter()
            .filter(|(session_id, _)| !visible.contains(*session_id))
            .map(|(_, view)| view.clone())
            .collect::<Vec<_>>();
        for view in hidden {
            view.update(cx, |view, cx| view.dismiss_windows_for_hidden_pane(cx));
        }
    }

    /// Reopens the modals of views whose pane is on screen again, on its current frame. Views that
    /// never lost their pane answer this with nothing to do.
    pub(crate) fn restore_native_chat_windows_entering_view(
        &mut self,
        visible: &HashSet<TerminalSessionId>,
        cx: &mut gpui::Context<Self>,
    ) {
        let shown = visible
            .iter()
            .filter_map(|session_id| self.native_chat_views.get(session_id).cloned())
            .collect::<Vec<_>>();
        for view in shown {
            view.update(cx, |view, cx| view.restore_windows_for_shown_pane(cx));
        }
    }

    /// A painted pane is on screen by definition: resume its runtime and give it back the modals it
    /// was holding, even if no reconcile ran since it was selected.
    pub(crate) fn note_native_chat_pane_painted(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.resume_native_chat_runtime_for_session(session_id, cx);
        if let Some(view) = self.native_chat_views.get(&session_id).cloned() {
            view.update(cx, |view, cx| view.restore_windows_for_shown_pane(cx));
        }
    }

    /// A painted view is visible by definition; resume it even if no reconcile ran since it was selected.
    pub(crate) fn resume_native_chat_runtime_for_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(generation) = self
            .agents_chat_page_states
            .get(&session_id)
            .map(|state| state.generation)
        else {
            return;
        };
        self.native_chat_visible_sessions.insert(session_id);
        self.resume_session_chat_runtime(generation, cx);
    }

    /// Drops a hidden view of the active project while keeping the session's chat mode, so showing it again recreates the view.
    fn evict_native_chat_view(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(view) = self.native_chat_views.remove(&session_id) {
            view.update(cx, |view, cx| view.dismiss_windows_for_hidden_pane(cx));
        }
        self.session_chat_composer_ready_sessions
            .remove(&session_id);
        self.session_chat_composer_empty_reports.remove(&session_id);
        if let Some(state) = self.agents_chat_page_states.remove(&session_id) {
            self.session_chat_paused_generations
                .remove(&state.generation);
        }
        self.record_session_chat_lifecycle(session_id, "sessionChat.nativePageRemoved", "warmPool");
    }

    fn evict_parked_native_chat_view(
        &mut self,
        project_id: &str,
        session_id: TerminalSessionId,
    ) {
        let Some(parked) = self
            .parked_agents_chat_runtimes_by_project
            .get_mut(project_id)
        else {
            return;
        };
        parked.native_views.remove(&session_id);
        parked.composer_ready_sessions.remove(&session_id);
        parked.composer_empty_reports.remove(&session_id);
        let state = parked.page_states.remove(&session_id);
        if let Some(state) = state {
            self.session_chat_paused_generations
                .remove(&state.generation);
        }
    }

    /// Native chat views alive across the active and parked projects.
    pub(crate) fn native_chat_views_total(&self) -> usize {
        self.native_chat_views.len()
            + self
                .parked_agents_chat_runtimes_by_project
                .values()
                .map(|parked| parked.native_views.len())
                .sum::<usize>()
    }
}
