//! The desktop host of the Quick Access model: it hands the model the store, client storage and
//! the clock, and performs what the model asks for.
//!
//! CDXC:AppModal 2026-09-25 WHY:
//! The controller ran in the app runtime and reached Rust over two bridges: its snapshots over
//! `postNativeQuickAccessSnapshot`, its requests over the modal host's `sidebarCommand`. Both are
//! in-process now. A post and a modal open are deferred to the next turn, as the bridge delivered
//! them, so an effect never re-enters the controller while it is still answering (Reopen a Session
//! re-targets Quick Access, which is another `open` command). The frame the runtime's
//! `requestAnimationFrame` waited for is a 16 ms timer here, as it was in QuickJS.
//!
//! SEE-ALSO: packages/gx-core/src/quick_access/ (the model),
//! apps/desktop/src/app/quick_access_modal_lifecycle.rs (the window's open and close).

use std::time::Duration;

use chrono::TimeZone;
use ghostex_gx_core::{
    QuickAccessClock, QuickAccessContext, QuickAccessController, QuickAccessEffect,
    QuickAccessUpdate,
};
use serde_json::{Value, json};

use super::storage::DesktopQuickAccessStorage;
use crate::GhostexGpuiApp;

/// What `requestAnimationFrame` was in the QuickJS runtime.
const PUBLISH_FRAME_MS: u64 = 16;

/// The controller and nothing else: the model owns the state.
pub(crate) struct QuickAccessHost {
    controller: QuickAccessController,
}

impl Default for QuickAccessHost {
    fn default() -> Self {
        Self {
            controller: QuickAccessController::new(now_ms()),
        }
    }
}

fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// This computer's clock and time zone.
struct LocalClock {
    now_ms: i64,
}

impl QuickAccessClock for LocalClock {
    fn now_ms(&self) -> i64 {
        self.now_ms
    }

    fn utc_offset_ms_at(&self, ms: i64) -> i64 {
        chrono::Local
            .timestamp_millis_opt(ms)
            .single()
            .map(|stamp| i64::from(stamp.offset().local_minus_utc()) * 1_000)
            .unwrap_or(0)
    }
}

impl GhostexGpuiApp {
    fn with_quick_access(
        &mut self,
        run: impl FnOnce(
            &mut QuickAccessController,
            &mut QuickAccessContext<'_>,
        ) -> Vec<QuickAccessEffect>,
    ) -> Vec<QuickAccessEffect> {
        let now = now_ms();
        let data = self.gx_store_quick_access_data(now.max(0) as u64);
        let mut storage = DesktopQuickAccessStorage { now_ms: now };
        let clock = LocalClock { now_ms: now };
        let mut context = QuickAccessContext {
            data: &data,
            storage: &mut storage,
            clock: &clock,
        };
        run(&mut self.quick_access.controller, &mut context)
    }

    /// One command from the Quick Access window (open, a keystroke, a click, a menu item).
    pub(crate) fn quick_access_command(&mut self, command: Value, cx: &mut gpui::Context<Self>) {
        let effects =
            self.with_quick_access(|controller, context| controller.command(&command, context));
        self.perform_quick_access_effects(effects, cx);
    }

    /// An answer to a request Quick Access posted (recent projects, a sessions page, transcript
    /// sizes, saved prompts and their tags).
    pub(crate) fn quick_access_receive(&mut self, message: Value, cx: &mut gpui::Context<Self>) {
        let effects =
            self.with_quick_access(|controller, context| controller.receive(&message, context));
        self.perform_quick_access_effects(effects, cx);
    }

    /// The store moved: a showing window republishes, as `sidebarStore.subscribe(publish)` did.
    pub(crate) fn gx_store_quick_access_store_changed(
        &mut self,
        changed: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if !changed || !self.quick_access.controller.is_open() {
            return;
        }
        let effects = self.quick_access.controller.store_changed();
        self.perform_quick_access_effects(effects, cx);
    }

    fn flush_quick_access_publish(&mut self, cx: &mut gpui::Context<Self>) {
        let effects =
            self.with_quick_access(|controller, context| controller.flush_publish(context));
        self.perform_quick_access_effects(effects, cx);
    }

    fn fire_quick_access_sessions_timer(&mut self, token: u64, cx: &mut gpui::Context<Self>) {
        let effects = self.with_quick_access(|controller, context| {
            controller.sessions_timer_fired(token, context)
        });
        self.perform_quick_access_effects(effects, cx);
    }

    fn perform_quick_access_effects(
        &mut self,
        effects: Vec<QuickAccessEffect>,
        cx: &mut gpui::Context<Self>,
    ) {
        for effect in effects {
            match effect {
                QuickAccessEffect::Update(update) => self.apply_quick_access_update(update, cx),
                QuickAccessEffect::Post(message) => {
                    self.defer_in_main_window(cx, move |app, window, cx| {
                        app.handle_gpui_app_modal_sidebar_command(
                            json!({ "message": message }),
                            window,
                            cx,
                        );
                    });
                }
                QuickAccessEffect::OpenModal(message) => {
                    self.defer_in_main_window(cx, move |app, _window, cx| {
                        app.open_app_modal_from_bridge(message, cx);
                    });
                }
                QuickAccessEffect::CopyText(text) => self.gpui_copy_session_details_text(&text, cx),
                QuickAccessEffect::SchedulePublish => {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(PUBLISH_FRAME_MS))
                            .await;
                        let _ = this.update(cx, |app, cx| app.flush_quick_access_publish(cx));
                    })
                    .detach();
                }
                QuickAccessEffect::ScheduleSessionsRequest { token, delay_ms } => {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(delay_ms))
                            .await;
                        let _ = this.update(cx, |app, cx| {
                            app.fire_quick_access_sessions_timer(token, cx)
                        });
                    })
                    .detach();
                }
            }
        }
    }

    fn apply_quick_access_update(
        &mut self,
        update: QuickAccessUpdate,
        cx: &mut gpui::Context<Self>,
    ) {
        match serde_json::from_value(update.to_json()) {
            Ok(update) => self.apply_native_quick_access_update(update, cx),
            Err(error) => crate::support_logs::append_repro(
                crate::support_logs::GpuiSupportLog::SidebarRefresh,
                "gpui.quickAccess.invalidSnapshot",
                json!({ "error": error.to_string() }),
            ),
        }
    }
}
