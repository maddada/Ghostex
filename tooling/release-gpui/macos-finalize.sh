#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
INPUT="${2:-}"
OUTPUT="${3:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" macos-arm64)}"
release_gpui_require_version "$VERSION"
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

DMG_NAME="ghostex-$VERSION-arm64.dmg"
[[ -f "$INPUT/$DMG_NAME" ]] || { echo "Preserved signed DMG is missing: $INPUT/$DMG_NAME" >&2; exit 1; }
[[ -f "$INPUT/bd-darwin-arm64.tar.gz" ]] || { echo "Preserved bd package is missing" >&2; exit 1; }
cp "$INPUT/$DMG_NAME" "$OUTPUT/$DMG_NAME"
cp "$INPUT/bd-darwin-arm64.tar.gz" "$OUTPUT/bd-darwin-arm64.tar.gz"
DMG="$OUTPUT/$DMG_NAME"

xcrun stapler validate "$DMG"
release_gpui_assert_dmg_budget "$DMG"

ATTACH_OUTPUT="$(hdiutil attach -nobrowse -readonly "$DMG")"
MOUNT_POINT="$(printf '%s\n' "$ATTACH_OUTPUT" | awk -F '\t' 'NF { value=$NF } END { print value }')"
[[ "$MOUNT_POINT" == /Volumes/* ]] || { echo "Could not resolve mounted DMG path" >&2; exit 1; }
trap 'hdiutil detach "$MOUNT_POINT" >/dev/null 2>&1 || true' EXIT
APP_PATH="$MOUNT_POINT/Ghostex.app"
if [[ ! -d "$APP_PATH" ]]; then
  APP_PATH="$MOUNT_POINT/ghostex.app"
fi
node --input-type=module -e \
  'import { validateMacosAppBundle } from "./tooling/validate-macos-app-bundle.mjs"; await validateMacosAppBundle({ appName: "Ghostex", appPath: process.argv[1], arch: "arm64" });' \
  "$APP_PATH"
hdiutil detach "$MOUNT_POINT"
trap - EXIT

if [[ "${GHOSTEX_RELEASE_UPDATE_SPARKLE:-1}" == "1" ]]; then
  SPARKLE_ROOT="$($SCRIPT_DIR/prepare-sparkle.sh)"
  APPCAST_WORK="$(mktemp -d "$REPO_ROOT/build/release-gpui/appcast-stage-XXXXXX")"
  trap 'rm -rf "$APPCAST_WORK"' EXIT
  cp "$REPO_ROOT/appcast.xml" "$APPCAST_WORK/appcast.xml"
  cp "$DMG" "$APPCAST_WORK/$DMG_NAME"
  VERSION="$VERSION" CHANGELOG_PATH="$REPO_ROOT/CHANGELOG.md" NOTES_PATH="$APPCAST_WORK/ghostex-$VERSION-arm64.md" node <<'JS'
const { readFileSync, writeFileSync } = require("node:fs");
const version = process.env.VERSION;
const changelog = readFileSync(process.env.CHANGELOG_PATH, "utf8");
const start = changelog.indexOf(`## ${version} -`);
if (start < 0) throw new Error(`CHANGELOG.md has no ${version} section`);
const next = changelog.indexOf("\n## ", start + 4);
writeFileSync(process.env.NOTES_PATH, `# Ghostex ${version}\n\n${changelog.slice(start, next < 0 ? undefined : next).trim()}\n`);
JS
  GENERATE_ARGS=(
    --download-url-prefix "https://github.com/maddada/Ghostex/releases/download/v$VERSION/"
    --full-release-notes-url "https://github.com/maddada/Ghostex/releases/tag/v$VERSION"
    --embed-release-notes
    --maximum-versions 6
    -o "$APPCAST_WORK/appcast.xml"
  )
  [[ -n "${SPARKLE_PRIVATE_KEY:-}" ]] || { echo "SPARKLE_PRIVATE_KEY is required for production metadata" >&2; exit 1; }
  printf '%s' "$SPARKLE_PRIVATE_KEY" | "$SPARKLE_ROOT/bin/generate_appcast" "${GENERATE_ARGS[@]}" --ed-key-file - "$APPCAST_WORK"
  cp "$APPCAST_WORK/appcast.xml" "$OUTPUT/appcast.xml"
  SIGNATURE="$(xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@*[local-name()='edSignature'])[1])" "$OUTPUT/appcast.xml")"
  printf '%s' "$SPARKLE_PRIVATE_KEY" | "$SPARKLE_ROOT/bin/sign_update" --ed-key-file - --verify "$DMG" "$SIGNATURE"
fi

release_gpui_write_manifest "$OUTPUT" macos-arm64 "$VERSION" "$DMG" "$OUTPUT/bd-darwin-arm64.tar.gz"
printf 'Finalized notarized GPUI macOS payload in %s\n' "$OUTPUT"
