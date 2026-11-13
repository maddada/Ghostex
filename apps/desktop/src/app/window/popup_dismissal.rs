//! When a popup that closes on click-away (the new-thread picker, the chat's option menus,
//! Quick Access and the app modals that opt in through `close_app_modal_on_click_away` or
//! `close_app_modal_on_click_in_ghostex`) has
//! really been dismissed. The other app modals never close on click-away (CDXC:AppModal
//! 2026-09-30 in app/window/modal_window_frame.rs).
//!
//! CDXC:AppModal 2026-09-30 WHY:
//! On Linux the window manager can move keyboard focus without a click: Hyprland (Omarchy's default) and other focus-follows-mouse setups focus the main window as soon as the pointer crosses onto it, so every popup that closed on focus loss closed when the mouse moved (Cmd+N's picker, the Saved Prompts Quick Access from a right-click, the chat's model menu, which flashed and vanished because the pointer is still over its trigger). Taking focus back fights the window manager on every pointer motion. So on Linux a focus loss dismisses a popup only once no Ghostex window holds focus (focus left the app), and a press in the main window after the popup opened is the click-away. macOS and Windows focus only on a click, so their focus loss stays the dismissal.
use gpui::{Context, Subscription, Window};

/// The latest pointer press in a Ghostex window (the main window, or a page shown in any window
/// on Linux), which Linux popups close on, and the window it landed in. Every platform records it:
/// the floating page window counts these presses as its second click away
/// (window/visual_page_modal.rs).
#[derive(Clone, Copy)]
pub(crate) struct MainWindowPointerPress {
    pub(crate) at: std::time::Instant,
    pub(crate) window: gpui::WindowId,
}

impl gpui::Global for MainWindowPointerPress {}

thread_local! {
    /// The notice modal's window while it is open (app/notice_modal_window.rs).
    static NOTICE_WINDOW: std::cell::Cell<Option<gpui::AnyWindowHandle>> =
        const { std::cell::Cell::new(None) };
}

/// Records the notice modal's window, or `None` once it closed.
///
/// CDXC:AppModal 2026-11-08 WHY: an approval can arrive while Settings is open (adding a remote computer asks Windows Hello for it). The notice modal takes focus over Settings, which is not a click away: Settings stays open behind it, with what the user typed.
pub(crate) fn set_notice_window(window: Option<gpui::AnyWindowHandle>) {
    NOTICE_WINDOW.with(|notice| notice.set(window));
}

fn is_notice_window(window: gpui::AnyWindowHandle) -> bool {
    NOTICE_WINDOW.with(|notice| notice.get() == Some(window))
}

/// How long a Linux focus loss waits for another Ghostex window to report focus before it counts
/// as focus leaving the app: the lost and the gained focus arrive as separate X11 events.
#[cfg(target_os = "linux")]
const FOCUS_SETTLE: std::time::Duration = std::time::Duration::from_millis(120);

/// Records a pointer press in `window`, closing the popups open before it except the one it landed
/// in (Linux).
pub(crate) fn note_main_window_pointer_press(window: gpui::WindowId, cx: &mut gpui::App) {
    cx.set_global(MainWindowPointerPress {
        at: std::time::Instant::now(),
        window,
    });
}

/// The latest pointer press inside an embedded web page (a CEF surface), and the window the page
/// is in. A page is its own native surface, so the press never reaches GPUI's mouse-down and the
/// popups drawn inside that window (the sidebar's menus, the titlebar's dropdowns), which close on
/// GPUI's click-outside, never hear it; their window closes them on this instead
/// (workspace_windows/open.rs).
///
/// CDXC:ContextMenus 2026-10-28 WHY:
/// User, on Omarchy: "when i have the context menu shown in the sidebar, then i click on a web page
/// in the embedded browser then it doesn't close that sidebar context menu". Popups in other
/// windows already closed on the page press above (`observe_main_window_press`); the ones in the
/// page's own window did not. Linux reports the press from Chromium's X11 child
/// (cef/linux_x11_pointer_focus.rs) and Windows from Chromium taking focus (cef/shell/lifecycle.rs);
/// macOS already closed them through its AppKit mouse-down observer
/// (`GhostexGpuiSidebarOutsideMouseDown`, `GhostexGpuiFirstResponderDidChange`).
#[derive(Clone, Copy)]
pub(crate) struct PagePointerPress {
    pub(crate) at: std::time::Instant,
    pub(crate) window: gpui::WindowId,
}

impl gpui::Global for PagePointerPress {}

/// Records a press in a page shown in `window`: a press in that window for the popups elsewhere,
/// and a page press for the popups drawn inside it.
pub(crate) fn note_page_pointer_press(window: gpui::WindowId, cx: &mut gpui::App) {
    note_main_window_pointer_press(window, cx);
    cx.set_global(PagePointerPress {
        at: std::time::Instant::now(),
        window,
    });
}

/// Records that a page shown in `window` took focus (Windows, where that is the only sign of a
/// click in it): a page press for the popups drawn inside that window, but not a press for the
/// other windows' click-away rules, since a focus the app hands a page itself looks the same.
pub(crate) fn note_page_focus(window: gpui::WindowId, cx: &mut gpui::App) {
    cx.set_global(PagePointerPress {
        at: std::time::Instant::now(),
        window,
    });
}

/// Whether a page press closes the popups drawn inside `own_window`: only a press in a page of
/// that same window does.
pub(crate) fn page_press_closes_popups_in(
    own_window: gpui::WindowId,
    press: PagePointerPress,
) -> bool {
    press.window == own_window
}

/// The latest recorded press, if any.
pub(crate) fn latest_main_window_press(cx: &gpui::App) -> Option<MainWindowPointerPress> {
    cx.try_global::<MainWindowPointerPress>().copied()
}

/// Call when a popup that closes on click-away has lost focus. Runs `dismiss` now, except on
/// Linux, where it runs only once focus has left every Ghostex window.
pub(crate) fn dismiss_on_focus_loss<T: 'static>(
    this: &mut T,
    window: &mut Window,
    cx: &mut Context<T>,
    dismiss: impl FnOnce(&mut T, &mut Window, &mut Context<T>) + 'static,
) {
    #[cfg(target_os = "linux")]
    {
        let _ = this;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(FOCUS_SETTLE).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !window.is_window_active() && cx.active_window().is_none() {
                    dismiss(this, window, cx);
                }
            });
        })
        .detach();
    }
    #[cfg(not(target_os = "linux"))]
    dismiss(this, window, cx);
}

/// Runs `dismiss` when the main window is pressed after this popup opened. Only Linux needs it:
/// elsewhere that press takes focus from the popup and `dismiss_on_focus_loss` covers it.
///
/// CDXC:AppModal 2026-10-16 WHY:
/// A click in a web page is reported from Chromium's own X11 child (cef/linux_x11_pointer_focus.rs), whichever window the page is in. When that counted as a main-window press, a click on any button of a page in the floating page window (or an extension modal's page) closed that very window on Linux (reported on KDE). A press is a click away only from the popups in other windows.
pub(crate) fn observe_main_window_press<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    dismiss: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
) -> Subscription {
    #[cfg(target_os = "linux")]
    {
        let opened_at = std::time::Instant::now();
        let own_window = window.window_handle().window_id();
        cx.observe_global_in::<MainWindowPointerPress>(window, move |this, window, cx| {
            let press = *cx.global::<MainWindowPointerPress>();
            if press.at > opened_at && press.window != own_window {
                dismiss(this, window, cx);
            }
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, cx, dismiss);
        Subscription::new(|| {})
    }
}

/// How long a popup's focus loss waits before reading who took focus: the lost and the gained
/// activation arrive as separate platform events.
pub(crate) const APP_SWITCH_SETTLE: std::time::Duration = std::time::Duration::from_millis(120);

/// Set each time a workspace window becomes the active window (macOS, Windows).
#[cfg(any(target_os = "windows", target_os = "macos"))]
#[derive(Clone, Copy)]
struct WorkspaceWindowActivation;

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl gpui::Global for WorkspaceWindowActivation {}

/// Records that workspace window `window` became active, which hands the keyboard back to a popup
/// left open while the user was in another app (`keep_open_across_app_switch`).
pub(crate) fn note_workspace_window_activation(window: gpui::WindowId, cx: &mut gpui::App) {
    let _ = window;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    cx.set_global(WorkspaceWindowActivation);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = cx;
}

/// Makes an app modal close on a click away, the way its Escape or corner X closes it
/// (`ModalCornerClose::close_from_corner`), unless `keeps_open_on_click_away` says it holds
/// unsaved input. Call once from the modal's constructor and keep the subscriptions.
///
/// CDXC:AppModal 2026-10-09 DECISION:
/// User (2026-10-08): "also close settings and find by prompt when i click on the main app window or click away outside them pls", then, picking from a list, the viewers (Browser History, the Markdown table viewer, the Mermaid diagram viewer, the Git file diff, the Export transcript result) and the Agents Hub; and (2026-10-09) "please when i click on the main app while the rename modal is shown then close the rename modal". These modals call this: Search by Prompt, the viewers above, Export Transcript once its export finished or failed, the Agents Hub, and Rename Session and Rename Worktree; every Settings page kind and the floating page window call `close_app_modal_on_click_in_ghostex` instead (CDXC:AppModal 2026-10-16 and 2026-11-13 there). They stay open while they hold unsaved input (`keeps_open_on_click_away`), except the two rename dialogs, which the user asked to close anyway: a click away cancels the rename, as Escape does. The notice modals (Update available, Missing project folder, Agent hooks required) and Add Project keep the 2026-09-30 "let's not make the modals close when i click away anymore" rule (window/modal_window_frame.rs). Quick Access has its own click-away close (window/quick_access/window.rs).
///
/// CDXC:AppModal 2026-10-08 WHY:
/// A dialog the modal opens itself (a file or folder picker, the system colour dialog, a confirm box, a UAC prompt) also takes focus from it, and must not count as a click away. So on macOS and Windows a focus loss closes the modal only when another Ghostex window took focus (the main window was clicked) or another app did; a window of this process that is not a GPUI window (the OS dialogs above) or no foreground window at all (the UAC secure desktop) keeps it open. Linux keeps the focus-follows-mouse rule above and closes only on a press in the main window, since its file pickers are portal dialogs in another process, indistinguishable from another app.
pub(crate) fn close_app_modal_on_click_away<V: super::native_modal_kit::ModalCornerClose>(
    window: &mut Window,
    cx: &mut Context<V>,
) -> Vec<Subscription> {
    close_app_modal_when_clicked_away(window, cx, true)
}

/// `close_app_modal_on_click_away` for Settings and the floating page window: switching to
/// another app and back keeps it open, and only a click in another Ghostex window closes it.
///
/// CDXC:AppModal 2026-10-16 DECISION:
/// User, asked what should close Settings on a click away, picked "Only a click in Ghostex closes it": the Settings window (all its page kinds: Settings, Hotkeys, Configure Agents, Configure Actions, Open Targets) no longer closes when focus goes to another app on macOS and Windows, only when another Ghostex window takes focus, as on Linux. The other click-away modals keep the 2026-10-09 rule above.
///
/// CDXC:AppModal 2026-11-13 DECISION:
/// User: "please let's not close the settings or the inline html or the ghostex form pop up when i open another app's window like we do now. it should stay on screen so i can paste from my clipboard app's window." Coming back to Ghostex no longer closes Settings (before, the main window becoming active after an app switch did, unless Settings was on Connectors, CDXC:AppModal 2026-10-17), and the floating page window (a page or a Ghostex form) now follows this rule too instead of hiding when another app took focus. Coming back without a click (an app switcher, a clipboard app pasting) hands the keyboard back to the popup so the paste lands in the field that had focus; coming back by a click in another Ghostex window is a click away and closes it as before (`keep_open_across_app_switch`).
pub(crate) fn close_app_modal_on_click_in_ghostex<V: super::native_modal_kit::ModalCornerClose>(
    window: &mut Window,
    cx: &mut Context<V>,
) -> Vec<Subscription> {
    close_app_modal_when_clicked_away(window, cx, false)
}

/// The two click-away rules; `other_apps_close` says whether focus going to another app closes.
fn close_app_modal_when_clicked_away<V: super::native_modal_kit::ModalCornerClose>(
    window: &mut Window,
    cx: &mut Context<V>,
    other_apps_close: bool,
) -> Vec<Subscription> {
    #[cfg_attr(
        not(any(target_os = "windows", target_os = "macos")),
        allow(unused_mut)
    )]
    let mut subscriptions = vec![observe_main_window_press(window, cx, close_unless_kept)];
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let modal = window.window_handle();
        let was_active = std::rc::Rc::new(std::cell::Cell::new(window.is_window_active()));
        subscriptions.push(cx.observe_window_activation(window, move |_, window, cx| {
            if window.is_window_active() {
                was_active.set(true);
                return;
            }
            // The inactive report a window gets before it is first shown is not a click away.
            if !was_active.get() {
                return;
            }
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(APP_SWITCH_SETTLE).await;
                let _ = this.update_in(cx, |this, window, cx| {
                    if window.is_window_active() {
                        return;
                    }
                    let left = focus_left_app();
                    let clicked_away = match cx.active_window() {
                        Some(active) => {
                            active != modal
                                && !is_notice_window(active)
                                && (other_apps_close || !left)
                        }
                        None => other_apps_close && left,
                    };
                    if clicked_away {
                        close_unless_kept(this, window, cx);
                    }
                });
            })
            .detach();
        }));
        if !other_apps_close {
            subscriptions.extend(keep_open_across_app_switch(window, cx, close_unless_kept));
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = other_apps_close;
    subscriptions
}

/// For a popup that stays open while the user is in another app (CDXC:AppModal 2026-11-13 on
/// `close_app_modal_on_click_in_ghostex`): when Ghostex comes back with a workspace window active
/// instead of the popup, runs `dismiss` if a click in Ghostex brought it back, and otherwise makes
/// the popup the key window again, so the keyboard (and a clipboard app's paste) reaches the field
/// that had focus. macOS and Windows; Linux popups never see the app switch as a click away.
pub(crate) fn keep_open_across_app_switch<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    dismiss: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
) -> Vec<Subscription> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use std::{cell::Cell, rc::Rc, time::Instant};
        let own_window = window.window_handle().window_id();
        let was_active = Rc::new(Cell::new(window.is_window_active()));
        // When focus left the app while this popup was the active window.
        let left_at: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
        let dismiss = Rc::new(dismiss);
        let activation = {
            let left_at = left_at.clone();
            cx.observe_window_activation(window, move |_, window, cx| {
                if window.is_window_active() {
                    was_active.set(true);
                    left_at.set(None);
                    return;
                }
                if !was_active.get() {
                    return;
                }
                let at = Instant::now();
                let left_at = left_at.clone();
                cx.spawn_in(window, async move |_, cx| {
                    cx.background_executor().timer(APP_SWITCH_SETTLE).await;
                    let _ = cx.update(|window, _| {
                        if !window.is_window_active() && focus_left_app() {
                            left_at.set(Some(at));
                        }
                    });
                })
                .detach();
            })
        };
        let comeback =
            cx.observe_global_in::<WorkspaceWindowActivation>(window, move |_, window, cx| {
                if left_at.get().is_none() || window.is_window_active() {
                    return;
                }
                let left_at = left_at.clone();
                let dismiss = dismiss.clone();
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(APP_SWITCH_SETTLE).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        let Some(at) = left_at.get() else {
                            return;
                        };
                        if window.is_window_active() || focus_left_app() {
                            return;
                        }
                        left_at.set(None);
                        if came_back_by_click(at, own_window, cx) {
                            dismiss(this, window, cx);
                        } else {
                            window.activate_window();
                        }
                    });
                })
                .detach();
            });
        vec![activation, comeback]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (window, cx, dismiss);
        Vec::new()
    }
}

/// Whether Ghostex came back by a press in a window other than `own_window` after `left_at`: a
/// GPUI press in a workspace window, or (macOS) any press in the app just now, which also sees a
/// press in an embedded web page.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn came_back_by_click(
    left_at: std::time::Instant,
    own_window: gpui::WindowId,
    cx: &gpui::App,
) -> bool {
    if latest_main_window_press(cx)
        .is_some_and(|press| press.at > left_at && press.window != own_window)
    {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn GhostexGpuiPointerPressedRecently() -> bool;
        }
        unsafe { GhostexGpuiPointerPressedRecently() }
    }
    #[cfg(not(target_os = "macos"))]
    false
}

/// Whether focus is now in another app, after `APP_SWITCH_SETTLE`: on Linux, where a popup's
/// focus loss reaches its dismissal only once no Ghostex window holds focus
/// (`dismiss_on_focus_loss`), that is no Ghostex window being active.
pub(crate) fn focus_went_to_another_app(cx: &gpui::App) -> bool {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let _ = cx;
        focus_left_app()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        cx.active_window().is_none()
    }
}

fn close_unless_kept<V: super::native_modal_kit::ModalCornerClose>(
    this: &mut V,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if !this.keeps_open_on_click_away(window, cx) {
        this.close_from_corner(window, cx);
    }
}

/// Whether another app holds the foreground, rather than a dialog of this process (a file
/// picker, the colour dialog, a message box) or nothing (the UAC secure desktop).
#[cfg(target_os = "windows")]
fn focus_left_app() -> bool {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(foreground, &mut process_id) };
    process_id != 0 && process_id != unsafe { GetCurrentProcessId() }
}

/// Whether another app is frontmost, rather than a panel of this process (NSOpenPanel,
/// NSColorPanel, an alert).
#[cfg(target_os = "macos")]
fn focus_left_app() -> bool {
    unsafe extern "C" {
        fn GhostexGpuiApplicationIsActive() -> bool;
    }
    !unsafe { GhostexGpuiApplicationIsActive() }
}

#[cfg(test)]
mod page_press_tests {
    use super::{PagePointerPress, page_press_closes_popups_in};

    #[test]
    fn a_page_press_closes_only_the_popups_of_its_own_window() {
        let main = gpui::WindowId::from(1);
        let other = gpui::WindowId::from(2);
        let press = PagePointerPress {
            at: std::time::Instant::now(),
            window: main,
        };
        assert!(page_press_closes_popups_in(main, press));
        assert!(!page_press_closes_popups_in(other, press));
    }
}
