//! The thin commands: each runs the JavaScript tool, shell script or cargo binary that still owns the work.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::bail;
use crate::util::{self, bun, bun_x, check, command, output, root, status_code, Res};

/// Runs each step in order and stops at the first that fails, returning its exit code.
fn steps(steps: Vec<Command>) -> Res<i32> {
    for mut step in steps {
        let code = status_code(&mut step)?;
        if code != 0 {
            return Ok(code);
        }
    }
    Ok(0)
}

fn no_arguments(name: &str, args: &[String]) -> Res {
    if let Some(arg) = args.first() {
        bail!("`cargo xtask {name}` takes no arguments (got {arg}).");
    }
    Ok(())
}

pub fn storage_check_command() -> Command {
    bun(["tooling/client-storage/check.mjs"])
}

/// The root gate: the storage lint, the generated Help files, the Annotate bundle's pinned commit, the root tsconfig, the release scripts and the formatting of
/// app-owned files. The format check runs in the background alongside the others and reports last.
pub fn typecheck(args: &[String]) -> Res<i32> {
    no_arguments("typecheck", args)?;
    let format = crate::format::start_check()?;
    let help = crate::help::check()?;
    if help != 0 {
        return Ok(help);
    }
    crate::agentation::check_pinned()?;
    let code = steps(vec![
        storage_check_command(),
        bun_x("tsc", ["--noEmit", "--pretty", "false"]),
        bun(["run", "release:typecheck"]),
    ])?;
    if code != 0 {
        return Ok(code);
    }
    format.finish()
}

/// `apps/desktop/tsconfig.json`: the CEF entry modules in apps/desktop/sidebar/ and the Files embed page.
pub fn desktop_typecheck(args: &[String]) -> Res<i32> {
    no_arguments("desktop-typecheck", args)?;
    steps(vec![
        storage_check_command(),
        bun_x("tsc", ["-p", "apps/desktop/tsconfig.json", "--noEmit"]),
    ])
}

pub fn storage_check(args: &[String]) -> Res<i32> {
    no_arguments("storage-check", args)?;
    status_code(&mut storage_check_command())
}

pub fn help_generate(args: &[String]) -> Res<i32> {
    no_arguments("help-generate", args)?;
    crate::help::generate()
}

pub fn help_check(args: &[String]) -> Res<i32> {
    no_arguments("help-check", args)?;
    crate::help::check()
}

/// vitest over the repository; extra arguments go to vitest (a file filter, `--watch`, ...).
pub fn test(args: &[String]) -> Res<i32> {
    let mut vitest = bun_x("vitest", ["run"]);
    vitest.args(args);
    status_code(&mut vitest)
}

/// macOS: build and sign the app bundle without installing or launching it.
pub fn build(args: &[String]) -> Res<i32> {
    no_arguments("build", args)?;
    if !cfg!(target_os = "macos") {
        bail!("`cargo xtask build` packages the macOS app; on Linux and Windows `cargo xtask start` builds and installs it.");
    }
    let scripts = root().join("apps/desktop/scripts");
    let mut prepare = command("/bin/bash");
    prepare.arg(scripts.join("prepare-macos-runtime.sh"));
    let mut package = command("/bin/bash");
    package.arg(scripts.join("build-macos-app.sh"));
    steps(vec![prepare, package])
}

pub fn build_editor(args: &[String]) -> Res<i32> {
    no_arguments("build-editor", args)?;
    let mut app = command("bash");
    app.arg("apps/editor/scripts/build-editor-app.sh");
    steps(vec![bun(["apps/editor/scripts/build-editor-web.mjs"]), app])
}

pub fn build_sidebar_css(args: &[String]) -> Res<i32> {
    no_arguments("build-sidebar-css", args)?;
    status_code(&mut bun_x(
        "tailwindcss",
        [
            "-i",
            "packages/core-ui/styles/shadcn.css",
            "-o",
            "packages/core-ui/styles/shadcn.generated.css",
            "--minify",
        ],
    ))
}

/// The phone app's generators live in the apps/mobile/app submodule and run with Node.
pub fn mobile_script(name: &str, script: &str, args: &[String]) -> Res<i32> {
    no_arguments(name, args)?;
    let mut node = command("node");
    node.arg(format!("apps/mobile/app/scripts/{script}"));
    status_code(&mut node)
}

/// Packages gxserver for remote Linux hosts with server/package-remote-linux.mjs (`--arch x64|arm64|all`, default all; other arguments such as `--allow-cross` pass through). `--release` runs the release packager instead.
pub fn gxserver_remote_linux(args: &[String]) -> Res<i32> {
    if args.first().map(String::as_str) == Some("--release") {
        let mut release = command(root().join("tooling/build-remote-gxserver-linux-release.sh"));
        release.args(&args[1..]);
        return status_code(&mut release);
    }
    let mut node = command("node");
    node.arg("server/package-remote-linux.mjs");
    if !args.iter().any(|arg| arg == "--arch") {
        node.args(["--arch", "all"]);
    }
    node.args(args);
    status_code(&mut node)
}

/// `cargo xtask history [args]` runs the ghostex-history CLI.
pub fn history(args: &[String]) -> Res<i32> {
    let mut cargo = command("cargo");
    cargo
        .args([
            "run",
            "--quiet",
            "--manifest-path",
            "apps/history-cli/Cargo.toml",
            "--",
        ])
        .args(args);
    status_code(&mut cargo)
}

/// `cargo xtask cli [args]` runs the checkout's `ghostex` CLI (built from server/, so its toolchain pin applies).
pub fn cli(args: &[String]) -> Res<i32> {
    let mut cargo = command("cargo");
    cargo
        .current_dir(root().join("server"))
        .args(["run", "--quiet", "--bin", "ghostex", "--"])
        .args(args);
    status_code(&mut cargo)
}

/// Windows: install the build prerequisites, then prepare sources without building (`cargo xtask start --prepare-only`).
pub fn setup_windows(args: &[String]) -> Res<i32> {
    no_arguments("setup-windows", args)?;
    if !cfg!(windows) {
        bail!("Run setup-windows from a native Windows shell.");
    }
    util::remove_powershell7_module_paths();
    let mut install = command("powershell.exe");
    install
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(root().join("tooling/prepare-windows-build.ps1"))
        .arg("-Install");
    check(&mut install)?;
    crate::start::run(&["--prepare-only".to_string()])
}

/// `--debug` builds the wasm without optimization (works, but about 100 MB and slow).
pub fn web_build(args: &[String]) -> Res<i32> {
    let release = web_wasm_mode("web-build", args)?;
    build_wasm(release)?;
    status_code(&mut bun_x(
        "vite",
        ["build", "--config", "apps/gpui-web/www/vite.config.js"],
    ))
}

/// The GPUI web crate's tests, built for this computer rather than wasm32 (the shared desktop files pick their web branches by `cfg(ghostex_web)`, apps/gpui-web/build.rs); extra arguments go to `cargo test`.
pub fn web_test(args: &[String]) -> Res<i32> {
    let mut test = command("cargo");
    test.current_dir(root().join("apps/gpui-web"))
        .args(["test", "--lib"])
        .args(args);
    status_code(&mut test)
}

pub fn web_dev(args: &[String]) -> Res<i32> {
    let release = web_wasm_mode("web-dev", args)?;
    build_wasm(release)?;
    status_code(&mut bun_x(
        "vite",
        ["--config", "apps/gpui-web/www/vite.config.js"],
    ))
}

/// Builds the web app and serves it with `ghostex web`; extra arguments go to `ghostex web`.
pub fn start_web(args: &[String]) -> Res<i32> {
    build_wasm(true)?;
    let code = status_code(&mut bun_x(
        "vite",
        ["build", "--config", "apps/gpui-web/www/vite.config.js"],
    ))?;
    if code != 0 {
        return Ok(code);
    }
    let mut serve = command("cargo");
    serve
        .current_dir(root().join("server"))
        .args([
            "run",
            "--quiet",
            "--bin",
            "ghostex",
            "--",
            "web",
            "--dist-dir",
            "../apps/gpui-web/www/dist",
        ])
        .args(args);
    status_code(&mut serve)
}

fn web_wasm_mode(name: &str, args: &[String]) -> Res<bool> {
    match args {
        [] => Ok(true),
        [flag] if flag == "--debug" => Ok(false),
        _ => bail!("Usage: cargo xtask {name} [--debug]"),
    }
}

/// Compiles apps/gpui-web to wasm (building libghostty-vt for wasm first when missing) and runs wasm-bindgen into www/src/wasm.
fn build_wasm(release: bool) -> Res {
    let web = root().join("apps/gpui-web");
    if cfg!(windows) {
        restore_windows_shared_source_links()?;
    }
    let prefix = web.join("target/libghostty-vt-wasm");
    if !prefix.join("lib/libghostty-vt.a").exists() {
        let ghostty = root().join(".dependencies/ghostty");
        let zon = fs::read_to_string(ghostty.join("build.zig.zon"))?;
        let Some(version) = zig_zon_version(&zon) else {
            bail!("No .version in {}", ghostty.join("build.zig.zon").display())
        };
        let zig = util::find_zig()?;
        check(
            Command::new(zig)
                .current_dir(&ghostty)
                .arg("build")
                .arg(format!("-Dversion-string={version}"))
                .args([
                    "-Demit-lib-vt=true",
                    "-Demit-lib-vt-shared=false",
                    "-Demit-xcframework=false",
                    "-Doptimize=ReleaseSmall",
                    "-Dtarget=wasm32-freestanding",
                    "--prefix",
                ])
                .arg(&prefix),
        )?;
    }
    let mut cargo = Command::new("cargo");
    cargo
        .current_dir(&web)
        .args(["build", "--target", "wasm32-unknown-unknown"]);
    if release {
        cargo.arg("--release");
    }
    // Cargo can use response files for web-sys's long feature list; the rustc wrapper's Windows process invocation (measured with sccache; kept off for kache) exceeds the command-line limit.
    if cfg!(windows) {
        cargo.env("RUSTC_WRAPPER", "");
    }
    check(&mut cargo)?;
    check(
        Command::new("wasm-bindgen")
            .current_dir(&web)
            .arg(format!(
                "target/wasm32-unknown-unknown/{}/ghostex_gpui_web.wasm",
                if release { "release" } else { "debug" }
            ))
            .args([
                "--out-dir",
                "www/src/wasm",
                "--target",
                "web",
                "--no-typescript",
            ]),
    )
}

fn zig_zon_version(zon: &str) -> Option<String> {
    let index = zon.find(".version")?;
    let rest = zon[index + ".version".len()..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start()
        .strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

/// CDXC:WebGpui 2026-09-25 WHY: Windows checkouts with core.symlinks=false contain path text where Rust needs the shared desktop sources. Restore actual links so the browser still compiles the same files as desktop.
fn restore_windows_shared_source_links() -> Res {
    let listing = output(command("git").args(["ls-files", "-s", "-z", "apps/gpui-web"]))?;
    if !listing.status.success() {
        bail!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&listing.stderr).trim()
        );
    }
    for entry in String::from_utf8_lossy(&listing.stdout).split('\0') {
        if !entry.starts_with("120000 ") {
            continue;
        }
        let Some((_, relative)) = entry.split_once('\t') else {
            continue;
        };
        let file = root().join(relative);
        /*
        CDXC:WebGpui 2026-10-07 WHY: Windows does not resolve a relative symlink whose target is written with forward slashes, so the links this step made from git's `../../…` text existed but could not be read, and the web build failed with "file not found for module". Targets are written with backslashes, and a link an earlier run wrote with forward slashes is made again.
        */
        let existing_link = fs::symlink_metadata(&file)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        let contents = if existing_link {
            let current = fs::read_link(&file)?.to_string_lossy().into_owned();
            if !current.contains('/') {
                continue;
            }
            current
        } else {
            fs::read_to_string(&file)?
        };
        let target = contents.trim();
        let resolved = file
            .parent()
            .expect("tracked file has a parent")
            .join(target);
        let inside = std::path::absolute(&resolved)
            .map(|p| p.starts_with(root()))
            .unwrap_or(false);
        if Path::new(target).is_absolute() || !inside || !resolved.exists() {
            bail!(
                "Cannot restore shared-source link {}: {target}",
                file.display()
            );
        }
        if existing_link && resolved.is_dir() {
            fs::remove_dir(&file)?;
        } else {
            fs::remove_file(&file)?;
        }
        #[cfg(windows)]
        {
            let native_target = target.replace('/', "\\");
            let linked = if resolved.is_dir() {
                std::os::windows::fs::symlink_dir(&native_target, &file)
            } else {
                std::os::windows::fs::symlink_file(&native_target, &file)
            };
            if let Err(error) = linked {
                fs::write(&file, &contents)?;
                bail!("Enable Windows Developer Mode or run with symlink privileges to build GPUI web: {error}");
            }
        }
    }
    Ok(())
}
