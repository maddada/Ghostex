//! When Meet the rail opens and what its buttons do. The pop-up itself is
//! app/window/rail_intro_modal.rs.
//!
//! CDXC:Sidebar 2026-11-20 DECISION:
//! User, for 11.12.0: "On first start of this version, existing users are switched to the Rail automatically (not left on Classic), and the pop-up explains it", "Done closes and keeps whichever layout is showing", and "make sure the pop-up shows only ONCE per install: once answered (Accept, Keep Classic or closed), it never shows again, including across restarts, updates and after switching layouts". The first start of this version writes `sidebarLayout` "rail" and `sidebarRailIntro` "pending" into an existing settings file, once, behind a marker (shared_settings/sidebar_rail_default.rs); the pop-up opens while that is "pending" and sets it to "done" the moment it opens, so a window closed any way (Done, Escape, its corner, another dialog replacing it, quitting) never shows it again. Both live in this computer's settings file and its state folder: `sidebarRailIntro` is in no sync category and is left out of settings export (it is app state, not agent-writable), so each computer shows it at most once and none shows it because of another. Analytics (server/src/telemetry/taxonomy.rs): `rail_intro.shown` with the starting layout, `sidebar_layout.changed` with source `intro` for every Switch, `rail_intro.answered` with the layout kept on Done.
//! SEE-ALSO: shared_settings/sidebar_rail_default.rs (who gets it), packages/settings-catalog/src/lib.rs (`switch_existing_settings_to_rail`).

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{AppContext as _, Context};

use crate::GhostexGpuiApp;
use crate::app::model::GpuiAppModalKind;
use crate::app::window::{
    GpuiRailIntroModalWindow, RAIL_INTRO_MODAL_INITIAL_HEIGHT, RAIL_INTRO_MODAL_WIDTH,
    RailIntroCommand, RailIntroHost, RailIntroModalConfig,
};

const SETTINGS_INTRO_KEY: &str = "sidebarRailIntro";

/// Opened in this run already (by any window), so a second window never opens it again before the
/// settings file has caught up.
static RAIL_INTRO_OPENED: AtomicBool = AtomicBool::new(false);

fn layout_name(rail: bool) -> &'static str {
    if rail { "rail" } else { "classic" }
}

impl GhostexGpuiApp {
    /// Opens Meet the rail once, when it is pending and nothing else is in the way.
    pub(crate) fn maybe_open_sidebar_rail_intro(&mut self, cx: &mut Context<Self>) {
        if RAIL_INTRO_OPENED.load(Ordering::Relaxed)
            || self.native_sidebar.snapshot.is_none()
            || self.native_app_modal_kind().is_some()
        {
            return;
        }
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        if settings
            .object()
            .get(SETTINGS_INTRO_KEY)
            .and_then(serde_json::Value::as_str)
            != Some("pending")
        {
            return;
        }
        RAIL_INTRO_OPENED.store(true, Ordering::Relaxed);
        // Shown is answered: it never opens again, however this one closes.
        self.write_sidebar_settings(|settings| {
            settings.insert(SETTINGS_INTRO_KEY.to_string(), "done".into());
        });
        let rail = super::sidebar_rail_on();
        crate::app::helpers::telemetry::record_gpui_rail_intro_shown_telemetry(layout_name(rail));
        let config = RailIntroModalConfig {
            palette: self.gpui_native_dialog_palette(),
            rail,
        };
        let host: RailIntroHost =
            self.native_app_modal_host(cx, |app, command, cx| match command {
                RailIntroCommand::Switch { rail } => {
                    app.write_sidebar_settings(|settings| {
                        settings.insert(
                            ghostex_gx_core::sidebar_rail::SIDEBAR_LAYOUT_SETTINGS_KEY.to_string(),
                            layout_name(rail).into(),
                        );
                    });
                    super::note_sidebar_layout_switched_from_intro(rail);
                    cx.notify();
                }
                RailIntroCommand::Done { rail } => {
                    crate::app::helpers::telemetry::record_gpui_rail_intro_answered_telemetry(
                        layout_name(rail),
                    );
                    app.release_native_app_modal_window(GpuiAppModalKind::RailIntro, cx);
                    cx.notify();
                }
            });
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| {
                app.open_native_app_modal(
                    GpuiAppModalKind::RailIntro,
                    RAIL_INTRO_MODAL_WIDTH,
                    RAIL_INTRO_MODAL_INITIAL_HEIGHT,
                    move |window, cx| {
                        cx.new(|cx| GpuiRailIntroModalWindow::new(config, host, window, cx))
                    },
                    cx,
                );
            });
        });
    }

    fn write_sidebar_settings(
        &mut self,
        edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
    ) {
        let mut settings = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        edit(&mut settings);
        if let Err(error) = crate::shared_settings::write_shared_sidebar_settings_object(settings) {
            log::error!("Meet the rail: could not save the sidebar layout: {error:?}");
        }
    }
}
