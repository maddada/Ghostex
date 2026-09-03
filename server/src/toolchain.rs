use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::platform::resources;
use crate::protocol::ToolCapabilityStatus;

#[derive(Clone, Copy)]
enum ToolSource {
    DevSubmodule,
    AppResource,
    GxserverBundle,
    SystemPath,
}

impl ToolSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::DevSubmodule => "devSubmodule",
            Self::AppResource => "appResource",
            Self::GxserverBundle => "gxserverBundle",
            Self::SystemPath => "systemPath",
        }
    }
}

struct ToolCandidate {
    executable_path: PathBuf,
    source: ToolSource,
}

enum CandidateInspection {
    Available,
    Missing,
    NotExecutable,
}

#[derive(Clone, Debug)]
pub struct GxserverResolvedTool {
    pub executable_path: String,
    pub source: String,
    pub tool: String,
}

/*
CDXC:GxserverToolchain 2026-06-14-20:37:
Managed terminal/search tools resolve from Ghostex-pinned development or bundled resources. Project-board operations intentionally resolve the user's machine-installed Beads CLI so Ghostex and shell agents use one binary and one schema owner.

CDXC:GxserverUbuntu 2026-06-23-07:52:
The same gxserver binary resolves zmx from package-relative resources on macOS and Ubuntu. Beads is the deliberate exception: resolve the user's system `bd` from portable PATH locations so local and remote boards follow the machine operator's installed version.

CDXC:AgentHistorySearch 2026-08-20:
Zehn is no longer in this list. Prompt-history search is compiled into gxserver
as a Rust crate, so there is no bundled `zehn` executable to resolve, report as
missing, or install on a remote host.
*/
pub fn get_gxserver_tool_statuses() -> Vec<ToolCapabilityStatus> {
    vec![resolve_bundled_tool_status("zmx"), get_bd_tool_status()]
}

pub fn require_bundled_zmx() -> Result<GxserverResolvedTool, String> {
    require_bundled_tool("zmx")
}

pub fn require_system_bd() -> Result<GxserverResolvedTool, String> {
    let status = get_bd_tool_status();
    if status.availability == "available" {
        if let (Some(executable_path), Some(source)) = (status.executable_path, status.source) {
            return Ok(GxserverResolvedTool {
                executable_path,
                source,
                tool: "bd".to_string(),
            });
        }
    }
    Err(status.message)
}

fn require_bundled_tool(tool: &str) -> Result<GxserverResolvedTool, String> {
    let status = resolve_bundled_tool_status(tool);
    if status.availability == "available" {
        if let (Some(executable_path), Some(source)) = (status.executable_path, status.source) {
            return Ok(GxserverResolvedTool {
                executable_path,
                source,
                tool: tool.to_string(),
            });
        }
    }
    Err(status.message)
}

fn resolve_bundled_tool_status(tool: &str) -> ToolCapabilityStatus {
    let candidates = bundled_tool_candidates(tool);
    let inspected = candidates
        .iter()
        .find(|candidate| is_executable_file(&candidate.executable_path));
    if let Some(candidate) = inspected {
        return ToolCapabilityStatus {
            availability: "available".to_string(),
            candidate_paths: None,
            capability: "zmxLifecycle".to_string(),
            executable_path: Some(candidate.executable_path.to_string_lossy().to_string()),
            guidance: None,
            message: format!("{tool} resolved from {}.", candidate.source.as_str()),
            source: Some(candidate.source.as_str().to_string()),
            tool: tool.to_string(),
        };
    }
    let candidate_paths = candidates
        .iter()
        .map(|candidate| candidate.executable_path.to_string_lossy().to_string())
        .collect();
    ToolCapabilityStatus {
        availability: "missing".to_string(),
        candidate_paths: Some(candidate_paths),
        capability: "zmxLifecycle".to_string(),
        executable_path: None,
        guidance: None,
        message: "Ghostex-managed zmx sessions require bundled zmx, but bundled zmx was not found."
            .to_string(),
        source: None,
        tool: tool.to_string(),
    }
}

fn get_bd_tool_status() -> ToolCapabilityStatus {
    get_bd_tool_status_for_candidates(&system_bd_tool_candidates())
}

fn get_bd_tool_status_for_candidates(candidates: &[ToolCandidate]) -> ToolCapabilityStatus {
    if let Some(candidate) = candidates
        .iter()
        .find(|candidate| matches!(inspect_candidate(candidate), CandidateInspection::Available))
    {
        return ToolCapabilityStatus {
            availability: "available".to_string(),
            candidate_paths: None,
            capability: "beadsProjectBoard".to_string(),
            executable_path: Some(candidate.executable_path.to_string_lossy().to_string()),
            guidance: None,
            message: format!("bd resolved from {}.", candidate.source.as_str()),
            source: Some(candidate.source.as_str().to_string()),
            tool: "bd".to_string(),
        };
    }
    let candidate_paths = candidates
        .iter()
        .map(|candidate| candidate.executable_path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if let Some(candidate) = candidates.iter().find(|candidate| {
        matches!(
            inspect_candidate(candidate),
            CandidateInspection::NotExecutable
        )
    }) {
        return ToolCapabilityStatus {
            availability: "notExecutable".to_string(),
            candidate_paths: Some(candidate_paths),
            capability: "beadsProjectBoard".to_string(),
            executable_path: None,
            guidance: Some("Install or update Beads to the latest release, ensure `bd` is executable and available on your PATH, then reopen the Project board. Official installer: `curl -fsSL https://raw.githubusercontent.com/gastownhall/beads/main/scripts/install.sh | bash`.".to_string()),
            message: format!(
                "Ghostex Project board found bd, but it is not executable: {}. Install or update Beads to the latest release.",
                candidate.executable_path.to_string_lossy()
            ),
            source: None,
            tool: "bd".to_string(),
        };
    }
    ToolCapabilityStatus {
        availability: "missing".to_string(),
        candidate_paths: Some(candidate_paths),
        capability: "beadsProjectBoard".to_string(),
        executable_path: None,
        guidance: Some(format!("Install the latest Beads CLI in {}, then reopen the Project board. Official installer: `curl -fsSL https://raw.githubusercontent.com/gastownhall/beads/main/scripts/install.sh | bash`. Other supported installers: https://github.com/gastownhall/beads/blob/main/docs/INSTALLING.md", beads_runtime_environment())),
        message: format!("The Beads CLI (`bd`) was not found in {}. Install the latest Beads release there and ensure `bd` is on that environment's PATH.", beads_runtime_environment()),
        source: None,
        tool: "bd".to_string(),
    }
}

fn bundled_tool_candidates(tool: &str) -> Vec<ToolCandidate> {
    let gxserver_root = default_gxserver_root();
    let repo_root = gxserver_root
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let mut candidates = vec![
        ToolCandidate {
            executable_path: repo_root.join(tool).join("zig-out").join("bin").join(tool),
            source: ToolSource::DevSubmodule,
        },
        ToolCandidate {
            executable_path: gxserver_root.join("bin").join(tool),
            source: ToolSource::GxserverBundle,
        },
        ToolCandidate {
            executable_path: gxserver_root.join("..").join("bin").join(tool),
            source: ToolSource::AppResource,
        },
        ToolCandidate {
            executable_path: gxserver_root.join("..").join("Web").join("bin").join(tool),
            source: ToolSource::AppResource,
        },
        ToolCandidate {
            executable_path: gxserver_root
                .join("..")
                .join("..")
                .join("Web")
                .join("bin")
                .join(tool),
            source: ToolSource::AppResource,
        },
    ];
    if let Some(path) = resources::source_web_resource(&format!("bin/{tool}")) {
        candidates.push(ToolCandidate {
            executable_path: path,
            source: ToolSource::AppResource,
        });
    }
    dedupe_candidates(candidates)
}

fn system_bd_tool_candidates() -> Vec<ToolCandidate> {
    let path_directories = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    let directories = system_bd_directories(
        path_directories,
        env::var_os("HOME").map(PathBuf::from),
        env::var_os("GOBIN").map(PathBuf::from),
    );
    dedupe_candidates(
        directories
            .into_iter()
            .map(|directory| ToolCandidate {
                executable_path: directory.join("bd"),
                source: ToolSource::SystemPath,
            })
            .collect(),
    )
}

fn system_bd_directories(
    mut directories: Vec<PathBuf>,
    home: Option<PathBuf>,
    gobin: Option<PathBuf>,
) -> Vec<PathBuf> {
    /*
    CDXC:ProjectBoardSystemBeads 2026-08-12:
    Finder, systemd, SSH, and WSL can all launch gxserver with a narrower PATH
    than the user's interactive shell. Search the native install locations used
    by Beads' supported installers on macOS and Linux/WSL. Do not cross into a
    Windows-mounted bd.exe from WSL: the Linux gxserver must use a Linux bd so
    filesystem paths and Dolt state stay inside the selected distribution.
    */
    if let Some(home) = home {
        directories.extend([
            home.join(".local/bin"),
            home.join("go/bin"),
            home.join(".cargo/bin"),
            home.join(".bun/bin"),
            home.join(".npm-global/bin"),
            home.join(".linuxbrew/bin"),
            home.join(".local/share/mise/shims"),
            home.join(".asdf/shims"),
            home.join(".nix-profile/bin"),
        ]);
    }
    if let Some(gobin) = gobin {
        directories.push(gobin);
    }
    directories.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/opt/local/bin"),
        PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
        PathBuf::from("/snap/bin"),
        PathBuf::from("/run/current-system/sw/bin"),
        PathBuf::from("/nix/var/nix/profiles/default/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);
    let mut seen = std::collections::HashSet::new();
    directories
        .into_iter()
        .filter(|directory| directory.is_absolute())
        .filter(|directory| seen.insert(directory.clone()))
        .collect()
}

fn beads_runtime_environment() -> &'static str {
    if env::var_os("WSL_DISTRO_NAME").is_some() || env::var_os("WSL_INTEROP").is_some() {
        "this WSL distribution"
    } else if cfg!(target_os = "linux") {
        "this Linux environment"
    } else {
        "this machine"
    }
}

fn default_gxserver_root() -> PathBuf {
    if let Ok(current_exe) = env::current_exe() {
        if let Some(root) = gxserver_root_from_executable_path(&current_exe) {
            return root;
        }
    }
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("gxserver")
}

fn gxserver_root_from_executable_path(executable_path: &Path) -> Option<PathBuf> {
    /*
    CDXC:GxserverToolchain 2026-06-21-13:59:
    The macOS app launches gxserver from Web/gxserver/bin/gxserver while the process current directory is not the package root. Resolve bundled zmx from the running executable's package root first so Rust starts with the same app resources the TypeScript daemon used.
    */
    let parent = executable_path.parent()?;
    if parent.file_name().and_then(|name| name.to_str()) != Some("bin") {
        return None;
    }
    parent.parent().map(Path::to_path_buf)
}

fn dedupe_candidates(candidates: Vec<ToolCandidate>) -> Vec<ToolCandidate> {
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| {
            let key = candidate
                .executable_path
                .components()
                .collect::<PathBuf>()
                .to_string_lossy()
                .to_string();
            seen.insert(key)
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn gxserver_root_from_packaged_executable_uses_package_parent() {
        let root = gxserver_root_from_executable_path(Path::new(
            "/Applications/Ghostex.app/Contents/Resources/Web/gxserver/bin/gxserver",
        ))
        .expect("packaged root");
        assert_eq!(
            root,
            PathBuf::from("/Applications/Ghostex.app/Contents/Resources/Web/gxserver")
        );
    }

    #[test]
    fn gxserver_root_from_dev_target_executable_keeps_cwd_fallback() {
        assert!(gxserver_root_from_executable_path(Path::new(
            "/repo/gxserver-rs/target/release/gxserver"
        ))
        .is_none());
    }

    #[test]
    fn bd_status_reports_system_resolution_source() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bd = dir.path().join("gxserver").join("bin").join("bd");
        fs::create_dir_all(bd.parent().expect("bd parent")).expect("create bd parent");
        fs::write(&bd, "#!/bin/sh\nexit 0\n").expect("write bd");
        make_executable(&bd);
        let status = get_bd_tool_status_for_candidates(&[ToolCandidate {
            executable_path: bd.clone(),
            source: ToolSource::SystemPath,
        }]);

        assert_eq!(status.availability, "available");
        assert_eq!(
            status.executable_path.as_deref(),
            Some(bd.to_str().unwrap())
        );
        assert_eq!(status.source.as_deref(), Some("systemPath"));
        assert!(status.message.contains("systemPath"));
    }

    #[cfg(unix)]
    #[test]
    fn bd_status_reports_present_but_not_executable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let bd = dir.path().join("app").join("bin").join("bd");
        fs::create_dir_all(bd.parent().expect("bd parent")).expect("create bd parent");
        fs::write(&bd, "#!/bin/sh\nexit 0\n").expect("write bd");
        fs::set_permissions(&bd, fs::Permissions::from_mode(0o644)).expect("chmod bd");
        let status = get_bd_tool_status_for_candidates(&[ToolCandidate {
            executable_path: bd.clone(),
            source: ToolSource::SystemPath,
        }]);

        assert_eq!(status.availability, "notExecutable");
        assert!(status.message.contains("not executable"));
        assert!(status.guidance.unwrap_or_default().contains("update Beads"));
    }

    #[test]
    fn bd_status_reports_missing_with_project_board_guidance() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing").join("bd");
        let status = get_bd_tool_status_for_candidates(&[ToolCandidate {
            executable_path: missing,
            source: ToolSource::SystemPath,
        }]);

        assert_eq!(status.availability, "missing");
        assert!(status.message.contains("was not found in"));
        assert!(status
            .guidance
            .unwrap_or_default()
            .contains("Install the latest Beads CLI"));
    }

    #[test]
    fn bd_system_directories_cover_linux_wsl_and_common_user_installers() {
        let home = PathBuf::from("/home/ghostex-user");
        let directories = system_bd_directories(
            vec![PathBuf::from("relative-bin"), PathBuf::from("/custom/bin")],
            Some(home.clone()),
            Some(PathBuf::from("/custom/go/bin")),
        );

        for expected in [
            PathBuf::from("/custom/bin"),
            PathBuf::from("/custom/go/bin"),
            home.join(".local/bin"),
            home.join("go/bin"),
            home.join(".bun/bin"),
            home.join(".npm-global/bin"),
            home.join(".linuxbrew/bin"),
            home.join(".nix-profile/bin"),
            PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
            PathBuf::from("/snap/bin"),
            PathBuf::from("/usr/local/bin"),
        ] {
            assert!(
                directories.contains(&expected),
                "missing {}",
                expected.display()
            );
        }
        assert!(!directories.contains(&PathBuf::from("relative-bin")));
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod executable");
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn inspect_candidate(candidate: &ToolCandidate) -> CandidateInspection {
    let Ok(metadata) = fs::metadata(&candidate.executable_path) else {
        return CandidateInspection::Missing;
    };
    if !metadata.is_file() {
        return CandidateInspection::Missing;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 != 0 {
            CandidateInspection::Available
        } else {
            CandidateInspection::NotExecutable
        }
    }
    #[cfg(not(unix))]
    {
        let _ = candidate;
        CandidateInspection::Available
    }
}
