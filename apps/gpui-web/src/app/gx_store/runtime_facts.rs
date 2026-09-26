//! The HUD and the per-row facts the list reads, under the desktop's names (`gx_store/runtime_facts.rs`), so the desktop's Git and HUD files write and read them unchanged. On the desktop this also ingested the old runtime's facts channel; the page never had one.
use std::collections::HashMap;
use std::sync::Arc;

use ghostex_gx_core::ProjectDiffStats;
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct SidebarRuntimeFacts {
    pub(crate) hud: Option<Arc<Value>>,
    /// Each project's +/- numbers, written by the Git poll (`git/poll.rs`).
    pub(crate) project_diff_stats: HashMap<String, ProjectDiffStats>,
    pub(crate) rows_generation: u64,
    pub(crate) hud_generation: u64,
    /// Each remote machine's client-parked projects; the page has no remote machines.
    pub(crate) remote_recent_projects: Vec<(String, Vec<Value>)>,
    pub(crate) counters: RuntimeFactsCounters,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RuntimeFactsCounters {
    pub(crate) hud_posts: u64,
}

impl SidebarRuntimeFacts {
    pub(crate) fn hud(&self) -> Option<&Arc<Value>> {
        self.hud.as_ref()
    }

    pub(crate) fn hud_value(&self) -> Arc<Value> {
        self.hud.clone().unwrap_or_else(|| Arc::new(json!({})))
    }
}
