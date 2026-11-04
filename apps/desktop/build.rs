use std::{
    env,
    fmt::Write as _,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

const GHOSTTYKIT_HEADER: &str =
    "../../.dependencies/ghostty/macos/GhosttyKit.xcframework/macos-arm64_x86_64/Headers/ghostty.h";
const GHOSTTYKIT_ARCHIVE: &str = "../../.dependencies/ghostty/macos/GhosttyKit.xcframework/macos-arm64_x86_64/ghostty-internal.a";
const GPUI_MACOS_DEPLOYMENT_TARGET_FLAG: &str = "-mmacosx-version-min=13.0";
const LIBGHOSTTY_VT_BUILD_SCRIPT: &str = "scripts/build-libghostty-vt.sh";
const WINDOWS_APP_ICON_SOURCE: &str = "resources/AppIcon.appiconset/icon_512x512.png";
const WINDOWS_APP_ICON_SIZES: [u32; 5] = [16, 32, 64, 128, 256];

struct LibGhosttyVtBuild {
    archive: PathBuf,
    themes_dir: PathBuf,
}

/// CDXC:Release 2026-10-22 WHY:
/// The Linux CEF component ships with its debug info stripped (apps/desktop/scripts/build-linux-app.sh), which changes its archive digest. Component tags are immutable and every shipped Linux app verifies the unstripped archive's digest, so the stripped archive gets its own identity: the CEF version plus this suffix, on Linux only (macOS and Windows keep the unsuffixed tag, so their installed runtimes stay valid). The suffix can be dropped with the next CEF update, whose tag has never held an unstripped Linux archive.
/// SEE-ALSO: apps/desktop/scripts/build-linux-app.sh (LINUX_CEF_COMPONENT_SUFFIX).
const LINUX_CEF_COMPONENT_SUFFIX: &str = "-stripped";

fn emit_cef_component_version() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let target = env::var("TARGET").expect("TARGET").replace('-', "_");
    let bindings = manifest_dir
        .join("../../.dependencies/cef-rs/sys/src/bindings")
        .join(format!("{target}.rs"));
    let source = fs::read_to_string(&bindings).unwrap_or_else(|error| {
        panic!(
            "failed to read the pinned CEF bindings {}: {error}",
            bindings.display()
        )
    });
    let raw_version = source
        .lines()
        .find_map(|line| {
            line.strip_prefix("pub const CEF_VERSION:")
                .and_then(|value| value.split_once("= b\"").map(|(_, value)| value))
                .and_then(|value| value.strip_suffix("\\0\";"))
        })
        .expect("pinned CEF bindings do not define CEF_VERSION");
    let mut component_version: String = raw_version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        component_version.push_str(LINUX_CEF_COMPONENT_SUFFIX);
    }
    println!("cargo:rerun-if-changed={}", bindings.display());
    println!("cargo:rustc-env=GHOSTEX_CEF_COMPONENT_VERSION={component_version}");
    // The exact string libcef reports (cef_version_full), checked against a libcef.so found
    // outside the component store (src/cef_library_version.rs).
    println!("cargo:rustc-env=GHOSTEX_CEF_VERSION={raw_version}");
}

/*
CDXC:Terminal 2026-07-03:
Phase 1 GPUI-composited terminals parse VT bytes through libghostty-vt, so
cargo builds must produce that static archive from the vendored ghostty tree
instead of assuming a manually built artifact. The build script owns Zig
version selection and macOS SDK redirection; this function only runs it with
an OUT_DIR install prefix and returns the archive path for direct link-arg
linking, the same mechanism used for the GhosttyKit archive below.
*/
fn build_libghostty_vt(manifest_dir: &Path) -> LibGhosttyVtBuild {
    let script = manifest_dir.join(LIBGHOSTTY_VT_BUILD_SCRIPT);
    let ghostty_dir = manifest_dir.join("../../.dependencies/ghostty");
    println!("cargo:rerun-if-changed={}", script.display());
    emit_libghostty_vt_rerun_hints(&ghostty_dir);

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let prefix = out_dir.join("libghostty-vt");
    let status = Command::new(&script)
        .arg(&prefix)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", script.display()));
    assert!(
        status.success(),
        "{} failed with {status}",
        script.display()
    );

    let archive = prefix.join("lib/libghostty-vt.a");
    assert!(
        archive.is_file(),
        "libghostty-vt build did not produce {}",
        archive.display()
    );
    LibGhosttyVtBuild {
        archive,
        themes_dir: prefix.join("share/ghostty/themes"),
    }
}

fn emit_libghostty_vt_rerun_hints(ghostty_dir: &Path) {
    println!(
        "cargo:rerun-if-changed={}",
        ghostty_dir.join("build.zig").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        ghostty_dir.join("build.zig.zon").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        ghostty_dir.join("src").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        ghostty_dir.join("include").display()
    );
    println!("cargo:rerun-if-env-changed=GHOSTEX_ZIG");
    println!("cargo:rerun-if-env-changed=ZIG");
    println!("cargo:rerun-if-env-changed=MISE_DATA_DIR");
}

/// CDXC:Build 2026-10-26 WHY: Zig is resolved here exactly as in tooling/zig-toolchain.sh (which holds the full rationale), so a plain build finds the required Zig when PATH `zig` is another version and never builds with the wrong one. The series is the vendored Ghostty's `.minimum_zig_version`; GHOSTEX_ZIG, then ZIG, must be that series when set; then PATH `zig`, Homebrew, /usr/local, mise installs, ~/apps tarballs and build/toolchains are searched; no match fails with an install hint.
/// SEE-ALSO: tooling/zig-toolchain.sh, tooling/xtask/src/util.rs (find_zig), server/package-remote-linux.mjs (resolveZigBinary).
fn find_zig(repo_root: &Path) -> String {
    let zon_path = repo_root.join(".dependencies/ghostty/build.zig.zon");
    let zon = std::fs::read_to_string(&zon_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", zon_path.display()));
    let series = zon
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix(".minimum_zig_version")?;
            let (_, value) = rest.split_once('"')?;
            let mut parts = value.split('.');
            Some(format!("{}.{}", parts.next()?, parts.next()?))
        })
        .unwrap_or_else(|| panic!("no .minimum_zig_version in {}", zon_path.display()));
    let version_of = |candidate: &str| -> Option<String> {
        let output = Command::new(candidate).arg("version").output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let wanted = format!("{series}.");
    for name in ["GHOSTEX_ZIG", "ZIG"] {
        let Some(value) = env::var(name).ok().filter(|value| !value.is_empty()) else {
            continue;
        };
        match version_of(&value) {
            Some(version) if version.starts_with(&wanted) => return value,
            version => panic!(
                "{name}={value} is not a Zig {series}.x binary (it reports: {}). Point {name} at Zig {series}.x, or unset it so the build finds one.",
                version.as_deref().unwrap_or("unavailable")
            ),
        }
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    let mise_zig = env::var_os("MISE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share/mise"))
        .join("installs/zig");
    let globbed = |dir: &Path, keep: &dyn Fn(&str) -> bool, suffix: &[&str]| -> Vec<String> {
        let mut entries: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| keep(&entry.file_name().to_string_lossy()))
            .map(|entry| {
                suffix
                    .iter()
                    .fold(entry.path(), |path, part| path.join(part))
                    .display()
                    .to_string()
            })
            .collect();
        entries.sort();
        entries
    };
    let in_series = |name: &str| name.starts_with(&wanted);
    let tarball = |name: &str| name.starts_with("zig-") && name.contains(&format!("-{wanted}"));
    let toolchain = |name: &str| name.starts_with(&format!("zig-{wanted}"));
    let mut candidates = vec![
        "zig".to_string(),
        format!("/opt/homebrew/opt/zig@{series}/bin/zig"),
        "/opt/homebrew/bin/zig".to_string(),
        format!("/usr/local/opt/zig@{series}/bin/zig"),
        "/usr/local/bin/zig".to_string(),
    ];
    candidates.extend(globbed(&mise_zig, &in_series, &["bin", "zig"]));
    candidates.extend(globbed(&mise_zig, &in_series, &["zig"]));
    candidates.extend(globbed(&home.join("apps"), &tarball, &["zig"]));
    candidates.extend(globbed(
        &repo_root.join("build/toolchains"),
        &toolchain,
        &["zig"],
    ));
    candidates.extend(globbed(
        &repo_root.join("build/toolchains"),
        &toolchain,
        &["zig.exe"],
    ));
    for candidate in candidates {
        if version_of(&candidate).is_some_and(|version| version.starts_with(&wanted)) {
            return candidate;
        }
    }
    panic!(
        "Zig {series}.x is required (the vendored Ghostty's minimum_zig_version), and none was found on PATH, in Homebrew, mise, ~/apps or build/toolchains. Install it with `mise install zig@{series}.0` (or Homebrew's zig when it is {series}), or set GHOSTEX_ZIG to a Zig {series}.x binary."
    );
}

/*
CDXC:Release 2026-10-19 DECISION: "Yes, cache its output": the Windows release jobs reuse the libghostty-vt library between releases instead of building it with Zig every time (6.8 minutes of the ARM64 critical path in run 39305668478, where the x64 Zig runs under emulation because native ARM64 Zig 0.16.0 crashes). The workflow restores GHOSTEX_LIBGHOSTTY_VT_CACHE_DIR by the exact key it passes as GHOSTEX_LIBGHOSTTY_VT_CACHE_KEY, which covers every input of the build: the Ghostty gitlink (its sources, build files, packages and patches), this file (target, optimize and cpu flags), the Zig pin and the runner image (the MSVC and Windows SDK headers Zig compiles against). A folder stamped with another key, or a stamp without the library, fails the build instead of being used; an empty folder builds as before and is filled for the next release. Unset, nothing changes.
*/
fn cached_libghostty_vt(manifest_dir: &Path, archive_relative_path: &str) -> LibGhosttyVtBuild {
    println!("cargo:rerun-if-env-changed=GHOSTEX_LIBGHOSTTY_VT_CACHE_DIR");
    println!("cargo:rerun-if-env-changed=GHOSTEX_LIBGHOSTTY_VT_CACHE_KEY");
    let setting = |name: &str| env::var(name).ok().filter(|value| !value.is_empty());
    let (cache_dir, key) = match (
        setting("GHOSTEX_LIBGHOSTTY_VT_CACHE_DIR"),
        setting("GHOSTEX_LIBGHOSTTY_VT_CACHE_KEY"),
    ) {
        (Some(cache_dir), Some(key)) => (PathBuf::from(cache_dir), key),
        (None, None) => return build_libghostty_vt_with_zig(manifest_dir, archive_relative_path),
        _ => panic!(
            "GHOSTEX_LIBGHOSTTY_VT_CACHE_DIR and GHOSTEX_LIBGHOSTTY_VT_CACHE_KEY must be set together"
        ),
    };
    let stamp = cache_dir.join("cache-key");
    let cached_archive = cache_dir.join(archive_relative_path);
    let cached_themes = cache_dir.join("share/ghostty/themes");
    let out_archive = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"))
        .join("libghostty-vt")
        .join(archive_relative_path);
    if stamp.exists() {
        emit_libghostty_vt_rerun_hints(&manifest_dir.join("../../.dependencies/ghostty"));
        let stored = fs::read_to_string(&stamp)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", stamp.display()));
        assert!(
            stored.trim() == key,
            "{} holds libghostty-vt for key {}, not {key}",
            cache_dir.display(),
            stored.trim()
        );
        assert!(
            cached_archive.is_file() && cached_themes.is_dir(),
            "{} is stamped {key} but lacks {} or share/ghostty/themes",
            cache_dir.display(),
            archive_relative_path
        );
        fs::create_dir_all(out_archive.parent().expect("archive parent"))
            .expect("failed to create Cargo libghostty-vt output directory");
        fs::copy(&cached_archive, &out_archive)
            .unwrap_or_else(|error| panic!("failed to copy {}: {error}", cached_archive.display()));
        println!("cargo:warning=libghostty-vt cache hit ({key}); Zig build skipped");
        return LibGhosttyVtBuild {
            archive: out_archive,
            themes_dir: cached_themes,
        };
    }
    if cache_dir.exists() {
        // Only an empty folder may stand in for a miss.
        fs::remove_dir(&cache_dir).unwrap_or_else(|error| {
            panic!(
                "{} has no cache-key stamp but is not empty ({error}); refusing a partial libghostty-vt cache",
                cache_dir.display()
            )
        });
    }
    let build = build_libghostty_vt_with_zig(manifest_dir, archive_relative_path);
    // Filled beside the cache folder and renamed into place, so a build the
    // stdout watchdog kills mid-copy never leaves a folder that looks complete.
    let staging = cache_dir.with_extension("partial");
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .unwrap_or_else(|error| panic!("failed to remove {}: {error}", staging.display()));
    }
    let staged_archive = staging.join(archive_relative_path);
    fs::create_dir_all(staged_archive.parent().expect("archive parent"))
        .expect("failed to create the libghostty-vt cache directory");
    fs::copy(&build.archive, &staged_archive)
        .unwrap_or_else(|error| panic!("failed to copy {}: {error}", build.archive.display()));
    copy_dir_files(&build.themes_dir, &staging.join("share/ghostty/themes"));
    fs::write(staging.join("cache-key"), &key)
        .unwrap_or_else(|error| panic!("failed to write the libghostty-vt cache key: {error}"));
    fs::create_dir_all(cache_dir.parent().expect("cache parent"))
        .expect("failed to create the libghostty-vt cache parent directory");
    fs::rename(&staging, &cache_dir)
        .unwrap_or_else(|error| panic!("failed to move {} into place: {error}", staging.display()));
    println!("cargo:warning=libghostty-vt cache miss ({key}); built with Zig and stored");
    build
}

fn copy_dir_files(source: &Path, destination: &Path) {
    fs::create_dir_all(destination)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", destination.display()));
    for entry in fs::read_dir(source)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()))
    {
        let path = entry.expect("directory entry").path();
        let target = destination.join(path.file_name().expect("file name"));
        if path.is_dir() {
            copy_dir_files(&path, &target);
        } else {
            fs::copy(&path, &target)
                .unwrap_or_else(|error| panic!("failed to copy {}: {error}", path.display()));
        }
    }
}

/*
CDXC:PlatformSupport 2026-07-04:
Windows and Linux builds need only libghostty-vt (the GPUI terminal engine);
the GhosttyKit archive, ObjC shims, and Apple frameworks are macOS-only by
design. Zig cross-compiles natively and the vendored ghostty build already
emits the static lib under both names (`lib/ghostty-vt-static.lib` on
Windows, avoiding the DLL import-lib collision; `lib/libghostty-vt.a`
elsewhere — see .dependencies/ghostty/build.zig), so this hook invokes zig directly
instead of the macOS bash script (which exists only to pick a Zig 0.17.x
binary and redirect the Xcode SDK, both meaningless off macOS).
Zig resolution: `find_zig`, the same order as every other build path.
NEEDS-DEVICE-VERIFY: written from code-reading on macOS, never executed on
Windows or Linux hardware.
*/
fn build_libghostty_vt_with_zig(
    manifest_dir: &Path,
    archive_relative_path: &str,
) -> LibGhosttyVtBuild {
    let ghostty_dir = manifest_dir.join("../../.dependencies/ghostty");
    emit_libghostty_vt_rerun_hints(&ghostty_dir);

    let zig = find_zig(&manifest_dir.join("../.."));
    let version = ghostty_app_version(&ghostty_dir);
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let prefix = out_dir.join("libghostty-vt");
    let is_windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let cargo_target = env::var("TARGET").expect("TARGET");
    let (build_prefix, prefix_arg) = if is_windows {
        let relative = PathBuf::from("zig-out").join(format!("ghostex-gpui-vt-{cargo_target}"));
        (ghostty_dir.join(&relative), relative)
    } else {
        (prefix.clone(), prefix.clone())
    };
    let optimize = if is_windows && cargo_target == "aarch64-pc-windows-msvc" {
        // Zig's ReleaseSafe Windows ARM64 stack-trace implementation fails
        // to compile in std.debug.SelfInfo.Windows due to an invalid
        // pointer-alignment cast (0.16, and still in 0.17.0). Production
        // ARM64 archives do not need that debug-only path.
        "ReleaseFast"
    } else {
        "ReleaseSafe"
    };
    let mut command = Command::new(&zig);
    command.current_dir(&ghostty_dir).arg("build");
    if cfg!(windows) {
        command.env("PATH", zig_build_path_without_untrusted_mount_points());
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        let zig_target = match cargo_target.as_str() {
            "x86_64-unknown-linux-gnu" => "x86_64-linux-gnu",
            "aarch64-unknown-linux-gnu" => "aarch64-linux-gnu",
            target => panic!("unsupported Linux libghostty-vt target: {target}"),
        };
        // CDXC:Build 2026-10-04 WHY: #190: Zig's native CPU default can put
        // unconditional AVX-512 in compiler_rt memcpy on release builders.
        command.arg(format!("-Dtarget={zig_target}"));
        command.arg("-Dcpu=baseline");
    }
    if is_windows {
        // Release runners use Zig's stable x64 Windows host binary, including
        // under Windows 11 ARM emulation. Keep the archive architecture tied
        // explicitly to Cargo instead of allowing Zig to inherit its host.
        let zig_target = match cargo_target.as_str() {
            "aarch64-pc-windows-msvc" => "aarch64-windows-msvc",
            "x86_64-pc-windows-msvc" => "x86_64-windows-msvc",
            target => panic!("unsupported Windows libghostty-vt target: {target}"),
        };
        command.arg(format!("-Dtarget={zig_target}"));
    }
    let status = command
        .arg(format!("-Dversion-string={version}"))
        .arg("-Demit-lib-vt=true")
        .arg("-Demit-xcframework=false")
        .arg(format!("-Doptimize={optimize}"))
        .arg("--prefix")
        .arg(&prefix_arg)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {zig} build: {error}"));
    assert!(status.success(), "{zig} build failed with {status}");

    let built_archive = build_prefix.join(archive_relative_path);
    assert!(
        built_archive.is_file(),
        "libghostty-vt build did not produce {}",
        built_archive.display()
    );
    if is_windows {
        let status = Command::new(&zig)
            .args(["ar", "d"])
            .arg(&built_archive)
            .arg("compiler_rt.obj")
            .status()
            .unwrap_or_else(|error| {
                panic!(
                    "failed to strip compiler_rt.obj from {}: {error}",
                    built_archive.display()
                )
            });
        assert!(
            status.success(),
            "failed to strip compiler_rt.obj from {}: {status}",
            built_archive.display()
        );
        let archive = prefix.join(archive_relative_path);
        std::fs::create_dir_all(archive.parent().expect("archive parent"))
            .expect("failed to create Cargo libghostty-vt output directory");
        std::fs::copy(&built_archive, &archive).unwrap_or_else(|error| {
            panic!(
                "failed to copy {} to {}: {error}",
                built_archive.display(),
                archive.display()
            )
        });
        return LibGhosttyVtBuild {
            archive,
            themes_dir: build_prefix.join("share/ghostty/themes"),
        };
    }
    LibGhosttyVtBuild {
        archive: built_archive,
        themes_dir: build_prefix.join("share/ghostty/themes"),
    }
}

/// CDXC:PlatformSupport 2026-10-03 WHY: A Windows process running with the RedirectionGuard mitigation (EnforceRedirectionTrust, inherited by every child, including agent sessions started under a gxserver that has it) may not traverse junctions a non-admin created, such as the Codex and cua-driver `bin` folders on PATH. Zig 0.16 opens every PATH folder while looking up `pkg-config`, maps the resulting STATUS_UNTRUSTED_MOUNT_POINT to `error.Unexpected` and aborts the whole libghostty-vt build (`translate-c wuffs_c.h … error: Unexpected`, `NTSTATUS=0xc00004bc`) instead of skipping the folder. Those folders are unusable to this process tree anyway, so zig gets PATH without exactly them and Cargo prints which ones were dropped.
fn zig_build_path_without_untrusted_mount_points() -> std::ffi::OsString {
    const ERROR_UNTRUSTED_MOUNT_POINT: i32 = 448;
    let path = env::var_os("PATH").unwrap_or_default();
    let entries: Vec<PathBuf> = env::split_paths(&path)
        .filter(|entry| {
            let untrusted = std::fs::read_dir(entry).is_err_and(|error| {
                error.raw_os_error() == Some(ERROR_UNTRUSTED_MOUNT_POINT)
            });
            if untrusted {
                println!(
                    "cargo:warning=leaving {} out of zig's PATH: it goes through a junction this process may not traverse (Windows RedirectionGuard), which makes zig abort",
                    entry.display()
                );
            }
            !untrusted
        })
        .collect();
    env::join_paths(entries).expect("PATH entries came from PATH")
}

/// Generate a compact Rust lookup table from Ghostty's audited theme files.
/// The files are fetched and installed by the same pinned Zig dependency that
/// builds libghostty-vt, so macOS, Windows, and Linux all compile the exact
/// theme definitions represented by Ghostex's Settings picker.
fn generate_embedded_ghostty_themes(themes_dir: &Path) {
    let mut theme_paths = std::fs::read_dir(themes_dir)
        .unwrap_or_else(|error| {
            panic!(
                "failed to read installed Ghostty themes at {}: {error}",
                themes_dir.display()
            )
        })
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    theme_paths.sort();
    assert!(
        !theme_paths.is_empty(),
        "Ghostty theme install produced no files at {}",
        themes_dir.display()
    );

    let mut generated = String::from(
        "fn embedded_ghostty_theme_source(name: &str) -> Option<&'static str> {\n    match name {\n",
    );
    for path in theme_paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("Ghostty theme name is not UTF-8: {}", path.display()));
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        writeln!(generated, "        {name:?} => Some({source:?}),").unwrap();
    }
    generated.push_str("        _ => None,\n    }\n}\n");

    let output =
        PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("embedded_ghostty_themes.rs");
    std::fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

/// Same version resolution as scripts/build-libghostty-vt.sh: the first
/// `.version = "…"` line in ghostty's build.zig.zon.
fn ghostty_app_version(ghostty_dir: &Path) -> String {
    let zon_path = ghostty_dir.join("build.zig.zon");
    let zon = std::fs::read_to_string(&zon_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", zon_path.display()));
    zon.lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix(".version")?;
            let (_, value) = rest.split_once('"')?;
            let (version, _) = value.split_once('"')?;
            Some(version.to_string())
        })
        .unwrap_or_else(|| {
            panic!(
                "could not resolve Ghostty app version from {}",
                zon_path.display()
            )
        })
}

fn gpui_macos_objc_build() -> cc::Build {
    /*
    CDXC:GhostexCapture 2026-09-30 WHY:
    GPUI Objective-C shims must compile against Ghostex's supported macOS 13.0 deployment target, matching the native Xcode project and GPUI package metadata. Do not inherit the current host OS as the minimum target: newer SDKs mark the WindowServer capture API Ghostex Capture uses (`CGWindowListCreateImage` in GpuiGhostexCapture.m) unavailable for future deployment targets.
    */
    let mut build = cc::Build::new();
    build
        .flag("-fobjc-arc")
        .flag("-fblocks")
        .flag("-Wno-deprecated-declarations")
        .flag(GPUI_MACOS_DEPLOYMENT_TARGET_FLAG);
    build
}

fn image_alpha_bounds(image: &image::RgbaImage) -> (u32, u32, u32, u32) {
    let (mut left, mut top) = (image.width(), image.height());
    let (mut right, mut bottom) = (0, 0);
    let mut found_visible_pixel = false;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel.0[3] == 0 {
            continue;
        }
        found_visible_pixel = true;
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    assert!(
        found_visible_pixel,
        "the canonical Ghostex app icon must contain visible pixels"
    );
    (left, top, right - left + 1, bottom - top + 1)
}

fn build_windows_app_resource(manifest_dir: &Path) {
    /*
    CDXC:Icons 2026-07-25:
    gpui_windows loads the application icon from Win32 resource id 1 before it
    registers the window class. Build one multi-size ICO from the canonical
    artwork, removing the macOS icon-mask safe area before resizing. Windows
    supplies its own taskbar/icon inset; retaining both platform insets makes
    the mark visibly undersized. Compile the ICO as exact resource id 1 and
    link it only into the main executable so Explorer, Alt-Tab, the taskbar,
    and the GPUI HWND all receive crisp native sizes.
    */
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let target = env::var("TARGET").expect("TARGET");
    let icon_path = out_dir.join("Ghostex.ico");
    let resource_script_path = out_dir.join("Ghostex.rc");
    let resource_path = out_dir.join("Ghostex.res");

    let source_path = manifest_dir.join(WINDOWS_APP_ICON_SOURCE);
    println!("cargo:rerun-if-changed={}", source_path.display());
    let source_image = image::open(&source_path)
        .unwrap_or_else(|error| panic!("failed to decode {}: {error}", source_path.display()))
        .into_rgba8();
    let (source_x, source_y, source_width, source_height) = image_alpha_bounds(&source_image);
    let artwork = image::imageops::crop_imm(
        &source_image,
        source_x,
        source_y,
        source_width,
        source_height,
    )
    .to_image();

    let icon_images = WINDOWS_APP_ICON_SIZES
        .iter()
        .map(|size| {
            let resized = image::imageops::resize(
                &artwork,
                *size,
                *size,
                image::imageops::FilterType::Lanczos3,
            );
            let mut encoded = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(resized)
                .write_to(&mut encoded, image::ImageFormat::Png)
                .unwrap_or_else(|error| {
                    panic!("failed to encode the {size}px Windows icon: {error}")
                });
            (
                if *size == 256 { 0 } else { *size as u8 },
                encoded.into_inner(),
            )
        })
        .collect::<Vec<_>>();

    let directory_bytes = 6 + icon_images.len() * 16;
    let image_bytes = icon_images
        .iter()
        .map(|(_, bytes)| bytes.len())
        .sum::<usize>();
    let mut icon_bytes = Vec::with_capacity(directory_bytes + image_bytes);
    icon_bytes.extend_from_slice(&0_u16.to_le_bytes());
    icon_bytes.extend_from_slice(&1_u16.to_le_bytes());
    icon_bytes.extend_from_slice(&(icon_images.len() as u16).to_le_bytes());
    let mut image_offset = directory_bytes as u32;
    for (dimension, bytes) in &icon_images {
        icon_bytes.push(*dimension);
        icon_bytes.push(*dimension);
        icon_bytes.push(0);
        icon_bytes.push(0);
        icon_bytes.extend_from_slice(&1_u16.to_le_bytes());
        icon_bytes.extend_from_slice(&32_u16.to_le_bytes());
        icon_bytes.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        icon_bytes.extend_from_slice(&image_offset.to_le_bytes());
        image_offset += bytes.len() as u32;
    }
    for (_, bytes) in icon_images {
        icon_bytes.extend_from_slice(&bytes);
    }
    fs::write(&icon_path, icon_bytes)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", icon_path.display()));

    let package_version = marketing_version().0;
    let mut numeric_version = [0_u16; 4];
    for (index, component) in package_version
        .split_once('-')
        .map_or(package_version.as_str(), |(version, _)| version)
        .split('.')
        .take(4)
        .enumerate()
    {
        numeric_version[index] = component.parse::<u16>().unwrap_or_else(|error| {
            panic!("invalid numeric Cargo package version component {component}: {error}")
        });
    }
    let [version_major, version_minor, version_patch, version_build] = numeric_version;
    let icon_resource_path = icon_path.to_string_lossy().replace('\\', "/");
    let resource_script = format!(
        "1 ICON \"{icon_resource_path}\"\n\
         1 VERSIONINFO\n\
         FILEVERSION {version_major},{version_minor},{version_patch},{version_build}\n\
         PRODUCTVERSION {version_major},{version_minor},{version_patch},{version_build}\n\
         FILEOS 0x40004\n\
         FILETYPE 0x1\n\
         BEGIN\n\
           BLOCK \"StringFileInfo\"\n\
           BEGIN\n\
             BLOCK \"040904B0\"\n\
             BEGIN\n\
               VALUE \"CompanyName\", \"Ghostex\\0\"\n\
               VALUE \"FileDescription\", \"Ghostex\\0\"\n\
               VALUE \"FileVersion\", \"{package_version}\\0\"\n\
               VALUE \"InternalName\", \"Ghostex\\0\"\n\
               VALUE \"LegalCopyright\", \"Copyright (c) Ghostex\\0\"\n\
               VALUE \"OriginalFilename\", \"Ghostex.exe\\0\"\n\
               VALUE \"ProductName\", \"Ghostex\\0\"\n\
               VALUE \"ProductVersion\", \"{package_version}\\0\"\n\
             END\n\
           END\n\
           BLOCK \"VarFileInfo\"\n\
           BEGIN\n\
             VALUE \"Translation\", 0x0409, 1200\n\
           END\n\
         END\n"
    );
    fs::write(&resource_script_path, resource_script).unwrap_or_else(|error| {
        panic!(
            "failed to write Windows resource script {}: {error}",
            resource_script_path.display()
        )
    });

    let resource_compiler = cc::windows_registry::find_tool(&target, "rc.exe")
        .unwrap_or_else(|| panic!("could not find rc.exe for target {target}"));
    let status = resource_compiler
        .to_command()
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource_path)
        .arg(&resource_script_path)
        .status()
        .expect("failed to run rc.exe for the Ghostex Windows app icon");
    assert!(status.success(), "rc.exe failed with {status}");
    println!(
        "cargo:rustc-link-arg-bin=ghostex-gpui={}",
        resource_path.display()
    );
    println!(
        "cargo:rustc-link-arg-bin=ghostex-gpui-cef-bootstrap={}",
        resource_path.display()
    );
}

/// The version the app reports, and whether the release tooling stamped it.
///
/// CDXC:Build 2026-10-07 WHY:
/// The crate versions stay at the placeholder `0.1.0` on purpose (the release skill bumps only the root `package.json`), so `CARGO_PKG_VERSION` is never the app version. Release scripts pass `GHOSTEX_GPUI_MARKETING_VERSION`; every other build (`cargo xtask start`, `cargo check`) reads the same root `package.json` here, so a dev build on any platform shows the real version in About and the feedback footer instead of 0.1.0. The stamped flag is separate because "is this a dev build" used to be answered by "version equals the crate placeholder", which a package.json version would break.
/// SEE-ALSO: server/build.rs reads the same two sources.
fn marketing_version() -> (String, bool) {
    if let Some(version) = env::var("GHOSTEX_GPUI_MARKETING_VERSION")
        .ok()
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
    {
        return (version, true);
    }
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let package_json = manifest_dir.join("../../package.json");
    println!("cargo:rerun-if-changed={}", package_json.display());
    let text = fs::read_to_string(&package_json)
        .unwrap_or_else(|error| panic!("read {}: {error}", package_json.display()));
    let version = text
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("\"version\"")?;
            let rest = rest.trim_start().strip_prefix(':')?;
            let rest = rest.trim_start().strip_prefix('"')?;
            Some(rest.split('"').next()?.to_string())
        })
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| panic!("no \"version\" in {}", package_json.display()));
    (version, false)
}

/// The terminal font faces, in the order `register_gpui_terminal_engine_fonts`
/// (src/terminal_gpui_engine.rs) registers them.
const TERMINAL_FONT_FACES: [&str; 4] = ["Regular", "Bold", "Italic", "BoldItalic"];
/// Part of the cache stamp, so changing how the frame is built rebuilds it.
const TERMINAL_FONT_FRAME_SETTINGS: &str = "zstd level 19, window log 24";

/// Packs the four terminal font faces into one zstd frame in OUT_DIR (the
/// binary embeds only that frame, see `register_gpui_terminal_engine_fonts`)
/// and writes their byte lengths so the unpacked bytes split back into the
/// original files. Level 19 takes about a second in an unoptimized build
/// script and this script also reruns for unrelated inputs, so unchanged faces
/// reuse the frame from the last run.
fn compress_terminal_font_faces() {
    use std::hash::Hasher as _;

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let font_dir = manifest_dir.join("../../.dependencies/ghostty/src/font/res");
    let faces = TERMINAL_FONT_FACES.map(|face| {
        let path = font_dir.join(format!("JetBrainsMonoNerdFont-{face}.ttf"));
        println!("cargo:rerun-if-changed={}", path.display());
        fs::read(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
    });
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let frame_path = out_dir.join("terminal_font_faces.zst");
    let lengths_path = out_dir.join("terminal_font_face_lengths.rs");
    let stamp_path = out_dir.join("terminal_font_faces.stamp");
    let mut hasher = std::hash::DefaultHasher::new();
    for face in &faces {
        hasher.write_usize(face.len());
        hasher.write(face);
    }
    let stamp = format!("{TERMINAL_FONT_FRAME_SETTINGS}: {:016x}", hasher.finish());
    if frame_path.is_file()
        && lengths_path.is_file()
        && fs::read_to_string(&stamp_path).is_ok_and(|saved| saved == stamp)
    {
        return;
    }
    let mut compressor = zstd::bulk::Compressor::new(19).expect("zstd compressor");
    // A window over the whole input, so each face's icon glyphs match the previous face's copy.
    compressor
        .set_parameter(zstd::zstd_safe::CParameter::WindowLog(24))
        .expect("zstd window log");
    compressor
        .set_parameter(zstd::zstd_safe::CParameter::ContentSizeFlag(true))
        .expect("zstd content size flag");
    let frame = compressor
        .compress(&faces.concat())
        .unwrap_or_else(|error| panic!("failed to compress the terminal font faces: {error}"));
    let lengths = faces
        .iter()
        .map(|face| face.len().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(&frame_path, frame)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", frame_path.display()));
    fs::write(&lengths_path, format!("[{lengths}]\n"))
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", lengths_path.display()));
    fs::write(&stamp_path, stamp)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", stamp_path.display()));
}

fn main() {
    // CDXC:Build 2026-10-07 WHY:
    // The About page, telemetry and the Linux update check read GHOSTEX_BUILD_MARKETING_VERSION on every platform. It used to be emitted only from the Windows resource step, so Linux and macOS release builds showed the crate version (0.1.0) instead of the release version.
    println!("cargo:rerun-if-env-changed=GHOSTEX_GPUI_MARKETING_VERSION");
    let (version, stamped) = marketing_version();
    println!("cargo:rustc-env=GHOSTEX_BUILD_MARKETING_VERSION={version}");
    println!(
        "cargo:rustc-env=GHOSTEX_BUILD_VERSION_STAMPED={}",
        u8::from(stamped)
    );
    // CDXC:Build 2026-10-02 WHY:
    // Cargo treats a rerun-if-changed path that does not exist as always changed, and GhosttyKit.xcframework only exists on macOS. Emitting these hints on Linux and Windows reran this build script on every build and recompiled the whole ghostex-gpui crate even when nothing had changed.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed={GHOSTTYKIT_HEADER}");
        println!("cargo:rerun-if-changed={GHOSTTYKIT_ARCHIVE}");
    }
    emit_cef_component_version();
    compress_terminal_font_faces();

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest_dir =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
        let libghostty_vt = cached_libghostty_vt(&manifest_dir, "lib/ghostty-vt-static.lib");
        generate_embedded_ghostty_themes(&libghostty_vt.themes_dir);
        build_windows_app_resource(&manifest_dir);
        println!("cargo:rustc-link-arg={}", libghostty_vt.archive.display());
        let windows_manifest = manifest_dir
            .join("native")
            .join("windows")
            .join("cef-app.exe.manifest");
        println!("cargo:rerun-if-changed={}", windows_manifest.display());
        println!("cargo:rustc-link-arg-bin=ghostex-gpui-cef-helper=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=ghostex-gpui-cef-helper=/MANIFESTINPUT:{}",
            windows_manifest.display()
        );
        /*
        CDXC:CefRuntime 2026-09-28 WHY:
        The web runtime is an optional component, so the app must start with no CEF files present.
        cef-dll-sys links libcef.dll at load time (and already links delayimp); delay-loading it
        means Windows resolves the DLL on the first CEF call, which cef/windows.rs makes only after
        loading the installed component's libcef.dll by full path. The helper stays load-time
        linked: it only ever runs as a CEF subprocess, with the runtime folder on PATH.
        */
        println!("cargo:rustc-link-arg-bin=ghostex-gpui=/DELAYLOAD:libcef.dll");
        return;
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        let manifest_dir =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
        let libghostty_vt = build_libghostty_vt_with_zig(&manifest_dir, "lib/libghostty-vt.a");
        generate_embedded_ghostty_themes(&libghostty_vt.themes_dir);
        println!("cargo:rustc-link-arg={}", libghostty_vt.archive.display());
        /*
        CDXC:PlatformSupport 2026-07-04:
        cef-dll-sys links libcef.so dynamically (`rustc-link-lib=dylib=cef`).
        Development layouts keep the CEF payload beside the executable, so
        $ORIGIN retains the direct dev-run contract. Release layouts enter
        through the CEF-free native bootstrap, which installs the verified
        component and launches the internal runtime with its component path
        on LD_LIBRARY_PATH.
        */
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        return;
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let ghosttykit_archive = manifest_dir.join(GHOSTTYKIT_ARCHIVE);
    let gpui_hooks = manifest_dir.join("native/macos/GpuiCefAppKitHooks.m");
    println!("cargo:rerun-if-changed=native/macos/GpuiWindowCorners.h");
    println!("cargo:rerun-if-changed=native/macos/GpuiKeyboardShortcuts.h");
    println!("cargo:rerun-if-changed=native/macos/GpuiKeyboardShortcuts.m");
    println!("cargo:rerun-if-changed=native/macos/GpuiNavigationGestures.h");
    println!("cargo:rerun-if-changed=native/macos/GpuiNavigationGestures.m");
    println!("cargo:rerun-if-changed=native/macos/GpuiSidebarReveal.m");
    println!("cargo:rerun-if-changed=native/macos/GpuiDocsDrawer.m");
    let gpui_terminal_mouse_cursor = manifest_dir.join("native/macos/GpuiTerminalMouseCursor.m");
    let gpui_settings_notifications = manifest_dir.join("native/macos/GpuiSettingsNotifications.m");
    let gpui_ghostex_capture = manifest_dir.join("native/macos/GpuiGhostexCapture.m");
    let gpui_app_icon = manifest_dir.join("native/macos/GpuiAppIcon.m");
    let gpui_accessibility_display_options =
        manifest_dir.join("native/macos/GpuiAccessibilityDisplayOptions.m");
    let gpui_workspace_power_events = manifest_dir.join("native/macos/GpuiWorkspacePowerEvents.m");
    let gpui_lid_sleep_helper_client = manifest_dir.join("native/macos/GpuiLidSleepHelperClient.m");
    let gpui_menu_bar_status_item = manifest_dir.join("native/macos/GpuiMenuBarStatusItem.m");
    let gpui_sparkle_updater = manifest_dir.join("native/macos/GpuiSparkleUpdater.m");
    let gpui_standard_about_panel = manifest_dir.join("native/macos/GpuiStandardAboutPanel.m");
    let gpui_app_toast_window_chrome = manifest_dir.join("native/macos/GpuiAppToastWindowChrome.m");
    let gpui_chat_dialog_window = manifest_dir.join("native/macos/GpuiChatDialogWindow.m");
    println!(
        "cargo:rerun-if-changed={}",
        gpui_chat_dialog_window.display()
    );

    println!("cargo:rerun-if-changed={}", gpui_hooks.display());
    println!(
        "cargo:rerun-if-changed={}",
        gpui_terminal_mouse_cursor.display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        gpui_settings_notifications.display()
    );
    println!("cargo:rerun-if-changed={}", gpui_app_icon.display());
    println!(
        "cargo:rerun-if-changed={}",
        gpui_accessibility_display_options.display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        gpui_workspace_power_events.display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        gpui_lid_sleep_helper_client.display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        gpui_menu_bar_status_item.display()
    );
    println!("cargo:rerun-if-changed={}", gpui_sparkle_updater.display());
    println!(
        "cargo:rerun-if-changed={}",
        gpui_standard_about_panel.display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        gpui_app_toast_window_chrome.display()
    );

    /*
    CDXC:CefRuntime 2026-06-14-15:25:
    CEF browser creation now comes from tauri-apps/cef-rs instead of GhostexCEFBridge.mm. Keep this build script limited to the AppKit protocol/message-pump shim required because GPUI owns NSApplication and the main run loop.
    */
    gpui_macos_objc_build()
        .file(gpui_hooks)
        .file(manifest_dir.join("native/macos/GpuiKeyboardShortcuts.m"))
        .file(manifest_dir.join("native/macos/GpuiNavigationGestures.m"))
        .file(manifest_dir.join("native/macos/GpuiSidebarReveal.m"))
        .file(manifest_dir.join("native/macos/GpuiDocsDrawer.m"))
        .compile("ghostex_gpui_cef_appkit_hooks");

    /*
    CDXC:Terminal 2026-07-12:
    Compile hide-while-typing cursor concealment as its own library. The
    composited terminal element calls only this function, and binaries
    without the app's Rust callback exports (terminal-element-demo) must be
    able to link it alone.
    */
    gpui_macos_objc_build()
        .file(gpui_terminal_mouse_cursor)
        .compile("ghostex_gpui_terminal_mouse_cursor");

    // Uncaught Objective-C exceptions into gpui-crash-reports.log (see the file's
    // CDXC:CrashReports note).
    let gpui_crash_reports = manifest_dir.join("native/macos/GpuiCrashReports.m");
    println!("cargo:rerun-if-changed={}", gpui_crash_reports.display());
    gpui_macos_objc_build()
        .file(gpui_crash_reports)
        .compile("ghostex_gpui_crash_reports");

    /*
    CDXC:Notifications 2026-06-24-12:44:
    Compile the GPUI Settings notification shim separately from CEF and terminal AppKit adapters. It owns only UserNotifications permission/status/test-banner calls, requests alert authorization, emits no notification sound, and must not grow into session attention routing or persistent logging.
    */
    gpui_macos_objc_build()
        .file(gpui_settings_notifications)
        .compile("ghostex_gpui_settings_notifications");

    // Ghostex Capture's floating windows, system-wide hotkeys and screenshots (see the file's
    // CDXC:GhostexCapture note).
    println!("cargo:rerun-if-changed={}", gpui_ghostex_capture.display());
    gpui_macos_objc_build()
        .file(gpui_ghostex_capture)
        .compile("ghostex_gpui_ghostex_capture");

    // The floating permission bar beside System Settings (see the file's CDXC:OsIntegration note).
    let gpui_permission_guide = manifest_dir.join("native/macos/GpuiPermissionGuide.m");
    println!("cargo:rerun-if-changed={}", gpui_permission_guide.display());
    gpui_macos_objc_build()
        .file(gpui_permission_guide)
        .compile("ghostex_gpui_permission_guide");

    /*
    CDXC:Icons 2026-07-12:
    GPUI reuses the shared Settings App Icon picker and ~/.ghostex/icons
    storage, while this dedicated AppKit shim owns only image masking,
    thumbnails, Dock/app-switcher application, bundle file-icon updates, and
    Finder reveal. Rust owns filename validation, bounded scanning/copying,
    settings persistence, and the typed modal message contract.
    */
    gpui_macos_objc_build()
        .file(gpui_app_icon)
        .compile("ghostex_gpui_app_icon");

    /*
    CDXC:DesignSystem 2026-06-26-07:31:
    Compile the GPUI accessibility display-options shim separately because Reduce Motion is a read-only macOS runtime source. It must not share renderer IPC, hidden views, logging, paths, settings JSON, or notification payloads with unrelated settings bridges.
    */
    gpui_macos_objc_build()
        .file(gpui_accessibility_display_options)
        .compile("ghostex_gpui_accessibility_display_options");

    /*
    CDXC:RemoteMachines 2026-08-12:
    Compile the NSWorkspace wake observer separately from connection logic.
    It forwards only the wake edge; Rust owns tunnel validation and status.
    */
    gpui_macos_objc_build()
        .file(gpui_workspace_power_events)
        .compile("ghostex_gpui_workspace_power_events");

    /*
    CDXC:KeepAwake 2026-06-26-00:09:
    Compile the GPUI lid-sleep helper client as its own macOS shim. Rust owns only start/heartbeat/disable decisions; this Objective-C boundary mirrors the Swift XPC installer/client and returns generic status without exposing helper paths, signing text, installer output, or privileged command details.
    */
    gpui_macos_objc_build()
        .file(gpui_lid_sleep_helper_client)
        .compile("ghostex_gpui_lid_sleep_helper_client");

    /*
    CDXC:StatusPet 2026-06-26-05:42:
    Compile the GPUI menu-bar status item as its own AppKit shim so badge rendering stays outside CEF, renderer IPC, terminal host views, hidden hit regions, broad event routing, logging, paths, URLs, command text, and terminal content.

    CDXC:StatusPet 2026-06-26-06:05:
    The same shim now owns the native Running Agents dropdown fed by Rust-owned sanitized rows, while CEF/sidebar routing still owns project/session focus callbacks.
    */
    gpui_macos_objc_build()
        .file(gpui_menu_bar_status_item)
        .compile("ghostex_gpui_menu_bar_status_item");

    // The Sparkle updater shim resolves Sparkle.framework from the packaged
    // bundle at runtime (NSBundle load + NSClassFromString), so it compiles
    // and links without a Sparkle SDK on the build machine.
    gpui_macos_objc_build()
        .file(gpui_sparkle_updater)
        .compile("ghostex_gpui_sparkle_updater");

    gpui_macos_objc_build()
        .file(gpui_standard_about_panel)
        .compile("ghostex_gpui_standard_about_panel");

    // The native Settings colour swatch's NSColorPanel (settings_modal_lifecycle.rs).
    let gpui_color_panel = manifest_dir.join("native/macos/GpuiColorPanel.m");
    println!("cargo:rerun-if-changed={}", gpui_color_panel.display());
    gpui_macos_objc_build()
        .file(gpui_color_panel)
        .compile("ghostex_gpui_color_panel");

    // The Files view's Open With menu (native_docs/open_externally.rs).
    let gpui_open_with = manifest_dir.join("native/macos/GpuiOpenWith.m");
    println!("cargo:rerun-if-changed={}", gpui_open_with.display());
    gpui_macos_objc_build()
        .file(gpui_open_with)
        .compile("ghostex_gpui_open_with");

    // The voice conversation's and Grok dictation's microphone and speaker (app/voice_agent/audio.rs).
    let gpui_voice_audio = manifest_dir.join("native/macos/GpuiVoiceAudio.m");
    println!("cargo:rerun-if-changed={}", gpui_voice_audio.display());
    gpui_macos_objc_build()
        .file(gpui_voice_audio)
        .compile("ghostex_gpui_voice_audio");

    // Read aloud's speaker: AVSpeechSynthesizer (app/read_aloud/macos.rs).
    let gpui_read_aloud = manifest_dir.join("native/macos/GpuiReadAloud.m");
    println!("cargo:rerun-if-changed={}", gpui_read_aloud.display());
    gpui_macos_objc_build()
        .file(gpui_read_aloud)
        .compile("ghostex_gpui_read_aloud");

    // The first-run intro video's WKWebView (window/onboarding/intro_web_view.rs).
    let gpui_intro_video_web_view = manifest_dir.join("native/macos/GpuiIntroVideoWebView.m");
    println!(
        "cargo:rerun-if-changed={}",
        gpui_intro_video_web_view.display()
    );
    gpui_macos_objc_build()
        .file(gpui_intro_video_web_view)
        .compile("ghostex_gpui_intro_video_web_view");

    /*
    CDXC:AppModal 2026-07-04:
    Compile the toast popup chrome shim separately because it only strips AppKit
    frame chrome from GPUI's transparent toast popup host. It must not grow into
    hit-test routing, overlays, modal behavior, CEF ownership, terminal focus,
    logging, or renderer IPC.
    */
    gpui_macos_objc_build()
        .file(gpui_app_toast_window_chrome)
        .file(gpui_chat_dialog_window)
        .compile("ghostex_gpui_app_toast_window_chrome");

    /*
    CDXC:Terminal 2026-06-22-22:29:
    GPUI now references real GhosttyKit/libghostty runtime and surface symbols from Rust, so macOS builds intentionally link the repo-local static archive plus the system libraries used by the native GhosttyKit embedding path. This build-time path output is allowed, but runtime code must still avoid logging private paths, terminal content, command text, URLs, tokens, or fallback surface state.

    CDXC:Terminal 2026-06-23-03:27:
    The Ghostty Metal renderer now pulls IOSurface symbols from the static GhosttyKit archive, so local GPUI builds must link IOSurface explicitly instead of relying on transitive framework flags from other crates.
    */
    let libghostty_vt = build_libghostty_vt(&manifest_dir);
    generate_embedded_ghostty_themes(&libghostty_vt.themes_dir);
    println!("cargo:rustc-link-arg={}", libghostty_vt.archive.display());

    if let Some(ghosttykit_archive_dir) = ghosttykit_archive.parent() {
        println!(
            "cargo:rustc-link-search=native={}",
            ghosttykit_archive_dir.display()
        );
        println!("cargo:rustc-link-arg={}", ghosttykit_archive.display());
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-lib=z");
    }
    /*
    CDXC:Release 2026-10-23 DECISION:
    The user chose to stop exporting symbols from the app's executables. The GhosttyKit archive CI builds leaves glslang, SPIRV-Cross and libc++ template instances as visible weak symbols, so the 10.30.0 app exported 17,784 of them: their export trie, names and weak binds cost about 0.8 MB of the DMG, and dyld coalesced every one at launch. Nothing loads symbols from these executables (GPUI's dlsym looks up CoreGraphics functions), so none need exporting.
    */
    println!("cargo:rustc-link-arg-bins=-Wl,-no_exported_symbols");

    println!("cargo:rustc-link-lib=framework=Cocoa");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=Carbon");
    println!("cargo:rustc-link-lib=framework=Metal");
    println!("cargo:rustc-link-lib=framework=QuartzCore");
    println!("cargo:rustc-link-lib=framework=CoreText");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rustc-link-lib=framework=Security");
    println!("cargo:rustc-link-lib=framework=ApplicationServices");
    println!("cargo:rustc-link-lib=framework=IOKit");
    println!("cargo:rustc-link-lib=framework=IOSurface");
    println!("cargo:rustc-link-lib=framework=UniformTypeIdentifiers");
    println!("cargo:rustc-link-lib=framework=UserNotifications");
    println!("cargo:rustc-link-lib=framework=WebKit");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
}
