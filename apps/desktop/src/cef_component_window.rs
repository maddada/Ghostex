use super::*;

const CEF_COMPONENT: &str = "cef";
pub(crate) const CEF_RUNTIME_DIR_ENV: &str = "GHOSTEX_CEF_DIR";
#[cfg(target_os = "macos")]
const CEF_FRAMEWORK_EXECUTABLE_ENV: &str = "GHOSTEX_CEF_FRAMEWORK_EXECUTABLE";
const CEF_COMPONENT_WINDOW_WIDTH: f32 = 520.0;
const CEF_COMPONENT_WINDOW_HEIGHT: f32 = 300.0;
#[cfg(target_os = "macos")]
const CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH: &str =
    "Chromium Embedded Framework.framework/Chromium Embedded Framework";

#[derive(Clone, Debug)]
enum CefComponentWindowState {
    Installing(CefComponentProgress),
    Failed(String),
}

#[derive(Clone, Copy, Debug)]
struct CefComponentProgress {
    downloaded_bytes: u64,
    phase: component_store::ComponentStoreProgressPhase,
    size_bytes: u64,
}

impl CefComponentProgress {
    fn checking() -> Self {
        Self {
            downloaded_bytes: 0,
            phase: component_store::ComponentStoreProgressPhase::Checking,
            size_bytes: 0,
        }
    }
}

pub(crate) struct GpuiCefComponentWindow {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    state: CefComponentWindowState,
    version: String,
}

impl GpuiCefComponentWindow {
    fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        state: CefComponentWindowState,
        version: String,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|_| Self {
            main_app,
            state,
            version,
        })
    }

    fn set_state(&mut self, state: CefComponentWindowState, cx: &mut gpui::Context<Self>) {
        self.state = state;
        cx.notify();
    }

    fn retry(&mut self, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update(cx, |app, cx| {
            app.start_cef_component_install(cx);
        });
    }
}

impl Render for GpuiCefComponentWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let (title, detail, progress, failed) = match &self.state {
            CefComponentWindowState::Installing(component_progress) => {
                let (detail, progress) = match component_progress.phase {
                    component_store::ComponentStoreProgressPhase::Checking => {
                        ("Checking the required runtime…".to_string(), 0.10)
                    }
                    component_store::ComponentStoreProgressPhase::Downloading => {
                        let download_ratio = if component_progress.size_bytes == 0 {
                            0.0
                        } else {
                            component_progress.downloaded_bytes as f32
                                / component_progress.size_bytes as f32
                        }
                        .clamp(0.0, 1.0);
                        let detail = if component_progress.size_bytes == 0 {
                            "Downloading the browser runtime…".to_string()
                        } else {
                            format!(
                                "Downloading the browser runtime… {}% · {} of {}",
                                (download_ratio * 100.0).round() as u32,
                                cef_component_bytes_label(component_progress.downloaded_bytes),
                                cef_component_bytes_label(component_progress.size_bytes),
                            )
                        };
                        (detail, 0.10 + download_ratio * 0.50)
                    }
                    component_store::ComponentStoreProgressPhase::Verifying => {
                        ("Verifying the signed download…".to_string(), 0.65)
                    }
                    component_store::ComponentStoreProgressPhase::Installing => {
                        ("Installing the verified runtime…".to_string(), 0.82)
                    }
                    component_store::ComponentStoreProgressPhase::Pruning => {
                        ("Finishing installation…".to_string(), 0.94)
                    }
                    component_store::ComponentStoreProgressPhase::Ready => {
                        ("Starting Ghostex…".to_string(), 1.0)
                    }
                };
                ("Preparing Ghostex", detail, progress, false)
            }
            CefComponentWindowState::Failed(message) => (
                "Chromium runtime download failed",
                message.clone(),
                0.0,
                true,
            ),
        };

        v_flex()
            .size_full()
            .bg(rgb(0x17191d))
            .p(px(28.0))
            .gap(px(14.0))
            .child(
                div()
                    .text_size(px(21.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff).opacity(0.95))
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .line_height(px(18.0))
                    .text_color(rgb(0xffffff).opacity(0.62))
                    .child(if failed {
                        "Ghostex needs Chromium Embedded Framework before browser surfaces can start. Check your connection to GitHub, then retry."
                    } else {
                        "Ghostex uses a shared Chromium runtime. It is downloaded once for this CEF version and reused by future app updates."
                    }),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(0xffffff).opacity(0.42))
                    .child(format!("CEF {}", self.version)),
            )
            .child(
                div()
                    .w_full()
                    .h(px(6.0))
                    .rounded(px(3.0))
                    .bg(rgb(0xffffff).opacity(0.08))
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress))
                            .rounded(px(3.0))
                            .bg(if failed {
                                rgb(0xe15a64)
                            } else {
                                rgb(0x4f8fe8)
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(if failed {
                        rgb(0xff9da5).into()
                    } else {
                        rgb(0xffffff).opacity(0.58)
                    })
                    .child(detail),
            )
            .when(failed, |this| {
                this.child(
                    div()
                        .id("ghostex-gpui-cef-component-retry")
                        .flex()
                        .h(px(30.0))
                        .w(px(92.0))
                        .items_center()
                        .justify_center()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(rgb(0xffffff).opacity(0.22))
                        .bg(rgb(0x377bd8))
                        .hover(|this| this.bg(rgb(0x4389e7)))
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0xffffff).opacity(0.94))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _: &MouseDownEvent, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseUpEvent, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.retry(cx);
                            }),
                        )
                        .child("Retry"),
                )
            })
    }
}

fn cef_component_bytes_label(bytes: u64) -> String {
    const MEBIBYTE: f64 = 1024.0 * 1024.0;
    let mebibytes = bytes as f64 / MEBIBYTE;
    if mebibytes >= 10.0 {
        format!("{mebibytes:.0} MB")
    } else {
        format!("{mebibytes:.1} MB")
    }
}

impl GhostexGpuiApp {
    pub(super) fn begin_cef_startup(&mut self, cx: &mut gpui::Context<Self>) {
        match verified_cef_runtime_readiness() {
            Ok(CefRuntimeReadiness::Ready) => self.initialize_cef(cx),
            Ok(CefRuntimeReadiness::InstallRequired { version }) => {
                self.open_cef_component_window(
                    CefComponentWindowState::Installing(CefComponentProgress::checking()),
                    version,
                    cx,
                );
                self.start_cef_component_install(cx);
            }
            Err(message) => {
                self.open_cef_component_window(
                    CefComponentWindowState::Failed(message),
                    expected_cef_component_version(),
                    cx,
                );
            }
        }
    }

    fn open_cef_component_window(
        &mut self,
        state: CefComponentWindowState,
        version: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(handle) = self.cef_component_window.clone() {
            if handle
                .update(cx, |view, window, cx| {
                    view.version.clone_from(&version);
                    view.set_state(state.clone(), cx);
                    window.activate_window();
                    window.refresh();
                })
                .is_ok()
            {
                return;
            }
            self.cef_component_window = None;
        }

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(
                size(
                    px(CEF_COMPONENT_WINDOW_WIDTH),
                    px(CEF_COMPONENT_WINDOW_HEIGHT),
                ),
                cx,
            )),
            app_id: gpui_platform_window_app_id(),
            focus: true,
            icon: gpui_platform_window_icon(),
            show: true,
            is_resizable: false,
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Ghostex Setup".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        };
        let main_app = cx.weak_entity();
        self.cef_component_window = cx
            .open_window(options, move |window, cx| {
                window.activate_window();
                GpuiCefComponentWindow::new(main_app, state, version, cx)
            })
            .ok();
    }

    pub(super) fn show_cef_startup_failure(
        &mut self,
        message: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_cef_component_window(
            CefComponentWindowState::Failed(message),
            expected_cef_component_version(),
            cx,
        );
    }

    fn start_cef_component_install(&mut self, cx: &mut gpui::Context<Self>) {
        self.cef_component_install_generation =
            self.cef_component_install_generation.wrapping_add(1);
        let generation = self.cef_component_install_generation;
        let version = expected_cef_component_version();
        self.open_cef_component_window(
            CefComponentWindowState::Installing(CefComponentProgress::checking()),
            version,
            cx,
        );

        let (progress_tx, mut progress_rx) = mpsc::unbounded();
        cx.spawn(async move |this, cx| {
            while let Some(progress) = progress_rx.next().await {
                let _ = this.update(cx, |this, cx| {
                    if this.cef_component_install_generation != generation {
                        return;
                    }
                    if let Some(handle) = this.cef_component_window.clone() {
                        let _ = handle.update(cx, |view, window, cx| {
                            view.set_state(CefComponentWindowState::Installing(progress), cx);
                            window.refresh();
                        });
                    }
                });
            }
        })
        .detach();

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { install_and_verify_cef_component(progress_tx) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.cef_component_install_generation != generation {
                    return;
                }
                match result {
                    Ok(()) => {
                        configure_cef_framework_path_for_process();
                        if let Some(handle) = this.cef_component_window.take() {
                            let _ = handle.update(cx, |_, window, _| window.remove_window());
                        }
                        this.initialize_cef(cx);
                    }
                    Err(message) => {
                        if let Some(handle) = this.cef_component_window.clone() {
                            let _ = handle.update(cx, |view, window, cx| {
                                view.set_state(
                                    CefComponentWindowState::Failed(message.clone()),
                                    cx,
                                );
                                window.refresh();
                            });
                        } else {
                            this.open_cef_component_window(
                                CefComponentWindowState::Failed(message),
                                expected_cef_component_version(),
                                cx,
                            );
                        }
                    }
                }
            });
        })
        .detach();
    }
}

enum CefRuntimeReadiness {
    Ready,
    InstallRequired { version: String },
}

pub(crate) fn configure_cef_framework_path_for_process() {
    let runtime_dir = bundled_cef_runtime_dir()
        .or_else(|| {
            let configured = env::var_os(CEF_RUNTIME_DIR_ENV).map(PathBuf::from)?;
            verify_cef_runtime_dir(&configured).ok()?;
            Some(configured)
        })
        .or_else(|| {
            let store = on_demand_component_store().ok().flatten()?;
            let component = store.component(CEF_COMPONENT)?;
            if component.component_version != expected_cef_component_version() {
                return None;
            }
            let installed = store.query_current(CEF_COMPONENT).ok()?;
            installed.installed.then_some(installed.path)
        });
    unsafe {
        if let Some(runtime_dir) = runtime_dir {
            env::set_var(CEF_RUNTIME_DIR_ENV, &runtime_dir);
            #[cfg(target_os = "macos")]
            env::set_var(
                CEF_FRAMEWORK_EXECUTABLE_ENV,
                runtime_dir.join(CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH),
            );
        } else {
            env::remove_var(CEF_RUNTIME_DIR_ENV);
            #[cfg(target_os = "macos")]
            env::remove_var(CEF_FRAMEWORK_EXECUTABLE_ENV);
        }
    }
}

fn verified_cef_runtime_readiness() -> Result<CefRuntimeReadiness, String> {
    if let Some(runtime_dir) = bundled_cef_runtime_dir() {
        verify_cef_runtime_dir(&runtime_dir)?;
        return Ok(CefRuntimeReadiness::Ready);
    }
    if let Some(runtime_dir) = env::var_os(CEF_RUNTIME_DIR_ENV).map(PathBuf::from) {
        verify_cef_runtime_dir(&runtime_dir)?;
        return Ok(CefRuntimeReadiness::Ready);
    }
    let store = on_demand_component_store()?
        .ok_or_else(|| "The sealed CEF component manifest is unavailable.".to_string())?;
    let component = store.component(CEF_COMPONENT).ok_or_else(|| {
        "The sealed manifest does not define the required CEF component.".to_string()
    })?;
    let expected_version = expected_cef_component_version();
    if component.component_version != expected_version {
        return Err(format!(
            "The sealed CEF component version {} does not match this app's required version {expected_version}.",
            component.component_version
        ));
    }
    let installed = store.query_current(CEF_COMPONENT)?;
    if !installed.installed {
        return Ok(CefRuntimeReadiness::InstallRequired {
            version: component.component_version.clone(),
        });
    }
    verify_cef_runtime_dir(&installed.path)?;
    Ok(CefRuntimeReadiness::Ready)
}

fn install_and_verify_cef_component(
    progress_tx: mpsc::UnboundedSender<CefComponentProgress>,
) -> Result<(), String> {
    let store = on_demand_component_store()?
        .ok_or_else(|| "The sealed CEF component manifest is unavailable.".to_string())?;
    let component = store.component(CEF_COMPONENT).ok_or_else(|| {
        "The sealed manifest does not define the required CEF component.".to_string()
    })?;
    let expected_version = expected_cef_component_version();
    if component.component_version != expected_version {
        return Err(format!(
            "The sealed CEF component version {} does not match this app's required version {expected_version}.",
            component.component_version
        ));
    }

    let current = store.query_current(CEF_COMPONENT)?;
    if current.installed && verify_cef_runtime_dir(&current.path).is_err() {
        store.uninstall(CEF_COMPONENT, &current.version)?;
    }

    let mut report_progress = |progress: component_store::ComponentStoreProgress| {
        let _ = progress_tx.unbounded_send(CefComponentProgress {
            downloaded_bytes: progress.downloaded_bytes,
            phase: progress.phase,
            size_bytes: progress.size_bytes,
        });
    };
    let installed = store.install(CEF_COMPONENT, &mut report_progress)?;
    verify_cef_runtime_dir(&installed.path)
}

pub(super) fn reinstall_and_verify_cef_component(
    progress_tx: mpsc::UnboundedSender<component_store::ComponentStoreProgressPhase>,
) -> Result<(), String> {
    let store = on_demand_component_store()?
        .ok_or_else(|| "The sealed CEF component manifest is unavailable.".to_string())?;
    let component = store.component(CEF_COMPONENT).ok_or_else(|| {
        "The sealed manifest does not define the required CEF component.".to_string()
    })?;
    let expected_version = expected_cef_component_version();
    if component.component_version != expected_version {
        return Err(format!(
            "The sealed CEF component version {} does not match this app's required version {expected_version}.",
            component.component_version
        ));
    }
    let current = store.query_current(CEF_COMPONENT)?;
    if current.installed {
        store.uninstall(CEF_COMPONENT, &current.version)?;
    }
    let mut report_progress = |progress: component_store::ComponentStoreProgress| {
        let _ = progress_tx.unbounded_send(progress.phase);
    };
    let installed = store.install(CEF_COMPONENT, &mut report_progress)?;
    verify_cef_runtime_dir(&installed.path)
}

fn bundled_cef_runtime_dir() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let executable_dir = executable.parent()?;
    #[cfg(target_os = "macos")]
    let path = executable_dir
        .join("../Frameworks")
        .join(CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH);
    #[cfg(target_os = "macos")]
    return path.is_file().then(|| {
        let runtime_dir = path
            .ancestors()
            .nth(2)
            .expect("CEF framework executable must have a component root");
        runtime_dir
            .canonicalize()
            .unwrap_or_else(|_| runtime_dir.to_path_buf())
    });
    #[cfg(target_os = "windows")]
    return executable_dir
        .join("libcef.dll")
        .is_file()
        .then(|| executable_dir.to_path_buf());
    #[cfg(target_os = "linux")]
    return executable_dir
        .join("libcef.so")
        .is_file()
        .then(|| executable_dir.to_path_buf());
}

fn verify_cef_runtime_dir(runtime_dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let library_path = runtime_dir.join(CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH);
    #[cfg(target_os = "windows")]
    let library_path = runtime_dir.join("libcef.dll");
    #[cfg(target_os = "linux")]
    let library_path = runtime_dir.join("libcef.so");
    if !library_path.is_file() {
        return Err(format!(
            "The verified CEF component is missing its runtime library at {}.",
            library_path.display()
        ));
    }
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    for required in [runtime_dir.join("icudtl.dat"), runtime_dir.join("locales")] {
        if !required.exists() {
            return Err(format!(
                "The verified CEF component is missing a required runtime resource at {}.",
                required.display()
            ));
        }
    }
    #[cfg(target_os = "macos")]
    let framework = library_path
        .parent()
        .ok_or_else(|| "The CEF framework executable has no parent directory.".to_string())?;
    #[cfg(target_os = "macos")]
    let verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(framework)
        .output()
        .map_err(|error| format!("Could not verify the downloaded CEF signature: {error}"))?;
    #[cfg(target_os = "macos")]
    if !verification.status.success() {
        return Err("The downloaded CEF component failed macOS code-signature verification. Retry to download a clean copy.".to_string());
    }

    #[cfg(target_os = "macos")]
    let app_team = code_signing_team_identifier(&env::current_exe().map_err(|error| {
        format!("Could not resolve the Ghostex executable for signature verification: {error}")
    })?)?;
    #[cfg(target_os = "macos")]
    let cef_team = code_signing_team_identifier(framework)?;
    #[cfg(target_os = "macos")]
    if app_team != cef_team {
        return Err(format!(
            "The downloaded CEF component is signed by a different Developer ID team (app: {}, CEF: {}).",
            app_team.as_deref().unwrap_or("ad hoc"),
            cef_team.as_deref().unwrap_or("ad hoc")
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn code_signing_team_identifier(path: &Path) -> Result<Option<String>, String> {
    let output = Command::new("/usr/bin/codesign")
        .args(["-d", "--verbose=4"])
        .arg(path)
        .output()
        .map_err(|error| {
            format!(
                "Could not inspect code signature for {}: {error}",
                path.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "Could not inspect code signature for {}.",
            path.display()
        ));
    }
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    Ok(diagnostics.lines().find_map(|line| {
        line.strip_prefix("TeamIdentifier=")
            .filter(|value| *value != "not set")
            .map(str::to_string)
    }))
}

fn expected_cef_component_version() -> String {
    env!("GHOSTEX_CEF_COMPONENT_VERSION")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect()
}
