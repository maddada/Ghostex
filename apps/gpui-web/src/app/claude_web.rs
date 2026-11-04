//! The page has no full-window Claude (apps/desktop/src/app/claude_web/), so opening a session has
//! nothing to turn off.

#[allow(dead_code)]
pub(crate) enum OpenedSession {
    Focused { keeps_view: bool },
    Created,
}

impl crate::GhostexGpuiApp {
    pub(crate) fn note_opened_session(
        &mut self,
        _opened: OpenedSession,
        _cx: &mut gpui::Context<Self>,
    ) {
    }
}
