//! mise, for an agent CLI that installs only through it (DeepSeek Harness) on a computer that has no
//! mise yet.
//!
//! CDXC:ManagedTools 2026-10-28 DECISION:
//! User: "please fix this for dsh-tui and all the others", after someone trying DeepSeek Harness "had to install something separately on my MacBook (mise for the CLI)". Install from Settings must work on a computer with nothing set up, so a mise install whose mise is missing first downloads mise from its official GitHub releases into Ghostex's tools folder, checked against the published SHA-256 sums, the way Ghostex installs Beads and the GitHub CLI. Nothing is added to the user's shell profile: gxserver finds its own tools folder, and a session runs a mise-installed agent through that mise (agent_cli/launch_path.rs).
//! SEE-ALSO: server/src/agent_cli/catalog.rs (the `mise` prerequisite), server/src/agent_cli/endpoint.rs (`prepare_prerequisite`).

use std::path::PathBuf;

use super::{
    download::{self, Scratch},
    jobs::Log,
    paths,
    platform::{self, Arch, Os},
};

const LATEST: &str = "https://github.com/jdx/mise/releases/latest";

fn downloads(tag: &str) -> String {
    format!("https://github.com/jdx/mise/releases/download/{tag}")
}

/// The release archive for this computer. Linux takes the static musl build, which runs on any glibc.
pub(crate) fn asset(tag: &str, os: Os, arch: Arch) -> String {
    let arch = match arch {
        Arch::X64 => "x64",
        Arch::Arm64 => "arm64",
    };
    match os {
        Os::Mac => format!("mise-{tag}-macos-{arch}.tar.gz"),
        Os::Linux => format!("mise-{tag}-linux-{arch}-musl.tar.gz"),
        Os::Windows => format!("mise-{tag}-windows-{arch}.zip"),
    }
}

/// The mise in Ghostex's tools folder, when Ghostex installed one.
pub(crate) fn managed_executable() -> Option<PathBuf> {
    Some(paths::bin_dir().join(paths::executable_name("mise"))).filter(|path| path.is_file())
}

/// Downloads the newest mise release into Ghostex's tools folder.
pub(crate) fn install(log: &Log) -> Result<PathBuf, String> {
    let arch = platform::arch().ok_or_else(platform::unsupported_cpu_reason)?;
    let tag = download::latest_release_tag(LATEST)?;
    let asset = asset(&tag, platform::os(), arch);
    let base = downloads(&tag);
    log.line(&format!(
        "Installing mise {tag} from its official GitHub releases."
    ));
    let scratch = Scratch::new()?;
    let archive = scratch.0.join(&asset);
    let sums = download::get_text(&format!("{base}/SHASUMS256.txt"))?;
    let expected = download::checksum_for(&sums, &asset)
        .ok_or_else(|| format!("mise published no checksum for {asset}."))?;
    download::download(&format!("{base}/{asset}"), &archive, "mise", log)?;
    download::verify(&archive, &expected, "mise", log)?;
    let unpacked = scratch.0.join("unpacked");
    download::extract(&archive, &unpacked)?;
    let file = paths::executable_name("mise");
    let found = download::find_file(&unpacked, &file)
        .ok_or_else(|| format!("The mise archive did not contain {file}."))?;
    let staged = scratch.0.join(&file);
    std::fs::copy(&found, &staged).map_err(|error| error.to_string())?;
    download::make_executable(&staged)?;
    let target = paths::bin_dir().join(&file);
    download::replace_path(&staged, &target)?;
    let installed = super::tools::version_of(&target, &["--version"])
        .ok_or_else(|| "mise was installed but did not start.".to_string())?;
    log.line(&format!(
        "mise {installed} is installed in {}.",
        paths::bin_dir().display()
    ));
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_platform_downloads_its_own_release_archive() {
        assert_eq!(
            asset("v2026.10.21", Os::Mac, Arch::Arm64),
            "mise-v2026.10.21-macos-arm64.tar.gz"
        );
        assert_eq!(
            asset("v2026.10.21", Os::Linux, Arch::X64),
            "mise-v2026.10.21-linux-x64-musl.tar.gz"
        );
        assert_eq!(
            asset("v2026.10.21", Os::Windows, Arch::X64),
            "mise-v2026.10.21-windows-x64.zip"
        );
        let sums = "abc  ./mise-v2026.10.21-linux-x64.tar.gz\n0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  ./mise-v2026.10.21-macos-arm64.tar.gz\n";
        assert_eq!(
            download::checksum_for(sums, "mise-v2026.10.21-macos-arm64.tar.gz").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
    }
}
