//! The open workspace windows, oldest first, and the one of them that runs the app-wide work.
//!
//! CDXC:AppWindows 2026-10-01 WHY:
//! The app object was written as the only one, so the work that must happen once per process stays with one lead window: the menu bar status item, notification banners and completion sounds, the updater, Keep Awake (a hold started from any window runs here, every window's More menu shows it, and it moves to the next lead), Ghostex Capture, the first-run prompts and the agent settings reconciliation. The first window leads; when the lead closes, the oldest remaining window takes the work over, and the app quits only when the last workspace window closes. A lead term, not a flag, marks the lead, so a closed lead whose app object is dropped late cannot unregister what its successor registered.

use std::cell::{Cell, RefCell};

use gpui::{AnyWindowHandle, App, Context, Entity, WeakEntity, Window, WindowId};

use super::slots::{forget_workspace_window_slot, persist_workspace_window_frame_now};
use crate::app::helpers::*;
use crate::*;

pub(super) struct WorkspaceWindow {
    /// The window's number for the Window menu's rows, unique for the run.
    pub(super) number: u64,
    pub(super) handle: AnyWindowHandle,
    pub(super) app: WeakEntity<GhostexGpuiApp>,
    /// Which saved layout, focus and frame files this window reads and writes (slots.rs).
    pub(super) slot: u32,
    /// The frame this window last reported, so a bounds callback that moved nothing (macOS sends
    /// them for key and order churn) is ignored, and so its slot's frame file can be written.
    pub(super) frame: Option<GpuiWindowFrameState>,
    /// The title last given to the window.
    pub(super) title: String,
    /// What the Window menu calls it: its project's name.
    pub(super) label: String,
    /// The user asked to close it and it is finishing (close.rs): its slot is already forgotten,
    /// so a quit in the meantime neither lists it nor writes its files.
    pub(super) closing: bool,
}

/// One row of the Window menu's list of windows.
pub(crate) struct WorkspaceWindowMenuRow {
    pub(crate) label: String,
    pub(crate) number: u64,
    pub(crate) active: bool,
}

thread_local! {
    pub(super) static WORKSPACE_WINDOWS: RefCell<Vec<WorkspaceWindow>> = const { RefCell::new(Vec::new()) };
    /// Bumped each time a window becomes the lead; only the window holding the current term leads.
    static LEAD_TERM: Cell<u64> = const { Cell::new(0) };
    static NEXT_WINDOW_NUMBER: Cell<u64> = const { Cell::new(1) };
    /// The workspace window that was last the active one, which the Window menu ticks.
    static ACTIVE_WINDOW_NUMBER: Cell<Option<u64>> = const { Cell::new(None) };
    /// The lead's Keep Awake period in minutes, which every window's More menu ticks.
    static APP_KEEP_AWAKE_MINUTES: Cell<Option<i64>> = const { Cell::new(None) };
    /// A closing lead's Keep Awake hold, for the window that takes over.
    static HANDED_OVER_KEEP_AWAKE: RefCell<Option<GpuiKeepAwakeRuntime>> = const { RefCell::new(None) };
}

/// The app's Keep Awake period, held by the lead window.
pub(crate) fn app_keep_awake_minutes() -> Option<i64> {
    APP_KEEP_AWAKE_MINUTES.get()
}

/// A closing lead's hold, which the next lead keeps running (close.rs).
pub(super) fn hand_over_app_keep_awake(runtime: Option<GpuiKeepAwakeRuntime>) {
    if runtime.is_some() {
        HANDED_OVER_KEEP_AWAKE.with(|handed_over| *handed_over.borrow_mut() = runtime);
    }
}

fn begin_lead_term() -> u64 {
    let term = LEAD_TERM.get() + 1;
    LEAD_TERM.set(term);
    term
}

/// What `on_window_closed` does after a window went away.
pub(crate) enum WorkspaceWindowClosed {
    /// Not a workspace window (a modal, toast or popup).
    NotWorkspace,
    /// The last workspace window closed: the app quits, and the window reopens at the next launch.
    Last,
    /// Other workspace windows are still open.
    OthersRemain,
}

pub(super) fn register_workspace_window(
    handle: AnyWindowHandle,
    app: WeakEntity<GhostexGpuiApp>,
    slot: u32,
    frame: Option<GpuiWindowFrameState>,
) {
    let number = NEXT_WINDOW_NUMBER.get();
    NEXT_WINDOW_NUMBER.set(number + 1);
    WORKSPACE_WINDOWS.with(|windows| {
        windows.borrow_mut().push(WorkspaceWindow {
            number,
            handle,
            app,
            slot,
            frame,
            title: String::new(),
            label: TITLEBAR_PROJECT_LABEL_FALLBACK.to_string(),
            closing: false,
        });
    });
}

/// The Window menu's list, oldest window first. Windows on the same project are numbered so the
/// rows can be told apart.
pub(crate) fn workspace_window_menu_rows() -> Vec<WorkspaceWindowMenuRow> {
    let active = ACTIVE_WINDOW_NUMBER.get();
    WORKSPACE_WINDOWS.with(|windows| {
        let windows = windows.borrow();
        windows
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let same_label_before = windows[..index]
                    .iter()
                    .filter(|other| other.label == entry.label)
                    .count();
                let same_label_total = windows
                    .iter()
                    .filter(|other| other.label == entry.label)
                    .count();
                let label = if same_label_total > 1 {
                    format!("{} {}", entry.label, same_label_before + 1)
                } else {
                    entry.label.clone()
                };
                WorkspaceWindowMenuRow {
                    label,
                    number: entry.number,
                    active: Some(entry.number) == active,
                }
            })
            .collect()
    })
}

/// The Window menu's row for `number`: brings that window to the front.
pub(crate) fn activate_workspace_window(number: u64, cx: &mut App) {
    let handle = WORKSPACE_WINDOWS.with(|windows| {
        windows
            .borrow()
            .iter()
            .find(|entry| entry.number == number)
            .map(|entry| entry.handle)
    });
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    }
}

/// Window > Cycle Through Windows (Cmd+`): the next workspace window, oldest first, wrapping.
pub(crate) fn activate_next_workspace_window(cx: &mut App) {
    let active = cx
        .active_window()
        .map(|window| window.window_id())
        .and_then(|active| {
            WORKSPACE_WINDOWS.with(|windows| {
                windows
                    .borrow()
                    .iter()
                    .find(|entry| entry.handle.window_id() == active)
                    .map(|entry| entry.number)
            })
        })
        .or(ACTIVE_WINDOW_NUMBER.get());
    let next = WORKSPACE_WINDOWS.with(|windows| {
        let windows = windows.borrow();
        let index = active
            .and_then(|active| windows.iter().position(|entry| entry.number == active))
            .map_or(0, |index| (index + 1) % windows.len().max(1));
        windows.get(index).map(|entry| entry.number)
    });
    if let Some(next) = next {
        activate_workspace_window(next, cx);
    }
}

/// A workspace window became the active one: the Window menu ticks it, and the menu bar takes
/// that window's own variant (`source_focus`: its Code editor has the keyboard, so Cmd+W and the
/// Edit keys go to the editor), which the window it replaced may have set differently.
pub(super) fn note_workspace_window_activated(window: &Window, source_focus: bool, cx: &mut App) {
    let window_id = gpui::Window::window_handle(window).window_id();
    let number = WORKSPACE_WINDOWS.with(|windows| {
        windows
            .borrow()
            .iter()
            .find(|entry| entry.handle.window_id() == window_id)
            .map(|entry| entry.number)
    });
    if number.is_none() {
        return;
    }
    // Activation churns on tab clicks, modal closes and Cmd+Tab; the menu bar is rebuilt only when
    // the tick or the menu variant really changes.
    if number == ACTIVE_WINDOW_NUMBER.get() && source_focus == installed_main_menus_source_focus() {
        return;
    }
    ACTIVE_WINDOW_NUMBER.set(number);
    cx.defer(move |cx| set_ghostex_gpui_main_menus(source_focus, cx));
}

/// The slots the next launch reopens: the open windows', oldest first, without one that is
/// closing.
pub(super) fn open_workspace_window_slots() -> Vec<u32> {
    WORKSPACE_WINDOWS.with(|windows| {
        windows
            .borrow()
            .iter()
            .filter(|entry| !entry.closing)
            .map(|entry| entry.slot)
            .collect()
    })
}

/// Every slot an open window holds, a closing one's included, which a new window must not take:
/// the closing window's slot is cleared once more when it finally goes.
pub(super) fn used_workspace_window_slots() -> Vec<u32> {
    WORKSPACE_WINDOWS.with(|windows| windows.borrow().iter().map(|entry| entry.slot).collect())
}

/// The user is closing `window_id` while another window stays open. Its slot is forgotten now
/// rather than when it goes: a quit while it finishes (its remote Action closes can take seconds)
/// must not bring it back at the next launch with the tabs it was closing.
pub(super) fn note_workspace_window_closing(window_id: WindowId) {
    let slot = WORKSPACE_WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        let entry = windows
            .iter_mut()
            .find(|entry| entry.handle.window_id() == window_id && !entry.closing)?;
        entry.closing = true;
        Some(entry.slot)
    });
    if let Some(slot) = slot {
        forget_workspace_window_slot(slot);
    }
}

/// The workspace window the user is in: the active window when it is one, else the lead, else
/// the oldest open one. Menu bar commands without a window of their own go here.
pub(crate) fn active_workspace_window(
    cx: &App,
) -> Option<(AnyWindowHandle, WeakEntity<GhostexGpuiApp>)> {
    let active = cx.active_window().map(|window| window.window_id());
    WORKSPACE_WINDOWS.with(|windows| {
        let windows = windows.borrow();
        let live = || windows.iter().filter(|entry| entry.app.upgrade().is_some());
        live()
            .find(|entry| Some(entry.handle.window_id()) == active)
            // A modal, Quick Access or a popup is the active window: the window it belongs to.
            .or_else(|| {
                let active = active?;
                live().find(|entry| {
                    entry
                        .app
                        .upgrade()
                        .is_some_and(|app| app.read(cx).owns_child_window(active))
                })
            })
            .or_else(|| {
                live().find(|entry| {
                    entry
                        .app
                        .upgrade()
                        .is_some_and(|app| app.read(cx).is_lead_window())
                })
            })
            .or_else(|| live().next())
            .map(|entry| (entry.handle, entry.app.clone()))
    })
}

/// The lead window, for the app-wide commands it owns (Check for Updates).
pub(crate) fn lead_workspace_window(
    cx: &App,
) -> Option<(AnyWindowHandle, WeakEntity<GhostexGpuiApp>)> {
    WORKSPACE_WINDOWS.with(|windows| {
        windows
            .borrow()
            .iter()
            .find(|entry| {
                entry
                    .app
                    .upgrade()
                    .is_some_and(|app| app.read(cx).is_lead_window())
            })
            .map(|entry| (entry.handle, entry.app.clone()))
    })
}

/// Another open workspace window whose app answers `holds`, for a click on something that lives in
/// one window's own state (a Browser page).
pub(super) fn other_workspace_window_where(
    except: gpui::EntityId,
    cx: &App,
    holds: impl Fn(&GhostexGpuiApp) -> bool,
) -> Option<(AnyWindowHandle, WeakEntity<GhostexGpuiApp>)> {
    WORKSPACE_WINDOWS.with(|windows| {
        windows
            .borrow()
            .iter()
            .filter(|entry| entry.app.entity_id() != except && !entry.closing)
            .find(|entry| entry.app.upgrade().is_some_and(|app| holds(app.read(cx))))
            .map(|entry| (entry.handle, entry.app.clone()))
    })
}

/// Every other open workspace window's app, for changes all windows must follow (a saved setting).
pub(crate) fn other_workspace_window_apps(except: gpui::EntityId) -> Vec<Entity<GhostexGpuiApp>> {
    WORKSPACE_WINDOWS.with(|windows| {
        windows
            .borrow()
            .iter()
            .filter(|entry| entry.app.entity_id() != except)
            .filter_map(|entry| entry.app.upgrade())
            .collect()
    })
}

/// The open window that shows the session behind `row_id` (focused or on screen), the one last
/// active first; `None` when no window shows it. `caller` is the asking window's app with its own
/// answer.
///
/// CDXC:AppWindows 2026-10-05 WHY:
/// The asking app is mid-update when it asks (a notification click, a menu bar row), and reading an entity that is being updated panics; on Windows that panic crossed the task wndproc and aborted Ghostex when an Attention toast was clicked. The caller answers for itself and only the other windows' apps are read.
pub(super) fn workspace_window_showing_session(
    row_id: &str,
    caller: (gpui::EntityId, bool),
    cx: &App,
) -> Option<(AnyWindowHandle, WeakEntity<GhostexGpuiApp>)> {
    let active = ACTIVE_WINDOW_NUMBER.get();
    let (caller_id, caller_shows) = caller;
    WORKSPACE_WINDOWS.with(|windows| {
        let windows = windows.borrow();
        let showing = windows
            .iter()
            .filter(|entry| !entry.closing)
            .filter(|entry| {
                if entry.app.entity_id() == caller_id {
                    return caller_shows;
                }
                entry
                    .app
                    .upgrade()
                    .is_some_and(|app| app.read(cx).gx_store_shows_session_row(row_id))
            })
            .collect::<Vec<_>>();
        showing
            .iter()
            .find(|entry| Some(entry.number) == active)
            .or_else(|| showing.first())
            .map(|entry| (entry.handle, entry.app.clone()))
    })
}

/// Whether `window_id` is an open workspace window (window glass, helpers/window_glass.rs).
pub(crate) fn is_workspace_window(window_id: WindowId) -> bool {
    WORKSPACE_WINDOWS.with(|windows| {
        windows.try_borrow().is_ok_and(|windows| {
            windows
                .iter()
                .any(|entry| entry.handle.window_id() == window_id)
        })
    })
}

/// Whether more than one workspace window is open.
pub(super) fn several_workspace_windows_open() -> bool {
    WORKSPACE_WINDOWS.with(|windows| windows.borrow().len() > 1)
}

/// The CEF demand signal starts the web runtime in every open window, so each creates the web views
/// it is waiting for once the runtime is ready (`cef::initialize` runs once).
pub(crate) fn request_cef_runtime_in_every_window(cx: &mut App) {
    let apps = WORKSPACE_WINDOWS.with(|windows| {
        windows
            .borrow()
            .iter()
            .map(|entry| entry.app.clone())
            .collect::<Vec<_>>()
    });
    for app in apps {
        let _ = app.update(cx, |app, cx| app.request_cef_runtime(cx));
    }
}

/// Names the window after its project, so Mission Control, the Dock menu and the Window menu
/// tell the windows apart. Called while the window draws, so the title is set on the next turn
/// rather than inside the draw; costs a comparison when nothing changed.
pub(crate) fn sync_workspace_window_title(window: &Window, project_name: &str, cx: &mut App) {
    let (title, label) =
        if project_name.is_empty() || project_name == TITLEBAR_PROJECT_LABEL_FALLBACK {
            (
                TITLEBAR_PROJECT_LABEL_FALLBACK.to_string(),
                TITLEBAR_PROJECT_LABEL_FALLBACK.to_string(),
            )
        } else {
            (
                format!("{project_name} - {TITLEBAR_PROJECT_LABEL_FALLBACK}"),
                project_name.to_string(),
            )
        };
    let window_id = gpui::Window::window_handle(window).window_id();
    let changed = WORKSPACE_WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        let Some(entry) = windows
            .iter_mut()
            .find(|entry| entry.handle.window_id() == window_id)
        else {
            return false;
        };
        if entry.title == title {
            return false;
        }
        entry.title = title.clone();
        entry.label = label;
        true
    });
    if changed {
        window.defer(cx, move |window, cx| {
            window.set_window_title(&title);
            refresh_ghostex_gpui_main_menus(cx);
        });
    }
}

/// A window closed. Forgets it when it was a workspace window: a window the user closed while
/// another stays open loses its saved slot, the last one keeps it for the next launch, and when
/// the closed window led, the oldest remaining one takes the app-wide work over.
pub(crate) fn workspace_window_closed(window_id: WindowId, cx: &mut App) -> WorkspaceWindowClosed {
    let (closed, remaining) = WORKSPACE_WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        let closed = windows
            .iter()
            .position(|entry| entry.handle.window_id() == window_id)
            .map(|index| windows.remove(index));
        let remaining = windows
            .iter()
            .map(|entry| (entry.handle, entry.app.clone()))
            .collect::<Vec<_>>();
        (closed, remaining)
    });
    let Some(closed) = closed else {
        return WorkspaceWindowClosed::NotWorkspace;
    };
    if ACTIVE_WINDOW_NUMBER.get() == Some(closed.number) {
        ACTIVE_WINDOW_NUMBER.set(None);
    }
    cx.defer(refresh_ghostex_gpui_main_menus);
    if remaining.is_empty() {
        persist_workspace_window_frame_now(closed.slot, closed.frame.as_ref());
        return WorkspaceWindowClosed::Last;
    }
    let _ = closed
        .app
        .update(cx, |app, _| app.workspace_window_closing = true);
    forget_workspace_window_slot(closed.slot);
    let lead_remains = remaining.iter().any(|(_, app)| {
        app.upgrade()
            .is_some_and(|app| app.read(cx).is_lead_window())
    });
    if !lead_remains {
        for (handle, app) in remaining {
            let took_over = handle
                .update(cx, |_, window, cx| {
                    app.update(cx, |app, cx| app.take_over_app_wide_duties(window, cx))
                        .is_ok()
                })
                .unwrap_or(false);
            if took_over {
                break;
            }
        }
    }
    WorkspaceWindowClosed::OthersRemain
}

impl GhostexGpuiApp {
    /// Whether this window runs the app-wide work (see the module comment).
    pub(crate) fn is_lead_window(&self) -> bool {
        self.lead_window_term
            .is_some_and(|term| term == LEAD_TERM.get())
    }

    /// Records the lead's Keep Awake period for every window's More menu. Called wherever the
    /// lead's hold starts or stops.
    pub(crate) fn publish_app_keep_awake(&self) {
        if self.is_lead_window() {
            APP_KEEP_AWAKE_MINUTES.set(
                self.keep_awake_runtime
                    .as_ref()
                    .map(|runtime| runtime.duration_minutes.minutes() as i64),
            );
        }
    }

    /// The lead term a new window starts with: a fresh one for the launch window, none otherwise.
    pub(crate) fn initial_lead_window_term(lead: bool) -> Option<u64> {
        lead.then(begin_lead_term)
    }

    /// Whether `window_id` is one of the windows this workspace window opened over itself: its
    /// app modal (Settings, Quick Access, the dialogs), the extension modal host, its toast, its
    /// titlebar popup (Tips, Resources, the menus), the plugins modal, the New Thread picker or
    /// the web runtime's install window.
    pub(crate) fn owns_child_window(&self, window_id: WindowId) -> bool {
        let owned = [
            self.app_modal_window.map(|handle| handle.window_id()),
            self.app_toast_window.map(|handle| handle.window_id()),
            self.titlebar_popup_window.map(|handle| handle.window_id()),
            self.new_thread_picker_window
                .map(|handle| handle.window_id()),
            self.native_app_modal
                .as_ref()
                .map(|modal| modal.window.window_id()),
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            self.cef_component_window
                .as_ref()
                .map(|handle| handle.window_id()),
        ];
        owned.contains(&Some(window_id))
    }

    /// Whether nothing else uses what this window's teardown would stop for the whole process:
    /// the app is quitting, or no other workspace window is open.
    pub(crate) fn is_last_workspace_window(&self) -> bool {
        if GPUI_APP_QUIT_IN_PROGRESS.load(std::sync::atomic::Ordering::Acquire) {
            return true;
        }
        let own = self.main_window_handle.map(|handle| handle.window_id());
        WORKSPACE_WINDOWS.with(|windows| {
            !windows
                .borrow()
                .iter()
                .any(|entry| Some(entry.handle.window_id()) != own && entry.app.upgrade().is_some())
        })
    }

    /// The process-wide callbacks only the lead receives: menu bar status item clicks, sidebar
    /// pointer crossings, notification clicks, Reduce Motion, power events, Sparkle and the
    /// operating system's URL and file opens.
    pub(crate) fn register_app_wide_callback_targets(&self, cx: &mut Context<Self>) {
        crate::app::browser_site_requests::register_browser_site_request_handler(cx);
        crate::app::voice_agent::typing::watch_typing(cx);
        #[cfg(target_os = "macos")]
        {
            register_gpui_menu_bar_status_callback_target(cx.weak_entity(), cx.to_async());
            register_gpui_sidebar_pointer_callback_target(cx.weak_entity(), cx.to_async());
            register_gpui_session_attention_notification_callback_target(
                cx.weak_entity(),
                cx.to_async(),
            );
            register_gpui_accessibility_display_options_callback_target(
                cx.weak_entity(),
                cx.to_async(),
            );
            register_gpui_workspace_power_events_callback_target(cx.weak_entity(), cx.to_async());
            register_gpui_sparkle_updater_callback_target(cx.weak_entity(), cx.to_async());
            register_gpui_os_integration_callback_target(cx.weak_entity(), cx.to_async());
        }
        crate::app::gx_store::note_app_started_for_session_links();
        #[cfg(not(target_os = "macos"))]
        register_gpui_os_integration_callback_target(cx.weak_entity(), cx.to_async());
        #[cfg(target_os = "windows")]
        register_gpui_windows_notification_click_target(cx);
        #[cfg(target_os = "linux")]
        register_gpui_linux_notification_click_target(cx);
    }

    /// The lead closed and this is the oldest remaining window: it runs the app-wide work from now
    /// on. Its own layout, focus and frame were already saved to its own slot.
    fn take_over_app_wide_duties(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.lead_window_term = Some(begin_lead_term());
        if let Some(runtime) = HANDED_OVER_KEEP_AWAKE.with(|handed_over| handed_over.take()) {
            match self.keep_awake_runtime.replace(runtime) {
                None => {}
                // Keeps the hold it had and stops the handed-over one.
                Some(own) => {
                    self.stop_gpui_keep_awake_runtime();
                    self.keep_awake_runtime = Some(own);
                }
            }
        }
        self.publish_app_keep_awake();
        self.register_app_wide_callback_targets(cx);
        self.initialize_ghostex_capture(cx);
        self.apply_gpui_menu_bar_status_item_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        // Sparkle starts once per process and answers the same again; this re-arms the periodic
        // availability probe the closed lead ran.
        self.start_gpui_updater(cx);
        cx.notify();
    }
}
