// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the macOS menu bar status item native
// owner bridging, power-events callback registration, and queued native
// callbacks. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_native_project_owner(
    project: &GpuiStatusIndicatorProjectState,
) -> Option<GpuiMenuBarStatusNativeProjectOwner> {
    let project_id = std::ffi::CString::new(project.project_id.as_str()).ok()?;
    let title = std::ffi::CString::new(project.title.as_str()).ok()?;
    let sessions = project
        .sessions
        .iter()
        .filter_map(gpui_menu_bar_status_native_session_owner)
        .collect::<Vec<_>>();
    let entries = sessions
        .iter()
        .map(|session| session.entry)
        .collect::<Vec<_>>();
    Some(GpuiMenuBarStatusNativeProjectOwner {
        project_id,
        title,
        sessions,
        entries,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_native_session_owner(
    session: &GpuiStatusIndicatorSessionState,
) -> Option<GpuiMenuBarStatusNativeSessionOwner> {
    let session_id = std::ffi::CString::new(session.session_id.as_str()).ok()?;
    let title = std::ffi::CString::new(session.title.as_str()).ok()?;
    let last_active_at = session
        .last_active_at
        .as_ref()
        .and_then(|timestamp| std::ffi::CString::new(timestamp.as_str()).ok());
    let entry = GpuiMenuBarStatusNativeSessionEntry {
        session_id: session_id.as_ptr(),
        title: title.as_ptr(),
        last_active_at: last_active_at
            .as_ref()
            .map_or(std::ptr::null(), |timestamp| timestamp.as_ptr()),
        status: gpui_menu_bar_status_native_kind(session.status),
        order: session.order,
    };
    Some(GpuiMenuBarStatusNativeSessionOwner {
        session_id,
        title,
        last_active_at,
        entry,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_native_kind(status: GpuiStatusIndicatorStatus) -> i32 {
    match status {
        GpuiStatusIndicatorStatus::Attention => 0,
        GpuiStatusIndicatorStatus::Working => 1,
        GpuiStatusIndicatorStatus::Available => 2,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn hide_gpui_menu_bar_status_item() {
    unsafe {
        GhostexGpuiHideMenuBarStatusItem();
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn hide_gpui_menu_bar_status_item() {}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_workspace_power_events_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_WORKSPACE_POWER_EVENTS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiWorkspacePowerEventsCallbackTarget { app, async_app });
    });
    unsafe {
        GhostexGpuiInstallWorkspacePowerEventsMonitor();
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_workspace_power_events_callback_target() {
    unsafe {
        GhostexGpuiRemoveWorkspacePowerEventsMonitor();
    }
    GPUI_WORKSPACE_POWER_EVENTS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_workspace_power_events_callback_target() -> Option<GpuiWorkspacePowerEventsCallbackTarget> {
    GPUI_WORKSPACE_POWER_EVENTS_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_workspace_terminal_escape_pressed(native_view: *mut std::ffi::c_void) {
    let Some(target) = gpui_terminal_key_event_callback_target_for_native_view(native_view) else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    let native_view = native_view as usize;
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.dispatch_gpui_workspace_terminal_escape_pressed_for_native_view(
                    native_view as *mut std::ffi::c_void,
                    cx,
                );
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_terminal_prompt_editor_shortcut(native_view: *mut std::ffi::c_void) -> bool {
    let Some(target) = gpui_terminal_key_event_callback_target_for_native_view(native_view) else {
        return false;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    let native_view = native_view as usize;
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.handle_native_terminal_prompt_editor_shortcut(
                    native_view as *mut std::ffi::c_void,
                    cx,
                );
            });
        })
        .detach();
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_os_integration_urls(urls: Vec<String>) {
    let Some(target) = gpui_os_integration_callback_target() else {
        GPUI_PENDING_OS_INTEGRATION_URLS.with(|pending| pending.borrow_mut().extend(urls));
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                this.receive_gpui_os_integration_urls(urls, window, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_menu_bar_status_project_click(project_id: String) {
    let Some(target) = gpui_menu_bar_status_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                window.activate_window();
                this.dispatch_gpui_menu_bar_project_activation(project_id.as_str(), cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_menu_bar_status_session_click(project_id: String, session_id: String) {
    let Some(target) = gpui_menu_bar_status_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                window.activate_window();
                this.dispatch_gpui_menu_bar_session_activation(
                    project_id.as_str(),
                    session_id.as_str(),
                    cx,
                );
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_workspace_did_wake() {
    let Some(target) = gpui_workspace_power_events_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.validate_gpui_remote_gxserver_connections(true, cx);
            });
        })
        .detach();
}

