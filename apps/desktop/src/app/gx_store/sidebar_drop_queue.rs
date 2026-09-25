//! A drag dropped before the stored key its write lands in has been read.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! A session move, a `createGroupFromSession` and a project move all edit a client-owned document,
//! so the store refuses them until the stored key is in hand: a write computed against a document
//! this app has not read would put an order over one it cannot see. That refusal used to be free,
//! because the sidebar page performed the drop instead. With the page gone the same refusal would
//! make the drop VANISH under the user's finger, which is the one answer a drag may never give.
//!
//! The drop is held here instead and replayed through the same entry point once the read lands, in
//! the order the drops were made. The queue is bounded (16), and a drop that has waited longer than
//! the deadline is dropped with a counter rather than applied late against a list that has moved
//! since: the reads retry for ever (`client_document.rs`, `workspace_groups.rs`), so without a
//! deadline a machine whose storage is broken would replay a minute-old drag.
//!
//! **The replay cannot loop.** It re-enters `dispatch_native_sidebar_ui`, which asks the same two
//! questions again, but a drop is only popped once its reads answer yes, and while the queue is
//! draining nothing may be queued: a drop that is refused for any other reason then takes the
//! ordinary route, exactly as it does today.
//!
//! **Counters** ride the `gxStore.sidebarDropQueue` record: `queued`, `applied`, `expired`,
//! `overflowed`. `queued` above zero with `applied` at zero on a run that ended is a drag the user
//! made and never saw land.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_drag.rs,
//! apps/desktop/src/app/gx_store/project_docs.rs,
//! apps/desktop/src/app/gx_store/client_document.rs.

use std::collections::VecDeque;
use std::time::Duration;
use web_time::Instant;

use ghostex_gx_core::CollectionsDocument;
use serde_json::{Value, json};

use super::diagnostics::{record, routine_logging_enabled};
use crate::GhostexGpuiApp;

/// Drops held at once. A drag is one gesture, so a user cannot make sixteen of them in the window
/// a storage read takes; a queue that grew without a bound would be a leak on a broken machine.
const MAX_QUEUED_DROPS: usize = 16;
/// How often the queue asks again whether the reads have landed.
const DROP_QUEUE_RETRY: Duration = Duration::from_millis(100);
/// How long a drop may wait. The first read retry is at five seconds, so this covers two of them.
const DROP_QUEUE_DEADLINE: Duration = Duration::from_secs(12);
/// Lines one app run may write about the queue.
const MAX_DROP_QUEUE_RECORDS: u32 = 64;

/// Which stored keys a held drop needs before it may be replayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DropQueueNeed {
    /// A session move and `createGroupFromSession`: the workspace session groups document.
    WorkspaceGroups,
    /// A project move: the collections document AND the workspace session groups document.
    ProjectDocuments,
}

struct QueuedDrop {
    command: Value,
    need: DropQueueNeed,
    queued_at: Instant,
}

/// What this app run did with drops made before their document was read. Memory only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DropQueueCounters {
    pub(crate) queued: u64,
    pub(crate) applied: u64,
    /// Drops let go because the reads did not land inside the deadline.
    pub(crate) expired: u64,
    /// Drops let go because sixteen newer ones were waiting.
    pub(crate) overflowed: u64,
}

#[derive(Default)]
pub(crate) struct SidebarDropQueue {
    drops: VecDeque<QueuedDrop>,
    /// A replay is running, so nothing may be queued: the drop is already in hand.
    draining: bool,
    drain_scheduled: bool,
    counters: DropQueueCounters,
    records: u32,
}

impl GhostexGpuiApp {
    /// Holds a drop whose document has not been read yet. Returns whether it was taken, in which
    /// case the caller must report the command as ANSWERED: it is this app's now, and sending it on
    /// as well would perform it twice.
    pub(super) fn gx_store_queue_sidebar_drop(
        &mut self,
        command: &Value,
        need: DropQueueNeed,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.gx_store.drop_queue.draining {
            return false;
        }
        let queue = &mut self.gx_store.drop_queue;
        while queue.drops.len() >= MAX_QUEUED_DROPS {
            queue.drops.pop_front();
            queue.counters.overflowed += 1;
        }
        queue.drops.push_back(QueuedDrop {
            command: command.clone(),
            need,
            queued_at: Instant::now(),
        });
        queue.counters.queued += 1;
        self.gx_store_record_drop_queue("queued");
        self.gx_store_schedule_sidebar_drop_drain(cx);
        true
    }

    /// Replays every held drop whose reads have landed, oldest first, and lets go of the ones that
    /// waited too long.
    fn gx_store_drain_sidebar_drop_queue(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.drop_queue.draining {
            return;
        }
        self.gx_store.drop_queue.draining = true;
        let mut expired = false;
        let mut applied = false;
        loop {
            let Some(front) = self.gx_store.drop_queue.drops.front() else {
                break;
            };
            let need = front.need;
            if front.queued_at.elapsed() >= DROP_QUEUE_DEADLINE {
                self.gx_store.drop_queue.drops.pop_front();
                self.gx_store.drop_queue.counters.expired += 1;
                expired = true;
                continue;
            }
            if !self.gx_store_drop_queue_ready(need, cx) {
                break;
            }
            let Some(drop) = self.gx_store.drop_queue.drops.pop_front() else {
                break;
            };
            self.gx_store.drop_queue.counters.applied += 1;
            applied = true;
            self.dispatch_native_sidebar_ui(drop.command, cx);
        }
        self.gx_store.drop_queue.draining = false;
        if applied {
            self.gx_store_record_drop_queue("applied");
        }
        if expired {
            self.gx_store_record_drop_queue("expired");
        }
        if !self.gx_store.drop_queue.drops.is_empty() {
            self.gx_store_schedule_sidebar_drop_drain(cx);
        }
    }

    /// Whether a held drop's documents are in hand. Asking books the read, which is the same call
    /// the drop itself made.
    fn gx_store_drop_queue_ready(
        &mut self,
        need: DropQueueNeed,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match need {
            DropQueueNeed::WorkspaceGroups => self.gx_store_restore_workspace_groups(cx),
            DropQueueNeed::ProjectDocuments => {
                // Both are asked, not short-circuited, so neither read is left unbooked.
                let collections = self.gx_document_restored::<CollectionsDocument>(cx);
                let groups = self.gx_store_restore_workspace_groups(cx);
                collections && groups
            }
        }
    }

    fn gx_store_schedule_sidebar_drop_drain(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.drop_queue.drain_scheduled {
            return;
        }
        self.gx_store.drop_queue.drain_scheduled = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DROP_QUEUE_RETRY).await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store.drop_queue.drain_scheduled = false;
                this.gx_store_drain_sidebar_drop_queue(cx);
            });
        })
        .detach();
    }

    /// One line per change of the queue: what happened and the totals. Never the command, which
    /// carries session, group and project ids.
    fn gx_store_record_drop_queue(&mut self, event: &'static str) {
        let queue = &mut self.gx_store.drop_queue;
        if queue.records >= MAX_DROP_QUEUE_RECORDS || !routine_logging_enabled() {
            return;
        }
        queue.records += 1;
        let counters = queue.counters;
        let held = queue.drops.len();
        record(
            "gxStore.sidebarDropQueue",
            json!({
                "event": event,
                "held": held,
                "queued": counters.queued,
                "applied": counters.applied,
                "expired": counters.expired,
                "overflowed": counters.overflowed,
            }),
        );
    }
}
