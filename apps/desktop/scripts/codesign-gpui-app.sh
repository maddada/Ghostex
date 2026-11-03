#!/usr/bin/env bash
set -euo pipefail

# GPUI-owned production-notarized macOS recipe: explicit inside-out signing of nested
# Mach-O payloads, Chromium-safe V8 entitlements on CEF helpers and the
# code-server Node runtime, hardened runtime everywhere, then the outer app.
# Release CEF assets use the same entry point in --cef-framework mode so the
# external framework and app share a Developer ID team without disabling
# hardened-runtime library validation.
# Sparkle.framework handling is GPUI-specific because the Xcode build signs it
# during embed on macOS while this bundle stages it by hand.
#
# Ad-hoc dev builds keep the historical single --deep pass so unsigned local
# packaging behavior is unchanged.

SIGN_MODE="app"
SIGN_TARGET="${1:-}"
if [[ "$SIGN_TARGET" == "--cef-framework" ]]; then
	SIGN_MODE="cef-framework"
	SIGN_TARGET="${2:-}"
fi
CODE_SIGN_IDENTITY="${GHOSTEX_GPUI_SIGN_IDENTITY:--}"
CODE_SIGN_TIMESTAMP_FLAG="${GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG:---timestamp}"
HELPER_APP_GLOB="${GHOSTEX_GPUI_HELPER_APP_GLOB:-Ghostex Helper*.app}"
LID_SLEEP_HELPER_LABEL="${GHOSTEX_GPUI_LID_SLEEP_HELPER_LABEL:-}"

if [[ -z "$SIGN_TARGET" ]]; then
	echo "Usage: $0 /path/to/Ghostex.app | $0 --cef-framework /path/to/Chromium\\ Embedded\\ Framework.framework" >&2
	exit 2
fi
if [[ ! -d "$SIGN_TARGET" ]]; then
	echo "Code-sign target does not exist: $SIGN_TARGET" >&2
	exit 1
fi
if [[ -z "$CODE_SIGN_IDENTITY" ]]; then
	cat >&2 <<'EOF'
GHOSTEX_GPUI_SIGN_IDENTITY cannot be empty.

Leave it unset for the default ad-hoc dev signing, or set:
  GHOSTEX_GPUI_SIGN_IDENTITY="Developer ID Application: Name (TEAMID)"
EOF
	exit 1
fi

# codesign rejects resource forks, Finder information (SetFile bundle bit),
# and similar xattrs. Clear them on the target before any signing pass.
/usr/bin/xattr -cr "$SIGN_TARGET"

if [[ "$CODE_SIGN_IDENTITY" == "-" ]]; then
	codesign --force --deep --pagesize 16384 --sign - "$SIGN_TARGET"
	codesign --verify --deep --strict --verbose=2 "$SIGN_TARGET"
	exit 0
fi

APP_PATH="$SIGN_TARGET"
echo "Signing $SIGN_TARGET"
echo "Identity: $CODE_SIGN_IDENTITY"

FRAMEWORKS_PATH="$APP_PATH/Contents/Frameworks"
CEF_ENTITLEMENTS="$(mktemp -t ghostex-gpui-cef-entitlements.XXXXXX.plist)"
trap 'rm -f "$CEF_ENTITLEMENTS"' EXIT
cat >"$CEF_ENTITLEMENTS" <<'EOF_ENTITLEMENTS'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>com.apple.security.cs.allow-jit</key>
	<true/>
	<key>com.apple.security.cs.allow-unsigned-executable-memory</key>
	<true/>
	<!-- Browser panes can grant a page microphone/camera access, and the
	hardened runtime blocks those devices without these resource-access
	entitlements. Chromium captures in helper processes as well as the browser
	process, so the same list signs the app and every CEF helper. -->
	<key>com.apple.security.device.audio-input</key>
	<true/>
	<key>com.apple.security.device.camera</key>
	<true/>
</dict>
</plist>
EOF_ENTITLEMENTS

# CDXC:Build 2026-09-05 WHY:
# Preserve unchanged nested signatures only when their signature details match a signature made by this exact recipe and identity.
# Deep verification catches changed resources inside a bundle even when its executable's CDHash is unchanged.
SIGNATURE_CACHE_DIR=""
SIGNATURE_RECIPE=""
if [[ "${GHOSTEX_LOCAL_START:-0}" == "1" ]]; then
	script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
	repo_root="$(cd "$script_dir/../../.." && pwd)"
	SIGNATURE_CACHE_DIR="${GHOSTEX_BUILD_CACHE_DIR:-$repo_root/build/${GHOSTEX_MACOS_ARCH:-$(uname -m)}/build-cache}/signatures"
	mkdir -p "$SIGNATURE_CACHE_DIR"
	SIGNATURE_RECIPE="$( { cat "${BASH_SOURCE[0]}"; printf '%s\0' "$CODE_SIGN_IDENTITY" "$CODE_SIGN_TIMESTAMP_FLAG"; } | shasum -a 256 | cut -d ' ' -f 1)"
fi

signature_stamp_path() {
	printf '%s/%s.sha256\n' "$SIGNATURE_CACHE_DIR" "$(printf '%s' "$1" | shasum -a 256 | cut -d ' ' -f 1)"
}

signature_fingerprint() {
	codesign -d -r- --verbose=4 "$1" 2>&1 | shasum -a 256 | cut -d ' ' -f 1
}

reuse_signature() {
	[[ -n "$SIGNATURE_CACHE_DIR" ]] || return 1
	local stamp signature_digest
	stamp="$(signature_stamp_path "$1")"
	[[ -f "$stamp" ]] || return 1
	signature_digest="$(signature_fingerprint "$1")" || return 1
	[[ -n "$signature_digest" && "$(<"$stamp")" == "$SIGNATURE_RECIPE:$signature_digest" ]] || return 1
	codesign --verify --deep --strict "$1" 2>/dev/null || return 1
	echo "Reusing signature: $1"
}

# CDXC:Release 2026-10-19 WHY: Apple's timestamp server sometimes refuses requests for a short while ("The timestamp service is not available."); in 10.22.0 (run 38358214637) that failed the macOS job after 50 minutes and the re-run cost 35 more. `codesign --force` can simply run again, so only that error is retried, three attempts in all; every other failure stops the build at once.
# CDXC:Release 2026-10-23 DECISION: the user chose to sign every Mach-O with 16 KB pages to shrink the DMG. The macos-15 runner's codesign hashes 4 KB pages, and those hashes do not compress: 16 KB pages store a quarter as many and cut about 1.3 MB from the DMG (measured on 10.30.0). Releases are arm64 only, whose pages are 16 KB; Spotify, Discord and Cursor ship 16 KB signatures to macOS 11 and 12.
codesign_retrying_timestamp() {
	local attempt output status
	for attempt in 1 2 3; do
		status=0
		output="$(codesign --pagesize 16384 "$@" 2>&1)" || status=$?
		[[ -z "$output" ]] || printf '%s\n' "$output" >&2
		[[ "$status" != 0 ]] || return 0
		if [[ "$attempt" == 3 || "$output" != *"The timestamp service is not available"* ]]; then
			return "$status"
		fi
		echo "codesign could not reach Apple's timestamp service (attempt $attempt/3); retrying in $((attempt * 15))s" >&2
		sleep "$((attempt * 15))"
	done
}

sign_code() {
	local code_path="${!#}" signature_digest
	if reuse_signature "$code_path"; then
		return 0
	fi
	codesign_retrying_timestamp "$@"
	if [[ -n "$SIGNATURE_CACHE_DIR" ]]; then
		signature_digest="$(signature_fingerprint "$code_path")"
		[[ -n "$signature_digest" ]]
		printf '%s\n' "$SIGNATURE_RECIPE:$signature_digest" >"$(signature_stamp_path "$code_path")"
	fi
}

requires_v8_runtime_entitlements() {
	local code_path="$1"
	# Dev bundles still sign the self-contained code-server Node runtime. Release
	# bundles have no such path because Node lives inside the downloadable asset.
	[[ -f "$APP_PATH/Contents/Resources/Web/code-server/lib/node" && "$code_path" == "$APP_PATH/Contents/Resources/Web/code-server/lib/node" ]]
}

# CDXC:RemotePairing 2026-11-03 WHY: macOS privacy settings remember gxserver and zmx by path plus their designated requirement, whose identifier codesign otherwise takes from the file name. Pinning the names they have always shipped with (`gxserver`, `zmx`) keeps every existing grant valid and stops a renamed or repackaged copy from reading as a new program; never change them to reverse-DNS names, which would make every Mac ask again once. SEE-ALSO: `move_release_into_fixed_folder` in server/src/setup.rs, which keeps the path.
sign_plain_macho() {
	local code_path="$1" code_name
	code_name="$(basename "$code_path")"
	if requires_v8_runtime_entitlements "$code_path"; then
		sign_code \
			--force \
			--options runtime \
			--entitlements "$CEF_ENTITLEMENTS" \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$code_path"
	elif [[ "$code_name" == "gxserver" || "$code_name" == "zmx" ]]; then
		sign_code \
			--force \
			--options runtime \
			--identifier "$code_name" \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$code_path"
	else
		sign_code \
			--force \
			--options runtime \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$code_path"
	fi
}

# CDXC:Build 2026-09-23 WHY:
# Independent nested items were signed one after another; signing leaves in parallel took a test set of 36 Mach-O files from 2.5s to 0.9s.
# Order stays inside-out: every parallel batch finishes before the bundle that contains it is signed.
SIGN_JOBS="${GHOSTEX_GPUI_SIGN_JOBS:-8}"
export -f signature_stamp_path signature_fingerprint reuse_signature codesign_retrying_timestamp sign_code requires_v8_runtime_entitlements sign_plain_macho
export APP_PATH CEF_ENTITLEMENTS CODE_SIGN_TIMESTAMP_FLAG CODE_SIGN_IDENTITY SIGNATURE_CACHE_DIR SIGNATURE_RECIPE

# Signs every Mach-O among the NUL-separated paths on stdin, several at a time.
sign_plain_machos_in_parallel() {
	xargs -0 -n 1 -P "$SIGN_JOBS" /bin/bash -c 'set -euo pipefail; if file "$1" | grep -q "Mach-O"; then sign_plain_macho "$1"; fi' _
}

sign_cef_framework() {
	local framework_path="$1"
	if reuse_signature "$framework_path"; then
		return 0
	fi
	find "$framework_path/Libraries" \
		-name '*.dylib' \
		-type f \
		-print0 2>/dev/null | sign_plain_machos_in_parallel
	sign_code \
		--force \
		--options runtime \
		"$CODE_SIGN_TIMESTAMP_FLAG" \
		--sign "$CODE_SIGN_IDENTITY" \
		"$framework_path"
}

if [[ "$SIGN_MODE" == "cef-framework" ]]; then
	sign_cef_framework "$SIGN_TARGET"
	codesign --verify --deep --strict --verbose=2 "$SIGN_TARGET"
	exit 0
fi

if [[ -d "$FRAMEWORKS_PATH/Chromium Embedded Framework.framework" ]]; then
	sign_cef_framework "$FRAMEWORKS_PATH/Chromium Embedded Framework.framework"
fi

SPARKLE_FRAMEWORK="$FRAMEWORKS_PATH/Sparkle.framework"
if [[ -d "$SPARKLE_FRAMEWORK" ]]; then
	# Sparkle 2 distribution recipe: sign the XPC services (preserving their
	# own entitlements), Autoupdate, and Updater.app before the framework so
	# notarization validates each nested code object.
	for xpc_service in \
		"$SPARKLE_FRAMEWORK/Versions/B/XPCServices/Downloader.xpc" \
		"$SPARKLE_FRAMEWORK/Versions/B/XPCServices/Installer.xpc"; do
		if [[ -d "$xpc_service" ]]; then
			sign_code \
				--force \
				--options runtime \
				--preserve-metadata=entitlements \
				"$CODE_SIGN_TIMESTAMP_FLAG" \
				--sign "$CODE_SIGN_IDENTITY" \
				"$xpc_service"
		fi
	done
	if [[ -f "$SPARKLE_FRAMEWORK/Versions/B/Autoupdate" ]]; then
		sign_code \
			--force \
			--options runtime \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$SPARKLE_FRAMEWORK/Versions/B/Autoupdate"
	fi
	if [[ -d "$SPARKLE_FRAMEWORK/Versions/B/Updater.app" ]]; then
		sign_code \
			--force \
			--options runtime \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$SPARKLE_FRAMEWORK/Versions/B/Updater.app"
	fi
	sign_code \
		--force \
		--options runtime \
		"$CODE_SIGN_TIMESTAMP_FLAG" \
		--sign "$CODE_SIGN_IDENTITY" \
		"$SPARKLE_FRAMEWORK"
fi

sign_cef_helper_app() {
	local helper_app="$1" helper_name helper_executable
	helper_name="$(basename "$helper_app" .app)"
	helper_executable="$helper_app/Contents/MacOS/$helper_name"
	if [[ -x "$helper_executable" ]]; then
		sign_code \
			--force \
			--options runtime \
			--entitlements "$CEF_ENTITLEMENTS" \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$helper_executable"
	fi
	sign_code \
		--force \
		--options runtime \
		--entitlements "$CEF_ENTITLEMENTS" \
		"$CODE_SIGN_TIMESTAMP_FLAG" \
		--sign "$CODE_SIGN_IDENTITY" \
		"$helper_app"
}
export -f sign_cef_helper_app

if [[ -d "$FRAMEWORKS_PATH" ]]; then
	find "$FRAMEWORKS_PATH" \
		-maxdepth 1 \
		-name "$HELPER_APP_GLOB" \
		-type d \
		-print0 |
		xargs -0 -n 1 -P "$SIGN_JOBS" /bin/bash -c 'set -euo pipefail; sign_cef_helper_app "$1"' _
fi

sign_nested_resource_code() {
	local resource_path="$1"
	if [[ ! -d "$resource_path" ]]; then
		return 0
	fi
	# Apple notarization validates every Mach-O payload independently. Linux
	# remote-gxserver packages under the same Web tree are ELF and are skipped
	# by the Mach-O check.
	find "$resource_path" \
		-type f \
		\( -perm -111 -o -name '*.node' -o -name '*.dylib' -o -name 'spawn-helper' \) \
		-print0 | sign_plain_machos_in_parallel
}

# The bundled Ctrl+G Monaco prompt-editor helper is a nested app bundle, so
# notarization needs its executable and then the bundle signed before the outer
# app. It is a plain WKWebView Swift app: no V8/JIT entitlements.
GHOSTEX_EDITOR_APP="$APP_PATH/Contents/Resources/GhostexEditor.app"
if [[ -d "$GHOSTEX_EDITOR_APP" ]] && ! reuse_signature "$GHOSTEX_EDITOR_APP"; then
	if [[ -x "$GHOSTEX_EDITOR_APP/Contents/MacOS/GhostexEditor" ]]; then
		sign_code \
			--force \
			--options runtime \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$GHOSTEX_EDITOR_APP/Contents/MacOS/GhostexEditor"
	fi
	sign_code \
		--force \
		--options runtime \
		"$CODE_SIGN_TIMESTAMP_FLAG" \
		--sign "$CODE_SIGN_IDENTITY" \
		"$GHOSTEX_EDITOR_APP"
fi

sign_nested_resource_code "$APP_PATH/Contents/Resources/Web"
sign_nested_resource_code "$APP_PATH/Contents/Resources/CLI"

LAUNCH_SERVICES_PATH="$APP_PATH/Contents/Library/LaunchServices"
if [[ -d "$LAUNCH_SERVICES_PATH" ]]; then
	# The privileged lid-sleep helper must be Developer ID signed with hardened
	# runtime and a secure timestamp before the outer app; no debug
	# entitlements such as get-task-allow. DevID signing makes the helper's
	# capture-at-install client requirement certificate-anchored (plan §14
	# Batch 9.3), replacing ad-hoc per-build signature_digest pinning.
	find "$LAUNCH_SERVICES_PATH" \
		-type f \
		-perm -111 \
		-print0 |
		while IFS= read -r -d '' helper_executable; do
			if file "$helper_executable" | grep -q 'Mach-O'; then
				sign_code \
					--force \
					--options runtime \
					"$CODE_SIGN_TIMESTAMP_FLAG" \
					--sign "$CODE_SIGN_IDENTITY" \
					"$helper_executable"
			fi
		done
fi
if [[ -n "$LID_SLEEP_HELPER_LABEL" && -e "$APP_PATH/Contents/Resources/$LID_SLEEP_HELPER_LABEL" ]]; then
	echo "Unexpected lid sleep helper copy in Contents/Resources. Remove it before signing: $APP_PATH/Contents/Resources/$LID_SLEEP_HELPER_LABEL" >&2
	exit 1
fi

# CDXC:Build 2026-09-05 WHY:
# Nested code is signed explicitly above; a recursive outer signing pass signed it a second time and added several seconds per start.
# Keep deep verification below to validate the complete bundle.
codesign_retrying_timestamp \
	--force \
	--options runtime \
	--entitlements "$CEF_ENTITLEMENTS" \
	"$CODE_SIGN_TIMESTAMP_FLAG" \
	--sign "$CODE_SIGN_IDENTITY" \
	"$APP_PATH"

# Local starts skip this: `cargo xtask start` deep-verifies the installed copy right after the sync and re-signs it if that fails.
if [[ "${GHOSTEX_LOCAL_START:-0}" != "1" ]]; then
	codesign --verify --deep --strict --verbose=2 "$APP_PATH"
fi

# Verify the nested helper retains its V8 entitlements after sealing the outer app.
first_helper="$(find "$FRAMEWORKS_PATH" -maxdepth 1 -name "$HELPER_APP_GLOB" -type d -print -quit)"
helper_entitlements=""
if [[ -n "$first_helper" ]]; then
	helper_entitlements="$(codesign -d --entitlements :- "$first_helper" 2>/dev/null || true)"
	if [[ "$helper_entitlements" != *"com.apple.security.cs.allow-jit"* ]]; then
		echo "CEF helper lost its V8 entitlements after the outer app signature: $first_helper" >&2
		exit 1
	fi
fi

app_entitlements="$(codesign -d --entitlements :- "$APP_PATH" 2>/dev/null || true)"
if [[ "$app_entitlements" == *"com.apple.security.cs.disable-library-validation"* ]]; then
	echo "GPUI app must not disable hardened-runtime library validation." >&2
	exit 1
fi
if [[ "$helper_entitlements" == *"com.apple.security.cs.disable-library-validation"* ]]; then
	echo "CEF helper must not disable hardened-runtime library validation: $first_helper" >&2
	exit 1
fi
