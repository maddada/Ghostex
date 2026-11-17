//! What a crash report says about this build and this computer: version, build number, commit,
//! how Ghostex was installed, the OS and its version, the computer model and the chip. Read when
//! the report is built (the next open, off the main thread), never in the crashing process.

use std::path::Path;
use std::process::Command;

pub(crate) const APP_VERSION: &str = env!("GHOSTEX_BUILD_MARKETING_VERSION");

/// The commit a release was built from (`GHOSTEX_BUILD_COMMIT`, set by build.rs from the release
/// job's `GITHUB_SHA`); local builds have none.
pub(crate) fn build_commit() -> Option<&'static str> {
    Some(env!("GHOSTEX_BUILD_COMMIT")).filter(|commit| !commit.is_empty())
}

/// `11.8.2` becomes `110802`, the number macOS shows as the build (`CFBundleVersion`).
pub(crate) fn build_number(version: &str) -> Option<String> {
    let mut parts = version
        .split(['-', '+'])
        .next()?
        .split('.')
        .map(|part| part.parse::<u64>().ok());
    let (major, minor, patch) = (parts.next()??, parts.next()??, parts.next()??);
    Some(format!("{}", major * 10_000 + minor * 100 + patch))
}

/// The facts the report's System line shows.
#[derive(Clone, Debug, Default)]
pub(crate) struct SystemFacts {
    /// `macOS 26.6.2 (25G83)`, `Windows 11 Pro 10.0.26100`, `Ubuntu 24.04.1 LTS (6.8.0-45-generic)`.
    pub(crate) os: String,
    /// `MacBook Air (Apple M1)`, `Mac14,2`, the DMI product name on Linux.
    pub(crate) model: Option<String>,
    pub(crate) arch: &'static str,
    /// `Mac app`, `Homebrew`, `Windows app`, `AppImage`, `deb`, `rpm`, `AUR`, `Linux package`,
    /// `Local build`.
    pub(crate) installed_as: &'static str,
}

/// Blocking: spawns `sw_vers`, `system_profiler` and the like. Call off the main thread.
pub(crate) fn read_system_facts() -> SystemFacts {
    SystemFacts {
        os: os_description(),
        model: computer_model(),
        // The names Apple and Microsoft use: arm64 rather than Rust's aarch64.
        arch: match std::env::consts::ARCH {
            "aarch64" => "arm64",
            other => other,
        },
        installed_as: installed_as(),
    }
}

fn output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(target_os = "macos")]
fn os_description() -> String {
    let version = output("/usr/bin/sw_vers", &["-productVersion"]).unwrap_or_default();
    match output("/usr/bin/sw_vers", &["-buildVersion"]) {
        Some(build) => format!("macOS {version} ({build})"),
        None => format!("macOS {version}"),
    }
}

#[cfg(target_os = "linux")]
fn os_description() -> String {
    let pretty = ["/etc/os-release", "/usr/lib/os-release"]
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("PRETTY_NAME=")
                    .map(|value| value.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "Linux".to_string());
    match output("uname", &["-r"]) {
        Some(kernel) => format!("{pretty} ({kernel})"),
        None => pretty,
    }
}

#[cfg(target_os = "windows")]
fn os_description() -> String {
    let key = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let product = registry_value(key, "ProductName").unwrap_or_else(|| "Windows".to_string());
    let build = registry_value(key, "CurrentBuild");
    let display = registry_value(key, "DisplayVersion");
    // Windows 11 still says "Windows 10" in ProductName; build 22000 and later is Windows 11.
    let product = match build.as_deref().and_then(|build| build.parse::<u32>().ok()) {
        Some(number) if number >= 22_000 => product.replacen("Windows 10", "Windows 11", 1),
        _ => product,
    };
    match (display, build) {
        (Some(display), Some(build)) => format!("{product} {display} (build {build})"),
        (None, Some(build)) => format!("{product} (build {build})"),
        _ => product,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn os_description() -> String {
    std::env::consts::OS.to_string()
}

#[cfg(target_os = "windows")]
fn registry_value(key: &str, name: &str) -> Option<String> {
    let text = output("reg", &["query", key, "/v", name])?;
    text.lines()
        .find(|line| line.trim_start().starts_with(name))
        .and_then(|line| line.split("REG_SZ").nth(1))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "macos")]
fn computer_model() -> Option<String> {
    // "Model Name: MacBook Air" and "Chip: Apple M1" (Intel Macs say "Processor Name").
    let profile = output("/usr/sbin/system_profiler", &["SPHardwareDataType"]);
    let field = |name: &str| {
        profile.as_deref().and_then(|text| {
            text.lines().find_map(|line| {
                line.trim()
                    .strip_prefix(name)
                    .and_then(|rest| rest.strip_prefix(':'))
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
        })
    };
    let name = field("Model Name");
    let chip = field("Chip").or_else(|| field("Processor Name"));
    match (name, chip) {
        (Some(name), Some(chip)) => Some(format!("{name} ({chip})")),
        (Some(name), None) => Some(name),
        _ => output("/usr/sbin/sysctl", &["-n", "hw.model"]),
    }
}

#[cfg(target_os = "linux")]
fn computer_model() -> Option<String> {
    let read = |name: &str| {
        std::fs::read_to_string(format!("/sys/devices/virtual/dmi/id/{name}"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| {
                !value.is_empty() && !value.eq_ignore_ascii_case("To be filled by O.E.M.")
            })
    };
    match (read("sys_vendor"), read("product_name")) {
        (Some(vendor), Some(product)) if !product.starts_with(&vendor) => {
            Some(format!("{vendor} {product}"))
        }
        (_, Some(product)) => Some(product),
        (vendor, None) => vendor,
    }
}

#[cfg(target_os = "windows")]
fn computer_model() -> Option<String> {
    let key = r"HKLM\HARDWARE\DESCRIPTION\System\BIOS";
    match (
        registry_value(key, "SystemManufacturer"),
        registry_value(key, "SystemProductName"),
    ) {
        (Some(maker), Some(product)) if !product.starts_with(&maker) => {
            Some(format!("{maker} {product}"))
        }
        (_, Some(product)) => Some(product),
        (maker, None) => maker,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn computer_model() -> Option<String> {
    None
}

/// How this copy was installed, read from where it runs.
fn installed_as() -> &'static str {
    let stamped = env!("GHOSTEX_BUILD_VERSION_STAMPED") == "1";
    let Ok(exe) = std::env::current_exe() else {
        return "Unknown";
    };
    installed_as_for(&exe, stamped)
}

#[cfg(target_os = "macos")]
fn installed_as_for(exe: &Path, stamped: bool) -> &'static str {
    if !stamped {
        return "Local build";
    }
    let homebrew = [
        "/opt/homebrew/Caskroom/ghostex",
        "/usr/local/Caskroom/ghostex",
    ]
    .iter()
    .any(|path| Path::new(path).is_dir());
    if homebrew && exe.starts_with("/Applications") {
        "Homebrew"
    } else {
        "Mac app"
    }
}

#[cfg(target_os = "windows")]
fn installed_as_for(exe: &Path, stamped: bool) -> &'static str {
    // Velopack installs keep their manifest beside the app (windows_updater.rs).
    let velopack = exe
        .parent()
        .is_some_and(|directory| directory.join("sq.version").is_file());
    match (stamped, velopack) {
        (_, true) => "Windows app",
        (true, false) => "Windows (portable)",
        (false, false) => "Local build",
    }
}

#[cfg(target_os = "linux")]
fn installed_as_for(exe: &Path, stamped: bool) -> &'static str {
    if exe
        .parent()
        .is_some_and(|directory| directory.join(".ghostex-appimage").is_file())
    {
        return "AppImage";
    }
    if !exe.starts_with("/opt/ghostex") {
        return if stamped {
            "Linux (other)"
        } else {
            "Local build"
        };
    }
    let any_entry = |directory: &str, prefix: &str| {
        std::fs::read_dir(directory).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        })
    };
    if any_entry("/var/lib/pacman/local", "ghostex") {
        "AUR"
    } else if Path::new("/var/lib/dpkg/info/ghostex.list").is_file() {
        "deb"
    } else if output("rpm", &["-q", "ghostex"]).is_some() {
        "rpm"
    } else {
        "Linux package"
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn installed_as_for(_exe: &Path, _stamped: bool) -> &'static str {
    "Unknown"
}
