#!/usr/bin/env bash
# Exports the phone app's over-the-air update for Android and iOS from this checkout:
#   <out>/<platform>/export/          `expo export` output (Hermes bundle, assets, metadata.json)
#   <out>/<platform>/expo-config.json the public app config the update carries (Constants.expoConfig)
#   <out>/<platform>/ota.json         platform, runtime version, release version, the main and phone
#                                     commits, and the update's "What's new"
#   <out>/certificate.pem             the certificate the app verifies updates with
# tooling/release-mobile/ota-publish.mjs signs and uploads it.
#
#   tooling/release-mobile/ota-export.sh <version> <out-dir>
#
# GHOSTEX_OTA_WHATS_NEW (optional) is the update's "What's new" as a JSON array of short lines
# (ota-release.mjs builds it from the phone commits since the last update); without it the update
# carries none. The phone's Settings > Updates shows it.
#
# The environment matches the GitHub APK build (tooling/release-mobile/android.sh) and the
# TestFlight build, so the runtime versions computed here equal the ones inside those binaries.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../release-gpui/common.sh
source "$SCRIPT_DIR/../release-gpui/common.sh"

VERSION="${1:-}"
OUT="${2:-}"
release_gpui_require_version "$VERSION"
[[ -n "$OUT" ]] || {
	echo "usage: ota-export.sh <version> <out-dir>" >&2
	exit 2
}
REPO_ROOT="$(release_gpui_repo_root)"
MOBILE_ROOT="$REPO_ROOT/apps/mobile/app"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

export GHOSTEX_RELEASE_VERSION="$VERSION"
GHOSTEX_RELEASE_BUILD_NUMBER="$(release_gpui_build_number "$VERSION")"
export GHOSTEX_RELEASE_BUILD_NUMBER
export GHOSTEX_ANDROID_SELF_UPDATE=1
GIT_SHA="$(git -C "$REPO_ROOT" rev-parse HEAD)"
MOBILE_SHA="$(git -C "$MOBILE_ROOT" rev-parse HEAD)"
WHATS_NEW="${GHOSTEX_OTA_WHATS_NEW:-[]}"

cd "$REPO_ROOT"
node apps/mobile/app/scripts/generate-chat-agents.mjs

cd "$MOBILE_ROOT"
cp certs/certificate.pem "$OUT/certificate.pem"
for platform in android ios; do
	dir="$OUT/$platform"
	rm -rf "$dir"
	mkdir -p "$dir"
	bunx expo export --platform "$platform" --output-dir "$dir/export"
	bunx expo config --type public --json >"$dir/expo-config.json"
	runtime="$(node scripts/ota-runtime-version.js "$platform")"
	PLATFORM="$platform" RUNTIME="$runtime" GIT_SHA="$GIT_SHA" MOBILE_SHA="$MOBILE_SHA" WHATS_NEW="$WHATS_NEW" \
		LIB="$SCRIPT_DIR/ota-release-lib.mjs" node --input-type=module -e '
		const { PLATFORM, RUNTIME, GHOSTEX_RELEASE_VERSION, GIT_SHA, MOBILE_SHA, WHATS_NEW, LIB } = process.env;
		const { parseWhatsNew } = await import(LIB);
		const info = { platform: PLATFORM, runtimeVersion: RUNTIME, version: GHOSTEX_RELEASE_VERSION, gitSha: GIT_SHA, mobileSha: MOBILE_SHA, whatsNew: parseWhatsNew(WHATS_NEW) };
		process.stdout.write(JSON.stringify(info, null, 2) + "\n");
	' >"$dir/ota.json"
	echo "Exported the $platform update for Ghostex $VERSION, runtime $runtime"
done
