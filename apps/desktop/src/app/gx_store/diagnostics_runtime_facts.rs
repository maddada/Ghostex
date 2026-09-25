//! The periodic line of the runtime facts channel and of the command route that replaced the
//! sidebar page, in a sibling because `diagnostics.rs` is over the size ceiling and waiting for a
//! quiet window.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! The line rides the same periodic path as `gxStore.sidebarList.summary`, for the reason that
//! cost two live rounds before it: a record emitted only when something happens has no line at all
//! in a run where nothing did, and the counts climbing are exactly what says the channel is alive.
//! The first line of a run goes out with every counter at zero on purpose. Nothing here carries an
//! id, a title or a path: the channel's payload never reaches the log, only its counts.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/runtime_facts.rs,
//! apps/desktop/src/app/gx_store/sidebar_runtime_route.rs.

use std::time::Instant;

use serde_json::json;

use super::diagnostics::{GxStoreDiagnostics, log_text, record, routine_logging_enabled};
use super::runtime_facts::RuntimeFactsCounters;
use super::sidebar_runtime_route::SidebarRuntimeRouteCounters;

impl GxStoreDiagnostics {
    /// The channel's totals and the command route's, at most once a minute and only when they
    /// moved.
    pub(super) fn runtime_facts_summary(
        &mut self,
        counters: RuntimeFactsCounters,
        route: SidebarRuntimeRouteCounters,
    ) {
        if self.runtime_facts_summary_written == Some((counters, route))
            || self
                .runtime_facts_summary_at
                .is_some_and(|at| at.elapsed() < super::diagnostics::PERIODIC_SUMMARY_INTERVAL)
        {
            return;
        }
        self.runtime_facts_summary_at = Some(Instant::now());
        if !routine_logging_enabled() {
            return;
        }
        self.runtime_facts_summary_written = Some((counters, route));
        record(
            "gxStore.runtimeFacts.summary",
            json!({
                "hudPosts": counters.hud_posts,
                // Held through the loading skeleton and answered once it lifted. The two move
                // together; a held one with no replay is a reveal that was dropped.
                "revealsHeld": counters.reveals_held,
                "revealsReplayed": counters.reveals_replayed,
                // The other half of "no page in the route": what the sidebar dispatch's
                // fall-through did with a command the store did not perform itself.
                "runtimeRoute": {
                    "uiOnly": route.ui_only,
                    // Above zero means a command has no owner on either side any more.
                    "unroutable": route.unroutable,
                    // Dropped at the door because the list was not ready yet (the launch window).
                    "beforeReady": route.before_ready,
                },
            }),
        );
    }

    /// A command that arrived before the list was ready. The TYPE only, which is a fixed word from
    /// the renderer's own closed set, never the payload.
    pub(super) fn sidebar_command_before_ready(&mut self, kind: Option<&str>) {
        if !routine_logging_enabled() {
            return;
        }
        record(
            "gxStore.sidebarCommandBeforeReady",
            json!({ "type": log_text(kind.unwrap_or("none")) }),
        );
    }

    /// A command that reached the end of the dispatch with no owner on either side. The
    /// TYPE only, which is a fixed word from the renderer's own closed set, never the payload.
    pub(super) fn sidebar_command_unroutable(&mut self, kind: Option<&str>) {
        if self.unroutable_command_warnings >= 8 {
            return;
        }
        self.unroutable_command_warnings += 1;
        record(
            "gxStore.sidebarCommandUnroutable",
            json!({ "type": log_text(kind.unwrap_or("none")) }),
        );
    }
}
