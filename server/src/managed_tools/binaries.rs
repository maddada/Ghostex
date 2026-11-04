//! Single-binary command-line tools downloaded from their official releases: Beads (`bd`), the
//! GitHub CLI (`gh`) and the GitLab CLI (`glab`).

use std::path::Path;

use super::{
    download::{self, Scratch},
    jobs::Log,
    paths,
    platform::{self, Arch, Os},
    tools::{self, Source, Status, ToolId},
};

struct Spec {
    tool: ToolId,
    binary: &'static str,
    /// The `releases/latest` page, which redirects to the newest tag.
    latest: &'static str,
    /// Where one release's files download from.
    downloads: fn(&str) -> String,
    asset: fn(&str, Os, Arch) -> String,
    checksums: fn(&str) -> String,
    source_name: &'static str,
}

fn spec(tool: ToolId) -> Spec {
    match tool {
        ToolId::Beads => Spec {
            tool,
            binary: "bd",
            latest: "https://github.com/gastownhall/beads/releases/latest",
            downloads: |tag| {
                format!("https://github.com/gastownhall/beads/releases/download/{tag}")
            },
            asset: |version, os, arch| {
                let (os, extension) = match os {
                    Os::Mac => ("darwin", "tar.gz"),
                    Os::Linux => ("linux", "tar.gz"),
                    Os::Windows => ("windows", "zip"),
                };
                format!("beads_{version}_{os}_{}.{extension}", go_arch(arch))
            },
            checksums: |_| "checksums.txt".into(),
            source_name: "the Beads GitHub releases",
        },
        ToolId::Gh => Spec {
            tool,
            binary: "gh",
            latest: "https://github.com/cli/cli/releases/latest",
            downloads: |tag| format!("https://github.com/cli/cli/releases/download/{tag}"),
            asset: |version, os, arch| match os {
                Os::Mac => format!("gh_{version}_macOS_{}.zip", go_arch(arch)),
                Os::Linux => format!("gh_{version}_linux_{}.tar.gz", go_arch(arch)),
                Os::Windows => format!("gh_{version}_windows_{}.zip", go_arch(arch)),
            },
            checksums: |version| format!("gh_{version}_checksums.txt"),
            source_name: "GitHub's official releases",
        },
        ToolId::Glab => Spec {
            tool,
            binary: "glab",
            latest: "https://gitlab.com/gitlab-org/cli/-/releases/permalink/latest",
            downloads: |tag| {
                format!("https://gitlab.com/gitlab-org/cli/-/releases/{tag}/downloads")
            },
            asset: |version, os, arch| {
                let (os, extension) = match os {
                    Os::Mac => ("darwin", "tar.gz"),
                    Os::Linux => ("linux", "tar.gz"),
                    Os::Windows => ("windows", "zip"),
                };
                format!("glab_{version}_{os}_{}.{extension}", go_arch(arch))
            },
            checksums: |_| "checksums.txt".into(),
            source_name: "GitLab's official releases",
        },
        _ => unreachable!("not a downloaded binary"),
    }
}

fn go_arch(arch: Arch) -> &'static str {
    match arch {
        Arch::X64 => "amd64",
        Arch::Arm64 => "arm64",
    }
}

/// Beads links against glibc 2.34 on Linux.
fn supported(tool: ToolId) -> Result<(), String> {
    if platform::arch().is_none() {
        return Err(platform::unsupported_cpu_reason());
    }
    if tool == ToolId::Beads {
        if let Some(reason) =
            platform::glibc_reason("Beads", (2, 34), "Ubuntu 22.04, Debian 12 or newer")
        {
            return Err(reason);
        }
    }
    Ok(())
}

pub(crate) fn latest_version(tool: ToolId) -> Result<String, String> {
    download::latest_release_tag(spec(tool).latest)
        .map(|tag| tag.trim_start_matches('v').to_string())
}

pub(crate) fn status(tool: ToolId, home: &Path) -> Status {
    let spec = spec(tool);
    if let Err(reason) = supported(tool) {
        // A copy the user installed some other way still counts.
        if let Some((path, source)) = tools::locate(spec.binary, home) {
            let mut status = Status::new(String::new());
            status.read_version(&path, &["--version"]);
            status.executable = Some(path);
            status.source = Some(source);
            status.install_blocker = Some(reason);
            return status;
        }
        return Status::unsupported(reason);
    }
    let mut status = Status::new(format!(
        "Downloads {} from {}, checks it against the published checksum and puts it in Ghostex's tools folder. Then it links {} into ~/.local/bin when that folder is on your PATH, or adds Ghostex's tools folder to the end of your PATH. No password needed.",
        tool.label(),
        spec.source_name,
        spec.binary
    ));
    match tools::locate(spec.binary, home) {
        Some((path, Source::Ghostex)) if !tools::reachable(spec.binary, home) => {
            status.read_version(&path, &["--version"]);
            status.detail = Some(format!(
                "Ghostex installed {} in {}, but new terminals and agents can't find it. Install again to put it on your PATH.",
                spec.binary,
                paths::bin_dir().display()
            ));
            status.executable = Some(path);
            status.source = Some(Source::Ghostex);
            status.off_path = true;
            status.operations = vec!["install", "uninstall"];
        }
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

pub(crate) fn install(tool: ToolId, log: &Log) -> Result<(), String> {
    supported(tool)?;
    let spec = spec(tool);
    let arch = platform::arch().ok_or_else(platform::unsupported_cpu_reason)?;
    let tag = download::latest_release_tag(spec.latest)?;
    let version = tag.trim_start_matches('v').to_string();
    let asset = (spec.asset)(&version, platform::os(), arch);
    let base = (spec.downloads)(&tag);
    log.line(&format!(
        "Installing {} {version} from {}.",
        tool.label(),
        spec.source_name
    ));
    let scratch = Scratch::new()?;
    let archive = scratch.0.join(&asset);
    let sums = download::get_text(&format!("{base}/{}", (spec.checksums)(&version)))?;
    let expected = download::checksum_for(&sums, &asset)
        .ok_or_else(|| format!("{} published no checksum for {asset}.", tool.label()))?;
    download::download(&format!("{base}/{asset}"), &archive, tool.label(), log)?;
    download::verify(&archive, &expected, tool.label(), log)?;
    let unpacked = scratch.0.join("unpacked");
    download::extract(&archive, &unpacked)?;
    let file = paths::executable_name(spec.binary);
    let found = download::find_file(&unpacked, &file)
        .ok_or_else(|| format!("The {} archive did not contain {file}.", tool.label()))?;
    let staged = scratch.0.join(&file);
    std::fs::copy(&found, &staged).map_err(|error| error.to_string())?;
    download::make_executable(&staged)?;
    let target = paths::bin_dir().join(&file);
    download::replace_path(&staged, &target)?;
    let installed = tools::version_of(&target, &["--version"])
        .ok_or_else(|| format!("{} was installed but did not start.", tool.label()))?;
    tools::forget_latest(spec.tool);
    log.line(&format!(
        "{} {installed} is installed in {}.",
        tool.label(),
        paths::bin_dir().display()
    ));
    Ok(())
}

/// CDXC:ManagedTools 2026-10-21 WHY: The board skills and the user's own terminal run `bd` by bare name, and a PATH line added to a shell profile reaches neither a terminal or agent that was already open nor a Linux terminal that skips the login profile, so Settings showed Beads installed while `bd` was "not found" (issue #269). Like the uv and Claude installers, Ghostex links the tool into ~/.local/bin when that folder is already on the user's PATH, which works at once everywhere; only when it isn't (or Windows, or the name there is taken by someone else's file, which is never replaced) does it add its tools folder to the end of PATH. Returns what changed, for the job output.
pub(crate) fn expose(tool: ToolId, home: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        let file = paths::executable_name(spec(tool).binary);
        let target = paths::bin_dir().join(&file);
        let links = user_bin_dir(home);
        if crate::agent_cli::path_setup::on_path(&links, home) {
            let link = links.join(&file);
            if std::fs::read_link(&link).is_ok_and(|current| current == target) {
                return None;
            }
            if link.symlink_metadata().is_err()
                && std::fs::create_dir_all(&links)
                    .and_then(|()| std::os::unix::fs::symlink(&target, &link))
                    .is_ok()
            {
                return Some(format!(
                    "Linked {} into {} so terminals and agents find it.",
                    file,
                    links.display()
                ));
            }
        }
    }
    match crate::agent_cli::path_setup::ensure_on_path_end(&paths::bin_dir(), home) {
        Ok(message) => message,
        Err(message) => Some(message),
    }
}

#[cfg(unix)]
fn user_bin_dir(home: &Path) -> std::path::PathBuf {
    home.join(".local").join("bin")
}

pub(crate) fn uninstall(tool: ToolId, home: &Path, log: &Log) -> Result<(), String> {
    let target = paths::bin_dir().join(paths::executable_name(spec(tool).binary));
    #[cfg(unix)]
    {
        let link = user_bin_dir(home).join(spec(tool).binary);
        if std::fs::read_link(&link).is_ok_and(|current| current == target) {
            std::fs::remove_file(&link)
                .map_err(|error| format!("Could not remove {}: {error}", link.display()))?;
        }
    }
    #[cfg(not(unix))]
    let _ = home;
    if target.exists() {
        std::fs::remove_file(&target)
            .map_err(|error| format!("Could not remove {}: {error}", target.display()))?;
    }
    log.line(&format!("Removed Ghostex's {}.", tool.label()));
    Ok(())
}
