//! The list's own clock: one wake booked for the next moment a row or its label reads differently (a Delayed Send or Close After Done countdown, a relative time, a snooze ending), as the desktop's `gx_store_book_sidebar_deadline` does, so an idle page wakes only when something on screen changes.
use std::time::Duration;

use super::host::now_ms;
use crate::GhostexGpuiApp;

/// A deadline already due still waits this long, so a label that keeps reading "now" cannot spin.
const MIN_DEADLINE_WAIT: Duration = Duration::from_millis(250);

impl GhostexGpuiApp {
    pub(crate) fn web_book_sidebar_deadline(&mut self, cx: &mut gpui::Context<Self>) {
        let now = now_ms();
        let Some(deadline) = self.gx_store.sidebar_list.next_deadline_ms(now) else {
            self.gx_store.sidebar_list.deadline_booked = None;
            return;
        };
        // A wake for an earlier or equal deadline already covers this one.
        if self
            .gx_store
            .sidebar_list
            .deadline_booked
            .is_some_and(|booked| booked <= deadline)
        {
            return;
        }
        self.gx_store.sidebar_list.deadline_booked = Some(deadline);
        let wait = Duration::from_millis(deadline.saturating_sub(now)).max(MIN_DEADLINE_WAIT);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                if this.gx_store.sidebar_list.deadline_booked != Some(deadline) {
                    return;
                }
                this.gx_store.sidebar_list.deadline_booked = None;
                this.gx_store_sidebar_state_changed(cx);
            });
        })
        .detach();
    }
}
