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
    let component_version: String = raw_version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect();
    println!("cargo:rerun-if-changed={}", bindings.display());
    println!("cargo:rustc-env=GHOSTEX_CEF_COMPONENT_VERSION={component_version}");
}

/*
CDXC:GPUILibghosttyVt 2026-07-03:
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
}

/*
CDXC:GPUIWindowsBringup 2026-07-04:
Windows and Linux builds need only libghostty-vt (the GPUI terminal engine);
the GhosttyKit archive, ObjC shims, and Apple frameworks are macOS-only by
design. Zig cross-compiles natively and the vendored ghostty build already
emits the static lib under both names (`lib/ghostty-vt-static.lib` on
Windows, avoiding the DLL import-lib collision; `lib/libghostty-vt.a`
elsewhere — see .dependencies/ghostty/build.zig), so this hook invokes zig directly
instead of the macOS bash script (which exists only to pick a Zig 0.16.x
binary and redirect the Xcode SDK, both meaningless off macOS).
Zig resolution: GHOSTEX_ZIG override, else `zig` on PATH; ghostty's
requireZig rejects mismatched versions with a clear message.
NEEDS-DEVICE-VERIFY: written from code-reading on macOS, never executed on
Windows or Linux hardware.
*/
fn build_libghostty_vt_with_zig(
    manifest_dir: &Path,
    archive_relative_path: &str,
) -> LibGhosttyVtBuild {
    let ghostty_dir = manifest_dir.join("../../.dependencies/ghostty");
    emit_libghostty_vt_rerun_hints(&ghostty_dir);

    let zig = env::var("GHOSTEX_ZIG").unwrap_or_else(|_| "zig".to_string());
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
        // Zig 0.16's ReleaseSafe Windows ARM64 stack-trace implementation
        // fails to compile in std.debug.SelfInfo.Windows due to an invalid
        // pointer-alignment cast. Production ARM64 archives do not need that
        // debug-only path.
        "ReleaseFast"
    } else {
        "ReleaseSafe"
    };
    let mut command = Command::new(&zig);
    command.current_dir(&ghostty_dir).arg("build");
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
    CDXC:GPUIAppShots 2026-06-26-04:18:
    GPUI Objective-C shims must compile against Ghostex's supported macOS 13.0 deployment target, matching the native Xcode project and GPUI package metadata. Do not inherit the current host OS as the minimum target because newer SDKs mark App Shots' real WindowServer capture API unavailable for future deployment targets.
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
    CDXC:GPUIWindowsAppIcon 2026-07-25:
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

    println!("cargo:rerun-if-env-changed=GHOSTEX_GPUI_MARKETING_VERSION");
    let package_version = env::var("GHOSTEX_GPUI_MARKETING_VERSION")
        .unwrap_or_else(|_| env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION"));
    println!("cargo:rustc-env=GHOSTEX_BUILD_MARKETING_VERSION={package_version}");
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

fn main() {
    println!("cargo:rerun-if-changed={GHOSTTYKIT_HEADER}");
    println!("cargo:rerun-if-changed={GHOSTTYKIT_ARCHIVE}");
    emit_cef_component_version();

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest_dir =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
        let libghostty_vt =
            build_libghostty_vt_with_zig(&manifest_dir, "lib/ghostty-vt-static.lib");
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
        return;
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        let manifest_dir =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
        let libghostty_vt = build_libghostty_vt_with_zig(&manifest_dir, "lib/libghostty-vt.a");
        generate_embedded_ghostty_themes(&libghostty_vt.themes_dir);
        println!("cargo:rustc-link-arg={}", libghostty_vt.archive.display());
        /*
        CDXC:GPUILinuxX11Backend 2026-07-04:
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
    let gpui_terminal_appkit_adapter =
        manifest_dir.join("native/macos/GpuiTerminalAppKitAdapter.m");
    let gpui_terminal_mouse_cursor = manifest_dir.join("native/macos/GpuiTerminalMouseCursor.m");
    let gpui_settings_notifications = manifest_dir.join("native/macos/GpuiSettingsNotifications.m");
    let gpui_app_shots = manifest_dir.join("native/macos/GpuiAppShots.m");
    let gpui_app_icon = manifest_dir.join("native/macos/GpuiAppIcon.m");
    let gpui_accessibility_display_options =
        manifest_dir.join("native/macos/GpuiAccessibilityDisplayOptions.m");
    let gpui_workspace_power_events = manifest_dir.join("native/macos/GpuiWorkspacePowerEvents.m");
    let gpui_lid_sleep_helper_client = manifest_dir.join("native/macos/GpuiLidSleepHelperClient.m");
    let gpui_menu_bar_status_item = manifest_dir.join("native/macos/GpuiMenuBarStatusItem.m");
    let gpui_sparkle_updater = manifest_dir.join("native/macos/GpuiSparkleUpdater.m");
    let gpui_standard_about_panel = manifest_dir.join("native/macos/GpuiStandardAboutPanel.m");
    let gpui_app_toast_window_chrome = manifest_dir.join("native/macos/GpuiAppToastWindowChrome.m");

    println!("cargo:rerun-if-changed={}", gpui_hooks.display());
    println!(
        "cargo:rerun-if-changed={}",
        gpui_terminal_appkit_adapter.display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        gpui_terminal_mouse_cursor.display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        gpui_settings_notifications.display()
    );
    println!("cargo:rerun-if-changed={}", gpui_app_shots.display());
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
    CDXC:GPUICefAppKitShim 2026-06-14-15:25:
    CEF browser creation now comes from tauri-apps/cef-rs instead of GhostexCEFBridge.mm. Keep this build script limited to the AppKit protocol/message-pump shim required because GPUI owns NSApplication and the main run loop.
    */
    gpui_macos_objc_build()
        .file(gpui_hooks)
        .compile("ghostex_gpui_cef_appkit_hooks");

    /*
    CDXC:GPUTerminalAppKitAdapter 2026-06-22-20:58:
    Compile the GPUI-local terminal AppKit adapter as a separate shim from CEF so real terminal host views can be positioned and shown or hidden through the owner path without fake views, logging, overlays, hit-test routing, or process behavior.

    CDXC:GPUTerminalAppKitAdapter 2026-06-22-22:42:
    GhosttyKit link flags are now declared separately below because Rust owns the real focused one-pane surface lifecycle; keep this Objective-C shim limited to AppKit view frame and visibility operations.
    */
    gpui_macos_objc_build()
        .file(gpui_terminal_appkit_adapter)
        .compile("ghostex_gpui_terminal_appkit_adapter");

    /*
    CDXC:GPUITerminalMouseCursor 2026-07-12:
    Compile hide-while-typing cursor concealment separately from the terminal
    host-view adapter. The composited terminal element calls only this
    function, and binaries without the app's Rust key/IME callback exports
    (terminal-element-demo) must not inherit the adapter's link dependencies
    through it.
    */
    gpui_macos_objc_build()
        .file(gpui_terminal_mouse_cursor)
        .compile("ghostex_gpui_terminal_mouse_cursor");

    /*
    CDXC:GPUISettingsNotifications 2026-06-24-12:44:
    Compile the GPUI Settings notification shim separately from CEF and terminal AppKit adapters. It owns only UserNotifications permission/status/test-banner calls, requests alert authorization, emits no notification sound, and must not grow into session attention routing or persistent logging.
    */
    gpui_macos_objc_build()
        .file(gpui_settings_notifications)
        .compile("ghostex_gpui_settings_notifications");

    /*
    CDXC:GPUIAppShots 2026-06-25-23:07:
    Compile App Shots as a dedicated macOS shim because it owns only shared-settings hotkey monitoring, WindowServer capture, and the `~/.ghostex/i` PNG write path. Keep it separate from CEF, terminal AppKit, and notification shims so the feature does not add overlays, hit-test routing, persistent logging, or renderer-provided screenshot authority.
    */
    gpui_macos_objc_build()
        .file(gpui_app_shots)
        .compile("ghostex_gpui_app_shots");

    /*
    CDXC:GPUIAppIcon 2026-07-12:
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
    CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
    Compile the GPUI accessibility display-options shim separately because Pet Overlay Reduce Motion is a read-only macOS runtime source. It must not share renderer IPC, hidden views, logging, paths, settings JSON, or notification payloads with unrelated settings bridges.
    */
    gpui_macos_objc_build()
        .file(gpui_accessibility_display_options)
        .compile("ghostex_gpui_accessibility_display_options");

    /*
    CDXC:GPUIRemoteWakeReconnect 2026-08-12:
    Compile the NSWorkspace wake observer separately from connection logic.
    It forwards only the wake edge; Rust owns tunnel validation and status.
    */
    gpui_macos_objc_build()
        .file(gpui_workspace_power_events)
        .compile("ghostex_gpui_workspace_power_events");

    /*
    CDXC:GPUITitlebarKeepAwake 2026-06-26-00:09:
    Compile the GPUI lid-sleep helper client as its own macOS shim. Rust owns only start/heartbeat/disable decisions; this Objective-C boundary mirrors the Swift XPC installer/client and returns generic status without exposing helper paths, signing text, installer output, or privileged command details.
    */
    gpui_macos_objc_build()
        .file(gpui_lid_sleep_helper_client)
        .compile("ghostex_gpui_lid_sleep_helper_client");

    /*
    CDXC:GPUIMenuBarStatusItem 2026-06-26-05:42:
    Compile the GPUI menu-bar status item as its own AppKit shim so badge rendering stays outside CEF, renderer IPC, terminal host views, hidden hit regions, broad event routing, logging, paths, URLs, command text, and terminal content.

    CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
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

    /*
    CDXC:GPUIAppToastWindowChrome 2026-07-04:
    Compile the toast popup chrome shim separately because it only strips AppKit
    frame chrome from GPUI's transparent toast popup host. It must not grow into
    hit-test routing, overlays, modal behavior, CEF ownership, terminal focus,
    logging, or renderer IPC.
    */
    gpui_macos_objc_build()
        .file(gpui_app_toast_window_chrome)
        .compile("ghostex_gpui_app_toast_window_chrome");

    /*
    CDXC:GPUIGhosttyKitAdapter 2026-06-22-22:29:
    GPUI now references real GhosttyKit/libghostty runtime and surface symbols from Rust, so macOS builds intentionally link the repo-local static archive plus the system libraries used by the native GhosttyKit embedding path. This build-time path output is allowed, but runtime code must still avoid logging private paths, terminal content, command text, URLs, tokens, or fallback surface state.

    CDXC:GPUIGhosttyKitAdapter 2026-06-23-03:27:
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
}
