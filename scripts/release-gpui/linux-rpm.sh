#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" linux-rpm-x64)}"
release_gpui_require_version "$VERSION"
release_gpui_require_command rpmbuild
release_gpui_require_command rpm
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

PACKAGE_ROOT="$REPO_ROOT/build/release-gpui/linux-rpm-package-root"
"$SCRIPT_DIR/linux-stage.sh" "$VERSION" "$PACKAGE_ROOT"
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
[[ -n "$RPM_BUILT" ]] || { echo "rpmbuild produced no RPM" >&2; exit 1; }
RPM="$OUTPUT/ghostex-$VERSION-1.x86_64.rpm"
cp "$RPM_BUILT" "$RPM"
rpm -qpi "$RPM" >/dev/null
release_gpui_write_manifest "$OUTPUT" linux-rpm-x64 "$VERSION" "$RPM"
printf 'Built Fedora/RHEL x64 release payload in %s\n' "$OUTPUT"
