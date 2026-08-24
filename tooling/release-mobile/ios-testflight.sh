#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../release-gpui/common.sh
source "$SCRIPT_DIR/../release-gpui/common.sh"

VERSION="${1:-}"
release_gpui_require_version "$VERSION"
REPO_ROOT="$(release_gpui_repo_root)"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" ios-testflight)}"
BUILD_NUMBER="$(release_gpui_build_number "$VERSION")"
MOBILE_ROOT="$REPO_ROOT/apps/mobile/app"
BUNDLE_ID="com.maddada.ghostex.ios"
TEAM_ID="KTKP595G3B"

release_gpui_require_command bun
release_gpui_require_command pod
release_gpui_require_command xcodebuild
: "${GHOSTEX_ASC_KEY_PATH:?Missing App Store Connect API key path}"
: "${GHOSTEX_ASC_KEY_ID:?Missing App Store Connect API key ID}"
: "${GHOSTEX_ASC_ISSUER_ID:?Missing App Store Connect API issuer ID}"

export GHOSTEX_RELEASE_VERSION="$VERSION"
export GHOSTEX_RELEASE_BUILD_NUMBER="$BUILD_NUMBER"

cd "$MOBILE_ROOT"
bunx expo prebuild --platform ios --no-install
pod install --project-directory=ios

ARCHIVE_PATH="$RUNNER_TEMP/Ghostex-$VERSION.xcarchive"
EXPORT_PATH="$RUNNER_TEMP/Ghostex-$VERSION-upload"
EXPORT_OPTIONS="$RUNNER_TEMP/Ghostex-$VERSION-ExportOptions.plist"
cat >"$EXPORT_OPTIONS" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>method</key><string>app-store-connect</string>
  <key>destination</key><string>upload</string>
  <key>signingStyle</key><string>automatic</string>
  <key>teamID</key><string>$TEAM_ID</string>
  <key>manageAppVersionAndBuildNumber</key><false/>
  <key>uploadSymbols</key><true/>
</dict></plist>
PLIST

AUTH_ARGS=(
	-allowProvisioningUpdates
	-authenticationKeyPath "$GHOSTEX_ASC_KEY_PATH"
	-authenticationKeyID "$GHOSTEX_ASC_KEY_ID"
	-authenticationKeyIssuerID "$GHOSTEX_ASC_ISSUER_ID"
)

xcodebuild \
	-workspace ios/Ghostex.xcworkspace \
	-scheme Ghostex \
	-configuration Release \
	-destination 'generic/platform=iOS' \
	-archivePath "$ARCHIVE_PATH" \
	DEVELOPMENT_TEAM="$TEAM_ID" \
	CODE_SIGN_STYLE=Automatic \
	"${AUTH_ARGS[@]}" \
	archive

INFO_PLIST="$ARCHIVE_PATH/Products/Applications/Ghostex.app/Info.plist"
[[ -f "$INFO_PLIST" ]] || {
	echo "Archived Ghostex.app Info.plist is missing" >&2
	exit 1
}
[[ "$(plutil -extract CFBundleIdentifier raw "$INFO_PLIST")" == "$BUNDLE_ID" ]]
[[ "$(plutil -extract CFBundleShortVersionString raw "$INFO_PLIST")" == "$VERSION" ]]
[[ "$(plutil -extract CFBundleVersion raw "$INFO_PLIST")" == "$BUILD_NUMBER" ]]

xcodebuild -exportArchive \
	-archivePath "$ARCHIVE_PATH" \
	-exportPath "$EXPORT_PATH" \
	-exportOptionsPlist "$EXPORT_OPTIONS" \
	"${AUTH_ARGS[@]}"

release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"
printf '{"build_number":"%s","bundle_id":"%s","distribution":"testflight","marketing_version":"%s","status":"uploaded"}\n' \
	"$BUILD_NUMBER" "$BUNDLE_ID" "$VERSION" >"$OUTPUT/testflight-attestation.json"
echo "Apple accepted Ghostex $VERSION ($BUILD_NUMBER) for TestFlight processing."
