//! Meet the rail: the one-time intro for anyone who had Ghostex before the rail, who keeps the
//! Classic sidebar (`keep_classic_sidebar_for_existing_settings`). It says what the rail is and how
//! it bends to them, switches the window between Rail and Classic live, and closes on Accept.
//!
//! CDXC:Sidebar 2026-11-20 DECISION:
//! User (rail build brief): "existing users keep Classic (today's sidebar) and get a one-time intro on this version: 'Meet the rail' showing how customizable it is (pin, reorder, temporary items), with a live Rail/Classic toggle and Accept. Setting to switch any time." The intro opens once the main window is up and no other dialog is open, while `sidebarRailIntro` is "pending"; Accept (or closing it) keeps the layout on screen and sets it to "done", so it never comes back. Settings > Sidebar > Sidebar layout switches any time.
//! SEE-ALSO: shared_settings/sidebar_rail_default.rs (who gets it), server/src/telemetry/taxonomy.rs (`rail_intro.answered`).

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{
    App, AppContext as _, Context, FocusHandle, IntoElement, KeyDownEvent, ParentElement as _,
    Render, Styled as _, Window, px,
};
use gpui_component::v_flex;

use crate::GhostexGpuiApp;
use crate::app::model::GpuiAppModalKind;
use crate::app::window::native_modal_kit::*;

const TITLE: &str = "Meet the rail";
const DESCRIPTION: &str = "A narrow column of icons at the window's left edge. It stays when you hide the sidebar, so Search, notifications, the Hubs, Hermes, Claude and Grok are always one click away.";
const POINTS: [(&str, &str); 4] = [
    (
        "Pin what you use",
        "Its ⋯ menu lists everything; the pin on a row puts it on the rail or takes it off.",
    ),
    (
        "Put it in your order",
        "Drag icons up and down, or right-click one to move or unpin it.",
    ),
    (
        "Open anything without pinning it",
        "A row from ⋯ opens at once and shows on the rail under a dashed line while it's open.",
    ),
    (
        "Hubs across the window",
        "Agents Hub and All Automations open across the whole window, like Claude. Click their icon again to go back.",
    ),
];
const SWITCH_HINT: &str = "Try both here. You can switch any time in Settings > Sidebar.";

const SETTINGS_INTRO_KEY: &str = "sidebarRailIntro";

/// Opened in this run already (by any window), so a second window never opens it again.
static RAIL_INTRO_OPENED: AtomicBool = AtomicBool::new(false);

pub(crate) enum RailIntroCommand {
    /// The layout picked in the intro, applied at once.
    Pick { rail: bool },
    /// Accept, the corner close or Escape: keep what is on screen.
    Done {
        tried_rail: bool,
        tried_classic: bool,
    },
}

pub(crate) type RailIntroHost = Rc<dyn Fn(RailIntroCommand, &mut App)>;

pub(crate) struct RailIntroModal {
    host: RailIntroHost,
    palette: ModalPalette,
    rail: bool,
    tried_rail: bool,
    tried_classic: bool,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl RailIntroModal {
    fn new(
        host: RailIntroHost,
        palette: ModalPalette,
        rail: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette,
            rail,
            tried_rail: false,
            tried_classic: false,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn pick(&mut self, rail: bool, cx: &mut Context<Self>) {
        if self.rail == rail {
            return;
        }
        self.rail = rail;
        if rail {
            self.tried_rail = true;
        } else {
            self.tried_classic = true;
        }
        (self.host)(RailIntroCommand::Pick { rail }, cx);
        cx.notify();
    }

    fn done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(
            RailIntroCommand::Done {
                tried_rail: self.tried_rail,
                tried_classic: self.tried_classic,
            },
            cx,
        );
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" | "enter" => self.done(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }
}

impl Render for RailIntroModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let items = [
            ModalSegmentedItem {
                icon: None,
                label: "Rail",
            },
            ModalSegmentedItem {
                icon: None,
                label: "Classic",
            },
        ];
        let switch = v_flex()
            .gap(px(8.0))
            .child(modal_section_title(&p, "Sidebar layout"))
            .child(modal_segmented_control(
                &p,
                "rail-intro-layout",
                &items,
                if self.rail { 0 } else { 1 },
                |this: &mut Self, index, _window, cx| this.pick(index == 0, cx),
                cx,
            ))
            .child(modal_hint(&p, SWITCH_HINT));
        let points = v_flex()
            .gap(px(10.0))
            .children(POINTS.iter().map(|(title, text)| {
                v_flex()
                    .gap(px(2.0))
                    .child(gpui::div().text_size(px(13.0)).child(*title))
                    .child(modal_hint(&p, *text))
            }));
        let content = vec![
            modal_header(&p, TITLE, Some(DESCRIPTION)),
            v_flex()
                .w_full()
                .gap(px(18.0))
                .child(points)
                .child(switch)
                .into_any_element(),
        ];
        let accept = modal_action_button(
            &p,
            "rail-intro-accept",
            "Accept",
            None,
            ModalButtonTone::Primary,
            false,
            |this: &mut Self, window, cx| this.done(window, cx),
            cx,
        );
        modal_shell(
            &p,
            "ghostex-gpui-rail-intro",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            modal_footer(vec![accept]),
            None,
            cx,
        )
    }
}

impl ModalCornerClose for RailIntroModal {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.done(window, cx);
    }
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
        let rail = super::sidebar_rail_on();
        let palette = self.gpui_native_dialog_palette();
        let host: RailIntroHost =
            self.native_app_modal_host(cx, |app, command, cx| match command {
                RailIntroCommand::Pick { rail } => app.set_sidebar_layout_from_intro(rail, cx),
                RailIntroCommand::Done {
                    tried_rail,
                    tried_classic,
                } => {
                    let tried = match (tried_rail, tried_classic) {
                        (true, true) => "both",
                        (true, false) => "rail",
                        (false, true) => "classic",
                        (false, false) => "none",
                    };
                    let chose = if super::sidebar_rail_on() {
                        "rail"
                    } else {
                        "classic"
                    };
                    crate::app::helpers::telemetry::record_gpui_rail_intro_answered_telemetry(
                        tried, chose,
                    );
                    app.write_sidebar_settings(|settings| {
                        settings.insert(SETTINGS_INTRO_KEY.to_string(), "done".into());
                    });
                    app.release_native_app_modal_window(GpuiAppModalKind::RailIntro, cx);
                    cx.notify();
                }
            });
        let (width, height) = (
            super::RAIL_INTRO_MODAL_WIDTH,
            super::RAIL_INTRO_MODAL_INITIAL_HEIGHT,
        );
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| {
                app.open_native_app_modal(
                    GpuiAppModalKind::RailIntro,
                    width,
                    height,
                    move |window, cx| {
                        cx.new(|cx| RailIntroModal::new(host, palette, rail, window, cx))
                    },
                    cx,
                );
            });
        });
    }

    fn set_sidebar_layout_from_intro(&mut self, rail: bool, cx: &mut Context<Self>) {
        super::note_sidebar_layout_change_from_intro();
        self.write_sidebar_settings(|settings| {
            settings.insert(
                ghostex_gx_core::sidebar_rail::SIDEBAR_LAYOUT_SETTINGS_KEY.to_string(),
                if rail { "rail" } else { "classic" }.into(),
            );
        });
        cx.notify();
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
