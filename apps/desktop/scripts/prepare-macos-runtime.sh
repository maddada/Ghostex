#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIGURATION="${CONFIGURATION:-Debug}"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
WEB_DIR="$REPO_ROOT/apps/desktop/runtime/macos/Web"
CLI_DIR="$REPO_ROOT/apps/desktop/runtime/macos/CLI"
GHOSTTY_ROOT="${GHOSTTY_ROOT:-}"

if ! xcrun xcodebuild -version >/dev/null 2>&1; then
	for developer_dir in \
		"/Applications/Xcode.app/Contents/Developer" \
		"/Applications/Xcode-beta.app/Contents/Developer"; do
		if [[ -x "$developer_dir/usr/bin/xcodebuild" ]]; then
			export DEVELOPER_DIR="$developer_dir"
			break
		fi
	done
fi

# CDXC:PromptSearch 2026-08-20: zehn is no longer a Zig submodule that this
# script builds and stages as Web/bin/zehn. Prompt-history search is a Rust crate
# compiled into gxserver, so there is nothing to build, cache, or copy here.
ZMX_ROOT_EXPLICITLY_CONFIGURED=0
[[ -n "${ZMX_ROOT:-}" ]] && ZMX_ROOT_EXPLICITLY_CONFIGURED=1
ZMX_ROOT="${ZMX_ROOT:-$REPO_ROOT/.dependencies/zmx}"
GXSERVER_RS_ROOT="${GXSERVER_RS_ROOT:-$REPO_ROOT/server}"
# CDXC:Release 2026-08-23: the vendored ghostex-tui terminal app was deleted
# from the repository, so nothing is built, fingerprinted, or staged as
# Web/bin/ghostex-tui any more. A herdr plugin replaces it (spec in
# docs/2026-08-23/tui2-herdr-plugin/).
CODE_SERVER_ROOT_EXPLICITLY_CONFIGURED=0
[[ -n "${CODE_SERVER_ROOT:-${GHOSTEX_CODE_SERVER_ROOT:-}}" ]] && CODE_SERVER_ROOT_EXPLICITLY_CONFIGURED=1
CODE_SERVER_ROOT="${CODE_SERVER_ROOT:-${GHOSTEX_CODE_SERVER_ROOT:-$REPO_ROOT/.dependencies/code-server}}"
CODE_SERVER_APP_NODE_VERSION="${CODE_SERVER_APP_NODE_VERSION:-}"
if [[ -z "$CODE_SERVER_APP_NODE_VERSION" && -f "$CODE_SERVER_ROOT/.node-version" ]]; then
	CODE_SERVER_APP_NODE_VERSION="$(tr -d '[:space:]' <"$CODE_SERVER_ROOT/.node-version")"
fi
CODE_SERVER_APP_NODE_VERSION="${CODE_SERVER_APP_NODE_VERSION:-24.18.1}"
CODE_SERVER_APP_NODE_MAJOR="${CODE_SERVER_APP_NODE_VERSION%%.*}"
CODE_SERVER_NODE_DOWNLOAD_BASE_URL="https://nodejs.org/dist/v$CODE_SERVER_APP_NODE_VERSION"
GHOSTEX_APP_VARIANT="${GHOSTEX_APP_VARIANT:-prod}"
case "$GHOSTEX_APP_VARIANT" in
prod) ;;
dev)
	# CDXC:Build 2026-06-09-09:27: Ghostex-dev builds were removed because agents were invoking the dev app path by mistake. Fail before toolchain checks or Xcode generation so direct build commands cannot create Ghostex-dev outside `bun run start`.
	echo "Ghostex-dev builds were removed. Use GHOSTEX_APP_VARIANT=prod or unset it." >&2
	exit 1
	;;
*)
	echo "Unsupported GHOSTEX_APP_VARIANT: $GHOSTEX_APP_VARIANT" >&2
	exit 1
	;;
esac

# CDXC:Build 2026-06-08-08:42: Apple Silicon local builds must produce Apple-native app resources even when the caller's shell is translated by Rosetta and `uname -m` reports x86_64. Use the physical arm64 capability as the default and keep GHOSTEX_MACOS_ARCH=x86_64 as the explicit Intel build path.
default_macos_arch() {
	if [[ "$(/usr/sbin/sysctl -in hw.optional.arm64 2>/dev/null || true)" == "1" ]]; then
		printf 'arm64\n'
		return 0
	fi
	uname -m
}

GHOSTEX_MACOS_ARCH="${GHOSTEX_MACOS_ARCH:-$(default_macos_arch)}"
case "$GHOSTEX_MACOS_ARCH" in
arm64 | aarch64)
	GHOSTEX_MACOS_ARCH="arm64"
	;;
x86_64 | x64 | amd64)
	GHOSTEX_MACOS_ARCH="x86_64"
	;;
*)
	echo "Unsupported GHOSTEX_MACOS_ARCH: $GHOSTEX_MACOS_ARCH" >&2
	exit 1
	;;
esac
BUILD_CACHE_DIR="${GHOSTEX_BUILD_CACHE_DIR:-$REPO_ROOT/build/$GHOSTEX_MACOS_ARCH/build-cache}"
GHOSTEX_REMOTE_GXSERVER_LINUX_X64_DEFAULT_PACKAGE="$REPO_ROOT/build/remote-gxserver-linux/x64/package"
GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_DEFAULT_PACKAGE="$REPO_ROOT/build/remote-gxserver-linux/arm64/package"
# CDXC:RemoteMachines 2026-06-23-23:16: Remote Linux gxserver package staging is optional for normal Rust local starts, but the staging probe still runs in every gxserver package mode. Define the deterministic default package paths before the package-mode switch so `set -u` can safely skip absent Linux packages instead of treating the defaults as mode-specific required variables.
GHOSTEX_GXSERVER_PACKAGE_MODE="${GHOSTEX_GXSERVER_PACKAGE_MODE:-rust}"
case "$GHOSTEX_GXSERVER_PACKAGE_MODE" in
rust | rs)
	GHOSTEX_GXSERVER_PACKAGE_MODE="rust"
	;;
*)
	echo "GPUI supports only the Rust gxserver runtime; remove GHOSTEX_GXSERVER_PACKAGE_MODE or set it to rust." >&2
	exit 1
	;;
esac
GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE="${GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE:-}"
GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE="${GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE:-}"
GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES="${GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES:-0}"
case "$(printf '%s' "$GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES" | tr '[:upper:]' '[:lower:]')" in
1 | true | yes | on)
	GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1
	;;
*)
	GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=0
	;;
esac
# CDXC:Release 2026-07-02-14:10: Release app bundles stop embedding the Ubuntu remote gxserver payloads. In this mode the build tars those payloads into build/on-demand-assets/<version>/ and seals their checksums into Web/on-demand-resources.json inside the signed app. Dev builds keep bundling everything locally, so this stays a release-only mode.
GHOSTEX_ON_DEMAND_ASSETS="${GHOSTEX_ON_DEMAND_ASSETS:-0}"
case "$(printf '%s' "$GHOSTEX_ON_DEMAND_ASSETS" | tr '[:upper:]' '[:lower:]')" in
1 | true | yes | on)
	GHOSTEX_ON_DEMAND_ASSETS=1
	;;
*)
	GHOSTEX_ON_DEMAND_ASSETS=0
	;;
esac
# CDXC:Build 2026-06-22-23:23: `bun run start` should stay stable for full maintainer checkouts while allowing contributor clones that omit optional submodules. Enable missing-optional-submodule skips only for local starts by default; release and direct strict builds must keep failing when Source resources are absent.
GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES="${GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES:-${GHOSTEX_LOCAL_START:-0}}"
case "$(printf '%s' "$GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES" | tr '[:upper:]' '[:lower:]')" in
1 | true | yes | on)
	GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES=1
	;;
*)
	GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES=0
	;;
esac

APP_CAPABILITY_SHARED_NODE_RUNTIME=false
APP_CAPABILITY_SOURCE_EDITOR=false
APP_CAPABILITY_ZMX=true
APP_OPTIONAL_RESOURCE_NOTES=()

record_optional_resource_note() {
	local feature="$1"
	local reason="$2"
	APP_OPTIONAL_RESOURCE_NOTES+=("$feature: $reason")
	printf 'Skipping optional %s: %s\n' "$feature" "$reason" >&2
}

acquire_local_start_lock_if_needed() {
	if [[ "${GHOSTEX_START_LOCK_HELD:-}" == "1" || "${GHOSTEX_BUILD_LOCK_HELD:-}" == "1" ]]; then
		return 0
	fi
	local lock_file="$REPO_ROOT/build/ghostex-local-start.lock"
	mkdir -p "$(dirname "$lock_file")"
	# CDXC:Build 2026-06-11-18:59: Direct native builds mutate the same DerivedData app bundle that `bun run start` later mirrors into /Applications. Re-enter under the local-start lock unless the launcher already owns it, so a direct build cannot remove generated CEF payloads while another process installs the signed app.
	exec /usr/bin/lockf -k "$lock_file" /usr/bin/env GHOSTEX_BUILD_LOCK_HELD=1 /bin/bash "$0" "$@"
}

acquire_local_start_lock_if_needed "$@"

# Content-hash stamp helpers (fingerprint_inputs, cache_matches,
# write_cache_stamp, path_identity) are shared with build-macos-app.sh.
# shellcheck source=build-cache.sh
source "$SCRIPT_DIR/build-cache.sh"

binary_supports_macos_arch() {
	local binary_path="$1"
	local expected_arch="$2"
	local archs
	if [[ ! -f "$binary_path" ]]; then
		return 1
	fi
	archs="$(/usr/bin/lipo -archs "$binary_path" 2>/dev/null || true)"
	for arch in $archs; do
		if [[ "$arch" == "$expected_arch" ]]; then
			return 0
		fi
	done
	return 1
}

node_pty_prebuild_platform_dir() {
	case "$GHOSTEX_MACOS_ARCH" in
	arm64)
		printf 'darwin-arm64\n'
		;;
	x86_64)
		printf 'darwin-x64\n'
		;;
	esac
}

normalize_node_pty_prebuilds() {
	local root="$1"
	local keep_platform node_pty_root prebuild_dir release_dir
	keep_platform="$(node_pty_prebuild_platform_dir)"
	if [[ ! -d "$root" ]]; then
		return 0
	fi
	# VS Code may compile node-pty into build/Release instead of retaining the
	# downloaded prebuild tree. App resources use one architecture-explicit
	# location so validation, signing, and node-pty's runtime loader all resolve
	# the same two native files.
	while IFS= read -r -d '' node_pty_root; do
		prebuild_dir="$node_pty_root/prebuilds/$keep_platform"
		release_dir="$node_pty_root/build/Release"
		if [[ ! -f "$prebuild_dir/pty.node" || ! -f "$prebuild_dir/spawn-helper" ]]; then
			if [[ ! -f "$release_dir/pty.node" || ! -f "$release_dir/spawn-helper" ]]; then
				echo "node-pty is missing native artifacts for $keep_platform under $node_pty_root" >&2
				return 1
			fi
			mkdir -p "$prebuild_dir"
			cp -p "$release_dir/pty.node" "$release_dir/spawn-helper" "$prebuild_dir/"
		fi
		rm -rf "$node_pty_root/build"
	done < <(find "$root" -path '*/node_modules/node-pty' -type d -print0)
}

prune_node_pty_prebuilds() {
	local root="$1"
	local keep_platform prebuilds_dir platform_dir
	keep_platform="$(node_pty_prebuild_platform_dir)"
	if [[ ! -d "$root" ]]; then
		return 0
	fi
	# CDXC:Release 2026-06-08-19:49: macOS DMGs are built per architecture, so bundled app resources must keep only the matching node-pty darwin prebuild. Prune Windows/Linux and opposite-arch prebuild directories from generated code-server payloads to reduce download size without changing runtime behavior.
	while IFS= read -r -d '' prebuilds_dir; do
		while IFS= read -r -d '' platform_dir; do
			if [[ "$(basename "$platform_dir")" != "$keep_platform" ]]; then
				rm -rf "$platform_dir"
			fi
		done < <(find "$prebuilds_dir" -mindepth 1 -maxdepth 1 -type d -print0)
	done < <(find "$root" -path '*/node_modules/node-pty/prebuilds' -type d -print0)
}

node_pty_prebuilds_match_arch() {
	local root="$1"
	local keep_platform prebuilds_dir platform_dir
	keep_platform="$(node_pty_prebuild_platform_dir)"
	if [[ ! -d "$root" ]]; then
		return 1
	fi
	while IFS= read -r -d '' prebuilds_dir; do
		if [[ ! -d "$prebuilds_dir/$keep_platform" ]]; then
			return 1
		fi
		while IFS= read -r -d '' platform_dir; do
			if [[ "$(basename "$platform_dir")" != "$keep_platform" ]]; then
				return 1
			fi
		done < <(find "$prebuilds_dir" -mindepth 1 -maxdepth 1 -type d -print0)
	done < <(find "$root" -path '*/node_modules/node-pty/prebuilds' -type d -print0)
	return 0
}

source "$SCRIPT_DIR/prepare-macos-code-server.sh"

portless_staged_cli_smoke_check() {
	local target_dir="$1"
	env NO_COLOR=1 PATH="$CODE_SERVER_NODE_DIR:$PATH" "$CODE_SERVER_NODE_BIN" "$target_dir/dist/cli.js" --help >/dev/null
}

package_portless_if_needed() {
	local source_dir="$REPO_ROOT/node_modules/portless"
	local source_cli="$source_dir/dist/cli.js"
	local target_dir="$WEB_DIR/portless"
	local package_digest package_version node_identity source_file
	local -a fingerprint_args

	if [[ ! -d "$source_dir" ]]; then
		echo "Portless package is missing at $source_dir." >&2
		echo "Run bun install before packaging Ghostex so node_modules/portless contains the pinned portless@0.14.0 package." >&2
		exit 1
	fi
	if [[ ! -f "$source_cli" ]]; then
		echo "Portless CLI is missing: $source_cli" >&2
		echo "Run bun install or rebuild the installed portless@0.14.0 package before packaging Ghostex; dist/cli.js is required." >&2
		exit 1
	fi

	package_version="$("$CODE_SERVER_NODE_BIN" -e "const fs=require('fs'); const pkg=JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); process.stdout.write(String(pkg.version || ''));" "$source_dir/package.json")"
	if [[ "$package_version" != "0.14.0" ]]; then
		echo "Ghostex packaging expected portless@0.14.0 in node_modules/portless, found version $package_version." >&2
		echo "Run bun install with the root lockfile before packaging Ghostex." >&2
		exit 1
	fi

	node_identity="$("$CODE_SERVER_NODE_BIN" -p 'process.version + ":" + process.versions.modules')"
	fingerprint_args=(
		--value "portless-package-v1"
		--value "arch=$GHOSTEX_MACOS_ARCH"
		--value "node=$node_identity"
		--value "version=$package_version"
		--path "$SCRIPT_DIR/prepare-macos-runtime.sh"
		--path "$REPO_ROOT/package.json"
		--path "$REPO_ROOT/bun.lock"
	)
	while IFS= read -r source_file; do
		fingerprint_args+=(--path "$source_file")
	done < <(find "$source_dir" -type f -print | LC_ALL=C sort)
	package_digest="$(fingerprint_inputs "${fingerprint_args[@]}")"

	# CDXC:Portless 2026-06-22-22:26: Ghostex packages the published portless@0.14.0 CLI as Web/portless and runs it with the shared Web/code-server/lib/node runtime. Do not stage a second Node runtime; fail packaging if the installed package does not contain dist/cli.js.
	if cache_matches "portless-package-$GHOSTEX_MACOS_ARCH" "$package_digest" "$target_dir/package.json" "$target_dir/dist/cli.js" &&
		portless_staged_cli_smoke_check "$target_dir" >/dev/null 2>&1; then
		echo "Portless package is current; skipping package rebuild."
		return 0
	fi

	rm -rf "$target_dir"
	mkdir -p "$target_dir"
	rsync -a --delete "$source_dir/" "$target_dir/"
	chmod 755 "$target_dir/dist/cli.js"
	if ! portless_staged_cli_smoke_check "$target_dir"; then
		echo "Staged Portless CLI failed to run with code-server Node: $CODE_SERVER_NODE_BIN" >&2
		exit 1
	fi
	write_cache_stamp "portless-package-$GHOSTEX_MACOS_ARCH" "$package_digest"
}

# The macOS 26/27 SDK gates INFINITY/NAN behind clang's __need_infinity_nan protocol, which
# Zig's bundled clang does not implement, so linking any exe with C++ objects (ghostty-vt pulls
# in simdutf/highway) fails inside libc++'s clamp_to_integral.h. This is NOT a Zig 0.15 quirk:
# 0.16 ships the same clang behaviour and fails identically, so the SDK overlay below stays for
# as long as the SDK keeps that gate.
macos_sdk_needs_infinity_fix() {
	local sdk="$1"
	[[ -f "$sdk/usr/include/math.h" ]] || return 1
	grep -q '__need_infinity_nan' "$sdk/usr/include/math.h" &&
		! grep -q 'Ghostex INFINITY fallback' "$sdk/usr/include/math.h"
}

synthesize_macos_sdk_overlay() {
	local source_sdk="$1"
	local overlay_sdk="$2"
	rm -rf "$overlay_sdk"
	mkdir -p "$overlay_sdk/usr/include"
	local entry name
	for entry in "$source_sdk"/*; do
		name="$(basename "$entry")"
		[[ "$name" == "usr" ]] && continue
		ln -s "$entry" "$overlay_sdk/$name"
	done
	for entry in "$source_sdk"/usr/*; do
		name="$(basename "$entry")"
		[[ "$name" == "include" ]] && continue
		ln -s "$entry" "$overlay_sdk/usr/$name"
	done
	for entry in "$source_sdk"/usr/include/*; do
		name="$(basename "$entry")"
		[[ "$name" == "math.h" ]] && continue
		ln -s "$entry" "$overlay_sdk/usr/include/$name"
	done
	{
		cat "$source_sdk/usr/include/math.h"
		cat <<'MATH_EOF'

/* Ghostex INFINITY fallback: the guards above skip these macros when clang
 * reports modules support but its float.h lacks __need_infinity_nan (true for
 * Zig's bundled clang, 0.15 and 0.16 alike). Harmless when already defined. */
#ifndef INFINITY
#define INFINITY    HUGE_VALF
#endif
#ifndef NAN
#define NAN         __builtin_nanf("0x7fc00000")
#endif
MATH_EOF
	} >"$overlay_sdk/usr/include/math.h"
}

build_zmx_if_needed() {
	local output_path="$ZMX_ROOT/zig-out/bin/zmx"
	local build_digest
	build_digest="$(fingerprint_inputs \
		--value "zmx-build-v1" \
		--value "target=$ZMX_TARGET" \
		--value "zig=$ZIG_VERSION" \
		--path "$ZMX_ROOT/src" \
		--path "$ZMX_ROOT/build.zig" \
		--path "$ZMX_ROOT/build.zig.zon")"
	if cache_matches "zmx-$GHOSTEX_MACOS_ARCH" "$build_digest" "$output_path"; then
		# CDXC:Build 2026-06-08-08:42: zmx writes every macOS target to .dependencies/zmx/zig-out/bin/zmx, so an old per-arch cache stamp is not enough to prove the shared output still contains the requested CPU slice. Verify the Mach-O architecture before skipping or Ghostex can launch Intel zmx from an arm64 app.
		if binary_supports_macos_arch "$output_path" "$GHOSTEX_MACOS_ARCH"; then
			echo "zmx is current; skipping Zig build."
			return 0
		fi
		echo "zmx cache is stale for $GHOSTEX_MACOS_ARCH; rebuilding Zig artifact."
	fi

	(
		cd "$ZMX_ROOT"
		# CDXC:Zmx 2026-05-20-10:23: Zig resolves the native build runner through the selected macOS Xcode SDK, which can fail before zmx compilation starts. Scope the Command Line Tools developer dir to the zmx submodule build only; the zmx artifact itself is still built for the explicit deployment target above. This is not version-specific: it applied to Zig 0.15 and still applies to the 0.16 toolchain zmx builds with today.
		ZMX_BUILD_ENV=(env -u LDFLAGS ZIG="$ZIG_BIN")
		if [[ -z "${ZMX_BUILD_DEVELOPER_DIR:-}" ]] &&
			DEVELOPER_DIR=/Library/Developer/CommandLineTools /usr/bin/xcrun --sdk macosx --show-sdk-path >/dev/null 2>&1; then
			ZMX_BUILD_DEVELOPER_DIR=/Library/Developer/CommandLineTools
		fi
		if [[ -n "${ZMX_BUILD_DEVELOPER_DIR:-}" ]]; then
			ZMX_BUILD_ENV+=(DEVELOPER_DIR="$ZMX_BUILD_DEVELOPER_DIR")
		fi
		if [[ -n "${ZMX_BUILD_DEVELOPER_DIR:-}" ]]; then
			zmx_sdk="$(DEVELOPER_DIR="$ZMX_BUILD_DEVELOPER_DIR" /usr/bin/xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)"
		else
			zmx_sdk="$(/usr/bin/xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)"
		fi
		if [[ -n "$zmx_sdk" ]] && macos_sdk_needs_infinity_fix "$zmx_sdk"; then
			overlay_sdk="$ZMX_ROOT/.zig-cache/ghostex-sdk-overlay/$(basename "$zmx_sdk")"
			if [[ ! -f "$overlay_sdk/usr/include/math.h" ]] ||
				[[ "$zmx_sdk/usr/include/math.h" -nt "$overlay_sdk/usr/include/math.h" ]]; then
				synthesize_macos_sdk_overlay "$zmx_sdk" "$overlay_sdk"
			fi
			shim_dir="$(mktemp -d "${TMPDIR:-/tmp}/ghostex-zmx-xcrun.XXXXXX")"
			trap 'rm -rf "$shim_dir"' EXIT
			cat >"$shim_dir/xcrun" <<XCRUN_EOF
#!/usr/bin/env bash
set -euo pipefail
if [[ "\${1:-}" == "--sdk" && "\${2:-}" == "macosx" && "\${3:-}" == "--show-sdk-path" ]]; then
	echo "$overlay_sdk"
	exit 0
fi
if [[ "\${1:-}" == "--show-sdk-path" ]]; then
	echo "$overlay_sdk"
	exit 0
fi
exec /usr/bin/xcrun "\$@"
XCRUN_EOF
			chmod +x "$shim_dir/xcrun"
			ZMX_BUILD_ENV+=(PATH="$shim_dir:$PATH")
			echo "zmx build: using INFINITY-patched SDK overlay at $overlay_sdk"
		fi
		"${ZMX_BUILD_ENV[@]}" "$ZIG_BIN" build -Doptimize=ReleaseSafe -Dtarget="$ZMX_TARGET"
	)
	write_cache_stamp "zmx-$GHOSTEX_MACOS_ARCH" "$build_digest"
}

gxserver_rust_cargo_target() {
	case "$GHOSTEX_MACOS_ARCH" in
	arm64)
		printf 'aarch64-apple-darwin\n'
		;;
	x86_64)
		printf 'x86_64-apple-darwin\n'
		;;
	esac
}

resolve_gxserver_rust_cargo() {
	local cargo_bin="${GXSERVER_RUST_CARGO:-${CARGO:-}}"
	if [[ -z "$cargo_bin" ]]; then
		cargo_bin="$(command -v cargo || true)"
	fi
	if [[ -z "$cargo_bin" ]]; then
		cat >&2 <<EOF
Cargo is required to build bundled Rust gxserver.

Install Rust, then rerun this script:
  rustup toolchain install stable
EOF
		exit 1
	fi
	printf '%s\n' "$cargo_bin"
}

# CDXC:Telemetry 2026-08-26: the marketing version gxserver bakes in
# (server/build.rs). Same resolution rule build-macos-app.sh uses for the desktop
# crate: an explicit env wins, otherwise the root package.json is the source of
# truth. Without this the daemon would report its placeholder crate version
# (0.1.0) as `server_version` in every analytics event and in every build.
resolve_gxserver_marketing_version() {
	if [[ -n "${GHOSTEX_GPUI_MARKETING_VERSION:-}" ]]; then
		printf '%s\n' "$GHOSTEX_GPUI_MARKETING_VERSION"
		return 0
	fi
	node -p "require('$REPO_ROOT/package.json').version"
}

build_gxserver_rust_if_needed() {
	local cargo_bin cargo_target output_path cli_output_path
	local marketing_version
	if [[ ! -f "$GXSERVER_RS_ROOT/Cargo.toml" ]]; then
		cat >&2 <<EOF
Rust gxserver source is missing:
  $GXSERVER_RS_ROOT

Initialize or provide gxserver-rs before building the app bundle.
EOF
		exit 1
	fi
	cargo_bin="$(resolve_gxserver_rust_cargo)"
	cargo_target="$(gxserver_rust_cargo_target)"
	output_path="$GXSERVER_RS_ROOT/target/$cargo_target/release/gxserver"
	cli_output_path="$GXSERVER_RS_ROOT/target/$cargo_target/release/ghostex"
	GXSERVER_RUST_BIN=""
	marketing_version="$(resolve_gxserver_marketing_version)"
	# CDXC:Build 2026-09-05 WHY:
	# The source-only stamp missed packages/find, packages/paths, build.rs and profile changes, allowing stale server binaries.
	# Cargo tracks the whole dependency graph and its warm freshness check takes about 0.2 seconds.

	# CDXC:Build 2026-06-24-20:22: Local start must fail before packaging when server no longer compiles. This function is called outside command substitution so `set -e` can abort on Cargo errors instead of stamping the current source digest and copying a stale daemon binary.
	# CDXC:Build 2026-09-02: cargo discovers `.cargo/config.toml` from its working directory, not from `--manifest-path`, and `bun run start` runs this script from the repo root. Build from inside the server crate so `server/.cargo/config.toml` (sccache rustc-wrapper) applies; the target dir is still `$GXSERVER_RS_ROOT/target`, so the output paths above are unchanged.
	# CDXC:Build 2026-09-04 WHY:
	# gxserver is one 168k-line leaf crate, so every local start that touched
	# server/ paid a full non-incremental release compile (~27s). The per-package
	# override enables incremental codegen for the gxserver package only;
	# dependencies keep their profile hash and stay served by sccache, and
	# release builds (no GHOSTEX_LOCAL_START) keep the plain release profile.
	# SEE-ALSO: apps/desktop/scripts/build-macos-rust.sh does the same for the desktop crate.
	local -a cargo_profile_args=()
	if [[ "${GHOSTEX_LOCAL_START:-0}" == "1" ]]; then
		cargo_profile_args+=(--config 'profile.release.package.gxserver.incremental=true')
	fi
	(
		cd "$GXSERVER_RS_ROOT"
		GHOSTEX_GPUI_MARKETING_VERSION="$marketing_version" \
			"$cargo_bin" build --release --bins --manifest-path "$GXSERVER_RS_ROOT/Cargo.toml" --target "$cargo_target" ${cargo_profile_args[@]+"${cargo_profile_args[@]}"}
	)
	if ! binary_supports_macos_arch "$output_path" "$GHOSTEX_MACOS_ARCH"; then
		echo "Rust gxserver binary does not contain $GHOSTEX_MACOS_ARCH: $output_path" >&2
		exit 1
	fi
	if ! binary_supports_macos_arch "$cli_output_path" "$GHOSTEX_MACOS_ARCH"; then
		echo "Rust ghostex CLI binary does not contain $GHOSTEX_MACOS_ARCH: $cli_output_path" >&2
		exit 1
	fi
	GXSERVER_RUST_BIN="$output_path"
}

gxserver_rust_package_supports_macos_arch() {
	local target_dir="$1"
	local binary_path
	for binary_path in \
		"$target_dir/bin/gxserver" \
		"$target_dir/bin/ghostex" \
		"$target_dir/bin/zmx"; do
		if ! binary_supports_macos_arch "$binary_path" "$GHOSTEX_MACOS_ARCH"; then
			return 1
		fi
	done
	return 0
}

gxserver_package_supports_macos_arch() {
	local target_dir="$1"
	gxserver_rust_package_supports_macos_arch "$target_dir"
}

gxserver_rust_package_version() {
	local cargo_bin metadata package_version
	cargo_bin="$(resolve_gxserver_rust_cargo)"
	metadata="$("$cargo_bin" metadata --format-version 1 --no-deps --manifest-path "$GXSERVER_RS_ROOT/Cargo.toml")"
	package_version="$(GXSERVER_METADATA_JSON="$metadata" "$GXSERVER_NODE_BIN" -e '
	const metadata = JSON.parse(process.env.GXSERVER_METADATA_JSON ?? "{}");
	const rootPackageId = metadata.root_package_id ?? metadata.resolve?.root;
	const rootPackage =
		metadata.packages.find((pkg) => pkg.id === rootPackageId) ??
		metadata.packages.find((pkg) => pkg.name === "gxserver") ??
		metadata.packages[0];
	process.stdout.write(String(rootPackage?.version ?? ""));
	')"
	if [[ -z "$package_version" ]]; then
		echo "Could not read gxserver-rs package version from $GXSERVER_RS_ROOT/Cargo.toml" >&2
		exit 1
	fi
	printf '%s\n' "$package_version"
}

stage_gxserver_protocol_exports() {
	local target_dir="$1"
	local protocol_stage_dir="$BUILD_CACHE_DIR/gxserver-protocol"
	local tsc_bin="$REPO_ROOT/node_modules/typescript/bin/tsc"
	if [[ ! -f "$REPO_ROOT/packages/shared/gxserver-protocol.ts" ]]; then
		echo "shared gxserver protocol source is missing: $REPO_ROOT/packages/shared/gxserver-protocol.ts" >&2
		exit 1
	fi
	if [[ ! -f "$tsc_bin" ]]; then
		echo "TypeScript compiler is missing at $tsc_bin. Run bun install before packaging gxserver." >&2
		exit 1
	fi
	rm -rf "$protocol_stage_dir"
	mkdir -p "$protocol_stage_dir/src" "$protocol_stage_dir/types" "$target_dir/dist/protocol"
	cp "$REPO_ROOT/packages/shared/gxserver-protocol.ts" "$protocol_stage_dir/src/index.ts"
	# CDXC:ServerApi 2026-08-21-12:10: packages/shared/gxserver-protocol.ts pulls in
	# sibling shared modules (session-chat.ts, which now pulls session-chat-queue.ts).
	# Stage the whole relative-import closure instead of a hand-kept file list so adding a
	# shared module never breaks packaging with a TS2307 "cannot find module" failure.
	GXSERVER_PROTOCOL_SHARED_DIR="$REPO_ROOT/packages/shared" \
		GXSERVER_PROTOCOL_STAGE_SRC_DIR="$protocol_stage_dir/src" \
		"$GXSERVER_NODE_BIN" <<'JS'
const fs = require("node:fs");
const path = require("node:path");

const sharedDir = process.env.GXSERVER_PROTOCOL_SHARED_DIR;
const stageSrcDir = process.env.GXSERVER_PROTOCOL_STAGE_SRC_DIR;
const relativeSpecifier = /(?:^|[\s;])(?:import|export)\s[^;]*?from\s*["'](\.[^"']*)["']/g;

const pending = [path.join(stageSrcDir, "index.ts")];
const staged = new Set(pending);
while (pending.length > 0) {
	const filePath = pending.pop();
	const source = fs.readFileSync(filePath, "utf8");
	for (const match of source.matchAll(relativeSpecifier)) {
		const specifier = match[1].replace(/\.(?:ts|tsx|js)$/, "");
		const moduleName = `${specifier.replace(/^\.\//, "")}.ts`;
		const sourcePath = path.join(sharedDir, moduleName);
		const stagedPath = path.join(stageSrcDir, moduleName);
		if (staged.has(stagedPath)) {
			continue;
		}
		if (!fs.existsSync(sourcePath)) {
			console.error(`shared gxserver protocol dependency is missing: ${sourcePath}`);
			process.exit(1);
		}
		fs.mkdirSync(path.dirname(stagedPath), { recursive: true });
		fs.copyFileSync(sourcePath, stagedPath);
		staged.add(stagedPath);
		pending.push(stagedPath);
	}
}
JS
	bun build "$protocol_stage_dir/src/index.ts" --outfile "$target_dir/dist/protocol/index.js" --format esm --target node
	"$GXSERVER_NODE_BIN" "$tsc_bin" \
		--declaration \
		--emitDeclarationOnly \
		--isolatedModules \
		--module ESNext \
		--moduleResolution bundler \
		--outDir "$protocol_stage_dir/types" \
		--rootDir "$protocol_stage_dir/src" \
		--skipLibCheck \
		--strict \
		--target ES2023 \
		"$protocol_stage_dir/src/index.ts"
	cp "$protocol_stage_dir"/types/*.d.ts "$target_dir/dist/protocol/"
}

write_gxserver_rust_package_manifest() {
	local target_dir="$1"
	local package_version="$2"
	GXSERVER_PACKAGE_DIR="$target_dir" GXSERVER_PACKAGE_VERSION="$package_version" "$GXSERVER_NODE_BIN" <<'JS'
const { writeFileSync } = require("node:fs");
const { join } = require("node:path");

const targetDir = process.env.GXSERVER_PACKAGE_DIR;
const version = process.env.GXSERVER_PACKAGE_VERSION;
writeFileSync(
	join(targetDir, "package.json"),
	`${JSON.stringify({
		name: "gxserver",
		version,
		private: true,
		description: "Ghostex gxserver daemon and shared protocol package.",
		type: "module",
		bin: {
			gxserver: "./bin/gxserver",
		},
		exports: {
			"./protocol": {
				types: "./dist/protocol/index.d.ts",
				default: "./dist/protocol/index.js",
			},
		},
	}, null, 2)}\n`,
	"utf8",
);
JS
}

write_gxserver_rust_package_readme() {
	local target_dir="$1"
	cat >"$target_dir/README.md" <<'EOF'
# gxserver server package

gxserver is the Ghostex daemon used by the desktop app and server-only remote installs.

## Runtime dependency

This package uses the bundled native gxserver executable in `bin/gxserver` and does not require Node.js or better-sqlite3 at runtime.

## Commands

- `bin/gxserver`: run gxserver in the foreground.
- `bin/gxserver start`: start gxserver in the background.
- `bin/gxserver status --json`: check runtime state for health/status automation.
- `bin/gxserver stop`: stop only the gxserver control plane; zmx sessions are not killed.
- `bin/gxserver stop-all`: kill gxserver-tracked zmx sessions, then stop the control plane.

The package includes Ghostex's pinned zmx artifact in `bin/`.
EOF
}

write_gxserver_package_build_identity() {
	local target_dir="$1"
	local package_version="$2"
	GXSERVER_PACKAGE_DIR="$target_dir" \
		GXSERVER_PACKAGE_VERSION="$package_version" \
		"$GXSERVER_NODE_BIN" <<'JS'
const { createHash } = require("node:crypto");
const { lstatSync, readFileSync, readdirSync, writeFileSync } = require("node:fs");
const { join, relative, sep } = require("node:path");

const targetDir = process.env.GXSERVER_PACKAGE_DIR;
const version = process.env.GXSERVER_PACKAGE_VERSION;
const hash = createHash("sha256");

function walk(dir) {
	for (const entry of readdirSync(dir, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
		const entryPath = join(dir, entry.name);
		const packagePath = relative(targetDir, entryPath).split(sep).join("/");
		if (packagePath === "build-identity.json") {
			continue;
		}
		if (entry.isDirectory()) {
			walk(entryPath);
			continue;
		}
		const stat = lstatSync(entryPath);
		if (!stat.isFile() && !stat.isSymbolicLink()) {
			continue;
		}
		hash.update(packagePath);
		hash.update("\0");
		hash.update(readFileSync(entryPath));
		hash.update("\0");
	}
}

walk(targetDir);
const fingerprint = `sha256:${hash.digest("hex")}`;
writeFileSync(
	join(targetDir, "build-identity.json"),
	`${JSON.stringify({
		buildIdentity: `gxserver:${version}:${fingerprint}`,
		fingerprint,
		packageVersion: version,
	}, null, 2)}\n`,
	"utf8",
);
JS
}

package_gxserver_rust_package() {
	local package_dir="$1"
	local rust_bin="$2"
	local package_version="$3"
	# CDXC:Release 2026-06-22-16:17: Local and release macOS builds no longer keep the deleted gxserver/ TypeScript source tree. Assemble the Rust daemon package directly from server, packages/shared/gxserver-protocol.ts, and app-owned tool binaries so `bun run start` never cds into gxserver/ for the default packaged daemon.
	# CDXC:Build 2026-06-22-23:23: zmx remains required.
	rm -rf "$package_dir"
	mkdir -p "$package_dir/bin"
	cp "$rust_bin" "$package_dir/bin/gxserver"
	# CDXC:Cli 2026-07-13: the public ghostex/gx CLI is the native
	# Rust binary built alongside gxserver; stage it in the same package so
	# app bundles and PATH wrappers resolve one implementation.
	cp "${rust_bin%/*}/ghostex" "$package_dir/bin/ghostex"
	cp "$WEB_DIR/bin/zmx" "$package_dir/bin/zmx"
	chmod 755 "$package_dir/bin/gxserver" "$package_dir/bin/ghostex" "$package_dir/bin/zmx"
	stage_gxserver_protocol_exports "$package_dir"
	write_gxserver_rust_package_manifest "$package_dir" "$package_version"
	write_gxserver_rust_package_readme "$package_dir"
	write_gxserver_package_build_identity "$package_dir" "$package_version"
}

validate_remote_gxserver_linux_package() {
	local package_dir="$1"
	local package_label="$2"
	local required_path file_output
	for required_path in \
		"bin/gxserver" \
		"bin/ghostex" \
		"bin/zmx"; do
		if [[ ! -e "$package_dir/$required_path" ]]; then
			echo "Remote gxserver $package_label package is missing required resource: $required_path" >&2
			return 1
		fi
	done
	for required_path in \
		"bin/gxserver" \
		"bin/zmx"; do
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
				echo "Remote gxserver $package_label package has the wrong Linux ELF architecture at $required_path: $file_output" >&2
				return 1
			fi
			;;
		LINUX_ARM64)
			if [[ "$file_output" != *"aarch64"* && "$file_output" != *"AArch64"* ]]; then
				echo "Remote gxserver $package_label package has the wrong Linux ELF architecture at $required_path: $file_output" >&2
				return 1
			fi
			;;
		esac
	done
}

stage_remote_gxserver_linux_package_if_configured() {
	local source_dir="$1"
	local target_name="$2"
	local package_label="$3"
	local default_source_dir="$4"
	local target_dir="$WEB_DIR/$target_name"
	local source_is_default=0
	local validation_output
	if [[ -z "$source_dir" && -d "$default_source_dir" ]]; then
		source_dir="$default_source_dir"
		source_is_default=1
	fi
	if [[ -z "$source_dir" ]]; then
		if [[ "$GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES" == "1" ]]; then
			echo "Missing $package_label remote gxserver package. Set GHOSTEX_REMOTE_GXSERVER_${package_label}_PACKAGE to a prebuilt Linux package directory." >&2
			exit 1
		fi
		rm -rf "$target_dir"
		return 0
	fi
	if [[ ! -d "$source_dir" ]]; then
		echo "Configured $package_label remote gxserver package is not a directory: $source_dir" >&2
		exit 1
	fi
	if ! validation_output="$(validate_remote_gxserver_linux_package "$source_dir" "$package_label" 2>&1)"; then
		# CDXC:RemoteMachines 2026-06-30-00:31: Normal local starts should not fail because an optional auto-discovered Ubuntu gxserver package under build/ is stale after CLI resource changes. Strict release builds and explicit package env vars still fail validation; local starts clear the staged Web package and continue without remote install resources.
		if [[ "$source_is_default" == "1" && "$GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES" != "1" ]]; then
			echo "Remote gxserver $package_label default package is stale or incomplete; skipping optional staging." >&2
			rm -rf "$target_dir"
			return 0
		fi
		printf '%s\n' "$validation_output" >&2
		exit 1
	fi
	# CDXC:RemoteMachines 2026-06-23-09:46: macOS app bundles may stage Linux remote gxserver packages only from explicit prebuilt directories. Validate required gxserver/zmx/Node/Portless/CLI resources and require Linux ELF payloads before copying to Web/gxserver-linux-* so the installer never uploads the host Darwin package to Ubuntu.
	#
	# CDXC:RemoteMachines 2026-06-23-10:07: The Ubuntu package builder writes build/remote-gxserver-linux/<arch>/package by default. Auto-stage that deterministic output when it exists so release/local app packaging can include the already-built Linux package without requiring another env var or rebuilding it in the macOS app pass.
	rm -rf "$target_dir"
	mkdir -p "$target_dir"
	rsync -a --delete "$source_dir"/ "$target_dir"/
}

stage_remote_gxserver_linux_packages_if_configured() {
	if [[ "$GHOSTEX_ON_DEMAND_ASSETS" == "1" ]]; then
		# CDXC:Release 2026-07-02-14:10: On-demand releases publish the Ubuntu packages as version-pinned GitHub release assets instead of embedding them in the app bundle. stage_on_demand_release_assets validates the same source packages and tars them; nothing is copied under Web/.
		rm -rf "$WEB_DIR/gxserver-linux-x64" "$WEB_DIR/gxserver-linux-arm64"
		return 0
	fi
	stage_remote_gxserver_linux_package_if_configured "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE" "gxserver-linux-x64" "LINUX_X64" "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_DEFAULT_PACKAGE"
	stage_remote_gxserver_linux_package_if_configured "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE" "gxserver-linux-arm64" "LINUX_ARM64" "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_DEFAULT_PACKAGE"
}

resolve_on_demand_linux_package_source() {
	local configured_dir="$1"
	local default_dir="$2"
	local package_label="$3"
	local source_dir="$configured_dir"
	if [[ -z "$source_dir" ]]; then
		source_dir="$default_dir"
	fi
	if [[ ! -d "$source_dir" ]]; then
		echo "Missing $package_label remote gxserver package for on-demand release assets: $source_dir" >&2
		echo "Build it with: tooling/build-remote-gxserver-linux-release.sh" >&2
		exit 1
	fi
	if ! validate_remote_gxserver_linux_package "$source_dir" "$package_label"; then
		exit 1
	fi
	printf '%s\n' "$source_dir"
}

stage_on_demand_release_assets() {
	local version asset_dir x64_source arm64_source
	local x64_sha arm64_sha component_manifest
	local -a manifest_args
	if [[ "$GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES" != "1" ]]; then
		echo "GHOSTEX_ON_DEMAND_ASSETS=1 is a release-only mode and requires GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1." >&2
		exit 1
	fi
	if [[ "$GHOSTEX_MACOS_ARCH" != "arm64" ]]; then
		echo "GHOSTEX_ON_DEMAND_ASSETS=1 supports only arm64 release builds." >&2
		exit 1
	fi

	version="$(node -p 'require(process.argv[1]).version' "$REPO_ROOT/package.json")"
	if [[ -z "$version" || "$version" == "undefined" ]]; then
		echo "Could not read the release version from package.json for on-demand asset naming." >&2
		exit 1
	fi
	asset_dir="$REPO_ROOT/build/on-demand-assets/$version"

	x64_source="$(resolve_on_demand_linux_package_source "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_PACKAGE" "$GHOSTEX_REMOTE_GXSERVER_LINUX_X64_DEFAULT_PACKAGE" "LINUX_X64")"
	arm64_source="$(resolve_on_demand_linux_package_source "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_PACKAGE" "$GHOSTEX_REMOTE_GXSERVER_LINUX_ARM64_DEFAULT_PACKAGE" "LINUX_ARM64")"

	echo "Packaging on-demand release assets for $version into $asset_dir"
	rm -rf "$asset_dir"
	mkdir -p "$asset_dir"

	if [[ -n "${GHOSTEX_ON_DEMAND_LINUX_X64_ARCHIVE:-}" || -n "${GHOSTEX_ON_DEMAND_LINUX_ARM64_ARCHIVE:-}" ]]; then
		[[ -f "${GHOSTEX_ON_DEMAND_LINUX_X64_ARCHIVE:-}" ]] || {
			echo "GHOSTEX_ON_DEMAND_LINUX_X64_ARCHIVE is missing." >&2
			exit 1
		}
		[[ -f "${GHOSTEX_ON_DEMAND_LINUX_ARM64_ARCHIVE:-}" ]] || {
			echo "GHOSTEX_ON_DEMAND_LINUX_ARM64_ARCHIVE is missing." >&2
			exit 1
		}
		cp "$GHOSTEX_ON_DEMAND_LINUX_X64_ARCHIVE" "$asset_dir/gxserver-linux-x64.tar.gz"
		cp "$GHOSTEX_ON_DEMAND_LINUX_ARM64_ARCHIVE" "$asset_dir/gxserver-linux-arm64.tar.gz"
	else
		COPYFILE_DISABLE=1 /usr/bin/tar -czf "$asset_dir/gxserver-linux-x64.tar.gz" -C "$x64_source" .
		COPYFILE_DISABLE=1 /usr/bin/tar -czf "$asset_dir/gxserver-linux-arm64.tar.gz" -C "$arm64_source" .
	fi

	x64_sha="$(/usr/bin/shasum -a 256 "$asset_dir/gxserver-linux-x64.tar.gz" | awk '{print $1}')"
	arm64_sha="$(/usr/bin/shasum -a 256 "$asset_dir/gxserver-linux-arm64.tar.gz" | awk '{print $1}')"

	GHOSTEX_ODA_VERSION="$version" \
		GHOSTEX_ODA_ASSET_DIR="$asset_dir" \
		GHOSTEX_ODA_X64_SHA="$x64_sha" \
		GHOSTEX_ODA_ARM64_SHA="$arm64_sha" \
		node -e '
		const fs = require("fs");
		const path = require("path");
		const env = process.env;
		const assetDir = env.GHOSTEX_ODA_ASSET_DIR;
		const entries = [
			{ key: "gxserver-linux-x64", name: "gxserver-linux-x64.tar.gz", sha256: env.GHOSTEX_ODA_X64_SHA },
			{ key: "gxserver-linux-arm64", name: "gxserver-linux-arm64.tar.gz", sha256: env.GHOSTEX_ODA_ARM64_SHA },
		].map((entry) => {
			const filePath = path.join(assetDir, entry.name);
			return { ...entry, bytes: fs.statSync(filePath).size, path: filePath };
		});
		for (const entry of entries) {
			if (!/^[0-9a-f]{64}$/.test(entry.sha256 ?? "")) {
				console.error(`Invalid sha256 for on-demand asset ${entry.name}: ${entry.sha256}`);
				process.exit(1);
			}
		}
		const buildManifest = {
			assets: entries.map(({ key, name, sha256, bytes, path: filePath }) => ({ bytes, key, name, path: filePath, sha256 })),
			version: env.GHOSTEX_ODA_VERSION,
		};
		fs.writeFileSync(path.join(assetDir, "assets.json"), `${JSON.stringify(buildManifest, null, 2)}\n`);
	'
	component_manifest="${GHOSTEX_ON_DEMAND_COMPONENTS_MANIFEST:-$REPO_ROOT/build/on-demand-components/components.json}"
	if [[ -n "${GHOSTEX_ON_DEMAND_COMPONENTS_MANIFEST:-}" && ! -f "$component_manifest" ]]; then
		echo "Configured component manifest does not exist: $component_manifest" >&2
		exit 1
	fi
	manifest_args=(
		seal
		--build-manifest "$asset_dir/assets.json"
		--output "$WEB_DIR/on-demand-resources.json"
		--repo "maddada/Ghostex"
	)
	if [[ -f "$component_manifest" ]]; then
		manifest_args+=(--component-manifest "$component_manifest")
	fi
	node "$REPO_ROOT/tooling/release-gpui/on-demand-manifest.mjs" "${manifest_args[@]}"
	node "$REPO_ROOT/tooling/release-gpui/on-demand-manifest.mjs" validate-macos \
		--manifest "$WEB_DIR/on-demand-resources.json"

	rm -rf "$WEB_DIR/gxserver-linux-x64" "$WEB_DIR/gxserver-linux-arm64"
	echo "On-demand release assets ready: x64=$x64_sha arm64=$arm64_sha"
}

package_gxserver_if_needed() {
	local target_dir="$WEB_DIR/gxserver"
	local package_dir package_digest package_version rust_bin
	# GPUI bundles the native Rust gxserver package used by standalone installs.
	# TypeScript daemon packaging is intentionally unsupported.
	#
	# CDXC:Build 2026-06-07-16:23: gxserver packaging should skip work when gxserver runtime sources, package metadata, packager code, the bundled zmx binary, and generated protocol inputs are unchanged.
	#
	# Rust packaging preserves generated TypeScript protocol exports for web
	# consumers, but the daemon and public CLI are native executables.
	package_dir="$BUILD_CACHE_DIR/gxserver-rs/server-package"
	build_gxserver_rust_if_needed
	rust_bin="$GXSERVER_RUST_BIN"
	if [[ -z "$rust_bin" || ! -x "$rust_bin" ]]; then
		echo "Rust gxserver build did not produce an executable daemon path." >&2
		exit 1
	fi
	package_version="$(gxserver_rust_package_version)"
	package_digest="$(fingerprint_inputs \
		--value "gxserver-package-v9-rust-only" \
		--value "arch=$GHOSTEX_MACOS_ARCH" \
		--value "version=$package_version" \
		--value "rust=$(path_identity "$rust_bin")" \
		--path "$SCRIPT_DIR/prepare-macos-runtime.sh" \
		--path "$REPO_ROOT/packages/shared/gxserver-protocol.ts" \
		--path "$GXSERVER_RS_ROOT/src" \
		--path "$GXSERVER_RS_ROOT/Cargo.toml" \
		--path "$GXSERVER_RS_ROOT/Cargo.lock" \
		--path "$WEB_DIR/bin/zmx")"
	local cache_outputs=("$target_dir/build-identity.json" "$target_dir/bin/gxserver" "$target_dir/dist/protocol/index.js" "$target_dir/dist/protocol/index.d.ts")
	cache_outputs+=("$target_dir/bin/ghostex")
	if cache_matches "gxserver-package-$GHOSTEX_MACOS_ARCH" "$package_digest" "${cache_outputs[@]}" &&
		gxserver_package_supports_macos_arch "$target_dir"; then
		# CDXC:Release 2026-06-08-16:23: Web/gxserver is also shared across dual-architecture release passes. Do not accept a cache hit unless the staged gxserver and zmx binaries match the requested architecture, or Intel and arm64 DMGs can silently inherit the previous pass's native artifacts.
		echo "gxserver package is current; skipping package rebuild."
		return 0
	fi

	echo "Packaging Rust gxserver with $rust_bin"
	package_gxserver_rust_package "$package_dir" "$rust_bin" "$package_version"
	rm -rf "$target_dir"
	cp -R "$package_dir" "$target_dir"
	write_cache_stamp "gxserver-package-$GHOSTEX_MACOS_ARCH" "$package_digest"
}

write_build_capabilities_manifest() {
	local notes_payload=""
	local note
	# CDXC:Build 2026-06-23-04:03: Local starts may have no skipped optional resources. macOS /bin/bash 3.2 treats an empty array expansion as unbound under `set -u`, so emit an empty notes payload without expanding the array when it has no entries.
	if ((${#APP_OPTIONAL_RESOURCE_NOTES[@]} > 0)); then
		for note in "${APP_OPTIONAL_RESOURCE_NOTES[@]}"; do
			notes_payload+="$note"$'\n'
		done
	fi
	GHOSTEX_CAP_SHARED_NODE_RUNTIME="$APP_CAPABILITY_SHARED_NODE_RUNTIME" \
		GHOSTEX_CAP_SOURCE_EDITOR="$APP_CAPABILITY_SOURCE_EDITOR" \
		GHOSTEX_CAP_ZMX="$APP_CAPABILITY_ZMX" \
		GHOSTEX_CAP_ALLOW_MISSING_OPTIONAL="$GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES" \
		GHOSTEX_CAP_NOTES="$notes_payload" \
		GHOSTEX_CAPABILITIES_PATH="$WEB_DIR/ghostex-build-capabilities.json" \
		"$GXSERVER_NODE_BIN" <<'JS'
const { writeFileSync } = require("node:fs");

const capabilityPath = process.env.GHOSTEX_CAPABILITIES_PATH;
const notes = String(process.env.GHOSTEX_CAP_NOTES || "")
  .split(/\n/)
  .map((note) => note.trim())
  .filter(Boolean);
const bool = (name) => process.env[name] === "true" || process.env[name] === "1";

/*
CDXC:Build 2026-06-22-23:23:
The app bundle needs a structured resource-capability manifest so local validation and Settings can distinguish intentionally omitted optional contributor modules from broken packaged resources. Keep the payload free of filesystem paths because persistent app diagnostics may include the same capability fields later.
*/
writeFileSync(
  capabilityPath,
  `${JSON.stringify({
    generatedBy: "prepare-macos-runtime.sh",
    optionalSubmodulesMayBeMissing: bool("GHOSTEX_CAP_ALLOW_MISSING_OPTIONAL"),
    resources: {
      sharedNodeRuntime: bool("GHOSTEX_CAP_SHARED_NODE_RUNTIME"),
      sourceEditor: bool("GHOSTEX_CAP_SOURCE_EDITOR"),
      zmx: bool("GHOSTEX_CAP_ZMX"),
    },
    skippedOptionalResources: notes,
    version: 1,
  }, null, 2)}\n`,
  "utf8",
);
JS
}

# CDXC:CodeEditor 2026-06-08-12:17: code-server owns the bundled Node runtime in the macOS app. Build code-server with its pinned Node version and stage that runtime inside Web/code-server/lib/node; explicit TypeScript gxserver packages reuse that runtime instead of shipping a duplicate Node.
CODE_SERVER_NODE_BIN="$(prepare_code_server_app_node_runtime)"
CODE_SERVER_NODE_DIR="$(cd "$(dirname "$CODE_SERVER_NODE_BIN")" && pwd)"
CODE_SERVER_NPM_BIN="$CODE_SERVER_NODE_DIR/npm"
if [[ ! -x "$CODE_SERVER_NPM_BIN" ]]; then
	echo "npm is required in the cached code-server Node distribution: $CODE_SERVER_NPM_BIN" >&2
	exit 1
fi
CODE_SERVER_ROOT="$(resolve_code_server_root || true)"
if [[ -z "$CODE_SERVER_ROOT" ]]; then
	# CDXC:RepoStructure 2026-08-25: An explicitly configured root that does
	# not resolve is a caller mistake, not a stranded checkout, so keep its own message.
	# Otherwise a previously initialized code-server must never fall through to the
	# contributor skip below; that skip is only correct when nothing was ever set up.
	if [[ "$CODE_SERVER_ROOT_EXPLICITLY_CONFIGURED" != "1" ]]; then
		fail_if_code_server_checkout_is_stranded
	fi
	if [[ "$CODE_SERVER_ROOT_EXPLICITLY_CONFIGURED" == "1" || "$GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES" == "0" ]]; then
		cat >&2 <<EOF
code-server source is required to package the embedded Source-tab runtime.

Set CODE_SERVER_ROOT or GHOSTEX_CODE_SERVER_ROOT to a code-server checkout, or place it at:
  $REPO_ROOT/.dependencies/code-server
EOF
		exit 1
	fi
	record_optional_resource_note "Source editor" "code-server checkout was not found"
fi
CODE_SERVER_NODE_VERSION="$("$CODE_SERVER_NODE_BIN" -p 'process.version')"
CODE_SERVER_NODE_MAJOR="$("$CODE_SERVER_NODE_BIN" -p 'process.versions.node.split(".")[0]')"
if [[ "$CODE_SERVER_NODE_MAJOR" != "$CODE_SERVER_APP_NODE_MAJOR" ]]; then
	echo "Ghostex app code-server packaging must use bundled Node.js $CODE_SERVER_APP_NODE_MAJOR, got $CODE_SERVER_NODE_VERSION at $CODE_SERVER_NODE_BIN." >&2
	exit 1
fi

GXSERVER_NODE_BIN="$CODE_SERVER_NODE_BIN"
GXSERVER_NODE_DIR="$CODE_SERVER_NODE_DIR"
GXSERVER_NPM_BIN="$CODE_SERVER_NPM_BIN"
GXSERVER_NODE_VERSION="$("$GXSERVER_NODE_BIN" -p 'process.version')"
GXSERVER_NODE_MAJOR="$("$GXSERVER_NODE_BIN" -p 'process.versions.node.split(".")[0]')"
if [[ "$GXSERVER_NODE_MAJOR" != "$CODE_SERVER_APP_NODE_MAJOR" ]]; then
	echo "Ghostex app gxserver packaging must use code-server's bundled Node.js $CODE_SERVER_APP_NODE_MAJOR, got $GXSERVER_NODE_VERSION at $GXSERVER_NODE_BIN." >&2
	exit 1
fi
GXSERVER_NODE_MODULE_VERSION="$("$GXSERVER_NODE_BIN" -p 'process.versions.modules')"

# CDXC:Build 2026-05-29-11:24: `bun run start` builds zmx and its Ghostty Zig dependency.
# Both are on Zig 0.16 now (zmx was re-ported onto upstream/main for 0.16, matching the
# vendored ghostty pin), so the repo needs exactly one Zig toolchain. An explicit `ZIG` still
# wins; otherwise prefer a 0.16 binary from PATH/Homebrew/mise instead of blindly taking the
# first PATH entry, which may still be an old 0.15 keg.
ZIG_BIN="${ZIG:-}"
if [[ -z "$ZIG_BIN" ]]; then
	for zig_candidate in \
		"$(command -v zig || true)" \
		/opt/homebrew/bin/zig \
		/opt/homebrew/opt/zig@0.16/bin/zig \
		"$HOME/.local/share/mise/installs/zig/0.16"*/bin/zig; do
		[[ -n "$zig_candidate" && -x "$zig_candidate" ]] || continue
		# Remember the first usable binary so the version error below can name it.
		[[ -n "$ZIG_BIN" ]] || ZIG_BIN="$zig_candidate"
		if [[ "$("$zig_candidate" version 2>/dev/null || true)" == 0.16.* ]]; then
			ZIG_BIN="$zig_candidate"
			break
		fi
	done
fi
if [[ -z "$ZIG_BIN" ]]; then
	cat >&2 <<EOF
Zig 0.16.x is required to build Ghostex's native zmx/Ghostty dependency.

Install it, then rerun this script:
  brew install zig
  # or: mise install zig@0.16.0
EOF
	exit 1
fi
ZIG_VERSION="$("$ZIG_BIN" version 2>/dev/null || true)"
if [[ "$ZIG_VERSION" != 0.16.* ]]; then
	cat >&2 <<EOF
Zig 0.16.x is required to build Ghostex's native zmx/Ghostty dependency.

Selected Zig:
  $ZIG_BIN
  version: ${ZIG_VERSION:-unknown}

Install a 0.16 toolchain or set ZIG explicitly:
  brew install zig
  mise install zig@0.16.0
  ZIG=/opt/homebrew/bin/zig bun run start
EOF
	exit 1
fi
export ZIG="$ZIG_BIN"

mkdir -p "$WEB_DIR"
rm -rf "$CLI_DIR"
mkdir -p "$CLI_DIR"

# CDXC:Zmx 2026-05-20-09:57: zmx pane refresh is now a zmx IPC feature, so Ghostex must bundle the pinned submodule binary instead of depending on whichever zmx happens to be on PATH. Build the submodule for the requested macOS architecture and copy it into app resources where TerminalWorkspaceView can launch it directly.
if [[ ! -f "$ZMX_ROOT/build.zig" ]]; then
	{
		if [[ "$ZMX_ROOT_EXPLICITLY_CONFIGURED" == "1" ]]; then
			printf 'zmx source is missing:\n  %s\n\n' "$ZMX_ROOT"
			printf 'ZMX_ROOT is set to an external checkout, so it is not a submodule of this repository.\nPoint ZMX_ROOT at a zmx checkout that contains build.zig, or unset it to use the bundled submodule.\n'
		else
			# CDXC:RepoStructure 2026-08-25: `git submodule update --init` is the
			# wrong repair for a checkout the 2026-08-22 restructure stranded at the old
			# top-level zmx path: it re-clones instead of reusing the tree that is already
			# on disk. Detect that signature and print the move plus pointer repair.
			ZMX_LEGACY_ROOT="$(legacy_submodule_checkout zmx build.zig || true)"
			if [[ -n "$ZMX_LEGACY_ROOT" ]]; then
				printf 'zmx source is stranded at its pre-restructure path:\n  %s\n\n' "$ZMX_LEGACY_ROOT"
				cat <<EOF
The 2026-08-22 restructure moved the submodule from zmx to .dependencies/zmx.
Git cannot move a submodule working tree with the gitlink, so your checkout
stayed at the old path and .dependencies/zmx is empty.

Unblock this build without moving anything:
  ZMX_ROOT=$ZMX_LEGACY_ROOT bun run start

Repair the checkout (keeps the built zig-out payload):
  cd $REPO_ROOT
  rmdir .dependencies/zmx
  mv zmx .dependencies/zmx
  echo 'gitdir: ../../.git/modules/zmx' > .dependencies/zmx/.git
  git config -f .git/modules/zmx/config core.worktree ../../../.dependencies/zmx

Then verify the repair:
  git -C .dependencies/zmx rev-parse HEAD
  git submodule status .dependencies/zmx
EOF
			else
				printf 'zmx source is missing:\n  %s\n\n' "$ZMX_ROOT"
				printf 'Initialize submodules before building:\n  git -C %q submodule update --init --recursive -- %q\n' \
					"$REPO_ROOT" "$ZMX_ROOT"
			fi
		fi
	} >&2
	exit 1
fi
case "$GHOSTEX_MACOS_ARCH" in
arm64)
	ZMX_TARGET="aarch64-macos.15.0"
	;;
x86_64)
	ZMX_TARGET="x86_64-macos.13.0"
	;;
esac
build_zmx_if_needed
rm -rf "$WEB_DIR/bin"
mkdir -p "$WEB_DIR/bin"
cp "$ZMX_ROOT/zig-out/bin/zmx" "$WEB_DIR/bin/zmx"
chmod 755 "$WEB_DIR/bin/zmx"
# CDXC:Build 2026-06-22-23:23: Optional contributor submodules should be packaged when present and strict, but absent optional checkouts should only disable their feature in local starts. Keep zmx above as the hard terminal/persistence dependency; gate Source independently so one missing feature cannot remove the rest of the app shell.
if [[ -n "$CODE_SERVER_ROOT" ]]; then
	if [[ "$GHOSTEX_ON_DEMAND_ASSETS" == "1" ]] && published_code_server_component_asset; then
		echo "Skipping local code-server packaging because its immutable macOS component is already published."
	else
		package_code_server_if_needed
	fi
	APP_CAPABILITY_SOURCE_EDITOR=true
fi
stage_shared_code_server_node_runtime
package_portless_if_needed
package_gxserver_if_needed
# CDXC:Cli 2026-05-10-03:28: Shells resolve the installed macOS
# executable as a terminal command. Bundle the native CLI in app resources
# so main.swift can proxy command argv before the AppKit app starts.
# CDXC:Cli 2026-05-26-15:11: Public CLI commands are now `ghostex`
# and `gx`; the bundled binary filename follows the long public CLI name while
# internal GHOSTEX_* environment names and storage paths remain implementation
# details. The macOS app bundle should ship executable `ghostex` and `gx`
# launchers automatically so Homebrew can install both public commands without
# asking users to add shell aliases by hand.
# CDXC:Cli 2026-06-07-13:53: The app CLI is not a web asset. Stage it under Contents/Resources/CLI so DMG and Homebrew installs can symlink public commands to one app-owned runtime while Web remains only the sidebar/runtime asset folder.
# CDXC:Cli 2026-07-13: the public CLI is the native Rust `ghostex`
# binary built with gxserver; the Node module + launcher scripts were deleted.
cp "$WEB_DIR/gxserver/bin/ghostex" "$CLI_DIR/ghostex"
ln -sfh "ghostex" "$CLI_DIR/gx"
chmod 755 "$CLI_DIR/ghostex"
stage_remote_gxserver_linux_packages_if_configured
if [[ "$GHOSTEX_ON_DEMAND_ASSETS" == "1" ]]; then
	stage_code_server_component_asset
	# Release bundles keep the self-contained runtime, including lib/node, only
	# in the verified component archive. The base app ships no Node runtime.
	rm -rf "$WEB_DIR/code-server"
	APP_CAPABILITY_SOURCE_EDITOR=false
	APP_CAPABILITY_SHARED_NODE_RUNTIME=false
	stage_on_demand_release_assets
else
	rm -f "$WEB_DIR/on-demand-resources.json"
fi
printf 'Prepared GPUI macOS runtime at %s\n' "$WEB_DIR"
