pub(in crate::app::native_chat) fn prepare(window: &mut gpui::Window) {
    #[cfg(all(target_os = "macos", not(ghostex_web)))]
    {
        use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
        unsafe extern "C" {
            fn GhostexGpuiPrepareChatDialogWindow(view: *mut std::ffi::c_void);
        }
        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::AppKit(handle) = handle.as_raw()
        {
            unsafe { GhostexGpuiPrepareChatDialogWindow(handle.ns_view.as_ptr()) };
        }
    }
    #[cfg(not(all(target_os = "macos", not(ghostex_web))))]
    let _ = window;
}
