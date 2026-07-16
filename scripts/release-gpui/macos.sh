#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" macos-arm64)}"
release_gpui_require_version "$VERSION"
release_gpui_require_command bun
release_gpui_require_command cargo
release_gpui_require_command codesign
release_gpui_require_command hdiutil
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

BUILD_NUMBER="$(release_gpui_build_number "$VERSION")"
SIGNING_IDENTITY="${GHOSTEX_CODE_SIGN_IDENTITY:-Developer ID Application: Mohamad Youssef (KTKP595G3B)}"
NOTARY_PROFILE="${GHOSTEX_NOTARY_PROFILE:-notarytool-profile}"
UPDATE_SPARKLE="${GHOSTEX_RELEASE_UPDATE_SPARKLE:-1}"
SPARKLE_ROOT="$($SCRIPT_DIR/prepare-sparkle.sh)"
SPARKLE_FRAMEWORK="$SPARKLE_ROOT/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework"
BEADS_ROOT="$(cd "$REPO_ROOT/../.." && pwd)/_references/beads"
REMOTE_ROOT="$REPO_ROOT/build/remote-gxserver-linux"

"$SCRIPT_DIR/prepare-references.sh"
if [[ ! -x "$REMOTE_ROOT/x64/package/bin/gxserver" || ! -x "$REMOTE_ROOT/arm64/package/bin/gxserver" ]]; then
  "$REPO_ROOT/scripts/build-remote-gxserver-linux-release.sh" --arch all
fi

GHOSTTY_ROOT="$REPO_ROOT/ghostty"
GHOSTTY_KIT="$GHOSTTY_ROOT/macos/GhosttyKit.xcframework"
if [[ ! -d "$GHOSTTY_KIT" ]]; then
  GHOSTTY_ZIG="${ZIG:-${GHOSTEX_ZIG:-}}"
  [[ -x "$GHOSTTY_ZIG" ]] || { echo "Zig 0.15.2 is required to build GhosttyKit" >&2; exit 1; }
  [[ "$("$GHOSTTY_ZIG" version)" == "0.15.2" ]] || { echo "GhosttyKit requires Zig 0.15.2" >&2; exit 1; }
  GHOSTTY_DEVELOPER_DIR="$(xcode-select -p)"
  GHOSTTY_SDKROOT="$(DEVELOPER_DIR="$GHOSTTY_DEVELOPER_DIR" xcrun --sdk macosx --show-sdk-path)"
  (
    cd "$GHOSTTY_ROOT"
    env \
      DEVELOPER_DIR="$GHOSTTY_DEVELOPER_DIR" \
      SDKROOT="$GHOSTTY_SDKROOT" \
      GHOSTTY_METAL_DEVELOPER_DIR="$GHOSTTY_DEVELOPER_DIR" \
      "$GHOSTTY_ZIG" build \
        -Demit-xcframework \
        -Dxcframework-target=universal \
        -Demit-macos-app=false
  )
fi
[[ -d "$GHOSTTY_KIT" ]] || { echo "GhosttyKit build did not produce $GHOSTTY_KIT" >&2; exit 1; }

# Prepare the GPUI-owned runtime tree and seal the on-demand checksums without
# invoking the retired Swift host build.
GHOSTEX_MACOS_ARCH=arm64 \
GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES=0 \
GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1 \
GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE="$REMOTE_ROOT/x64/package" \
GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE="$REMOTE_ROOT/arm64/package" \
GHOSTEX_ON_DEMAND_ASSETS=1 \
GHOSTEX_CODE_SIGN_IDENTITY="$SIGNING_IDENTITY" \
GHOSTEX_CODE_SIGN_TIMESTAMP_FLAG=--timestamp \
BEADS_ROOT="$BEADS_ROOT" \
  "$REPO_ROOT/gpui/scripts/prepare-macos-runtime.sh"

GHOSTEX_MACOS_ARCH=arm64 \
GHOSTEX_GPUI_APP_NAME=Ghostex \
GHOSTEX_GPUI_BUNDLE_ID=com.madda.ghostex.host \
GHOSTEX_GPUI_MARKETING_VERSION="$VERSION" \
GHOSTEX_GPUI_BUILD_VERSION="$BUILD_NUMBER" \
GHOSTEX_GPUI_SPARKLE_FEED_URL=https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml \
GHOSTEX_GPUI_SPARKLE_FRAMEWORK="$SPARKLE_FRAMEWORK" \
GHOSTEX_REQUIRE_SPARKLE=1 \
GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1 \
GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE="$REMOTE_ROOT/x64/package" \
GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE="$REMOTE_ROOT/arm64/package" \
GHOSTEX_ON_DEMAND_ASSETS=1 \
GHOSTEX_GPUI_SIGN_IDENTITY="$SIGNING_IDENTITY" \
GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG=--timestamp \
  "$REPO_ROOT/gpui/scripts/build-macos-app.sh"

APP_PATH="$REPO_ROOT/gpui/build/macos/Ghostex.app"
INFO_PLIST="$APP_PATH/Contents/Info.plist"
[[ -d "$APP_PATH" ]] || { echo "GPUI build did not produce $APP_PATH" >&2; exit 1; }
[[ "$(plutil -extract CFBundleIdentifier raw "$INFO_PLIST")" == "com.madda.ghostex.host" ]]
[[ "$(plutil -extract CFBundleName raw "$INFO_PLIST")" == "Ghostex" ]]
[[ "$(plutil -extract CFBundleExecutable raw "$INFO_PLIST")" == "Ghostex" ]]
[[ "$(plutil -extract CFBundleShortVersionString raw "$INFO_PLIST")" == "$VERSION" ]]
[[ "$(plutil -extract CFBundleVersion raw "$INFO_PLIST")" == "$BUILD_NUMBER" ]]
[[ "$(plutil -extract SUFeedURL raw "$INFO_PLIST")" == "https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml" ]]
codesign --verify --deep --strict --verbose=2 "$APP_PATH"
node --input-type=module -e \
  'import { validateMacosAppBundle } from "./scripts/validate-macos-app-bundle.mjs"; await validateMacosAppBundle({ appName: "Ghostex", appPath: process.argv[1], arch: "arm64" });' \
  "$APP_PATH"

STAGE="$(mktemp -d "$REPO_ROOT/build/release-gpui/macos-stage-XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP_PATH" "$STAGE/Ghostex.app"
ln -s /Applications "$STAGE/Applications"
DMG="$OUTPUT/ghostex-$VERSION-arm64.dmg"
hdiutil create -volname Ghostex -srcfolder "$STAGE" -format UDZO "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"

if [[ "$UPDATE_SPARKLE" == "1" ]]; then
  APPCAST_WORK="$(mktemp -d "$REPO_ROOT/build/release-gpui/appcast-stage-XXXXXX")"
  cp "$REPO_ROOT/appcast.xml" "$APPCAST_WORK/appcast.xml"
  cp "$DMG" "$APPCAST_WORK/$(basename "$DMG")"
  VERSION="$VERSION" CHANGELOG_PATH="$REPO_ROOT/CHANGELOG.md" NOTES_PATH="$APPCAST_WORK/ghostex-$VERSION-arm64.md" node <<'JS'
const { readFileSync, writeFileSync } = require("node:fs");
const version = process.env.VERSION;
const changelog = readFileSync(process.env.CHANGELOG_PATH, "utf8");
const start = changelog.indexOf(`## ${version} -`);
if (start < 0) throw new Error(`CHANGELOG.md has no ${version} section`);
const next = changelog.indexOf("\n## ", start + 4);
const section = changelog.slice(start, next < 0 ? undefined : next).trim();
writeFileSync(process.env.NOTES_PATH, `# Ghostex ${version}\n\n${section}\n`);
JS

  GENERATE_ARGS=(
    --download-url-prefix "https://github.com/maddada/Ghostex/releases/download/v$VERSION/"
    --full-release-notes-url "https://github.com/maddada/Ghostex/releases/tag/v$VERSION"
    --embed-release-notes
    --maximum-versions 6
    -o "$APPCAST_WORK/appcast.xml"
  )
  if [[ -n "${SPARKLE_PRIVATE_KEY:-}" ]]; then
    printf '%s' "$SPARKLE_PRIVATE_KEY" | "$SPARKLE_ROOT/bin/generate_appcast" "${GENERATE_ARGS[@]}" --ed-key-file - "$APPCAST_WORK"
  else
    "$SPARKLE_ROOT/bin/generate_appcast" "${GENERATE_ARGS[@]}" "$APPCAST_WORK"
  fi
  cp "$APPCAST_WORK/appcast.xml" "$OUTPUT/appcast.xml"
  APPCAST_SIGNATURE="$(xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@*[local-name()='edSignature'])[1])" "$OUTPUT/appcast.xml")"
  "$SPARKLE_ROOT/bin/sign_update" --verify "$DMG" "$APPCAST_SIGNATURE"
  [[ "$(xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='version'])[1])" "$OUTPUT/appcast.xml")" == "$BUILD_NUMBER" ]]
  [[ "$(xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='shortVersionString'])[1])" "$OUTPUT/appcast.xml")" == "$VERSION" ]]
  rm -rf "$APPCAST_WORK"
fi

ON_DEMAND_ROOT="$REPO_ROOT/build/on-demand-assets/$VERSION"
ASSETS=("$DMG")
for name in gxserver-linux-x64.tar.gz gxserver-linux-arm64.tar.gz bd-darwin-arm64.tar.gz; do
  [[ -f "$ON_DEMAND_ROOT/$name" ]] || { echo "Missing on-demand asset: $name" >&2; exit 1; }
done
cp "$ON_DEMAND_ROOT/bd-darwin-arm64.tar.gz" "$OUTPUT/bd-darwin-arm64.tar.gz"
ASSETS+=("$OUTPUT/bd-darwin-arm64.tar.gz")
MANIFEST_PATH="$APP_PATH/Contents/Resources/Web/on-demand-resources.json" \
ASSET_PATH="$ON_DEMAND_ROOT" node <<'JS'
const { createHash } = require("node:crypto");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const manifest = JSON.parse(readFileSync(process.env.MANIFEST_PATH, "utf8"));
for (const asset of Object.values(manifest.assets ?? {})) {
  const file = join(process.env.ASSET_PATH, asset.name);
  const actual = createHash("sha256").update(readFileSync(file)).digest("hex");
  if (actual !== asset.sha256) throw new Error(`Sealed on-demand checksum mismatch for ${asset.name}`);
}
JS
release_gpui_write_manifest "$OUTPUT" macos-arm64 "$VERSION" "${ASSETS[@]}"
printf 'Built GPUI macOS release payload in %s\n' "$OUTPUT"
