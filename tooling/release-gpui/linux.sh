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
release_gpui_require_command zstd
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

if [[ "$(uname -m)" != "x86_64" ]]; then
	echo "Linux release packaging is x64-only and requires an x86_64 runner." >&2
	exit 1
fi

"$SCRIPT_DIR/prepare-references.sh"
if [[ ! -x "$REPO_ROOT/build/remote-gxserver-linux/x64/package/bin/gxserver" ]]; then
	"$REPO_ROOT/tooling/build-remote-gxserver-linux-release.sh" --arch x64
fi
GHOSTEX_ON_DEMAND_ASSETS=1 \
	GHOSTEX_GPUI_MARKETING_VERSION="$VERSION" \
	"$REPO_ROOT/gpui/scripts/build-linux-app.sh"

APP_DIR="$REPO_ROOT/gpui/build/linux/Ghostex"
[[ -x "$APP_DIR/Ghostex" ]] || {
	echo "Linux build is missing Ghostex" >&2
	exit 1
}
[[ -x "$APP_DIR/ghostex-gpui-runtime" ]] || {
	echo "Linux build is missing its internal GPUI runtime" >&2
	exit 1
}
[[ ! -e "$APP_DIR/libcef.so" ]] || {
	echo "Linux release build still bundles libcef.so" >&2
	exit 1
}
[[ -x "$APP_DIR/gxserver/bin/gxserver" ]] || {
	echo "Linux build is missing bundled gxserver" >&2
	exit 1
}
[[ -x "$APP_DIR/gxserver/bin/ghostex" ]] || {
	echo "Linux build is missing bundled ghostex CLI" >&2
	exit 1
}
ON_DEMAND_MANIFEST="$APP_DIR/resources/on-demand-resources.json"
CEF_COMPONENT_VERSION="$(node -e '
const fs = require("node:fs");
const manifest = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const component = manifest.components?.cef;
const asset = component?.platforms?.["linux-x64"];
if (!component?.componentVersion || component.downloadTag !== `cef-${component.componentVersion}` || !/^[0-9a-f]{64}$/.test(asset?.sha256 ?? "")) process.exit(1);
process.stdout.write(component.componentVersion);
' "$ON_DEMAND_MANIFEST")" || {
	echo "Linux release build has an invalid sealed CEF component manifest" >&2
	exit 1
}
node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs" \
	--component cef \
	--version "$CEF_COMPONENT_VERSION" \
	--asset-dir "$REPO_ROOT/build/on-demand-components/assets" \
	--output "$REPO_ROOT/build/on-demand-components/components.json"

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
exec /opt/ghostex/gxserver/bin/ghostex "$@"
EOF
chmod 755 "$PACKAGE_ROOT/usr/bin/ghostex"
ln -s ghostex "$PACKAGE_ROOT/usr/bin/gx"
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
StartupWMClass=ghostex
EOF

mkdir -p "$PACKAGE_ROOT/DEBIAN"
cat >"$PACKAGE_ROOT/DEBIAN/control" <<EOF
Package: ghostex
Version: $VERSION
Section: devel
Priority: optional
Architecture: amd64
Maintainer: Ghostex <support@ghostex.app>
Depends: libasound2t64 | libasound2, libatk-bridge2.0-0t64 | libatk-bridge2.0-0, libatk1.0-0t64 | libatk1.0-0, libatspi2.0-0t64 | libatspi2.0-0, libc6, libcairo2, libcups2t64 | libcups2, libdbus-1-3, libdrm2, libexpat1, libfontconfig1, libgbm1, libglib2.0-0t64 | libglib2.0-0, libgtk-3-0t64 | libgtk-3-0, libnspr4, libnss3, libpango-1.0-0, libpangocairo-1.0-0, libx11-6, libx11-xcb1, libxcb1, libxcomposite1, libxdamage1, libxext6, libxfixes3, libxkbcommon0, libxrandr2, libxshmfence1, wmctrl
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
Requires: alsa-lib, atk, cairo, cups-libs, dbus-libs, expat, fontconfig, gtk3, libX11, libXcomposite, libXdamage, libXext, libXfixes, libXrandr, libdrm, libxcb, mesa-libgbm, nspr, nss, pango, wmctrl

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
/usr/bin/gx
/usr/share/applications/ghostex.desktop
/usr/share/icons/hicolor/256x256/apps/ghostex.png
EOF
rpmbuild --define "_topdir $RPM_ROOT" -bb "$RPM_ROOT/SPECS/ghostex.spec"
RPM_BUILT="$(find "$RPM_ROOT/RPMS" -type f -name '*.rpm' -print -quit)"
[[ -n "$RPM_BUILT" ]] || {
	echo "rpmbuild produced no RPM" >&2
	exit 1
}
RPM="$OUTPUT/ghostex-$VERSION-1.x86_64.rpm"
cp "$RPM_BUILT" "$RPM"
rpm -qpi "$RPM" >/dev/null

# Prefix-preserving portable tarball for Arch and anything installing straight
# from the GitHub release (ubi, mise, the AUR package). Same rules as
# linux-tar.sh: DEBIAN/ is already gone, symlinks stay symlinks (no `-h`), and
# mtimes are normalized through tar rather than by rewriting the staged tree.
TARBALL="$OUTPUT/ghostex-${VERSION}-linux-x64.tar.zst"
TAR_FILE_LIST="$(mktemp)"
trap 'rm -f "$TAR_FILE_LIST"' EXIT
(
	cd "$PACKAGE_ROOT"
	find . -mindepth 1 -print0 | LC_ALL=C sort -z >"$TAR_FILE_LIST"
	tar --format=gnu \
		--owner=0 --group=0 --numeric-owner \
		--mtime=@946684800 \
		--no-recursion --null --files-from "$TAR_FILE_LIST" -cf - |
		zstd -19 -T0 -q -f -o "$TARBALL" -
)
TAR_MEMBERS="$(zstd -dc "$TARBALL" | tar -tf -)"
if grep -q '^\./DEBIAN' <<<"$TAR_MEMBERS"; then
	echo "Linux tarball leaked DEBIAN/ control metadata" >&2
	exit 1
fi

release_gpui_write_manifest "$OUTPUT" linux-x64 "$VERSION" "$DEB" "$RPM" "$TARBALL"
printf 'Built Linux x64 release payload in %s\n' "$OUTPUT"
