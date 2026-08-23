use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};

use crate::config::read_selected_local_api_port;

/*
CDXC:RemoteMinimalDeps 2026-07-13:
`gxserver setup` owns the package-activation steps that used to live in a long
generated POSIX-sh install script inside the macOS app (stale-listener stop,
package symlink swap, tool links, CLI wrapper). Keeping this logic in the
binary means every installer (gpui macOS today, Windows later) runs the same
few-line "extract archive, run setup" script, and the remote host only needs
tar plus a shell to bootstrap. The listener stop shells out to ss/lsof exactly
like the old script so setup does not gain new privileges or dependencies.
*/

const REMOTE_TOOL_NAMES: [&str; 4] = ["gxserver", "ghostex", "zmx", "bd"];

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

#[derive(Debug)]
struct SetupOptions {
    install_root: PathBuf,
    release_dir: PathBuf,
    upload_path: Option<PathBuf>,
}

fn parse_setup_args(args: &[String]) -> Result<SetupOptions> {
    let mut install_root: Option<PathBuf> = None;
    let mut release_dir: Option<PathBuf> = None;
    let mut upload_path: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let take_value = |name: &str| -> Result<PathBuf> {
            let value = args
                .get(index + 1)
                .ok_or_else(|| anyhow!("Missing value for {name}"))?;
            Ok(PathBuf::from(value))
        };
        match arg {
            "--install-root" => install_root = Some(take_value("--install-root")?),
            "--release-dir" => release_dir = Some(take_value("--release-dir")?),
            "--upload-path" => upload_path = Some(take_value("--upload-path")?),
            other => return Err(anyhow!("Unknown gxserver setup argument: {other}")),
        }
        index += 2;
    }
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
    })
}

fn default_install_root() -> Result<PathBuf> {
    Ok(ghostex_paths::GhostexPaths::resolve().gxserver_data_dir())
}

#[cfg(unix)]
fn run_setup_unix(options: &SetupOptions) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let release_dir = &options.release_dir;
    let release_gxserver = release_dir.join("bin").join("gxserver");
    if !release_gxserver.is_file() {
        return Err(anyhow!(
            "Release directory {} does not contain bin/gxserver.",
            release_dir.display()
        ));
    }
    let release_name = release_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("Release directory has no usable name."))?
        .to_string();
    let package_link = options.install_root.join("package");

    stop_existing_gxserver(&package_link);

    /*
    A pre-symlink install layout kept the package as a real directory; move it
    aside instead of deleting so a broken upgrade can be recovered by hand.
    */
    if package_link.symlink_metadata().is_ok() && !package_link.is_symlink() {
        let backup = options
            .install_root
            .join(format!("package.backup.{release_name}"));
        fs::rename(&package_link, &backup)
            .with_context(|| format!("move previous package directory to {}", backup.display()))?;
    }
    replace_symlink(release_dir, &package_link)?;

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
    CDXC:GhostexRustCli 2026-07-13:
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
    println!("gxserver setup complete: {}", release_dir.display());
    Ok(())
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

fn command_looks_like_gxserver(command: &str, package_link: &Path) -> bool {
    // Require the executable name after a current or legacy package root so
    // an editor command that merely mentions a gxserver config file cannot match.
    if executable_path_looks_like_packaged_gxserver(command, package_link) {
        return true;
    }
    command.contains("gxserver --foreground")
}

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

fn wait_for_pid_exit(pid: u32) -> bool {
    for _ in 0..30 {
        if !is_pid_alive(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    !is_pid_alive(pid)
}

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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(parse_setup_args(&["--bogus".to_string()]).is_err());
    }
}
