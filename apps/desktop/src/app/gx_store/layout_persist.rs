//! Shell layout persistence as "mark dirty": one task serializes the layout at most every
//! `LAYOUT_PERSIST_INTERVAL` while something changed.

use std::cell::Cell;
use std::time::Duration;

use futures::StreamExt as _;
use futures::channel::mpsc;

use crate::GhostexGpuiApp;
use crate::app::model::persist_gpui_workspace_shell_state;

/// CDXC:Workarea 2026-11-26 WHY: was 250 ms; each save still builds the layout snapshot on the UI
/// thread, and the quit path writes the final layout itself (`flush_shell_layout_state`), so a
/// second between saves loses nothing on a normal quit.
const LAYOUT_PERSIST_INTERVAL: Duration = Duration::from_millis(1000);

/// CDXC:Workarea 2026-09-19 WHY:
/// `persist_shell_layout_state` has about 165 call sites and used to serialize the whole shell layout to JSON on the UI thread at each of them, several times per tab step (the disk write already ran on a writer thread, 2026-09-16). A held "next tab" key paid that per repeat.
/// Callers now only mark the layout dirty. The quit path does not go through here: `flush_shell_layout_state` serializes the current state itself, so the last layout is always on disk.
#[derive(Default)]
pub(crate) struct LayoutPersist {
    /// `persist_shell_layout_state` takes `&self` at every call site, hence the cells.
    dirty: Cell<bool>,
    /// One wake per interval is enough; more would only queue.
    wake_sent: Cell<bool>,
    wake: Option<mpsc::UnboundedSender<()>>,
    serializations: Cell<u64>,
    marks: Cell<u64>,
}

impl LayoutPersist {
    /// The layout changed. Costs two flag writes; the task picks it up.
    pub(crate) fn mark_dirty(&self) {
        self.marks.set(self.marks.get() + 1);
        self.dirty.set(true);
        if let Some(wake) = &self.wake
            && !self.wake_sent.replace(true)
        {
            let _ = wake.unbounded_send(());
        }
    }

    /// `(marks, serializations)` since the app started.
    pub(crate) fn counters(&self) -> (u64, u64) {
        (self.marks.get(), self.serializations.get())
    }
}

impl GhostexGpuiApp {
    /// Starts the one task that writes the shell layout. Idempotent.
    ///
    /// Lifecycle: the task sleeps on the wake channel. A `mark_dirty` sends at most one wake per
    /// interval; the task then waits the interval, serializes once if the layout is still dirty,
    /// and goes back to the channel. It cannot spin, because every turn awaits either a wake that
    /// only a change sends or the interval timer, and it ends when the app entity is gone.
    pub(super) fn start_gx_store_layout_persist_task(&mut self, cx: &mut gpui::Context<Self>) {
        let persist = &mut self.gx_store.layout_persist;
        if persist.wake.is_some() {
            return;
        }
        let (wake, mut wakes) = mpsc::unbounded::<()>();
        // A change marked before the task existed is written by its first turn.
        if persist.dirty.get() {
            persist.wake_sent.set(true);
            let _ = wake.unbounded_send(());
        }
        persist.wake = Some(wake);
        cx.spawn(async move |this, cx| {
            while wakes.next().await.is_some() {
                cx.background_executor()
                    .timer(LAYOUT_PERSIST_INTERVAL)
                    .await;
                let alive = this.update(cx, |this, _| {
                    let persist = &this.gx_store.layout_persist;
                    persist.wake_sent.set(false);
                    if persist.dirty.replace(false) {
                        persist.serializations.set(persist.serializations.get() + 1);
                        persist_gpui_workspace_shell_state(this);
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
        })
        .detach();
    }
}
