fn main() {
    if invoked_as_cli() {
        std::process::exit(gxserver::ghostex_cli::run());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = gxserver::cli::run_hook_command(&args) {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    // Before the runtime starts any thread: PATH is process-wide state. A run started by one of the
    // app's bundled launchd agents first gets the user's PATH and its launch log
    // (CDXC:BackgroundItems 2026-11-08 in packages/paths/src/macos_launch_agent.rs).
    #[cfg(target_os = "macos")]
    ghostex_paths::macos_launch_agent::prepare_bundled_agent_process();
    gxserver::prepare_process_environment();
    raise_open_file_limit();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("start the gxserver runtime");
    runtime.block_on(async {
        if let Err(error) = gxserver::cli::run(args).await {
            // With its causes ("read gxserver config: Permission denied"): the desktop app quotes this line when gxserver stops right after starting.
            eprintln!("{error:#}");
            std::process::exit(1);
        }
    });
    // Dropping the runtime waits for every blocking task without limit, and one stuck on a hung child or drive kept finished processes alive for hours.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
}

/// CDXC:Release 2026-10-23 DECISION:
/// The user chose to ship gxserver and the `ghostex` CLI as one program on every OS ("literally combine the 2 executables into 1 executable on all oses to trim duplicate stuff") to shrink the downloads: the CLI was a second copy of mostly the same code (SQLite, networking, the shared server modules). Measured 2026-10-23: the DMG shrinks by about 2 MB, the Linux server package by 4.2 MB and the Windows installer by about 3 MB, and every install by 6 to 8 MB. A CLI call starts about 0.35 ms slower on macOS and about 2 ms slower on Windows. `ghostex` and `gx` are links to gxserver, and the name the program was started with picks the CLI. The Windows installer package cannot hold links, so the desktop app makes `ghostex.exe` a hard link to gxserver.exe itself (apps/desktop/src/windows_cli_link.rs).
/// SEE-ALSO: `current_cli_executable` in server/src/ghostex_cli/launchers.rs, `gpui_is_package_cli_link_target` in apps/desktop/src/app/helpers/os_cli/cli_install.rs, build-macos-app.sh and prepare-macos-runtime.sh (macOS bundle), server/package-remote-linux.mjs (Linux package), tooling/xtask/src/start_server.rs, apps/desktop/src/windows_cli_link.rs and tooling/install-windows-gpui.ps1 (Windows).
#[cfg(unix)]
fn invoked_as_cli() -> bool {
    std::env::args_os()
        .next()
        .as_deref()
        .map(std::path::Path::new)
        .and_then(std::path::Path::file_name)
        .is_some_and(|name| name == "ghostex" || name == "gx")
}

/// Windows file names ignore case and argv[0] may leave out `.exe`, so the CLI is told apart by the stem of the running executable's path, which keeps the name it was started under.
#[cfg(windows)]
fn invoked_as_cli() -> bool {
    std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(std::path::Path::file_stem)
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|stem| stem.eq_ignore_ascii_case("ghostex") || stem.eq_ignore_ascii_case("gx"))
}

/// CDXC:ServerDaemon 2026-11-05 WHY:
/// launchd starts gxserver with a soft limit of 256 open files. A daemon that holds every client's
/// connections, its database, transcript reads and the pipes of every process it starts ran out of
/// them on 2026-11-05 (rebuild 111, a folder watch that held descriptors): from then on no git, zmx
/// or `security` could start and the clients' requests piled up. The soft limit is raised to the
/// hard limit, at most 8,192, so a leak shows as slow growth instead of every process start failing.
#[cfg(unix)]
fn raise_open_file_limit() {
    const WANTED: libc::rlim_t = 8192;
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: getrlimit and setrlimit only read and write `limit`.
    unsafe {
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 || limit.rlim_cur >= WANTED {
            return;
        }
        limit.rlim_cur = WANTED.min(limit.rlim_max);
        let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &limit);
    }
}

#[cfg(not(unix))]
fn raise_open_file_limit() {}
