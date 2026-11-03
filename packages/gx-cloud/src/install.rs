//! The script that puts the Linux gxserver package on a box (`/opt/ghostex`) from the GitHub
//! release, unless the box already has that version (a box from a snapshot keeps the package, so
//! only blank boxes and version changes download it, R10).

use ghostex_gx_protocol::cloud_box::BOX_PACKAGE_DIR;

/// The GitHub repository whose releases carry `gxserver-linux-<arch>.tar.gz`.
pub const RELEASE_REPOSITORY: &str = "maddada/Ghostex";

/// The release asset of a version and a package arch (`x64`, `arm64`).
pub fn release_asset_url(version: &str, arch: &str) -> String {
    format!("https://github.com/{RELEASE_REPOSITORY}/releases/download/v{version}/gxserver-linux-{arch}.tar.gz")
}

/// A `sh` script (run with `sh -c`) that installs `version` unless it is there. It picks the arch
/// from `uname -m`, uses curl or wget, and sudo when it isn't root (Instabox's guest user).
pub fn install_script(version: &str) -> String {
    let dir = BOX_PACKAGE_DIR;
    let x64 = release_asset_url(version, "x64");
    let arm64 = release_asset_url(version, "arm64");
    format!(
        r#"set -e
if [ "$({dir}/bin/gxserver --version 2>/dev/null)" = "{version}" ]; then echo installed; exit 0; fi
case "$(uname -m)" in x86_64|amd64) url="{x64}";; aarch64|arm64) url="{arm64}";; *) echo "unsupported arch $(uname -m)" >&2; exit 2;; esac
SUDO=""; [ "$(id -u)" = 0 ] || SUDO="sudo -n"
$SUDO mkdir -p {dir}
tmp="$(mktemp)"
if command -v curl >/dev/null 2>&1; then curl -fsSL "$url" -o "$tmp"; else wget -qO "$tmp" "$url"; fi
$SUDO tar -xzf "$tmp" -C {dir}
rm -f "$tmp"
[ "$(id -u)" = 0 ] || $SUDO chown -R "$(id -u):$(id -g)" {dir}
echo downloaded
"#
    )
}
