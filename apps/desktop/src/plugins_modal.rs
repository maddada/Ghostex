use super::*;

const CODE_SERVER_COMPONENT: &str = "code-server";
const CEF_COMPONENT: &str = "cef";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PluginsModalStatus {
    Installed,
    NotInstalled,
    Installing(component_store::ComponentStoreProgressPhase),
    Failed,
}

impl PluginsModalStatus {
    fn label(self) -> String {
        match self {
            Self::Installed => "Installed".to_string(),
            Self::NotInstalled => "Not installed".to_string(),
            Self::Failed => "Install failed".to_string(),
            Self::Installing(phase) => match phase {
                component_store::ComponentStoreProgressPhase::Checking => "Checking…",
                component_store::ComponentStoreProgressPhase::Downloading => "Downloading…",
                component_store::ComponentStoreProgressPhase::Verifying => "Verifying…",
                component_store::ComponentStoreProgressPhase::Installing => "Installing…",
                component_store::ComponentStoreProgressPhase::Pruning => "Finishing…",
                component_store::ComponentStoreProgressPhase::Ready => "Installed",
            }
            .to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PluginsModalAction {
    InstallCodeServer,
    UninstallCodeServer,
    InstallWebRuntime,
    UninstallWebRuntime,
    HideCodeViewTab,
    ShowCodeViewTab,
}

#[derive(Clone)]
struct PluginsModalRow {
    actions: Vec<PluginsModalAction>,
    status: PluginsModalStatus,
    version: String,
}

/// What the plugin rows read from the app, taken on the UI thread so that the rows, which read
/// the disk, can be built off it (`publish_plugin_settings_status`).
#[derive(Clone)]
struct PluginStatusAppState {
    code_installing: bool,
    code_failure: Option<SourceCodeServerRuntimeFailure>,
    code_install_progress: Option<component_store::ComponentStoreProgressPhase>,
    code_view_hidden: bool,
    action_progress: HashMap<&'static str, component_store::ComponentStoreProgressPhase>,
    action_errors: HashMap<&'static str, String>,
    web_runtime_started_in_process: bool,
}

impl PluginStatusAppState {
    fn read(app: &GhostexGpuiApp) -> Self {
        Self {
            code_installing: app.source_code_server_runtime.state
                == SourceCodeServerRuntimeLaunchState::Installing,
            code_failure: app.source_code_server_runtime.failure,
            code_install_progress: app.source_code_server_runtime.install_progress,
            code_view_hidden: gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source),
            action_progress: app.plugin_settings_action_progress.clone(),
            action_errors: app.plugin_settings_action_errors.clone(),
            web_runtime_started_in_process: app.web_runtime_started_in_process(),
        }
    }
}

/// The latest `publish_plugin_settings_status`: an older one finishing later is dropped.
static PLUGIN_STATUS_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

impl GhostexGpuiApp {
    pub(super) fn refresh_gpui_plugins_modal(&mut self, cx: &mut gpui::Context<Self>) {
        self.publish_plugin_settings_status(cx);
    }

    pub(super) fn request_plugin_settings_status(&mut self, cx: &mut gpui::Context<Self>) {
        self.publish_plugin_settings_status(cx);
    }

    /// CDXC:Settings 2026-11-20 WHY:
    /// User, on Windows: Settings opened on its Extensions page showed only its blurred background for about 0.7 s. That page asks for this status when it is built, and the status was built on the UI thread, before Settings' first frame: it walked the code-server and web runtime folders for sizes no Settings page shows any more (6,217 files for the bundled code-server), and a managed Windows code-server's check hashes its archive. The rows are now built on a background thread from what they read of the app (`PluginStatusAppState`) and published when ready, and they carry no sizes. This supersedes the 2026-11-19 note that the Plugins page still measured them.
    fn publish_plugin_settings_status(&mut self, cx: &mut gpui::Context<Self>) {
        use std::sync::atomic::Ordering;
        let state = PluginStatusAppState::read(self);
        let generation = PLUGIN_STATUS_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let status = background
                .spawn(async move { plugin_settings_status(&state) })
                .await;
            if PLUGIN_STATUS_GENERATION.load(Ordering::Relaxed) != generation {
                return;
            }
            let _ = this.update(cx, |app, cx| {
                app.dispatch_open_gpui_app_modal_sidebar_state_payload(status, cx)
            });
        })
        .detach();
    }

    pub(super) fn reinstall_plugin_from_settings(
        &mut self,
        plugin_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        match plugin_id {
            "code" => {
                self.plugin_settings_action_errors.remove("code");
                if self.source_code_server_runtime.state
                    == SourceCodeServerRuntimeLaunchState::Installing
                {
                    return;
                }
                self.stop_source_code_server_runtime(cx);
                let remove_result = (|| {
                    let store = on_demand_component_store()?.ok_or_else(|| {
                        "The sealed code-server component manifest is unavailable.".to_string()
                    })?;
                    let installed = source_code_server_query_local_component(&store)?;
                    store.uninstall(CODE_SERVER_COMPONENT, &installed.version)?;
                    Ok::<(), String>(())
                })();
                match remove_result {
                    Ok(()) => self.install_source_code_server_component(cx),
                    Err(message) => {
                        self.plugin_settings_action_errors.insert("code", message);
                        self.refresh_gpui_plugins_modal(cx);
                    }
                }
            }
            // Install when the runtime is missing or broken, Reinstall otherwise.
            "cef" => {
                let reinstall = matches!(
                    crate::app::helpers::web_runtime::web_runtime_state(),
                    crate::app::helpers::web_runtime::WebRuntimeState::Installed
                        | crate::app::helpers::web_runtime::WebRuntimeState::Running
                );
                self.install_web_runtime(reinstall, cx);
            }
            _ => {}
        }
    }

    /// Settings' Uninstall for a runtime component; only the web runtime offers it there.
    pub(super) fn uninstall_plugin_from_settings(
        &mut self,
        plugin_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if plugin_id == "cef" {
            self.uninstall_web_runtime(cx);
        }
    }
}

fn code_server_row(
    app: &PluginStatusAppState,
    store: Option<&component_store::ComponentStore>,
    warning: &mut Option<String>,
) -> PluginsModalRow {
    let installing = app.code_installing;
    if let Some(
        failure @ (SourceCodeServerRuntimeFailure::InstallDownload
        | SourceCodeServerRuntimeFailure::InstallIntegrity
        | SourceCodeServerRuntimeFailure::InstallOther),
    ) = app.code_failure
    {
        set_first_warning(warning, failure.placeholder_message().to_string());
    }
    let mut actions = Vec::new();
    let mut version = "Unavailable".to_string();
    let mut installed = false;
    let mut managed_component = false;
    let mut managed_installed = false;

    if let Some(store) = store
        && let Some(component) = store.component(CODE_SERVER_COMPONENT)
    {
        managed_component = true;
        version = component.component_version.clone();
        match source_code_server_local_component_installed(store) {
            Ok(component_installed) => {
                installed = component_installed;
                managed_installed = component_installed;
            }
            Err(error) => set_first_warning(warning, error),
        }
    }

    if !installed
        && !installing
        && let Ok(path) = source_code_server_resolve_repo_root()
    {
        installed = true;
        if version == "Unavailable" {
            version = source_code_server_checkout_version(&path)
                .unwrap_or_else(|| "Source checkout".to_string());
        }
    }

    let status = if app.action_errors.contains_key("code")
        || matches!(
            app.code_failure,
            Some(
                SourceCodeServerRuntimeFailure::InstallDownload
                    | SourceCodeServerRuntimeFailure::InstallIntegrity
                    | SourceCodeServerRuntimeFailure::InstallOther
            )
        ) {
        PluginsModalStatus::Failed
    } else if installing {
        PluginsModalStatus::Installing(
            app.code_install_progress
                .unwrap_or(component_store::ComponentStoreProgressPhase::Checking),
        )
    } else if installed {
        PluginsModalStatus::Installed
    } else {
        PluginsModalStatus::NotInstalled
    };

    if !installing && managed_component && (!installed || managed_installed) {
        actions.push(if managed_installed {
            PluginsModalAction::UninstallCodeServer
        } else {
            PluginsModalAction::InstallCodeServer
        });
    }
    actions.push(if app.code_view_hidden {
        PluginsModalAction::ShowCodeViewTab
    } else {
        PluginsModalAction::HideCodeViewTab
    });

    PluginsModalRow {
        actions,
        status,
        version,
    }
}

fn source_code_server_checkout_version(repo_root: &Path) -> Option<String> {
    /*
    CDXC:CodeEditor 2026-08-13:
    The source checkout's top-level code-server package intentionally reports
    0.0.0, so it is not useful runtime identity. Surface the version of the
    VS Code payload that Code actually runs instead of the generic
    "Development" label used before managed component metadata is available.
    */
    let package_json = fs::read_to_string(repo_root.join("lib/vscode/package.json")).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&package_json).ok()?;
    let version = value.get("version")?.as_str()?.trim();
    (!version.is_empty()).then(|| version.to_string())
}

/// The same rows built now, for the Updates list, which reads only their state.
///
/// CDXC:Updates 2026-11-19 WHY: a row's size came from walking its folder on disk, and the
/// Updates list was built on the UI thread every time it was published (its watch every few
/// seconds while a computer's list moved, and twice whenever Settings opened). On Windows the
/// bundled code-server folder is 6,217 files: one walk froze the window for over a second
/// (a profile of the user's app had it at 64% of a two-second spike). The rows carry no sizes
/// since 2026-11-20 (`publish_plugin_settings_status`).
pub(super) fn plugin_settings_status_now(app: &GhostexGpuiApp) -> serde_json::Value {
    plugin_settings_status(&PluginStatusAppState::read(app))
}

fn plugin_settings_status(app: &PluginStatusAppState) -> serde_json::Value {
    let store_result = on_demand_component_store();
    let mut warning = store_result.as_ref().err().cloned();
    let store = store_result.ok().flatten();
    let rows = [
        ("code", code_server_row(app, store.as_ref(), &mut warning)),
        ("cef", cef_row(store.as_ref(), &mut warning)),
    ];
    let plugins = rows
        .into_iter()
        .map(|(id, mut row)| {
            if let Some(progress) = app.action_progress.get(id).copied() {
                row.status = PluginsModalStatus::Installing(progress);
            }
            let mut error_message = app.action_errors.get(id).cloned();
            // The web runtime's row keeps the state it reads (a refused Uninstall does not make it
            // "failed"); a failed install or start shows its message.
            if id == "cef" {
                if error_message.is_none()
                    && let crate::app::helpers::web_runtime::WebRuntimeState::Failed(message) =
                        crate::app::helpers::web_runtime::web_runtime_state()
                {
                    error_message = Some(message);
                }
            } else if error_message.is_some() {
                row.status = PluginsModalStatus::Failed;
            }
            // A change to the web runtime waiting for the next launch
            // (app/helpers/web_runtime_pending.rs): Plugins says so and offers Undo instead.
            let pending_change = (id == "cef")
                .then(crate::app::helpers::web_runtime_pending::pending_web_runtime_change)
                .flatten();
            let can_uninstall = row
                .actions
                .contains(&PluginsModalAction::UninstallWebRuntime)
                && pending_change.is_none()
                && (crate::app::helpers::web_runtime_pending::PENDING_CHANGES_SUPPORTED
                    || !app.web_runtime_started_in_process);
            let can_reinstall = match id {
                "code" => store
                    .as_ref()
                    .and_then(|store| store.component(CODE_SERVER_COMPONENT))
                    .is_some(),
                "cef" => store
                    .as_ref()
                    .and_then(|store| store.component(CEF_COMPONENT))
                    .is_some(),
                _ => false,
            } && !matches!(row.status, PluginsModalStatus::Installing(_));
            let (status, status_label) = plugin_settings_status_wire(row.status);
            serde_json::json!({
                "canReinstall": can_reinstall,
                "canUninstall": can_uninstall,
                "errorMessage": error_message,
                "pendingChange": pending_change.map(|change| change.wire()),
                "id": id,
                "status": status,
                "statusLabel": status_label,
                "version": row.version,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "plugins": plugins,
        "type": "pluginSettingsStatus",
    })
}

fn plugin_settings_status_wire(status: PluginsModalStatus) -> (&'static str, String) {
    let wire = match status {
        PluginsModalStatus::Installed => "installed",
        PluginsModalStatus::NotInstalled => "notInstalled",
        PluginsModalStatus::Failed => "failed",
        PluginsModalStatus::Installing(phase) => match phase {
            component_store::ComponentStoreProgressPhase::Checking => "checking",
            component_store::ComponentStoreProgressPhase::Downloading => "downloading",
            component_store::ComponentStoreProgressPhase::Verifying => "verifying",
            component_store::ComponentStoreProgressPhase::Installing => "installing",
            component_store::ComponentStoreProgressPhase::Pruning => "finishing",
            component_store::ComponentStoreProgressPhase::Ready => "installed",
        },
    };
    (wire, status.label())
}

/// The web runtime's row: its state comes from app/helpers/web_runtime.rs, and it offers Install
/// and Uninstall like the Code editor, because it is an optional component
/// (CDXC:CefRuntime 2026-09-28 there).
fn cef_row(
    store: Option<&component_store::ComponentStore>,
    warning: &mut Option<String>,
) -> PluginsModalRow {
    use crate::app::helpers::web_runtime::{WebRuntimeState, web_runtime_state};
    let state = web_runtime_state();
    let bundled_path = bundled_cef_runtime_path();
    let mut version = compiled_cef_version();
    let mut managed_installed = false;
    let managed = store
        .and_then(|store| store.component(CEF_COMPONENT))
        .is_some();
    if let Some(store) = store
        && let Some(component) = store.component(CEF_COMPONENT)
    {
        version = component.component_version.clone();
        match store.installed_current(CEF_COMPONENT) {
            Ok(installed) => managed_installed = installed,
            Err(error) => set_first_warning(warning, error),
        }
    }
    let status = match state {
        WebRuntimeState::Installing(phase) => PluginsModalStatus::Installing(phase),
        WebRuntimeState::Failed(_) => PluginsModalStatus::Failed,
        WebRuntimeState::Installed | WebRuntimeState::Running => PluginsModalStatus::Installed,
        WebRuntimeState::NotInstalled => PluginsModalStatus::NotInstalled,
    };
    let mut actions = Vec::new();
    if !matches!(status, PluginsModalStatus::Installing(_)) {
        if managed && !managed_installed && bundled_path.is_none() {
            actions.push(PluginsModalAction::InstallWebRuntime);
        }
        if managed_installed && bundled_path.is_none() {
            actions.push(PluginsModalAction::UninstallWebRuntime);
        }
    }
    PluginsModalRow {
        actions,
        status,
        version,
    }
}

fn set_first_warning(warning: &mut Option<String>, message: String) {
    if warning.is_none() {
        *warning = Some(message);
    }
}

fn compiled_cef_version() -> String {
    let bytes = ::cef::sys::CEF_VERSION;
    std::str::from_utf8(&bytes[..bytes.len().saturating_sub(1)])
        .unwrap_or("Unknown")
        .to_string()
}

#[cfg(target_os = "macos")]
fn bundled_cef_runtime_path() -> Option<PathBuf> {
    let resources = gpui_app_bundle_resources_dir()?;
    let path = resources
        .parent()?
        .join("Frameworks/Chromium Embedded Framework.framework");
    path.is_dir().then_some(path)
}

#[cfg(target_os = "windows")]
fn bundled_cef_runtime_path() -> Option<PathBuf> {
    let executable_dir = env::current_exe().ok()?.parent()?.to_path_buf();
    [executable_dir.join("resources"), executable_dir.clone()]
        .into_iter()
        .find(|path| path.join("libcef.dll").is_file())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn bundled_cef_runtime_path() -> Option<PathBuf> {
    let executable_dir = env::current_exe().ok()?.parent()?.to_path_buf();
    [executable_dir.join("lib"), executable_dir.clone()]
        .into_iter()
        .find(|path| path.join("libcef.so").is_file())
}
