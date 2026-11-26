//! The Windows half of the start: native PowerShell and Windows driven from WSL.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::Start;
use crate::bail;
use crate::util::{self, bun, env_trimmed, output, root, Log, Res};

pub fn powershell(is_wsl: bool) -> &'static str {
    if is_wsl {
        "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"
    } else {
        "powershell.exe"
    }
}

pub fn system_executable(name: &str, is_wsl: bool) -> String {
    if is_wsl {
        format!("/mnt/c/Windows/System32/{name}.exe")
    } else {
        format!("{name}.exe")
    }
}

/// Which install a Windows start writes: the per-user release layout (default), Program Files (`--machine`), or `GHOSTEX_INSTALL_DIR`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InstallScope {
    User,
    Machine,
    Custom,
}

impl InstallScope {
    /// The `-Scope` value install-windows-gpui.ps1 takes.
    pub fn script_name(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Machine => "Machine",
            Self::Custom => "Custom",
        }
    }
}

/// The installed app folder (where Ghostex.exe lives), as this host and as Windows see it.
pub struct InstallPaths {
    pub host_path: String,
    pub windows_path: String,
    pub scope: InstallScope,
}

/// CDXC:Build 2026-10-07 DECISION:
/// User (Q74 a): "Yes: per-user, Velopack-style, like real users; `--machine` keeps Program Files as an option." A Windows start installs into `%LOCALAPPDATA%\Ghostex\current`, the folder the release's Velopack installer puts the app in (packId Ghostex), with no administrator prompt, the same per-user Start Menu shortcut and AppUserModelID, and the same process paths users have; `--machine` keeps the `C:\Program Files\Ghostex` install for testing that case. Velopack's Update.exe and its update manifest are not faked, so a dev build never updates itself from the release feed.
/// SEE-ALSO: tooling/install-windows-gpui.ps1 (layout, shortcut, moving off Program Files), tooling/release-gpui/windows.ps1 (vpk pack), apps/desktop/src/windows_updater.rs (the AppUserModelID).
///
/// CDXC:Build 2026-09-28 WHY:
/// Windows reads GHOSTEX_INSTALL_DIR, not the generic INSTALL_DIR that Linux honours: toolchains and shells set INSTALL_DIR for their own use, and inheriting it would silently move the Windows install (macOS ignores it for the same reason). Under WSL the value may be a Windows path (`D:/Ghostex/build/local`) or a WSL path.
pub fn resolve_install_paths(is_wsl: bool, machine: bool, app_name: &str) -> Res<InstallPaths> {
    if let Some(configured) = env_trimmed("GHOSTEX_INSTALL_DIR") {
        if machine {
            bail!("--machine and GHOSTEX_INSTALL_DIR both choose where Ghostex installs; use one of them.");
        }
        let looks_windows = configured.starts_with("\\\\")
            || (configured.len() >= 3
                && configured.as_bytes()[0].is_ascii_alphabetic()
                && configured.as_bytes()[1] == b':'
                && matches!(configured.as_bytes()[2], b'\\' | b'/'));
        if is_wsl && looks_windows {
            let windows_path = format!(
                "{}\\{app_name}",
                configured.replace('/', "\\").trim_end_matches('\\')
            );
            return Ok(InstallPaths {
                host_path: wslpath("-u", &windows_path)?,
                windows_path,
                scope: InstallScope::Custom,
            });
        }
        let host_parent = std::path::absolute(&configured)?;
        let windows_parent = if is_wsl {
            wslpath("-w", &host_parent.display().to_string())?
        } else {
            host_parent.display().to_string()
        };
        return Ok(InstallPaths {
            host_path: host_parent.join(app_name).display().to_string(),
            windows_path: format!("{}\\{app_name}", windows_parent.trim_end_matches('\\')),
            scope: InstallScope::Custom,
        });
    }
    let (scope, base) = if machine {
        (InstallScope::Machine, windows_program_files(is_wsl)?)
    } else {
        (InstallScope::User, windows_local_app_data(is_wsl)?)
    };
    let base = base.trim_end_matches('\\');
    let windows_path = match scope {
        InstallScope::User => format!("{base}\\{app_name}\\current"),
        _ => format!("{base}\\{app_name}"),
    };
    let host_path = if is_wsl {
        wslpath("-u", &windows_path)?
    } else {
        windows_path.clone()
    };
    Ok(InstallPaths {
        host_path,
        windows_path,
        scope,
    })
}

/// The folder Windows reports through PowerShell, or the environment variable a native start already has.
fn windows_folder(is_wsl: bool, variable: &str, expression: &str, label: &str) -> Res<String> {
    if !is_wsl {
        if let Some(value) = env_trimmed(variable) {
            return Ok(value);
        }
    }
    let out = output(Command::new(powershell(is_wsl)).current_dir(root()).args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        expression,
    ]))?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || path.is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!(
            "{}",
            if stderr.is_empty() {
                format!("Windows did not report its {label} directory.")
            } else {
                stderr
            }
        );
    }
    Ok(path)
}

fn windows_program_files(is_wsl: bool) -> Res<String> {
    windows_folder(
        is_wsl,
        "ProgramW6432",
        "$dir = $env:ProgramW6432; if (-not $dir) { $dir = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles) }; $dir",
        "Program Files",
    )
}

fn windows_local_app_data(is_wsl: bool) -> Res<String> {
    windows_folder(
        is_wsl,
        "LOCALAPPDATA",
        "[Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)",
        "local application data",
    )
}

fn wslpath(flag: &str, path: &str) -> Res<String> {
    let out = output(
        Command::new("wslpath")
            .arg(flag)
            .arg(path)
            .current_dir(root()),
    )?;
    let converted = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || converted.is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!(
            "{}",
            if stderr.is_empty() {
                format!("Could not convert {path} with wslpath {flag}.")
            } else {
                stderr
            }
        );
    }
    Ok(converted)
}

/// The path Windows sees for a path on this host (a WSL path becomes `\\wsl.localhost\...` or `C:\...`).
pub fn windows_path_for_host_path(host_path: &Path, is_wsl: bool) -> Res<String> {
    if !is_wsl {
        return Ok(host_path.display().to_string());
    }
    wslpath("-w", &host_path.display().to_string())
}

fn windows_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    }
}

/// The release whose WSL2 gxserver runtime this start uses: `app_version` once it is cached or
/// published, otherwise the newest cached earlier release.
///
/// CDXC:PlatformSupport 2026-10-08 WHY:
/// `chore: prepare X.Y.Z` bumps package.json before the release workflow publishes vX.Y.Z (and a cancelled release never publishes it), so a start in that window asked for a runtime asset that does not exist and failed with a 404. The WSL2 runtime only serves WSL projects; until vX.Y.Z is published the start uses the newest runtime it already holds, and the first start after publishing downloads the real one into its own `ghostex-X.Y.Z` folder.
fn published_runtime_version(app_version: &str, arch: &str) -> String {
    let artifacts = root().join("build/runtime-artifacts");
    let archive_name = format!("gxserver-linux-{arch}.tar.gz");
    let cached = |version: &str| {
        artifacts
            .join(format!("ghostex-{version}"))
            .join(arch)
            .join(&archive_name)
            .exists()
    };
    if cached(app_version) {
        return app_version.to_string();
    }
    let curl = if cfg!(windows) { "curl.exe" } else { "curl" };
    let published = output(
        Command::new(curl)
            .args(["-fsIL", "-o"])
            .arg(if cfg!(windows) { "NUL" } else { "/dev/null" })
            .arg(format!(
                "https://github.com/maddada/Ghostex/releases/download/v{app_version}/{archive_name}"
            )),
    )
    .is_ok_and(|result| result.status.success());
    if published {
        return app_version.to_string();
    }
    let parse = |version: &str| -> Option<(u64, u64, u64)> {
        let mut parts = version.split('.').map(|part| part.parse::<u64>().ok());
        Some((parts.next()??, parts.next()??, parts.next()??))
    };
    let Some(wanted) = parse(app_version) else {
        return app_version.to_string();
    };
    fs::read_dir(&artifacts)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let version = name.strip_prefix("ghostex-")?.to_string();
            let parsed = parse(&version)?;
            (parsed < wanted && cached(&version)).then_some((parsed, version))
        })
        .max()
        .map(|(_, version)| version)
        .unwrap_or_else(|| app_version.to_string())
}

/// The WSL2 gxserver runtime and the code-server Source runtime a Windows build bundles.
pub struct RuntimeArchives {
    require_wsl_runtime: bool,
    explicit_gxserver_archive: bool,
    explicit_code_server_archive: bool,
    gxserver_archive: PathBuf,
    code_server_archive: PathBuf,
    code_server_component_version: String,
    code_server_download_tag: String,
    code_server_archive_name: String,
    app_version: String,
}

impl RuntimeArchives {
    pub fn resolve(require_wsl_runtime: bool) -> Res<Self> {
        let arch = windows_arch();
        let package: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root().join("package.json"))?)?;
        let app_version = package
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        // The code-server component identity is release tooling (JavaScript until the release port); its CLI prints the names the release published.
        let identity = output(&mut bun([
            "tooling/release-gpui/code-server-component-identity.mjs",
            "--root",
            ".dependencies/code-server",
            "--platform",
            &format!("linux-{arch}"),
            "--github-output",
        ]))?;
        if !identity.status.success() {
            bail!(
                "Could not resolve the code-server component identity: {}",
                String::from_utf8_lossy(&identity.stderr).trim()
            );
        }
        let identity = String::from_utf8_lossy(&identity.stdout).into_owned();
        let field = |key: &str| {
            identity
                .lines()
                .find_map(|line| line.strip_prefix(&format!("{key}=")).map(str::to_string))
                .ok_or_else(|| format!("code-server-component-identity.mjs printed no {key}"))
        };
        let explicit_gxserver = env_trimmed("GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE");
        let explicit_code_server = env_trimmed("GHOSTEX_WINDOWS_WSL_CODE_SERVER_ARCHIVE");
        let code_server_archive_name = field("archive_name")?;
        // CDXC:PlatformSupport 2026-08-09:
        // The gxserver release asset keeps the same filename across Ghostex releases, so an architecture-only cache could reuse a stale runtime from an earlier release. Scope the cache to the immutable Ghostex release tag so a version can only consume the WSL runtime published with that version.
        let runtime_version = match &explicit_gxserver {
            Some(_) => app_version.clone(),
            None => published_runtime_version(&app_version, arch),
        };
        let gxserver_archive = match &explicit_gxserver {
            Some(path) => std::path::absolute(path)?,
            None => root()
                .join("build/runtime-artifacts")
                .join(format!("ghostex-{runtime_version}"))
                .join(arch)
                .join(format!("gxserver-linux-{arch}.tar.gz")),
        };
        let code_server_archive = match &explicit_code_server {
            Some(path) => std::path::absolute(path)?,
            None => root()
                .join("build/runtime-artifacts")
                .join(arch)
                .join(&code_server_archive_name),
        };
        Ok(Self {
            require_wsl_runtime,
            explicit_gxserver_archive: explicit_gxserver.is_some(),
            explicit_code_server_archive: explicit_code_server.is_some(),
            gxserver_archive,
            code_server_archive,
            code_server_component_version: field("component_version")?,
            code_server_download_tag: field("download_tag")?,
            code_server_archive_name,
            app_version: runtime_version,
        })
    }

    pub fn build_environment(&self, is_wsl: bool, verbose: bool) -> Vec<(String, String)> {
        let mut environment = vec![
            (
                "GHOSTEX_WINDOWS_ARCH".to_string(),
                windows_arch().to_string(),
            ),
            (
                "GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME".into(),
                if self.require_wsl_runtime { "1" } else { "0" }.into(),
            ),
            (
                "GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE".into(),
                if self.require_wsl_runtime || self.explicit_gxserver_archive {
                    self.gxserver_archive.display().to_string()
                } else {
                    String::new()
                },
            ),
            (
                "GHOSTEX_WINDOWS_WSL_CODE_SERVER_ARCHIVE".into(),
                self.code_server_archive.display().to_string(),
            ),
            (
                "GHOSTEX_CODE_SERVER_COMPONENT_VERSION".into(),
                self.code_server_component_version.clone(),
            ),
        ];
        if is_wsl && !verbose {
            environment.push((
                "GHOSTEX_WINDOWS_BUILD_PROGRESS_PATH".into(),
                format!("/proc/{}/fd/1", std::process::id()),
            ));
        }
        environment
    }

    /// CDXC:PlatformSupport 2026-09-22 WHY:
    /// The gxserver runtime is a public release asset, so it is fetched straight from the release download URL. `gh release download` was used here before, but the GitHub CLI refuses to run unauthenticated and answers 401 whenever its stored token has expired, which blocked the start on a machine that never needed GitHub credentials to build the app.
    pub fn ensure_downloaded(&self, log: &Log) -> Res {
        if (self.require_wsl_runtime || self.explicit_gxserver_archive)
            && !self.gxserver_archive.exists()
        {
            if self.explicit_gxserver_archive {
                bail!(
                    "GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE does not exist: {}",
                    self.gxserver_archive.display()
                );
            }
            fs::create_dir_all(
                self.gxserver_archive
                    .parent()
                    .expect("archive has a parent"),
            )?;
            log.step(&format!(
                "Downloading the Ghostex {} WSL2 runtime...",
                self.app_version
            ));
            let name = self
                .gxserver_archive
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            util::download(
                &format!(
                    "https://github.com/maddada/Ghostex/releases/download/v{}/{name}",
                    self.app_version
                ),
                &self.gxserver_archive,
            )?;
        }
        let sidecar = PathBuf::from(format!("{}.sha256", self.code_server_archive.display()));
        let has_archive = self.code_server_archive.exists();
        let has_sidecar = sidecar.exists();
        if has_archive != has_sidecar {
            bail!(
                "The cached WSL2 Source runtime must contain both {} and its filename-bound .sha256 sidecar.",
                self.code_server_archive_name
            );
        }
        if !has_archive {
            if self.explicit_code_server_archive {
                bail!(
                    "GHOSTEX_WINDOWS_WSL_CODE_SERVER_ARCHIVE and its filename-bound .sha256 sidecar must exist: {}",
                    self.code_server_archive.display()
                );
            }
            fs::create_dir_all(
                self.code_server_archive
                    .parent()
                    .expect("archive has a parent"),
            )?;
            log.step(&format!(
                "Downloading the Ghostex WSL2 Source runtime {}...",
                self.code_server_component_version
            ));
            let repository = components_repo()?;
            let base = format!(
                "https://github.com/{repository}/releases/download/{}",
                self.code_server_download_tag
            );
            util::download(
                &format!("{base}/{}", self.code_server_archive_name),
                &self.code_server_archive,
            )?;
            util::download(
                &format!("{base}/{}.sha256", self.code_server_archive_name),
                &sidecar,
            )?;
        }
        Ok(())
    }
}

/// The public repository on-demand components are published to (GHOSTEX_COMPONENTS_REPO overrides it); release tooling owns the name.
fn components_repo() -> Res<String> {
    let out = output(&mut bun(["tooling/release-gpui/components-repo.mjs"]))?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Set (by the hand-off script) for the start that installs in the desktop session on behalf of a session-0 start, which keeps holding the start lock while it waits.
pub const DESKTOP_HANDOFF_ENV: &str = "GHOSTEX_START_DESKTOP_HANDOFF";

/// Whether this process runs in Windows session 0, the services session, which has no desktop.
pub fn runs_in_services_session() -> Res<bool> {
    #[cfg(windows)]
    {
        Ok(super::windows_native::current_session_id()? == 0)
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

/// A run-once task that runs a script in the signed-in user's desktop session; it is deleted when dropped.
struct DesktopTask {
    name: String,
}

impl DesktopTask {
    fn run(name: String, script: &Path) -> Res<Self> {
        // /IT runs the task only in the user's interactive session, with their desktop token; /SC ONCE never fires on its own because the task is run once with /Run and deleted after.
        let created = output(
            Command::new("schtasks.exe")
                .args(["/Create", "/F", "/IT", "/SC", "ONCE", "/ST", "00:00", "/TN"])
                .arg(&name)
                .arg("/TR")
                .arg(format!("\"{}\"", script.display())),
        )?;
        if !created.status.success() {
            bail!(
                "Could not create the scheduled task that installs in your desktop session: {}",
                String::from_utf8_lossy(&created.stderr).trim()
            );
        }
        let task = Self { name };
        let ran = output(Command::new("schtasks.exe").args(["/Run", "/TN", &task.name]))?;
        if !ran.status.success() {
            bail!(
                "Could not run the scheduled task that installs in your desktop session: {}",
                String::from_utf8_lossy(&ran.stderr).trim()
            );
        }
        Ok(task)
    }
}

impl Drop for DesktopTask {
    fn drop(&mut self) {
        let _ = output(Command::new("schtasks.exe").args(["/Delete", "/F", "/TN", &self.name]));
    }
}

/// Prints whatever was appended to `path` since `offset`.
fn relay_new_output(path: &Path, offset: &mut u64) {
    use std::io::{Read, Seek, SeekFrom, Write};
    let Ok(mut file) = fs::File::open(path) else {
        return;
    };
    if file.seek(SeekFrom::Start(*offset)).is_err() {
        return;
    }
    let mut appended = Vec::new();
    if file.read_to_end(&mut appended).is_ok() && !appended.is_empty() {
        *offset += appended.len() as u64;
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(&appended);
        let _ = stdout.flush();
    }
}

impl Start {
    /// CDXC:Build 2026-10-03 WHY:
    /// A start running in Windows session 0 (an SSH shell, or a Ghostex terminal whose gxserver was started over SSH) has no desktop: it cannot show the administrator prompt the Program Files install needs, taskkill cannot ask the app's window to close, and the app it launches runs with no visible window. Such a start builds as usual and then runs `cargo xtask start --install-only` in the signed-in user's desktop session through a run-once interactive scheduled task, relaying its output and failing with its result. The relaunched app also gets the desktop session's environment instead of the terminal's.
    pub fn hand_off_install_to_desktop(&self) -> Res {
        self.log.step(&format!(
            "Installing {} from your desktop session...",
            self.app_name
        ));
        self.log.detail("This start runs in Windows session 0 (SSH, or a terminal of a gxserver started over SSH), which has no desktop.");
        if self.opts.machine {
            self.log
                .detail("Approve the administrator prompt on the desktop when it appears.");
        }
        let handoff_dir = root()
            .join("build")
            .join("local-start-handoff")
            .join(std::process::id().to_string());
        let _ = fs::remove_dir_all(&handoff_dir);
        fs::create_dir_all(&handoff_dir)?;
        let log_path = handoff_dir.join("install.log");
        let exit_code_path = handoff_dir.join("exit-code");
        let script_path = handoff_dir.join("install.cmd");
        let mut arguments = vec!["start".to_string(), "--install-only".to_string()];
        if self.log.verbose {
            arguments.push("--verbose".into());
        }
        if self.opts.profile {
            arguments.push("--profile".into());
        }
        if self.opts.machine {
            arguments.push("--machine".into());
        }
        if let Some(config) = &self.isolated {
            arguments.push(format!("--isolated={}", config.variant));
        }
        let mut script = format!(
            "@echo off\r\ntitle Installing {} (cargo xtask start)\r\ncd /d \"{}\"\r\nset \"{DESKTOP_HANDOFF_ENV}=1\"\r\n",
            self.app_name,
            root().display()
        );
        // The task starts from the user's own environment; carry over only what decides where the app installs and keeps its state.
        for key in ["GHOSTEX_INSTALL_DIR", "GHOSTEX_HOME"] {
            if let Some(value) = env_trimmed(key) {
                script += &format!("set \"{key}={value}\"\r\n");
            }
        }
        // A test copy's key store opt-in goes only together with its GHOSTEX_HOME (CDXC:Security
        // 2026-11-26 in packages/owner-keys/src/store.rs): never to the user's own app.
        let store = env_trimmed("GHOSTEX_HOME")
            .and(env_trimmed("GHOSTEX_OWNER_KEYS_STORE"))
            .unwrap_or_default();
        // An empty value unsets one the user's own environment might hold.
        script += &format!("set \"GHOSTEX_OWNER_KEYS_STORE={store}\"\r\n");
        script += &format!(
            "\"{}\" {} > \"{}\" 2>&1\r\n> \"{}\" echo %ERRORLEVEL%\r\n",
            std::env::current_exe()?.display(),
            arguments.join(" "),
            log_path.display(),
            exit_code_path.display()
        );
        fs::write(&script_path, script)?;
        let _task = DesktopTask::run(
            format!("Ghostex local start {}", std::process::id()),
            &script_path,
        )?;
        let started = std::time::Instant::now();
        let mut offset = 0u64;
        loop {
            relay_new_output(&log_path, &mut offset);
            if let Some(code) = fs::read_to_string(&exit_code_path)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
            {
                relay_new_output(&log_path, &mut offset);
                if code != 0 {
                    bail!(
                        "The install in your desktop session failed with exit code {code}. Full log: {}",
                        log_path.display()
                    );
                }
                let _ = fs::remove_dir_all(&handoff_dir);
                return Ok(());
            }
            if !log_path.exists() && started.elapsed() > std::time::Duration::from_secs(30) {
                bail!(
                    "Windows did not start the install in a desktop session within 30 seconds. Sign in to this PC's desktop as {}, then rerun this start or run `cargo xtask start --install-only` in a terminal there.",
                    env_trimmed("USERNAME").unwrap_or_else(|| "this user".into())
                );
            }
            if started.elapsed() > std::time::Duration::from_secs(30 * 60) {
                bail!(
                    "The install in your desktop session did not finish within 30 minutes (was the administrator prompt answered?). It keeps running there and writes its result to {}",
                    log_path.display()
                );
            }
            util::sleep_ms(250);
        }
    }

    /// CDXC:PlatformSupport 2026-09-18:
    /// Local Windows development can be driven entirely by the WSL bash launcher. Query the product-specific image names with tasklist instead of using a PowerShell CIM pipeline; no other application ships these executable names, and taskkill closes each matching process before the staged directory is replaced.
    /// Supersedes the 2026-08-02 two-name list, which omitted ghostex-gpui-runtime.exe. An installed release runs Ghostex.exe only as the CEF-free bootstrap; the long-lived app is the runtime it launches. Killing just the bootstrap and the CEF helpers left the runtime alive, and it respawned its helpers faster than the exit wait polled, so every start failed with "Ghostex did not exit". GhostexEditor.exe is bundled under resources/ and holds open handles inside the install directory, which Windows will not let the installer replace.
    pub fn windows_app_pids(&self) -> Vec<String> {
        let mut pids = Vec::new();
        for image in [
            "Ghostex.exe",
            "ghostex-gpui-runtime.exe",
            "ghostex-gpui-cef-helper.exe",
            "GhostexEditor.exe",
        ] {
            let Some(out) = util::stdout_if_ok(
                Command::new(system_executable("tasklist", self.is_wsl)).args([
                    "/FI",
                    &format!("IMAGENAME eq {image}"),
                    "/FO",
                    "CSV",
                    "/NH",
                ]),
            ) else {
                continue;
            };
            for line in out.lines() {
                // "Ghostex.exe","1234",...
                let mut fields = line.split("\",\"");
                let (Some(first), Some(pid)) = (fields.next(), fields.next()) else {
                    continue;
                };
                // CDXC:PlatformSupport 2026-10-02 WHY: tasklist's IMAGENAME filter ignores case, so "Ghostex.exe" also matched the `ghostex.exe` CLI, including one a SYSTEM shell (an SSH remote call) ran in session 0 that taskkill cannot end, and the start refused to install. Only the exact image name is the app.
                if first.strip_prefix('"') == Some(image)
                    && !pid.is_empty()
                    && pid.bytes().all(|b| b.is_ascii_digit())
                {
                    pids.push(pid.to_string());
                }
            }
        }
        pids
    }

    /// CDXC:PlatformSupport 2026-09-18 WHY:
    /// Kill each app process by pid without taskkill /T, the same per-pid close macOS does. The runtime is the parent of gxserver.exe, and gxserver parents live wmx.exe terminal sessions, so a tree kill ended every agent running in those sessions. Every app process is already named in windows_app_pids, so the tree walk was never needed to reach the CEF helpers.
    pub fn terminate_windows_pids(&self, pids: &[String], force: bool) {
        for pid in pids {
            let mut kill = Command::new(system_executable("taskkill", self.is_wsl));
            kill.args(["/PID", pid]);
            if force {
                kill.arg("/F");
            }
            let _ = output(&mut kill);
        }
    }

    pub fn install_windows_app(&self) -> Res {
        let installed = self.windows_installed_app_path.clone().unwrap_or_default();
        let scope = self.windows_install_scope.unwrap_or(InstallScope::User);
        self.log
            .step(&format!("Installing {} to {installed}...", self.app_name));
        if scope == InstallScope::Machine {
            self.log
                .detail("The Program Files location requires administrator approval.");
        }
        let installer = root().join("tooling").join("install-windows-gpui.ps1");
        let out = output(
            Command::new(powershell(self.is_wsl))
                .current_dir(root())
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(windows_path_for_host_path(&installer, self.is_wsl)?)
                .arg("-StagedAppPath")
                .arg(windows_path_for_host_path(&self.app_path, self.is_wsl)?)
                .arg("-InstallDir")
                .arg(&installed)
                .arg("-Scope")
                .arg(scope.script_name()),
        )?;
        print!("{}", String::from_utf8_lossy(&out.stdout));
        eprint!("{}", String::from_utf8_lossy(&out.stderr));
        if !out.status.success() {
            bail!(
                "The Windows Ghostex installer failed with exit code {}.",
                out.status.code().unwrap_or(1)
            );
        }
        if !self.installed_app_path.join("Ghostex.exe").exists() {
            bail!("The installed Ghostex executable is missing at {installed}\\Ghostex.exe.");
        }
        self.log
            .detail("Installed app and Start Menu shortcut are ready.");
        Ok(())
    }

    /// CDXC:Build 2026-10-04 WHY:
    /// A native Windows start hands the launch to the Explorer shell instead of spawning the app as its own child. `bun run start` puts its children in a kill-on-close job (libuv's), and `cargo run` puts xtask in a job that refuses CREATE_BREAKAWAY_FROM_JOB, so a Ghostex spawned here, even DETACHED_PROCESS, was killed the moment bun exited. The spawned app also inherited the caller's pipe handles (keeping a `| Select-Object` pipeline open while it lived) and the terminal's GHOSTEX_* environment. A run-once scheduled task is not used here because Task Scheduler ends a task's process after its 72-hour limit.
    pub fn launch_windows_app(&self) -> Res {
        let executable = self.installed_app_path.join("Ghostex.exe");
        #[cfg(windows)]
        {
            let arguments = if self.opts.profile { "--profile" } else { "" };
            super::windows_native::shell_execute_from_desktop(
                &executable,
                arguments,
                &self.installed_app_path,
            )
            .map_err(|error| {
                format!(
                    "Could not launch {} through Explorer: {error}",
                    executable.display()
                )
            })?;
            println!("Launched {} through Explorer.", executable.display());
            Ok(())
        }
        // A start driven from WSL launches the Windows app through WSL interop.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let mut launch = Command::new(&executable);
            launch.current_dir(&self.installed_app_path);
            if self.opts.profile {
                launch.arg("--profile");
            }
            launch
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .process_group(0);
            let child = launch
                .spawn()
                .map_err(|error| util::spawn_error(&launch, error))?;
            println!("Launched {} (pid {}).", executable.display(), child.id());
            Ok(())
        }
    }

    /// CDXC:ServerDaemon 2026-09-23 WHY:
    /// A previous daemon's shutdown could remove its replacement's runtime metadata while the replacement still owned its listening socket. Discover Windows listeners from the exact installed, staged, or managed CLI package executable's process, and retain that endpoint through shutdown polling; metadata disappearance is not proof the server stopped. This supersedes discovery solely from runtime/server.json.
    ///
    /// CDXC:ServerDaemon 2026-10-03 WHY:
    /// The listeners and process paths are read with GetExtendedTcpTable and QueryFullProcessImageNameW. The PowerShell `Get-NetTCPConnection` used before goes through CIM, which refuses an SSH session's network logon token ("Cannot connect to CIM server. Access denied"), so every start run from a Ghostex terminal hosted by an SSH-started gxserver failed here after a full build.
    ///
    /// CDXC:Build 2026-10-07 WHY:
    /// The first per-user start on a machine that ran the Program Files install (or a `--machine` start after per-user ones) finds the other install's gxserver holding the port. That one is always stopped, even when it reports the same build identity, so the app launched from the new folder starts its own gxserver and wmx and the old install stops being used. Live wmx sessions keep running from the old folder's image (the same wire generation serves them).
    /// Each endpoint comes with whether a server already running the bundled build may be kept.
    pub fn windows_gxserver_endpoints(&self) -> Res<Vec<(String, Option<u64>, bool)>> {
        let data_dir = crate::gxserver::explicit_ghostex_home().unwrap_or_else(|| {
            crate::gxserver::local_app_data()
                .join("Ghostex")
                .join("Data")
        });
        let server_in = |dir: &Path| {
            dir.join("resources")
                .join("native")
                .join("gxserver.exe")
                .display()
                .to_string()
                .to_lowercase()
        };
        let server_paths: Vec<String> = [&self.installed_app_path, &self.app_path]
            .iter()
            .map(|dir| server_in(dir))
            .chain(std::iter::once(
                data_dir
                    .join("gxserver")
                    .join("package")
                    .join("bin")
                    .join("gxserver.exe")
                    .display()
                    .to_string()
                    .to_lowercase(),
            ))
            .collect();
        let other_install_servers: Vec<String> = [
            env_trimmed("LOCALAPPDATA")
                .map(|dir| PathBuf::from(dir).join(&self.app_name).join("current")),
            env_trimmed("ProgramW6432")
                .or_else(|| env_trimmed("ProgramFiles"))
                .map(|dir| PathBuf::from(dir).join(&self.app_name)),
        ]
        .into_iter()
        .flatten()
        .map(|dir| server_in(&dir))
        .filter(|path| !server_paths.contains(path))
        .collect();
        /*
        CDXC:ServerDaemon 2026-10-12 WHY:
        QueryFullProcessImageNameW reports where a running image lives now, and refreshing the managed package or installing the app renames a running gxserver.exe into a `.retired-native` folder. The superseded daemon that held the port on 2026-10-12 was therefore matched by no path, the 04:11 start said "No running gxserver control plane found" and launched the app beside it. A gxserver running from one of those folders is always stopped: a newer build replaced it.
        */
        let retired_server_folders: Vec<String> = [
            data_dir.join("gxserver").join("package"),
            self.installed_app_path.clone(),
            self.app_path.clone(),
        ]
        .iter()
        .map(|dir| {
            format!(
                "{}{}",
                dir.join(".retired-native")
                    .display()
                    .to_string()
                    .to_lowercase(),
                std::path::MAIN_SEPARATOR
            )
        })
        .collect();
        let retired_server = |path: &str| {
            path.ends_with("gxserver.exe")
                && retired_server_folders
                    .iter()
                    .any(|folder| path.starts_with(folder.as_str()))
        };
        #[cfg(windows)]
        {
            let listeners = super::windows_native::loopback_listeners().map_err(|error| {
                format!("Could not inspect Windows gxserver listeners: {error}")
            })?;
            Ok(listeners
                .into_iter()
                .filter_map(|(pid, port)| {
                    let path = super::windows_native::process_image_path(pid)?.to_lowercase();
                    let keep_same_build = if server_paths.contains(&path) {
                        true
                    } else if other_install_servers.contains(&path) || retired_server(&path) {
                        false
                    } else {
                        return None;
                    };
                    Some((
                        format!("http://127.0.0.1:{port}"),
                        Some(u64::from(pid)),
                        keep_same_build,
                    ))
                })
                .collect())
        }
        #[cfg(not(windows))]
        {
            let _ = (server_paths, other_install_servers, retired_server);
            bail!("gxserver listeners are inspected only by a native Windows start.")
        }
    }
}
