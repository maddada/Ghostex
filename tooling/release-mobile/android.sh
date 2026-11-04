#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../release-gpui/common.sh
source "$SCRIPT_DIR/../release-gpui/common.sh"

VERSION="${1:-}"
release_gpui_require_version "$VERSION"
REPO_ROOT="$(release_gpui_repo_root)"

# CDXC:Release 2026-10-19 WHY: Ghostex ships the phone app two ways from the same source.
# `github` (the default) is the sideloaded APK on the GitHub release, with the in-app updater
# that installs newer APKs. `play` is the Android App Bundle for Google Play: Play accepts only
# bundles for new apps and rejects apps that can install other packages, so it is built with
# GHOSTEX_ANDROID_SELF_UPDATE=0 (apps/mobile/app/app.config.js) and is checked below to carry no
# installer or overlay permission. Both are signed with the same release key, so a phone can move
# from the GitHub APK to the Play install without uninstalling.
DISTRIBUTION="${GHOSTEX_ANDROID_DISTRIBUTION:-github}"
case "$DISTRIBUTION" in
github)
	PLATFORM=android
	GRADLE_TASK=":app:assembleRelease"
	export GHOSTEX_ANDROID_SELF_UPDATE=1
	;;
play)
	PLATFORM=android-play
	GRADLE_TASK=":app:bundleRelease"
	export GHOSTEX_ANDROID_SELF_UPDATE=0
	: "${GHOSTEX_BUNDLETOOL_JAR:?Missing bundletool jar path (GHOSTEX_BUNDLETOOL_JAR) for the Play bundle checks}"
	[[ -s "$GHOSTEX_BUNDLETOOL_JAR" ]] || {
		echo "bundletool jar not found: $GHOSTEX_BUNDLETOOL_JAR" >&2
		exit 1
	}
	;;
*)
	echo "GHOSTEX_ANDROID_DISTRIBUTION must be github or play, got $DISTRIBUTION" >&2
	exit 2
	;;
esac
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" "$PLATFORM")}"
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

# The React Native app's list of chat-capable agents is generated from
# packages/shared/session-chat-agents.ts. Regenerate it from the release checkout so a
# committed list from an earlier revision can never reach the APK.
cd "$REPO_ROOT"
node apps/mobile/app/scripts/generate-chat-agents.mjs

# The release workflow sets GHOSTEX_ANDROID_ABIS (space separated); every ABI
# listed must have its gomobile and chat-core libraries staged.
ANDROID_ABIS="${GHOSTEX_ANDROID_ABIS:-arm64-v8a}"
read -r -a ANDROID_ABI_LIST <<<"$ANDROID_ABIS"
[[ ${#ANDROID_ABI_LIST[@]} -gt 0 ]] || {
	echo "GHOSTEX_ANDROID_ABIS is empty" >&2
	exit 1
}
REACT_NATIVE_ARCHITECTURES="$(IFS=,; echo "${ANDROID_ABI_LIST[*]}")"

cd "$MOBILE_ROOT"
bunx expo prebuild --platform android --no-install
# CDXC:Release 2026-10-16 WHY: since the release APK compresses its native libraries, packaging
# deflates each large library in memory, and the template's 2 GB Gradle heap ran out in
# :app:packageRelease (10.24.0, run 38836965394). A command-line value outranks the generated
# gradle.properties.
./android/gradlew --no-daemon --stacktrace -p android \
	"-Dorg.gradle.jvmargs=-Xmx6g -XX:MaxMetaspaceSize=1g -Dfile.encoding=UTF-8" \
	"-PreactNativeArchitectures=$REACT_NATIVE_ARCHITECTURES" "$GRADLE_TASK"

BUILD_TOOLS="${ANDROID_HOME:?ANDROID_HOME is required}/build-tools/37.0.0"
AAPT="$BUILD_TOOLS/aapt"
APKSIGNER="$BUILD_TOOLS/apksigner"
[[ -x "$AAPT" && -x "$APKSIGNER" ]] || {
	echo "Android build-tools 37.0.0 are incomplete" >&2
	exit 1
}

KEYSTORE_CERT="$(keytool -exportcert -keystore "$GHOSTEX_ANDROID_SIGNING_STORE_FILE" -storepass "$GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD" -alias "$GHOSTEX_ANDROID_SIGNING_KEY_ALIAS" | release_gpui_sha256 /dev/stdin)"

if [[ "$DISTRIBUTION" == play ]]; then
	AAB_SOURCE="$MOBILE_ROOT/android/app/build/outputs/bundle/release/app-release.aab"
	[[ -f "$AAB_SOURCE" ]] || {
		echo "React Native release bundle was not produced" >&2
		exit 1
	}
	java -jar "$GHOSTEX_BUNDLETOOL_JAR" validate --bundle="$AAB_SOURCE" >/dev/null
	AAB_CERT="$(keytool -printcert -jarfile "$AAB_SOURCE" | sed -n 's/^[[:space:]]*SHA256: //p' | head -n 1 | tr -d ': ' | tr '[:upper:]' '[:lower:]')"
	if [[ -z "$AAB_CERT" || "$AAB_CERT" != "$KEYSTORE_CERT" ]]; then
		echo "Bundle signing certificate does not match the established Ghostex keystore ($AAB_CERT != $KEYSTORE_CERT)" >&2
		exit 1
	fi
	# The checks below read an APK. The universal APK bundletool builds from the bundle carries
	# the bundle's manifest, dex files and native libraries, which is what the split APKs Play
	# serves are cut from.
	UNIVERSAL_DIR="$(mktemp -d)"
	trap 'rm -rf "$UNIVERSAL_DIR"' EXIT
	(umask 077 && printf '%s' "$GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD" >"$UNIVERSAL_DIR/store-pass" && printf '%s' "$GHOSTEX_ANDROID_SIGNING_KEY_PASSWORD" >"$UNIVERSAL_DIR/key-pass")
	java -jar "$GHOSTEX_BUNDLETOOL_JAR" build-apks --mode=universal \
		--bundle="$AAB_SOURCE" --output="$UNIVERSAL_DIR/universal.apks" \
		--ks="$GHOSTEX_ANDROID_SIGNING_STORE_FILE" --ks-pass="file:$UNIVERSAL_DIR/store-pass" \
		--ks-key-alias="$GHOSTEX_ANDROID_SIGNING_KEY_ALIAS" --key-pass="file:$UNIVERSAL_DIR/key-pass"
	unzip -q -o "$UNIVERSAL_DIR/universal.apks" universal.apk -d "$UNIVERSAL_DIR"
	APK_SOURCE="$UNIVERSAL_DIR/universal.apk"
else
	APK_SOURCE="$MOBILE_ROOT/android/app/build/outputs/apk/release/app-release.apk"
fi
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

# Google Play rejects the installer permission (the GitHub self-updater); the overlay permission
# is an Expo template leftover the app never uses (blocked in apps/mobile/app/app.json).
PERMISSIONS="$($AAPT dump permissions "$APK_SOURCE")"
FORBIDDEN_PERMISSIONS=(android.permission.SYSTEM_ALERT_WINDOW)
if [[ "$DISTRIBUTION" == play ]]; then
	FORBIDDEN_PERMISSIONS+=(android.permission.REQUEST_INSTALL_PACKAGES)
fi
for permission in "${FORBIDDEN_PERMISSIONS[@]}"; do
	if grep -q "name='$permission'" <<<"$PERMISSIONS"; then
		echo "The $DISTRIBUTION build must not request $permission" >&2
		exit 1
	fi
done

APK_ABIS="$(unzip -Z1 "$APK_SOURCE" 'lib/*' | cut -d/ -f2 | sort -u | tr '\n' ' ')"
EXPECTED_ABIS="$(printf '%s\n' "${ANDROID_ABI_LIST[@]}" | sort -u | tr '\n' ' ')"
[[ "$APK_ABIS" == "$EXPECTED_ABIS" ]] || {
	echo "APK ships ABIs '$APK_ABIS', expected '$EXPECTED_ABIS'" >&2
	exit 1
}

# R8 and native-library compression are on for release builds
# (apps/mobile/app/plugins/withAndroidReleaseSize.js). AGP already fails the
# build on classes R8 reports missing; this gate also fails it when shrinking
# did not run, when a library is stored uncompressed, or when a class that
# native code or reflection reaches by name was renamed or removed.
"$SCRIPT_DIR/android-release-check.sh" "$APK_SOURCE" "$MOBILE_ROOT/android/app/build/outputs/mapping/release" "$BUILD_TOOLS"

$APKSIGNER verify --verbose "$APK_SOURCE"
# CDXC:Release 2026-10-26 WHY: build-tools 37's apksigner prefixes each certificate line with the
# scheme it verified, `V2 Signer: certificate SHA-256 digest: <hex>` (`V2 Signer #2: ...` when there
# are several signers), where 36 printed `Signer #1 certificate SHA-256 digest: <hex>`. This reads
# 37's lines (one per scheme, so the same certificate can repeat) and requires exactly one distinct
# signing certificate.
APK_SIGNER_CERTS="$($APKSIGNER verify --print-certs "$APK_SOURCE" | sed -nE 's/^V[0-9]+(\.[0-9]+)? Signer( #[0-9]+)?: certificate SHA-256 digest: ([0-9a-f]{64})$/\3/p' | sort -u)"
APK_SIGNER_COUNT="$(grep -c . <<<"$APK_SIGNER_CERTS" || true)"
if [[ "$APK_SIGNER_COUNT" != 1 ]]; then
	echo "Expected one APK signing certificate in apksigner's output, found $APK_SIGNER_COUNT" >&2
	exit 1
fi
APK_CERT="$APK_SIGNER_CERTS"
if [[ -z "$APK_CERT" || "$APK_CERT" != "$KEYSTORE_CERT" ]]; then
	echo "APK signing certificate does not match the established Ghostex keystore ($APK_CERT != $KEYSTORE_CERT)" >&2
	exit 1
fi

release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"
if [[ "$DISTRIBUTION" == play ]]; then
	ARTIFACT="$OUTPUT/ghostex-android-play.aab"
	cp "$AAB_SOURCE" "$ARTIFACT"
else
	ARTIFACT="$OUTPUT/ghostex-android.apk"
	cp "$APK_SOURCE" "$ARTIFACT"
fi
R8_OUTPUT="$(dirname "$OUTPUT")/$PLATFORM-r8"
rm -rf "$R8_OUTPUT"
mkdir -p "$R8_OUTPUT"
cp "$MOBILE_ROOT/android/app/build/outputs/mapping/release/mapping.txt" "$R8_OUTPUT/"
release_gpui_write_manifest "$OUTPUT" "$PLATFORM" "$VERSION" "$ARTIFACT"
echo "React Native Android $DISTRIBUTION build ready: $ARTIFACT"
