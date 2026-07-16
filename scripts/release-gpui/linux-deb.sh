#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" linux-deb-x64)}"
release_gpui_require_version "$VERSION"
release_gpui_require_command dpkg-deb
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

PACKAGE_ROOT="$REPO_ROOT/build/release-gpui/linux-deb-package-root"
"$SCRIPT_DIR/linux-stage.sh" "$VERSION" "$PACKAGE_ROOT"
mkdir -p "$PACKAGE_ROOT/DEBIAN"
cat >"$PACKAGE_ROOT/DEBIAN/control" <<EOF
Package: ghostex
Version: $VERSION
Section: devel
Priority: optional
Architecture: amd64
Maintainer: Ghostex <support@ghostex.app>
Depends: libasound2, libatk1.0-0, libc6, libcairo2, libcups2, libdbus-1-3, libdrm2, libexpat1, libfontconfig1, libgbm1, libglib2.0-0, libgtk-3-0, libnspr4, libnss3, libpango-1.0-0, libx11-6, libxcb1, libxcomposite1, libxdamage1, libxext6, libxfixes3, libxkbcommon0, libxrandr2
Description: Ghostex desktop application
 Ghostex provides native AI development workspaces, terminals, and project tools.
EOF
DEB="$OUTPUT/ghostex_${VERSION}_amd64.deb"
dpkg-deb --build --root-owner-group "$PACKAGE_ROOT" "$DEB"
dpkg-deb --info "$DEB" >/dev/null
release_gpui_write_manifest "$OUTPUT" linux-deb-x64 "$VERSION" "$DEB"
printf 'Built Debian/Ubuntu x64 release payload in %s\n' "$OUTPUT"
