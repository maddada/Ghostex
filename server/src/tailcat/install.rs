use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Deserialize;

use crate::paths::GxserverPaths;

use super::{
    binary::managed_tailcat_binary, read_tailcat_binary_version, resolve_client_tailcat_binary,
    resolve_host_tailcat_binary, start_tailcat_from_persisted_state, TailcatRuntime,
};

/// The tailcat version this gxserver installs, and the newest Settings > Updates offers.
///
/// CDXC:RemotePairing 2026-09-15 SEE-ALSO:
/// apps/mobile/tailcat-bridge/go.mod pins the tailcat library the phone app tunnels with; keep it at least as new as this helper.
/// v0.6.0 serves addresses with a WireGuard pre-shared key that clients v0.5.0 and older cannot complete a handshake with, which is exactly the skew that broke phone pairing on 2026-09-15.
/// Raising it offers every Mac and Windows computer an Update of its helper (server/src/updates/items.rs); check first that the new version serves the same address from the same key, or Update keeps the old helper (`update`).
macro_rules! tailcat_version {
    () => {
        "v0.6.0"
    };
}
pub(crate) const TAILCAT_VERSION: &str = tailcat_version!();
const TAILCAT_MODULE: &str = concat!(
    "github.com/tailscale/tailcat/cmd/tailcat@",
    tailcat_version!()
);

static INSTALLATION: Mutex<InstallationStatus> = Mutex::new(InstallationStatus {
    running: false,
    updating: false,
    progress: None,
    error: None,
});

#[derive(Clone)]
pub(crate) struct InstallationStatus {
    pub running: bool,
    /// The running job is an Update of a helper that is already installed.
    pub updating: bool,
    pub progress: Option<String>,
    /// What went wrong, in plain words (the detail goes to gxserver's log).
    pub error: Option<String>,
}

pub(crate) fn installation_status() -> InstallationStatus {
    INSTALLATION.lock().unwrap().clone()
}

/// A failure the user reads (`plain`) and the detail gxserver logs.
struct Failure {
    plain: String,
    detail: String,
}

impl Failure {
    fn new(plain: impl Into<String>, detail: impl std::fmt::Display) -> Self {
        Self {
            plain: plain.into(),
            detail: detail.to_string(),
        }
    }
}

type Step<T> = Result<T, Failure>;

const NO_NETWORK: &str = "Ghostex couldn't download what the Easy Connect helper needs. Check that this computer is online, then try again.";
const BUILD_FAILED: &str = "The Easy Connect helper didn't build on this computer. Try again; if it fails the same way, send feedback so we can look at it.";

/// Whether the build's output says it couldn't reach the network (the Go module proxy).
fn looks_offline(output: &str) -> bool {
    let output = output.to_ascii_lowercase();
    [
        "dial tcp",
        "no such host",
        "i/o timeout",
        "connection refused",
        "network is unreachable",
        "tls handshake timeout",
        "proxy.golang.org",
    ]
    .iter()
    .any(|sign| output.contains(sign))
}

/*
CDXC:RemotePairing 2026-09-05 DECISION:
User: install the Tailcat CLI with one click instead of copying a command into a terminal.
The job belongs to the daemon so closing Settings does not cancel it; status polling reports progress and failures across reopened pages.

CDXC:RemotePairing 2026-11-11 DECISION:
GitHub #559: a Mac had Easy Connect on with no helper and no install running, and the only words were "Reinstall Ghostex", which cannot fix it (the Mac app does not carry the helper). The user: "please make this have good ux. i dont want to bundle tailcat tho since it'll increase size of app. also please add tailcat in updates system so we can click to update it when update comes". So the helper stays a separate install: Connect a phone and Settings > Remote say plainly that it isn't installed yet and offer Install, every step reports here (downloading with a percentage, building, done), a failure says in plain words what to do, and Settings > Updates lists the helper with Update when this gxserver pins a newer version than the one installed (`start_update`).
*/
/// Installs the helper when this computer has none Easy Connect can use; once it is in place,
/// Easy Connect starts on it if it is turned on.
pub(crate) fn start_installation(paths: GxserverPaths, runtime: TailcatRuntime) {
    start_job(paths, runtime, false);
}

/// Builds the pinned helper again over Ghostex's own install and, when Easy Connect hosts with
/// it, starts the helpers again on it (`update`). `false` when an install or update already runs.
pub(crate) fn start_update(paths: GxserverPaths, runtime: TailcatRuntime) -> bool {
    start_job(paths, runtime, true)
}

fn start_job(paths: GxserverPaths, runtime: TailcatRuntime, updating: bool) -> bool {
    let mut status = INSTALLATION.lock().unwrap();
    if status.running {
        return false;
    }
    *status = InstallationStatus {
        running: true,
        updating,
        progress: Some(if updating {
            "Getting ready to update the Easy Connect helper…".into()
        } else {
            "Getting ready to install the Easy Connect helper…".into()
        }),
        error: None,
    };
    drop(status);
    tokio::spawn(async move {
        let outcome = tokio::task::spawn_blocking(move || {
            if updating {
                update(&paths, &runtime)
            } else {
                install(&paths)?;
                progress("Starting Easy Connect…");
                start_tailcat_from_persisted_state(&paths, &runtime);
                Ok(())
            }
        })
        .await;
        let error = match outcome {
            Ok(Ok(())) => None,
            Ok(Err(failure)) => {
                eprintln!(
                    "[gxserver] Easy Connect helper {} failed: {}",
                    if updating { "update" } else { "install" },
                    failure.detail
                );
                Some(failure.plain)
            }
            Err(error) => {
                eprintln!("[gxserver] Easy Connect helper job stopped: {error}");
                Some(
                    "The Easy Connect helper's install stopped before it finished. Try again."
                        .into(),
                )
            }
        };
        *INSTALLATION.lock().unwrap() = InstallationStatus {
            running: false,
            updating: false,
            progress: None,
            error,
        };
        // Settings > Updates reads the helper's new version (or its absence) on its next ask.
        crate::updates::items::invalidate();
    });
    true
}

fn progress(message: &str) {
    INSTALLATION.lock().unwrap().progress = Some(message.into());
}

struct InstallDirectory(PathBuf);

impl Drop for InstallDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn install(paths: &GxserverPaths) -> Step<()> {
    // An older helper elsewhere (`~/go/bin`) does not count: it can't dial a current server.
    if resolve_client_tailcat_binary().is_some() && resolve_host_tailcat_binary(paths).is_some() {
        return Ok(());
    }
    let directory = build_directory(paths)?;
    let built = build(&directory.0)?;
    progress("Finishing up…");
    let destination = managed_tailcat_binary();
    let parent = destination
        .parent()
        .ok_or_else(|| Failure::new(BUILD_FAILED, "missing helper directory"))?;
    let staged = stage_beside(&built, parent)?;
    let result = fs::rename(&staged, &destination);
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result.map_err(|error| save_failure(&error))?;
    Ok(())
}

fn build_directory(paths: &GxserverPaths) -> Step<InstallDirectory> {
    let directory = InstallDirectory(
        paths
            .app_cache_dir
            .join("tailcat-install")
            .join(uuid::Uuid::new_v4().to_string()),
    );
    fs::create_dir_all(&directory.0).map_err(|error| save_failure(&error))?;
    Ok(directory)
}

fn save_failure(error: &std::io::Error) -> Failure {
    Failure::new(
        format!("Ghostex couldn't save the Easy Connect helper on this computer ({error}). Check that the disk has room, then try again."),
        error,
    )
}

/// Downloads a private Go toolchain and builds the pinned helper with it; returns the built file.
fn build(directory: &Path) -> Step<PathBuf> {
    let go = download_toolchain(directory)?;
    progress("Building the Easy Connect helper. This takes a few minutes…");
    let bin_dir = directory.join("bin");
    fs::create_dir_all(&bin_dir).map_err(|error| save_failure(&error))?;
    // CDXC:RemotePairing 2026-09-05 WHY: A private Go toolchain makes the install work without Homebrew, developer tools, or changes to the user's Go environment, including on macOS where upstream does not ship a binary archive.
    run_command(
        Command::new(go)
            .args(["install", TAILCAT_MODULE])
            .current_dir(directory)
            .env("GOENV", "off")
            .env("GOWORK", "off")
            .env("GOFLAGS", "-modcacherw")
            .env("GOMAXPROCS", "2")
            .env("GOOS", go_os())
            .env("GOARCH", go_arch()?)
            .env("GOROOT", directory.join("go"))
            .env("GOTOOLCHAIN", "local")
            .env("GOPATH", directory.join("gopath"))
            .env("GOCACHE", directory.join("go-cache"))
            .env("GOBIN", &bin_dir)
            .env("CGO_ENABLED", "0"),
        directory,
        Duration::from_secs(15 * 60),
    )?;
    let built = bin_dir.join(
        managed_tailcat_binary()
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("tailcat")),
    );
    if read_tailcat_binary_version(&built).is_none() {
        return Err(Failure::new(
            BUILD_FAILED,
            "the built helper did not answer its version check",
        ));
    }
    Ok(built)
}

/// A copy of `built` in `parent` under a hidden name, ready to be renamed into place.
fn stage_beside(built: &Path, parent: &Path) -> Step<PathBuf> {
    fs::create_dir_all(parent).map_err(|error| save_failure(&error))?;
    let staged = parent.join(format!(".tailcat-{}", uuid::Uuid::new_v4()));
    if let Err(error) = fs::copy(built, &staged) {
        let _ = fs::remove_file(&staged);
        return Err(save_failure(&error));
    }
    Ok(staged)
}

/// How long the new helper gets to come up and publish its addresses after an update.
const UPDATE_START_WAIT: Duration = Duration::from_secs(45);

/// The two addresses paired devices dial (the public relays' and the chosen relay's), as the
/// running helpers publish them; `None` for a lane that isn't serving.
fn published_addresses(runtime: &TailcatRuntime) -> (Option<String>, Option<String>) {
    let snapshot = runtime.snapshot();
    (snapshot.token, snapshot.relay.token)
}

/*
CDXC:RemotePairing 2026-11-11 DECISION:
User (GitHub #559): "add tailcat in updates system so we can click to update it when update comes". The brief adds: an update must not drop a connection for longer than a helper restart takes, and must never change pairing, approval or which devices are allowed in.
WHY: the new helper is built next to the running one first (minutes, while every link keeps working), then renamed over Ghostex's install, which a running helper doesn't notice, and the helpers start again once on the new file, with the saved state untouched: the same keys, the same allow-list, the same ports. A tailcat version that writes a different address from the same key would cut off every paired device (v0.6.0 did, with its pre-shared key), so the addresses the new helpers publish are compared with the ones before, and a different address, or a helper that doesn't come up, puts the old file back and starts the helpers on it again.
SEE-ALSO: server/src/tailcat/binary.rs (`select_host_tailcat_binary`: an update never moves hosting to another helper), server/src/updates/items.rs (the Updates row), supervisor.rs (`restart_helpers`).
*/
fn update(paths: &GxserverPaths, runtime: &TailcatRuntime) -> Step<()> {
    let destination = managed_tailcat_binary();
    let parent = destination
        .parent()
        .ok_or_else(|| Failure::new(BUILD_FAILED, "missing helper directory"))?
        .to_path_buf();
    let directory = build_directory(paths)?;
    let built = build(&directory.0)?;
    progress("Finishing up…");
    let hosts_with_managed = resolve_host_tailcat_binary(paths).as_deref() == Some(&*destination);
    let before = published_addresses(runtime);
    let staged = stage_beside(&built, &parent)?;
    let previous = parent.join(".tailcat-previous");
    let had_previous = destination.is_file();
    if had_previous {
        let _ = fs::remove_file(&previous);
        if let Err(error) = fs::rename(&destination, &previous) {
            let _ = fs::remove_file(&staged);
            return Err(save_failure(&error));
        }
    }
    if let Err(error) = fs::rename(&staged, &destination) {
        let _ = fs::remove_file(&staged);
        if had_previous {
            let _ = fs::rename(&previous, &destination);
        }
        return Err(save_failure(&error));
    }
    runtime.forget_binary_version();
    let serving = before.0.is_some() || before.1.is_some();
    if !hosts_with_managed || !serving {
        // Nothing runs on this file now: the next start uses it.
        let _ = fs::remove_file(&previous);
        return Ok(());
    }
    progress("Starting Easy Connect again on the new helper…");
    runtime.restart_helpers(paths, "helperUpdated");
    let after = wait_for_addresses(runtime, &before);
    if after.as_ref() == Some(&before) {
        let _ = fs::remove_file(&previous);
        return Ok(());
    }
    // Put the helper that served these addresses back, and serve them again.
    let plain = if after.is_none() {
        "The new Easy Connect helper didn't start, so Ghostex went back to the one you had. Your phones and computers connect as before."
    } else {
        "This Easy Connect helper version would give this computer a new address and cut off your paired phones and computers, so Ghostex kept the one you had."
    };
    if had_previous {
        let _ = fs::rename(&previous, &destination);
        runtime.forget_binary_version();
        runtime.restart_helpers(paths, "helperUpdateRolledBack");
    }
    Err(Failure::new(
        plain,
        format!("addresses before {before:?}, after {after:?}"),
    ))
}

/// The addresses the helpers publish once every lane that served before serves again; `None`
/// when they didn't within [`UPDATE_START_WAIT`].
fn wait_for_addresses(
    runtime: &TailcatRuntime,
    before: &(Option<String>, Option<String>),
) -> Option<(Option<String>, Option<String>)> {
    let deadline = Instant::now() + UPDATE_START_WAIT;
    loop {
        let now = published_addresses(runtime);
        let ready =
            (before.0.is_none() || now.0.is_some()) && (before.1.is_none() || now.1.is_some());
        if ready {
            return Some(now);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[derive(Deserialize)]
struct GoRelease {
    stable: bool,
    files: Vec<GoArchive>,
}

#[derive(Deserialize)]
struct GoArchive {
    filename: String,
    os: String,
    arch: String,
    kind: String,
    sha256: String,
    #[serde(default)]
    size: u64,
}

fn go_os() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else {
        std::env::consts::OS
    }
}

fn go_arch() -> Step<&'static str> {
    match std::env::consts::ARCH {
        "aarch64" => Ok("arm64"),
        "x86_64" => Ok("amd64"),
        other => Err(Failure::new(
            format!("Easy Connect isn't available for this computer's processor ({other}). Use Tailscale or your own network to connect your phone."),
            other,
        )),
    }
}

fn download_toolchain(directory: &Path) -> Step<PathBuf> {
    progress("Downloading the build tools…");
    let agent: ureq::Agent = crate::http_agent::config()
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into();
    let catalog = agent
        .get("https://go.dev/dl/?mode=json")
        .call()
        .map_err(|error| {
            Failure::new(NO_NETWORK, format!("read the Go download catalog: {error}"))
        })?;
    let releases: Vec<GoRelease> = crate::http_agent::read_json(catalog).map_err(|error| {
        Failure::new(
            NO_NETWORK,
            format!("read the Go download catalog: {error:#}"),
        )
    })?;
    let arch = go_arch()?;
    let archive = releases
        .into_iter()
        .filter(|release| release.stable)
        .flat_map(|release| release.files)
        .find(|file| file.os == go_os() && file.arch == arch && file.kind == "archive")
        .ok_or_else(|| Failure::new(BUILD_FAILED, "no supported Go toolchain in the catalog"))?;
    if Path::new(&archive.filename)
        .file_name()
        .and_then(|name| name.to_str())
        != Some(&archive.filename)
    {
        return Err(Failure::new(BUILD_FAILED, "invalid Go archive filename"));
    }
    let archive_path = directory.join(&archive.filename);
    let response = agent
        .get(&format!("https://go.dev/dl/{}", archive.filename))
        .call()
        .map_err(|error| {
            Failure::new(NO_NETWORK, format!("download the Go build tools: {error}"))
        })?;
    let mut file = File::create(&archive_path).map_err(|error| save_failure(&error))?;
    let mut reader = response.into_body().into_reader().take(200 * 1024 * 1024);
    let mut buffer = vec![0u8; 256 * 1024];
    let mut done: u64 = 0;
    let mut shown = None;
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            Failure::new(NO_NETWORK, format!("download the Go build tools: {error}"))
        })?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| save_failure(&error))?;
        done += read as u64;
        if let Some(percent) = (done * 100).checked_div(archive.size) {
            let percent = percent.min(100);
            if shown != Some(percent) {
                shown = Some(percent);
                progress(&format!(
                    "Downloading the build tools… {percent}% of {} MB",
                    archive.size.div_ceil(1024 * 1024)
                ));
            }
        }
    }
    drop(file);
    let digest = crate::managed_tools::download::sha256_file(&archive_path)
        .map_err(|error| Failure::new(BUILD_FAILED, error))?;
    if digest != archive.sha256 {
        return Err(Failure::new(
            "The build tools download arrived damaged. Try again.",
            "checksum mismatch",
        ));
    }
    progress("Unpacking the build tools…");
    if cfg!(windows) {
        File::open(&archive_path)
            .map_err(|error| error.to_string())
            .and_then(|archive| zip::ZipArchive::new(archive).map_err(|error| error.to_string()))
            .and_then(|mut archive| {
                archive
                    .extract(directory)
                    .map_err(|error| error.to_string())
            })
            .map_err(|error| Failure::new(BUILD_FAILED, format!("unpack Go: {error}")))?;
    } else {
        run_command(
            Command::new("/usr/bin/tar")
                .arg("-xzf")
                .arg(&archive_path)
                .arg("-C")
                .arg(directory),
            directory,
            Duration::from_secs(120),
        )?;
    }
    Ok(directory
        .join("go/bin")
        .join(if cfg!(windows) { "go.exe" } else { "go" }))
}

fn run_command(command: &mut Command, directory: &Path, timeout: Duration) -> Step<()> {
    let output_path = directory.join("build-output.txt");
    let output = File::create(&output_path).map_err(|error| save_failure(&error))?;
    let output_err = output.try_clone().map_err(|error| save_failure(&error))?;
    super::supervisor::configure_process_group(command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(output_err)
        .stderr(output)
        .spawn()
        .map_err(|error| Failure::new(BUILD_FAILED, format!("start the build: {error}")))?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => (),
            Err(error) => {
                super::supervisor::terminate_process_group(&mut child);
                return Err(Failure::new(
                    BUILD_FAILED,
                    format!("wait for the build: {error}"),
                ));
            }
        }
        if Instant::now() >= deadline {
            super::supervisor::terminate_process_group(&mut child);
            return Err(Failure::new(
                "Building the Easy Connect helper took too long. Check that this computer is online, then try again.",
                "build timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if !status.success() {
        let detail = read_tail(&output_path).unwrap_or_default();
        let plain = if looks_offline(&detail) {
            NO_NETWORK
        } else {
            BUILD_FAILED
        };
        return Err(Failure::new(
            plain,
            format!("exited with {status}: {detail}"),
        ));
    }
    Ok(())
}

/// The last 4000 bytes of the build's output.
fn read_tail(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let start = file.metadata()?.len().saturating_sub(4000);
    file.seek(SeekFrom::Start(start))?;
    let mut detail = Vec::new();
    file.read_to_end(&mut detail)?;
    Ok(String::from_utf8_lossy(&detail).trim().to_string())
}
