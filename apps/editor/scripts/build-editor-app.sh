#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
EDITOR_ROOT="$REPO_ROOT/apps/editor"
MACOS_PACKAGE="$EDITOR_ROOT/macos"
DIST_DIR="$EDITOR_ROOT/dist"
WEB_DIST="$DIST_DIR/web"
APP_ROOT="$DIST_DIR/GhostexEditor.app"
CONTENTS_DIR="$APP_ROOT/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

# Packagers (apps/desktop/scripts/build-macos-app.sh) pin the slice and the toolchain so
# the bundled helper matches the app they stage it into instead of whatever the
# builder host happens to be.
EDITOR_ARCH="${GHOSTEX_EDITOR_ARCH:-}"
EDITOR_SWIFT_DEVELOPER_DIR="${GHOSTEX_EDITOR_SWIFT_DEVELOPER_DIR:-}"

cd "$REPO_ROOT"

bun apps/editor/scripts/build-editor-web.mjs

if [[ -n "$EDITOR_SWIFT_DEVELOPER_DIR" ]]; then
	export DEVELOPER_DIR="$EDITOR_SWIFT_DEVELOPER_DIR"
elif ! xcrun xcodebuild -version >/dev/null 2>&1; then
	for developer_dir in \
		"/Applications/Xcode.app/Contents/Developer" \
		"/Applications/Xcode-beta.app/Contents/Developer"; do
		if [[ -x "$developer_dir/usr/bin/xcodebuild" ]]; then
			export DEVELOPER_DIR="$developer_dir"
			break
		fi
	done
fi

SWIFT_BUILD_ARGS=(-c release --package-path "$MACOS_PACKAGE")
if [[ -n "$EDITOR_ARCH" ]]; then
	SWIFT_BUILD_ARGS+=(--triple "$EDITOR_ARCH-apple-macosx13.0")
fi

swift build "${SWIFT_BUILD_ARGS[@]}"
BIN_DIR="$(swift build "${SWIFT_BUILD_ARGS[@]}" --show-bin-path)"
BUILT_BINARY="$BIN_DIR/GhostexEditor"

if [[ ! -x "$BUILT_BINARY" ]]; then
	echo "GhostexEditor binary was not produced at $BUILT_BINARY" >&2
	exit 1
fi
if [[ ! -f "$WEB_DIST/index.html" ]]; then
	echo "Editor web build did not produce $WEB_DIST/index.html" >&2
	exit 1
fi
if [[ ! -f "$WEB_DIST/monaco/vs/loader.js" ]]; then
	echo "Editor web build did not produce $WEB_DIST/monaco/vs/loader.js" >&2
	exit 1
fi

# Rebuild the bundle from scratch: a stale payload left behind by an earlier
# build would be copied into the app bundle and break its code signature.
rm -rf "$APP_ROOT"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR/Web"
install -m 755 "$BUILT_BINARY" "$MACOS_DIR/GhostexEditor"
ditto "$WEB_DIST" "$RESOURCES_DIR/Web"

cat >"$CONTENTS_DIR/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleExecutable</key>
	<string>GhostexEditor</string>
	<key>CFBundleIdentifier</key>
	<string>com.madda.ghostex.host.editor</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>Ghostex Editor</string>
	<key>CFBundleDisplayName</key>
	<string>Ghostex Editor</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>1.0</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
	<key>LSUIElement</key>
	<true/>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
PLIST

if [[ -n "$EDITOR_ARCH" ]] && ! /usr/bin/lipo -archs "$MACOS_DIR/GhostexEditor" | tr ' ' '\n' | grep -Fxq "$EDITOR_ARCH"; then
	echo "GhostexEditor binary does not contain $EDITOR_ARCH: $MACOS_DIR/GhostexEditor" >&2
	exit 1
fi

codesign --force --deep --sign - "$APP_ROOT"

echo "Built $APP_ROOT"
