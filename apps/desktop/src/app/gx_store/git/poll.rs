//! The background Git poll behind the project headers' +/- numbers, and the active project's
//! titlebar state on the same cadence. Ported from the old runtime's `git/diff-stats.ts`; each probe
//! is one gxserver read (`/api/readProjectGitState` with `diffStatsOnly`).
//!
//! CDXC:Git 2026-08-16:
//! Background polls must be invisible unless the numbers actually change: a probe publishes only
//! resolved, changed numbers, and a failed probe keeps the last ones for the next cycle.

use std::time::Duration;

use ghostex_gx_core::git_menu::{GitPollTarget, plan_git_poll_cycle};
use ghostex_gx_core::{MachineId, ProjectDiffStats, ProjectKey, is_chat_project_path};

use super::calls::{self, Remote};
use super::hud::GitReadOptions;
use crate::GhostexGpuiApp;

/// One project a cycle probes.
#[derive(Clone, Debug)]
struct PollProbe {
    target: GitPollTarget,
    key: ProjectKey,
}

impl GhostexGpuiApp {
    /// Starts the poll once (`startGitPollingDriver`).
    pub(crate) fn gx_store_git_start_poll(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.git.poll_started {
            return;
        }
        self.gx_store.git.poll_started = true;
        self.git_schedule_poll_cycle(cx);
    }

    /// One cycle: every project that renders a header, spread over the cycle, then the next cycle
    /// (`scheduleGitPollingCycle`).
    fn git_schedule_poll_cycle(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.git.poll_generation += 1;
        let generation = self.gx_store.git.poll_generation;
        self.gx_store.git.counters.poll_cycles += 1;
        let probes = self.git_poll_probes();
        let cycle = plan_git_poll_cycle(probes.iter().map(|probe| probe.target.clone()).collect());
        for (delay_ms, target) in cycle.probes {
            let Some(probe) = probes
                .iter()
                .find(|probe| probe.target.key == target.key)
                .cloned()
            else {
                continue;
            };
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(delay_ms))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.gx_store.git.poll_generation == generation {
                        this.git_poll_probe(probe, cx);
                    }
                });
            })
            .detach();
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(cycle.cycle_ms))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.gx_store.git.poll_generation == generation {
                    this.git_schedule_poll_cycle(cx);
                }
            });
        })
        .detach();
    }

    /// The projects whose header the sidebar draws right now: this computer's projects with a
    /// folder (not a Chats container or a Quick project), and a remote machine's while its stream
    /// is live (`getVisibleProjectDiffStatsRefreshTargets`).
    fn git_poll_probes(&self) -> Vec<PollProbe> {
        let view = self.gx_store.sidebar_list.view();
        let presentation = self.gx_store.core.presentation();
        view.groups
            .iter()
            .filter_map(|group| {
                let context = group.core.project_context.as_ref()?;
                if group.core.is_stale {
                    return None;
                }
                let key = match &group.core.remote_machine {
                    Some(machine) => {
                        ProjectKey::remote(machine.machine_id.clone(), machine.project_id.clone()?)
                    }
                    None => ProjectKey::local(context.project_id.clone()),
                };
                if !key.machine.is_local() && presentation.loaded_live(&key.machine).is_none() {
                    return None;
                }
                let path = context.path.trim().trim_end_matches('/');
                if path.is_empty() || is_chat_project_path(path) {
                    return None;
                }
                let target = match key.machine {
                    MachineId::Local => GitPollTarget::local(&key.project_id),
                    MachineId::Remote(_) => GitPollTarget::remote(&key.to_workspace_project_id()),
                };
                Some(PollProbe { target, key })
            })
            .collect()
    }

    /// One project's probe (`refreshProjectDiffStatsTarget`): its numbers, and for the active local
    /// project its titlebar state too, with the GitHub probe deferred so this loop never puts a
    /// `gh pr view` into a switch-time burst (`CDXC:Git 2026-07-29`).
    fn git_poll_probe(&mut self, probe: PollProbe, cx: &mut gpui::Context<Self>) {
        let scope = match self.git_scope_for_key(probe.key.clone()) {
            Ok(scope) if !scope.projectless => scope,
            _ => return,
        };
        self.git_refresh_diff_stats(scope.remote.clone(), &probe.key, cx);
        if !scope.is_remote() && self.git_scope_is_active(&scope) {
            self.git_refresh_state(
                scope,
                GitReadOptions {
                    defer_git_hub: true,
                    force: true,
                    ..GitReadOptions::default()
                },
                cx,
            )
            .detach();
        }
    }

    /// Re-reads one project's header numbers (`refreshProjectDiffStats`). Also run right after a
    /// write (`CDXC:Git 2026-08-16`: the cycle stretches with sidebar size, so a commit could
    /// otherwise leave the header stale for the better part of a minute).
    pub(crate) fn git_refresh_diff_stats(
        &mut self,
        remote: Remote,
        key: &ProjectKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let pending_key = key.to_workspace_project_id();
        if !self
            .gx_store
            .git
            .pending_diff_probes
            .insert(pending_key.clone())
        {
            return;
        }
        self.gx_store.git.counters.poll_probes += 1;
        let count_untracked = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("showUntrackedProjectDiffWhenNoTrackedChanges")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let project_id = key.project_id.clone();
        cx.spawn(async move |this, cx| {
            let result = calls::read_diff_stats(remote, project_id.clone(), count_untracked).await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store.git.pending_diff_probes.remove(&pending_key);
                let Some(stats) = result.ok().and_then(|read| read.diff_stats) else {
                    return;
                };
                this.git_set_diff_stats(project_id, stats.into_stats(), cx);
            });
        })
        .detach();
    }

    /// `setProjectDiffStats`: the header is redrawn only when its numbers changed.
    fn git_set_diff_stats(
        &mut self,
        project_id: String,
        stats: ProjectDiffStats,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gx_store.git.diff_stats.get(&project_id) == Some(&stats) {
            return;
        }
        self.gx_store
            .git
            .diff_stats
            .insert(project_id.clone(), stats);
        // The list reads the numbers from the same inputs the runtime's facts channel filled, keyed
        // by the project id the view looks them up by.
        let facts = &mut self.gx_store.runtime_facts;
        facts.project_diff_stats.insert(project_id, stats);
        facts.rows_generation += 1;
        self.gx_store_sidebar_state_changed(cx);
    }
}
