#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GPUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$GPUI_DIR/../.." && pwd)"
APP_NAME="${GHOSTEX_GPUI_APP_NAME:-Ghostex}"
# CDXC:GPUIBundleIdentity 2026-06-28-16:18:
# GPUI source and packaged helper identity should no longer carry the historical phase label. Use one stable GPUI bundle id so CEF helper bundle ids and the lid-sleep helper label match the app's current product identity.
GPUI_BUNDLE_ID="${GHOSTEX_GPUI_BUNDLE_ID:-com.madda.ghostex.gpui}"
GPUI_LID_SLEEP_HELPER_LABEL="$GPUI_BUNDLE_ID.LidSleepHelper"
APP_PATH="$GPUI_DIR/build/macos/$APP_NAME.app"
RUN_APP=0
SOUND_SRC_DIR="$REPO_ROOT/media/sounds"
SOUND_DEST_DIR="$APP_PATH/Contents/Resources/sidebar/sounds"
CLI_DIR="$APP_PATH/Contents/Resources/CLI"
WEB_DIR="$APP_PATH/Contents/Resources/Web"
WEB_SOURCE_DIR="$GPUI_DIR/runtime/macos/Web"
WEB_BIN_SOURCE_DIR="$WEB_SOURCE_DIR/bin"
GXSERVER_SOURCE_DIR="$WEB_SOURCE_DIR/gxserver"
APP_ICON_SOURCE_SET="$GPUI_DIR/resources/AppIcon.appiconset"
APP_ICON_DEST="$APP_PATH/Contents/Resources/AppIcon.icns"
LID_SLEEP_HELPER_SOURCE_DIR="$GPUI_DIR/native/macos/lid-sleep-helper"
LID_SLEEP_HELPER_BUILD_DIR="$GPUI_DIR/build/macos-lid-sleep-helper"
GHOSTEX_EDITOR_BUILD_SCRIPT="$REPO_ROOT/apps/editor/scripts/build-editor-app.sh"
GHOSTEX_EDITOR_APP_SOURCE="$REPO_ROOT/apps/editor/dist/GhostexEditor.app"
GHOSTEX_EDITOR_APP_DEST="$APP_PATH/Contents/Resources/GhostexEditor.app"
GHOSTEX_REMOTE_GXSERVER_LINUX_X64_DEFAULT_PACKAGE="$REPO_ROOT/build/remote-gxserver-linux/x64/package"
GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_DEFAULT_PACKAGE="$REPO_ROOT/build/remote-gxserver-linux/arm64/package"
GHOSTEX_REMOTE_GXSERVER_LINUX_X64_STAGED_PACKAGE="$WEB_SOURCE_DIR/gxserver-linux-x64"
GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_STAGED_PACKAGE="$WEB_SOURCE_DIR/gxserver-linux-arm64"
GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE="${GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE:-}"
GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE="${GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE:-}"
GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES="${GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES:-0}"
GHOSTEX_ON_DEMAND_ASSETS="${GHOSTEX_ON_DEMAND_ASSETS:-0}"
GHOSTEX_GPUI_USE_PREBUILT_RUST="${GHOSTEX_GPUI_USE_PREBUILT_RUST:-0}"
case "$(printf '%s' "$GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES" | tr '[:upper:]' '[:lower:]')" in
1 | true | yes | on)
	GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1
	;;
*)
	GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=0
	;;
esac
case "$(printf '%s' "$GHOSTEX_ON_DEMAND_ASSETS" | tr '[:upper:]' '[:lower:]')" in
1 | true | yes | on)
	GHOSTEX_ON_DEMAND_ASSETS=1
	;;
*)
	GHOSTEX_ON_DEMAND_ASSETS=0
	;;
esac
case "$(printf '%s' "$GHOSTEX_GPUI_USE_PREBUILT_RUST" | tr '[:upper:]' '[:lower:]')" in
1 | true | yes | on)
	GHOSTEX_GPUI_USE_PREBUILT_RUST=1
	;;
*)
	GHOSTEX_GPUI_USE_PREBUILT_RUST=0
	;;
esac

# Versioning: Sparkle compares CFBundleVersion, so packaged GPUI builds carry
# the same semver-derived numeric build value scheme as the macOS app
# (release-ghostex.mjs releaseBuildVersion). Defaults come from the repo
# package.json version; release automation passes both explicitly.
GHOSTEX_GPUI_MARKETING_VERSION="${GHOSTEX_GPUI_MARKETING_VERSION:-}"
GHOSTEX_GPUI_BUILD_VERSION="${GHOSTEX_GPUI_BUILD_VERSION:-}"

# Sparkle auto-update: the framework is staged from the macOS app's SwiftPM
# artifacts (or GHOSTEX_GPUI_SPARKLE_FRAMEWORK); dev builds without it simply
# run without an updater. Release packaging sets GHOSTEX_REQUIRE_SPARKLE=1.
GHOSTEX_GPUI_SPARKLE_FRAMEWORK="${GHOSTEX_GPUI_SPARKLE_FRAMEWORK:-}"
GHOSTEX_GPUI_SPARKLE_FEED_URL="${GHOSTEX_GPUI_SPARKLE_FEED_URL:-https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml}"
GHOSTEX_GPUI_SPARKLE_PUBLIC_ED_KEY="${GHOSTEX_GPUI_SPARKLE_PUBLIC_ED_KEY:-AGWDPeMqfhmbjt8Pbk+VTC9fDfXAYq+cZoLGCYuGn70=}"
GHOSTEX_REQUIRE_SPARKLE="${GHOSTEX_REQUIRE_SPARKLE:-0}"
case "$(printf '%s' "$GHOSTEX_REQUIRE_SPARKLE" | tr '[:upper:]' '[:lower:]')" in
1 | true | yes | on)
	GHOSTEX_REQUIRE_SPARKLE=1
	;;
*)
	GHOSTEX_REQUIRE_SPARKLE=0
	;;
esac

# Signing: unset identity keeps the historical ad-hoc dev signing. Release
# builds pass the Developer ID identity; notarization is opt-in and uses the
# same notarytool keychain profile as the macOS release pipeline.
GHOSTEX_GPUI_SIGN_IDENTITY="${GHOSTEX_GPUI_SIGN_IDENTITY:-}"
GHOSTEX_GPUI_NOTARIZE="${GHOSTEX_GPUI_NOTARIZE:-0}"
case "$(printf '%s' "$GHOSTEX_GPUI_NOTARIZE" | tr '[:upper:]' '[:lower:]')" in
1 | true | yes | on)
	GHOSTEX_GPUI_NOTARIZE=1
	;;
*)
	GHOSTEX_GPUI_NOTARIZE=0
	;;
esac
GHOSTEX_NOTARY_PROFILE="${GHOSTEX_NOTARY_PROFILE:-notarytool-profile}"

# CDXC:GPUISettingsSounds 2026-06-24-12:10:
# Packaged GPUI Settings must preview completion sounds and run test-agent-completion from the same trusted bundle path used by the runtime lookup. Keep the packaged asset set explicit and copy only repository-owned MP3s into Contents/Resources/sidebar/sounds so React-provided values cannot expand playback to arbitrary paths.
completion_sound_assets=(
	arcade.mp3
	arcadeboost.mp3
	coin-collect.mp3
	confirmation-001.mp3
	confirmation-002.mp3
	confirmation-003.mp3
	confirmation-004.mp3
	flawless-victory.mp3
	glass.mp3
	glimmer.mp3
	high-down.mp3
	high-up.mp3
	low-three-tone.mp3
	notification-pop.mp3
	phaser-up-5.mp3
	ping.mp3
	pingdouble.mp3
	power-up-5.mp3
	power-up-6.mp3
	power-up-8.mp3
	shamisen.mp3
	shamisenreverb.mp3
	success-chime.mp3
	three-tone-1.mp3
	three-tone-2.mp3
	tone-1.mp3
	two-tone-1.mp3
	two-tone-2.mp3
	voiceover-pack-female-congratulations.mp3
	voiceover-pack-female-mission-completed.mp3
	voiceover-pack-male-mission-completed.mp3
	voiceover-pack-male-you-win.mp3
	zap-two-tone.mp3
)

bundled_cli_skill_assets=(
	ghostex-browser-use
	ghostex-embedded-browser-use
	ghostex-computer-use
	ghostex-cli
	ghostex-fable-5.6-orchestration
	ghostex-manage-beads
	ghostex-auto-rename-session
	ghostex-move-codex-session
	ghostex-manage-beads
)

validate_completion_sound_assets() {
	local missing=0
	local asset

	if [[ ! -d "$SOUND_SRC_DIR" ]]; then
		echo "Missing GPUI completion sound source directory: $SOUND_SRC_DIR" >&2
		exit 1
	fi

	for asset in "${completion_sound_assets[@]}"; do
		if [[ ! -f "$SOUND_SRC_DIR/$asset" ]]; then
			echo "Missing GPUI completion sound asset: $SOUND_SRC_DIR/$asset" >&2
			missing=1
		fi
	done

	if [[ "$missing" == "1" ]]; then
		echo "Run \`bun run start\` from the repo root so GPUI-owned shared resources are refreshed before packaging." >&2
		exit 1
	fi
}

validate_cli_resources() {
	local missing=0
	local skill_name

	# CDXC:GhostexRustCli 2026-07-13: the public CLI is the native `ghostex`
	# binary staged inside the app-owned gxserver package (no Node module or
	# shell launcher sources are needed anymore).
	if [[ ! -x "$GXSERVER_SOURCE_DIR/bin/ghostex" ]]; then
		echo "Missing GPUI CLI binary: $GXSERVER_SOURCE_DIR/bin/ghostex (run bun run start to refresh shared resources)" >&2
		missing=1
	fi

	for skill_name in "${bundled_cli_skill_assets[@]}"; do
		if [[ ! -f "$REPO_ROOT/skills/$skill_name/SKILL.md" ]]; then
			echo "Missing GPUI bundled CLI skill: $REPO_ROOT/skills/$skill_name/SKILL.md" >&2
			missing=1
		fi
	done

	if [[ "$missing" == "1" ]]; then
		exit 1
	fi
}

validate_portless_admin_runtime_resources() {
	local missing=0

	if [[ "$GHOSTEX_ON_DEMAND_ASSETS" == "1" ]]; then
		if [[ ! -f "$WEB_SOURCE_DIR/on-demand-resources.json" ]]; then
			echo "Missing sealed GPUI on-demand component manifest: $WEB_SOURCE_DIR/on-demand-resources.json" >&2
			missing=1
		fi
	elif [[ "${GHOSTEX_LOCAL_START:-0}" == "1" && ! -f "$WEB_SOURCE_DIR/code-server/out/node/entry.js" ]]; then
		# CDXC:LocalStartSourceOptional 2026-08-09:
		# prepare-macos-runtime.sh already treats a missing code-server checkout
		# as an optional skip for local starts ("Skipping optional Source
		# editor"), so failing here on the runtime it deliberately did not stage
		# made a local dev build impossible without a full VS Code build. The
		# Source tab is the only thing absent from such a build.
		# Keyed on the entrypoint, not the directory: prepare stages the bundled
		# Node runtime into code-server/lib/node even when no checkout exists,
		# so the directory is always present and proves nothing.
		echo "Local start without a staged Source runtime: skipping Source validation." >&2
	else
		# Development/bundled builds retain the complete self-contained Source
		# runtime and validate its own lib/node beside the entrypoint.
		for required_path in \
			"$WEB_SOURCE_DIR/code-server/lib/node" \
			"$WEB_SOURCE_DIR/code-server/out/node/entry.js" \
			"$WEB_SOURCE_DIR/code-server/lib/vscode/package.json" \
			"$WEB_SOURCE_DIR/code-server/lib/vscode/out/server-main.js" \
			"$WEB_SOURCE_DIR/code-server/lib/vscode/extensions/git/node_modules/@vscode/fs-copyfile/build/Release/vscode_fs.node"; do
			if [[ ! -e "$required_path" ]]; then
				echo "Missing GPUI bundled Source runtime resource: $required_path" >&2
				missing=1
			fi
		done
	fi
	if [[ ! -f "$WEB_SOURCE_DIR/portless/dist/cli.js" ]]; then
		echo "Missing GPUI Portless CLI payload: $WEB_SOURCE_DIR/portless/dist/cli.js" >&2
		missing=1
	fi

	if [[ "$missing" == "1" ]]; then
		exit 1
	fi
}

validate_local_gxserver_runtime_resources() {
	local missing=0
	local required_path executable_path

	# CDXC:GPUIStartCommand 2026-07-08-04:55:
	# `bun run start` refreshes apps/desktop/runtime/macos/Web through the GPUI-owned
	# shared-resource build, then this packager seals the
	# app-owned gxserver package into the GPUI bundle. Runtime should resolve
	# Contents/Resources/Web/gxserver first instead of depending on the main
	# Ghostex.app install or source-tree daemon paths.
	for required_path in \
		"bin/zmx" \
		"gxserver/bin/gxserver" \
		"gxserver/bin/ghostex" \
		"gxserver/bin/zmx" \
		"gxserver/build-identity.json" \
		"gxserver/dist/protocol/index.js" \
		"gxserver/dist/protocol/index.d.ts"; do
		if [[ ! -e "$WEB_SOURCE_DIR/$required_path" ]]; then
			echo "Missing GPUI local gxserver resource: $WEB_SOURCE_DIR/$required_path" >&2
			missing=1
		fi
	done

	for executable_path in \
		"$WEB_BIN_SOURCE_DIR/zmx" \
		"$GXSERVER_SOURCE_DIR/bin/gxserver" \
		"$GXSERVER_SOURCE_DIR/bin/zmx"; do
		if [[ ! -x "$executable_path" ]]; then
			echo "GPUI local gxserver resource is not executable: $executable_path" >&2
			missing=1
		fi
	done

	if [[ -e "$GXSERVER_SOURCE_DIR/bin/bd" && ! -x "$WEB_BIN_SOURCE_DIR/bd" ]]; then
		echo "GPUI local gxserver bd launcher requires executable shared Beads binary: $WEB_BIN_SOURCE_DIR/bd" >&2
		missing=1
	fi

	if [[ "$missing" == "1" ]]; then
		exit 1
	fi
}

# CDXC:GPUILocalToolchainValidation 2026-08-10:
# Local cargo builds invoke cef-dll-sys, whose CMake setup selects the Ninja
# generator unconditionally but reports a generic "build program not found"
# error that is easy to misread as a compiler or CEF issue. Fail up front with
# an actionable message instead of letting the Cargo build stall and fail.
validate_build_toolchain_dependencies() {
	local missing=0

	if ! command -v ninja >/dev/null 2>&1; then
		echo "Required build tool 'ninja' was not found on PATH." >&2
		echo "The cef-dll-sys crate drives CMake with the Ninja generator; without it the CEF build fails late." >&2
		echo "Install it with: brew install ninja" >&2
		missing=1
	fi
	if ! command -v cmake >/dev/null 2>&1; then
		echo "Required build tool 'cmake' was not found on PATH." >&2
		echo "The cef-dll-sys crate requires CMake to configure the bundled CEF sources." >&2
		echo "Install it with: brew install cmake" >&2
		missing=1
	fi

	if [[ "$missing" == "1" ]]; then
		exit 1
	fi

	# The CEF build script compiles Metal shaders, so the Metal toolchain must
	# be present even for host-only local builds.
	if ! xcrun --sdk macosx --find metal >/dev/null 2>&1; then
		echo "The macOS Metal toolchain is not installed; the CEF build cannot compile its shaders." >&2
		echo "Install it with: xcodebuild -downloadComponent MetalToolchain" >&2
		exit 1
	fi
}

# CDXC:GPUIGhosttyKitLocalPrereq 2026-08-10:
# apps/desktop/build.rs links the repo-local GhosttyKit static archive by exact path
# (apps/desktop/build.rs links .dependencies/ghostty/macos/GhosttyKit.xcframework/.../ghostty-internal.a).
# The dev path never builds it; the release pipeline does. Detecting the absence
# up front avoids a long Rust compile that only fails at link time.
validate_ghosttykit_archive() {
	local ghostty_kit="$REPO_ROOT/.dependencies/ghostty/macos/GhosttyKit.xcframework/macos-arm64_x86_64"
	if [[ ! -f "$ghostty_kit/ghostty-internal.a" || ! -f "$ghostty_kit/Headers/ghostty.h" ]]; then
		echo "Missing repo-local GhosttyKit static archive: $ghostty_kit" >&2
		echo "gpui/build.rs links this archive by exact path; the dev build does not build it." >&2
		echo "Build it from the vendored Ghostty source with:" >&2
		echo "  (cd \"$REPO_ROOT/.dependencies/ghostty\" && ZIG=\${ZIG:-\$(command -v zig)} && \\" >&2
		echo "     env DEVELOPER_DIR=\"\$(xcode-select -p)\" \\" >&2
		echo "       SDKROOT=\"\$(DEVELOPER_DIR=\"\$(xcode-select -p)\" xcrun --sdk macosx --show-sdk-path)\" \\" >&2
		echo "       GHOSTTY_METAL_DEVELOPER_DIR=\"\$(xcode-select -p)\" \\" >&2
		echo "       \"\$ZIG\" build -Demit-xcframework -Dxcframework-target=universal -Demit-macos-app=false -Doptimize=ReleaseSafe)" >&2
		echo "Requires Zig 0.16.x (\`mise install zig@0.16.0\` or \`brew install zig\`) and the Metal toolchain." >&2
		exit 1
	fi
}

run_developer_xcrun() {
	local developer_dir="$1"
	shift
	if [[ -n "$developer_dir" ]]; then
		DEVELOPER_DIR="$developer_dir" xcrun "$@"
	else
		xcrun "$@"
	fi
}

developer_macos_sdk_path() {
	local developer_dir="$1"
	run_developer_xcrun "$developer_dir" --sdk macosx --show-sdk-path
}

gpui_lid_sleep_helper_swift_supported() {
	local developer_dir="$1"
	local swift_target="$2"
	local sdk_path

	sdk_path="$(developer_macos_sdk_path "$developer_dir" 2>/dev/null)" || return 1
	if [[ ! -d "$sdk_path" ]]; then
		return 1
	fi

	if [[ -n "$developer_dir" ]]; then
		DEVELOPER_DIR="$developer_dir" SDKROOT="$sdk_path" xcrun swiftc \
			-target "$swift_target" \
			-sdk "$sdk_path" \
			-typecheck \
			"$LID_SLEEP_HELPER_SOURCE_DIR/Shared/GhostexLidSleepHelperProtocol.swift" \
			"$LID_SLEEP_HELPER_SOURCE_DIR/GhostexLidSleepHelper/main.swift" \
			>/dev/null 2>&1
	else
		SDKROOT="$sdk_path" xcrun swiftc \
			-target "$swift_target" \
			-sdk "$sdk_path" \
			-typecheck \
			"$LID_SLEEP_HELPER_SOURCE_DIR/Shared/GhostexLidSleepHelperProtocol.swift" \
			"$LID_SLEEP_HELPER_SOURCE_DIR/GhostexLidSleepHelper/main.swift" \
			>/dev/null 2>&1
	fi
}

resolve_gpui_lid_sleep_helper_swift_developer_dir() {
	local swift_target="$1"
	local candidate
	local active_developer_dir
	local checked_developer_dirs=""

	if [[ -n "${GHOSTEX_GPUI_SWIFT_DEVELOPER_DIR:-}" ]]; then
		if gpui_lid_sleep_helper_swift_supported "$GHOSTEX_GPUI_SWIFT_DEVELOPER_DIR" "$swift_target"; then
			printf '%s\n' "$GHOSTEX_GPUI_SWIFT_DEVELOPER_DIR"
			return
		fi
		echo "GHOSTEX_GPUI_SWIFT_DEVELOPER_DIR does not provide a Swift compiler and macOS SDK that can build the GPUI lid-sleep helper: $GHOSTEX_GPUI_SWIFT_DEVELOPER_DIR" >&2
		exit 1
	fi

	if [[ -n "${DEVELOPER_DIR:-}" ]]; then
		if gpui_lid_sleep_helper_swift_supported "$DEVELOPER_DIR" "$swift_target"; then
			printf '%s\n' "$DEVELOPER_DIR"
			return
		fi
		echo "DEVELOPER_DIR does not provide a Swift compiler and macOS SDK that can build the GPUI lid-sleep helper: $DEVELOPER_DIR" >&2
		exit 1
	fi

	active_developer_dir="$(xcode-select -p 2>/dev/null || true)"
	for candidate in \
		"$active_developer_dir" \
		"/Applications/Xcode.app/Contents/Developer" \
		"/Applications/Xcode-beta.app/Contents/Developer"; do
		if [[ -z "$candidate" || ! -d "$candidate" ]]; then
			continue
		fi
		case "$checked_developer_dirs" in
		*"
$candidate
"*)
			continue
			;;
		esac
		checked_developer_dirs="$checked_developer_dirs
$candidate
"
		if gpui_lid_sleep_helper_swift_supported "$candidate" "$swift_target"; then
			printf '%s\n' "$candidate"
			return
		fi
	done

	echo "Could not find a Swift compiler and macOS SDK pair that can build the GPUI lid-sleep helper." >&2
	echo "Install or select a matching Xcode, or set GHOSTEX_GPUI_SWIFT_DEVELOPER_DIR to a valid Contents/Developer path." >&2
	exit 1
}

build_gpui_lid_sleep_helper() {
	local helper_build_dir="$LID_SLEEP_HELPER_BUILD_DIR/$GHOSTEX_MACOS_ARCH"
	local helper_binary="$helper_build_dir/$GPUI_LID_SLEEP_HELPER_LABEL"
	local helper_info_plist="$helper_build_dir/Info.plist"
	local swift_target="$GHOSTEX_MACOS_ARCH-apple-macos13.0"
	local helper_swift_developer_dir
	local helper_sdk_path

	# CDXC:GPUITitlebarKeepAwake 2026-06-26-00:09:
	# Packaged GPUI closed-lid Keep Awake must ship the real Swift privileged helper under the GPUI helper label. Build from the native app's reviewed helper/protocol sources with an embedded helper bundle id so GPUI installs the same narrow XPC daemon instead of a stub or direct pmset path.
	rm -rf "$helper_build_dir"
	mkdir -p "$helper_build_dir"
	cat >"$helper_info_plist" <<EOF_HELPER_PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleExecutable</key>
	<string>$GPUI_LID_SLEEP_HELPER_LABEL</string>
	<key>CFBundleIdentifier</key>
	<string>$GPUI_LID_SLEEP_HELPER_LABEL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>$GPUI_LID_SLEEP_HELPER_LABEL</string>
	<key>CFBundlePackageType</key>
	<string>BNDL</string>
	<key>CFBundleShortVersionString</key>
	<string>$GPUI_MARKETING_VERSION</string>
	<key>CFBundleVersion</key>
	<string>$GPUI_BUILD_VERSION</string>
	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
</dict>
</plist>
EOF_HELPER_PLIST
	helper_swift_developer_dir="$(resolve_gpui_lid_sleep_helper_swift_developer_dir "$swift_target")"
	helper_sdk_path="$(developer_macos_sdk_path "$helper_swift_developer_dir")"
	echo "Building GPUI lid-sleep helper with $helper_swift_developer_dir and $helper_sdk_path" >&2
	if ! DEVELOPER_DIR="$helper_swift_developer_dir" SDKROOT="$helper_sdk_path" xcrun swiftc \
		-target "$swift_target" \
		-sdk "$helper_sdk_path" \
		-O \
		-module-name GhostexLidSleepHelper \
		-o "$helper_binary" \
		"$LID_SLEEP_HELPER_SOURCE_DIR/Shared/GhostexLidSleepHelperProtocol.swift" \
		"$LID_SLEEP_HELPER_SOURCE_DIR/GhostexLidSleepHelper/main.swift" \
		-Xlinker -sectcreate \
		-Xlinker __TEXT \
		-Xlinker __info_plist \
		-Xlinker "$helper_info_plist"; then
		echo "GPUI lid-sleep helper Swift build failed." >&2
		exit 1
	fi
	chmod 755 "$helper_binary"
	if [[ ! -x "$helper_binary" ]]; then
		echo "GPUI lid-sleep helper build did not produce an executable helper." >&2
		exit 1
	fi
	printf '%s\n' "$helper_binary"
}

# The Ctrl+G "Monaco editor" prompt-editor backend is served by the standalone
# GhostexEditor daemon. Ship it inside the app bundle so installed builds resolve
# a real helper; without it the setting silently degrades to the machine editor
# (vi for anyone with no $EDITOR).
stage_gpui_ghostex_editor_app() {
	local editor_executable="$GHOSTEX_EDITOR_APP_DEST/Contents/MacOS/GhostexEditor"

	echo "Building bundled GhostexEditor helper for $GHOSTEX_MACOS_ARCH" >&2
	GHOSTEX_EDITOR_ARCH="$GHOSTEX_MACOS_ARCH" \
		GHOSTEX_EDITOR_SWIFT_DEVELOPER_DIR="${GHOSTEX_GPUI_SWIFT_DEVELOPER_DIR:-${DEVELOPER_DIR:-}}" \
		bash "$GHOSTEX_EDITOR_BUILD_SCRIPT"

	if [[ ! -x "$GHOSTEX_EDITOR_APP_SOURCE/Contents/MacOS/GhostexEditor" ]]; then
		echo "GhostexEditor build did not produce $GHOSTEX_EDITOR_APP_SOURCE/Contents/MacOS/GhostexEditor" >&2
		exit 1
	fi

	rm -rf "$GHOSTEX_EDITOR_APP_DEST"
	/usr/bin/ditto "$GHOSTEX_EDITOR_APP_SOURCE" "$GHOSTEX_EDITOR_APP_DEST"
	if [[ ! -x "$editor_executable" ]]; then
		echo "Packaged GhostexEditor helper is missing or not executable: $editor_executable" >&2
		exit 1
	fi
	if [[ ! -f "$GHOSTEX_EDITOR_APP_DEST/Contents/Resources/Web/index.html" || ! -f "$GHOSTEX_EDITOR_APP_DEST/Contents/Resources/Web/monaco/vs/loader.js" ]]; then
		echo "Packaged GhostexEditor helper is missing its Monaco web payload." >&2
		exit 1
	fi
	if ! /usr/bin/lipo -archs "$editor_executable" | tr ' ' '\n' | grep -Fxq "$GHOSTEX_MACOS_ARCH"; then
		echo "Packaged GhostexEditor helper does not contain $GHOSTEX_MACOS_ARCH: $editor_executable" >&2
		exit 1
	fi
}

stage_gpui_lid_sleep_helper() {
	local helper_source helper_dir helper_target

	helper_source="$(build_gpui_lid_sleep_helper)"
	helper_dir="$APP_PATH/Contents/Library/LaunchServices"
	helper_target="$helper_dir/$GPUI_LID_SLEEP_HELPER_LABEL"
	mkdir -p "$helper_dir"
	install -m 0755 "$helper_source" "$helper_target"
	if [[ ! -x "$helper_target" ]]; then
		echo "Packaged GPUI lid-sleep helper is missing or not executable." >&2
		exit 1
	fi
}

validate_remote_gxserver_linux_package() {
	local package_dir="$1"
	local package_label="$2"
	local required_path file_output

	# Portless is macOS launchd-only and is no longer staged in Linux packages.
	for required_path in \
		"bin/gxserver" \
		"bin/zmx" \
		"bin/bd"; do
		if [[ ! -e "$package_dir/$required_path" ]]; then
			echo "Remote gxserver $package_label package is missing required resource: $required_path" >&2
			return 1
		fi
	done

	if [[ ! -f "$package_dir/bin/ghostex" ]]; then
		echo "Remote gxserver $package_label package is missing the native Ghostex CLI: bin/ghostex" >&2
		return 1
	fi

	for required_path in \
		"bin/gxserver" \
		"bin/zmx" \
		"bin/bd"; do
		file_output="$(file "$package_dir/$required_path")"
		if [[ "$file_output" == *"Mach-O"* ]]; then
			echo "Remote gxserver $package_label package contains a macOS binary at $required_path; Linux packages must not ship Mach-O payloads." >&2
			return 1
		fi
		if [[ "$file_output" != *"ELF"* ]]; then
			echo "Remote gxserver $package_label package must contain a native Linux ELF payload at $required_path." >&2
			return 1
		fi
		case "$package_label" in
		LINUX_X64)
			if [[ "$file_output" != *"x86-64"* && "$file_output" != *"x86_64"* ]]; then
				echo "Remote gxserver $package_label package has the wrong Linux ELF architecture at $required_path." >&2
				return 1
			fi
			;;
		LINUX_ARM64)
			if [[ "$file_output" != *"aarch64"* && "$file_output" != *"AArch64"* ]]; then
				echo "Remote gxserver $package_label package has the wrong Linux ELF architecture at $required_path." >&2
				return 1
			fi
			;;
		esac
	done
}

resolve_remote_gxserver_linux_package_source() {
	local configured_source="$1"
	local default_source="$2"
	local staged_source="$3"
	if [[ -n "$configured_source" ]]; then
		printf '%s\n' "$configured_source"
	elif [[ -d "$default_source" ]]; then
		printf '%s\n' "$default_source"
	elif [[ -d "$staged_source" ]]; then
		printf '%s\n' "$staged_source"
	fi
	return 0
}

stage_remote_gxserver_linux_package_if_available() {
	local configured_source="$1"
	local target_name="$2"
	local package_label="$3"
	local default_source="$4"
	local staged_source="$5"
	local source_dir target_dir validation_output
	local source_is_default=0

	source_dir="$(resolve_remote_gxserver_linux_package_source "$configured_source" "$default_source" "$staged_source")"
	target_dir="$WEB_DIR/$target_name"
	if [[ -z "$configured_source" && -d "$default_source" && "$source_dir" == "$default_source" ]]; then
		source_is_default=1
	fi
	if [[ -z "$source_dir" ]]; then
		if [[ "$GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES" == "1" ]]; then
			echo "Missing $package_label remote gxserver package. Set GHOSTEX_REMOTE_GXSERVER_${package_label}_PACKAGE to a prebuilt Linux package directory." >&2
			exit 1
		fi
		return 0
	fi
	if [[ ! -d "$source_dir" ]]; then
		echo "Configured $package_label remote gxserver package is not a directory." >&2
		exit 1
	fi
	if ! validation_output="$(validate_remote_gxserver_linux_package "$source_dir" "$package_label" 2>&1)"; then
		# Auto-discovered packages under build/ are optional for local starts and
		# can become stale when the Linux package contract gains a new resource.
		# Explicit package paths and release builds remain strict.
		if [[ "$source_is_default" == "1" && "$GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES" != "1" ]]; then
			echo "Remote gxserver $package_label default package is stale or incomplete; skipping optional staging." >&2
			rm -rf "$target_dir"
			return 0
		fi
		printf '%s\n' "$validation_output" >&2
		exit 1
	fi
	# CDXC:GPUIRemoteMachines 2026-06-24-20:08:
	# GPUI remote gxserver install parity may stage only explicit prebuilt Linux packages into Contents/Resources/Web. Validate required gxserver/zmx/bd/Node/Portless/CLI resources and reject Mach-O or wrong-architecture payloads before copying, so runtime install never falls back to source checkout paths or uploads a host macOS package to Linux.
	rm -rf "$target_dir"
	mkdir -p "$target_dir"
	rsync -a --delete "$source_dir"/ "$target_dir"/
}

stage_remote_gxserver_linux_packages_if_available() {
	stage_remote_gxserver_linux_package_if_available "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE" "gxserver-linux-x64" "LINUX_X64" "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_DEFAULT_PACKAGE" "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_STAGED_PACKAGE"
	stage_remote_gxserver_linux_package_if_available "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE" "gxserver-linux-arm64" "LINUX_ARM64" "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_DEFAULT_PACKAGE" "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_STAGED_PACKAGE"
}

stage_on_demand_remote_gxserver_manifest() {
	local version asset_dir build_manifest component_manifest x64_source arm64_source x64_archive arm64_archive x64_sha arm64_sha
	local -a manifest_args

	version="$(resolve_gpui_marketing_version)"
	asset_dir="${GHOSTEX_ON_DEMAND_ASSET_DIR:-$REPO_ROOT/build/on-demand-assets/$version}"
	build_manifest="$asset_dir/assets.json"
	if [[ ! -f "$build_manifest" ]]; then
		x64_source="$(resolve_remote_gxserver_linux_package_source "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE" "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_DEFAULT_PACKAGE" "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_STAGED_PACKAGE")"
		arm64_source="$(resolve_remote_gxserver_linux_package_source "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE" "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_DEFAULT_PACKAGE" "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_STAGED_PACKAGE")"
		if [[ -z "$x64_source" || -z "$arm64_source" ]]; then
			echo "GHOSTEX_ON_DEMAND_ASSETS=1 requires prebuilt Linux x64 and arm64 remote gxserver packages." >&2
			exit 1
		fi
		validate_remote_gxserver_linux_package "$x64_source" "LINUX_X64" || exit 1
		validate_remote_gxserver_linux_package "$arm64_source" "LINUX_ARM64" || exit 1
		rm -rf "$asset_dir"
		mkdir -p "$asset_dir"
		x64_archive="$asset_dir/gxserver-linux-x64.tar.gz"
		arm64_archive="$asset_dir/gxserver-linux-arm64.tar.gz"
		COPYFILE_DISABLE=1 /usr/bin/tar -czf "$x64_archive" -C "$x64_source" .
		COPYFILE_DISABLE=1 /usr/bin/tar -czf "$arm64_archive" -C "$arm64_source" .
		x64_sha="$(/usr/bin/shasum -a 256 "$x64_archive" | awk '{print $1}')"
		arm64_sha="$(/usr/bin/shasum -a 256 "$arm64_archive" | awk '{print $1}')"
		GHOSTEX_ODA_VERSION="$version" \
			GHOSTEX_ODA_ASSET_DIR="$asset_dir" \
			GHOSTEX_ODA_X64_SHA="$x64_sha" \
			GHOSTEX_ODA_ARM64_SHA="$arm64_sha" \
			node -e '
			const fs = require("fs");
			const path = require("path");
			const env = process.env;
			const entries = [
				{ key: "gxserver-linux-x64", name: "gxserver-linux-x64.tar.gz", sha256: env.GHOSTEX_ODA_X64_SHA },
				{ key: "gxserver-linux-arm64", name: "gxserver-linux-arm64.tar.gz", sha256: env.GHOSTEX_ODA_ARM64_SHA },
			].map((entry) => {
				const filePath = path.join(env.GHOSTEX_ODA_ASSET_DIR, entry.name);
				return { ...entry, bytes: fs.statSync(filePath).size, path: filePath };
			});
			for (const entry of entries) {
				if (!/^[0-9a-f]{64}$/.test(entry.sha256 ?? "")) {
					console.error(`Invalid sha256 for on-demand asset ${entry.name}: ${entry.sha256}`);
					process.exit(1);
				}
			}
			fs.writeFileSync(
				path.join(env.GHOSTEX_ODA_ASSET_DIR, "assets.json"),
				`${JSON.stringify({ assets: entries, version: env.GHOSTEX_ODA_VERSION }, null, 2)}\n`,
			);
		'
	fi
	component_manifest="${GHOSTEX_ON_DEMAND_COMPONENTS_MANIFEST:-$REPO_ROOT/build/on-demand-components/components.json}"
	if [[ -n "${GHOSTEX_ON_DEMAND_COMPONENTS_MANIFEST:-}" && ! -f "$component_manifest" ]]; then
		echo "Configured component manifest does not exist: $component_manifest" >&2
		exit 1
	fi
	manifest_args=(
		seal
		--build-manifest "$build_manifest"
		--output "$WEB_DIR/on-demand-resources.json"
		--repo "maddada/Ghostex"
	)
	if [[ -f "$component_manifest" ]]; then
		manifest_args+=(--component-manifest "$component_manifest")
	fi
	node "$REPO_ROOT/tooling/release-gpui/on-demand-manifest.mjs" "${manifest_args[@]}"
	node "$REPO_ROOT/tooling/release-gpui/on-demand-manifest.mjs" validate-macos \
		--manifest "$WEB_DIR/on-demand-resources.json"
	rm -rf "$WEB_DIR/gxserver-linux-x64" "$WEB_DIR/gxserver-linux-arm64" "$WEB_DIR/gxserver-linux-amd64" "$WEB_DIR/gxserver-linux-aarch64"
}

resolve_gpui_marketing_version() {
	if [[ -n "$GHOSTEX_GPUI_MARKETING_VERSION" ]]; then
		printf '%s\n' "$GHOSTEX_GPUI_MARKETING_VERSION"
		return
	fi
	local package_version
	package_version="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$REPO_ROOT/package.json" | head -n 1)"
	if [[ -z "$package_version" ]]; then
		echo "Could not resolve the GPUI marketing version from package.json; set GHOSTEX_GPUI_MARKETING_VERSION." >&2
		exit 1
	fi
	printf '%s\n' "$package_version"
}

derive_gpui_build_version() {
	local version="${1%%-*}"
	local major minor patch rest
	IFS='.' read -r major minor patch rest <<<"$version"
	if ! [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ && "$patch" =~ ^[0-9]+$ ]]; then
		echo "GPUI version '$1' is not MAJOR.MINOR.PATCH; set GHOSTEX_GPUI_BUILD_VERSION explicitly." >&2
		exit 1
	fi
	printf '%s\n' "$((major * 10000 + minor * 100 + patch))"
}

resolve_gpui_sparkle_framework_source() {
	if [[ -n "$GHOSTEX_GPUI_SPARKLE_FRAMEWORK" ]]; then
		printf '%s\n' "$GHOSTEX_GPUI_SPARKLE_FRAMEWORK"
		return
	fi
	local candidate
	for candidate in \
		"$REPO_ROOT/build/arm64/SourcePackages/artifacts/sparkle/Sparkle/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework" \
		"$REPO_ROOT/build/SourcePackages/artifacts/sparkle/Sparkle/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework" \
		"/tmp/ghostex-xcodebuild/SourcePackages/artifacts/sparkle/Sparkle/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework"; do
		if [[ -d "$candidate" ]]; then
			printf '%s\n' "$candidate"
			return
		fi
	done
	return 0
}

stage_gpui_sparkle_framework_if_available() {
	local source_dir
	source_dir="$(resolve_gpui_sparkle_framework_source)"
	if [[ -z "$source_dir" || ! -d "$source_dir" ]]; then
		if [[ "$GHOSTEX_REQUIRE_SPARKLE" == "1" ]]; then
			echo "Missing Sparkle.framework for the GPUI bundle. Build the macOS app once so SwiftPM downloads Sparkle, or set GHOSTEX_GPUI_SPARKLE_FRAMEWORK to a Sparkle.framework directory." >&2
			exit 1
		fi
		return 0
	fi
	if [[ ! -f "$source_dir/Sparkle" && ! -L "$source_dir/Sparkle" ]]; then
		echo "Configured Sparkle framework source does not look like Sparkle.framework: $source_dir" >&2
		exit 1
	fi
	stage_framework_directory "$source_dir" "$APP_PATH/Contents/Frameworks/$(basename "$source_dir")"
}

stage_gpui_app_icon() {
	local writer="$SCRIPT_DIR/write-macos-icns.py"

	if [[ ! -d "$APP_ICON_SOURCE_SET" ]]; then
		echo "Missing canonical Ghostex app icon asset set: $APP_ICON_SOURCE_SET" >&2
		exit 1
	fi
	if ! command -v python3 >/dev/null 2>&1; then
		echo "Missing python3; cannot encode the GPUI app icon from $APP_ICON_SOURCE_SET." >&2
		exit 1
	fi
	if [[ ! -f "$writer" ]]; then
		echo "Missing app icon encoder: $writer" >&2
		exit 1
	fi

	mkdir -p "$(dirname "$APP_ICON_DEST")"
	python3 "$writer" "$APP_ICON_SOURCE_SET" "$APP_ICON_DEST"
	if [[ ! -s "$APP_ICON_DEST" ]]; then
		echo "App icon encoder did not write $APP_ICON_DEST" >&2
		exit 1
	fi
}

sign_gpui_app_bundle() {
	GHOSTEX_GPUI_SIGN_IDENTITY="${GHOSTEX_GPUI_SIGN_IDENTITY:--}" \
		GHOSTEX_GPUI_LID_SLEEP_HELPER_LABEL="$GPUI_LID_SLEEP_HELPER_LABEL" \
		GHOSTEX_GPUI_HELPER_APP_GLOB="$APP_NAME Helper*.app" \
		/bin/bash "$SCRIPT_DIR/codesign-gpui-app.sh" "$APP_PATH"
}

notarize_and_staple_gpui_app_if_requested() {
	if [[ "$GHOSTEX_GPUI_NOTARIZE" != "1" ]]; then
		return 0
	fi
	if [[ -z "$GHOSTEX_GPUI_SIGN_IDENTITY" || "$GHOSTEX_GPUI_SIGN_IDENTITY" == "-" ]]; then
		echo "GHOSTEX_GPUI_NOTARIZE=1 requires GHOSTEX_GPUI_SIGN_IDENTITY (ad-hoc signatures cannot be notarized)." >&2
		exit 1
	fi
	local notarize_zip="$GPUI_DIR/build/macos/$APP_NAME-notarize.zip"
	rm -f "$notarize_zip"
	/usr/bin/ditto -c -k --keepParent "$APP_PATH" "$notarize_zip"
	xcrun notarytool submit "$notarize_zip" --keychain-profile "$GHOSTEX_NOTARY_PROFILE" --wait
	rm -f "$notarize_zip"
	xcrun stapler staple "$APP_PATH"
	xcrun stapler validate "$APP_PATH"
}

prepare_gpui_app_bundle_path() {
	if [[ ! -e "$APP_PATH" ]]; then
		return
	fi

	# CDXC:GPUIPackaging 2026-06-26-05:23:
	# Rebuilding GPUI must replace the app bundle even when the previous CEF framework leaves a partially removable directory tree. Move the old bundle off the canonical path first, then best-effort clean the stale bundle so packaging can create a fresh app without launching, restarting, or relying on in-place framework deletion.
	local stale_app_path="$APP_PATH.stale.$$"
	rm -rf "$stale_app_path"
	mv "$APP_PATH" "$stale_app_path"
	chmod -R u+w "$stale_app_path" 2>/dev/null || true
	if ! rm -rf "$stale_app_path"; then
		printf 'Warning: could not fully remove stale GPUI app bundle: %s\n' "$stale_app_path" >&2
	fi
}

stage_framework_directory() {
	local source_dir="$1"
	local target_dir="$2"
	local target_parent temp_dir

	if [[ ! -d "$source_dir" ]]; then
		echo "Framework source is not a directory: $source_dir" >&2
		exit 1
	fi

	target_parent="$(dirname "$target_dir")"
	temp_dir="$target_parent/.$(basename "$target_dir").staging.$$"
	rm -rf "$temp_dir"
	mkdir -p "$temp_dir"
	if ! rsync -a "$source_dir"/ "$temp_dir"/; then
		rm -rf "$temp_dir"
		exit 1
	fi
	if ! rm -rf "$target_dir"; then
		rm -rf "$temp_dir"
		exit 1
	fi
	if ! mv "$temp_dir" "$target_dir"; then
		rm -rf "$temp_dir"
		exit 1
	fi
}

cef_component_version() {
	local cef_distribution_root version_header raw_version
	cef_distribution_root="$(dirname "$CEF_FRAMEWORK")"
	version_header="$cef_distribution_root/include/cef_version.h"
	raw_version="$(sed -n 's/^#define CEF_VERSION "\([^"]*\)"$/\1/p' "$version_header" | head -n 1)"
	if [[ -z "$raw_version" ]]; then
		echo "Could not resolve the cef-rs CEF version from $version_header" >&2
		exit 1
	fi
	printf '%s\n' "$raw_version" | sed 's/[^A-Za-z0-9._-]/-/g'
}

prepare_cef_component_asset() {
	local component_version component_tag asset_dir asset_path component_manifest stage_root staged_framework
	local published_asset_names signing_identity expected_team actual_team
	component_version="$(cef_component_version)"
	component_tag="cef-$component_version"
	asset_dir="${GHOSTEX_ON_DEMAND_COMPONENT_ASSET_DIR:-$REPO_ROOT/build/on-demand-components/assets}"
	component_manifest="${GHOSTEX_ON_DEMAND_COMPONENTS_MANIFEST:-$REPO_ROOT/build/on-demand-components/components.json}"
	asset_path="$asset_dir/cef-$component_version-darwin-$GHOSTEX_MACOS_ARCH.tar.gz"
	stage_root="$(mktemp -d "$GPUI_DIR/build/cef-component-stage-XXXXXX")"
	staged_framework="$stage_root/Chromium Embedded Framework.framework"
	mkdir -p "$asset_dir"
	published_asset_names=""
	if command -v gh >/dev/null 2>&1; then
		published_asset_names="$(gh release view "$component_tag" --repo maddada/Ghostex --json assets --jq '.assets[].name' 2>/dev/null || true)"
	fi
	if printf '%s\n' "$published_asset_names" | grep -Fxq "$(basename "$asset_path")"; then
		gh release download "$component_tag" \
			--repo maddada/Ghostex \
			--pattern "$(basename "$asset_path")" \
			--dir "$asset_dir" \
			--clobber
		/usr/bin/tar -xzf "$asset_path" -C "$stage_root"
		codesign --verify --deep --strict --verbose=2 "$staged_framework"
		echo "Reused published signed CEF component $component_tag."
	else
		stage_framework_directory "$CEF_FRAMEWORK" "$staged_framework"
		GHOSTEX_GPUI_SIGN_IDENTITY="${GHOSTEX_GPUI_SIGN_IDENTITY:--}" \
			GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG="${GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG:---timestamp}" \
			"$SCRIPT_DIR/codesign-gpui-app.sh" --cef-framework "$staged_framework"
		"$REPO_ROOT/tooling/release-gpui/create-deterministic-tar.sh" "$stage_root" "$asset_path"
	fi
	signing_identity="${GHOSTEX_GPUI_SIGN_IDENTITY:--}"
	if [[ "$signing_identity" != "-" ]]; then
		expected_team="$(printf '%s\n' "$signing_identity" | sed -n 's/.*(\([A-Z0-9][A-Z0-9]*\))$/\1/p')"
		actual_team="$(codesign -dv --verbose=4 "$staged_framework" 2>&1 | sed -n 's/^TeamIdentifier=//p')"
		if [[ -z "$expected_team" || "$actual_team" != "$expected_team" ]]; then
			echo "Signed CEF component team mismatch: expected ${expected_team:-unknown}, found ${actual_team:-none}." >&2
			exit 1
		fi
	fi
	rm -rf "$stage_root"
	node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs" \
		--metadata-only \
		--component cef \
		--version "$component_version" \
		--asset-dir "$asset_dir" \
		--output "$component_manifest"
	echo "Prepared signed CEF component $component_version: $asset_path"
}

while [[ $# -gt 0 ]]; do
	case "$1" in
	--run)
		RUN_APP=1
		shift
		;;
	*)
		echo "Unknown argument: $1" >&2
		exit 1
		;;
	esac
done

default_macos_arch() {
	if [[ "$(/usr/sbin/sysctl -in hw.optional.arm64 2>/dev/null || true)" == "1" ]]; then
		printf 'arm64\n'
		return
	fi
	uname -m
}

GHOSTEX_MACOS_ARCH="${GHOSTEX_MACOS_ARCH:-$(default_macos_arch)}"
case "$GHOSTEX_MACOS_ARCH" in
arm64 | aarch64)
	GHOSTEX_MACOS_ARCH="arm64"
	RUST_TARGET_ARCH="aarch64"
	;;
x86_64 | x64 | amd64)
	GHOSTEX_MACOS_ARCH="x86_64"
	RUST_TARGET_ARCH="x86_64"
	;;
*)
	echo "Unsupported GHOSTEX_MACOS_ARCH: $GHOSTEX_MACOS_ARCH" >&2
	exit 1
	;;
esac

CEF_CACHE_DIR="$GPUI_DIR/build/cef-cache"
export CEF_PATH="$CEF_CACHE_DIR"

GPUI_MARKETING_VERSION="$(resolve_gpui_marketing_version)"
GPUI_BUILD_VERSION="${GHOSTEX_GPUI_BUILD_VERSION:-$(derive_gpui_build_version "$GPUI_MARKETING_VERSION")}"

validate_completion_sound_assets
validate_cli_resources
validate_portless_admin_runtime_resources
validate_local_gxserver_runtime_resources
validate_build_toolchain_dependencies
validate_ghosttykit_archive

(
	cd "$REPO_ROOT"
	bun run build:sidebar-css
	bunx vite build --config "$GPUI_DIR/vite.config.ts"
)

if [[ "$GHOSTEX_GPUI_USE_PREBUILT_RUST" == "1" ]]; then
	for binary_path in \
		"$GPUI_DIR/target/release/ghostex-gpui" \
		"$GPUI_DIR/target/release/ghostex-gpui-cef-helper"; do
		if [[ ! -x "$binary_path" ]]; then
			echo "Prebuilt GPUI Rust artifact is missing executable: $binary_path" >&2
			exit 1
		fi
		if ! /usr/bin/lipo -archs "$binary_path" | tr ' ' '\n' | grep -Fxq "$GHOSTEX_MACOS_ARCH"; then
			echo "Prebuilt GPUI Rust artifact does not contain $GHOSTEX_MACOS_ARCH: $binary_path" >&2
			exit 1
		fi
	done
	echo "Using prebuilt GPUI Rust binaries and CEF payload."
else
	(
		cd "$GPUI_DIR"
		cargo build --release --bins
	)
fi

CEF_FRAMEWORK="$(find "$CEF_CACHE_DIR" -path '*/Chromium Embedded Framework.framework' -type d -print -quit)"
if [[ -z "$CEF_FRAMEWORK" || ! -d "$CEF_FRAMEWORK" ]]; then
	echo "cef-rs did not produce a Chromium Embedded Framework under $CEF_CACHE_DIR" >&2
	exit 1
fi

if [[ "$GHOSTEX_ON_DEMAND_ASSETS" == "1" ]]; then
	prepare_cef_component_asset
fi

prepare_gpui_app_bundle_path
mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources" "$APP_PATH/Contents/Frameworks"
cp "$GPUI_DIR/target/release/ghostex-gpui" "$APP_PATH/Contents/MacOS/$APP_NAME"
chmod 755 "$APP_PATH/Contents/MacOS/$APP_NAME"
stage_gpui_app_icon
if [[ "$GHOSTEX_ON_DEMAND_ASSETS" != "1" ]]; then
	stage_framework_directory "$CEF_FRAMEWORK" "$APP_PATH/Contents/Frameworks/$(basename "$CEF_FRAMEWORK")"
fi
rsync -a --delete "$GPUI_DIR/dist/sidebar/" "$APP_PATH/Contents/Resources/sidebar/"
# The composited terminal engine uses Ghostty's real zsh integration for OSC
# 133 prompt semantics. Keep the upstream loader beside the packaged binary so
# an idle shell is recognized as safe to close instead of being mistaken for a
# running foreground command.
rm -rf "$APP_PATH/Contents/Resources/shell-integration"
mkdir -p "$APP_PATH/Contents/Resources/shell-integration/zsh"
install -m 0644 "$REPO_ROOT/.dependencies/ghostty/src/shell-integration/zsh/.zshenv" \
	"$APP_PATH/Contents/Resources/shell-integration/zsh/.zshenv"
install -m 0644 "$REPO_ROOT/.dependencies/ghostty/src/shell-integration/zsh/ghostty-integration" \
	"$APP_PATH/Contents/Resources/shell-integration/zsh/ghostty-integration"
rm -rf "$SOUND_DEST_DIR"
mkdir -p "$SOUND_DEST_DIR"
for asset in "${completion_sound_assets[@]}"; do
	install -m 0644 "$SOUND_SRC_DIR/$asset" "$SOUND_DEST_DIR/$asset"
done

# CDXC:GPUISettingsCliInstall 2026-06-24-12:56:
# GPUI Settings CLI repair may only link public wrappers to app-owned bundled resources. Stage the public ghostex/gx commands and bundled Ghostex skills under Contents/Resources/CLI so packaged repairs and fixed `ghostex ... install-skill` actions do not depend on a source checkout.
# CDXC:CodexSessionMove 2026-06-26-13:47: The GPUI app bundle must carry `$ghostex-move-codex-session` with the other CLI skills so `ghostex move-codex-session install-skill` works from installed builds.
# CDXC:GhostexRustCli 2026-07-13: `ghostex` is the native Rust CLI from the
# app-owned gxserver package; `gx` is a bundle-internal symlink to it.
rm -rf "$CLI_DIR"
mkdir -p "$CLI_DIR/skills"
install -m 0755 "$GXSERVER_SOURCE_DIR/bin/ghostex" "$CLI_DIR/ghostex"
ln -sfh "ghostex" "$CLI_DIR/gx"
copy_cli_skill() {
	local skill_name="$1"
	mkdir -p "$CLI_DIR/skills/$skill_name"
	cp -R "$REPO_ROOT/skills/$skill_name/." "$CLI_DIR/skills/$skill_name/"
}
for skill_name in "${bundled_cli_skill_assets[@]}"; do
	copy_cli_skill "$skill_name"
done

# CDXC:GPUISourceRuntime 2026-06-24-23:17:
# Stage the full native-reviewed Web/code-server runtime beside the GPUI app resources so Source opens from the packaged app exactly like macOS. Portless still reuses Web/code-server/lib/node and must not carry a second Node runtime.
# CDXC:GPUIStartCommand 2026-07-08-04:55:
# GPUI local starts seal the freshly built app-owned gxserver package and shared
# Web/bin tools into Contents/Resources/Web, matching the native start command's
# daemon ownership instead of launching against the main Ghostex.app bundle.
rm -rf "$WEB_DIR/bin" "$WEB_DIR/code-server" "$WEB_DIR/gxserver" "$WEB_DIR/portless"
mkdir -p "$WEB_DIR/portless"
rsync -a --delete "$WEB_BIN_SOURCE_DIR/" "$WEB_DIR/bin/"
rsync -a --delete "$GXSERVER_SOURCE_DIR/" "$WEB_DIR/gxserver/"
if [[ "$GHOSTEX_ON_DEMAND_ASSETS" != "1" ]]; then
	rsync -a --delete "$WEB_SOURCE_DIR/code-server/" "$WEB_DIR/code-server/"
	chmod 755 "$WEB_DIR/code-server/lib/node"
fi
rsync -a --delete "$WEB_SOURCE_DIR/portless/" "$WEB_DIR/portless/"
chmod 755 "$WEB_DIR/portless/dist/cli.js"
if [[ "$GHOSTEX_ON_DEMAND_ASSETS" == "1" ]]; then
	stage_on_demand_remote_gxserver_manifest
else
	rm -f "$WEB_DIR/on-demand-resources.json"
	stage_remote_gxserver_linux_packages_if_available
fi

cat >"$APP_PATH/Contents/Info.plist" <<EOF_PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleExecutable</key>
	<string>$APP_NAME</string>
	<key>CFBundleIdentifier</key>
	<string>$GPUI_BUNDLE_ID</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<!-- CDXC:GPUIOSIntegration 2026-06-24-13:15:
	GPUI packaging must mirror the Swift app Launch Services declarations so Settings can report the packaged app as an available editor, script handler, and ghostex:// target.
	Use LSHandlerRank Alternate here because app installation should only register GPUI as a candidate handler; explicit Settings actions own default-setting mutations.
	Helper app plists intentionally omit these declarations because only the main app bundle should register document and URL handling.
	-->
	<key>CFBundleDocumentTypes</key>
	<array>
		<dict>
			<key>CFBundleTypeExtensions</key>
			<array>
				<string>*</string>
			</array>
			<key>CFBundleTypeName</key>
			<string>Editable Files</string>
			<key>CFBundleTypeRole</key>
			<string>Editor</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<key>LSItemContentTypes</key>
			<array>
				<string>public.text</string>
				<string>public.source-code</string>
				<string>public.script</string>
				<string>public.data</string>
			</array>
		</dict>
		<dict>
			<key>CFBundleTypeExtensions</key>
			<array>
				<string>command</string>
				<string>tool</string>
				<string>sh</string>
			</array>
			<key>CFBundleTypeName</key>
			<string>Script Files</string>
			<key>CFBundleTypeRole</key>
			<string>Shell</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<key>LSItemContentTypes</key>
			<array>
				<string>public.shell-script</string>
				<string>public.unix-executable</string>
			</array>
		</dict>
	</array>
	<key>CFBundleURLTypes</key>
	<array>
		<dict>
			<key>CFBundleURLName</key>
			<string>Ghostex URL</string>
			<key>CFBundleURLSchemes</key>
			<array>
				<string>ghostex</string>
			</array>
		</dict>
	</array>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>$APP_NAME</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$GPUI_MARKETING_VERSION</string>
	<key>CFBundleVersion</key>
	<string>$GPUI_BUILD_VERSION</string>
	<!-- The GHOSTEXHomeDirectoryName/.ghostex-gpui keys were removed as
	vestigial and misleading: only the macOS Swift host's GhostexAppStorage
	reads them and none of it ships in this bundle. GPUI Rust resolves
	GHOSTEX_HOME or the same XDG storage directories used on Linux. -->
	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<!-- Chromium's passkey/security-key flow can request Bluetooth transport.
	macOS terminates apps that reach that privacy boundary without a usage
	description, so keep this declaration in parity with the Swift host. -->
	<key>NSBluetoothAlwaysUsageDescription</key>
	<string>Ghostex uses Bluetooth only when a website in a Chromium browser pane requests browser features such as passkeys, security keys, or Bluetooth devices.</string>
	<!-- Browser panes prompt in-app before a page may capture the microphone
	or camera, and macOS then applies its own consent on top. Both usage
	descriptions must exist or macOS terminates the app at that boundary
	instead of showing its prompt. -->
	<key>NSMicrophoneUsageDescription</key>
	<string>Ghostex uses the microphone only when you allow a website in a Chromium browser pane to use it, for features such as voice input or calls.</string>
	<key>NSCameraUsageDescription</key>
	<string>Ghostex uses the camera only when you allow a website in a Chromium browser pane to use it, for features such as video calls.</string>
	<key>SUEnableDownloaderService</key>
	<true/>
	<key>SUEnableInstallerLauncherService</key>
	<true/>
	<key>SUFeedURL</key>
	<string>$GHOSTEX_GPUI_SPARKLE_FEED_URL</string>
	<key>SUPublicEDKey</key>
	<string>$GHOSTEX_GPUI_SPARKLE_PUBLIC_ED_KEY</string>
</dict>
</plist>
EOF_PLIST

helper_names=(
	"$APP_NAME Helper"
	"$APP_NAME Helper (Alerts)"
	"$APP_NAME Helper (GPU)"
	"$APP_NAME Helper (Plugin)"
	"$APP_NAME Helper (Renderer)"
)

for helper_name in "${helper_names[@]}"; do
	helper_app="$APP_PATH/Contents/Frameworks/$helper_name.app"
	helper_macos="$helper_app/Contents/MacOS"
	mkdir -p "$helper_macos"
	cp "$GPUI_DIR/target/release/ghostex-gpui-cef-helper" "$helper_macos/$helper_name"
	chmod 755 "$helper_macos/$helper_name"
	cat >"$helper_app/Contents/Info.plist" <<EOF_HELPER
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>$helper_name</string>
	<key>CFBundleIdentifier</key>
	<string>$GPUI_BUNDLE_ID.$(printf '%s' "$helper_name" | tr ' ()' '---')</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>$helper_name</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$GPUI_MARKETING_VERSION</string>
	<key>CFBundleVersion</key>
	<string>$GPUI_BUILD_VERSION</string>
	<key>LSUIElement</key>
	<string>1</string>
</dict>
</plist>
EOF_HELPER
done

stage_gpui_lid_sleep_helper
stage_gpui_ghostex_editor_app
stage_gpui_sparkle_framework_if_available

# CDXC:GPUICefDistribution 2026-08-03:
# The GPUI shell consumes the CEF distribution pinned by cef-rs. Development
# bundles keep the framework in Contents/Frameworks. Release bundles keep only
# the small helper apps and seal the separately signed framework component into
# the on-demand manifest.

# Signing: unset GHOSTEX_GPUI_SIGN_IDENTITY keeps the historical ad-hoc --deep
# re-sign for dev builds; a Developer ID identity runs the inside-out
# hardened-runtime recipe in codesign-gpui-app.sh (macOS
# codesign-ghostex-host.sh port), and GHOSTEX_GPUI_NOTARIZE=1 notarizes and
# staples the app for distribution outside a DMG release.
sign_gpui_app_bundle
notarize_and_staple_gpui_app_if_requested

# CDXC:GPUIMacBundlePackaging 2026-08-03:
# The GPUI macOS app retains CEF helper apps in every bundle. Development apps
# also embed the framework; release apps resolve the verified shared component.
printf 'Built %s for %s (%s)\n' "$APP_PATH" "$GHOSTEX_MACOS_ARCH" "$RUST_TARGET_ARCH"

if [[ "$RUN_APP" == "1" ]]; then
	open -n "$APP_PATH"
fi
