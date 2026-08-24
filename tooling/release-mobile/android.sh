#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../release-gpui/common.sh
source "$SCRIPT_DIR/../release-gpui/common.sh"

VERSION="${1:-}"
release_gpui_require_version "$VERSION"
REPO_ROOT="$(release_gpui_repo_root)"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" android)}"
BUILD_NUMBER="$(release_gpui_build_number "$VERSION")"
MOBILE_ROOT="$REPO_ROOT/apps/mobile/app"

release_gpui_require_command bun
release_gpui_require_command java
release_gpui_require_command keytool
: "${GHOSTEX_ANDROID_SIGNING_STORE_FILE:?Missing Android release keystore path}"
: "${GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD:?Missing Android release keystore password}"
: "${GHOSTEX_ANDROID_SIGNING_KEY_ALIAS:?Missing Android release key alias}"
: "${GHOSTEX_ANDROID_SIGNING_KEY_PASSWORD:?Missing Android release key password}"

export GHOSTEX_RELEASE_VERSION="$VERSION"
export GHOSTEX_RELEASE_BUILD_NUMBER="$BUILD_NUMBER"
export GHOSTEX_RELEASE_APPLICATION_ID="io.ghostex"
export GHOSTEX_RELEASE_SOURCE_KIND="react-native-mobile"
export ORG_GRADLE_PROJECT_ghostexReleaseStoreFile="$GHOSTEX_ANDROID_SIGNING_STORE_FILE"
export ORG_GRADLE_PROJECT_ghostexReleaseStorePassword="$GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD"
export ORG_GRADLE_PROJECT_ghostexReleaseKeyAlias="$GHOSTEX_ANDROID_SIGNING_KEY_ALIAS"
export ORG_GRADLE_PROJECT_ghostexReleaseKeyPassword="$GHOSTEX_ANDROID_SIGNING_KEY_PASSWORD"

cd "$MOBILE_ROOT"
bunx expo prebuild --platform android --no-install
./android/gradlew --no-daemon --stacktrace -p android :app:assembleRelease

BUILD_TOOLS="${ANDROID_HOME:?ANDROID_HOME is required}/build-tools/36.0.0"
AAPT="$BUILD_TOOLS/aapt"
APKSIGNER="$BUILD_TOOLS/apksigner"
[[ -x "$AAPT" && -x "$APKSIGNER" ]] || {
	echo "Android build-tools 36.0.0 are incomplete" >&2
	exit 1
}

APK_SOURCE="$MOBILE_ROOT/android/app/build/outputs/apk/release/app-release.apk"
[[ -f "$APK_SOURCE" ]] || {
	echo "React Native release APK was not produced" >&2
	exit 1
}
BADGING="$($AAPT dump badging "$APK_SOURCE" | head -n 1)"
[[ "$BADGING" == *"name='io.ghostex'"* ]] || {
	echo "APK package is not io.ghostex: $BADGING" >&2
	exit 1
}
[[ "$BADGING" == *"versionCode='$BUILD_NUMBER'"* ]] || {
	echo "APK versionCode is not $BUILD_NUMBER: $BADGING" >&2
	exit 1
}
[[ "$BADGING" == *"versionName='$VERSION'"* ]] || {
	echo "APK versionName is not $VERSION: $BADGING" >&2
	exit 1
}

$APKSIGNER verify --verbose "$APK_SOURCE"
APK_CERT="$($APKSIGNER verify --print-certs "$APK_SOURCE" | sed -n 's/^Signer #1 certificate SHA-256 digest: //p' | tr -d ': ' | tr '[:upper:]' '[:lower:]')"
KEYSTORE_CERT="$(keytool -exportcert -keystore "$GHOSTEX_ANDROID_SIGNING_STORE_FILE" -storepass "$GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD" -alias "$GHOSTEX_ANDROID_SIGNING_KEY_ALIAS" | release_gpui_sha256 /dev/stdin)"
if [[ -z "$APK_CERT" || "$APK_CERT" != "$KEYSTORE_CERT" ]]; then
	echo "APK signing certificate does not match the established Ghostex keystore ($APK_CERT != $KEYSTORE_CERT)" >&2
	exit 1
fi

release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"
APK="$OUTPUT/ghostex-android.apk"
cp "$APK_SOURCE" "$APK"
release_gpui_write_manifest "$OUTPUT" android "$VERSION" "$APK"
echo "React Native Android APK ready: $APK"
