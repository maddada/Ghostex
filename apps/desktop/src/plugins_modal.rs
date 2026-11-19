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
    size_bytes: u64,
    status: PluginsModalStatus,
    version: String,
}

impl GhostexGpuiApp {
    pub(super) fn refresh_gpui_plugins_modal(&mut self, cx: &mut gpui::Context<Self>) {
        let status = plugin_settings_status_message(self);
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(status, cx);
    }

    pub(super) fn request_plugin_settings_status(&mut self, cx: &mut gpui::Context<Self>) {
        let status = plugin_settings_status_message(self);
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(status, cx);
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
    app: &GhostexGpuiApp,
    store: Option<&component_store::ComponentStore>,
    warning: &mut Option<String>,
    measure_sizes: bool,
) -> PluginsModalRow {
    let installing =
        app.source_code_server_runtime.state == SourceCodeServerRuntimeLaunchState::Installing;
    if let Some(
        failure @ (SourceCodeServerRuntimeFailure::InstallDownload
        | SourceCodeServerRuntimeFailure::InstallIntegrity
        | SourceCodeServerRuntimeFailure::InstallOther),
    ) = app.source_code_server_runtime.failure
    {
        set_first_warning(warning, failure.placeholder_message().to_string());
    }
    let mut actions = Vec::new();
    let mut size_bytes = 0;
    let mut version = "Unavailable".to_string();
    let mut installed = false;
    let mut managed_component = false;
    let mut managed_installed = false;

    if let Some(store) = store
        && let Some(component) = store.component(CODE_SERVER_COMPONENT)
    {
        managed_component = true;
        version = component.component_version.clone();
        match source_code_server_query_local_component(store) {
            Ok(component) => {
                installed = component.installed;
                managed_installed = component.installed;
                size_bytes = component.size_bytes;
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
        if measure_sizes {
            size_bytes = component_store::path_size_bytes(&path).unwrap_or(0);
        }
    }

    let status = if app.plugin_settings_action_errors.contains_key("code")
        || matches!(
            app.source_code_server_runtime.failure,
            Some(
                SourceCodeServerRuntimeFailure::InstallDownload
                    | SourceCodeServerRuntimeFailure::InstallIntegrity
                    | SourceCodeServerRuntimeFailure::InstallOther
            )
        ) {
        PluginsModalStatus::Failed
    } else if installing {
        PluginsModalStatus::Installing(
            app.source_code_server_runtime
                .install_progress
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
    actions.push(
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source) {
            PluginsModalAction::ShowCodeViewTab
        } else {
            PluginsModalAction::HideCodeViewTab
        },
    );

    PluginsModalRow {
        actions,
        size_bytes,
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

pub(super) fn plugin_settings_status_message(app: &GhostexGpuiApp) -> serde_json::Value {
    plugin_settings_status(app, true)
}

/// The same rows without their sizes, for the Updates list, which reads only their state.
///
/// CDXC:Updates 2026-11-19 WHY: a row's size comes from walking its folder on disk, and the
/// Updates list was built on the UI thread every time it was published (its watch every few
/// seconds while a computer's list moved, and twice whenever Settings opened). On Windows the
/// bundled code-server folder is 6,217 files: one walk froze the window for over a second
/// (a profile of the user's app had it at 64% of a two-second spike).
pub(super) fn plugin_settings_status_without_sizes(app: &GhostexGpuiApp) -> serde_json::Value {
    plugin_settings_status(app, false)
}

fn plugin_settings_status(app: &GhostexGpuiApp, measure_sizes: bool) -> serde_json::Value {
    let store_result = on_demand_component_store();
    let mut warning = store_result.as_ref().err().cloned();
    let store = store_result.ok().flatten();
    let rows = [
        (
            "code",
            code_server_row(app, store.as_ref(), &mut warning, measure_sizes),
        ),
        ("cef", cef_row(store.as_ref(), &mut warning, measure_sizes)),
    ];
    let plugins = rows
        .into_iter()
        .map(|(id, mut row)| {
            if let Some(progress) = app.plugin_settings_action_progress.get(id).copied() {
                row.status = PluginsModalStatus::Installing(progress);
            }
            let mut error_message = app.plugin_settings_action_errors.get(id).cloned();
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
                    || !app.web_runtime_started_in_process());
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
                "sizeBytes": row.size_bytes,
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
    measure_sizes: bool,
) -> PluginsModalRow {
    use crate::app::helpers::web_runtime::{WebRuntimeState, web_runtime_state};
    let state = web_runtime_state();
    let bundled_path = bundled_cef_runtime_path();
    let mut version = compiled_cef_version();
    let mut size_bytes = 0;
    let mut managed_installed = false;
    let managed = store
        .and_then(|store| store.component(CEF_COMPONENT))
        .is_some();
    if let Some(store) = store
        && let Some(component) = store.component(CEF_COMPONENT)
    {
        version = component.component_version.clone();
        match store.query_current(CEF_COMPONENT) {
            Ok(installed) => {
                managed_installed = installed.installed;
                size_bytes = installed.size_bytes;
            }
            Err(error) => set_first_warning(warning, error),
        }
    }
    if measure_sizes
        && !managed_installed
        && let Some(path) = bundled_path.as_deref()
    {
        size_bytes = component_store::path_size_bytes(path).unwrap_or_else(|error| {
            set_first_warning(warning, error);
            0
        });
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
        size_bytes,
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
