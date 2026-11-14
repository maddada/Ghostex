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
# CI sets RUSTC_WRAPPER to kache (kache-action in release-gpui-linux.yml). cargo does not fall
# back when the wrapper is missing from PATH; it fails the compile after the
# sidebar bundle has already been built. Fail here instead, by name.
[[ -z "${RUSTC_WRAPPER:-}" ]] || release_gpui_require_command "$RUSTC_WRAPPER"

LINUX_ARCH="$(release_gpui_linux_arch)"
LINUX_ARCH_UPPER="$(printf '%s' "$LINUX_ARCH" | tr '[:lower:]' '[:upper:]')"

"$SCRIPT_DIR/prepare-references.sh"
if [[ ! -x "$REPO_ROOT/build/remote-gxserver-linux/$LINUX_ARCH/package/bin/gxserver" ]]; then
	GHOSTEX_GPUI_MARKETING_VERSION="$VERSION" \
		"$REPO_ROOT/tooling/build-remote-gxserver-linux-release.sh" --arch "$LINUX_ARCH"
fi
CODE_SERVER_COMPONENT_VERSION="${GHOSTEX_CODE_SERVER_COMPONENT_VERSION:-$(node "$SCRIPT_DIR/code-server-component-identity.mjs" --root "$REPO_ROOT/.dependencies/code-server")}"
CODE_SERVER_ARCHIVE_ENV="GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_${LINUX_ARCH_UPPER}_ARCHIVE"
CODE_SERVER_ARCHIVE="${!CODE_SERVER_ARCHIVE_ENV:-$REPO_ROOT/build/runtime-artifacts/code-server-$LINUX_ARCH/code-server-$CODE_SERVER_COMPONENT_VERSION-linux-$LINUX_ARCH.tar.gz}"
[[ -f "$CODE_SERVER_ARCHIVE" && -f "$CODE_SERVER_ARCHIVE.sha256" ]] || {
	echo "Linux release packaging requires the verified Source component archive: $CODE_SERVER_ARCHIVE" >&2
	exit 1
}
export "$CODE_SERVER_ARCHIVE_ENV=$CODE_SERVER_ARCHIVE"
GHOSTEX_CODE_SERVER_COMPONENT_VERSION="$CODE_SERVER_COMPONENT_VERSION" \
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
# CDXC:CloudBoxes 2026-11-02 WHY:
# Every Linux package ships the gxserver package's prebuilt tailcat (pinned in
# server/package-remote-linux.mjs) so a Linux desktop dials Cloud Boxes and
# Easy Connect machines without installing one (the user approved the ~20 MB).
# The desktop app finds it beside this gxserver (easy_connect_forward.rs).
[[ -x "$APP_DIR/gxserver/bin/tailcat" ]] || {
	echo "Linux build is missing the bundled tailcat in its gxserver package (rebuild it with server/package-remote-linux.mjs)" >&2
	exit 1
}
[[ -f "$APP_DIR/gxserver/skills/ghostex-cli/SKILL.md" ]] || {
	echo "Linux build is missing the bundled agent skills in its gxserver package (rebuild it with server/package-remote-linux.mjs)" >&2
	exit 1
}
for sound in arcade.mp3 copy.mp3; do
	[[ -f "$APP_DIR/resources/sounds/$sound" ]] || {
		echo "Linux build is missing the bundled sound resources/sounds/$sound" >&2
		exit 1
	}
done
# CDXC:Release 2026-11-14 SEE-ALSO:
# The CEF and Source component publishing lives in linux-publish-components.sh. A release that
# defers component signatures (GHOSTEX_COMPONENT_SIGNATURES_DEFERRED=1, CDXC:Release 2026-11-14 in
# publish-component.mjs) runs it as a step of its own that holds the signing key and builds nothing;
# everything else runs it here, in its old place.
if [[ "${GHOSTEX_COMPONENT_SIGNATURES_DEFERRED:-0}" != "1" ]]; then
	"$SCRIPT_DIR/linux-publish-components.sh" "$CODE_SERVER_COMPONENT_VERSION" "$CODE_SERVER_ARCHIVE"
fi

rm -rf "$PACKAGE_ROOT"
mkdir -p \
	"$PACKAGE_ROOT/opt/ghostex" \
	"$PACKAGE_ROOT/usr/bin" \
	"$PACKAGE_ROOT/usr/share/applications" \
	"$PACKAGE_ROOT/usr/share/icons/hicolor/256x256/apps"
cp -a "$APP_DIR/." "$PACKAGE_ROOT/opt/ghostex/"
cp "$REPO_ROOT/apps/desktop/resources/AppIcon.appiconset/icon_256x256.png" \
	"$PACKAGE_ROOT/usr/share/icons/hicolor/256x256/apps/ghostex.png"
# CDXC:Cli 2026-10-26 SEE-ALSO:
# The desktop app recognises this script as Ghostex's own CLI only byte for
# byte (GPUI_LINUX_PACKAGE_CLI_WRAPPER in
# apps/desktop/src/app/helpers/os_cli/cli_install.rs); change both together.
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
MimeType=x-scheme-handler/ghostex;
EOF

printf 'Staged Linux %s package root in %s\n' "$LINUX_ARCH" "$PACKAGE_ROOT"
