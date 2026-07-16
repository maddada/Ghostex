#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" linux-x64)}"
release_gpui_require_version "$VERSION"
release_gpui_require_command bun
release_gpui_require_command cargo
release_gpui_require_command dpkg-deb
release_gpui_require_command rpmbuild
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

if [[ "$(uname -m)" != "x86_64" ]]; then
  echo "Linux release packaging is x64-only and requires an x86_64 runner." >&2
  exit 1
fi

"$SCRIPT_DIR/prepare-references.sh"
if [[ ! -x "$REPO_ROOT/build/remote-gxserver-linux/x64/package/bin/gxserver" ]]; then
  "$REPO_ROOT/scripts/build-remote-gxserver-linux-release.sh" --arch x64
fi
"$REPO_ROOT/gpui/scripts/build-linux-app.sh"

APP_DIR="$REPO_ROOT/gpui/build/linux/Ghostex"
[[ -x "$APP_DIR/ghostex-gpui" ]] || { echo "Linux build is missing ghostex-gpui" >&2; exit 1; }
[[ -f "$APP_DIR/libcef.so" ]] || { echo "Linux build is missing libcef.so" >&2; exit 1; }
[[ -x "$APP_DIR/gxserver/bin/gxserver" ]] || { echo "Linux build is missing bundled gxserver" >&2; exit 1; }

PACKAGE_ROOT="$REPO_ROOT/build/release-gpui/linux-package-root"
rm -rf "$PACKAGE_ROOT"
mkdir -p \
  "$PACKAGE_ROOT/opt/ghostex" \
  "$PACKAGE_ROOT/usr/bin" \
  "$PACKAGE_ROOT/usr/share/applications" \
  "$PACKAGE_ROOT/usr/share/icons/hicolor/256x256/apps"
cp -a "$APP_DIR/." "$PACKAGE_ROOT/opt/ghostex/"
cp "$REPO_ROOT/gpui/resources/AppIcon.appiconset/icon_256x256.png" \
  "$PACKAGE_ROOT/usr/share/icons/hicolor/256x256/apps/ghostex.png"
cat >"$PACKAGE_ROOT/usr/bin/ghostex" <<'EOF'
#!/usr/bin/env bash
exec /opt/ghostex/ghostex-gpui "$@"
EOF
chmod 755 "$PACKAGE_ROOT/usr/bin/ghostex"
cat >"$PACKAGE_ROOT/usr/share/applications/ghostex.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Ghostex
Comment=AI development workspaces and terminals
Exec=/usr/bin/ghostex %U
Icon=ghostex
Terminal=false
Categories=Development;TerminalEmulator;
StartupNotify=true
EOF

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

rm -rf "$PACKAGE_ROOT/DEBIAN"
RPM_ROOT="$REPO_ROOT/build/release-gpui/rpmbuild"
rm -rf "$RPM_ROOT"
mkdir -p "$RPM_ROOT"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
tar -C "$PACKAGE_ROOT" -czf "$RPM_ROOT/SOURCES/ghostex-$VERSION.tar.gz" .
cat >"$RPM_ROOT/SPECS/ghostex.spec" <<EOF
Name: ghostex
Version: $VERSION
Release: 1%{?dist}
Summary: Ghostex desktop application
License: Proprietary
URL: https://ghostex.app
Source0: ghostex-$VERSION.tar.gz
BuildArch: x86_64
Requires: alsa-lib, atk, cairo, cups-libs, dbus-libs, expat, fontconfig, gtk3, libX11, libXcomposite, libXdamage, libXext, libXfixes, libXrandr, libdrm, libxcb, mesa-libgbm, nspr, nss, pango

%description
Ghostex provides native AI development workspaces, terminals, and project tools.

%prep
mkdir -p %{_builddir}/ghostex-root
tar -xzf %{SOURCE0} -C %{_builddir}/ghostex-root

%install
mkdir -p %{buildroot}
cp -a %{_builddir}/ghostex-root/. %{buildroot}/

%files
/opt/ghostex
/usr/bin/ghostex
/usr/share/applications/ghostex.desktop
/usr/share/icons/hicolor/256x256/apps/ghostex.png
EOF
rpmbuild --define "_topdir $RPM_ROOT" -bb "$RPM_ROOT/SPECS/ghostex.spec"
RPM_BUILT="$(find "$RPM_ROOT/RPMS" -type f -name '*.rpm' -print -quit)"
[[ -n "$RPM_BUILT" ]] || { echo "rpmbuild produced no RPM" >&2; exit 1; }
RPM="$OUTPUT/ghostex-$VERSION-1.x86_64.rpm"
cp "$RPM_BUILT" "$RPM"
rpm -qpi "$RPM" >/dev/null

release_gpui_write_manifest "$OUTPUT" linux-x64 "$VERSION" "$DEB" "$RPM"
printf 'Built Linux x64 release payload in %s\n' "$OUTPUT"
