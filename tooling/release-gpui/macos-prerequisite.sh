#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
COMPONENT="${1:-}"
OUTPUT="${2:-}"

if [[ -z "$COMPONENT" || -z "$OUTPUT" ]]; then
  echo "Usage: macos-prerequisite.sh ghosttykit|runtime|rust <output.tar>" >&2
  exit 2
fi

mkdir -p "$(dirname "$OUTPUT")"
rm -f "$OUTPUT"

case "$COMPONENT" in
  ghosttykit)
    GHOSTTY_ROOT="$REPO_ROOT/.dependencies/ghostty"
    GHOSTTY_ZIG="${GHOSTEX_ZIG:-${ZIG:-}}"
    [[ -x "$GHOSTTY_ZIG" ]] || { echo "Zig 0.16.0 is required to build GhosttyKit" >&2; exit 1; }
    [[ "$("$GHOSTTY_ZIG" version)" == "0.16.0" ]] || { echo "GhosttyKit requires Zig 0.16.0" >&2; exit 1; }
    DEVELOPER_DIR="$(xcode-select -p)"
    SDKROOT="$(DEVELOPER_DIR="$DEVELOPER_DIR" xcrun --sdk macosx --show-sdk-path)"
    (
      cd "$GHOSTTY_ROOT"
      env DEVELOPER_DIR="$DEVELOPER_DIR" SDKROOT="$SDKROOT" \
        GHOSTTY_METAL_DEVELOPER_DIR="$DEVELOPER_DIR" \
        "$GHOSTTY_ZIG" build -Demit-xcframework -Dxcframework-target=universal -Demit-macos-app=false
    )
    SLICE=".dependencies/ghostty/macos/GhosttyKit.xcframework/macos-arm64_x86_64"
    [[ -f "$REPO_ROOT/$SLICE/ghostty-internal.a" && -f "$REPO_ROOT/$SLICE/Headers/ghostty.h" ]] || {
      echo "GhosttyKit macOS slice is incomplete" >&2
      exit 1
    }
    tar -cf "$OUTPUT" -C "$REPO_ROOT" "$SLICE"
    ;;
  runtime)
    VERSION="${GHOSTEX_RELEASE_VERSION:-}"
    release_gpui_require_version "$VERSION"
    REMOTE_ROOT="$REPO_ROOT/build/remote-gxserver-linux"
    CODE_SERVER_COMPONENT_VERSION="$(node "$SCRIPT_DIR/code-server-component-identity.mjs" --root "$REPO_ROOT/.dependencies/code-server")"
    CODE_SERVER_LINUX_X64_ARCHIVE="${GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_X64_ARCHIVE:-$REPO_ROOT/build/runtime-artifacts/code-server-x64/code-server-$CODE_SERVER_COMPONENT_VERSION-linux-x64.tar.gz}"
    CODE_SERVER_LINUX_ARM64_ARCHIVE="${GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_ARM64_ARCHIVE:-$REPO_ROOT/build/runtime-artifacts/code-server-arm64/code-server-$CODE_SERVER_COMPONENT_VERSION-linux-arm64.tar.gz}"
    [[ -f "$CODE_SERVER_LINUX_X64_ARCHIVE" ]] || { echo "macOS runtime preparation requires Linux x64 code-server archive: $CODE_SERVER_LINUX_X64_ARCHIVE" >&2; exit 1; }
    [[ -f "$CODE_SERVER_LINUX_ARM64_ARCHIVE" ]] || { echo "macOS runtime preparation requires Linux arm64 code-server archive: $CODE_SERVER_LINUX_ARM64_ARCHIVE" >&2; exit 1; }
    GHOSTEX_MACOS_ARCH=arm64 \
    GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES=0 \
    GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1 \
    GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE="$REMOTE_ROOT/x64/package" \
    GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE="$REMOTE_ROOT/arm64/package" \
    GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_X64_ARCHIVE="$CODE_SERVER_LINUX_X64_ARCHIVE" \
    GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_ARM64_ARCHIVE="$CODE_SERVER_LINUX_ARM64_ARCHIVE" \
    GHOSTEX_CODE_SERVER_COMPONENT_VERSION="$CODE_SERVER_COMPONENT_VERSION" \
    GHOSTEX_ON_DEMAND_ASSETS=1 \
    GHOSTEX_REQUIRE_BEADS_SMOKE=1 \
    GHOSTEX_CODE_SIGN_IDENTITY="${GHOSTEX_CODE_SIGN_IDENTITY:-Developer ID Application: Mohamad Youssef (KTKP595G3B)}" \
    GHOSTEX_CODE_SIGN_TIMESTAMP_FLAG=--timestamp \
      "$REPO_ROOT/apps/desktop/scripts/prepare-macos-runtime.sh"
    for required_path in \
      "apps/desktop/runtime/macos/Web/on-demand-resources.json" \
      "apps/desktop/runtime/macos/Web/gxserver/bin/gxserver" \
      "apps/desktop/runtime/macos/CLI/ghostex" \
      "build/on-demand-components/components.json" \
      "build/on-demand-assets/$VERSION/gxserver-linux-x64.tar.gz" \
      "build/on-demand-assets/$VERSION/gxserver-linux-arm64.tar.gz" \
      "build/on-demand-assets/$VERSION/bd-darwin-arm64.tar.gz"; do
      [[ -e "$REPO_ROOT/$required_path" ]] || { echo "Prepared runtime is missing $required_path" >&2; exit 1; }
    done
    node "$REPO_ROOT/tooling/release-gpui/on-demand-manifest.mjs" validate-macos \
      --manifest "$REPO_ROOT/apps/desktop/runtime/macos/Web/on-demand-resources.json"
    tar -cf "$OUTPUT" -C "$REPO_ROOT" \
      apps/desktop/runtime/macos \
      "build/on-demand-assets/$VERSION" \
      build/on-demand-components
    ;;
  rust)
    export CEF_PATH="$REPO_ROOT/apps/desktop/build/cef-cache"
    (
      cd "$REPO_ROOT/gpui"
      cargo build --release --bins
    )
    for binary_path in apps/desktop/target/release/ghostex-gpui apps/desktop/target/release/ghostex-gpui-cef-helper; do
      [[ -x "$REPO_ROOT/$binary_path" ]] || { echo "Rust build is missing $binary_path" >&2; exit 1; }
      /usr/bin/lipo -archs "$REPO_ROOT/$binary_path" | tr ' ' '\n' | grep -Fxq arm64 || {
        echo "Rust build produced a non-arm64 binary: $binary_path" >&2
        exit 1
      }
    done
    CEF_FRAMEWORK="$(find "$CEF_PATH" -path '*/Chromium Embedded Framework.framework' -type d -print -quit)"
    [[ -d "$CEF_FRAMEWORK" ]] || { echo "Rust build did not produce the CEF framework" >&2; exit 1; }
    CEF_VERSION_HEADER="$(dirname "$CEF_FRAMEWORK")/include/cef_version.h"
    [[ -f "$CEF_VERSION_HEADER" ]] || { echo "Rust build did not produce $CEF_VERSION_HEADER" >&2; exit 1; }
    CEF_RELATIVE="${CEF_FRAMEWORK#"$REPO_ROOT/"}"
    CEF_VERSION_HEADER_RELATIVE="${CEF_VERSION_HEADER#"$REPO_ROOT/"}"
    tar -cf "$OUTPUT" -C "$REPO_ROOT" \
      apps/desktop/target/release/ghostex-gpui \
      apps/desktop/target/release/ghostex-gpui-cef-helper \
      "$CEF_RELATIVE" \
      "$CEF_VERSION_HEADER_RELATIVE"
    ;;
  *)
    echo "Unknown macOS prerequisite: $COMPONENT" >&2
    exit 2
    ;;
esac

shasum -a 256 "$OUTPUT"
