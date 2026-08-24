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
	"$REPO_ROOT/tooling/build-remote-gxserver-linux-release.sh" --arch x64
fi
GHOSTEX_ON_DEMAND_ASSETS=1 \
	GHOSTEX_GPUI_MARKETING_VERSION="$VERSION" \
	"$REPO_ROOT/apps/desktop/scripts/build-linux-app.sh"

APP_DIR="$REPO_ROOT/apps/desktop/build/linux/Ghostex"
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

rm -rf "$PACKAGE_ROOT"
mkdir -p \
	"$PACKAGE_ROOT/opt/ghostex" \
	"$PACKAGE_ROOT/usr/bin" \
	"$PACKAGE_ROOT/usr/share/applications" \
	"$PACKAGE_ROOT/usr/share/icons/hicolor/256x256/apps"
cp -a "$APP_DIR/." "$PACKAGE_ROOT/opt/ghostex/"
cp "$REPO_ROOT/apps/desktop/resources/AppIcon.appiconset/icon_256x256.png" \
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

printf 'Staged Linux x64 package root in %s\n' "$PACKAGE_ROOT"
