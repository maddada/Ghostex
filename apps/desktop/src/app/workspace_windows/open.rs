//! Opening a workspace window: the ones the app reopens at launch and File > New Window.
//!
//! CDXC:AppWindows 2026-10-01 DECISION:
//! User: "two windows of the Ghostex app be able to launch simultaneously by clicking on New Window", so Ghostex can be used on two or more monitors or Spaces at once (an agent testing the app in one window while the user works in another); any number of windows, every UX choice left to the agent.
//! Each window is its own `GhostexGpuiApp` in this one process, with its own gxserver socket, sidebar focus, open views, panes, terminals, chats and saved layout; settings, themes, hotkeys, projects, sessions, the chat host and the web runtime are shared. A New Window opens on the project of the window it came from with no session open, cascaded from it on the same display and never full screen, so it does not take a new Space and does not resize a terminal the other window shows. The same session can still be opened in both windows: each attaches its own zmx client, and zmx already gives the grid to whichever client last claimed it visible or typed into it.
//! SEE-ALSO: apps/desktop/src/main.rs (opens the saved windows at launch), apps/desktop/src/app/core.rs (`Drop`), apps/desktop/src/app/app_new.rs.

use gpui::{App, AppContext as _, WindowBounds, WindowOptions, px, size};
use gpui_component::Root;

use super::close::install_workspace_window_close_handler;
use super::registry::{
    active_workspace_window, lead_workspace_window, note_workspace_window_activated,
    register_workspace_window,
};
use super::slots::*;
use crate::app::helpers::*;
use crate::*;

/// How far a New Window opens from the window it was opened from, down and to the right.
const NEW_WINDOW_CASCADE_OFFSET: f32 = 28.0;

/// How a workspace window starts.
pub(crate) enum WorkspaceWindowStart {
    /// A window the app reopens at launch from its saved slot; the first one leads.
    Restore { slot: u32, lead: bool },
    /// File > New Window: a fresh slot, on the project and workspace of the window it was opened
    /// from, or on the workspace the workspace tile's menu chose.
    New {
        slot: u32,
        active_project_id: Option<String>,
        workspace_id: Option<String>,
    },
}

impl WorkspaceWindowStart {
    pub(crate) fn slot(&self) -> u32 {
        match self {
            Self::Restore { slot, .. } | Self::New { slot, .. } => *slot,
        }
    }

    pub(crate) fn lead(&self) -> bool {
        matches!(self, Self::Restore { lead: true, .. })
    }
}

/// The window options every workspace window opens with. `display_id` must come with the bounds:
/// the macOS window opener resolves bounds against it (CDXC:Workarea 2026-09-19 DECISION in
/// helpers/os_cli/app_state_persistence.rs).
pub(crate) fn workspace_window_options(
    window_bounds: WindowBounds,
    display_id: Option<gpui::DisplayId>,
) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(window_bounds),
        window_background: window_glass_background_appearance(),
        display_id,
        window_min_size: Some(size(
            px(GPUI_WINDOW_FRAME_MIN_WIDTH),
            px(GPUI_WINDOW_FRAME_MIN_HEIGHT),
        )),
        app_id: crate::gpui_platform_window_app_id(),
        icon: crate::gpui_platform_window_icon(),
        /*
        Linux draws the same integrated Ghostex titlebar and caption
        controls as Windows, so the X11 host must ask KWin (or another
        window manager) to remove its server-side frame. GPUI translates
        this request into the standard _MOTIF_WM_HINTS decoration hint.
        The rendered decoration mode remains authoritative: if the X11
        session cannot provide client decorations, Ghostex keeps the
        server frame and omits its own caption buttons.
        */
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui::WindowDecorations::Client),
        /*
        CDXC:Titlebar 2026-09-20 WHY:
        The lights are placed against whatever row owns the window's top-left corner, and that
        row is no longer the 28px titlebar this offset was measured for. It is the sidebar's
        35px Search row while the sidebar is docked and the 36px work area header while it is
        collapsed, and both centre their own contents near y = 18. At the old y = 8 the lights
        sat three and a half pixels above everything beside them in both states; 11.5 puts a
        12px light on that same centreline.
        */
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(TITLEBAR_PROJECT_LABEL_FALLBACK.into()),
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(11.0), px(11.5))),
        }),
        // See `window_drag_region`: the top band holds draggable tabs, so the app, not AppKit,
        // decides which drags there move the window.
        app_owns_titlebar_drag: true,
        ..Default::default()
    }
}

fn default_workspace_window_bounds(cx: &App) -> WindowBounds {
    WindowBounds::centered(size(px(1280.0), px(820.0)), cx)
}

/// Launch: every window open when the app last quit comes back where it was, the first one as the
/// lead and the key window; the others open behind it without taking focus.
pub(crate) fn open_saved_workspace_windows(cx: &mut App) {
    let mut lead_window = None;
    for (index, slot) in saved_workspace_window_slots().into_iter().enumerate() {
        let lead = index == 0;
        let (window_bounds, display_id) = match restored_workspace_window_bounds(slot, cx) {
            Some((window_bounds, display_id)) => (window_bounds, Some(display_id)),
            None => (default_workspace_window_bounds(cx), None),
        };
        let mut options = workspace_window_options(window_bounds, display_id);
        options.focus = lead;
        let opened =
            open_workspace_window(options, WorkspaceWindowStart::Restore { slot, lead }, cx);
        if lead {
            lead_window = Some(opened.expect("failed to open GPUI window"));
        }
    }
    write_workspace_windows_manifest();
    if let Some(lead_window) = lead_window {
        let _ = lead_window.update(cx, |_, window, _| window.activate_window());
    }
}

/// File > New Window: a window on the active window's project, cascaded from it.
pub(crate) fn open_new_workspace_window(cx: &mut App) {
    let source = active_workspace_window(cx);
    open_new_workspace_window_from(source, cx);
}

/// New Window from a command that knows which window it was used in (Quick Access, the sidebar's
/// More menu): the new window cascades from that one and opens on its project.
pub(crate) fn open_new_workspace_window_from(
    source: Option<(gpui::AnyWindowHandle, gpui::WeakEntity<GhostexGpuiApp>)>,
    cx: &mut App,
) {
    let workspace_id = source.as_ref().and_then(|(_, app)| {
        app.upgrade()
            .and_then(|app| app.read(cx).gx_store_window_workspace_id())
    });
    open_new_workspace_window_on(source, workspace_id, cx);
}

/// A New Window on `workspace_id` (`None` = the default workspace), cascaded from `source`. It
/// starts on `source`'s project only when it shows the same workspace; on another one it starts
/// the way switching to that workspace does (workspace_landing.rs).
pub(crate) fn open_new_workspace_window_on(
    source: Option<(gpui::AnyWindowHandle, gpui::WeakEntity<GhostexGpuiApp>)>,
    workspace_id: Option<String>,
    cx: &mut App,
) {
    let (frame, active_project_id) = source
        .and_then(|(handle, app)| {
            handle
                .update(cx, |_, window, cx| {
                    let frame = gpui_window_frame_state_from_window(window, cx);
                    let project = app.upgrade().and_then(|app| {
                        let app = app.read(cx);
                        app.shows_same_workspace_as(workspace_id.as_deref())
                            .then(|| {
                                app.sidebar_gxserver_presentation_focus_state
                                    .active_project_id
                                    .clone()
                            })
                            .flatten()
                    });
                    (frame, project)
                })
                .ok()
        })
        .unwrap_or((None, None));
    let (window_bounds, display_id) = cascaded_workspace_window_bounds(frame, cx)
        .map(|(window_bounds, display_id)| (window_bounds, Some(display_id)))
        .unwrap_or_else(|| (default_workspace_window_bounds(cx), None));
    let slot = allocate_workspace_window_slot();
    let start = WorkspaceWindowStart::New {
        slot,
        active_project_id,
        workspace_id,
    };
    match open_workspace_window(
        workspace_window_options(window_bounds, display_id),
        start,
        cx,
    ) {
        Ok(_) => write_workspace_windows_manifest(),
        Err(error) => support_logs::append(
            support_logs::GpuiSupportLog::HostLifecycle,
            "gpui.host.newWindowFailed",
            serde_json::json!({ "error": format!("{error:#}") }),
        ),
    }
}

/// The source frame moved down and to the right, kept on its display, and always windowed: a New
/// Window opened from a full-screen one should not take a new Space.
fn cascaded_workspace_window_bounds(
    frame: Option<GpuiWindowFrameState>,
    cx: &App,
) -> Option<(WindowBounds, gpui::DisplayId)> {
    let mut frame = frame?;
    frame.state = "windowed".to_string();
    frame.relative_origin_x += NEW_WINDOW_CASCADE_OFFSET;
    frame.relative_origin_y += NEW_WINDOW_CASCADE_OFFSET;
    restored_gpui_window_bounds_from_state(
        frame,
        GPUI_WINDOW_FRAME_MIN_WIDTH,
        GPUI_WINDOW_FRAME_MIN_HEIGHT,
        cx,
    )
}

/// Opens one workspace window.
pub(crate) fn open_workspace_window(
    options: WindowOptions,
    start: WorkspaceWindowStart,
    cx: &mut App,
) -> anyhow::Result<gpui::WindowHandle<Root>> {
    let lead = start.lead();
    let slot = start.slot();
    let activate = options.focus;
    /*
    CDXC:CefRuntime 2026-06-14-13:10:
    CEF surfaces need an actual GPUI platform window before they attach native AppKit children. Create the GPUI window first, then let the CEF bridge wait for non-zero layout bounds before creating browser hosts. (CEF is optional since 2026-09-28: see app/helpers/web_runtime.rs.)

    CDXC:CefRuntime 2026-06-14-13:09:
    CEF startup must run after GPUI completes the first frame because initializing native Chromium children during root construction can stall the GPUI launch path without producing helper processes. The first frame only installs the listener that starts CEF when a web view is shown, then explicitly refreshes the window so the sidebar and browser elements enter the normal GPUI layout pass.
    */
    cx.open_window(options, move |window, cx| {
        if activate {
            window.activate_window();
        }
        let view = GhostexGpuiApp::new_workspace_window(window, &start, cx)
            .expect("failed to create Ghostex app");
        view.update(cx, |app, _| app.restore_window_workspace(&start));
        let window_handle = gpui::Window::window_handle(window);
        register_workspace_window(
            window_handle,
            view.downgrade(),
            slot,
            gpui_window_frame_state_from_window(window, cx),
        );
        if lead {
            register_ghostex_gpui_main_menu_actions(view.downgrade(), window_handle, cx);
        }
        if activate {
            note_workspace_window_activated(window, false, cx);
        } else {
            cx.defer(refresh_ghostex_gpui_main_menus);
        }
        let view_for_cef = view.clone();
        window.on_next_frame(move |window, cx| {
            view_for_cef.update(cx, |app, cx| {
                #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
                app.begin_deferred_cef_startup(cx);
                #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
                app.initialize_cef(cx);
            });
            window.refresh();
        });
        install_workspace_window_close_handler(view.downgrade(), window, cx);
        view.update(cx, |app, cx| {
            if lead {
                app.start_gpui_support_log_maintenance(cx);
                /*
                CDXC:ServerDaemon 2026-09-09 DECISION:
                User: do not show the "Loading sessions" toast when the app starts.
                */
                app.start_gpui_local_gxserver_bootstrap(false, cx);
            } else {
                // The lead starts gxserver; this window only connects to it.
                app.replay_sidebar_gxserver_bootstrap(cx);
                app.refresh_titlebar_accounts(cx);
                // The lead runs the updater; this window shows what it found (updater.rs).
                if let Some(lead) = lead_workspace_window(cx).and_then(|(_, lead)| lead.upgrade()) {
                    let lead = lead.read(cx);
                    let (available, downloading, progress) = (
                        lead.update_available,
                        lead.update_downloading,
                        lead.update_download_progress,
                    );
                    app.update_available = available;
                    app.update_downloading = downloading;
                    app.update_download_progress = progress;
                }
            }
            app.start_gpui_workspace_open_target_availability_scan(cx);
            if lead {
                app.start_gpui_updater(cx);
            }
            cx.on_app_quit(move |this, cx| {
                this.flush_gpui_quit_persistence(cx);
                persist_workspace_window_slot_frame(slot);
                async {}
            })
            .detach();
        });
        view.update(cx, |_, cx| {
            record_opened_workspace_window_frame(slot, window, cx);
            super::owned_windows::follow_owner_window_frame(window, cx);
            cx.observe_window_bounds(window, |app, window, cx| {
                app.main_window_bounds = window.bounds();
                app.main_window_display_id = window.display(cx).map(|display| display.id());
                super::owned_windows::follow_owner_window_frame(window, cx);
                /*
                macOS delivers bounds observer callbacks for window events
                that do not actually change the frame (e.g. key/order
                churn when a child panel opens). Close the anchored
                titlebar dropdown only when the frame genuinely moved or
                resized, otherwise every dropdown open self-closed within
                one frame.
                */
                let change = note_workspace_window_frame(window, cx);
                if change == WorkspaceWindowFrameChange::Resized {
                    app.close_floating_windows_on_resize(window, cx);
                }
                if change != WorkspaceWindowFrameChange::Unchanged {
                    app.close_gpui_titlebar_popup(None, window, cx);
                    app.schedule_gpui_new_thread_picker_recycle(cx);
                }
            })
            .detach();
            cx.observe_window_activation(window, |app, window, cx| {
                if !window.is_window_active() {
                    app.close_gpui_titlebar_popup(None, window, cx);
                    /*
                    CDXC:Sidebar 2026-08-02:
                    Pointer-moved events stop arriving once the window is
                    no longer active, so the last crossing the observer saw
                    may have been an enter. Report the pointer as outside
                    and close any open sidebar context menu, the same way
                    leaving for another app closes a native menu.

                    CDXC:Sidebar 2026-08-20:
                    Route the "outside" report through the AppKit observer
                    instead of writing the page flag here. This used to
                    call `dispatch_gpui_sidebar_pointer_inside(false)`
                    directly, which left the observer's cache saying
                    "inside" while the page said "false"; the next real
                    crossing back into the sidebar then matched the cache
                    and was dropped as redundant, so hovering a session row
                    showed neither the row background nor the hover-only
                    Close button until the pointer left the sidebar and
                    came back. Clicking a tab in the tab strip churns window
                    activation, which is why that click was the reliable way
                    to get into the broken state.
                    */
                    #[cfg(target_os = "macos")]
                    {
                        cef::report_sidebar_pointer_outside();
                        app.dispatch_gpui_sidebar_dismiss_context_menus(cx);
                    }
                    #[cfg(not(target_os = "macos"))]
                    app.dismiss_native_sidebar_menu(cx);
                } else {
                    #[cfg(target_os = "macos")]
                    let source_focus = app.source_workarea_cef_menu_passthrough_active;
                    #[cfg(not(target_os = "macos"))]
                    let source_focus = false;
                    note_workspace_window_activated(window, source_focus, cx);
                    /*
                    CDXC:Sidebar 2026-08-20:
                    Coming back active is the other half: the pointer can
                    already be sitting on a session row, and a pointer that
                    does not move produces no event to recompute from, so
                    resolve the crossing from the real pointer location.
                    */
                    #[cfg(target_os = "macos")]
                    cef::refresh_sidebar_pointer_inside();
                }
            })
            .detach();
        });
        cx.new(|cx| {
            // Transparent rather than unset, which would paint the theme's opaque
            // background: the app view's root paints the window's fill and follows window
            // glass, which this style (set once, here) could not.
            let root = Root::new(view, window, cx).bg(gpui::transparent_black());
            /*
            Ghostex owns an exact, non-overlapping Linux resize frame
            inside its main view. Disable gpui-component's generic
            shadow overlay there so resize input never extends across
            the workspace or embedded CEF children.
            */
            #[cfg(target_os = "linux")]
            let root = root.bordered(false);
            root
        })
    })
}
