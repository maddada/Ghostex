#!/usr/bin/env bash
# CDXC:PlatformSupport 2026-07-04:
# Linux packaging skeleton for the GPUI app, mirroring the shape of
# build-windows-app.ps1: build the sidebar bundle, build both Rust binaries,
# then stage a flat CEF-conventional layout.
# CDXC:PlatformSupport 2026-07-05: device-verified on Ubuntu 26.04 —
# builds, stages, and the staged app launches with working CEF sidebar and
# local gxserver. Deliberately not yet covered here (macOS-script parity
# items to port as Linux support matures): completion sound assets, CLI
# resources, portless admin runtime, updater integration, signing,
# desktop-entry/icon install, and package formats (deb/rpm/AppImage/flatpak).
# Source is an on-demand code-server component in relocatable packages. Dev
# builds resolve the repository checkout through the baked CARGO_MANIFEST_DIR
# candidate instead.
#
# Development layouts keep CEF beside the executable. Release layouts stage a
# CEF-free native bootstrap plus the internal runtime; the bootstrap installs
# the sealed component and starts that runtime with the component directory on
# LD_LIBRARY_PATH.
#   build/linux/Ghostex/
#     Ghostex
#     ghostex-gpui-cef-helper          <- cef/linux_x11.rs sets this as
#                                         browser_subprocess_path (sibling)
#     libcef.so, libEGL.so, ...        <- CEF Release/ payload
#     icudtl.dat, *.pak, *.bin,
#     locales/                         <- CEF Resources/ payload
#     dist/sidebar/                    <- sidebar bundle; the /dist/sidebar/
#                                         path segment is load-bearing for the
#                                         CEF helper first-party URL check and
#                                         the sidebar_url() Linux arm.
#
# Runtime notes:
# - The app forces X11 app-wide (XWayland on Wayland desktops) and appends
#   --ozone-platform=x11 to Chromium itself (cef/linux_x11.rs); no launcher
#   flags are needed.
# - CEF's SUID chrome-sandbox binary is intentionally not staged: the app
#   initializes CEF with no_sandbox, matching the macOS/Windows builds.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GPUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$GPUI_DIR/../.." && pwd)"
APP_NAME="Ghostex"
APP_DIR="$GPUI_DIR/build/linux/$APP_NAME"
ON_DEMAND_COMPONENTS="${GHOSTEX_ON_DEMAND_ASSETS:-0}"
RELEASE_VERSION="${GHOSTEX_GPUI_MARKETING_VERSION:-$(node -p "require('$REPO_ROOT/package.json').version")}"
CARGO_OUTPUT_ROOT="${CARGO_TARGET_DIR:-$GPUI_DIR/target}"
if [[ "$CARGO_OUTPUT_ROOT" != /* ]]; then
	CARGO_OUTPUT_ROOT="$GPUI_DIR/$CARGO_OUTPUT_ROOT"
fi

# Same CEF cache location contract as build-macos-app.sh / the Windows
# script: cef-dll-sys's build script downloads the CEF binary distribution
# into CEF_PATH. Honor an explicit location so WSL builds can keep the CEF
# archive on the Linux filesystem, where unpacking can preserve timestamps.
export CEF_PATH="${CEF_PATH:-$GPUI_DIR/build/cef-cache}"

# 1) Sidebar bundle (same steps as the macOS script).
(
	cd "$REPO_ROOT"
	bun run build:sidebar-css
	bunx vite build --config "$GPUI_DIR/vite.config.ts"
)

# 2) Rust binaries (main app + CEF helper). Requires cmake and ninja
# (cef-dll-sys builds libcef_dll_wrapper), plus Zig 0.16.x for
# libghostty-vt (GHOSTEX_ZIG override honored by apps/desktop/build.rs).
(
	cd "$GPUI_DIR"
	cargo build --release --bins
)

# 3) Locate the extracted CEF distribution. Unlike the macOS bundle layout,
# download-cef extracts the Linux minimal distribution FLAT (verified on
# device 2026-07-05): binaries, .pak/.dat/.bin resources, and locales/ all
# sit directly in $CEF_PATH/<cef-version>/cef_linux_<arch>/ with no Release/
# or Resources/ subdirectories, alongside SDK-only build support
# (CMakeLists.txt, cmake/, include/, libcef_dll/, archive.json).
CEF_PAYLOAD=""
while IFS= read -r candidate; do
	CEF_PAYLOAD="$(dirname "$candidate")"
	break
done < <(find "$CEF_PATH" -type f -name libcef.so 2>/dev/null)
if [[ -z "$CEF_PAYLOAD" ]]; then
	echo "cef-rs did not produce libcef.so under $CEF_PATH" >&2
	exit 1
fi
if [[ ! -f "$CEF_PAYLOAD/icudtl.dat" ]]; then
	echo "CEF payload directory $CEF_PAYLOAD is missing icudtl.dat" >&2
	exit 1
fi

CEF_COMPONENT_VERSION="$(sed -n 's/^#define CEF_VERSION "\([^"]*\)"$/\1/p' "$CEF_PAYLOAD/include/cef_version.h" | head -n 1 | sed 's/[^A-Za-z0-9._-]/-/g')"
if [[ -z "$CEF_COMPONENT_VERSION" ]]; then
	echo "Could not resolve the CEF component version from $CEF_PAYLOAD/include/cef_version.h" >&2
	exit 1
fi
CEF_COMPONENT_ARCH="x64"
if [[ "$(uname -m)" == "aarch64" ]]; then
	CEF_COMPONENT_ARCH="arm64"
fi

prepare_cef_component() {
	local component_root asset_dir component_manifest stage_root archive_path build_manifest
	local code_server_archive code_server_component_version code_server_asset_dir expected_code_server_archive
	component_root="${GHOSTEX_ON_DEMAND_COMPONENT_ROOT:-$REPO_ROOT/build/on-demand-components}"
	asset_dir="${GHOSTEX_ON_DEMAND_COMPONENT_ASSET_DIR:-$component_root/assets}"
	component_manifest="${GHOSTEX_ON_DEMAND_COMPONENTS_MANIFEST:-$component_root/components.json}"
	stage_root="$(mktemp -d "$GPUI_DIR/build/cef-linux-component-XXXXXX")"
	archive_path="$asset_dir/cef-$CEF_COMPONENT_VERSION-linux-$CEF_COMPONENT_ARCH.tar.gz"
	mkdir -p "$asset_dir"
	printf '{"components":{}}\n' >"$component_manifest"
	cp -R "$CEF_PAYLOAD/." "$stage_root/"
	rm -rf "$stage_root/CMakeLists.txt" "$stage_root/cmake" "$stage_root/include" \
		"$stage_root/libcef_dll" "$stage_root/archive.json"
	rm -f "$stage_root/chrome-sandbox"
	"$REPO_ROOT/tooling/release-gpui/create-deterministic-tar.sh" "$stage_root" "$archive_path"
	rm -rf "$stage_root"
	node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs" \
		--metadata-only \
		--component cef \
		--version "$CEF_COMPONENT_VERSION" \
		--asset-dir "$asset_dir" \
		--output "$component_manifest"
	code_server_archive="${GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_X64_ARCHIVE:-}"
	if [[ -n "$code_server_archive" ]]; then
		code_server_component_version="${GHOSTEX_CODE_SERVER_COMPONENT_VERSION:-}"
		[[ -n "$code_server_component_version" ]] || {
			echo "GHOSTEX_CODE_SERVER_COMPONENT_VERSION is required with the Linux Source component archive" >&2
			exit 1
		}
		expected_code_server_archive="code-server-$code_server_component_version-linux-x64.tar.gz"
		[[ "$(basename "$code_server_archive")" == "$expected_code_server_archive" ]] || {
			echo "Linux Source component identity mismatch: expected $expected_code_server_archive" >&2
			exit 1
		}
		[[ -f "$code_server_archive" && -f "$code_server_archive.sha256" ]] || {
			echo "Linux Source component archive or checksum sidecar is missing: $code_server_archive" >&2
			exit 1
		}
		node "$REPO_ROOT/tooling/release-gpui/verify-code-server-archive.mjs" \
			--archive "$code_server_archive" \
			--version "$code_server_component_version" \
			--platform linux-x64
		code_server_asset_dir="$(dirname "$code_server_archive")"
		node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs" \
			--metadata-only \
			--require-sha256-sidecars \
			--component code-server \
			--version "$code_server_component_version" \
			--asset-dir "$code_server_asset_dir" \
			--output "$component_manifest"
	fi
	build_manifest="$component_root/linux-$CEF_COMPONENT_ARCH-assets.json"
	node -e 'const fs=require("node:fs");fs.writeFileSync(process.argv[1],JSON.stringify({assets:[],version:process.argv[2]},null,2)+"\n")' \
		"$build_manifest" "$RELEASE_VERSION"
	mkdir -p "$APP_DIR/resources"
	node "$REPO_ROOT/tooling/release-gpui/on-demand-manifest.mjs" seal \
		--build-manifest "$build_manifest" \
		--component-manifest "$component_manifest" \
		--output "$APP_DIR/resources/on-demand-resources.json" \
		--repo maddada/Ghostex
}

# 4) Stage the app directory.
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR"

if [[ "$ON_DEMAND_COMPONENTS" == "1" ]]; then
	cp "$CARGO_OUTPUT_ROOT/release/ghostex-gpui-cef-bootstrap" "$APP_DIR/Ghostex"
	cp "$CARGO_OUTPUT_ROOT/release/ghostex-gpui" "$APP_DIR/ghostex-gpui-runtime"
else
	cp "$CARGO_OUTPUT_ROOT/release/ghostex-gpui" "$APP_DIR/Ghostex"
fi
cp "$CARGO_OUTPUT_ROOT/release/ghostex-gpui-cef-helper" "$APP_DIR/"
if [[ "$ON_DEMAND_COMPONENTS" == "1" ]]; then
	prepare_cef_component
else
	cp -R "$CEF_PAYLOAD/." "$APP_DIR/"
	# SDK build-support files are not runtime payload.
	rm -rf "$APP_DIR/CMakeLists.txt" "$APP_DIR/cmake" "$APP_DIR/include" \
		"$APP_DIR/libcef_dll" "$APP_DIR/archive.json"
	# no_sandbox runtime: the SUID sandbox helper stays out of the layout.
	rm -f "$APP_DIR/chrome-sandbox"
fi
mkdir -p "$APP_DIR/dist"
cp -R "$GPUI_DIR/dist/sidebar" "$APP_DIR/dist/sidebar"

# 5) Bundle the local gxserver package (bin/gxserver + zmx + node runtime),
# produced by server/package-remote-linux.mjs. The GPUI app resolves it
# at <executable dir>/gxserver/bin/gxserver in this flat layout
# (gpui_resolve_local_gxserver_binary), matching macOS's bundled
# Contents/Resources/Web/gxserver.
GXSERVER_ARCH="x64"
if [[ "$(uname -m)" == "aarch64" ]]; then
	GXSERVER_ARCH="arm64"
fi
GXSERVER_PACKAGE="$REPO_ROOT/build/remote-gxserver-linux/$GXSERVER_ARCH/package"
if [[ ! -x "$GXSERVER_PACKAGE/bin/gxserver" ]]; then
	echo "gxserver package not found at $GXSERVER_PACKAGE." >&2
	echo "Build it first: bun server/package-remote-linux.mjs --arch $GXSERVER_ARCH" >&2
	exit 1
fi
cp -R "$GXSERVER_PACKAGE" "$APP_DIR/gxserver"

echo "Staged $APP_DIR"
