use std::sync::atomic::Ordering;

use gpui::App;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn set_ghostex_gpui_main_menus(source_workarea_cef_owns_native_focus: bool, cx: &App) {
    cx.set_menus(ghostex_gpui_main_menus_for_source_focus(
        source_workarea_cef_owns_native_focus,
    ));
    #[cfg(target_os = "macos")]
    cef::refresh_application_menu_hooks();
}

pub(crate) fn register_ghostex_gpui_main_menu_actions(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    main_window: gpui::AnyWindowHandle,
    cx: &mut App,
) {
    /*
    CDXC:GPUIMainMenuActions 2026-07-10:
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
            CDXC:GPUIMainMenuUpdater 2026-07-24:
            App-menu update checks dispatch while GPUI already owns the active
            window update. Defer the Sparkle handoff until that action cycle
            returns so the main window can be borrowed and Sparkle can present
            its standard user-initiated update or no-update UI.
            */
            let app = app.clone();
            cx.defer(move |cx| {
                let _ = main_window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| app.check_for_gpui_updates(window, cx));
                });
            });
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &OpenGpuiSettingsModal, cx| {
            /*
            CDXC:GPUIMainMenuSettings 2026-07-24:
            Native app-menu actions dispatch while GPUI already owns the active
            window update. Defer the Settings window mutation until that action
            cycle returns so the main window can be borrowed normally instead
            of silently rejecting a re-entrant update.
            */
            let app = app.clone();
            cx.defer(move |cx| {
                let _ = main_window.update(cx, |_, window, cx| {
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
        move |_: &OpenGpuiPluginsModal, cx| {
            let app = app.clone();
            cx.defer(move |cx| {
                let _ = main_window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.open_gpui_settings_plugins_page(Some(window), cx);
                    });
                });
            });
        }
    });
    cx.on_action(|_: &HideGhostexGpui, cx| cx.hide());
    cx.on_action(|_: &HideGhostexGpuiOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAllGhostexGpuiApps, cx| cx.unhide_other_apps());
    cx.on_action(|_: &QuitGhostexGpui, cx| {
        GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
        cx.quit();
    });
    cx.on_action(move |_: &MinimizeGhostexGpuiWindow, cx| {
        let _ = main_window.update(cx, |_, window, _cx| window.minimize_window());
    });
    cx.on_action(move |_: &ZoomGhostexGpuiWindow, cx| {
        let _ = main_window.update(cx, |_, window, _cx| window.zoom_window());
    });
}

/// Native app menu bar (macOS `installMainMenu` parity, AppDelegate.swift
/// :2533-2663): App (About/Check for Updates/Settings/Hide/Quit),
/// File → Close Pane ⌘W, the Edit clipboard set (first-responder OS actions so
/// CEF and Ghostty views handle them natively), and Window → Minimize/Zoom.
/// Undo/Redo are omitted from the GPUI-owned menu because gpui routes them
/// through app actions instead of first-responder selectors; the macOS CEF hook
/// installs them after each menu replacement.
pub(crate) fn ghostex_gpui_main_menus_for_source_focus(
    source_workarea_cef_owns_native_focus: bool,
) -> Vec<gpui::Menu> {
    use gpui::{Menu, MenuItem, OsAction};
    /*
    CDXC:GPUISourceViewHotkeyPassthrough 2026-07-05:
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
            MenuItem::action("Quit Ghostex", QuitGhostexGpui),
        ]),
        Menu::new("File").items(vec![close_pane_item]),
        Menu::new("Edit").items(vec![
            MenuItem::os_action("Cut", GpuiEditMenuCut, OsAction::Cut),
            MenuItem::os_action("Copy", GpuiEditMenuCopy, OsAction::Copy),
            MenuItem::os_action("Paste", GpuiEditMenuPaste, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action("Select All", GpuiEditMenuSelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Window").items(vec![
            MenuItem::action("Minimize", MinimizeGhostexGpuiWindow),
            MenuItem::action("Zoom", ZoomGhostexGpuiWindow),
        ]),
    ]
}
