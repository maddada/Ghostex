#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
PACKAGE_ROOT="${2:-$REPO_ROOT/build/release-gpui/linux-package-root}"
release_gpui_require_version "$VERSION"
release_gpui_require_command bun
release_gpui_require_command cargo

if [[ "$(uname -m)" != "x86_64" ]]; then
  echo "Linux desktop release packaging is x64-only and requires an x86_64 runner." >&2
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

printf 'Staged Linux x64 package root in %s\n' "$PACKAGE_ROOT"
