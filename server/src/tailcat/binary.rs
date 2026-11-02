use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

/*
CDXC:RemotePairing 2026-09-05 SEE-ALSO:
Keep this lookup in sync with apps/desktop/src/app/helpers/remote/easy_connect_forward.rs so both serving and connecting find the helper installed by Settings.
The managed install follows explicit overrides and PATH; existing Go and Homebrew installs are still recognized.
*/
fn tailcat_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    candidates.extend(override_tailcat_binary());
    candidates.extend(bundled_tailcat_binary());
    candidates.extend(installed_tailcat_binaries());
    candidates
}

fn override_tailcat_binary() -> Option<PathBuf> {
    env::var_os("GHOSTEX_TAILCAT_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Beside gxserver: the Linux package (remote computers, Cloud Boxes, the Linux desktop packages)
/// ships tailcat in its bin/.
fn bundled_tailcat_binary() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    ghostex_paths::tailcat::bundled_tailcat_binary(&exe)
}

/// Every tailcat installed on this computer apart from the bundled one, in the order Easy Connect
/// hosted with before Ghostex packages shipped their own.
fn installed_tailcat_binaries() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("PATH") {
        candidates
            .extend(env::split_paths(&path).map(|directory| directory.join(executable_name())));
    }
    candidates.push(managed_tailcat_binary());
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join("go").join("bin").join(executable_name()));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin").join(executable_name()));
    candidates
}

/// What [`select_host_tailcat_binary`] chooses from.
#[derive(Clone, Debug, Default)]
pub(crate) struct HostTailcatCandidates {
    pub(crate) override_binary: Option<PathBuf>,
    /// The helper that last hosted Easy Connect here ([`tailcat_host_binary_file`]).
    pub(crate) recorded: Option<PathBuf>,
    /// Whether this computer already has an Easy Connect identity (its server key).
    pub(crate) has_identity: bool,
    pub(crate) managed: PathBuf,
    pub(crate) bundled: Option<PathBuf>,
    pub(crate) installed: Vec<PathBuf>,
}

/// The tailcat this computer hosts Easy Connect with.
///
/// CDXC:RemotePairing 2026-11-02 DECISION: the user chose to keep every paired phone's address.
/// A computer that already hosts Easy Connect (it has a server key) keeps the helper that hosted
/// it: the one recorded beside the key, or for a computer that hosted before the record existed,
/// the first installed helper in the old order (PATH, the Settings install, `~/go/bin`, Homebrew),
/// with the copy bundled beside gxserver last. Moving to another helper version changes the address
/// (v0.6.0 adds a pre-shared key) and would cut off every phone paired with the old one. A computer
/// without an identity starts on the Settings install, then the bundled copy, so a fresh Linux
/// package hosts with the copy it ships. `GHOSTEX_TAILCAT_BIN` always wins.
/// SEE-ALSO: server/src/tailcat/keys.rs (the key and the record), supervisor.rs (writes the record).
pub(crate) fn select_host_tailcat_binary(
    candidates: HostTailcatCandidates,
    is_executable: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let HostTailcatCandidates {
        override_binary,
        recorded,
        has_identity,
        managed,
        bundled,
        installed,
    } = candidates;
    let mut order: Vec<PathBuf> = override_binary.into_iter().collect();
    if has_identity {
        order.extend(recorded);
        order.extend(installed);
        order.extend(bundled);
    } else {
        order.push(managed.clone());
        order.extend(bundled);
        order.extend(installed.into_iter().filter(|path| *path != managed));
    }
    order.into_iter().find(|candidate| is_executable(candidate))
}

/// The tailcat this computer hosts Easy Connect with ([`select_host_tailcat_binary`]).
pub fn resolve_host_tailcat_binary(paths: &crate::paths::GxserverPaths) -> Option<PathBuf> {
    select_host_tailcat_binary(
        HostTailcatCandidates {
            override_binary: override_tailcat_binary(),
            recorded: read_recorded_host_tailcat_binary(paths),
            has_identity: super::keys::tailcat_key_file(paths).is_file(),
            managed: managed_tailcat_binary(),
            bundled: bundled_tailcat_binary(),
            installed: installed_tailcat_binaries(),
        },
        is_executable_file,
    )
}

fn read_recorded_host_tailcat_binary(paths: &crate::paths::GxserverPaths) -> Option<PathBuf> {
    let text = fs::read_to_string(super::keys::tailcat_host_binary_file(paths)).ok()?;
    let path = text.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Remembers the helper that hosts Easy Connect, so a later start keeps it (and the address).
pub fn record_host_tailcat_binary(
    paths: &crate::paths::GxserverPaths,
    binary_path: &Path,
) -> std::io::Result<()> {
    if read_recorded_host_tailcat_binary(paths).as_deref() == Some(binary_path) {
        return Ok(());
    }
    let file = super::keys::tailcat_host_binary_file(paths);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, format!("{}\n", binary_path.display()))
}

/// The helper this computer dials a box or an Easy Connect machine with: Ghostex's own install
/// first, then the serving lookup's other candidates, whichever is at least
/// `ghostex_paths::tailcat::TAILCAT_MIN_VERSION` (an older client never completes a v0.6.0
/// server's handshake). `None` when none is.
///
/// CDXC:RemotePairing 2026-11-02 WHY: serving keeps [`resolve_host_tailcat_binary`]'s choice on purpose:
/// moving this computer's own Easy Connect server to another helper version changes its address
/// (v0.6.0 adds a pre-shared key) and would cut off phones paired with the old one. Dialing out
/// has no such state, so it takes the newest-enough helper and leaves an old one where it is.
/// SEE-ALSO: apps/desktop/src/app/helpers/remote/easy_connect_forward.rs (the desktop's same rule).
pub fn resolve_client_tailcat_binary() -> Option<PathBuf> {
    let mut candidates = vec![managed_tailcat_binary()];
    candidates.extend(tailcat_binary_candidates());
    candidates
        .into_iter()
        .filter(|candidate| is_executable_file(candidate))
        .find(|candidate| {
            read_tailcat_binary_version(candidate)
                .is_some_and(|version| ghostex_paths::tailcat::tailcat_version_supported(&version))
        })
}

pub(crate) fn managed_tailcat_binary() -> PathBuf {
    ghostex_paths::tailcat::managed_tailcat_binary(&ghostex_paths::GhostexPaths::resolve().data_dir)
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "tailcat.exe"
    } else {
        "tailcat"
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

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

pub fn read_tailcat_binary_version(binary_path: &Path) -> Option<String> {
    let mut child = crate::platform::process::background_command(binary_path)
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + VERSION_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => return None,
        }
    }
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text.lines().next().unwrap_or_default().trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("/t/{name}/tailcat"))
    }

    fn candidates(has_identity: bool, recorded: Option<&str>) -> HostTailcatCandidates {
        HostTailcatCandidates {
            override_binary: None,
            recorded: recorded.map(path),
            has_identity,
            managed: path("managed"),
            bundled: Some(path("bundled")),
            installed: vec![path("on-path"), path("managed"), path("go")],
        }
    }

    fn pick(candidates: HostTailcatCandidates, present: &[&str]) -> Option<PathBuf> {
        let present: Vec<PathBuf> = present.iter().map(|name| path(name)).collect();
        select_host_tailcat_binary(candidates, |candidate| {
            present.iter().any(|known| known == candidate)
        })
    }

    #[test]
    fn a_hosting_computer_keeps_the_recorded_helper_over_the_bundled_one() {
        let chosen = pick(
            candidates(true, Some("go")),
            &["go", "managed", "bundled", "on-path"],
        );
        assert_eq!(chosen, Some(path("go")));
    }

    #[test]
    fn a_computer_that_hosted_before_the_record_keeps_the_old_order_and_the_bundled_copy_is_last() {
        assert_eq!(
            pick(candidates(true, None), &["managed", "bundled", "go"]),
            Some(path("managed"))
        );
        assert_eq!(
            pick(candidates(true, None), &["go", "bundled"]),
            Some(path("go"))
        );
        assert_eq!(
            pick(candidates(true, None), &["bundled"]),
            Some(path("bundled"))
        );
    }

    #[test]
    fn a_recorded_helper_that_is_gone_falls_back_to_the_old_order() {
        assert_eq!(
            pick(candidates(true, Some("removed")), &["on-path", "bundled"]),
            Some(path("on-path"))
        );
    }

    #[test]
    fn a_fresh_computer_hosts_with_the_settings_install_then_the_bundled_copy() {
        assert_eq!(
            pick(candidates(false, None), &["managed", "bundled", "on-path"]),
            Some(path("managed"))
        );
        assert_eq!(
            pick(candidates(false, None), &["bundled", "on-path", "go"]),
            Some(path("bundled"))
        );
        assert_eq!(pick(candidates(false, None), &["go"]), Some(path("go")));
        assert_eq!(pick(candidates(false, None), &[]), None);
    }

    #[test]
    fn the_override_always_wins() {
        let mut with_override = candidates(true, Some("go"));
        with_override.override_binary = Some(path("override"));
        assert_eq!(
            pick(with_override, &["override", "go", "bundled"]),
            Some(path("override"))
        );
    }

    #[test]
    fn the_host_record_round_trips_beside_the_key() {
        let home = tempfile::tempdir().expect("temp home");
        let paths = crate::paths::get_gxserver_paths(Some(home.path().to_path_buf()));
        assert_eq!(read_recorded_host_tailcat_binary(&paths), None);
        record_host_tailcat_binary(&paths, &path("managed")).expect("record");
        assert_eq!(
            read_recorded_host_tailcat_binary(&paths),
            Some(path("managed"))
        );
        assert!(
            super::super::keys::tailcat_host_binary_file(&paths).starts_with(&paths.tailcat_dir)
        );
        record_host_tailcat_binary(&paths, &path("bundled")).expect("record again");
        assert_eq!(
            read_recorded_host_tailcat_binary(&paths),
            Some(path("bundled"))
        );
    }

    /// The real resolution on a computer with a key and a record: the recorded helper wins over
    /// an executable that sits where the bundled copy would.
    #[cfg(unix)]
    #[test]
    fn resolve_host_tailcat_binary_follows_the_record_when_an_identity_exists() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().expect("temp home");
        let paths = crate::paths::get_gxserver_paths(Some(home.path().to_path_buf()));
        fs::create_dir_all(&paths.tailcat_dir).expect("tailcat dir");
        fs::write(super::super::keys::tailcat_key_file(&paths), "{}").expect("key");
        let hosted = home.path().join("hosted-tailcat");
        fs::write(&hosted, "#!/bin/sh\n").expect("helper");
        fs::set_permissions(&hosted, fs::Permissions::from_mode(0o755)).expect("chmod");
        record_host_tailcat_binary(&paths, &hosted).expect("record");
        if env::var_os("GHOSTEX_TAILCAT_BIN").is_none() {
            assert_eq!(resolve_host_tailcat_binary(&paths), Some(hosted));
        }
    }
}
