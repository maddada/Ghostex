use std::sync::atomic::Ordering;

use gpui::App;

use crate::app::helpers::*;
use crate::*;

thread_local! {
    /// The menu variant last installed, so the Window menu's list of windows can be rebuilt
    /// without changing it.
    static MAIN_MENUS_SOURCE_FOCUS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn set_ghostex_gpui_main_menus(source_workarea_cef_owns_native_focus: bool, cx: &App) {
    MAIN_MENUS_SOURCE_FOCUS.set(source_workarea_cef_owns_native_focus);
    cx.set_menus(ghostex_gpui_main_menus_for_source_focus(
        source_workarea_cef_owns_native_focus,
    ));
    #[cfg(target_os = "macos")]
    cef::refresh_application_menu_hooks();
}

/// The menu variant installed now (`set_ghostex_gpui_main_menus`).
pub(crate) fn installed_main_menus_source_focus() -> bool {
    MAIN_MENUS_SOURCE_FOCUS.get()
}

/// Rebuilds the menu bar after a workspace window opened, closed, was renamed or became the
/// active one, so the Window menu lists the windows as they are (app/workspace_windows/).
pub(crate) fn refresh_ghostex_gpui_main_menus(cx: &mut App) {
    set_ghostex_gpui_main_menus(MAIN_MENUS_SOURCE_FOCUS.get(), cx);
}

pub(crate) fn register_ghostex_gpui_main_menu_actions(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    main_window: gpui::AnyWindowHandle,
    cx: &mut App,
) {
    /*
    CDXC:OsIntegration 2026-07-10:
    GPUI validates native menu items against the active window dispatch tree
    plus app-global action listeners. Menu validation can run without that
    GPUI dispatch tree while native CEF/Ghostty responders or child panels own
    AppKit focus, so application and window commands must be registered
    globally instead of existing only on the rendered shell root. Keep
    focus-sensitive File/Edit commands on their normal window/responder paths.
    */
    cx.on_action(|_: &AboutGhostexGpui, _cx| {
        #[cfg(target_os = "macos")]
        unsafe {
            GhostexGpuiShowStandardAboutPanel()
        };
    });
    cx.on_action({
        let app = app.clone();
        move |_: &CheckForGhostexGpuiUpdates, cx| {
            /*
            CDXC:OsIntegration 2026-07-24:
            App-menu update checks dispatch while GPUI already owns the active
            window update. Defer the Sparkle handoff until that action cycle
            returns so the main window can be borrowed and Sparkle can present
            its standard user-initiated update or no-update UI.
            */
            let (window, app) = crate::app::workspace_windows::lead_workspace_window(cx)
                .unwrap_or_else(|| (main_window, app.clone()));
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| app.check_for_gpui_updates(window, cx));
                });
            });
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &OpenGpuiSettingsModal, cx| {
            /*
            CDXC:OsIntegration 2026-07-24:
            Native app-menu actions dispatch while GPUI already owns the active
            window update. Defer the Settings window mutation until that action
            cycle returns so the main window can be borrowed normally instead
            of silently rejecting a re-entrant update.
            */
            let (window, app) = menu_target_window(main_window, &app, cx);
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.open_gpui_app_modal_from_titlebar(
                            GpuiAppModalKind::Settings,
                            window,
                            cx,
                        );
                    });
                });
            });
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &OpenGpuiExtensionsModal, cx| {
            let (window, app) = menu_target_window(main_window, &app, cx);
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.open_gpui_settings_extensions_page(Some(window), cx);
                    });
                });
            });
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &OpenGpuiAccountsModal, cx| {
            let (window, app) = menu_target_window(main_window, &app, cx);
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.open_gpui_settings_accounts_page(Some(window), cx);
                    });
                });
            });
        }
    });
    cx.on_action(|_: &HideGhostexGpui, cx| cx.hide());
    cx.on_action(|_: &HideGhostexGpuiOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAllGhostexGpuiApps, cx| cx.unhide_other_apps());
    cx.on_action(|_: &RestartGhostexGpui, cx| {
        GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
        cx.restart();
    });
    cx.on_action(|_: &QuitGhostexGpui, cx| {
        GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
        cx.quit();
    });
    cx.on_action(|_: &QuitGhostexGpuiAndBackgroundServices, cx| {
        /*
        CDXC:OsIntegration 2026-08-28:
        Plain Quit deliberately leaves gxserver, the zmx session daemons, and
        the GhostexEditor daemon running so agents survive app restarts. This
        item is the opposite promise: nothing Ghostex started stays behind. The
        teardown runs blocking subprocess/HTTP work, so it happens on the
        background executor and the app quits only after it finishes (every
        step inside is bounded, so quitting can never hang indefinitely).
        */
        GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
        let teardown = cx
            .background_executor()
            .spawn(async { gpui_stop_all_ghostex_background_services() });
        cx.spawn(async move |cx| {
            teardown.await;
            let _ = cx.update(|cx| cx.quit());
        })
        .detach();
    });
    cx.on_action({
        let app = app.clone();
        move |_: &MinimizeGhostexGpuiWindow, cx| {
            let (window, _) = menu_target_window(main_window, &app, cx);
            let _ = window.update(cx, |_, window, _cx| window.minimize_window());
        }
    });
    cx.on_action(move |_: &ZoomGhostexGpuiWindow, cx| {
        let (window, _) = menu_target_window(main_window, &app, cx);
        let _ = window.update(cx, |_, window, _cx| window.zoom_window());
    });
}

/// The window a menu bar command acts in: the workspace window the user is in, since File > New
/// Window can open several (app/workspace_windows/), else the launch window.
fn menu_target_window(
    main_window: gpui::AnyWindowHandle,
    app: &gpui::WeakEntity<GhostexGpuiApp>,
    cx: &App,
) -> (gpui::AnyWindowHandle, gpui::WeakEntity<GhostexGpuiApp>) {
    crate::app::workspace_windows::active_workspace_window(cx)
        .unwrap_or_else(|| (main_window, app.clone()))
}

/// Native app menu bar (macOS `installMainMenu` parity, AppDelegate.swift
/// :2533-2663): App (About/Check for Updates/Settings/Hide/Restart/Quit),
/// File → New Window ⇧⌘N and Close Pane ⌘W, the Edit clipboard set (first-responder OS actions so
/// CEF and Ghostty views handle them natively), and Window → Minimize/Zoom, Cycle Through Windows
/// and the open windows.
/// Undo/Redo are omitted from the GPUI-owned menu because gpui routes them
/// through app actions instead of first-responder selectors; the macOS CEF hook
/// installs them after each menu replacement, targeted at an object that picks
/// the focused web page or [`dispatch_text_input_history_from_edit_menu`].
pub(crate) fn ghostex_gpui_main_menus_for_source_focus(
    source_workarea_cef_owns_native_focus: bool,
) -> Vec<gpui::Menu> {
    use gpui::{Menu, MenuItem, OsAction};
    /*
    CDXC:Hotkeys 2026-07-05:
    GPUI macOS derives menu item key equivalents from the keymap when
    `cx.set_menus` builds NSMenuItems. While Source CEF owns native focus,
    replace File > Close Pane with a menu-only action that has no keybinding,
    so `[NSApp mainMenu] performKeyEquivalent:` cannot consume Cmd-W before
    AppKit offers it to embedded VSCode. The CEF menu-install hook similarly
    removes standard Edit key equivalents while Source owns focus, without
    removing their clickable actions. App-reserved quit/hide/minimize
    equivalents stay on their normal actions.
    */
    let close_pane_item = if source_workarea_cef_owns_native_focus {
        MenuItem::action("Close Pane", CloseFocusedSurfaceMenuOnly)
    } else {
        MenuItem::action("Close Pane", CloseFocusedSurface)
    };
    vec![
        Menu::new("Ghostex").items(vec![
            MenuItem::action("About Ghostex", AboutGhostexGpui),
            MenuItem::action("Check for Updates…", CheckForGhostexGpuiUpdates),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenGpuiSettingsModal),
            MenuItem::separator(),
            MenuItem::action("Hide Ghostex", HideGhostexGpui),
            MenuItem::action("Hide Others", HideGhostexGpuiOthers),
            MenuItem::action("Show All", ShowAllGhostexGpuiApps),
            MenuItem::separator(),
            // CDXC:OsIntegration 2026-09-05 DECISION:
            // User: add Restart Ghostex to the app menu directly above Quit Ghostex.
            MenuItem::action("Restart Ghostex", RestartGhostexGpui),
            MenuItem::action("Quit Ghostex", QuitGhostexGpui),
            MenuItem::action(
                "Quit Ghostex & BG Service",
                QuitGhostexGpuiAndBackgroundServices,
            ),
        ]),
        Menu::new("File").items(vec![
            MenuItem::action("New Window", NewGhostexGpuiWindow),
            MenuItem::separator(),
            close_pane_item,
        ]),
        /*
        CDXC:Clipboard 2026-10-17 DECISION:
        User: Edit › Copy, Cut, Paste and Select All work in the terminal and the chat, enabled
        only when they can act (Copy with a selection), so apps that read a selection by
        pressing Edit › Copy get the text. The OS selectors reach CEF views natively; for GPUI
        views the app delegate dispatches these actions and validates them by whether a
        handler is on the focus path: `TerminalEditMenuActions` (terminal_element.rs) and
        `ChatEditMenuActions` (native_chat/edit_shortcuts.rs) register them only then. A
        terminal has nothing to cut, so Cut stays disabled there.
        */
        Menu::new("Edit").items(vec![
            MenuItem::os_action("Cut", GpuiEditMenuCut, OsAction::Cut),
            MenuItem::os_action("Copy", GpuiEditMenuCopy, OsAction::Copy),
            MenuItem::os_action("Paste", GpuiEditMenuPaste, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action("Select All", GpuiEditMenuSelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Window").items(ghostex_gpui_window_menu_items()),
    ]
}

/// Window: Minimize, Zoom, Cycle Through Windows (Cmd+`), and one row per open workspace window
/// with a tick on the active one (app/workspace_windows/).
fn ghostex_gpui_window_menu_items() -> Vec<gpui::MenuItem> {
    use gpui::MenuItem;
    let mut items = vec![
        MenuItem::action("Minimize", MinimizeGhostexGpuiWindow),
        MenuItem::action("Zoom", ZoomGhostexGpuiWindow),
        MenuItem::separator(),
        MenuItem::action("Cycle Through Windows", CycleGhostexGpuiWindows),
    ];
    let rows = crate::app::workspace_windows::workspace_window_menu_rows();
    if !rows.is_empty() {
        items.push(MenuItem::separator());
        items.extend(rows.into_iter().map(|row| {
            MenuItem::action(row.label, ActivateGhostexGpuiWindow { number: row.number })
                .checked(row.active)
        }));
    }
    items
}

/// Runs Edit > Undo or Redo against the focused text field of the active GPUI window. The macOS
/// menu target calls this when no web page owns the keyboard; a Cmd+Z the text field already took
/// never gets here.
#[cfg(target_os = "macos")]
pub(crate) fn dispatch_text_input_history_from_edit_menu(
    gpui_root_view: *mut std::ffi::c_void,
    redo: bool,
) -> bool {
    let async_app = GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        let targets = targets.borrow();
        targets
            .get(&(gpui_root_view as usize))
            .or_else(|| targets.values().next())
            .map(|target| target.async_app.clone())
    });
    let Some(async_app) = async_app else {
        return false;
    };
    async_app
        .foreground_executor()
        .spawn({
            let async_app = async_app.clone();
            async move {
                async_app.update(|cx| {
                    let Some(window) = cx.active_window() else {
                        return;
                    };
                    let _ = window.update(cx, |_, window, cx| {
                        let action: Box<dyn gpui::Action> = if redo {
                            Box::new(gpui_component::input::Redo)
                        } else {
                            Box::new(gpui_component::input::Undo)
                        };
                        window.dispatch_action(action, cx);
                    });
                });
            }
        })
        .detach();
    true
}
