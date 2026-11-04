#!/usr/bin/env bash
set -euo pipefail

# Lays out a macOS gxserver package's programs as two minimal app bundles:
#
#   <package>/GhostexServer.app/Contents/MacOS/gxserver   (com.madda.ghostex.gxserver)
#   <package>/GhostexSessions.app/Contents/MacOS/zmx      (com.madda.ghostex.zmx)
#   <package>/bin/gxserver -> ../GhostexServer.app/Contents/MacOS/gxserver
#   <package>/bin/zmx      -> ../GhostexSessions.app/Contents/MacOS/zmx
#   <package>/bin/ghostex  -> gxserver
#
# Usage: package-macos-gxserver-bundles.sh <package-dir> <gxserver-binary> <zmx-binary> <version>
#
# CDXC:RemotePairing 2026-11-04 WHY: macOS remembers a program that is not inside an app by its
# file and signature, and Local Network privacy also uses the program's build UUID (TN3179), so a
# Mac running gxserver without the Ghostex app (a remote Mac mini) asked for its permissions again
# after every Ghostex update and filled Privacy & Security with duplicate gxserver and zmx rows
# (Discord, randomsand). A program that is an app's main executable is remembered by the app's
# bundle identifier and Developer ID signature, which every build keeps. Each program is the main
# executable of its own bundle because a second program in Contents/MacOS is still identified as a
# loose tool (checked with `codesign -dv <pid>`: a bundle's helper reports "Info.plist=not bound").
# The bin/ links keep every caller's path (launchd jobs, the CLI, zmx session supervisors, remote
# setup), and a program started through a link is still identified as its bundle. The executables
# keep their names, so process lists and `pgrep zmx` read as before. A bundle that touches a
# protected resource without a usage string is ended by macOS, and anything run in a terminal
# session may reach any of them, so both carry the same set a terminal app declares.
# SEE-ALSO: sign_gxserver_bundles in codesign-gpui-app.sh, `sign_package` in
# tooling/xtask/src/start_server.rs, `remove_leftover_releases` in server/src/setup.rs.

if [[ "$#" -ne 4 ]]; then
	echo "Usage: $0 <package-dir> <gxserver-binary> <zmx-binary> <version>" >&2
	exit 2
fi
PACKAGE_DIR="$1"
GXSERVER_BINARY="$2"
ZMX_BINARY="$3"
VERSION="$4"

GXSERVER_BUNDLE="GhostexServer.app"
ZMX_BUNDLE="GhostexSessions.app"

xml_escape() {
	local value="$1"
	value="${value//&/&amp;}"
	value="${value//</&lt;}"
	value="${value//>/&gt;}"
	printf '%s' "$value"
}

# Prints the usage strings, one <key>/<string> pair per line, for a bundle named $1.
usage_descriptions() {
	local who="$1" key text
	while IFS='|' read -r key text; do
		printf '\t<key>%s</key>\n\t<string>%s</string>\n' "$key" "$(xml_escape "${text//@WHO@/$who}")"
	done <<'EOF_USAGE'
NSLocalNetworkUsageDescription|@WHO@ or a program running in a Ghostex session would like to reach devices on your local network.
NSDocumentsFolderUsageDescription|@WHO@ or a program running in a Ghostex session would like to access your Documents folder.
NSDesktopFolderUsageDescription|@WHO@ or a program running in a Ghostex session would like to access your Desktop folder.
NSDownloadsFolderUsageDescription|@WHO@ or a program running in a Ghostex session would like to access your Downloads folder.
NSNetworkVolumesUsageDescription|@WHO@ or a program running in a Ghostex session would like to access files on a network volume.
NSRemovableVolumesUsageDescription|@WHO@ or a program running in a Ghostex session would like to access files on a removable volume.
NSFileProviderDomainUsageDescription|@WHO@ or a program running in a Ghostex session would like to access files managed by a cloud storage app.
NSAppleEventsUsageDescription|@WHO@ or a program running in a Ghostex session would like to use AppleScript.
NSSystemAdministrationUsageDescription|@WHO@ or a program running in a Ghostex session requires elevated privileges.
NSMicrophoneUsageDescription|A program running in a Ghostex session would like to use your microphone.
NSCameraUsageDescription|A program running in a Ghostex session would like to use the camera.
NSAudioCaptureUsageDescription|A program running in a Ghostex session would like to access your system's audio.
NSBluetoothAlwaysUsageDescription|A program running in a Ghostex session would like to use Bluetooth.
NSSpeechRecognitionUsageDescription|A program running in a Ghostex session would like to use speech recognition.
NSContactsUsageDescription|A program running in a Ghostex session would like to access your Contacts.
NSCalendarsUsageDescription|A program running in a Ghostex session would like to access your Calendar.
NSRemindersUsageDescription|A program running in a Ghostex session would like to access your reminders.
NSPhotoLibraryUsageDescription|A program running in a Ghostex session would like to access your Photo Library.
NSLocationUsageDescription|A program running in a Ghostex session would like to access your location.
NSMotionUsageDescription|A program running in a Ghostex session would like to access motion data.
EOF_USAGE
}

# write_bundle <bundle-dir-name> <executable-name> <source-binary> <bundle-id> <display-name>
write_bundle() {
	local bundle_dir="$PACKAGE_DIR/$1" executable="$2" source_binary="$3" bundle_id="$4" name="$5"
	rm -rf "$bundle_dir"
	mkdir -p "$bundle_dir/Contents/MacOS"
	cp "$source_binary" "$bundle_dir/Contents/MacOS/$executable"
	chmod 755 "$bundle_dir/Contents/MacOS/$executable"
	cat >"$bundle_dir/Contents/Info.plist" <<EOF_PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleExecutable</key>
	<string>$executable</string>
	<key>CFBundleIdentifier</key>
	<string>$bundle_id</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>$name</string>
	<key>CFBundleDisplayName</key>
	<string>$name</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$(xml_escape "$VERSION")</string>
	<key>CFBundleVersion</key>
	<string>$(xml_escape "$VERSION")</string>
	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
	<key>LSUIElement</key>
	<true/>
$(usage_descriptions "$name")
</dict>
</plist>
EOF_PLIST
	/usr/bin/plutil -lint -s "$bundle_dir/Contents/Info.plist"
}

mkdir -p "$PACKAGE_DIR/bin"
write_bundle "$GXSERVER_BUNDLE" gxserver "$GXSERVER_BINARY" com.madda.ghostex.gxserver "Ghostex Server"
write_bundle "$ZMX_BUNDLE" zmx "$ZMX_BINARY" com.madda.ghostex.zmx "Ghostex Sessions"
rm -f "$PACKAGE_DIR/bin/gxserver" "$PACKAGE_DIR/bin/zmx" "$PACKAGE_DIR/bin/ghostex"
ln -s "../$GXSERVER_BUNDLE/Contents/MacOS/gxserver" "$PACKAGE_DIR/bin/gxserver"
ln -s "../$ZMX_BUNDLE/Contents/MacOS/zmx" "$PACKAGE_DIR/bin/zmx"
# The public ghostex/gx CLI is gxserver started under the name `ghostex` (CDXC:Release 2026-10-23 in server/src/main.rs).
ln -s gxserver "$PACKAGE_DIR/bin/ghostex"
