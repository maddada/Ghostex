//! Opens the "isn't working" message (app/window/go_back_modal.rs) at launch, before any Ghostex
//! window, and runs its buttons. Try again opens Ghostex the usual way; Go back hands over to
//! swap.rs and quits.
use super::{GoBackHow, GoBackOffer, note_try_again, release_page, swap};
use crate::app::window::go_back_modal::{
    GO_BACK_MODAL_INITIAL_HEIGHT, GO_BACK_MODAL_WIDTH, GoBackModalCommand, GoBackModalConfig,
    GoBackModalHost, GoBackModalWay, GoBackModalWindow,
};
use crate::app::window::native_modal_kit::ModalPalette;
use gpui::{
    App, AppContext as _, Bounds, Entity, Styled as _, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, px, size,
};
use gpui_component::Root;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

type Continue = Box<dyn FnOnce(&mut App)>;

/// The message's palette, as the app's dialogs draw it, read before the app exists.
fn palette() -> ModalPalette {
    use crate::app::helpers::{frosted_dialog_fill, titlebar_background, window_glass_active};
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let palette = ModalPalette::resolve(
        crate::app::helpers::CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed),
        settings
            .object()
            .get("sidebarTheme")
            .and_then(serde_json::Value::as_str),
    )
    .tinted(titlebar_background().into());
    if !window_glass_active() {
        return palette;
    }
    palette.frosted_dialog(frosted_dialog_fill(palette.surface.into()).into())
}

/// Shows the message; `continue_startup` opens Ghostex's windows when the user picks Try again.
pub(crate) fn open(offer: GoBackOffer, continue_startup: Continue, cx: &mut App) {
    let palette = palette();
    let config = GoBackModalConfig {
        current: offer.current.clone(),
        previous: offer.previous.clone(),
        way: match &offer.how {
            GoBackHow::Swap => GoBackModalWay::Swap,
            GoBackHow::Command(command) => GoBackModalWay::Command(command.clone()),
            GoBackHow::Download(_) => GoBackModalWay::Download,
        },
        can_send_report: crash_report::available(),
    };
    let continuation: Rc<RefCell<Option<Continue>>> = Rc::new(RefCell::new(Some(continue_startup)));
    let window_slot: Rc<Cell<Option<WindowHandle<Root>>>> = Rc::new(Cell::new(None));
    let view_slot: Rc<RefCell<Option<Entity<GoBackModalWindow>>>> = Rc::new(RefCell::new(None));

    let run: Rc<dyn Fn(GoBackModalCommand, &mut App)> = {
        let continuation = continuation.clone();
        let window_slot = window_slot.clone();
        let view_slot = view_slot.clone();
        Rc::new(move |command, cx| match command {
            GoBackModalCommand::TryAgain => {
                note_try_again();
                // Ghostex's windows open first: closing the last window quits the app.
                if let Some(next) = continuation.borrow_mut().take() {
                    next(cx);
                }
                if let Some(handle) = window_slot.get() {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                }
            }
            GoBackModalCommand::GoBack { send_report } => {
                let view = view_slot.borrow().clone();
                match swap::start(&offer) {
                    Ok(()) => {
                        if let Some(view) = &view {
                            view.update(cx, |view, cx| view.set_going(cx));
                        }
                        let sending = send_report.then(|| crash_report::send_last(cx));
                        // The report gets a few seconds to go out (the swap waits for this
                        // process to end), and the message a moment to say what happens.
                        cx.spawn(async move |cx| {
                            let timer = cx.background_executor().timer(Duration::from_millis(
                                if sending.is_some() { 8_000 } else { 600 },
                            ));
                            match sending {
                                Some(sending) => {
                                    let _ =
                                        futures::future::select(Box::pin(sending), Box::pin(timer))
                                            .await;
                                }
                                None => timer.await,
                            }
                            cx.update(|cx| {
                                crate::app::consts::GPUI_APP_QUIT_IN_PROGRESS
                                    .store(true, Ordering::Release);
                                cx.quit();
                            });
                        })
                        .detach();
                    }
                    Err(reason) => {
                        if let Some(view) = &view {
                            view.update(cx, |view, cx| view.set_failed(reason, cx));
                        }
                    }
                }
            }
            GoBackModalCommand::CopyCommand(_) => {}
            GoBackModalCommand::Download { send_report } => {
                if send_report {
                    crash_report::send_last(cx).detach();
                }
                cx.open_url(&release_page(&offer.previous));
            }
        })
    };
    // A button press arrives while the message's view (and its window) is being updated, and
    // every action updates that view or closes that window: run it on the next turn, after the
    // press is done. Updating it in place panicked ("already being updated"), the same kind of
    // re-entrant update that crashed 11.8.0.
    let host: GoBackModalHost = Rc::new(move |command, cx| {
        let run = run.clone();
        cx.defer(move |cx| run(command, cx));
    });

    let bounds = Bounds::centered(
        None,
        size(px(GO_BACK_MODAL_WIDTH), px(GO_BACK_MODAL_INITIAL_HEIGHT)),
        cx,
    );
    let options = WindowOptions {
        kind: WindowKind::Normal,
        window_decorations: crate::app::window::popup_frame::child_window_decorations(),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: crate::gpui_platform_window_app_id(),
        icon: crate::gpui_platform_window_icon(),
        focus: true,
        show: true,
        is_resizable: false,
        is_minimizable: false,
        titlebar: None,
        window_background: crate::app::helpers::window_glass_background_appearance(),
        ..Default::default()
    };
    let border = palette.window_border();
    let view_out = view_slot.clone();
    let window = cx.open_window(options, move |window, cx| {
        crate::app::window::popup_frame::frame_app_modal_window(window, border);
        crate::app::helpers::apply_frosted_menu_blur(window);
        window.set_window_title("Ghostex");
        window.activate_window();
        let view = cx.new(|cx| GoBackModalWindow::new(config, palette, host, window, cx));
        *view_out.borrow_mut() = Some(view.clone());
        cx.new(|cx| {
            Root::new(view, window, cx)
                .bordered(false)
                .bg(gpui::transparent_black())
        })
    });
    match window {
        Ok(handle) => window_slot.set(Some(handle)),
        // No message window: open Ghostex as usual rather than nothing.
        Err(_) => {
            if let Some(next) = continuation.borrow_mut().take() {
                next(cx);
            }
        }
    }
}

/// "Also send the crash report": the crash reporter (crash_report/) sends the newest pending crash
/// with what its own window would send, without asking again.
mod crash_report {
    pub(super) fn available() -> bool {
        crate::crash_report::collect_pending().is_some()
    }

    pub(super) fn send_last(cx: &mut gpui::App) -> gpui::Task<()> {
        let scrub = crate::app::feedback_modal_lifecycle::feedback_scrub_context();
        cx.background_executor().spawn(async move {
            if let Err(reason) = crate::crash_report::send_pending_crash_report(scrub).await {
                crate::support_logs::append(
                    crate::support_logs::GpuiSupportLog::HostLifecycle,
                    "goBack.crashReportFailed",
                    serde_json::json!({ "reason": reason }),
                );
            }
        })
    }
}
