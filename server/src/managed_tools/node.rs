//! Node.js (with npm), from nodejs.org's official builds.

use std::path::Path;

use super::{
    download::{self, Scratch},
    jobs::Log,
    paths,
    platform::{self, Arch, Os},
    tools::{self, Source, Status, ToolId},
};

/// Node 24 (the current LTS line) needs glibc 2.28 on Linux.
const GLIBC_MINIMUM: (u32, u32) = (2, 28);

/// The newest LTS release and its archive name for this computer.
pub(crate) fn latest_lts() -> Result<(String, String), String> {
    let index = download::get_json("https://nodejs.org/dist/index.json")?;
    let release = index
        .as_array()
        .and_then(|releases| {
            releases
                .iter()
                .find(|release| release["lts"].as_str().is_some_and(|name| !name.is_empty()))
        })
        .ok_or("nodejs.org did not list an LTS release.")?;
    let version = release["version"]
        .as_str()
        .ok_or("nodejs.org listed an LTS release without a version.")?
        .trim_start_matches('v')
        .to_string();
    let archive = archive_name(&version)?;
    Ok((version, archive))
}

fn archive_name(version: &str) -> Result<String, String> {
    let arch = match platform::arch() {
        Some(Arch::X64) => "x64",
        Some(Arch::Arm64) => "arm64",
        None => return Err(platform::unsupported_cpu_reason()),
    };
    Ok(match platform::os() {
        Os::Mac => format!("node-v{version}-darwin-{arch}.tar.gz"),
        Os::Linux => format!("node-v{version}-linux-{arch}.tar.gz"),
        Os::Windows => format!("node-v{version}-win-{arch}.zip"),
    })
}

pub(crate) fn supported() -> Result<(), String> {
    if platform::arch().is_none() {
        return Err(platform::unsupported_cpu_reason());
    }
    match platform::glibc_reason("Node.js", GLIBC_MINIMUM, "Ubuntu 20.04, Debian 10 or newer") {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

pub(crate) fn status(home: &Path) -> Status {
    if let Err(reason) = supported() {
        return Status::unsupported(reason);
    }
    let mut status = Status::new(
        "Downloads the current Node.js LTS from nodejs.org, checks it against the published checksum, puts it in Ghostex's tools folder and adds it to the end of your PATH. No password needed.".into(),
    );
    if let Some((npm, source)) = tools::locate("npm", home) {
        let node = npm.with_file_name(paths::executable_name("node"));
        status.read_version(if node.is_file() { &node } else { &npm }, &["--version"]);
        status.executable = Some(npm);
        status.source = Some(source);
        status.operations = match source {
            Source::Ghostex => vec!["update", "reinstall", "uninstall"],
            Source::System => Vec::new(),
        };
    } else {
        status.operations = vec!["install"];
    }
    status
}

/// Installs (or replaces) Ghostex's own Node.js.
pub(crate) fn install(log: &Log) -> Result<(), String> {
    supported()?;
    let (version, archive) = latest_lts()?;
    log.line(&format!(
        "Installing Node.js {version} (LTS) from nodejs.org."
    ));
    let scratch = Scratch::new()?;
    let archive_path = scratch.0.join(&archive);
    let base = format!("https://nodejs.org/dist/v{version}");
    let sums = download::get_text(&format!("{base}/SHASUMS256.txt"))?;
    let expected = download::checksum_for(&sums, &archive)
        .ok_or_else(|| format!("nodejs.org published no checksum for {archive}."))?;
    download::download(
        &format!("{base}/{archive}"),
        &archive_path,
        &format!("Node.js {version}"),
        log,
    )?;
    download::verify(&archive_path, &expected, "Node.js", log)?;
    log.line("Unpacking Node.js…");
    let unpacked = scratch.0.join("unpacked");
    download::extract(&archive_path, &unpacked)?;
    let top = std::fs::read_dir(&unpacked)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .ok_or("The Node.js archive was empty.")?;
    download::replace_path(&top, &paths::node_dir())?;
    let node = paths::node_bin_dir().join(paths::executable_name("node"));
    let installed = tools::version_of(&node, &["--version"])
        .ok_or("Node.js was unpacked but did not start. Try reinstalling it.")?;
    tools::forget_latest(ToolId::Node);
    log.line(&format!(
        "Node.js {installed} is installed in {}.",
        paths::node_dir().display()
    ));
    Ok(())
}

pub(crate) fn uninstall(log: &Log) -> Result<(), String> {
    let dir = paths::node_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|error| format!("Could not remove {}: {error}", dir.display()))?;
    }
    log.line("Removed Ghostex's Node.js. Agent CLIs installed with it stop working until Node.js is installed again.");
    Ok(())
}
