//! uv, from Astral's GitHub releases. It installs Claude Swap and downloads the Python it needs.

use std::path::{Path, PathBuf};

use super::{
    download::{self, Scratch},
    jobs::Log,
    paths,
    platform::{self, Arch, Os},
    tools::{self, Source, Status, ToolId},
};

const LATEST: &str = "https://github.com/astral-sh/uv/releases/latest";

fn target() -> Result<(&'static str, &'static str), String> {
    let arch = match platform::arch() {
        Some(Arch::X64) => "x86_64",
        Some(Arch::Arm64) => "aarch64",
        None => return Err(platform::unsupported_cpu_reason()),
    };
    // The musl build is fully static, so it runs on every Linux whatever its C library.
    Ok(match (platform::os(), arch) {
        (Os::Mac, "x86_64") => ("x86_64-apple-darwin", "tar.gz"),
        (Os::Mac, _) => ("aarch64-apple-darwin", "tar.gz"),
        (Os::Linux, "x86_64") => ("x86_64-unknown-linux-musl", "tar.gz"),
        (Os::Linux, _) => ("aarch64-unknown-linux-musl", "tar.gz"),
        (Os::Windows, "x86_64") => ("x86_64-pc-windows-msvc", "zip"),
        (Os::Windows, _) => ("aarch64-pc-windows-msvc", "zip"),
    })
}

pub(crate) fn latest_version() -> Result<String, String> {
    download::latest_release_tag(LATEST).map(|tag| tag.trim_start_matches('v').to_string())
}

pub(crate) fn status(home: &Path) -> Status {
    if let Err(reason) = target() {
        return Status::unsupported(reason);
    }
    let mut status = Status::new(
        "Downloads uv from Astral's GitHub releases, checks it against the published checksum and puts it in Ghostex's tools folder. No password needed.".into(),
    );
    match tools::locate("uv", home) {
        Some((path, source)) => {
            status.read_version(&path, &["--version"]);
            status.executable = Some(path);
            status.source = Some(source);
            status.operations = match source {
                Source::Ghostex => vec!["update", "reinstall", "uninstall"],
                Source::System => Vec::new(),
            };
        }
        None => status.operations = vec!["install"],
    }
    status
}

/// Ghostex's own uv, when installed.
pub(crate) fn managed_executable() -> Option<PathBuf> {
    let path = paths::uv_dir().join(paths::executable_name("uv"));
    path.is_file().then_some(path)
}

pub(crate) fn install(log: &Log) -> Result<(), String> {
    let (target, extension) = target()?;
    let tag = download::latest_release_tag(LATEST)?;
    let asset = format!("uv-{target}.{extension}");
    let base = format!("https://github.com/astral-sh/uv/releases/download/{tag}");
    log.line(&format!(
        "Installing uv {} from Astral's GitHub releases.",
        tag.trim_start_matches('v')
    ));
    let scratch = Scratch::new()?;
    let archive = scratch.0.join(&asset);
    let sums = download::get_text(&format!("{base}/{asset}.sha256"))?;
    let expected = download::checksum_for(&sums, &asset)
        .ok_or_else(|| format!("Astral published no checksum for {asset}."))?;
    download::download(&format!("{base}/{asset}"), &archive, "uv", log)?;
    download::verify(&archive, &expected, "uv", log)?;
    let unpacked = scratch.0.join("unpacked");
    download::extract(&archive, &unpacked)?;
    let staged = scratch.0.join("uv");
    std::fs::create_dir_all(&staged).map_err(|error| error.to_string())?;
    for name in ["uv", "uvx", "uvw"] {
        let file = paths::executable_name(name);
        if let Some(found) = download::find_file(&unpacked, &file) {
            std::fs::copy(&found, staged.join(&file)).map_err(|error| error.to_string())?;
            download::make_executable(&staged.join(&file))?;
        } else if name == "uv" {
            return Err("The uv archive did not contain uv.".into());
        }
    }
    download::replace_path(&staged, &paths::uv_dir())?;
    let installed = managed_executable()
        .and_then(|uv| tools::version_of(&uv, &["--version"]))
        .ok_or("uv was unpacked but did not start. Try reinstalling it.")?;
    tools::forget_latest(ToolId::Uv);
    log.line(&format!(
        "uv {installed} is installed in {}.",
        paths::uv_dir().display()
    ));
    Ok(())
}

pub(crate) fn uninstall(log: &Log) -> Result<(), String> {
    let dir = paths::uv_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|error| format!("Could not remove {}: {error}", dir.display()))?;
    }
    log.line("Removed Ghostex's uv. Tools it installed, such as Claude Swap, keep working.");
    Ok(())
}
