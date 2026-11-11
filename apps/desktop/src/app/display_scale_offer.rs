//! The one-time offer, on Linux, to set UI Scale back to 100% once Ghostex follows the
//! desktop's display scale (CDXC:PlatformSupport 2026-11-08 DECISION in the settings catalog's
//! `interface_scale.rs`).

use crate::GhostexGpuiApp;
use ghostex_settings_catalog::interface_scale::{
    INTERFACE_SCALE_SETTING, SCALED_AREA_SETTINGS, area_value_at_scale, scale_areas_separately,
};
use serde_json::{Map, json};

/// How long after the window opens the offer appears, so it lands on a drawn window.
#[cfg(target_os = "linux")]
const OFFER_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

impl GhostexGpuiApp {
    /// CDXC:PlatformSupport 2026-11-08 DECISION:
    /// User (issue #503, "offer a one-time reset"): before this release COSMIC users drew at 1.0 and raised UI Scale to make up for it; once Ghostex takes COSMIC's scale from XSETTINGS, that raise doubles everything again. The first launch that finds a display scale above 100% in XSETTINGS (and no fixed Display scale) offers once to set UI Scale back to 100%; closing the note keeps it. The marker is written on that launch whether or not the note shows, so someone who raises UI Scale later is never asked.
    #[cfg(target_os = "linux")]
    pub(crate) fn offer_interface_scale_reset_for_display_scale(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        use crate::app::window::{
            GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel,
        };
        use crate::linux_x11_scale::{ScaleSource, detected_scale};

        let Some(found) = detected_scale() else {
            return;
        };
        if found.source != ScaleSource::XSettings || found.fixed.is_some() {
            return;
        }
        let Some(scale) = found.automatic.filter(|scale| *scale > 1.01) else {
            return;
        };
        let marker = crate::shared_settings::ghostex_storage_paths()
            .state_dir
            .join("display-scale-interface-reset-offered");
        if marker.exists() {
            return;
        }
        if let Err(error) = marker
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&marker, b""))
        {
            // Without the marker the offer would come back every launch.
            log::warn!("Could not note the UI Scale offer: {error}");
            return;
        }
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        if !ghostex_settings_catalog::interface_scale::interface_scale_raised(settings.object()) {
            return;
        }
        let percent = (scale * 100.0).round() as i64;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(OFFER_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                this.upsert_gpui_app_toast(
                    GpuiAppToast {
                        copy_text: None,
                        id: "gpui-display-scale-interface-reset".to_string(),
                        level: GpuiAppToastLevel::Info,
                        title: "Ghostex follows your display scale".to_string(),
                        description: Some(format!(
                            "It now draws at your desktop's {percent}%, so your raised UI Scale makes everything extra large. Set UI Scale to 100%, or close this to keep it."
                        )),
                        action: Some(GpuiAppToastAction::ResetInterfaceScale),
                        loading: false,
                        persistent: true,
                        duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                        epoch: 0,
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    /// The offer's Set to 100%: Scale back to 100%, and with Scale areas separately on, each area
    /// back to the size Scale gives it at 100%.
    pub(crate) fn reset_interface_scale_to_default(&mut self, cx: &mut gpui::Context<Self>) {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let mut patch = Map::new();
        patch.insert(INTERFACE_SCALE_SETTING.to_string(), json!(100.0));
        if scale_areas_separately(settings.object()) {
            for key in SCALED_AREA_SETTINGS {
                patch.insert(key.to_string(), json!(area_value_at_scale(key, 100.0)));
            }
        }
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({ "patch": patch, "source": "interfaceScale:displayScaleReset" }),
            cx,
        );
    }
}
