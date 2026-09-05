use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::paths::GxserverPaths;

use super::{
    binary::managed_tailcat_binary, read_tailcat_binary_version, resolve_tailcat_binary,
    start_tailcat_from_persisted_state, TailcatRuntime,
};

const TAILCAT_MODULE: &str = "github.com/tailscale/tailcat/cmd/tailcat@v0.6.0";
static INSTALLATION: Mutex<InstallationStatus> = Mutex::new(InstallationStatus {
    running: false,
    progress: None,
    error: None,
});

#[derive(Clone)]
pub(crate) struct InstallationStatus {
    pub running: bool,
    pub progress: Option<String>,
    pub error: Option<String>,
}

pub(crate) fn installation_status() -> InstallationStatus {
    INSTALLATION.lock().unwrap().clone()
}

/// CDXC:RemotePairing 2026-09-05 DECISION:
/// User: install the Tailcat CLI with one click instead of copying a command into a terminal.
/// The job belongs to the daemon so closing Settings does not cancel it; status polling reports progress and failures across reopened pages.
pub(crate) fn start_installation(paths: GxserverPaths, runtime: TailcatRuntime) {
    let mut status = INSTALLATION.lock().unwrap();
    if status.running {
        return;
    }
    *status = InstallationStatus {
        running: true,
        progress: Some("Preparing Easy Connect…".into()),
        error: None,
    };
    drop(status);
    tokio::spawn(async move {
        let outcome = tokio::task::spawn_blocking(move || {
            install(&paths)?;
            start_tailcat_from_persisted_state(&paths, &runtime);
            Ok::<_, anyhow::Error>(())
        })
        .await;
        let error = match outcome {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(format!("Easy Connect installation failed: {error:#}")),
            Err(error) => Some(format!("Easy Connect installation stopped: {error}")),
        };
        *INSTALLATION.lock().unwrap() = InstallationStatus {
            running: false,
            progress: None,
            error,
        };
    });
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

fn install(paths: &GxserverPaths) -> Result<()> {
    if resolve_tailcat_binary().is_some() {
        return Ok(());
    }
    let directory = InstallDirectory(
        paths
            .app_cache_dir
            .join("tailcat-install")
            .join(uuid::Uuid::new_v4().to_string()),
    );
    fs::create_dir_all(&directory.0).context("create the Easy Connect build directory")?;
    let go = download_toolchain(&directory.0)?;
    progress("Building the Easy Connect helper. This can take a few minutes…");
    let bin_dir = directory.0.join("bin");
    fs::create_dir_all(&bin_dir)?;
    // CDXC:RemotePairing 2026-09-05 WHY: A private Go toolchain makes the install work without Homebrew, developer tools, or changes to the user's Go environment, including on macOS where upstream does not ship a binary archive.
    run_command(
        Command::new(go)
            .args(["install", TAILCAT_MODULE])
            .current_dir(&directory.0)
            .env("GOENV", "off")
            .env("GOWORK", "off")
            .env("GOFLAGS", "-modcacherw")
            .env("GOMAXPROCS", "2")
            .env("GOOS", go_os())
            .env("GOARCH", go_arch()?)
            .env("GOROOT", directory.0.join("go"))
            .env("GOTOOLCHAIN", "local")
            .env("GOPATH", directory.0.join("gopath"))
            .env("GOCACHE", directory.0.join("go-cache"))
            .env("GOBIN", &bin_dir)
            .env("CGO_ENABLED", "0"),
        &directory.0,
        Duration::from_secs(15 * 60),
    )?;
    progress("Finishing Easy Connect installation…");
    let destination = managed_tailcat_binary();
    let built = bin_dir.join(destination.file_name().context("missing helper filename")?);
    if read_tailcat_binary_version(&built).is_none() {
        bail!("the downloaded helper did not answer its version check");
    }
    let parent = destination.parent().context("missing helper directory")?;
    fs::create_dir_all(parent)?;
    let staged = parent.join(format!(".tailcat-{}", uuid::Uuid::new_v4()));
    let result = fs::copy(&built, &staged).and_then(|_| fs::rename(&staged, &destination));
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result.context("save the Easy Connect helper")?;
    Ok(())
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
}

fn go_os() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else {
        std::env::consts::OS
    }
}

fn go_arch() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "aarch64" => Ok("arm64"),
        "x86_64" => Ok("amd64"),
        other => bail!("Easy Connect installation is not supported on {other}"),
    }
}

fn download_toolchain(directory: &Path) -> Result<PathBuf> {
    progress("Downloading the tools needed to install Easy Connect…");
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(180))
        .build();
    let releases: Vec<GoRelease> = agent
        .get("https://go.dev/dl/?mode=json")
        .call()
        .context("read the Go download catalog")?
        .into_json()?;
    let arch = go_arch()?;
    let archive = releases
        .into_iter()
        .filter(|release| release.stable)
        .flat_map(|release| release.files)
        .find(|file| file.os == go_os() && file.arch == arch && file.kind == "archive")
        .context("no supported Go toolchain was found for this computer")?;
    if Path::new(&archive.filename)
        .file_name()
        .and_then(|name| name.to_str())
        != Some(&archive.filename)
    {
        bail!("invalid Go archive filename");
    }
    let archive_path = directory.join(&archive.filename);
    let response = agent
        .get(&format!("https://go.dev/dl/{}", archive.filename))
        .call()
        .context("download the Go build tools")?;
    let mut file = File::create(&archive_path)?;
    std::io::copy(
        &mut response.into_reader().take(200 * 1024 * 1024),
        &mut file,
    )?;
    drop(file);
    let mut file = File::open(&archive_path)?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    if format!("{:x}", digest.finalize()) != archive.sha256 {
        bail!("the Go build tools failed checksum verification; try installing again");
    }
    progress("Preparing the Easy Connect build tools…");
    if cfg!(windows) {
        zip::ZipArchive::new(File::open(&archive_path)?)?.extract(directory)?;
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

fn run_command(command: &mut Command, directory: &Path, timeout: Duration) -> Result<()> {
    let output_path = directory.join("build-output.txt");
    let output = File::create(&output_path)?;
    super::supervisor::configure_process_group(command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0200 | 0x0800_0000);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        .spawn()
        .context("start the Easy Connect installer")?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => (),
            Err(error) => {
                super::supervisor::terminate_process_group(&mut child);
                return Err(error).context("wait for the Easy Connect installer");
            }
        }
        if Instant::now() >= deadline {
            super::supervisor::terminate_process_group(&mut child);
            bail!("installation timed out; check your internet connection and try again");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if !status.success() {
        let mut file = File::open(output_path)?;
        let start = file.metadata()?.len().saturating_sub(4000);
        file.seek(SeekFrom::Start(start))?;
        let mut detail = Vec::new();
        file.read_to_end(&mut detail)?;
        bail!(
            "installer exited with {status}: {}",
            String::from_utf8_lossy(&detail).trim()
        );
    }
    Ok(())
}
