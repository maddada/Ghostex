#!/usr/bin/env bash
set -euo pipefail

# CDXC:Terminal 2026-07-03:
# Phase 1 of the GPUI cross-platform plan renders terminals as GPUI elements
# driven by libghostty-vt, so cargo builds must produce the static archive
# from the vendored Ghostty tree instead of depending on a manually built
# artifact. apps/desktop/build.rs invokes this script with an install prefix inside
# OUT_DIR and links {prefix}/lib/libghostty-vt.a directly, mirroring how the
# GhosttyKit archive is linked.
#
# The vendored Ghostty build pins a Zig series (build.zig.zon
# minimum_zig_version) while the machine default `zig` may be another version.
# tooling/zig-toolchain.sh resolves it the same way every build path does and
# fails with an install hint instead of building with a mismatched Zig.
#
# CDXC:Terminal 2026-05-22-11:17 (workaround retained from the
# now-archived iOS terminal build):
# Xcode 26's macOS SDK exposes libSystem as arm64e-only in the TBD stub,
# which Zig 0.15.x cannot use for native aarch64 links (the libghostty-vt
# shared library link fails with undefined libc symbols). Redirect only
# macosx SDK discovery to the newest Command Line Tools SDK that still
# exports arm64 while leaving other xcrun queries on the default toolchain.
#
# CDXC:Terminal 2026-07-11:
# The redirect exists only for SDKs whose libSystem TBD lacks plain arm64
# exports. Newer toolchains (Xcode 27 beta) export arm64-macos again, and
# machines without Command Line Tools have no redirect target at all, so
# probe the default xcrun macOS SDK first and use it unmodified when it
# already exports arm64; only fall back to the CLT redirect when the
# default SDK cannot link arm64.

if [[ $# -ne 1 ]]; then
	echo "usage: $(basename "$0") <install-prefix>" >&2
	exit 64
fi
PREFIX="$1"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GHOSTTY_DIR="$ROOT_DIR/.dependencies/ghostty"
# shellcheck source=../../../tooling/zig-toolchain.sh
source "$ROOT_DIR/tooling/zig-toolchain.sh"
ZIG="$(ghostex_find_zig "$ROOT_DIR")"
echo "Zig for libghostty-vt: $ZIG ($("$ZIG" version))"
GHOSTTY_APP_VERSION="$(
	sed -n -E 's/^[[:space:]]*\.version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$GHOSTTY_DIR/build.zig.zon" |
		head -n 1
)"
if [[ -z "$GHOSTTY_APP_VERSION" ]]; then
	echo "Could not resolve Ghostty app version from $GHOSTTY_DIR/build.zig.zon." >&2
	exit 1
fi

cd "$GHOSTTY_DIR"

# The SDK is usable for Zig arm64 links only when the MAIN libSystem TBD
# document (the first `targets:` line) declares plain arm64-macos. Newer
# SDKs keep arm64-macos in later sub-documents while the main document is
# arm64e-only, so a whole-file grep reports false positives.
sdk_exports_arm64() {
	local tbd="$1/usr/lib/libSystem.tbd"
	[[ -f "$tbd" ]] || return 1
	sed -n '/^targets:/{p;q;}' "$tbd" | grep -Eq '(^|[ \[,])arm64-macos[] ,]'
}

# CDXC:Terminal 2026-07-11:
# When no SDK on the machine can link plain arm64 (Xcode 26+/27 SDKs are
# arm64e-only in the main libSystem document and Command Line Tools may be
# absent), synthesize an overlay SDK next to the install prefix: symlink the
# default SDK and rewrite only libSystem.tbd so every targets list that
# declares arm64e-macos also declares arm64-macos. The dylib symbol tables
# are identical for arm64/arm64e, so the patched stub links correctly.
synthesize_arm64_sdk_overlay() {
	local source_sdk="$1"
	local overlay_sdk="$2"
	rm -rf "$overlay_sdk"
	mkdir -p "$overlay_sdk/usr/lib"
	local entry name
	for entry in "$source_sdk"/*; do
		name="$(basename "$entry")"
		[[ "$name" == "usr" ]] && continue
		ln -s "$entry" "$overlay_sdk/$name"
	done
	for entry in "$source_sdk"/usr/*; do
		name="$(basename "$entry")"
		[[ "$name" == "lib" ]] && continue
		ln -s "$entry" "$overlay_sdk/usr/$name"
	done
	for entry in "$source_sdk"/usr/lib/*; do
		name="$(basename "$entry")"
		[[ "$name" == "libSystem.tbd" ]] && continue
		ln -s "$entry" "$overlay_sdk/usr/lib/$name"
	done
	sed -E '/(^|[ \[,])arm64-macos[] ,]/! s/arm64e-macos/arm64-macos, arm64e-macos/g' \
		"$source_sdk/usr/lib/libSystem.tbd" >"$overlay_sdk/usr/lib/libSystem.tbd"
}

if [[ "$(uname)" == "Darwin" ]] && ! sdk_exports_arm64 "$(/usr/bin/xcrun --sdk macosx --show-sdk-path 2>/dev/null)"; then
	WRAPPER_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ghostex-xcrun.XXXXXX")"
	trap 'rm -rf "$WRAPPER_DIR"' EXIT
	MACOS_SDK=""
	if [[ -d /Library/Developer/CommandLineTools/SDKs ]]; then
		MACOS_SDK="$(
			find /Library/Developer/CommandLineTools/SDKs -maxdepth 1 -type d -name 'MacOSX*.sdk' 2>/dev/null |
				while IFS= read -r sdk; do
					if sdk_exports_arm64 "$sdk"; then
						printf '%s\n' "$sdk"
					fi
				done |
				sort -Vr |
				head -n 1
		)"
	fi
	if [[ -z "$MACOS_SDK" ]]; then
		DEFAULT_SDK="$(/usr/bin/xcrun --sdk macosx --show-sdk-path 2>/dev/null)"
		if [[ -z "$DEFAULT_SDK" || ! -f "$DEFAULT_SDK/usr/lib/libSystem.tbd" ]]; then
			echo "No macOS SDK with arm64 libSystem exports was found, and no default macOS SDK is available to synthesize one from." >&2
			exit 1
		fi
		OVERLAY_SDK="${PREFIX}-sdk-overlay/$(basename "$DEFAULT_SDK")"
		if [[ ! -f "$OVERLAY_SDK/usr/lib/libSystem.tbd" ]] ||
			[[ "$DEFAULT_SDK/usr/lib/libSystem.tbd" -nt "$OVERLAY_SDK/usr/lib/libSystem.tbd" ]]; then
			synthesize_arm64_sdk_overlay "$DEFAULT_SDK" "$OVERLAY_SDK"
		fi
		if ! sdk_exports_arm64 "$OVERLAY_SDK"; then
			echo "Failed to synthesize an arm64-capable SDK overlay from $DEFAULT_SDK." >&2
			exit 1
		fi
		MACOS_SDK="$OVERLAY_SDK"
	fi
	cat >"$WRAPPER_DIR/xcrun" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if [[ "\${1:-}" == "--sdk" && "\${2:-}" == "macosx" && "\${3:-}" == "--show-sdk-path" ]]; then
  echo "$MACOS_SDK"
  exit 0
fi
/usr/bin/xcrun "\$@"
EOF
	chmod +x "$WRAPPER_DIR/xcrun"
	export PATH="$WRAPPER_DIR:$PATH"
fi

# Ghostex patch (2026-07-11): cargo links only the STATIC libghostty-vt
# archive; skip the shared dylib emit. Linking a Zig-built dylib/exe that
# pulls in C++ against the Xcode 26+/27 SDKs still fails (the bundled libc++
# sub-compilation trips the SDK's math.h INFINITY / __need_infinity_nan
# handling, plus arm64e stub issues). Re-verified on Zig 0.16 (2026-08-23):
# this is not a 0.15-only limitation, so the emit stays off. Without it the
# unconditional dylib emit failed the whole cargo build on machines without
# an older Command Line Tools SDK even though the static archive builds fine.
ZIG_BUILD_ARGS=(
	-Dversion-string="$GHOSTTY_APP_VERSION"
	-Demit-lib-vt=true
	-Demit-lib-vt-shared=false
	-Demit-xcframework=false
	-Doptimize=ReleaseSafe
	--prefix "$PREFIX"
)

# CDXC:Build 2026-11-04 WHY: after `cargo clean` of apps/desktop the next build
# failed in Zig with "unable to parse libc paths file ... FileNotFound" and
# "'strings.h' file not found". ghostty's build.zig asks `xcrun --show-sdk-path`
# for the libc paths while configuring, and the xcrun wrapper above answers
# with the SDK overlay inside this run's OUT_DIR. Zig 0.17 caches that
# configuration in .dependencies/ghostty/.zig-cache under a key that covers
# neither --prefix nor xcrun's answer, so every OUT_DIR reused the
# configuration of whichever OUT_DIR configured first, and once that OUT_DIR
# was deleted every build pointed at a missing overlay. The overlay path is
# configure-time state Zig cannot track, which is what --cache-poison=poisoned
# is for: Zig configures afresh (about a second) instead of trusting the cached
# configuration. Compiled objects stay cached. This script only runs when cargo
# reruns the ghostex-gpui build script.
if [[ -n "${WRAPPER_DIR:-}" ]]; then
	ZIG_BUILD_ARGS+=(--cache-poison=poisoned)
fi

exec "$ZIG" build "${ZIG_BUILD_ARGS[@]}"
