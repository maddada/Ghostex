use gpui::{App, Entity, Window, rgb};

/// CDXC:SessionChat 2026-09-17 DECISION: User: compare GPUI and React chat side by side in a separate app. Only the visible reference pane uses CEF; native chat behavior stays in QuickJS.
pub(super) fn create(window: &Window, cx: &mut App) -> Result<Entity<crate::CefSurface>, String> {
    let parent =
        crate::app::helpers::cef_parent_native_view(window).map_err(|error| error.to_string())?;
    crate::CefSurface::try_new(
        "chat-lab-react".into(),
        parent,
        "http://127.0.0.1:5188/?embedded=1".into(),
        "default".into(),
        0xff0d0d0d,
        false,
        rgb(0x0d0d0d).into(),
        None,
        true,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        cx,
    )
}
