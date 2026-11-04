#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
LINUX_ARCH="$(release_gpui_linux_arch)"
DEB_ARCH="$(release_gpui_linux_deb_arch "$LINUX_ARCH")"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" "linux-deb-$LINUX_ARCH")}"
release_gpui_require_version "$VERSION"
release_gpui_require_command dpkg-deb
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

# CDXC:Release 2026-08-13: the release workflow stages the
# Linux payload once and produces both packages from that identical tree, so a
# caller that already ran linux-stage.sh passes the staged root in instead of
# paying for a second full cargo build.
PACKAGE_ROOT="${GHOSTEX_LINUX_PACKAGE_ROOT:-$REPO_ROOT/build/release-gpui/linux-deb-package-root}"
if [[ "${GHOSTEX_LINUX_PACKAGE_ROOT_READY:-0}" != "1" ]]; then
	"$SCRIPT_DIR/linux-stage.sh" "$VERSION" "$PACKAGE_ROOT"
fi
[[ -d "$PACKAGE_ROOT/opt/ghostex" ]] || {
	echo "Linux package root is not staged: $PACKAGE_ROOT" >&2
	exit 1
}
mkdir -p "$PACKAGE_ROOT/DEBIAN"
cat >"$PACKAGE_ROOT/DEBIAN/control" <<EOF
Package: ghostex
Version: $VERSION
Section: devel
Priority: optional
Architecture: $DEB_ARCH
Maintainer: Ghostex <support@ghostex.app>
Depends: ffmpeg, libasound2t64 | libasound2, libatk-bridge2.0-0t64 | libatk-bridge2.0-0, libatk1.0-0t64 | libatk1.0-0, libatspi2.0-0t64 | libatspi2.0-0, libc6, libcairo2, libcups2t64 | libcups2, libdbus-1-3, libdrm2, libexpat1, libfontconfig1, libgbm1, libglib2.0-0t64 | libglib2.0-0, libgtk-3-0t64 | libgtk-3-0, libnspr4, libnss3, libpango-1.0-0, libpangocairo-1.0-0, libx11-6, libx11-xcb1, libxcb1, libxcomposite1, libxdamage1, libxext6, libxfixes3, libxkbcommon0, libxkbcommon-x11-0, libxrandr2, libxshmfence1
Recommends: wmctrl
Description: Ghostex desktop application
 Ghostex provides native AI development workspaces, terminals, and project tools.
EOF
DEB="$OUTPUT/ghostex_${VERSION}_${DEB_ARCH}.deb"
# CDXC:Release 2026-10-22 WHY:
# The runner's dpkg-deb defaults to a zstd -19 data.tar.zst: the 10.29.0 x64
# DEB was 92.0 MB, where xz -9e holds the same files in about 77.5 MB. xz is
# also the member every dpkg reads (since 1.15.6), while zstd members need
# dpkg 1.21.18 or an Ubuntu-patched dpkg, so Debian 11 could not install the
# zstd package at all. Compressing takes about 674 MiB of memory per xz
# thread; installing needs 65 MiB.
dpkg-deb --build --root-owner-group -Zxz -z9 -Sextreme "$PACKAGE_ROOT" "$DEB"
dpkg-deb --info "$DEB" >/dev/null
release_gpui_write_manifest "$OUTPUT" "linux-deb-$LINUX_ARCH" "$VERSION" "$DEB"
printf 'Built Debian/Ubuntu %s release payload in %s\n' "$LINUX_ARCH" "$OUTPUT"
