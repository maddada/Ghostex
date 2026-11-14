//! `cargo xtask start`: build the desktop app, install it and launch it (macOS, Linux, Windows and Windows driven from WSL).

mod close;
mod install;
mod sweep;
mod windows;
#[cfg(windows)]
mod windows_native;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Instant;

use crate::isolated::{self, Isolated};
use crate::util::{
    self, acquire_start_lock, bun, env_trimmed, env_var, format_duration, home_dir, output, root,
    truthy, Log, Res, Summary,
};
use crate::{bail, codesign};

pub struct Options {
    pub verbose: bool,
    pub profile: bool,
    pub optimized: bool,
    pub prepare_only: bool,
    pub build_only: bool,
    pub install_only: bool,
    /// Windows: install to Program Files instead of the per-user release layout.
    pub machine: bool,
    /// `--debug-info[=<level>]` or `GHOSTEX_DEBUG_INFO`: build with debug info and symbols for crash debugging (CDXC:Build 2026-10-11).
    pub debug_info: Option<String>,
}

/// Everything one start resolves up front: the app identity, where it is staged and installed, and the build environment.
pub struct Start {
    pub log: Log,
    pub opts: Options,
    pub isolated: Option<Isolated>,
    pub app_name: String,
    pub bundle_id: String,
    pub is_darwin: bool,
    pub is_windows: bool,
    pub is_wsl: bool,
    pub targets_windows: bool,
    pub install_dir: PathBuf,
    pub app_path: PathBuf,
    pub linux_packaged_app_path: Option<PathBuf>,
    pub installed_app_path: PathBuf,
    pub linux_user_installed_app_path: PathBuf,
    pub windows_installed_app_path: Option<String>,
    pub windows_install_scope: Option<windows::InstallScope>,
    pub gxserver_base_url: String,
    pub build_env: Vec<(String, String)>,
    /// Where build-macos-app.sh keeps local-start code-server builds outside the bundle (CDXC:CodeEditor 2026-09-23).
    pub code_server_store_root: PathBuf,
}

const USAGE: &str = "cargo xtask start [--verbose|-v] [--profile] [--optimized] [--debug-info[=line-tables-only|limited|full]] [--build-only] [--install-only (Linux, Windows)] [--isolated[=<variant>]] [--prepare-only (Windows)] [--machine (Windows)]";

pub fn run(args: &[String]) -> Res<i32> {
    if !(cfg!(target_os = "macos") || cfg!(target_os = "linux") || cfg!(windows)) {
        bail!("The GPUI local app currently runs on macOS, Linux, and Windows.");
    }
    let is_darwin = cfg!(target_os = "macos");
    let is_windows = cfg!(windows);
    let is_wsl = runs_in_wsl();
    let targets_windows = is_windows || is_wsl;

    let isolated = resolve_isolated_variant(args)?
        .map(|variant| isolated::configuration(&variant))
        .transpose()?;
    let opts = parse_options(args, targets_windows)?;
    let rerun_hint = match &isolated {
        Some(config) => isolated::start_command(&config.variant),
        None => "cargo xtask start".to_string(),
    } + if opts.prepare_only {
        " --prepare-only"
    } else {
        ""
    };

    if let Some(config) = &isolated {
        isolated::prepare(config)?;
    }
    // The desktop-session install a session-0 start hands off to runs while that start holds the lock and has already checked the sources.
    let desktop_handoff = is_windows && env_trimmed(windows::DESKTOP_HANDOFF_ENV).is_some();
    std::env::remove_var(windows::DESKTOP_HANDOFF_ENV);
    let _lock = if desktop_handoff {
        None
    } else {
        Some(acquire_start_lock("start")?)
    };
    if !desktop_handoff {
        check_client_storage()?;
    }
    if let Some(config) = &isolated {
        for (key, value) in &config.environment {
            std::env::set_var(key, value);
        }
    }
    util::remove_color_disabling_environment();
    util::remove_powershell7_module_paths();

    let app_name = isolated
        .as_ref()
        .map_or("Ghostex".to_string(), |c| c.app_name.clone());
    let bundle_id = isolated
        .as_ref()
        .map_or("com.madda.ghostex.gpui".to_string(), |c| {
            c.bundle_id.clone()
        });
    let gpui_dir = root().join("apps").join("desktop");
    let windows_install = if targets_windows {
        Some(windows::resolve_install_paths(
            is_wsl,
            opts.machine,
            &app_name,
        )?)
    } else {
        None
    };
    let install_dir = match (&isolated, &windows_install) {
        (_, Some(paths)) => PathBuf::from(&paths.host_path)
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default(),
        (Some(config), None) => config.install_dir.clone(),
        (None, None) => resolve_gpui_install_dir(is_darwin),
    };
    // CDXC:Build 2026-07-08-04:55:
    // The start builds the staged GPUI package and installs it to a stable, platform-appropriate location before launch. macOS refreshes shared resources, then installs to /Applications and opens through LaunchServices. Windows installs the staged CEF package to the per-user release layout (Program Files with --machine, or GHOSTEX_INSTALL_DIR; CDXC:Build 2026-10-07), creates a Start Menu shortcut, and launches that installed copy. Linux installs the flat CEF package under XDG data (or INSTALL_DIR), preserves gxserver/zmx sessions across the relaunch, and runs the installed executable.
    let app_path = if is_darwin {
        gpui_dir
            .join("build")
            .join("macos.noindex")
            .join(format!("{app_name}.app"))
    } else if targets_windows {
        gpui_dir.join("build").join("windows").join(&app_name)
    } else {
        gpui_dir.join("build").join("linux").join(&app_name)
    };
    let linux_packaged_app_path = if cfg!(target_os = "linux")
        && !is_wsl
        && isolated.is_none()
        && env_trimmed("INSTALL_DIR").is_none()
    {
        install::resolve_linux_packaged_app_path()
    } else {
        None
    };
    let installed_app_path = match &windows_install {
        Some(paths) => Some(PathBuf::from(&paths.host_path)),
        None => linux_packaged_app_path.clone(),
    }
    .unwrap_or_else(|| {
        install_dir.join(if is_darwin {
            format!("{app_name}.app")
        } else {
            app_name.clone()
        })
    });
    let gxserver_port = isolated
        .as_ref()
        .and_then(|c| c.env("GHOSTEX_GXSERVER_DEV_PORT"))
        .unwrap_or("58744")
        .to_string();

    let mut start = Start {
        log: Log::new(
            truthy(env_var("GHOSTEX_GPUI_START_VERBOSE"))
                || truthy(env_var("GHOSTEX_START_VERBOSE"))
                || opts.verbose,
            rerun_hint,
        ),
        opts,
        app_name: app_name.clone(),
        bundle_id,
        is_darwin,
        is_windows,
        is_wsl,
        targets_windows,
        linux_user_installed_app_path: install_dir.join(&app_name),
        windows_installed_app_path: windows_install
            .as_ref()
            .map(|paths| paths.windows_path.clone()),
        windows_install_scope: windows_install.as_ref().map(|paths| paths.scope),
        install_dir,
        app_path,
        linux_packaged_app_path,
        installed_app_path,
        gxserver_base_url: format!("http://127.0.0.1:{gxserver_port}"),
        build_env: Vec::new(),
        code_server_store_root: root()
            .join("build")
            .join("dev-components.noindex")
            .join("code-server"),
        isolated,
    };
    start.opts.verbose = start.log.verbose;
    start.run()
}

impl Start {
    fn run(&mut self) -> Res<i32> {
        if self.opts.install_only {
            return self.install_staged_build();
        }
        let gpui_dir = root().join("apps").join("desktop");
        let require_wsl_runtime =
            env_var("GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME").as_deref() != Some("0");
        let platform_label = if self.is_darwin {
            format!("{}, {}", configuration(), macos_arch()?)
        } else if self.targets_windows {
            if self.is_wsl {
                "Windows via WSL2".into()
            } else if require_wsl_runtime {
                "Windows, WSL2".into()
            } else {
                "Windows, PowerShell".into()
            }
        } else {
            "Linux".into()
        };
        self.log.step(&format!(
            "Checking local GPUI resources ({platform_label})..."
        ));
        if self.is_windows {
            let mut prepare = Command::new(windows::powershell(self.is_wsl));
            prepare
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(root().join("tooling").join("prepare-windows-build.ps1"))
                .current_dir(root());
            self.log
                .run(&mut prepare, "Windows build prerequisites", Summary::None)?;
        }
        self.ensure_reference_checkouts()?;
        self.log.detail("Reference checkouts are ready.");

        let windows_runtime = if self.targets_windows {
            Some(windows::RuntimeArchives::resolve(require_wsl_runtime)?)
        } else {
            None
        };

        if self.is_darwin {
            // The identity probe runs security and codesign, so it happens once, under the lock.
            let identity = codesign::resolve_identity(&self.installed_app_path)?;
            self.build_env.extend([
                ("CONFIGURATION".into(), configuration()),
                ("GHOSTEX_APP_VARIANT".into(), "prod".into()),
                // Keep the packager output path identical to app_path/installed_app_path so the installed application and every macOS-owned label use the public Ghostex product name.
                ("GHOSTEX_GPUI_APP_NAME".into(), self.app_name.clone()),
                ("GHOSTEX_GPUI_BUNDLE_ID".into(), self.bundle_id.clone()),
                ("GHOSTEX_GPUI_SIGN_IDENTITY".into(), identity),
                (
                    "GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG".into(),
                    codesign::timestamp_flag(),
                ),
                ("GHOSTEX_LOCAL_START".into(), "1".into()),
                ("GHOSTEX_MACOS_ARCH".into(), macos_arch()?),
            ]);
            if self.opts.optimized || truthy(env_var("GHOSTEX_START_OPTIMIZED")) {
                self.build_env
                    .push(("GHOSTEX_START_OPTIMIZED".into(), "1".into()));
            }
            if self.log.verbose {
                self.build_env
                    .push(("GHOSTEX_GPUI_START_VERBOSE".into(), "1".into()));
                self.build_env
                    .push(("GHOSTEX_START_VERBOSE".into(), "1".into()));
            }
        }
        if self.isolated.is_some() {
            self.build_env
                .push(("GHOSTEX_GPUI_ISOLATED_START".into(), "1".into()));
        }
        if let Some(level) = &self.opts.debug_info {
            self.log.detail(&format!(
                "Building with debug info ({level}) and symbols for crash debugging."
            ));
            self.build_env.extend(util::debug_info_environment(level));
        }
        if let Some(runtime) = &windows_runtime {
            self.build_env
                .extend(runtime.build_environment(self.is_wsl, self.log.verbose));
            runtime.ensure_downloaded(&self.log)?;
        }
        if self.opts.prepare_only {
            self.log.finish_step();
            println!("Windows sources and build prerequisites are ready. Run cargo xtask start to build and launch Ghostex.");
            return Ok(0);
        }
        if !self.is_darwin && !self.targets_windows {
            let staged = self.app_path.clone();
            self.close_running_bundle(
                &staged,
                &format!("before rebuilding {}", staged.display()),
                false,
            )?;
        }
        let mut desktop_rust_build = None;
        if self.is_darwin {
            let mut rust_build =
                self.start_background_build("Desktop Rust build", "build-macos-rust.sh")?;
            let mut pages_build =
                self.start_background_build("React pages build", "build-macos-sidebar.sh")?;
            self.log.step(
                "Building GPUI runtime resources (Rust and React builds running alongside)...",
            );
            let mut prepare = self.build_command("/bin/bash");
            prepare.arg(gpui_dir.join("scripts").join("prepare-macos-runtime.sh"));
            self.log
                .run(&mut prepare, "GPUI runtime resource build", Summary::None)?;
            self.log.detail("GPUI runtime resources are ready.");
            let staged = self.app_path.clone();
            self.close_running_bundle(
                &staged,
                &format!("before replacing staged build bundle {}", staged.display()),
                false,
            )?;
            rust_build.finish(&self.log)?;
            pages_build.finish(&self.log)?;
            desktop_rust_build = Some(rust_build);
        }
        if !self.is_darwin && !self.targets_windows {
            // CDXC:PlatformSupport 2026-07-18:
            // gxserver and zmx are one protocol-coupled runtime. The Linux app packager previously reused whichever build/remote-gxserver-linux package happened to exist, so a freshly compiled gxserver could emit flags unsupported by the stale bundled zmx client. Rebuild the host-architecture package from the current source before staging every local GPUI build.
            self.log.step("Building local gxserver and zmx runtime...");
            let arm = cfg!(target_arch = "aarch64");
            let mut package = self.build_command("bun");
            package
                .arg(root().join("server").join("package-remote-linux.mjs"))
                .args([
                    "--arch",
                    if arm { "arm64" } else { "x64" },
                    "--rust-target",
                    if arm {
                        "aarch64-unknown-linux-gnu"
                    } else {
                        "x86_64-unknown-linux-gnu"
                    },
                    "--zig-target",
                    if arm {
                        "aarch64-linux-gnu"
                    } else {
                        "x86_64-linux-gnu"
                    },
                ]);
            self.log
                .run(&mut package, "Linux gxserver runtime build", Summary::None)?;
            self.log.detail("Linux gxserver and zmx runtime is ready.");
        }
        self.log
            .step("Building GPUI app resources and native shell...");
        let build_script = gpui_dir.join("scripts").join(if self.is_darwin {
            "build-macos-app.sh"
        } else if self.targets_windows {
            if self.is_wsl {
                "build-windows-app-wsl.sh"
            } else {
                "build-windows-app.ps1"
            }
        } else {
            "build-linux-app.sh"
        });
        let mut build = if self.is_windows {
            let mut command = self.build_command("powershell.exe");
            command
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&build_script);
            command
        } else {
            let mut command = self.build_command("/bin/bash");
            command.arg(&build_script);
            command
        };
        if desktop_rust_build.is_some() {
            build.env("GHOSTEX_GPUI_USE_PREBUILT_RUST", "1");
        }
        if self.targets_windows {
            // CDXC:Build 2026-10-02 WHY: the Windows script compiles every Rust binary in release with its output hidden, which reads as a hang for several minutes; say so, and where to watch it.
            self.log.detail(
                "Compiling the Windows release binaries; this can take several minutes. Live output: build/local-start-logs/ (or rerun with --verbose).",
            );
        }
        let build_started = std::time::SystemTime::now();
        self.log.run(
            &mut build,
            &format!("{} build", self.app_name),
            Summary::None,
        )?;
        self.log.detail("GPUI build completed.");
        if self.is_windows {
            self.sweep_stale_release_artifacts(build_started);
        }
        if !self.app_path.exists() {
            bail!("Built GPUI app is missing at {}.", self.app_path.display());
        }
        if self.opts.build_only {
            self.log.finish_step();
            println!(
                "Built and staged {}; nothing was installed or launched.",
                self.app_path.display()
            );
            return Ok(0);
        }
        self.install_and_launch()?;
        self.prune_stale_incremental_caches();
        self.log.finish_step();
        println!("{}", util::local_timestamp());
        Ok(0)
    }

    /// CDXC:Build 2026-10-02 WHY:
    /// `cargo xtask remote-start` builds on a rented Blacksmith machine and writes the result into the staged app here. Installing it must take the same path as a local start (close the running app, hand gxserver over, sync in place so live zmx sessions survive), so it reuses install_and_launch instead of copying files by hand.
    fn install_staged_build(&mut self) -> Res<i32> {
        if self.is_darwin || self.is_wsl {
            bail!("--install-only only installs Linux and native Windows builds for now.");
        }
        let staged_executable = if self.is_windows {
            self.app_path.join("Ghostex.exe")
        } else {
            self.app_path.join(&self.app_name)
        };
        if !staged_executable.exists() {
            bail!(
                "There is no staged build at {}. Run `cargo xtask start --build-only` (or `cargo xtask remote-start` on Linux) first.",
                self.app_path.display()
            );
        }
        self.log.step(&format!(
            "Installing the staged build from {}...",
            self.app_path.display()
        ));
        self.install_and_launch()?;
        self.log.finish_step();
        println!("{}", util::local_timestamp());
        Ok(0)
    }

    /// A command that runs from the repository root with the build environment.
    pub fn build_command(&self, program: &str) -> Command {
        let mut command = util::command(program);
        command.envs(self.build_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        command
    }

    /// CDXC:Build 2026-08-02:
    /// Zed, cef-rs, and gpui-component are pinned submodules under the repository's `.dependencies` tree. Initialize an absent checkout, but never replace a present incomplete directory because it may contain user or agent work.
    fn ensure_reference_checkouts(&self) -> Res {
        fs::create_dir_all(root().join(".dependencies"))?;
        if self.targets_windows {
            self.ensure_reference_checkout("code-server", "ci/build/build-code-server.sh")?;
            self.ensure_reference_checkout("wmx", "Cargo.toml")?;
        }
        self.ensure_reference_checkout("zed", "crates/gpui/Cargo.toml")?;
        self.ensure_reference_checkout("cef-rs", "cef/Cargo.toml")?;
        self.ensure_reference_checkout("gpui-component", "crates/component/Cargo.toml")
    }

    fn ensure_reference_checkout(&self, name: &str, required: &str) -> Res {
        let checkout = root().join(".dependencies").join(name);
        let required_path = checkout.join(required);
        if !required_path.exists() {
            if fs::symlink_metadata(&checkout).is_ok() && !submodule_is_uninitialized(name)? {
                bail!(
                    "GPUI dependency {} exists, but {} is missing. Refusing to overwrite it; fix or replace that submodule checkout manually.",
                    checkout.display(),
                    required_path.display()
                );
            }
            let mut init = util::command("git");
            init.arg("-c")
                .arg(format!("safe.directory={}", root().display()))
                .args(["submodule", "update", "--init", "--depth=1", "--"])
                .arg(format!(".dependencies/{name}"));
            self.log.run(
                &mut init,
                &format!("{name} dependency checkout"),
                Summary::None,
            )?;
            if !required_path.exists() {
                bail!(
                    "GPUI dependency {} is incomplete after submodule initialization.",
                    checkout.display()
                );
            }
        }
        self.report_local_dependency_revision(name, &checkout)
    }

    /// CDXC:Build 2026-09-22 WHY:
    /// Local starts must build dependency edits before their gitlinks are committed. Requiring the parent HEAD revision blocked valid local GPUI work; initialize missing submodules at the pin, but use an existing checkout as-is.
    fn report_local_dependency_revision(&self, name: &str, checkout: &Path) -> Res {
        let expected = git_output(
            root(),
            &["rev-parse", &format!("HEAD:.dependencies/{name}")],
        )?;
        let revision = git_output(checkout, &["rev-parse", "HEAD"])?;
        if revision != expected {
            self.log.detail(&format!(
                "Using local {name} revision {revision} (committed pin {expected})."
            ));
        }
        Ok(())
    }

    /// CDXC:Build 2026-09-05 WHY:
    /// Rust and the React pages have independent outputs, so both can build during runtime preparation. The packager checks the frontend fingerprint again after the handoff to catch edits made during compilation.
    fn start_background_build(&self, label: &str, script: &str) -> Res<BackgroundBuild> {
        let log_path = util::quiet_log_path(label);
        let mut command = self.build_command("/bin/bash");
        command.arg(
            root()
                .join("apps")
                .join("desktop")
                .join("scripts")
                .join(script),
        );
        util::redirect_into_log(&mut command, &log_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command
            .spawn()
            .map_err(|error| util::spawn_error(&command, error))?;
        Ok(BackgroundBuild {
            label: label.to_string(),
            child: Some(child),
            log_path,
            started: Instant::now(),
        })
    }

    /// CDXC:Build 2026-09-23 DECISION:
    /// User asked for rustc's incremental caches to be pruned automatically, keeping one cache per binary.
    /// Every build configuration (check, build, test, opt-level, target) gets its own cache directory and nothing ever removes the old ones: apps/desktop/target/debug/incremental had grown to 22GB across ~25 caches per binary.
    /// Each start keeps the newest cache per crate in every incremental folder of the desktop and gxserver targets, and leaves anything touched in the last 30 minutes alone so a build running in another session is never disturbed.
    /// The removal runs detached so the start does not wait for it.
    fn prune_stale_incremental_caches(&self) {
        if self.is_windows {
            return;
        }
        let recent_cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 60);
        let mut incremental_dirs = Vec::new();
        for target_root in [
            root().join("apps/desktop/target"),
            root().join("server/target"),
        ] {
            for first in read_dir_names(&target_root) {
                let first_path = target_root.join(&first);
                incremental_dirs.push(first_path.join("incremental"));
                for second in read_dir_names(&first_path) {
                    incremental_dirs.push(first_path.join(second).join("incremental"));
                }
            }
        }
        let mut stale = Vec::new();
        for incremental in incremental_dirs {
            let mut by_crate: std::collections::HashMap<
                String,
                Vec<(PathBuf, std::time::SystemTime)>,
            > = Default::default();
            for entry in read_dir_names(&incremental) {
                let entry_path = incremental.join(&entry);
                let Ok(metadata) = fs::metadata(&entry_path) else {
                    continue;
                };
                if !metadata.is_dir() {
                    continue;
                }
                let Ok(modified) = metadata.modified() else {
                    continue;
                };
                let crate_name = entry
                    .rsplit_once('-')
                    .map_or(entry.as_str(), |(name, _)| name)
                    .to_string();
                by_crate
                    .entry(crate_name)
                    .or_default()
                    .push((entry_path, modified));
            }
            for mut caches in by_crate.into_values() {
                caches.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
                stale.extend(
                    caches
                        .into_iter()
                        .skip(1)
                        .filter(|(_, modified)| *modified < recent_cutoff)
                        .map(|(p, _)| p),
                );
            }
        }
        if stale.is_empty() {
            return;
        }
        let mut remove = Command::new("/bin/rm");
        remove
            .arg("-rf")
            .args(&stale)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            remove.process_group(0);
        }
        if remove.spawn().is_ok() {
            self.log.detail(&format!(
                "Removing {} old incremental build cache{} in the background.",
                stale.len(),
                if stale.len() == 1 { "" } else { "s" }
            ));
        }
    }
}

/// A packaging script running while the start does other work; its process group is ended if the start fails first.
struct BackgroundBuild {
    label: String,
    child: Option<Child>,
    log_path: PathBuf,
    started: Instant,
}

impl BackgroundBuild {
    fn finish(&mut self, log: &Log) -> Res {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        if child.try_wait()?.is_none() {
            log.step(&format!("Waiting for {}...", self.label.to_lowercase()));
        }
        let status = child.wait()?;
        if log.verbose {
            print!("{}", fs::read_to_string(&self.log_path).unwrap_or_default());
        }
        if !status.success() {
            log.report_failure(&self.label, status.code().unwrap_or(1), &self.log_path);
            bail!(
                "{} failed with exit code {}.",
                self.label,
                status.code().unwrap_or(1)
            );
        }
        log.detail(&format!(
            "{} completed ({} alongside).",
            self.label,
            format_duration(self.started.elapsed())
        ));
        let _ = fs::remove_file(&self.log_path);
        Ok(())
    }
}

impl Drop for BackgroundBuild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            if matches!(child.try_wait(), Ok(None)) {
                #[cfg(unix)]
                util::kill_pid(-(child.id() as i32), false);
            }
        }
    }
}

fn read_dir_names(directory: &Path) -> Vec<String> {
    fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The isolated instance is selected before the arguments are validated, because the app name, the install directory and the gxserver port all derive from the variant.
fn resolve_isolated_variant(args: &[String]) -> Res<Option<String>> {
    let mut variant: Option<String> = None;
    for arg in args {
        if arg == "--" {
            continue;
        }
        if let Some(selected) = isolated::parse_argument(arg)? {
            if let Some(existing) = &variant {
                if *existing != selected {
                    bail!("Start one isolated variant at a time: --isolated={existing} or --isolated={selected}.");
                }
            }
            variant = Some(selected);
        }
    }
    Ok(variant)
}

fn parse_options(args: &[String], targets_windows: bool) -> Res<Options> {
    let mut opts = Options {
        verbose: false,
        profile: false,
        optimized: false,
        prepare_only: false,
        build_only: false,
        install_only: false,
        machine: false,
        debug_info: None,
    };
    let mut debug_info_flag = None;
    for arg in args {
        if arg == "--debug-info" {
            debug_info_flag = Some(String::new());
            continue;
        }
        if let Some(level) = arg.strip_prefix("--debug-info=") {
            debug_info_flag = Some(level.to_string());
            continue;
        }
        match arg.as_str() {
            "--" => {}
            "--profile" => opts.profile = true,
            "--prepare-only" if targets_windows => opts.prepare_only = true,
            // macOS only: build the app and gxserver crates with full release optimization (see build-macos-rust.sh).
            "--optimized" => opts.optimized = true,
            "--build-only" => opts.build_only = true,
            "--install-only" => opts.install_only = true,
            "--machine" if targets_windows => opts.machine = true,
            "--verbose" | "-v" => opts.verbose = true,
            other if isolated::parse_argument(other)?.is_some() => {}
            other => bail!("Unknown start argument: {other}. Usage: {USAGE}"),
        }
    }
    opts.debug_info = util::debug_info_level(debug_info_flag.as_deref())?;
    if opts.install_only && (opts.build_only || opts.prepare_only) {
        bail!("--install-only installs an existing staged build; it cannot be combined with --build-only or --prepare-only.");
    }
    Ok(opts)
}

fn configuration() -> String {
    env_trimmed("CONFIGURATION").unwrap_or_else(|| "Release".into())
}

fn macos_arch() -> Res<String> {
    if let Some(explicit) = env_trimmed("GHOSTEX_MACOS_ARCH") {
        return match explicit.as_str() {
            "arm64" | "aarch64" => Ok("arm64".into()),
            "x86_64" | "x64" | "amd64" => Ok("x86_64".into()),
            other => bail!("Unsupported GHOSTEX_MACOS_ARCH: {other}. Use arm64 or x86_64."),
        };
    }
    if util::stdout_if_ok(Command::new("/usr/sbin/sysctl").args(["-in", "hw.optional.arm64"]))
        .is_some_and(|out| out.trim() == "1")
    {
        return Ok("arm64".into());
    }
    let machine = output(Command::new("uname").arg("-m"))?;
    let machine = String::from_utf8_lossy(&machine.stdout).trim().to_string();
    Ok(if machine.is_empty() {
        "x86_64".into()
    } else {
        machine
    })
}

/// Linux under WSL, where the start builds and installs the Windows app instead.
pub fn runs_in_wsl() -> bool {
    cfg!(target_os = "linux")
        && (env_trimmed("WSL_DISTRO_NAME").is_some()
            || fs::read_to_string("/proc/sys/kernel/osrelease")
                .unwrap_or_default()
                .to_lowercase()
                .contains("microsoft"))
}

/// The installed app a plain (not isolated) native Linux start replaces and opens: the packaged copy (/opt/ghostex) unless INSTALL_DIR says otherwise, else XDG data.
pub fn linux_installed_app_path() -> PathBuf {
    env_trimmed("INSTALL_DIR")
        .is_none()
        .then(install::resolve_linux_packaged_app_path)
        .flatten()
        .unwrap_or_else(|| resolve_gpui_install_dir(false).join("Ghostex"))
}

fn resolve_gpui_install_dir(is_darwin: bool) -> PathBuf {
    if is_darwin {
        // Local macOS GPUI debugging has one canonical app identity and location. Do not inherit a generic INSTALL_DIR from a shell/toolchain and create a second LaunchServices-visible Ghostex.app beside /Applications/Ghostex.app.
        return PathBuf::from("/Applications");
    }
    if let Some(configured) = env_trimmed("INSTALL_DIR") {
        return PathBuf::from(configured);
    }
    env_trimmed("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local").join("share"))
}

/// CDXC:Build 2026-09-23 WHY:
/// Checked once per start, by the process that builds: it parses about 1,100 files (~0.7s), and it is a TypeScript-AST lint, so it stays JavaScript until the TypeScript it checks is gone.
fn check_client_storage() -> Res {
    let out = output(&mut bun(["tooling/client-storage/check.mjs"]))?;
    if !out.status.success() {
        bail!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

fn submodule_is_uninitialized(name: &str) -> Res<bool> {
    let out = output(
        util::command("git")
            .arg("-c")
            .arg(format!("safe.directory={}", root().display()))
            .args(["submodule", "status", "--"])
            .arg(format!(".dependencies/{name}")),
    )?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!(
            "{}",
            if stderr.is_empty() {
                format!("Unable to inspect GPUI dependency .dependencies/{name}.")
            } else {
                stderr
            }
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .trim_start()
        .starts_with('-'))
}

fn git_output(checkout: &Path, args: &[&str]) -> Res<String> {
    let out = output(
        Command::new("git")
            .arg("-c")
            .arg(format!("safe.directory={}", checkout.display()))
            .arg("-C")
            .arg(checkout)
            .args(args),
    )?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!(
            "{}",
            if stderr.is_empty() {
                format!("git {} failed for {}.", args.join(" "), checkout.display())
            } else {
                stderr
            }
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
