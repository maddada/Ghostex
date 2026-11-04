#[cfg(unix)]
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

#[cfg(unix)]
use anyhow::Context;
use anyhow::{anyhow, Result};

#[cfg(unix)]
use crate::config::read_selected_local_api_port;

/*
CDXC:RemotePairing 2026-07-13:
`gxserver setup` owns the package-activation steps that used to live in a long
generated POSIX-sh install script inside the macOS app (stale-listener stop,
package symlink swap, tool links, CLI wrapper). Keeping this logic in the
binary means every installer (gpui macOS today, Windows later) runs the same
few-line "extract archive, run setup" script, and the remote host only needs
tar plus a shell to bootstrap. The listener stop shells out to ss/lsof exactly
like the old script so setup does not gain new privileges or dependencies.
*/

#[cfg(unix)]
const REMOTE_TOOL_NAMES: [&str; 3] = ["gxserver", "ghostex", "zmx"];

pub fn run_setup(args: Vec<String>) -> Result<()> {
    #[cfg(not(unix))]
    {
        let _ = args;
        Err(anyhow!(
            "gxserver setup currently supports Unix hosts only."
        ))
    }
    #[cfg(unix)]
    {
        let options = parse_setup_args(&args)?;
        run_setup_unix(&options)
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct SetupOptions {
    install_root: PathBuf,
    release_dir: PathBuf,
    upload_path: Option<PathBuf>,
    /*
    CDXC:Telemetry 2026-08-26:
    Declared by the installer, never guessed here. `gxserver setup` runs for BOTH
    remote SSH installs and the Windows desktop app's local WSL install, so the
    subcommand itself is not evidence of a remote role — only the caller knows.
    */
    analytics_role_is_remote: bool,
}

#[cfg(unix)]
fn parse_setup_args(args: &[String]) -> Result<SetupOptions> {
    let mut install_root: Option<PathBuf> = None;
    let mut release_dir: Option<PathBuf> = None;
    let mut upload_path: Option<PathBuf> = None;
    let mut analytics_role: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let take_value = |name: &str| -> Result<PathBuf> {
            let value = args
                .get(index + 1)
                .ok_or_else(|| anyhow!("Missing value for {name}"))?;
            Ok(PathBuf::from(value))
        };
        let take_text = |name: &str| -> Result<String> {
            args.get(index + 1)
                .cloned()
                .ok_or_else(|| anyhow!("Missing value for {name}"))
        };
        match arg {
            "--install-root" => install_root = Some(take_value("--install-root")?),
            "--release-dir" => release_dir = Some(take_value("--release-dir")?),
            "--upload-path" => upload_path = Some(take_value("--upload-path")?),
            "--analytics-role" => analytics_role = Some(take_text("--analytics-role")?),
            other => return Err(anyhow!("Unknown gxserver setup argument: {other}")),
        }
        index += 2;
    }
    let analytics_role_is_remote = match analytics_role.as_deref().map(str::trim) {
        None => false,
        Some(value) if value.eq_ignore_ascii_case(crate::telemetry::role::REMOTE_ROLE_VALUE) => {
            true
        }
        Some(value) if value.eq_ignore_ascii_case("local") => false,
        Some(other) => {
            return Err(anyhow!(
                "Unknown gxserver setup --analytics-role value: {other}"
            ))
        }
    };
    let install_root = match install_root {
        Some(value) => value,
        None => default_install_root()?,
    };
    /*
    The setup binary lives at <release_dir>/bin/gxserver after extraction, so
    the release directory is derivable from the running executable when the
    installer does not pass it explicitly.
    */
    let release_dir = match release_dir {
        Some(value) => value,
        None => env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().and_then(Path::parent).map(Path::to_path_buf))
            .ok_or_else(|| anyhow!("Could not infer --release-dir from the running binary."))?,
    };
    Ok(SetupOptions {
        install_root,
        release_dir,
        upload_path,
        analytics_role_is_remote,
    })
}

#[cfg(unix)]
fn default_install_root() -> Result<PathBuf> {
    Ok(ghostex_paths::GhostexPaths::resolve().gxserver_data_dir())
}

#[cfg(unix)]
fn run_setup_unix(options: &SetupOptions) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let release_dir = &options.release_dir;
    if let Some(tool) = release_missing_tool(release_dir) {
        return Err(anyhow!(
            "Release directory {} does not contain a working bin/{tool}, so the current install was left as it is.",
            release_dir.display()
        ));
    }
    let release_name = release_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("Release directory has no usable name."))?
        .to_string();
    let package_link = options.install_root.join("package");

    /*
    CDXC:Telemetry 2026-08-26:
    Written BEFORE the old daemon is stopped and the new package activated, so
    no gxserver this install is responsible for can ever run without the marker
    already on disk. It lands in the state dir, which upgrades never touch, so
    one install covers every later restart, reboot, and package upgrade.
    */
    if options.analytics_role_is_remote {
        crate::telemetry::role::write_remote_marker(&crate::paths::get_gxserver_paths(None));
    }

    stop_existing_gxserver(&package_link);

    activate_release_package(
        &options.install_root,
        release_dir,
        &release_name,
        cfg!(target_os = "macos"),
    )?;
    #[cfg(target_os = "macos")]
    {
        remove_leftover_releases(&options.install_root, &running_executable_paths());
        register_app_bundles(&package_link);
    }

    let local_bin = local_bin_dir();
    if let Some(local_bin) = &local_bin {
        let _ = fs::create_dir_all(local_bin);
    }
    for tool in REMOTE_TOOL_NAMES {
        let tool_path = package_link.join("bin").join(tool);
        if !tool_path.is_file() {
            continue;
        }
        let _ = fs::set_permissions(&tool_path, fs::Permissions::from_mode(0o755));
        if let Some(local_bin) = &local_bin {
            let _ = replace_symlink(&tool_path, &local_bin.join(tool));
        }
    }
    /*
    CDXC:ProjectBoard 2026-09-03:
    Ghostex no longer ships Beads. Earlier releases linked the package's bin/bd
    into ~/.local/bin on remote SSH hosts and inside WSL, so those hosts keep a
    dangling `bd` symlink into a Ghostex release directory once the package
    stops carrying it. Remove only that symlink: a real file, or a link that
    points outside the install root, is the user's own Beads install.
    */
    if let Some(local_bin) = &local_bin {
        remove_stale_ghostex_bd_link(&local_bin.join("bd"), &options.install_root);
    }
    /*
    CDXC:Cli 2026-07-13:
    bin/ghostex is the native Rust CLI shipped in the package (it replaced the
    Node CLI wrapper + CLI/ghostex-cli.mjs). The tool loop above already
    linked it into ~/.local/bin; here only the `gx` alias is created.
    */
    let ghostex_path = package_link.join("bin").join("ghostex");
    let gx_path = package_link.join("bin").join("gx");
    if ghostex_path.is_file() {
        let _ = replace_symlink(&ghostex_path, &gx_path);
    }
    if is_executable_file(&gx_path) {
        if let Some(local_bin) = &local_bin {
            let _ = replace_symlink(&gx_path, &local_bin.join("gx"));
        }
    }

    if let Some(upload_path) = &options.upload_path {
        let _ = fs::remove_file(upload_path);
    }
    let installed = fs::canonicalize(&package_link).unwrap_or_else(|_| release_dir.clone());
    println!("gxserver setup complete: {}", installed.display());
    Ok(())
}

/// The first program the release cannot run without that is missing or a dangling link
/// (`is_file` follows links). Checked before anything stops or moves.
///
/// CDXC:RemoteMachines 2026-10-31 WHY: the tool loop in `run_setup_unix` skips a program it cannot
/// find, so a release without a working zmx activated and its machine read connected until a
/// session failed (GitHub #360).
#[cfg(unix)]
fn release_missing_tool(release_dir: &Path) -> Option<&'static str> {
    REMOTE_TOOL_NAMES
        .into_iter()
        .find(|tool| !release_dir.join("bin").join(tool).is_file())
}

/// Points `package` at the release, first moving it into the fixed `current` folder when
/// `fixed_folder` is set (macOS).
#[cfg(unix)]
fn activate_release_package(
    install_root: &Path,
    release_dir: &Path,
    release_name: &str,
    fixed_folder: bool,
) -> Result<()> {
    let package_link = install_root.join("package");
    /*
    A pre-symlink install layout kept the package as a real directory; move it
    aside instead of deleting so a broken upgrade can be recovered by hand.
    */
    if package_link.symlink_metadata().is_ok() && !package_link.is_symlink() {
        let backup = install_root.join(format!("package.backup.{release_name}"));
        fs::rename(&package_link, &backup)
            .with_context(|| format!("move previous package directory to {}", backup.display()))?;
    }
    if fixed_folder {
        let current = move_release_into_fixed_folder(install_root, release_dir, release_name)?;
        return replace_symlink(&current, &package_link);
    }
    replace_symlink(release_dir, &package_link)
}

/// CDXC:RemotePairing 2026-11-03 WHY: macOS privacy settings (Files and Folders, Local Network and the like) remember a program that is not an app by its file path plus its signature. Every install and upgrade unpacks into a new `releases/release-<uuid>` folder, so a Mac running only gxserver (a remote Mac mini) saw a new gxserver and zmx after each Ghostex update, asked for every permission again and filled Privacy & Security with duplicate rows (Discord, 2026-11-03). The release is moved into one fixed `current` folder instead, the way an app update replaces Ghostex.app in place, so the same Developer ID signed programs keep one path and their grants. Running zmx sessions keep their programs (a rename never touches a running file) and their launchd supervisors only watch the session socket. The replaced release stays beside the others for recovery by hand; older replaced copies are removed. Linux has no such permission store and keeps a folder per release.
#[cfg(unix)]
fn move_release_into_fixed_folder(
    install_root: &Path,
    release_dir: &Path,
    release_name: &str,
) -> Result<PathBuf> {
    const REPLACED_PREFIX: &str = "replaced-by-";
    let current = install_root.join("current");
    // A release dir inferred from a `package/bin/gxserver` path is the link, not the folder.
    let release_dir = &fs::canonicalize(release_dir).unwrap_or_else(|_| release_dir.to_path_buf());
    // `setup` run again from the installed copy.
    if fs::canonicalize(&current).ok().as_ref() == Some(release_dir) {
        return Ok(current);
    }
    let releases = install_root.join("releases");
    let replaced = releases.join(format!("{REPLACED_PREFIX}{release_name}"));
    let had_current = current.symlink_metadata().is_ok();
    if had_current {
        fs::create_dir_all(&releases).context("create releases directory")?;
        let _ = fs::remove_dir_all(&replaced);
        fs::rename(&current, &replaced)
            .with_context(|| format!("move the installed release to {}", replaced.display()))?;
    }
    if let Err(error) = fs::rename(release_dir, &current) {
        if had_current {
            let _ = fs::rename(&replaced, &current);
        }
        return Err(error).with_context(|| {
            format!(
                "move release {} to {}",
                release_dir.display(),
                current.display()
            )
        });
    }
    if let Ok(entries) = fs::read_dir(&releases) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path != replaced && name.to_string_lossy().starts_with(REPLACED_PREFIX) {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
    Ok(current)
}

/// CDXC:RemotePairing 2026-11-04 WHY: installs from before the fixed `current` folder left one `releases/release-<uuid>` folder per update (about 150 MB each) and, from the pre-symlink layout, `package.backup.<release>` folders, which nothing removed (Discord, randomsand). Once a macOS setup has activated `current` they are removed, except a folder `package` still resolves into or that a running program (a zmx session started before the upgrade, which keeps its program until it ends) was started from; the next setup removes those once they are free. The one `replaced-by-*` copy stays for recovery by hand.
#[cfg(unix)]
fn remove_leftover_releases(install_root: &Path, running_executables: &[PathBuf]) {
    let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let package = canonical(&install_root.join("package"));
    let running: Vec<PathBuf> = running_executables
        .iter()
        .map(|exe| canonical(exe))
        .collect();
    let in_use = |folder: &Path| {
        let folder = canonical(folder);
        package.starts_with(&folder) || running.iter().any(|exe| exe.starts_with(&folder))
    };
    let leftovers = [
        (install_root.join("releases"), "release-"),
        (install_root.to_path_buf(), "package.backup."),
    ];
    for (parent, prefix) in leftovers {
        let Ok(entries) = fs::read_dir(&parent) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
            if is_folder
                && entry.file_name().to_string_lossy().starts_with(prefix)
                && !in_use(&path)
            {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
}

/// CDXC:RemotePairing 2026-11-04 WHY: zmx session jobs name GhostexSessions.app in `AssociatedBundleIdentifiers` (`macos_zmx_bundle_identifier` in server/src/zmx/launch.rs), which macOS resolves through Launch Services, and nothing else registers bundles under a hidden ~/.local folder. Best effort: an unregistered bundle only loses that association.
#[cfg(target_os = "macos")]
fn register_app_bundles(package_link: &Path) {
    const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    for bundle in ["GhostexServer.app", "GhostexSessions.app"] {
        if let Ok(bundle) = fs::canonicalize(package_link.join(bundle)) {
            let _ = Command::new(LSREGISTER).arg("-f").arg(bundle).output();
        }
    }
}

/// The program file of every process this user can see (`proc_pidpath` reports where the file
/// is now, so a program moved into `current` reads as `current`).
#[cfg(target_os = "macos")]
fn running_executable_paths() -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStrExt;

    let mut pids = vec![0 as libc::pid_t; 8192];
    let count = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int,
        )
    };
    pids.truncate(count.max(0) as usize);
    let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    pids.into_iter()
        .filter_map(|pid| {
            let length =
                unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
            (length > 0)
                .then(|| PathBuf::from(std::ffi::OsStr::from_bytes(&buffer[..length as usize])))
        })
        .collect()
}

#[cfg(unix)]
fn remove_stale_ghostex_bd_link(link_path: &Path, install_root: &Path) {
    if !link_path.is_symlink() {
        return;
    }
    let Ok(target) = fs::read_link(link_path) else {
        return;
    };
    let target = if target.is_absolute() {
        target
    } else {
        link_path
            .parent()
            .map(|parent| parent.join(&target))
            .unwrap_or(target)
    };
    /*
    The link is usually dangling by now (the release dir it pointed into is
    gone), so compare the raw target as well as its canonical form against both
    spellings of the install root.
    */
    let resolved = fs::canonicalize(&target).unwrap_or_else(|_| target.clone());
    let canonical_root =
        fs::canonicalize(install_root).unwrap_or_else(|_| install_root.to_path_buf());
    let points_into_install_root = [&target, &resolved]
        .iter()
        .any(|path| path.starts_with(install_root) || path.starts_with(&canonical_root));
    if points_into_install_root {
        let _ = fs::remove_file(link_path);
    }
}

#[cfg(unix)]
fn replace_symlink(target: &Path, link_path: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    /*
    Match `ln -sfn`: create under a temporary name, then rename over the link
    path so an existing link is swapped without a window where it is missing.
    */
    let parent = link_path
        .parent()
        .ok_or_else(|| anyhow!("Symlink path {} has no parent.", link_path.display()))?;
    let temp_path = parent.join(format!(
        ".{}.tmp-link-{}",
        link_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("link"),
        std::process::id()
    ));
    let _ = fs::remove_file(&temp_path);
    symlink(target, &temp_path)
        .with_context(|| format!("create symlink for {}", link_path.display()))?;
    fs::rename(&temp_path, link_path)
        .with_context(|| format!("activate symlink at {}", link_path.display()))?;
    Ok(())
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    match path.metadata() {
        Ok(metadata) => metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(unix)]
fn local_bin_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("bin"))
}

/*
Best-effort stop of the previously installed gxserver so the new release can
bind the API port: ask the old binary to stop itself, then TERM/KILL any
process still listening on the gxserver port that verifiably is a gxserver.
Every step tolerates missing tools (ss, lsof, ps) exactly like the shell
script this replaces.
*/
#[cfg(unix)]
fn stop_existing_gxserver(package_link: &Path) {
    let old_gxserver = package_link.join("bin").join("gxserver");
    #[cfg(unix)]
    if is_executable_file(&old_gxserver) {
        let stopped = Command::new(&old_gxserver)
            .args(["stop", "--json"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if !stopped {
            let _ = Command::new(&old_gxserver).arg("stop").output();
        }
    }
    let port = read_selected_local_api_port().unwrap_or(crate::constants::GXSERVER_LOCAL_API_PORT);
    for pid in listener_pids(port) {
        if !is_gxserver_pid(pid, package_link) {
            continue;
        }
        terminate_pid(pid, false);
        if !wait_for_pid_exit(pid) && is_gxserver_pid(pid, package_link) {
            terminate_pid(pid, true);
        }
    }
}

#[cfg(unix)]
fn listener_pids(port: u16) -> Vec<u32> {
    let mut pids = Vec::new();
    if let Ok(output) = Command::new("ss").args(["-ltnp"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let port_token = format!(":{port} ");
        for line in stdout.lines() {
            if !line.contains(&port_token) {
                continue;
            }
            let mut rest = line;
            while let Some(start) = rest.find("pid=") {
                let tail = &rest[start + 4..];
                let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
                if let Ok(pid) = digits.parse::<u32>() {
                    pids.push(pid);
                }
                rest = tail;
            }
        }
    }
    if let Ok(output) = Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fp"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(digits) = line.strip_prefix('p') {
                if let Ok(pid) = digits.parse::<u32>() {
                    pids.push(pid);
                }
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

#[cfg(unix)]
fn is_gxserver_pid(pid: u32, package_link: &Path) -> bool {
    if pid == 0 {
        return false;
    }
    if let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) {
        let cmdline = String::from_utf8_lossy(&cmdline).replace('\0', " ");
        if command_looks_like_gxserver(&cmdline, package_link) {
            return true;
        }
    }
    if let Ok(output) = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
    {
        let command = String::from_utf8_lossy(&output.stdout);
        if command_looks_like_gxserver(&command, package_link) {
            return true;
        }
    }
    if let Ok(exe) = fs::read_link(format!("/proc/{pid}/exe")) {
        let exe = exe.to_string_lossy();
        if executable_path_looks_like_packaged_gxserver(&exe, package_link) {
            return true;
        }
        if exe.ends_with("/gxserver (deleted)") {
            return true;
        }
    }
    false
}

#[cfg(unix)]
fn executable_path_looks_like_packaged_gxserver(command: &str, package_link: &Path) -> bool {
    let package_executable = package_link.join("bin/gxserver");
    if command.contains(package_executable.to_string_lossy().as_ref()) {
        return true;
    }
    if let Ok(release_executable) = fs::canonicalize(&package_executable) {
        if command.contains(release_executable.to_string_lossy().as_ref()) {
            return true;
        }
    }
    for marker in ["/ghostex/gxserver/", "/.ghostex/gxserver/"] {
        if let Some(index) = command.find(marker) {
            if command[index + marker.len()..].contains("gxserver") {
                return true;
            }
        }
    }
    false
}

#[cfg(unix)]
fn command_looks_like_gxserver(command: &str, package_link: &Path) -> bool {
    // Require the executable name after a current or legacy package root so
    // an editor command that merely mentions a gxserver config file cannot match.
    if executable_path_looks_like_packaged_gxserver(command, package_link) {
        return true;
    }
    command.contains("gxserver --foreground")
}

#[cfg(unix)]
fn terminate_pid(pid: u32, force: bool) {
    #[cfg(unix)]
    {
        let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
        unsafe {
            libc::kill(pid as libc::pid_t, signal);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, force);
    }
}

#[cfg(unix)]
fn wait_for_pid_exit(pid: u32) -> bool {
    for _ in 0..30 {
        if !is_pid_alive(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    !is_pid_alive(pid)
}

#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// The report's install, a real package directory with a dangling `zmx` link, repaired by a
    /// whole release: the broken one is kept aside and `package` runs the new one. A release that
    /// is itself broken is refused before anything moves. Never calls `run_setup_unix`, whose
    /// listener stop would reach this computer's own gxserver.
    #[test]
    fn a_broken_package_directory_is_replaced_and_a_broken_release_is_refused() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let tools = |dir: &Path, zmx_link: Option<&str>| {
            fs::create_dir_all(dir.join("bin")).expect("bin");
            fs::write(dir.join("bin/gxserver"), "").expect("gxserver");
            symlink("gxserver", dir.join("bin/ghostex")).expect("ghostex");
            match zmx_link {
                Some(target) => symlink(target, dir.join("bin/zmx")).expect("zmx link"),
                None => fs::write(dir.join("bin/zmx"), "").expect("zmx"),
            }
        };
        tools(&root.join("package"), Some("../../bin/zmx"));
        assert_eq!(release_missing_tool(&root.join("package")), Some("zmx"));

        let broken_release = root.join("releases/release-broken");
        tools(&broken_release, Some("../../bin/zmx"));
        assert_eq!(release_missing_tool(&broken_release), Some("zmx"));

        let release = root.join("releases/release-good");
        tools(&release, None);
        assert_eq!(release_missing_tool(&release), None);
        activate_release_package(root, &release, "release-good", false).expect("activate");
        assert!(root.join("package").is_symlink());
        assert!(root.join("package/bin/zmx").is_file());
        assert!(root
            .join("package.backup.release-good/bin/zmx")
            .is_symlink());
    }

    /// Two upgrades on macOS: each release ends up in the same `current` folder, so the programs
    /// keep one path (and their macOS privacy grants); the release it replaced is kept once, and a
    /// rerun from the installed copy changes nothing.
    #[test]
    fn macos_upgrades_keep_one_program_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let release = |name: &str, build: &str| {
            let dir = root.join("releases").join(name);
            fs::create_dir_all(dir.join("bin")).expect("bin");
            for tool in REMOTE_TOOL_NAMES {
                fs::write(dir.join("bin").join(tool), build).expect("tool");
            }
            dir
        };
        let first = release("release-a", "a");
        activate_release_package(root, &first, "release-a", true).expect("first install");
        let installed_zmx = fs::canonicalize(root.join("package/bin/zmx")).expect("zmx");
        assert_eq!(
            installed_zmx,
            fs::canonicalize(root).unwrap().join("current/bin/zmx")
        );
        assert!(!first.exists());

        let second = release("release-b", "b");
        activate_release_package(root, &second, "release-b", true).expect("upgrade");
        assert_eq!(
            fs::canonicalize(root.join("package/bin/zmx")).unwrap(),
            installed_zmx
        );
        assert_eq!(
            fs::read_to_string(root.join("package/bin/zmx")).unwrap(),
            "b"
        );
        assert_eq!(
            fs::read_to_string(root.join("releases/replaced-by-release-b/bin/zmx")).unwrap(),
            "a"
        );

        let third = release("release-c", "c");
        activate_release_package(root, &third, "release-c", true).expect("second upgrade");
        assert_eq!(
            fs::canonicalize(root.join("package/bin/gxserver")).unwrap(),
            installed_zmx.with_file_name("gxserver")
        );
        assert!(!root.join("releases/replaced-by-release-b").exists());
        assert!(root.join("releases/replaced-by-release-c").is_dir());

        activate_release_package(root, &root.join("package"), "package", true).expect("rerun");
        assert_eq!(
            fs::read_to_string(root.join("current/bin/zmx")).unwrap(),
            "c"
        );
        assert!(root.join("releases/replaced-by-release-c").is_dir());
    }

    /// The report's install after an upgrade into `current`: old `release-<uuid>` folders and a
    /// `package.backup.*` folder go, the folder a running zmx was started from stays until it ends,
    /// and `current`, the one `replaced-by-*` copy and anything else in the install are untouched.
    #[test]
    fn leftover_releases_are_removed_unless_something_runs_from_them() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let folder = |relative: &str| {
            let dir = root.join(relative);
            fs::create_dir_all(dir.join("bin")).expect("folder");
            fs::write(dir.join("bin/zmx"), "").expect("zmx");
            dir
        };
        for relative in [
            "releases/release-1111",
            "releases/release-2222",
            "releases/release-busy",
            "releases/replaced-by-release-3333",
            "package.backup.release-0000",
            "current",
            "state",
        ] {
            folder(relative);
        }
        fs::write(root.join("releases/release-note.txt"), "").expect("file");
        std::os::unix::fs::symlink(root.join("current"), root.join("package")).expect("package");
        let running_zmx = root.join("releases/release-busy/bin/zmx");

        remove_leftover_releases(root, &[running_zmx.clone(), PathBuf::from("/bin/zsh")]);
        assert!(!root.join("releases/release-1111").exists());
        assert!(!root.join("releases/release-2222").exists());
        assert!(!root.join("package.backup.release-0000").exists());
        assert!(running_zmx.is_file());
        assert!(root
            .join("releases/replaced-by-release-3333/bin/zmx")
            .is_file());
        assert!(root.join("releases/release-note.txt").is_file());
        assert!(root.join("current/bin/zmx").is_file());
        assert!(root.join("state").is_dir());

        // Once the session ends, the next setup removes its folder.
        remove_leftover_releases(root, &[]);
        assert!(!root.join("releases/release-busy").exists());
        assert!(root.join("package/bin/zmx").is_file());
    }

    /// A Linux-style install whose `package` still points into a `release-<uuid>` folder keeps it.
    #[test]
    fn the_folder_package_points_at_is_never_removed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let release = root.join("releases/release-live");
        fs::create_dir_all(release.join("bin")).expect("release");
        std::os::unix::fs::symlink(&release, root.join("package")).expect("package");
        remove_leftover_releases(root, &[]);
        assert!(release.join("bin").is_dir());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn running_programs_include_this_test() {
        let this = fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
        assert!(running_executable_paths()
            .iter()
            .any(|path| fs::canonicalize(path).ok().as_ref() == Some(&this)));
    }

    #[test]
    fn command_matching_requires_gxserver_markers() {
        let package_link = Path::new("/absolute/custom-root/gxserver/package");
        assert!(command_looks_like_gxserver(
            "/absolute/custom-root/gxserver/package/bin/gxserver --foreground",
            package_link,
        ));
        assert!(command_looks_like_gxserver(
            "/home/user/.ghostex/gxserver/package/bin/gxserver --foreground",
            package_link,
        ));
        assert!(command_looks_like_gxserver(
            "/home/user/.local/share/ghostex/gxserver/package/bin/gxserver --foreground",
            package_link,
        ));
        assert!(command_looks_like_gxserver(
            "gxserver --foreground",
            package_link
        ));
        assert!(!command_looks_like_gxserver(
            "/usr/bin/node server.js",
            package_link
        ));
        assert!(!command_looks_like_gxserver(
            "vi /home/user/.ghostex/gxserver/config.json",
            package_link,
        ));
    }

    #[test]
    fn parse_setup_args_requires_known_flags() {
        let parsed = parse_setup_args(&[
            "--install-root".to_string(),
            "/tmp/root".to_string(),
            "--release-dir".to_string(),
            "/tmp/root/releases/release-1".to_string(),
            "--upload-path".to_string(),
            "/tmp/root/upload.tar.gz".to_string(),
        ])
        .expect("parse setup args");
        assert_eq!(parsed.install_root, PathBuf::from("/tmp/root"));
        assert_eq!(
            parsed.release_dir,
            PathBuf::from("/tmp/root/releases/release-1")
        );
        assert_eq!(
            parsed.upload_path,
            Some(PathBuf::from("/tmp/root/upload.tar.gz"))
        );
        assert!(!parsed.analytics_role_is_remote);
        assert!(parse_setup_args(&["--bogus".to_string()]).is_err());
    }
}
