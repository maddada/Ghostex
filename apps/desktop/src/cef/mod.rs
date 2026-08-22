/*
CDXC:GPUICefPlatformSeam 2026-07-04:
Windowed-CEF module layout: `shell` owns every OS-agnostic piece (runtime
init/shutdown, app/client/bridge handlers, the CefBrowser wrapper) and calls
into exactly one `platform` module for the truly per-OS glue (framework
loading, message-pump scheduling, child-view frame/visibility/focus,
child WindowInfo construction). Each supported OS provides that module via
`#[path]` so shared code never branches on target_os. OSes without an
adapter keep the explicit `unsupported` stub instead of a fallback backend.
*/
pub(crate) mod sidebar_bridge_manifest;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub(crate) mod shell;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;
// Linux is X11-only for v1: CEF child windows require an X11 host window,
// so the adapter (and the whole app) targets X11/XWayland; see linux_x11.rs.
#[cfg(target_os = "linux")]
#[path = "linux_x11.rs"]
mod platform;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use shell::*;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod unsupported;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub use unsupported::*;
