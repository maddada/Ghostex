#!/usr/bin/env bash
set -euo pipefail

# Usage: macos.sh [--phase <name>] <version> [output-dir]
#
# CDXC:Release 2026-09-02: the GitHub Actions job used to run this
# script as one 24-minute step, so per-phase timing was invisible from the jobs
# API. The script is now a sequence of phase functions dispatched by `--phase`.
# Without `--phase` (or with `--phase all`) it runs every phase in the original
# order, in one process, exactly as before, so release-build-macos.yml,
# release-amend-existing.yml, and local runs are unaffected.
#
# Every value a later phase needs is derived deterministically in the prologue
# below (paths, version, build number, identities), so no shell state has to be
# persisted between phase steps. The only prologue side effect that must not
# repeat is release_gpui_prepare_output (it wipes the output dir); it runs in
# `all` and `prepare` only, and later phases require the dir to already exist.
#
# Phases, in order:
#   prepare        pinned references, code-server archives, remote gxserver
#                  packages, GhosttyKit
#   build-server   opt-in cargo pre-warm of server/ (never part of `all`)
#   stage-runtime  apps/desktop/scripts/prepare-macos-runtime.sh (or the
#                  prepared-runtime validation)
#   build-desktop  opt-in cargo pre-warm of apps/desktop/ (never part of `all`)
#   assemble       build-macos-app.sh (vite, cargo, Swift helper, bundle, sign),
#                  component publishing, bundle validation
#   dmg            DMG creation, size budget, on-demand asset copy
#   notarize       notarytool submit + poll + staple
#   finalize       appcast, sealed checksum verification, manifest
#
# The two build-* phases run the exact cargo command, cwd, and env that
# prepare-macos-runtime.sh and build-macos-app.sh run later, so those scripts
# still execute their own cargo build unchanged and find every artifact fresh.
# The shipped binaries are always produced by the original command in the
# original position; the pre-warm only moves the compile time into its own
# step. They are opt-in so `all` stays byte-identical to the historical flow.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
PHASE="all"
if [[ "${1:-}" == "--phase" ]]; then
	PHASE="${2:-}"
	[[ -n "$PHASE" ]] || {
		echo "Usage: macos.sh [--phase <name>] <version> [output-dir]" >&2
		exit 2
	}
	shift 2
fi
case "$PHASE" in
all | prepare | build-server | stage-runtime | build-desktop | assemble | dmg | notarize | finalize) ;;
*)
	echo "Unknown macOS release phase: $PHASE" >&2
	echo "Expected one of: all prepare build-server stage-runtime build-desktop assemble dmg notarize finalize" >&2
	exit 2
	;;
esac
VERSION="${1:-}"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" macos-arm64)}"
release_gpui_require_version "$VERSION"
release_gpui_require_command bun
release_gpui_require_command cargo
release_gpui_require_command codesign
release_gpui_require_command hdiutil
if [[ "$PHASE" == "all" || "$PHASE" == "prepare" ]]; then
	release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"
else
	[[ -d "$OUTPUT" ]] || {
		echo "Release output $OUTPUT does not exist; run --phase prepare first" >&2
		exit 1
	}
fi

BUILD_NUMBER="$(release_gpui_build_number "$VERSION")"
SIGNING_IDENTITY="${GHOSTEX_CODE_SIGN_IDENTITY:-Developer ID Application: Mohamad Youssef (KTKP595G3B)}"
NOTARY_PROFILE="${GHOSTEX_NOTARY_PROFILE:-notarytool-profile}"
UPDATE_SPARKLE="${GHOSTEX_RELEASE_UPDATE_SPARKLE:-1}"
MACOS_STAGE="${GHOSTEX_MACOS_RELEASE_STAGE:-all}"
SPARKLE_ROOT="$($SCRIPT_DIR/prepare-sparkle.sh)"
SPARKLE_FRAMEWORK="$SPARKLE_ROOT/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework"
REMOTE_ROOT="$REPO_ROOT/build/remote-gxserver-linux"
CODE_SERVER_COMPONENT_VERSION=""
CODE_SERVER_LINUX_X64_ARCHIVE=""
CODE_SERVER_LINUX_ARM64_ARCHIVE=""
CODE_SERVER_ARCHIVES_RESOLVED=0
GHOSTTY_ROOT="$REPO_ROOT/.dependencies/ghostty"
GHOSTTY_KIT="$GHOSTTY_ROOT/macos/GhosttyKit.xcframework"
APP_PATH="$REPO_ROOT/apps/desktop/build/macos.noindex/Ghostex.app"
INFO_PLIST="$APP_PATH/Contents/Info.plist"
DMG="$OUTPUT/ghostex-$VERSION-arm64.dmg"
ON_DEMAND_ROOT="$REPO_ROOT/build/on-demand-assets/$VERSION"

release_gpui_truthy() {
	case "$(printf '%s' "${1:-0}" | tr '[:upper:]' '[:lower:]')" in
	1 | true | yes | on) return 0 ;;
	*) return 1 ;;
	esac
}

USE_PREPARED_RUNTIME=0
USE_PREBUILT_RUST=0
SKIP_PREPARE_REFERENCES=0
release_gpui_truthy "${GHOSTEX_MACOS_USE_PREPARED_RUNTIME:-0}" && USE_PREPARED_RUNTIME=1
release_gpui_truthy "${GHOSTEX_GPUI_USE_PREBUILT_RUST:-0}" && USE_PREBUILT_RUST=1
release_gpui_truthy "${GHOSTEX_MACOS_SKIP_PREPARE_REFERENCES:-0}" && SKIP_PREPARE_REFERENCES=1

# Resolves the Linux code-server component archives the runtime stage seals.
# In `all` this runs once, right after prepare-references (which initializes
# the code-server submodule the identity script reads); the stage-runtime
# phase re-derives the same values when it runs in its own process.
resolve_code_server_archives() {
	if [[ "$USE_PREPARED_RUNTIME" != "1" ]]; then
		CODE_SERVER_COMPONENT_VERSION="$(node "$SCRIPT_DIR/code-server-component-identity.mjs" --root "$REPO_ROOT/.dependencies/code-server")"
		CODE_SERVER_LINUX_X64_ARCHIVE="${GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_X64_ARCHIVE:-$REPO_ROOT/build/runtime-artifacts/code-server-x64/code-server-$CODE_SERVER_COMPONENT_VERSION-linux-x64.tar.gz}"
		CODE_SERVER_LINUX_ARM64_ARCHIVE="${GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_ARM64_ARCHIVE:-$REPO_ROOT/build/runtime-artifacts/code-server-arm64/code-server-$CODE_SERVER_COMPONENT_VERSION-linux-arm64.tar.gz}"
		[[ -f "$CODE_SERVER_LINUX_X64_ARCHIVE" ]] || {
			echo "macOS release requires Linux x64 code-server archive: $CODE_SERVER_LINUX_X64_ARCHIVE" >&2
			exit 1
		}
		[[ -f "$CODE_SERVER_LINUX_ARM64_ARCHIVE" ]] || {
			echo "macOS release requires Linux arm64 code-server archive: $CODE_SERVER_LINUX_ARM64_ARCHIVE" >&2
			exit 1
		}
	fi
	CODE_SERVER_ARCHIVES_RESOLVED=1
}

phase_prepare() {
	if [[ "$SKIP_PREPARE_REFERENCES" != "1" ]]; then
		"$SCRIPT_DIR/prepare-references.sh"
	fi
	resolve_code_server_archives
	if [[ ! -x "$REMOTE_ROOT/x64/package/bin/gxserver" || ! -x "$REMOTE_ROOT/arm64/package/bin/gxserver" ]]; then
		"$REPO_ROOT/tooling/build-remote-gxserver-linux-release.sh" --arch all
	fi

	if [[ "$USE_PREBUILT_RUST" != "1" && ! -d "$GHOSTTY_KIT" ]]; then
		GHOSTTY_ZIG="${GHOSTEX_ZIG:-${ZIG:-}}"
		[[ -x "$GHOSTTY_ZIG" ]] || {
			echo "Zig 0.16.0 is required to build GhosttyKit" >&2
			exit 1
		}
		[[ "$("$GHOSTTY_ZIG" version)" == "0.16.0" ]] || {
			echo "GhosttyKit requires Zig 0.16.0" >&2
			exit 1
		}
		GHOSTTY_DEVELOPER_DIR="$(xcode-select -p)"
		GHOSTTY_SDKROOT="$(DEVELOPER_DIR="$GHOSTTY_DEVELOPER_DIR" xcrun --sdk macosx --show-sdk-path)"
		(
			cd "$GHOSTTY_ROOT"
			env \
				DEVELOPER_DIR="$GHOSTTY_DEVELOPER_DIR" \
				SDKROOT="$GHOSTTY_SDKROOT" \
				GHOSTTY_METAL_DEVELOPER_DIR="$GHOSTTY_DEVELOPER_DIR" \
				"$GHOSTTY_ZIG" build \
				-Demit-xcframework \
				-Dxcframework-target=universal \
				-Demit-macos-app=false
		)
	fi
	if [[ "$USE_PREBUILT_RUST" != "1" ]]; then
		[[ -d "$GHOSTTY_KIT" ]] || {
			echo "GhosttyKit build did not produce $GHOSTTY_KIT" >&2
			exit 1
		}
	fi
}

# Opt-in pre-warm of the server crates. Mirrors build_gxserver_rust_if_needed
# in apps/desktop/scripts/prepare-macos-runtime.sh: same cargo resolution
# (GXSERVER_RUST_CARGO, then CARGO, then PATH), same manifest, same target for
# GHOSTEX_MACOS_ARCH=arm64, same GHOSTEX_GPUI_MARKETING_VERSION (the only env
# server/build.rs reads), and the caller's cwd, which is what that script uses.
phase_build_server() {
	if [[ "$USE_PREPARED_RUNTIME" == "1" ]]; then
		echo "Prepared macOS runtime is in use; nothing to pre-build for gxserver."
		return 0
	fi
	local gxserver_root cargo_bin
	gxserver_root="${GXSERVER_RS_ROOT:-$REPO_ROOT/server}"
	cargo_bin="${GXSERVER_RUST_CARGO:-${CARGO:-}}"
	if [[ -z "$cargo_bin" ]]; then
		cargo_bin="$(command -v cargo)"
	fi
	GHOSTEX_GPUI_MARKETING_VERSION="$VERSION" \
		"$cargo_bin" build --release --bins --manifest-path "$gxserver_root/Cargo.toml" --target aarch64-apple-darwin
}

phase_stage_runtime() {
	if [[ "$CODE_SERVER_ARCHIVES_RESOLVED" != "1" ]]; then
		resolve_code_server_archives
	fi
	# Prepare the GPUI-owned runtime tree and seal the on-demand checksums without
	# invoking the retired Swift host build.
	if [[ "$USE_PREPARED_RUNTIME" == "1" ]]; then
		PREPARED_WEB="$REPO_ROOT/apps/desktop/runtime/macos/Web"
		for required_path in \
			"$PREPARED_WEB/bin/zmx" \
			"$PREPARED_WEB/on-demand-resources.json" \
			"$PREPARED_WEB/gxserver/bin/gxserver" \
			"$PREPARED_WEB/portless/dist/cli.js" \
			"$REPO_ROOT/apps/desktop/runtime/macos/CLI/ghostex" \
			"$REPO_ROOT/build/on-demand-components/components.json" \
			"$REPO_ROOT/build/on-demand-assets/$VERSION/gxserver-linux-x64.tar.gz" \
			"$REPO_ROOT/build/on-demand-assets/$VERSION/gxserver-linux-arm64.tar.gz"; do
			[[ -e "$required_path" ]] || {
				echo "Prepared macOS runtime is missing $required_path" >&2
				exit 1
			}
		done
		node "$REPO_ROOT/tooling/release-gpui/on-demand-manifest.mjs" validate-macos \
			--manifest "$PREPARED_WEB/on-demand-resources.json"
		echo "Using checksum-bound prepared macOS runtime for $VERSION."
	else
		GHOSTEX_MACOS_ARCH=arm64 \
			GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES=0 \
			GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1 \
			GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE="$REMOTE_ROOT/x64/package" \
			GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE="$REMOTE_ROOT/arm64/package" \
			GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_X64_ARCHIVE="$CODE_SERVER_LINUX_X64_ARCHIVE" \
			GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_ARM64_ARCHIVE="$CODE_SERVER_LINUX_ARM64_ARCHIVE" \
			GHOSTEX_CODE_SERVER_COMPONENT_VERSION="$CODE_SERVER_COMPONENT_VERSION" \
			GHOSTEX_ON_DEMAND_ASSETS=1 \
			GHOSTEX_GPUI_MARKETING_VERSION="$VERSION" \
			"$REPO_ROOT/apps/desktop/scripts/prepare-macos-runtime.sh"
	fi
}

# Opt-in pre-warm of the desktop crate. Mirrors the cargo step inside
# apps/desktop/scripts/build-macos-app.sh: cwd apps/desktop, CEF_PATH exported
# to the crate-local cef-cache, and the same environment block phase_assemble
# hands that script (keep the two lists identical) so every rerun-if-env-changed
# input matches and the later build is a no-op.
phase_build_desktop() {
	if [[ "$USE_PREBUILT_RUST" == "1" ]]; then
		echo "Prebuilt GPUI Rust binaries are in use; nothing to pre-build for ghostex-gpui."
		return 0
	fi
	(
		cd "$REPO_ROOT/apps/desktop"
		export CEF_PATH="$REPO_ROOT/apps/desktop/build/cef-cache"
		GHOSTEX_MACOS_ARCH=arm64 \
			GHOSTEX_GPUI_APP_NAME=Ghostex \
			GHOSTEX_GPUI_BUNDLE_ID=com.madda.ghostex.host \
			GHOSTEX_GPUI_MARKETING_VERSION="$VERSION" \
			GHOSTEX_GPUI_BUILD_VERSION="$BUILD_NUMBER" \
			GHOSTEX_GPUI_SPARKLE_FEED_URL=https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml \
			GHOSTEX_GPUI_SPARKLE_FRAMEWORK="$SPARKLE_FRAMEWORK" \
			GHOSTEX_REQUIRE_SPARKLE=1 \
			GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1 \
			GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE="$REMOTE_ROOT/x64/package" \
			GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE="$REMOTE_ROOT/arm64/package" \
			GHOSTEX_ON_DEMAND_ASSETS=1 \
			GHOSTEX_GPUI_SIGN_IDENTITY="$SIGNING_IDENTITY" \
			GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG=--timestamp \
			cargo build --release --bins
	)
}

phase_assemble() {
	GHOSTEX_MACOS_ARCH=arm64 \
		GHOSTEX_GPUI_APP_NAME=Ghostex \
		GHOSTEX_GPUI_BUNDLE_ID=com.madda.ghostex.host \
		GHOSTEX_GPUI_MARKETING_VERSION="$VERSION" \
		GHOSTEX_GPUI_BUILD_VERSION="$BUILD_NUMBER" \
		GHOSTEX_GPUI_SPARKLE_FEED_URL=https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml \
		GHOSTEX_GPUI_SPARKLE_FRAMEWORK="$SPARKLE_FRAMEWORK" \
		GHOSTEX_REQUIRE_SPARKLE=1 \
		GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1 \
		GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE="$REMOTE_ROOT/x64/package" \
		GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE="$REMOTE_ROOT/arm64/package" \
		GHOSTEX_ON_DEMAND_ASSETS=1 \
		GHOSTEX_GPUI_SIGN_IDENTITY="$SIGNING_IDENTITY" \
		GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG=--timestamp \
		"$REPO_ROOT/apps/desktop/scripts/build-macos-app.sh"

	COMPONENT_MANIFEST="$REPO_ROOT/build/on-demand-components/components.json"
	for component in code-server cef; do
		component_version="$(COMPONENT="$component" node -e '
const fs = require("fs");
const manifest = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const version = (manifest.components ?? manifest)[process.env.COMPONENT]?.componentVersion;
if (!version) process.exit(1);
process.stdout.write(version);
' "$COMPONENT_MANIFEST")"
		PUBLISH_ARGS=(
			node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs"
			--component "$component"
			--version "$component_version"
			--asset-dir "$REPO_ROOT/build/on-demand-components/assets"
			--output "$COMPONENT_MANIFEST"
		)
		if [[ "$component" == "code-server" ]]; then
			PUBLISH_ARGS+=(--require-platforms darwin-arm64,linux-x64,linux-arm64 --require-sha256-sidecars)
		fi
		"${PUBLISH_ARGS[@]}"
	done

	[[ -d "$APP_PATH" ]] || {
		echo "GPUI build did not produce $APP_PATH" >&2
		exit 1
	}
	# The shared app builder marks its staging wrapper as a Finder package for
	# local rsync installs. Distribution bundles must not carry FinderInfo or any
	# other extended attributes into the strict signature/notarization gate.
	/usr/bin/xattr -cr "$APP_PATH"
	[[ "$(plutil -extract CFBundleIdentifier raw "$INFO_PLIST")" == "com.madda.ghostex.host" ]]
	[[ "$(plutil -extract CFBundleName raw "$INFO_PLIST")" == "Ghostex" ]]
	[[ "$(plutil -extract CFBundleExecutable raw "$INFO_PLIST")" == "Ghostex" ]]
	[[ "$(plutil -extract CFBundleShortVersionString raw "$INFO_PLIST")" == "$VERSION" ]]
	[[ "$(plutil -extract CFBundleVersion raw "$INFO_PLIST")" == "$BUILD_NUMBER" ]]
	[[ "$(plutil -extract SUFeedURL raw "$INFO_PLIST")" == "https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml" ]]
	codesign --verify --deep --strict --verbose=2 "$APP_PATH"
	node --input-type=module -e \
		'import { validateMacosAppBundle } from "./tooling/validate-macos-app-bundle.mjs"; await validateMacosAppBundle({ appName: "Ghostex", appPath: process.argv[1], arch: "arm64" });' \
		"$APP_PATH"
}

phase_dmg() {
	[[ -d "$APP_PATH" ]] || {
		echo "Signed app bundle is missing: $APP_PATH (run --phase assemble first)" >&2
		exit 1
	}
	STAGE_ROOT="$REPO_ROOT/build/release-gpui/macos-staging.noindex"
	mkdir -p "$STAGE_ROOT"
	STAGE="$(mktemp -d "$STAGE_ROOT/stage-XXXXXX")"
	trap 'rm -rf "$STAGE"' EXIT
	ditto "$APP_PATH" "$STAGE/Ghostex.app"
	ln -s /Applications "$STAGE/Applications"
	# CDXC:Release 2026-08-22:
	# `hdiutil create` attaches a device to read the staging folder, and another
	# process on the runner — Spotlight indexing the stage it has just seen appear,
	# or a leftover attachment — can hold it and fail the whole macOS build with
	# "Resource busy" minutes after the app was signed and validated. 7.13.0 lost a
	# 30-minute build to exactly that, and the retry produced the DMG. Bounded, and
	# the partial image is removed first so a retry never reads a stale file.
	for attempt in 1 2 3; do
		if hdiutil create -volname Ghostex -srcfolder "$STAGE" -format UDZO "$DMG"; then
			break
		fi
		if [[ "$attempt" == 3 ]]; then
			echo "hdiutil create failed after $attempt attempts" >&2
			exit 1
		fi
		echo "hdiutil create failed (attempt $attempt); retrying" >&2
		rm -f "$DMG"
		sleep "$((attempt * 10))"
	done
	release_gpui_assert_dmg_budget "$DMG"

	for name in gxserver-linux-x64.tar.gz gxserver-linux-arm64.tar.gz; do
		[[ -f "$ON_DEMAND_ROOT/$name" ]] || {
			echo "Missing on-demand asset: $name" >&2
			exit 1
		}
	done

	if [[ "$MACOS_STAGE" == "build-sign" ]]; then
		release_gpui_write_manifest \
			"$OUTPUT" \
			macos-arm64-signed \
			"$VERSION" \
			"$DMG"
		printf 'Built signed, unstapled GPUI macOS payload in %s\n' "$OUTPUT"
		exit 0
	fi
}

phase_notarize() {
	if [[ "$MACOS_STAGE" == "build-sign" ]]; then
		printf 'GHOSTEX_MACOS_RELEASE_STAGE=build-sign ends at the DMG phase; skipping notarization.\n'
		exit 0
	fi
	NOTARY_SUBMISSION="$(mktemp "$REPO_ROOT/build/release-gpui/notary-submission-XXXXXX.json")"
	SUBMISSION_ID="$(
		GHOSTEX_NOTARY_PROFILE="$NOTARY_PROFILE" \
			"$SCRIPT_DIR/macos-notary.sh" submit "$VERSION" "$DMG" "$NOTARY_SUBMISSION" | tail -n 1
	)"
	GHOSTEX_NOTARY_PROFILE="$NOTARY_PROFILE" \
		"$SCRIPT_DIR/macos-notary.sh" poll "$VERSION" "$DMG" "$SUBMISSION_ID"
	rm -f "$NOTARY_SUBMISSION"
}

phase_finalize() {
	if [[ "$MACOS_STAGE" == "build-sign" ]]; then
		printf 'GHOSTEX_MACOS_RELEASE_STAGE=build-sign ends at the DMG phase; skipping finalization.\n'
		exit 0
	fi
	[[ -f "$DMG" ]] || {
		echo "Notarized DMG is missing: $DMG (run --phase dmg and --phase notarize first)" >&2
		exit 1
	}
	if [[ "$UPDATE_SPARKLE" == "1" ]]; then
		APPCAST_WORK="$(mktemp -d "$REPO_ROOT/build/release-gpui/appcast-stage-XXXXXX")"
		cp "$REPO_ROOT/appcast.xml" "$APPCAST_WORK/appcast.xml"
		cp "$DMG" "$APPCAST_WORK/$(basename "$DMG")"
		VERSION="$VERSION" CHANGELOG_PATH="$REPO_ROOT/CHANGELOG.md" NOTES_PATH="$APPCAST_WORK/ghostex-$VERSION-arm64.md" node <<'JS'
const { readFileSync, writeFileSync } = require("node:fs");
const version = process.env.VERSION;
const changelog = readFileSync(process.env.CHANGELOG_PATH, "utf8");
const start = changelog.indexOf(`## ${version} -`);
if (start < 0) throw new Error(`CHANGELOG.md has no ${version} section`);
const next = changelog.indexOf("\n## ", start + 4);
const section = changelog.slice(start, next < 0 ? undefined : next).trim();
writeFileSync(process.env.NOTES_PATH, `# Ghostex ${version}\n\n${section}\n`);
JS

		GENERATE_ARGS=(
			--download-url-prefix "https://github.com/maddada/Ghostex/releases/download/v$VERSION/"
			--full-release-notes-url "https://github.com/maddada/Ghostex/releases/tag/v$VERSION"
			--embed-release-notes
			--maximum-versions 6
			-o "$APPCAST_WORK/appcast.xml"
		)
		if [[ -n "${SPARKLE_PRIVATE_KEY:-}" ]]; then
			printf '%s' "$SPARKLE_PRIVATE_KEY" | "$SPARKLE_ROOT/bin/generate_appcast" "${GENERATE_ARGS[@]}" --ed-key-file - "$APPCAST_WORK"
		else
			"$SPARKLE_ROOT/bin/generate_appcast" "${GENERATE_ARGS[@]}" "$APPCAST_WORK"
		fi
		cp "$APPCAST_WORK/appcast.xml" "$OUTPUT/appcast.xml"
		APPCAST_SIGNATURE="$(xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@*[local-name()='edSignature'])[1])" "$OUTPUT/appcast.xml")"
		if [[ -n "${SPARKLE_PRIVATE_KEY:-}" ]]; then
			printf '%s' "$SPARKLE_PRIVATE_KEY" | "$SPARKLE_ROOT/bin/sign_update" --ed-key-file - --verify "$DMG" "$APPCAST_SIGNATURE"
		else
			"$SPARKLE_ROOT/bin/sign_update" --verify "$DMG" "$APPCAST_SIGNATURE"
		fi
		[[ "$(xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='version'])[1])" "$OUTPUT/appcast.xml")" == "$BUILD_NUMBER" ]]
		[[ "$(xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='shortVersionString'])[1])" "$OUTPUT/appcast.xml")" == "$VERSION" ]]
		rm -rf "$APPCAST_WORK"
	fi

	ASSETS=("$DMG")
	MANIFEST_PATH="$APP_PATH/Contents/Resources/Web/on-demand-resources.json" \
		ASSET_PATH="$ON_DEMAND_ROOT" node <<'JS'
const { createHash } = require("node:crypto");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const manifest = JSON.parse(readFileSync(process.env.MANIFEST_PATH, "utf8"));
for (const asset of Object.values(manifest.assets ?? {})) {
  const file = join(process.env.ASSET_PATH, asset.name);
  const actual = createHash("sha256").update(readFileSync(file)).digest("hex");
  if (actual !== asset.sha256) throw new Error(`Sealed on-demand checksum mismatch for ${asset.name}`);
}
JS
	release_gpui_write_manifest "$OUTPUT" macos-arm64 "$VERSION" "${ASSETS[@]}"
	printf 'Built GPUI macOS release payload in %s\n' "$OUTPUT"
}

case "$PHASE" in
all)
	phase_prepare
	phase_stage_runtime
	phase_assemble
	phase_dmg
	phase_notarize
	phase_finalize
	;;
prepare) phase_prepare ;;
build-server) phase_build_server ;;
stage-runtime) phase_stage_runtime ;;
build-desktop) phase_build_desktop ;;
assemble) phase_assemble ;;
dmg) phase_dmg ;;
notarize) phase_notarize ;;
finalize) phase_finalize ;;
esac
