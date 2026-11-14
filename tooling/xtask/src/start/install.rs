//! Installing the staged build over the installed app, handing gxserver over, and launching.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::{windows, Start};
use crate::bail;
use crate::gxserver::{self, Probe};
use crate::http::NoAnswer;
use crate::util::{self, home_dir, root, sleep_ms, Res, Summary};

impl Start {
    pub fn install_and_launch(&self) -> Res {
        let installed = self.installed_app_path.clone();
        if self.is_windows && windows::runs_in_services_session()? {
            return self.hand_off_install_to_desktop();
        }
        if self.targets_windows {
            let target = self.windows_installed_app_path.clone().unwrap_or_default();
            self.close_running_bundle(
                &installed,
                &format!("before installing rebuilt app to {target}"),
                false,
            )?;
            // CDXC:ServerDaemon 2026-09-18 WHY:
            // Closing the app leaves its gxserver running, as on macOS, so live wmx sessions survive the relaunch.
            // Without this stop the previous build's control plane kept the port and went on serving the rebuilt app, so a local start never ran the gxserver it had just compiled.
            // /api/control/stop ends only the control plane (stopAll is the call that kills sessions); the rebuilt app then starts its bundled gxserver.
            // A WSL-driven start is not covered: its state lives on the Windows side, which ghostex_state_dir does not resolve from Linux.
            // Native Linux stops it too (2026-10-01): otherwise a start that replaced an older installed package kept that package's gxserver serving the rebuilt app.
            if self.is_windows {
                self.stop_gxserver_control_plane()?;
            }
            self.install_windows_app()?;
            self.log.step(&format!("Opening {}...", self.app_name));
            return self.launch_windows_app();
        }
        if self.is_darwin {
            self.ensure_installed_macos_app_is_writable()?;
            self.remove_stale_macos_app_copies();
            self.close_running_bundle(
                &installed,
                &format!("before installing rebuilt app to {}", installed.display()),
                true,
            )?;
            self.stop_gxserver_control_plane()?;
            return self.install_and_open_macos_app();
        }
        let needs_root = self.linux_install_needs_root();
        if needs_root {
            // Ask for the password before anything is closed, so a cancelled prompt leaves the app running.
            self.log.step(&format!(
                "Installing to {} needs administrator rights (sudo)...",
                installed.display()
            ));
            util::check(util::command("sudo").arg("-v"))?;
        }
        let mut running = vec![installed.clone()];
        if self
            .linux_packaged_app_path
            .as_ref()
            .is_some_and(|packaged| *packaged != self.linux_user_installed_app_path)
        {
            running.push(self.linux_user_installed_app_path.clone());
        }
        for bundle in running {
            self.close_running_bundle(
                &bundle,
                &format!("before installing rebuilt app to {}", installed.display()),
                false,
            )?;
        }
        self.stop_gxserver_control_plane()?;
        self.log.step(&format!(
            "Installing {} to {}...",
            self.app_name,
            installed.display()
        ));
        if !needs_root {
            fs::create_dir_all(installed.parent().expect("install path has a parent"))?;
        }
        self.sync_installed_bundle(needs_root)?;
        self.log.step(&format!("Opening {}...", self.app_name));
        self.launch_linux_app()
    }

    /// CDXC:Build 2026-09-23 WHY:
    /// On 2026-09-21 macOS App Management refused the sync into the installed app halfway ("Operation not permitted"), leaving a broken app, and an agent then installed every later build by hand as Ghostex-new.app plus `mv Ghostex.app Ghostex.old-<time>.app`, leaving a 1.7GB copy per install.
    /// Probe before the running app is closed, so a blocked install stops with the fix instead of breaking the app.
    /// Installing in place, not by swapping in a new bundle, is deliberate: a kept gxserver and live zmx sessions run from files inside the installed bundle.
    fn ensure_installed_macos_app_is_writable(&self) -> Res {
        if !self.installed_app_path.exists() {
            return Ok(());
        }
        let probe = self
            .installed_app_path
            .join("Contents")
            .join(format!(".ghostex-install-probe-{}", std::process::id()));
        if let Err(error) = fs::write(&probe, "") {
            bail!(
                "macOS will not let this start modify {} ({error}).\nThis is the App Management privacy setting. Open System Settings > Privacy & Security > App Management and turn it on for the app this command runs in (Ghostex, or your terminal), then run the start again.\nDo not install the build by hand under another name: every such install leaves a full copy of the app behind.",
                self.installed_app_path.display()
            );
        }
        let _ = fs::remove_file(probe);
        Ok(())
    }

    /// Removes full app copies that hand installs left beside the installed app, identified by their bundle id and never while running.
    fn remove_stale_macos_app_copies(&self) {
        let Ok(entries) = fs::read_dir(&self.install_dir) else {
            return;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if !is_leftover_copy_name(&name, &self.app_name) {
                continue;
            }
            let copy_path = self.install_dir.join(&name);
            let info_plist = copy_path.join("Contents").join("Info.plist");
            let bundle_id = util::stdout_if_ok(
                Command::new("/usr/libexec/PlistBuddy")
                    .args(["-c", "Print CFBundleIdentifier"])
                    .arg(&info_plist),
            )
            .map(|out| out.trim().to_string());
            if bundle_id.as_deref() != Some(self.bundle_id.as_str())
                || !self.pids_by_bundle_path(&copy_path).is_empty()
            {
                continue;
            }
            if fs::remove_dir_all(&copy_path).is_ok() {
                self.log.detail(&format!(
                    "Removed leftover app copy {}.",
                    copy_path.display()
                ));
            }
        }
    }

    fn install_and_open_macos_app(&self) -> Res {
        self.log.step(&format!(
            "Installing {} to {}...",
            self.app_name,
            self.install_dir.display()
        ));
        self.sync_installed_bundle(false)?;
        self.log.step("Checking installed GPUI app signature...");
        self.ensure_installed_app_signature()?;
        // CDXC:Build 2026-08-25:
        // rsync copies staged bundle contents into the existing /Applications wrapper and does not copy Finder package flags. Without the bundle bit, Launch Services reports kLSNoExecutableErr even though the Mach-O exists. Set the bit after signing: SetFile writes Finder information, which codesign rejects as detritus if it is present beforehand.
        util::check(
            Command::new("/usr/bin/SetFile")
                .args(["-a", "B"])
                .arg(&self.installed_app_path),
        )?;
        self.log.step("Preparing LaunchServices environment...");
        let published = if self.isolated.is_some() {
            0
        } else {
            publish_launch_services_gxserver_environment()
        };
        self.log.detail(&if published > 0 {
            format!(
                "Published {published} explicit gxserver daemon override{}.",
                if published == 1 { "" } else { "s" }
            )
        } else {
            "No explicit gxserver daemon override is set; GPUI will use its bundled daemon."
                .to_string()
        });
        self.log.step(&format!("Opening {}...", self.app_name));
        let mut open = util::command("open");
        open.arg(&self.installed_app_path);
        if self.opts.profile {
            open.args(["--args", "--profile"]);
        }
        util::check(&mut open)?;
        self.verify_canonical_macos_launch()?;
        self.log.detail(&format!(
            "One canonical app process is running from {}.",
            self.installed_app_path.display()
        ));
        Ok(())
    }

    fn verify_canonical_macos_launch(&self) -> Res {
        let deadline = Instant::now() + Duration::from_secs(10);
        let (mut bundle_pids, mut canonical_pids) = (Vec::new(), Vec::new());
        while Instant::now() < deadline {
            bundle_pids = self.pids_by_bundle_id();
            canonical_pids = self.pids_by_bundle_path(&self.installed_app_path);
            if bundle_pids.len() == 1 && canonical_pids.contains(&bundle_pids[0]) {
                return Ok(());
            }
            sleep_ms(100);
        }
        bail!(
            "Expected exactly one {} app launched from {}; found {} bundle process(es) and {} canonical bundle process(es).",
            self.bundle_id,
            self.installed_app_path.display(),
            bundle_pids.len(),
            canonical_pids.len()
        )
    }

    fn sync_installed_bundle(&self, as_root: bool) -> Res {
        // A package-owned folder stays root-owned, like the package manager left it.
        let source = format!("{}/", self.app_path.display());
        let target = format!("{}/", self.installed_app_path.display());
        if self.log.verbose || as_root {
            let mut rsync = if as_root {
                util::command("sudo")
            } else {
                util::command("rsync")
            };
            if as_root {
                rsync.args(["rsync", "-a", "--delete", "--chown=root:root"]);
            } else {
                rsync.args(["-a", "--delete"]);
            }
            util::check(rsync.arg(&source).arg(&target))?;
        } else {
            let mut rsync = util::command("rsync");
            rsync
                .args(["-a", "--delete", "--itemize-changes"])
                .arg(&source)
                .arg(&target);
            self.log.run(
                &mut rsync,
                &format!("Install {} bundle", self.app_name),
                Summary::Rsync,
            )?;
        }
        self.log.detail(&format!(
            "Installed bundle synced to {}.",
            self.installed_app_path.display()
        ));
        Ok(())
    }

    fn ensure_installed_app_signature(&self) -> Res {
        let (reusable, reason) = self.inspect_installed_signature()?;
        if reusable {
            self.log.detail(&format!(
                "Installed signature is current; skipping re-sign ({reason})."
            ));
            return Ok(());
        }
        self.log
            .detail(&format!("Re-signing installed GPUI app bundle ({reason})."));
        let mut sign = self.build_command(
            &root()
                .join("apps/desktop/scripts/codesign-gpui-app.sh")
                .display()
                .to_string(),
        );
        sign.arg(&self.installed_app_path);
        self.log.run(
            &mut sign,
            &format!("Installed {} signing", self.app_name),
            Summary::Codesign,
        )?;
        self.log.detail("Installed app bundle signed.");
        Ok(())
    }

    fn inspect_installed_signature(&self) -> Res<(bool, &'static str)> {
        // CDXC:Build 2026-09-04 WHY:
        // This check used `--strict`, which rejects the Finder bundle bit that the install sets on the installed wrapper after every install, so it failed on every start and the whole 1.4GB installed bundle was re-signed inside-out each time. Deep verification without `--strict` still validates every nested signature and the resource seal, and strict verification already ran on the staged bundle at sign time, before the bundle bit was set.
        let verify = util::output(
            Command::new("codesign")
                .args(["--verify", "--deep"])
                .arg(&self.installed_app_path),
        )?;
        if !verify.status.success() {
            return Ok((false, "existing signature failed deep verification"));
        }
        let Some(details) = crate::codesign::signature_details(&self.installed_app_path) else {
            return Ok((
                false,
                "existing signature does not match the requested local-start identity",
            ));
        };
        let expected = self
            .build_env
            .iter()
            .find(|(key, _)| key == "GHOSTEX_GPUI_SIGN_IDENTITY")
            .map(|(_, value)| value.as_str())
            .unwrap_or("-");
        let matches = if expected.is_empty() || expected == "-" {
            details.contains("Signature=adhoc") || details.contains("TeamIdentifier=not set")
        } else {
            details
                .lines()
                .map(str::trim)
                .any(|line| line == format!("Authority={expected}"))
        };
        Ok(if matches {
            (true, "deep verification and signing identity match")
        } else {
            (
                false,
                "existing signature does not match the requested local-start identity",
            )
        })
    }

    pub fn stop_gxserver_control_plane(&self) -> Res {
        self.log.step("Checking gxserver control plane...");
        let expected = self.bundled_gxserver_build_identity();
        if expected.is_none() {
            eprintln!("The built GPUI app has no bundled gxserver build identity; stopping any running control plane anyway.");
        }
        let Some((token, token_file)) = gxserver::read_token_file() else {
            self.log
                .detail("No gxserver auth token found; nothing to stop.");
            return Ok(());
        };
        let endpoints = if self.is_windows {
            self.windows_gxserver_endpoints()?
        } else {
            vec![(self.gxserver_base_url.clone(), None, true)]
        };
        if endpoints.is_empty() {
            self.log.detail("No running gxserver control plane found.");
        }
        // A gxserver holds more than one loopback port (its API and the connected-computers port), so the Windows listener list names each process once per port: decide per process.
        let mut processes: Vec<(Option<u64>, bool, Vec<String>)> = Vec::new();
        for (base_url, pid, keep_same_build) in endpoints {
            match processes
                .iter_mut()
                .find(|(seen, _, _)| pid.is_some() && *seen == pid)
            {
                Some((_, _, urls)) => urls.push(base_url),
                None => processes.push((pid, keep_same_build, vec![base_url])),
            }
        }
        for (pid, keep_same_build, urls) in processes {
            let expected = expected.as_deref().filter(|_| keep_same_build);
            self.stop_gxserver_process(&urls, pid, &token, &token_file, expected)?;
        }
        Ok(())
    }

    fn stop_gxserver_process(
        &self,
        urls: &[String],
        pid: Option<u64>,
        token: &str,
        token_file: &Path,
        expected: Option<&str>,
    ) -> Res {
        let probes: Vec<(&str, Probe)> = urls
            .iter()
            .map(|url| (url.as_str(), gxserver::probe_patiently(url, token)))
            .collect();
        let heard = |answered: bool| {
            probes
                .iter()
                .filter(|(_, probe)| probe.answered() == answered)
                .map(|(url, probe)| format!("{url} {}", probe.describe()))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let healthy = probes.iter().find_map(|(url, probe)| match probe {
            Probe::Healthy(health) => Some((*url, health)),
            _ => None,
        });
        let Some((base_url, health)) = healthy else {
            let who = pid.map_or_else(
                || "gxserver".to_string(),
                |pid| format!("gxserver pid {pid}"),
            );
            if probes.iter().any(|(_, probe)| probe.answered()) {
                // It answers, so it is not hung; it only refuses this token on every port it holds.
                let message = format!(
                    "{who} is running ({}), but none of its ports accepted the token in {}, so it was left running.",
                    heard(true),
                    token_file.display()
                );
                return match pid {
                    Some(pid) => {
                        bail!("{message} Quit Ghostex, or end pid {pid}, and start again.")
                    }
                    None => {
                        self.log.detail(&message);
                        Ok(())
                    }
                };
            }
            let Some(pid) = pid else {
                if probes.iter().all(|(_, probe)| {
                    matches!(
                        probe,
                        Probe::NoAnswer(NoAnswer::Refused | NoAnswer::ConnectTimedOut)
                    )
                }) {
                    self.log.detail("No running gxserver control plane found.");
                } else {
                    self.log.detail(&format!(
                        "{} (asked for {} s); leaving it running.",
                        heard(false),
                        gxserver::PATIENT_PROBE.as_secs()
                    ));
                }
                return Ok(());
            };
            // CDXC:ServerDaemon 2026-10-12 WHY: a gxserver whose listener is bound but not served (a slow startup, a hung shutdown) used to fail the start here or be launched beside; it is ended so the app starts its bundled daemon. Only after every port it holds stayed silent for PATIENT_PROBE (2026-11-09).
            self.log.detail(&format!(
                "gxserver pid {pid} holds its ports but did not answer for {} s ({}); ending it.",
                gxserver::PATIENT_PROBE.as_secs(),
                heard(false)
            ));
            return self.end_gxserver_process(pid);
        };
        for (url, probe) in &probes {
            if *url != base_url {
                self.log.detail(&format!(
                    "{url} {}: not the API port of this gxserver; checking {base_url}.",
                    probe.describe()
                ));
            }
        }
        if let Some(pid) = pid {
            if health.get("pid").and_then(|p| p.as_u64()) != Some(pid) {
                bail!("Could not verify the gxserver listener owned by pid {pid} at {base_url}.");
            }
        }
        let health = health.clone();
        let actual = gxserver::build_identity_of(&health);
        // CDXC:ServerDaemon 2026-09-23 WHY:
        // Every start used to stop the control plane even when the rebuilt app bundles the same gxserver, costing up to 5s plus a daemon cold start on launch. The build identity hashes the whole staged gxserver package, so a match means the running daemon already is the code this start would launch.
        if expected.is_some_and(|expected| expected == actual) {
            self.log
                .detail("gxserver control plane already runs the bundled build; keeping it.");
            return Ok(());
        }
        if self.log.verbose {
            let suffix = match expected {
                Some(expected) if !actual.is_empty() => {
                    format!(" (build identity {actual} -> {expected})")
                }
                _ => String::new(),
            };
            println!(
                "Stopping gxserver control plane before opening {}{suffix}.",
                self.app_name
            );
        } else {
            let reason = match expected {
                None => {
                    "it runs from another install, or the bundled build identity is unavailable"
                }
                Some(_) if actual.is_empty() => "running daemon did not report a build identity",
                Some(_) => "bundled daemon changed",
            };
            self.log.detail(&format!(
                "Stopping running gxserver control plane ({reason})."
            ));
        }
        gxserver::request_stop(base_url, token);
        if !gxserver::wait_for_stop(base_url, token, Duration::from_secs(5)) {
            let Some(pid) = pid else {
                bail!(
                    "gxserver stop was requested, but the old control plane is still responding."
                );
            };
            self.log.detail(&format!(
                "gxserver pid {pid} did not stop when asked; ending it."
            ));
            return self.end_gxserver_process(pid);
        }
        self.log.detail(
            "gxserver control plane stopped; GPUI will start its bundled daemon on launch.",
        );
        Ok(())
    }

    /// Ends a gxserver process found on a Windows listener (only those endpoints carry a pid).
    fn end_gxserver_process(&self, pid: u64) -> Res {
        #[cfg(windows)]
        if u32::try_from(pid).is_ok_and(|pid| super::windows_native::terminate_process(pid, 10_000))
        {
            self.log.detail(
                "gxserver control plane ended; GPUI will start its bundled daemon on launch.",
            );
            return Ok(());
        }
        bail!("Could not end the gxserver process {pid} that holds its port.")
    }

    fn bundled_gxserver_build_identity(&self) -> Option<String> {
        let identity_path = if self.is_windows {
            self.app_path.join("resources").join("build-identity.json")
        } else if self.is_darwin {
            self.app_path
                .join("Contents/Resources/Web/gxserver/build-identity.json")
        } else {
            self.app_path.join("gxserver").join("build-identity.json")
        };
        gxserver::read_build_identity(&identity_path)
    }

    fn linux_install_needs_root(&self) -> bool {
        let target = if self.installed_app_path.exists() {
            self.installed_app_path.clone()
        } else {
            self.installed_app_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default()
        };
        !writable(&target)
    }

    /// Opens the installed app in the desktop session (crate::linux_desktop holds the why).
    fn launch_linux_app(&self) -> Res {
        let executable = self.installed_app_path.join("Ghostex");
        let desktop = crate::linux_desktop::find().map_err(|why| {
            format!(
                "{} is installed at {}, but was not opened: {why}.",
                self.app_name,
                self.installed_app_path.display()
            )
        })?;
        let mut launch = if desktop.via_systemd {
            let mut scope = Command::new("systemd-run");
            scope
                .args(["--user", "--scope", "--collect", "--quiet"])
                .arg(format!(
                    "--unit=app-ghostex-{}-{}.scope",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or_default()
                ))
                .arg(format!("--description={}", self.app_name))
                .arg("--")
                .arg(&executable);
            scope
        } else {
            Command::new(&executable)
        };
        launch.env_clear().envs(desktop.env);
        if let Some(home) = util::env_trimmed("GHOSTEX_HOME") {
            launch.env("GHOSTEX_HOME", home);
        }
        if let Some(config) = &self.isolated {
            launch.envs(config.environment.iter().map(|(k, v)| (*k, v.as_str())));
        }
        launch.current_dir(&self.app_path);
        if self.opts.profile {
            launch.arg("--profile");
        }
        self.log.detail(&format!("Opening in {}.", desktop.summary));
        launch
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            launch.process_group(0);
        }
        let child = launch
            .spawn()
            .map_err(|error| util::spawn_error(&launch, error))?;
        println!("Launched {} (pid {}).", executable.display(), child.id());
        Ok(())
    }
}

/// `<app>.old-*`, `.broken-*`, `.bak-*`, `.backup-*`, `-new` and `-old` copies of the app bundle.
fn is_leftover_copy_name(name: &str, app_name: &str) -> bool {
    let Some(middle) = name
        .strip_prefix(app_name)
        .and_then(|rest| rest.strip_suffix(".app"))
    else {
        return false;
    };
    if middle == "-new" || middle == "-old" {
        return true;
    }
    ["old", "broken", "bak", "backup"].iter().any(|kind| {
        middle
            .strip_prefix(&format!(".{kind}-"))
            .is_some_and(|rest| !rest.contains('/'))
    })
}

fn publish_launch_services_gxserver_environment() -> usize {
    let mut published = 0;
    for key in ["GHOSTEX_GXSERVER_CLI", "GHOSTEX_GXSERVER_BIN"] {
        match util::env_trimmed(key) {
            Some(value) => {
                if util::output(Command::new("launchctl").args(["setenv", key, &value]))
                    .is_ok_and(|o| o.status.success())
                {
                    published += 1;
                }
            }
            None => {
                let _ = util::output(Command::new("launchctl").args(["unsetenv", key]));
            }
        }
    }
    published
}

#[cfg(unix)]
fn writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(not(unix))]
fn writable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false)
}

/// CDXC:Build 2026-10-01 DECISION:
/// User asked that the Linux start go "and replaces the currently installed app from aur or from whatever package manager". A package (the AUR `ghostex-bin`, the .deb) puts the app folder at /opt/ghostex and a `/usr/bin/ghostex` wrapper that runs `<app>/gxserver/bin/ghostex`, and the menu entry and the `ghostex` command both open that copy, so a start installs over it (with sudo) instead of into ~/.local/share where nothing launches it. The next package upgrade overwrites it again, which is expected. `INSTALL_DIR` and `--isolated` still install where they say.
pub fn resolve_linux_packaged_app_path() -> Option<PathBuf> {
    for launcher in ["/usr/bin/ghostex", "/usr/local/bin/ghostex"] {
        let Ok(resolved) = fs::canonicalize(launcher) else {
            continue;
        };
        let cli_path = if resolved
            .to_string_lossy()
            .ends_with("/gxserver/bin/ghostex")
        {
            Some(resolved.display().to_string())
        } else if fs::metadata(&resolved).is_ok_and(|m| m.len() < 64 * 1024) {
            fs::read_to_string(&resolved)
                .ok()
                .and_then(|text| wrapper_exec_target(&text))
        } else {
            None
        };
        let Some(cli_path) = cli_path else { continue };
        let app_root = Path::new(&cli_path)
            .parent()?
            .parent()?
            .parent()?
            .to_path_buf();
        let home_prefix = format!("{}/", home_dir().display());
        if !app_root.to_string_lossy().starts_with(&home_prefix)
            && app_root.join("Ghostex").exists()
        {
            return Some(app_root);
        }
    }
    None
}

/// The absolute `.../gxserver/bin/ghostex` path in a wrapper script's `exec "<path>"` line.
fn wrapper_exec_target(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(index) = rest.find("exec") {
        let after = &rest[index + 4..];
        rest = after;
        if !after.starts_with(char::is_whitespace) {
            continue;
        }
        let candidate = after.trim_start().trim_start_matches('"');
        if !candidate.starts_with('/') {
            continue;
        }
        let end = candidate
            .find(|c: char| c == '"' || c.is_whitespace())
            .unwrap_or(candidate.len());
        let path = &candidate[..end];
        if path.ends_with("/gxserver/bin/ghostex") {
            return Some(path.to_string());
        }
    }
    None
}
