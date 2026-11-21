/*
CDXC:Telemetry 2026-08-26:
Bake the SHIPPING marketing version into the gxserver binary, mirroring
`apps/desktop/build.rs`.

`CARGO_PKG_VERSION` for this crate is the placeholder `0.1.0` and has never
tracked releases, so without this every analytics event, and anything else that
wants to know what build it is, would report the same value for every version
Ghostex has ever shipped. The release scripts already resolve the real marketing
version for the desktop crate; they now pass the same value here.

CDXC:Build 2026-10-07 WHY:
Builds without `GHOSTEX_GPUI_MARKETING_VERSION` (`cargo xtask start`, `cargo check`)
read the same root `package.json` version, so a dev build reports the real app
version too instead of 0.1.0. They stay identifiable as dev builds through
`GHOSTEX_BUILD_VERSION_STAMPED=0` (`telemetry::base::is_dev_build`), not through
a version that differs from the release. Mirrors `apps/desktop/build.rs`.
*/

use std::{env, fs, path::PathBuf};

#[path = "build_windows_version.rs"]
mod windows_version;

fn main() {
    println!("cargo:rerun-if-env-changed=GHOSTEX_GPUI_MARKETING_VERSION");
    let (marketing_version, stamped) = marketing_version();
    println!("cargo:rustc-env=GHOSTEX_BUILD_MARKETING_VERSION={marketing_version}");
    println!(
        "cargo:rustc-env=GHOSTEX_BUILD_VERSION_STAMPED={}",
        u8::from(stamped)
    );
    embed_windows_version_info(&marketing_version);
}

/// CDXC:HelperNames 2026-11-21: on Windows, gxserver.exe reads "Ghostex Server" and ghostex.exe
/// "Ghostex CLI" in Firewall and Defender dialogs and Task Manager (WHY in build_windows_version.rs).
/// The app installs ghostex.exe as a hard link to gxserver.exe (apps/desktop/src/windows_cli_link.rs),
/// so on a PC both names show "Ghostex Server"; the separately built ghostex.exe the release
/// uploads reads "Ghostex CLI".
fn embed_windows_version_info(version: &str) {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
    {
        return;
    }
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    for (bin, description) in [("gxserver", "Ghostex Server"), ("ghostex", "Ghostex CLI")] {
        let file_name = format!("{bin}.exe");
        let info = windows_version::VersionInfo {
            file_description: description,
            product_name: "Ghostex",
            company_name: "Ghostex",
            internal_name: bin,
            original_filename: &file_name,
            version_text: version,
            version: windows_version::numeric_version(version),
        };
        let path = out_dir.join(format!("{bin}-version.res"));
        fs::write(&path, windows_version::version_resource_file(&info))
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
        println!("cargo:rustc-link-arg-bin={bin}={}", path.display());
    }
}

fn marketing_version() -> (String, bool) {
    if let Some(version) = env::var("GHOSTEX_GPUI_MARKETING_VERSION")
        .ok()
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
    {
        return (version, true);
    }
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let package_json = manifest_dir.join("../package.json");
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
