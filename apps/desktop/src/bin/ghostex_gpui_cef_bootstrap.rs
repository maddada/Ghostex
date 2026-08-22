#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![cfg_attr(not(any(target_os = "windows", target_os = "linux")), allow(dead_code))]

#[cfg(target_os = "windows")]
#[path = "../windows_updater.rs"]
mod windows_updater;

#[cfg(any(target_os = "windows", target_os = "linux"))]
#[path = "../cef_component_window.rs"]
mod cef_component_window;
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[path = "../component_store.rs"]
mod component_store;

#[cfg(target_os = "linux")]
use std::sync::OnceLock;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

#[cfg(any(target_os = "windows", target_os = "linux"))]
use futures::{StreamExt as _, channel::mpsc};
#[cfg(any(target_os = "windows", target_os = "linux"))]
use gpui::prelude::FluentBuilder as _;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use gpui::{
    App, AppContext as _, Entity, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    MouseUpEvent, Render, Window, WindowBounds, WindowHandle, WindowOptions, div, px, relative,
    rgb, size,
};
#[cfg(any(target_os = "windows", target_os = "linux"))]
use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
#[cfg(any(target_os = "windows", target_os = "linux"))]
use gpui_component::v_flex;

#[cfg(any(target_os = "windows", target_os = "linux"))]
struct GhostexGpuiApp {
    cef_component_window: Option<WindowHandle<cef_component_window::GpuiCefComponentWindow>>,
    cef_component_install_generation: u64,
}

/*
CDXC:WindowsCefBootstrapLifetime 2026-08-09:
Context::spawn deliberately gives asynchronous work only a WeakEntity. Keep
the bootstrap controller strongly owned by GPUI global state until the process
quits; otherwise the local Entity created in main is released after the run
callback, leaving a completed CEF install unable to update its window or launch
the real runtime.
*/
#[cfg(any(target_os = "windows", target_os = "linux"))]
struct BootstrapAppEntity {
    _entity: Entity<GhostexGpuiApp>,
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
impl gpui::Global for BootstrapAppEntity {}

#[cfg(any(target_os = "windows", target_os = "linux"))]
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
        let executable_dir = match env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
        {
            Some(path) => path,
            None => {
                self.show_cef_startup_failure(
                    "Ghostex could not resolve its installed runtime directory.".to_string(),
                    cx,
                );
                return;
            }
        };
        #[cfg(target_os = "windows")]
        let runtime_executable = executable_dir.join("ghostex-gpui-runtime.exe");
        #[cfg(target_os = "linux")]
        let runtime_executable = executable_dir.join("ghostex-gpui-runtime");
        if !runtime_executable.is_file() {
            self.show_cef_startup_failure(
                format!(
                    "The installed Ghostex runtime is missing at {}.",
                    runtime_executable.display()
                ),
                cx,
            );
            return;
        }

        let mut command = Command::new(&runtime_executable);
        command
            .args(env::args_os().skip(1))
            .env(cef_component_window::CEF_RUNTIME_DIR_ENV, &runtime_dir)
            .stdin(Stdio::null());
        #[cfg(target_os = "windows")]
        if let Some(user_profile) = env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
        {
            /*
            CDXC:WindowsInstalledCwd 2026-08-09:
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
        #[cfg(target_os = "windows")]
        let loader_path_variable = "PATH";
        #[cfg(target_os = "linux")]
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

        #[cfg(target_os = "windows")]
        match command.spawn() {
            Ok(_) => cx.quit(),
            Err(error) => self.show_cef_startup_failure(
                format!("Ghostex could not start its verified Chromium runtime: {error}"),
                cx,
            ),
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt as _;
            let error = command.exec();
            self.show_cef_startup_failure(
                format!("Ghostex could not start its verified Chromium runtime: {error}"),
                cx,
            );
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn gpui_platform_window_app_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    return Some("ghostex".to_string());
    #[cfg(target_os = "windows")]
    return None;
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn gpui_platform_window_icon() -> Option<Arc<image::RgbaImage>> {
    #[cfg(target_os = "linux")]
    {
        static ICON: OnceLock<Arc<image::RgbaImage>> = OnceLock::new();
        return Some(
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
        );
    }
    #[cfg(target_os = "windows")]
    return None;
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
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

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn on_demand_component_store() -> Result<Option<component_store::ComponentStore>, String> {
    let Some(manifest_path) = on_demand_component_manifest_path() else {
        return Ok(None);
    };
    let manifest = component_store::OnDemandManifest::load(&manifest_path)?;
    component_store::ComponentStore::from_manifest(manifest).map(Some)
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn main() {
    #[cfg(target_os = "windows")]
    windows_updater::run_startup_hooks();

    let application = gpui_platform::application();
    application.run(|cx| {
        gpui_component::init(cx);
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
