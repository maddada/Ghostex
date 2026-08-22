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

if [[ "$CODE_SIGN_IDENTITY" == "-" ]]; then
	codesign --force --deep --sign - "$SIGN_TARGET"
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

requires_v8_runtime_entitlements() {
	local code_path="$1"
	# Dev bundles still sign the self-contained code-server Node runtime. Release
	# bundles have no such path because Node lives inside the downloadable asset.
	[[ -f "$APP_PATH/Contents/Resources/Web/code-server/lib/node" && "$code_path" == "$APP_PATH/Contents/Resources/Web/code-server/lib/node" ]]
}

sign_plain_macho() {
	local code_path="$1"
	if requires_v8_runtime_entitlements "$code_path"; then
		codesign \
			--force \
			--options runtime \
			--entitlements "$CEF_ENTITLEMENTS" \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$code_path"
	else
		codesign \
			--force \
			--options runtime \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$code_path"
	fi
}

sign_cef_framework() {
	local framework_path="$1"
	find "$framework_path/Libraries" \
		-name '*.dylib' \
		-type f \
		-print0 2>/dev/null |
		while IFS= read -r -d '' dylib_path; do
			sign_plain_macho "$dylib_path"
		done
	codesign \
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
			codesign \
				--force \
				--options runtime \
				--preserve-metadata=entitlements \
				"$CODE_SIGN_TIMESTAMP_FLAG" \
				--sign "$CODE_SIGN_IDENTITY" \
				"$xpc_service"
		fi
	done
	if [[ -f "$SPARKLE_FRAMEWORK/Versions/B/Autoupdate" ]]; then
		codesign \
			--force \
			--options runtime \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$SPARKLE_FRAMEWORK/Versions/B/Autoupdate"
	fi
	if [[ -d "$SPARKLE_FRAMEWORK/Versions/B/Updater.app" ]]; then
		codesign \
			--force \
			--options runtime \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$SPARKLE_FRAMEWORK/Versions/B/Updater.app"
	fi
	codesign \
		--force \
		--options runtime \
		"$CODE_SIGN_TIMESTAMP_FLAG" \
		--sign "$CODE_SIGN_IDENTITY" \
		"$SPARKLE_FRAMEWORK"
fi

if [[ -d "$FRAMEWORKS_PATH" ]]; then
	find "$FRAMEWORKS_PATH" \
		-maxdepth 1 \
		-name "$HELPER_APP_GLOB" \
		-type d \
		-print0 |
		while IFS= read -r -d '' helper_app; do
			helper_name="$(basename "$helper_app" .app)"
			helper_executable="$helper_app/Contents/MacOS/$helper_name"
			if [[ -x "$helper_executable" ]]; then
				codesign \
					--force \
					--options runtime \
					--entitlements "$CEF_ENTITLEMENTS" \
					"$CODE_SIGN_TIMESTAMP_FLAG" \
					--sign "$CODE_SIGN_IDENTITY" \
					"$helper_executable"
			fi
			codesign \
				--force \
				--options runtime \
				--entitlements "$CEF_ENTITLEMENTS" \
				"$CODE_SIGN_TIMESTAMP_FLAG" \
				--sign "$CODE_SIGN_IDENTITY" \
				"$helper_app"
		done
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
		-print0 |
		while IFS= read -r -d '' resource_code; do
			if file "$resource_code" | grep -q 'Mach-O'; then
				sign_plain_macho "$resource_code"
			fi
		done
}

# The bundled Ctrl+G Monaco prompt-editor helper is a nested app bundle, so
# notarization needs its executable and then the bundle signed before the outer
# app. It is a plain WKWebView Swift app: no V8/JIT entitlements.
GHOSTEX_EDITOR_APP="$APP_PATH/Contents/Resources/GhostexEditor.app"
if [[ -d "$GHOSTEX_EDITOR_APP" ]]; then
	if [[ -x "$GHOSTEX_EDITOR_APP/Contents/MacOS/GhostexEditor" ]]; then
		codesign \
			--force \
			--options runtime \
			"$CODE_SIGN_TIMESTAMP_FLAG" \
			--sign "$CODE_SIGN_IDENTITY" \
			"$GHOSTEX_EDITOR_APP/Contents/MacOS/GhostexEditor"
	fi
	codesign \
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
	# Batch 9.3), replacing ad-hoc per-build cdhash pinning.
	find "$LAUNCH_SERVICES_PATH" \
		-type f \
		-perm -111 \
		-print0 |
		while IFS= read -r -d '' helper_executable; do
			if file "$helper_executable" | grep -q 'Mach-O'; then
				codesign \
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

codesign \
	--force \
	--deep \
	--options runtime \
	--entitlements "$CEF_ENTITLEMENTS" \
	"$CODE_SIGN_TIMESTAMP_FLAG" \
	--sign "$CODE_SIGN_IDENTITY" \
	"$APP_PATH"

codesign --verify --deep --strict --verbose=2 "$APP_PATH"

# The final --deep pass must preserve the nested V8 entitlements (codesign
# deep re-signing preserves nested entitlement metadata); verify one CEF
# helper still carries allow-jit so a codesign behavior change fails the build
# instead of shipping browser panes that trap under the hardened runtime.
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
