#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![cfg_attr(not(any(target_os = "windows", target_os = "linux")), allow(dead_code))]

//! The release entry point (`Ghostex.exe` on Windows, `Ghostex` on Linux) that starts the real app,
//! `ghostex-gpui-runtime`.
//!
//! CDXC:CefRuntime 2026-09-28 WHY:
//! The web runtime (CEF) is an optional install (app/helpers/web_runtime.rs). On Windows the
//! runtime delay-loads libcef.dll and installs CEF itself when the user asks, so this launcher
//! only starts it. Linux still links libcef.so at load time, so there this launcher keeps
//! installing the verified component first and puts it on the loader path.

#[cfg(target_os = "windows")]
#[path = "../windows_cli_link.rs"]
mod windows_cli_link;
#[cfg(target_os = "windows")]
#[path = "../windows_updater.rs"]
mod windows_updater;
#[cfg(target_os = "windows")]
#[path = "../windows_url_scheme.rs"]
mod windows_url_scheme;

#[cfg(target_os = "linux")]
#[path = "../cef_component_window.rs"]
mod cef_component_window;
#[cfg(target_os = "linux")]
#[path = "../cef_library_version.rs"]
mod cef_library_version;
#[cfg(target_os = "linux")]
#[path = "../component_store/mod.rs"]
mod component_store;
#[cfg(target_os = "linux")]
#[path = "../linux_x11_scale.rs"]
mod linux_x11_scale;

#[cfg(target_os = "linux")]
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::sync::OnceLock;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(target_os = "linux")]
use futures::{StreamExt as _, channel::mpsc};
#[cfg(target_os = "linux")]
use gpui::prelude::FluentBuilder as _;
#[cfg(target_os = "linux")]
use gpui::{
    App, AppContext as _, Entity, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    MouseUpEvent, Render, Window, WindowBounds, WindowHandle, WindowOptions, div, px, relative,
    rgb, size,
};
#[cfg(target_os = "linux")]
use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};

#[cfg(target_os = "linux")]
struct GhostexGpuiApp {
    cef_component_window: Option<WindowHandle<cef_component_window::GpuiCefComponentWindow>>,
    cef_component_install_generation: u64,
}

/*
CDXC:PlatformSupport 2026-08-09:
Context::spawn deliberately gives asynchronous work only a WeakEntity. Keep
the bootstrap controller strongly owned by GPUI global state until the process
quits; otherwise the local Entity created in main is released after the run
callback, leaving a completed CEF install unable to update its window or launch
the real runtime.
*/
#[cfg(target_os = "linux")]
struct BootstrapAppEntity {
    _entity: Entity<GhostexGpuiApp>,
}

#[cfg(target_os = "linux")]
impl gpui::Global for BootstrapAppEntity {}

/// The real app next to this launcher.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn runtime_executable() -> Result<PathBuf, String> {
    let executable_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .ok_or_else(|| "Ghostex could not resolve its installed runtime directory.".to_string())?;
    #[cfg(target_os = "windows")]
    let runtime_executable = executable_dir.join("ghostex-gpui-runtime.exe");
    #[cfg(target_os = "linux")]
    let runtime_executable = executable_dir.join("ghostex-gpui-runtime");
    if !runtime_executable.is_file() {
        return Err(format!(
            "The installed Ghostex runtime is missing at {}.",
            runtime_executable.display()
        ));
    }
    Ok(runtime_executable)
}

/// Starts the runtime with this launcher's arguments.
#[cfg(target_os = "windows")]
fn launch_windows_runtime() -> Result<(), String> {
    let mut command = Command::new(runtime_executable()?);
    command.args(env::args_os().skip(1)).stdin(Stdio::null());
    if let Some(user_profile) = env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    {
        /*
        CDXC:PlatformSupport 2026-08-09:
        Velopack starts Ghostex with its installed `current` directory as
        the working directory. The real runtime launches long-lived WSL
        processes, so inheriting that directory keeps it open after the UI
        exits and prevents uninstall from removing the install root. Give
        the runtime a stable per-user working directory before it creates
        any descendants; explicit terminal/project cwd handling remains in
        the Windows terminal backend.
        */
        command.current_dir(user_profile);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Ghostex could not start: {error}"))
}

#[cfg(target_os = "windows")]
fn show_windows_launch_failure(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let wide = |text: &str| {
        text.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let text = wide(message);
    let caption = wide("Ghostex");
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(target_os = "linux")]
impl GhostexGpuiApp {
    fn initialize_cef(&mut self, cx: &mut gpui::Context<Self>) {
        cef_component_window::configure_cef_framework_path_for_process();
        let Some(runtime_dir) =
            env::var_os(cef_component_window::CEF_RUNTIME_DIR_ENV).map(PathBuf::from)
        else {
            self.show_cef_startup_failure(
                "The verified Chromium runtime path is unavailable after installation.".to_string(),
                cx,
            );
            return;
        };
        let runtime_executable = match runtime_executable() {
            Ok(path) => path,
            Err(message) => {
                self.show_cef_startup_failure(message, cx);
                return;
            }
        };

        let mut command = Command::new(&runtime_executable);
        command
            .args(env::args_os().skip(1))
            .env(cef_component_window::CEF_RUNTIME_DIR_ENV, &runtime_dir)
            .stdin(Stdio::null());
        let loader_path_variable = "LD_LIBRARY_PATH";
        let mut loader_paths = vec![runtime_dir.as_os_str().to_owned()];
        if let Some(paths) = env::var_os(loader_path_variable) {
            loader_paths.extend(env::split_paths(&paths).map(|path| path.into_os_string()));
        }
        let loader_path = match env::join_paths(loader_paths) {
            Ok(path) => path,
            Err(error) => {
                self.show_cef_startup_failure(
                    format!("Ghostex could not configure the Chromium loader path: {error}"),
                    cx,
                );
                return;
            }
        };
        command.env(loader_path_variable, loader_path);
        // The runtime makes its own X11 choice and keeps the Wayland socket for its terminals.
        if let Some(wayland_display) = linux_inherited_wayland_display() {
            command.env("WAYLAND_DISPLAY", wayland_display);
        }
        if linux_x11_scale::pinned_scale_factor().is_some() {
            command.env_remove("GPUI_X11_SCALE_FACTOR");
        }

        use std::os::unix::process::CommandExt as _;
        let error = command.exec();
        self.show_cef_startup_failure(
            format!("Ghostex could not start its verified Chromium runtime: {error}"),
            cx,
        );
    }
}

#[cfg(target_os = "linux")]
fn gpui_platform_window_app_id() -> Option<String> {
    Some("ghostex".to_string())
}

#[cfg(target_os = "linux")]
fn gpui_platform_window_icon() -> Option<Arc<image::RgbaImage>> {
    static ICON: OnceLock<Arc<image::RgbaImage>> = OnceLock::new();
    Some(
        ICON.get_or_init(|| {
            Arc::new(
                image::load_from_memory_with_format(
                    include_bytes!("../../resources/AppIcon.appiconset/icon_256x256.png"),
                    image::ImageFormat::Png,
                )
                .expect("the embedded Ghostex bootstrap icon must be a valid PNG")
                .into_rgba8(),
            )
        })
        .clone(),
    )
}

#[cfg(target_os = "linux")]
fn on_demand_component_manifest_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("GHOSTEX_ON_DEMAND_MANIFEST").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let executable = env::current_exe().ok()?;
    let executable_dir = executable.parent()?;
    [
        executable_dir.join("resources/on-demand-resources.json"),
        executable_dir.join("resources/Web/on-demand-resources.json"),
        executable_dir.join("on-demand-resources.json"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

#[cfg(target_os = "linux")]
fn on_demand_component_store() -> Result<Option<component_store::ComponentStore>, String> {
    let Some(manifest_path) = on_demand_component_manifest_path() else {
        return Ok(None);
    };
    let manifest = component_store::OnDemandManifest::load(&manifest_path)?;
    component_store::ComponentStore::from_manifest(manifest).map(Some)
}

#[cfg(target_os = "windows")]
fn main() {
    windows_updater::run_startup_hooks();
    if let Err(message) = launch_windows_runtime() {
        show_windows_launch_failure(&message);
    }
}

#[cfg(target_os = "linux")]
static LINUX_INHERITED_WAYLAND_DISPLAY: OnceLock<Option<String>> = OnceLock::new();

/// The Wayland socket this launcher started with, before `force_gpui_x11_backend` hid it.
#[cfg(target_os = "linux")]
fn linux_inherited_wayland_display() -> Option<&'static str> {
    LINUX_INHERITED_WAYLAND_DISPLAY
        .get()
        .and_then(|value| value.as_deref())
}

/// CDXC:PlatformSupport 2026-10-28 WHY:
/// The setup window ("Preparing Ghostex") was the one Ghostex window drawn as a native Wayland toplevel: this launcher never hid `WAYLAND_DISPLAY` the way the runtime does (`force_gpui_x11_backend_for_windowed_cef` in main.rs), and GPUI's Wayland backend gives a fixed-size window no equal min/max size, so Hyprland tiled it across the whole workspace on the first run. As an X11 client like the rest of the app it carries the fixed-size hints tiling window managers float (CDXC:PlatformSupport 2026-09-24 in the X11 window code) and the same pinned scale. Without an X11 display it stays on Wayland, where it can still show why the runtime will not start.
#[cfg(target_os = "linux")]
fn force_gpui_x11_backend() {
    let inherited = env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|value| !value.is_empty());
    let _ = LINUX_INHERITED_WAYLAND_DISPLAY.set(inherited);
    if !env::var("DISPLAY").is_ok_and(|value| !value.is_empty()) {
        return;
    }
    // SAFETY: the first statement of main(), before GPUI or any other thread reads the environment.
    unsafe { env::remove_var("WAYLAND_DISPLAY") };
    linux_x11_scale::pin_gpui_x11_scale_factor();
}

/// CDXC:PlatformSupport 2026-10-22 WHY:
/// The setup window is drawn from plain GPUI elements (cef_component_window.rs), so this launcher neither imports nor initializes gpui-component. Using it for one `v_flex()` linked the whole widget kit and every tree-sitter grammar into the Linux `Ghostex` launcher: 77.5 MB on arm64 (82 MB on x64) with it, 29.5 MB without.
#[cfg(target_os = "linux")]
fn main() {
    force_gpui_x11_backend();
    let application = gpui_platform::application();
    application.run(|cx| {
        let app = cx.new(|_| GhostexGpuiApp {
            cef_component_window: None,
            cef_component_install_generation: 0,
        });
        app.update(cx, |app, cx| app.begin_cef_startup(cx));
        cx.set_global(BootstrapAppEntity { _entity: app });
    });
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn main() {}
