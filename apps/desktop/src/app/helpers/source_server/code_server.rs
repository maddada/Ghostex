use super::*;

/*
CDXC:Settings 2026-06-24-12:19:
The GPUI app-modal write bridge validates bounded Settings agent/action messages from CEF, while read hydration comes from gxserver's `/api/readSidebarHud` contract instead of app-modal Rust reimplementing the custom launcher/action projection.

CDXC:AgentLauncher 2026-06-24-20:54:
Agent/action Settings writes now persist through gxserver's `/api/mutateSidebarHudSettings` contract. Keep this parser as a shape/type boundary only; do not mutate custom agent/action project metadata locally or log launcher commands, URLs, project names, paths, prompts, tokens, stdout/stderr, daemon bodies, or renderer payload contents.
*/

pub(crate) fn source_code_server_runtime_target_from_project_snapshot(
    snapshot: &GpuiProjectSnapshot,
    endpoint: SourceCodeServerRuntimeEndpoint,
) -> Option<SourceCodeServerRuntimeTarget> {
    /*
    CDXC:CodeEditor 2026-06-24-23:17:
    Source folder URLs are authorized only from the strict in-memory sidebar project snapshot: explicit project id, Source workarea id, project path, and Source availability. This mirrors the macOS sidebar's `createCodeServerProjectEditorUrl(project.path)` contract without accepting renderer URLs, filesystem probes, fallback localhost strings, or persisted project facts.
    */
    if !snapshot.feature_availability.source || snapshot.is_quick_projectless {
        return None;
    }
    let active_project_id = snapshot.active_project_id.as_ref()?.clone();
    let source_workarea_id = snapshot.surface_ids.source_workarea_id.as_ref()?.clone();
    let project_path = snapshot.in_memory_project_path.as_ref()?.clone();
    Some(SourceCodeServerRuntimeTarget {
        active_project_id,
        source_workarea_id,
        project_path,
        endpoint,
    })
}

pub(crate) fn source_code_server_runtime_url(
    runtime_origin: &str,
    project_path: &Path,
) -> Option<ProjectWorkareaRealRuntimeUrl> {
    ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(append_url_query_params(
        format!("{}/", runtime_origin.trim_end_matches('/')),
        &[("folder", gpui_path_string(project_path))],
    ))
}

pub(crate) fn source_code_server_start_runtime_for_target(
    target: SourceCodeServerRuntimeTarget,
    settings: SourceCodeServerRuntimeSettings,
    startup_deadline: Instant,
) -> Result<
    (
        SourceCodeServerRuntimeTarget,
        SourceCodeServerRuntimeSettings,
        SourceCodeServerRuntimeStartOutput,
    ),
    (
        SourceCodeServerRuntimeTarget,
        SourceCodeServerRuntimeSettings,
        String,
    ),
> {
    let result = source_code_server_spawn_runtime(&target, &settings, startup_deadline);
    match result {
        Ok(output) => Ok((target, settings, output)),
        Err(message) => Err((target, settings, message)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceCodeServerRuntimeAvailability {
    Available,
    InstallRequired,
    Failed(SourceCodeServerRuntimeFailure),
}

pub(crate) fn on_demand_component_manifest_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("GHOSTEX_ON_DEMAND_MANIFEST") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(resources_dir) = gpui_app_bundle_resources_dir() {
        let path = resources_dir.join("Web/on-demand-resources.json");
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(executable) = env::current_exe()
        && let Some(executable_dir) = executable.parent()
    {
        for path in [
            executable_dir.join("resources/on-demand-resources.json"),
            executable_dir.join("resources/Web/on-demand-resources.json"),
            executable_dir.join("on-demand-resources.json"),
        ] {
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

pub(crate) fn on_demand_component_store() -> Result<Option<component_store::ComponentStore>, String>
{
    let Some(manifest_path) = on_demand_component_manifest_path() else {
        return Ok(None);
    };
    let manifest = component_store::OnDemandManifest::load(&manifest_path)?;
    component_store::ComponentStore::from_manifest(manifest).map(Some)
}

pub(crate) fn source_code_server_runtime_availability(
    target: &SourceCodeServerRuntimeTarget,
) -> SourceCodeServerRuntimeAvailability {
    #[cfg(not(target_os = "windows"))]
    if target.component_platform().is_none() && source_code_server_resolve_repo_root().is_ok() {
        return SourceCodeServerRuntimeAvailability::Available;
    }
    let store = match on_demand_component_store() {
        Ok(Some(store)) => store,
        Ok(None) => {
            return SourceCodeServerRuntimeAvailability::Failed(
                SourceCodeServerRuntimeFailure::Launch,
            );
        }
        Err(_) => {
            return SourceCodeServerRuntimeAvailability::Failed(
                SourceCodeServerRuntimeFailure::InstallOther,
            );
        }
    };
    let installed = match target.component_platform() {
        Some(platform) => {
            store.query_current_for_platform(SOURCE_CODE_SERVER_COMPONENT_NAME, platform)
        }
        None => store.query_current(SOURCE_CODE_SERVER_COMPONENT_NAME),
    };
    match installed {
        Ok(installed) if installed.installed => SourceCodeServerRuntimeAvailability::Available,
        Ok(_) => SourceCodeServerRuntimeAvailability::InstallRequired,
        Err(_) => SourceCodeServerRuntimeAvailability::Failed(
            SourceCodeServerRuntimeFailure::InstallOther,
        ),
    }
}

pub(crate) fn source_code_server_install_component(
    target: Option<&SourceCodeServerRuntimeTarget>,
    progress_tx: mpsc::UnboundedSender<component_store::ComponentStoreProgressPhase>,
) -> Result<component_store::InstalledComponent, String> {
    let store = on_demand_component_store()?
        .ok_or_else(|| "The sealed code-server component manifest is unavailable.".to_string())?;
    let mut report_progress = |progress: component_store::ComponentStoreProgress| {
        let _ = progress_tx.unbounded_send(progress.phase);
    };
    let component_platform = target.and_then(SourceCodeServerRuntimeTarget::component_platform);
    let installed = match component_platform {
        Some(platform) => store.install_for_platform(
            SOURCE_CODE_SERVER_COMPONENT_NAME,
            platform,
            &mut report_progress,
        )?,
        None => store.install(SOURCE_CODE_SERVER_COMPONENT_NAME, &mut report_progress)?,
    };
    #[cfg(not(target_os = "windows"))]
    {
        if component_platform.is_some() {
            source_code_server_validate_remote_linux_payload(&installed.path)?;
        } else {
            source_code_server_validate_development_payload(&installed.path)?;
            let node_path = installed.path.join("lib/node");
            if source_code_server_node_major(&node_path)
                != Some(SOURCE_CODE_SERVER_DEFAULT_NODE_MAJOR)
            {
                return Err(
                    "The installed code-server component has an invalid Node runtime.".to_string(),
                );
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        component_store::verify_installed_windows_code_server_component(
            &installed.path,
            &installed.version,
            &installed.platform,
        )?;
    }
    Ok(installed)
}

pub(crate) fn source_code_server_install_failure(message: &str) -> SourceCodeServerRuntimeFailure {
    let message = message.to_ascii_lowercase();
    if message.contains("sha-256")
        || message.contains("checksum")
        || message.contains("size mismatch")
        || message.contains("verification")
    {
        SourceCodeServerRuntimeFailure::InstallIntegrity
    } else if message.contains("download") || message.contains("invoke-webrequest") {
        SourceCodeServerRuntimeFailure::InstallDownload
    } else {
        SourceCodeServerRuntimeFailure::InstallOther
    }
}

pub(crate) fn source_code_server_resolve_repo_root() -> Result<PathBuf, String> {
    let configured_root = env::var("GHOSTEX_CODE_SERVER_ROOT")
        .ok()
        .or_else(|| env::var("ghostex_CODE_SERVER_ROOT").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let configured_root_is_set = configured_root.is_some();
    let candidates = if let Some(configured_root) = configured_root {
        vec![PathBuf::from(configured_root)]
    } else {
        source_code_server_repo_root_candidates()
    };
    for candidate in candidates {
        if candidate.join("out/node/entry.js").exists()
            && source_code_server_validate_development_payload(&candidate).is_ok()
        {
            return Ok(candidate);
        }
    }
    if !configured_root_is_set && let Some(store) = on_demand_component_store()? {
        let installed = store.query_current(SOURCE_CODE_SERVER_COMPONENT_NAME)?;
        if installed.installed
            && installed.path.join("out/node/entry.js").is_file()
            && source_code_server_validate_development_payload(&installed.path).is_ok()
        {
            return Ok(installed.path);
        }
    }
    Err("code-server runtime is unavailable".to_string())
}

pub(crate) fn source_code_server_repo_root_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut append = |candidate: PathBuf| {
        if !candidates.iter().any(|existing| existing == &candidate) {
            candidates.push(candidate);
        }
    };
    if let Ok(executable) = env::current_exe() {
        #[cfg(target_os = "macos")]
        if let Some(bundle_root) = find_app_bundle_root(&executable) {
            let resources_dir = bundle_root.join("Contents/Resources");
            append(resources_dir.join("Web/code-server"));
            append(resources_dir.join("code-server"));
        }
        // Non-macOS staged layouts are flat: bundled payloads sit beside the
        // executable (same contract as the staged gxserver package).
        #[cfg(not(target_os = "macos"))]
        if let Some(exe_dir) = executable.parent() {
            append(exe_dir.join("code-server"));
        }
    }
    if let Ok(repo_root) = env::var("ghostex_REPO_ROOT") {
        let repo_root = repo_root.trim();
        if !repo_root.is_empty() {
            append(PathBuf::from(repo_root).join(".dependencies/code-server"));
        }
    }
    if let Ok(cwd) = env::current_dir() {
        append(cwd.join(".dependencies/code-server"));
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(repo_root) = manifest_dir.parent().and_then(Path::parent) {
        append(repo_root.join(".dependencies/code-server"));
        #[cfg(target_os = "macos")]
        append(repo_root.join("apps/desktop/runtime/macos/Web/code-server"));
    }
    candidates
}

pub(crate) fn source_code_server_validate_development_payload(
    repo_root: &Path,
) -> Result<(), String> {
    if !repo_root.join("lib/vscode/package.json").exists() {
        return Err("code-server VS Code payload is unavailable".to_string());
    }
    if !repo_root.join("lib/vscode/out/server-main.js").exists() {
        return Err("code-server VS Code server output is unavailable".to_string());
    }
    // fs-copyfile only compiles a native binding on macOS (binding.gyp is
    // `type: none` elsewhere and the JS falls back to node:fs), so the
    // installed-package check is the platform-correct git-extension probe
    // off macOS.
    #[cfg(target_os = "macos")]
    let git_extension_probe =
        "lib/vscode/extensions/git/node_modules/@vscode/fs-copyfile/build/Release/vscode_fs.node";
    #[cfg(not(target_os = "macos"))]
    let git_extension_probe =
        "lib/vscode/extensions/git/node_modules/@vscode/fs-copyfile/package.json";
    if !repo_root.join(git_extension_probe).exists() {
        return Err("code-server Git extension native module is unavailable".to_string());
    }
    Ok(())
}

pub(crate) fn source_code_server_runtime_storage() -> Result<(PathBuf, PathBuf), String> {
    let storage = shared_settings::ghostex_storage_paths().code_server_runtime_dir();
    let user_data_dir = storage.join("user-data");
    let extensions_dir = storage.join("extensions");
    fs::create_dir_all(&user_data_dir)
        .map_err(|_| "failed to prepare Source runtime storage".to_string())?;
    fs::create_dir_all(&extensions_dir)
        .map_err(|_| "failed to prepare Source runtime storage".to_string())?;
    Ok((user_data_dir, extensions_dir))
}

pub(crate) fn source_code_server_should_seed_default_theme(
    settings: &SourceCodeServerRuntimeSettings,
) -> bool {
    let Some(linked_dir) = settings.linked_vscode_user_config_dir() else {
        return true;
    };
    !PathBuf::from(linked_dir).join("settings.json").exists()
}

pub(crate) fn source_code_server_ensure_default_theme(user_data_dir: &Path) -> Result<(), String> {
    /*
    CDXC:CodeEditor 2026-06-24-23:17:
    When GPUI owns the code-server user-data profile, seed the same Dark 2026 default theme as macOS only if the profile has no settings file. Do not overwrite user-edited runtime settings or linked local VS Code settings.
    */
    let user_dir = user_data_dir.join("User");
    let settings_path = user_dir.join("settings.json");
    fs::create_dir_all(&user_dir)
        .map_err(|_| "failed to prepare Source runtime settings".to_string())?;
    if settings_path.exists() {
        return Ok(());
    }
    fs::write(
        settings_path,
        "{\n  \"workbench.colorTheme\": \"Dark 2026\"\n}\n",
    )
    .map_err(|_| "failed to prepare Source runtime settings".to_string())
}

pub(crate) fn source_code_server_resolve_node_path(repo_root: &Path) -> Result<PathBuf, String> {
    let required_major = source_code_server_required_node_major(repo_root)
        .unwrap_or(SOURCE_CODE_SERVER_DEFAULT_NODE_MAJOR);
    if let Some(configured) = env::var("GHOSTEX_CODE_SERVER_NODE_PATH")
        .ok()
        .or_else(|| env::var("ghostex_CODE_SERVER_NODE_PATH").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let configured = PathBuf::from(configured);
        if source_code_server_node_major(&configured) == Some(required_major) {
            return Ok(configured);
        }
        return Err("configured Source Node runtime is incompatible".to_string());
    }

    for candidate in source_code_server_bundled_node_candidates(repo_root) {
        if source_code_server_node_major(&candidate) == Some(required_major) {
            return Ok(candidate);
        }
    }
    if source_code_server_is_bundled_repo_root(repo_root) {
        return Err("bundled Source Node runtime is unavailable".to_string());
    }
    for candidate in source_code_server_system_node_candidates(required_major) {
        if source_code_server_node_major(&candidate) == Some(required_major) {
            return Ok(candidate);
        }
    }
    Err("Source Node runtime is unavailable".to_string())
}

pub(crate) fn source_code_server_required_node_major(repo_root: &Path) -> Option<u64> {
    let package_json = fs::read_to_string(repo_root.join("package.json")).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&package_json).ok()?;
    let node_engine = value.get("engines")?.get("node")?.as_str()?;
    source_code_server_first_integer(node_engine)
}

pub(crate) fn source_code_server_bundled_node_candidates(repo_root: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut append = |candidate: PathBuf| {
        if !candidates.iter().any(|existing| existing == &candidate) {
            candidates.push(candidate);
        }
    };
    append(repo_root.join("lib/node"));
    #[cfg(target_os = "macos")]
    {
        if let Ok(executable) = env::current_exe() {
            if let Some(bundle_root) = find_app_bundle_root(&executable) {
                let resources_dir = bundle_root.join("Contents/Resources");
                append(resources_dir.join("Web/code-server/lib/node"));
                append(resources_dir.join("code-server/lib/node"));
            }
        }
        if let Ok(cwd) = env::current_dir() {
            append(cwd.join("apps/desktop/runtime/macos/Web/code-server/lib/node"));
        }
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if let Some(repo_root) = manifest_dir.parent().and_then(Path::parent) {
            append(repo_root.join("apps/desktop/runtime/macos/Web/code-server/lib/node"));
        }
    }
    // Non-macOS staged layouts keep bundled payloads beside the executable;
    // the staged gxserver package also carries a matching Node runtime.
    #[cfg(not(target_os = "macos"))]
    if let Ok(executable) = env::current_exe() {
        if let Some(exe_dir) = executable.parent() {
            append(exe_dir.join("code-server/lib/node"));
            append(exe_dir.join("gxserver/code-server/lib/node"));
        }
    }
    candidates
}

pub(crate) fn source_code_server_system_node_candidates(required_major: u64) -> Vec<PathBuf> {
    let home = home_dir();
    let mut candidates = Vec::new();
    for directory in [
        home.join(".local/node"),
        home.join(".local/share/mise/installs/node"),
        home.join(".asdf/installs/nodejs"),
        home.join(".nvm/versions/node"),
    ] {
        candidates.extend(source_code_server_node_install_candidates(
            &directory,
            required_major,
        ));
    }
    #[cfg(target_os = "macos")]
    {
        candidates.push(PathBuf::from(format!(
            "/opt/homebrew/opt/node@{required_major}/bin/node"
        )));
        candidates.push(PathBuf::from(format!(
            "/usr/local/opt/node@{required_major}/bin/node"
        )));
    }
    if let Some(path_node) = source_code_server_resolve_command_path("node") {
        candidates.push(path_node);
    }
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.clone()))
        .collect()
}

pub(crate) fn source_code_server_node_install_candidates(
    directory: &Path,
    required_major: u64,
) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut candidates = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| entry.is_dir())
        .filter(|entry| {
            let name = entry
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            name.starts_with(&format!("node-v{required_major}."))
                || name.starts_with(&format!("v{required_major}."))
                || name.starts_with(&format!("{required_major}."))
        })
        .map(|entry| entry.join("bin/node"))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.cmp(left));
    candidates
}

pub(crate) fn source_code_server_node_major(node_path: &Path) -> Option<u64> {
    if !node_path.is_file() {
        return None;
    }
    let output = Command::new(node_path)
        .arg("-v")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8(output.stdout).ok()?;
    source_code_server_first_integer(version.trim())
}

pub(crate) fn source_code_server_first_integer(value: &str) -> Option<u64> {
    let mut digits = String::new();
    for character in value.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        if !digits.is_empty() {
            break;
        }
    }
    digits.parse().ok()
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_resolve_command_path(command: &str) -> Option<PathBuf> {
    // macOS GUI apps inherit a minimal PATH, so consult the user's login
    // shell for the real one.
    let output = Command::new("/bin/zsh")
        .arg("-lc")
        .arg(format!("command -v {}", gpui_shell_quote(command)))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn source_code_server_resolve_command_path(command: &str) -> Option<PathBuf> {
    // Outside macOS the process PATH is the session PATH; scan it directly
    // instead of assuming a specific login shell exists.
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(command))
        .find(|candidate| candidate.is_file())
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_is_bundled_repo_root(repo_root: &Path) -> bool {
    let Ok(executable) = env::current_exe() else {
        return false;
    };
    let Some(bundle_root) = find_app_bundle_root(&executable) else {
        return false;
    };
    let resources_dir = bundle_root.join("Contents/Resources");
    let repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    for bundled in [
        resources_dir.join("Web/code-server"),
        resources_dir.join("code-server"),
    ] {
        let bundled = bundled.canonicalize().unwrap_or(bundled);
        if repo_root == bundled || repo_root.starts_with(&bundled) {
            return true;
        }
    }
    false
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn source_code_server_is_bundled_repo_root(repo_root: &Path) -> bool {
    // Flat staged layouts bundle code-server beside the executable.
    let Ok(executable) = env::current_exe() else {
        return false;
    };
    let Some(exe_dir) = executable.parent() else {
        return false;
    };
    let repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let bundled = exe_dir.join("code-server");
    let bundled = bundled.canonicalize().unwrap_or(bundled);
    repo_root == bundled || repo_root.starts_with(&bundled)
}

pub(crate) fn source_code_server_runtime_environment(repo_root: &Path) -> HashMap<String, String> {
    let mut environment: HashMap<String, String> = env::vars().collect();
    environment.insert("VSCODE_IPC_HOOK_CLI".to_string(), String::new());
    environment.remove("CODE_SERVER_PARENT_PID");
    if source_code_server_is_bundled_runtime(repo_root) {
        environment.remove("VSCODE_DEV");
        environment.insert("NODE_ENV".to_string(), "production".to_string());
    } else {
        environment.insert("VSCODE_DEV".to_string(), "1".to_string());
        environment.insert("NODE_ENV".to_string(), "development".to_string());
        let mut path_entries = vec![repo_root.join("node_modules/.bin")];
        match environment.get("PATH") {
            Some(path) => path_entries.extend(env::split_paths(path)),
            None => path_entries.extend(env::split_paths(
                "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            )),
        }
        if let Ok(path) = env::join_paths(path_entries) {
            environment.insert("PATH".to_string(), path.to_string_lossy().to_string());
        }
    }
    environment
}

pub(crate) fn source_code_server_is_bundled_runtime(repo_root: &Path) -> bool {
    repo_root.join("lib/node").is_file()
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SourceCodeServerReadiness {
    pub(crate) http_runtime_ready: bool,
    pub(crate) prompt_editor_ipc_ready: bool,
}

impl SourceCodeServerReadiness {
    pub(crate) fn is_ready(self) -> bool {
        self.http_runtime_ready && self.prompt_editor_ipc_ready
    }
}

pub(crate) fn source_code_server_readiness_at(port: u16) -> SourceCodeServerReadiness {
    let Ok(address) =
        format!("{}:{}", SOURCE_CODE_SERVER_EDITOR_HOST, port).parse::<std::net::SocketAddr>()
    else {
        return SourceCodeServerReadiness::default();
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(1)) else {
        return SourceCodeServerReadiness::default();
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let request = format!(
        "GET /healthz HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        SOURCE_CODE_SERVER_EDITOR_HOST, port
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return SourceCodeServerReadiness::default();
    }
    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() {
        return SourceCodeServerReadiness::default();
    }
    let http_runtime_ready =
        response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200");
    if !http_runtime_ready {
        return SourceCodeServerReadiness::default();
    }
    let prompt_editor_ipc_ready = response
        .split_once("\r\n\r\n")
        .and_then(|(_, body)| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|body| {
            body.get("promptEditorIpcReady")
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(false);
    SourceCodeServerReadiness {
        http_runtime_ready,
        prompt_editor_ipc_ready,
    }
}

pub(crate) fn source_code_server_health_check_at(port: u16) -> bool {
    source_code_server_readiness_at(port).http_runtime_ready
}

pub(crate) fn source_code_server_health_check() -> bool {
    source_code_server_health_check_at(source_code_server_editor_port())
}

/// CDXC:CodeEditor 2026-09-13 WHY:
/// The editor has its own HTTP listener, separate from gxserver and CEF debugging.
/// Sharing 3777 made normal Ghostex load Ghostex-3's remote settings and file service.
pub(crate) fn source_code_server_editor_port() -> u16 {
    env::var("GHOSTEX_CODE_SERVER_PORT")
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
        .filter(|port| *port != 0)
        .unwrap_or(SOURCE_CODE_SERVER_EDITOR_PORT)
}

pub(crate) fn source_code_server_editor_origin() -> String {
    format!(
        "http://{SOURCE_CODE_SERVER_EDITOR_HOST}:{}",
        source_code_server_editor_port()
    )
}

pub(crate) fn source_code_server_wait_until_responsive_at(
    port: u16,
    timeout: Duration,
) -> SourceCodeServerReadiness {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let readiness = source_code_server_readiness_at(port);
        if readiness.is_ready() {
            return readiness;
        }
        thread::sleep(SOURCE_CODE_SERVER_HEALTH_POLL_INTERVAL);
    }
    source_code_server_readiness_at(port)
}

pub(crate) fn source_code_server_wait_until_responsive(
    timeout: Duration,
) -> SourceCodeServerReadiness {
    source_code_server_wait_until_responsive_at(source_code_server_editor_port(), timeout)
}

pub(crate) fn source_code_server_wait_until_not_responsive(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !source_code_server_health_check() {
            return true;
        }
        thread::sleep(SOURCE_CODE_SERVER_HEALTH_POLL_INTERVAL);
    }
    !source_code_server_health_check()
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn source_code_server_open_file_in_existing_instance(
    file_path: &Path,
    line: Option<u32>,
    column: Option<u32>,
    workspace_folder: &Path,
) -> Result<(), String> {
    /*
    Hand the validated file to code-server's process-local open queue. If the
    matching Source workbench has not registered its VS Code socket yet, the
    session manager keeps only the newest request under this fixed key and
    delivers it on registration. Never launch a second editor server or fall
    back to an external application.

    CDXC:CodeEditor 2026-09-11 WHY:
    The queue matched the workbench by the file's location, so a chat link to a file outside the project (a home-folder config, another checkout) never found a workbench and stayed queued forever after the "Opening file in Code view" toast.
    The request now names the project folder whose Code view the app just switched to, and code-server delivers the file to the workbench rooted there.
    SEE-ALSO: .dependencies/code-server/src/node/vscodeSocket.ts (queued open delivery), apps/desktop/src/windows_terminal_backend.rs (WSL variant).
    */
    let repo_root = source_code_server_resolve_repo_root()?;
    let node_path = source_code_server_resolve_node_path(&repo_root)?;
    let entrypoint_path = repo_root.join("out/node/entry.js");
    let (user_data_dir, _) = source_code_server_runtime_storage()?;
    let session_socket = user_data_dir.join("code-server-ipc.sock");
    let mut command = Command::new(&node_path);
    command
        .arg(&entrypoint_path)
        .arg("--user-data-dir")
        .arg(&user_data_dir)
        .arg("--session-socket")
        .arg(&session_socket)
        .arg("--reuse-window")
        .arg("--queue-open")
        .arg("--open-request-key")
        .arg("ghostex-source-file-open")
        .arg("--open-workspace-folder")
        .arg(workspace_folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .envs(source_code_server_runtime_environment(&repo_root));
    if let Some(line) = line {
        let mut target = file_path.as_os_str().to_owned();
        target.push(format!(":{line}"));
        if let Some(column) = column {
            target.push(format!(":{column}"));
        }
        // The open pipe already enables goto-line parsing. code-server's
        // wrapper does not accept VS Code's `--goto` flag, so the positioned
        // target must remain a positional argument.
        command.arg(target);
    } else {
        command.arg(file_path);
    }
    if let Some(parent) = file_path.parent() {
        command.current_dir(parent);
    }
    if command.status().is_ok_and(|status| status.success()) {
        Ok(())
    } else {
        Err("Ghostex Code could not accept the file-open request.".to_string())
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn source_code_server_open_file_in_existing_instance(
    file_path: &Path,
    line: Option<u32>,
    column: Option<u32>,
    workspace_folder: &Path,
) -> Result<(), String> {
    let mut command = windows_terminal_backend::source_code_server_open_file_command(
        file_path,
        line,
        column,
        workspace_folder,
        SOURCE_CODE_SERVER_DEFAULT_NODE_MAJOR,
    )?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if command.status().is_ok_and(|status| status.success()) {
        Ok(())
    } else {
        Err("Ghostex Code could not accept the file-open request.".to_string())
    }
}
