//! The Executor release Ghostex downloads the first time a workspace turns Executor on: one pinned
//! build per platform, checked against its published SHA-256 and unpacked whole into Ghostex's
//! tools folder, where nothing inside it is ever changed.

use std::path::{Path, PathBuf};

use crate::managed_tools::download::{self, Scratch};
use crate::managed_tools::jobs::Log;
use crate::managed_tools::paths as tool_paths;
use crate::managed_tools::platform::{self, Arch, Os};

/// The Executor build Ghostex runs.
///
/// CDXC:Connectors 2026-10-15 WHY:
/// Executor 2 ships a beta almost every night, some with breaking API changes, and a newer release upgrades a data folder so older ones can no longer open it. Ghostex runs one tested beta, checked against the hashes recorded here (GitHub's asset digests, matching the release's SHA256SUMS), and moves to a newer one only after testing it.
pub(crate) const VERSION: &str = "2.0.0-beta.22";
const RELEASE_URL: &str =
    "https://github.com/UsefulSoftwareCo/executor/releases/download/executor%402.0.0-beta.22";

struct Asset {
    name: &'static str,
    sha256: &'static str,
}

fn asset() -> Result<Asset, String> {
    let arch = platform::arch().ok_or_else(platform::unsupported_cpu_reason)?;
    let (name, sha256) = match (platform::os(), arch) {
        (Os::Mac, Arch::Arm64) => (
            "executor-2.0.0-beta.22-darwin-arm64.tgz",
            "1297b8cc243d6218793197a7385fc074f3669a85aebd05db24da297be34b4886",
        ),
        (Os::Mac, Arch::X64) => (
            "executor-2.0.0-beta.22-darwin-x64.tgz",
            "b400822d2f7f1a263ff7ff78d39be10cbe1442b8abb7a8314ad90119a5aa23c4",
        ),
        (Os::Linux, Arch::Arm64) => (
            "executor-2.0.0-beta.22-linux-arm64.tgz",
            "b739bb14f633ba9804bcf9cc0c9a796736ae48558d314565bc548b171b4e5119",
        ),
        (Os::Linux, Arch::X64) => (
            "executor-2.0.0-beta.22-linux-x64.tgz",
            "f240180da58f9f90bba8bfadf97afe9f56e28e1dc8230cb4cff84213b700a715",
        ),
        (Os::Windows, Arch::X64) => (
            "executor-2.0.0-beta.22-win32-x64.tgz",
            "849890026dc2b468ca042d1b1845ee8e42dee9c94d7ad493e0cb4ea63b1e244c",
        ),
        (Os::Windows, Arch::Arm64) => {
            return Err("Executor has no build for Windows on Arm.".to_string())
        }
    };
    if platform::os() == Os::Linux && platform::glibc_version().is_none() {
        return Err(
            "Executor needs a Linux with the GNU C library (it has no build for Alpine or other musl systems)."
                .to_string(),
        );
    }
    Ok(Asset { name, sha256 })
}

/// `<tools>/executor`, one folder per version.
fn versions_dir() -> PathBuf {
    tool_paths::tools_root().join("executor")
}

/// Where this version is unpacked: the release's `package/` folder.
pub(super) fn install_dir() -> PathBuf {
    versions_dir().join(VERSION)
}

pub(super) fn executable() -> PathBuf {
    install_dir().join(tool_paths::executable_name("executor"))
}

pub(crate) fn installed() -> bool {
    executable().is_file()
}

/// The version an Executor program reports (`executor v2.0.0-beta.22`).
fn version_of(executable: &Path) -> Option<String> {
    let mut command = crate::platform::process::background_command(executable);
    command
        .arg("--version")
        .env("DO_NOT_TRACK", "1")
        .env("EXECUTOR_NO_UPDATE_CHECK", "1");
    let output =
        crate::managed_tools::run::output_within(command, crate::managed_tools::run::PROBE_TIMEOUT)
            .ok()?;
    output
        .stdout
        .trim()
        .strip_prefix("executor v")
        .map(str::to_string)
}

/// Downloads, checks and unpacks the pinned build, then removes other versions.
pub(super) fn install(log: &Log) -> Result<(), String> {
    let asset = asset()?;
    log.line(&format!(
        "Installing Executor {VERSION} from its GitHub release."
    ));
    let scratch = Scratch::new()?;
    let archive = scratch.0.join(asset.name);
    download::download(
        &format!("{RELEASE_URL}/{}", asset.name),
        &archive,
        "Executor",
        log,
    )?;
    download::verify(&archive, asset.sha256, "Executor", log)?;
    log.line("Unpacking…");
    let unpacked = scratch.0.join("unpacked");
    download::extract(&archive, &unpacked)?;
    let package = unpacked.join("package");
    if !package
        .join(tool_paths::executable_name("executor"))
        .is_file()
    {
        return Err("The Executor download did not contain its program.".to_string());
    }
    download::replace_path(&package, &install_dir())?;
    match version_of(&executable()) {
        Some(version) if version == VERSION => {}
        other => {
            return Err(format!(
                "Executor was unpacked but reported version {} instead of {VERSION}.",
                other.unwrap_or_else(|| "nothing".to_string())
            ))
        }
    }
    remove_other_versions();
    log.line(&format!(
        "Executor {VERSION} is installed in {}.",
        install_dir().display()
    ));
    Ok(())
}

fn remove_other_versions() {
    let Ok(entries) = std::fs::read_dir(versions_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy() != VERSION && entry.path().is_dir() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
