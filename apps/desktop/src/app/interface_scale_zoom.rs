//! The zoom chords (Cmd+= / Cmd+- / Cmd+0, Ctrl on Windows and Linux) while Interface scale keeps
//! the areas together: they step Scale (`interfaceScalePercent`), so the sidebar, every chat, Files
//! and the terminal font change size at once (CDXC:Settings 2026-11-05 in the settings catalog's
//! `interface_scale.rs`). With Scale areas separately on, each area's own chord handler runs
//! instead, as before.

use gpui::{Context, Task};
use std::cell::Cell;

use crate::GhostexGpuiApp;
use crate::app::native_chat::zoom::{ZoomStep, set_pending_chat_zoom_percent, step_zoom_percent};
use crate::ui_fonts::{set_pending_files_zoom_percent, take_pending_files_zoom_percent};
use ghostex_settings_catalog::interface_scale::{
    INTERFACE_SCALE_SETTING, interface_scale_percent, scale_areas_separately,
};

/// Matches the chat's and Files' zoom saves: a held chord shows every rung at once and saves
/// once, after the keys go quiet.
const ZOOM_SAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(180);

thread_local! {
    /// The Scale the chords asked for that the debounced save has not written yet, so the next
    /// press steps from what is on screen.
    static PENDING_INTERFACE_SCALE_PERCENT: Cell<Option<f32>> = const { Cell::new(None) };
}

/// The one debounced save; a later press replaces it.
struct InterfaceScaleSave(#[allow(dead_code)] Task<()>);
impl gpui::Global for InterfaceScaleSave {}

impl GhostexGpuiApp {
    /// Steps Scale by one rung (the chat zoom's), or sets 100% for a reset; false when the areas
    /// scale separately, so the chord goes on to the focused area's own zoom.
    pub(crate) fn step_interface_scale(&mut self, step: ZoomStep, cx: &mut Context<Self>) -> bool {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        if scale_areas_separately(settings.object()) {
            return false;
        }
        let current = PENDING_INTERFACE_SCALE_PERCENT
            .with(Cell::get)
            .unwrap_or(interface_scale_percent(settings.object()) as f32);
        let next = step_zoom_percent(current, step);
        if next == current {
            // At the end of the range the chord is still Scale's.
            return true;
        }
        PENDING_INTERFACE_SCALE_PERCENT.with(|pending| pending.set(Some(next)));
        // The chats and Files redraw at the new size now; the sidebar and the terminals follow
        // when the save lands, since they size from the saved settings.
        set_pending_chat_zoom_percent(Some(next));
        set_pending_files_zoom_percent(Some(next));
        cx.refresh_windows();
        let save = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ZOOM_SAVE_DEBOUNCE).await;
            cx.update(|cx| {
                // Saved through the Settings patch, which updates the shared settings snapshot in
                // the same effect cycle, so no frame paints the old value between.
                if let Some(percent) = PENDING_INTERFACE_SCALE_PERCENT.with(Cell::take) {
                    if let Err(error) = this.update(cx, |this, cx| {
                        this.handle_gpui_app_modal_update_settings_patch_message(
                            &serde_json::json!({
                                "patch": { INTERFACE_SCALE_SETTING: percent },
                                "source": "interfaceScale:zoom",
                            }),
                            cx,
                        );
                    }) {
                        log::warn!("Could not save the interface scale: {error}");
                    }
                }
                set_pending_chat_zoom_percent(None);
                take_pending_files_zoom_percent();
                cx.refresh_windows();
            });
        });
        cx.set_global(InterfaceScaleSave(save));
        true
    }
}
