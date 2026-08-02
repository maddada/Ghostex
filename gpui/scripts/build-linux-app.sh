#!/usr/bin/env bash
# CDXC:GPUILinuxX11Backend 2026-07-04:
# Linux packaging skeleton for the GPUI app, mirroring the shape of
# build-windows-app.ps1: build the sidebar bundle, build both Rust binaries,
# then stage a flat CEF-conventional layout.
# CDXC:GPUILinuxX11Backend 2026-07-05: device-verified on Ubuntu 26.04 —
# builds, stages, and the staged app launches with working CEF sidebar and
# local gxserver. Deliberately not yet covered here (macOS-script parity
# items to port as Linux support matures): completion sound assets, CLI
# resources, portless admin runtime, updater integration, signing,
# desktop-entry/icon install, and package formats (deb/rpm/AppImage/flatpak).
# Also not yet staged: the Source code-server payload (<app>/code-server).
# Dev builds resolve the repo checkout at <repo>/code-server through the
# baked CARGO_MANIFEST_DIR candidate, so staging only matters for
# relocatable packages.
#
# Layout contract (all beside the executable, per CEF Linux conventions —
# libcef.so, its .so companions, .pak/.dat/.bin resources, and locales/ must
# live in the executable directory; the executable reaches libcef.so through
# the $ORIGIN rpath emitted by gpui/build.rs):
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
REPO_ROOT="$(cd "$GPUI_DIR/.." && pwd)"
APP_NAME="Ghostex"
APP_DIR="$GPUI_DIR/build/linux/$APP_NAME"
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
# (cef-dll-sys builds libcef_dll_wrapper), plus a Zig 0.15.x for
# libghostty-vt (GHOSTEX_ZIG override honored by gpui/build.rs).
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

# 4) Stage the app directory.
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR"

cp "$CARGO_OUTPUT_ROOT/release/ghostex-gpui" "$APP_DIR/Ghostex"
cp "$CARGO_OUTPUT_ROOT/release/ghostex-gpui-cef-helper" "$APP_DIR/"
cp -R "$CEF_PAYLOAD/." "$APP_DIR/"
# SDK build-support files are not runtime payload.
rm -rf "$APP_DIR/CMakeLists.txt" "$APP_DIR/cmake" "$APP_DIR/include" \
  "$APP_DIR/libcef_dll" "$APP_DIR/archive.json"
# no_sandbox runtime: the SUID sandbox helper stays out of the layout.
rm -f "$APP_DIR/chrome-sandbox"
mkdir -p "$APP_DIR/dist"
cp -R "$GPUI_DIR/dist/sidebar" "$APP_DIR/dist/sidebar"

# 5) Bundle the local gxserver package (bin/gxserver + zmx + node runtime),
# produced by gxserver-rs/package-remote-linux.mjs. The GPUI app resolves it
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
  echo "Build it first: bun gxserver-rs/package-remote-linux.mjs --arch $GXSERVER_ARCH" >&2
  exit 1
fi
cp -R "$GXSERVER_PACKAGE" "$APP_DIR/gxserver"

echo "Staged $APP_DIR"
