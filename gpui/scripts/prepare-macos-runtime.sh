#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIGURATION="${CONFIGURATION:-Debug}"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WEB_DIR="$REPO_ROOT/gpui/runtime/macos/Web"
CLI_DIR="$REPO_ROOT/gpui/runtime/macos/CLI"
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

# CDXC:AgentHistorySearch 2026-08-20: zehn is no longer a Zig submodule that this
# script builds and stages as Web/bin/zehn. Prompt-history search is a Rust crate
# compiled into gxserver, so there is nothing to build, cache, or copy here.
ZMX_ROOT="${ZMX_ROOT:-$REPO_ROOT/zmx}"
GXSERVER_RS_ROOT="${GXSERVER_RS_ROOT:-$REPO_ROOT/gxserver-rs}"
TUI_ROOT_EXPLICITLY_CONFIGURED=0
[[ -n "${TUI_ROOT:-}" ]] && TUI_ROOT_EXPLICITLY_CONFIGURED=1
# CDXC:GhostexTui 2026-07-01-02:10: The old `tui/` submodule is no longer the app launched by `gx tui`; build the promoted GX 2 source from `tui2/` into the canonical `ghostex-tui` binary so installed and remote launch contracts do not carry the transitional `ghostex-tui2` name.
TUI_ROOT="${TUI_ROOT:-$REPO_ROOT/tui2}"
CODE_SERVER_ROOT_EXPLICITLY_CONFIGURED=0
[[ -n "${CODE_SERVER_ROOT:-${GHOSTEX_CODE_SERVER_ROOT:-}}" ]] && CODE_SERVER_ROOT_EXPLICITLY_CONFIGURED=1
CODE_SERVER_ROOT="${CODE_SERVER_ROOT:-${GHOSTEX_CODE_SERVER_ROOT:-$REPO_ROOT/code-server}}"
CODE_SERVER_APP_NODE_VERSION="${CODE_SERVER_APP_NODE_VERSION:-}"
if [[ -z "$CODE_SERVER_APP_NODE_VERSION" && -f "$CODE_SERVER_ROOT/.node-version" ]]; then
	CODE_SERVER_APP_NODE_VERSION="$(tr -d '[:space:]' <"$CODE_SERVER_ROOT/.node-version")"
fi
CODE_SERVER_APP_NODE_VERSION="${CODE_SERVER_APP_NODE_VERSION:-22.22.1}"
CODE_SERVER_APP_NODE_MAJOR="${CODE_SERVER_APP_NODE_VERSION%%.*}"
CODE_SERVER_NODE_DOWNLOAD_BASE_URL="https://nodejs.org/dist/v$CODE_SERVER_APP_NODE_VERSION"
GHOSTEX_APP_VARIANT="${GHOSTEX_APP_VARIANT:-prod}"
case "$GHOSTEX_APP_VARIANT" in
	prod)
		;;
	dev)
		# CDXC:LocalStartSingleApp 2026-06-09-09:27: Ghostex-dev builds were removed because agents were invoking the dev app path by mistake. Fail before toolchain checks or Xcode generation so direct build commands cannot create Ghostex-dev outside `bun run start`.
		echo "Ghostex-dev builds were removed. Use GHOSTEX_APP_VARIANT=prod or unset it." >&2
		exit 1
		;;
	*)
		echo "Unsupported GHOSTEX_APP_VARIANT: $GHOSTEX_APP_VARIANT" >&2
		exit 1
		;;
esac

# CDXC:LocalStartArchitecture 2026-06-08-08:42: Apple Silicon local builds must produce Apple-native app resources even when the caller's shell is translated by Rosetta and `uname -m` reports x86_64. Use the physical arm64 capability as the default and keep GHOSTEX_MACOS_ARCH=x86_64 as the explicit Intel build path.
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
# CDXC:OnDemandAssets 2026-07-02-14:10: Release app bundles stop embedding the Ubuntu remote gxserver payloads and the macOS Beads binary. In this mode the build tars those payloads into build/on-demand-assets/<version>/, seals their checksums into Web/on-demand-resources.json inside the signed app, and ships Web/bin/bd as a download-on-first-use launcher. Dev builds keep bundling everything locally, so this stays a release-only mode.
GHOSTEX_ON_DEMAND_ASSETS="${GHOSTEX_ON_DEMAND_ASSETS:-0}"
case "$(printf '%s' "$GHOSTEX_ON_DEMAND_ASSETS" | tr '[:upper:]' '[:lower:]')" in
	1 | true | yes | on)
		GHOSTEX_ON_DEMAND_ASSETS=1
		;;
	*)
		GHOSTEX_ON_DEMAND_ASSETS=0
		;;
esac
# CDXC:ContributorStart 2026-06-22-23:23: `bun run start` should stay stable for full maintainer checkouts while allowing contributor clones that omit optional submodules. Enable missing-optional-submodule skips only for local starts by default; release and direct strict builds must keep failing when Source, TUI, or Zehn resources are absent. Beads is a checksum-pinned release artifact rather than a source submodule input.
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
APP_CAPABILITY_TUI=false
APP_CAPABILITY_BEADS=false
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
	# CDXC:LocalStartConcurrency 2026-06-11-18:59: Direct native builds mutate the same DerivedData app bundle that `bun run start` later mirrors into /Applications. Re-enter under the local-start lock unless the launcher already owns it, so a direct build cannot remove generated CEF payloads while another process installs the signed app.
	exec /usr/bin/lockf -k "$lock_file" /usr/bin/env GHOSTEX_BUILD_LOCK_HELD=1 /bin/bash "$0" "$@"
}

acquire_local_start_lock_if_needed "$@"

# CDXC:LocalStartFast 2026-06-07-16:23: Local starts should rebuild expensive bundled resources only when their runtime inputs change. Store content-hash stamps under build/<arch> so repeated `bun run start` calls do not churn source files or rely on generated folders that may be deleted by other build steps.
fingerprint_inputs() {
	"${GXSERVER_NODE_BIN:-node}" "$REPO_ROOT/scripts/fingerprint-build-inputs.mjs" "$@"
}

cache_stamp_path() {
	printf '%s/%s.sha256\n' "$BUILD_CACHE_DIR" "$1"
}

cache_matches() {
	local key="$1"
	local digest="$2"
	shift 2
	local stamp
	stamp="$(cache_stamp_path "$key")"
	if [[ ! -f "$stamp" || "$(<"$stamp")" != "$digest" ]]; then
		return 1
	fi
	local output_path
	for output_path in "$@"; do
		if [[ ! -e "$output_path" ]]; then
			return 1
		fi
	done
	return 0
}

write_cache_stamp() {
	local key="$1"
	local digest="$2"
	mkdir -p "$BUILD_CACHE_DIR"
	printf '%s\n' "$digest" >"$(cache_stamp_path "$key")"
}

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
	# CDXC:ReleaseBundleSize 2026-06-08-19:49: macOS DMGs are built per architecture, so bundled app resources must keep only the matching node-pty darwin prebuild. Prune Windows/Linux and opposite-arch prebuild directories from generated code-server payloads to reduce download size without changing runtime behavior.
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

write_gxserver_shared_bd_launcher() {
	local launcher_path="$1"
	mkdir -p "$(dirname "$launcher_path")"
	# CDXC:ReleaseBundleSize 2026-06-08-19:49: Ghostex already ships the arch-specific Beads CLI once at Web/bin/bd. Keep gxserver's historical bin/bd entry as a tiny launcher to that shared app resource so Project board commands keep working without bundling the large bd binary twice.
	cat >"$launcher_path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
APP_BD="$HERE/../../bin/bd"
exec "$APP_BD" "$@"
EOF
	chmod 755 "$launcher_path"
}

path_identity() {
	local candidate="$1"
	if [[ -e "$candidate" ]]; then
		stat -f '%m:%z:%N' "$candidate"
	else
		printf 'missing:%s\n' "$candidate"
	fi
}

code_server_node_distribution_arch() {
	case "$GHOSTEX_MACOS_ARCH" in
		arm64)
			printf 'arm64\n'
			;;
		x86_64)
			printf 'x64\n'
			;;
	esac
}

code_server_node_distribution_sha256() {
	local distribution_arch="$1"
	if [[ "$CODE_SERVER_APP_NODE_VERSION" == "22.22.1" ]]; then
		case "$distribution_arch" in
			arm64)
				printf '261da057fb25ff2912dd6abb7842fc915ddf7947a2cb3c8cce90875d2b9bb667\n'
				return 0
				;;
			x64)
				printf '91227fa5a3bfd988be1953c0384ceb98bd69a6a377a7416c40eb39779d6ab17f\n'
				return 0
				;;
		esac
	fi
	echo "Unsupported code-server Node distribution: v$CODE_SERVER_APP_NODE_VERSION darwin-$distribution_arch" >&2
	echo "Update code_server_node_distribution_sha256 before changing code-server/.node-version." >&2
	return 1
}

verify_sha256_file() {
	local file_path="$1"
	local expected_sha256="$2"
	local actual_sha256
	actual_sha256="$(shasum -a 256 "$file_path" | awk '{print $1}')"
	[[ "$actual_sha256" == "$expected_sha256" ]]
}

prepare_code_server_app_node_runtime() {
	local distribution_arch package_name cache_root extract_root tarball_path expected_sha256 node_bin
	distribution_arch="$(code_server_node_distribution_arch)"
	package_name="node-v$CODE_SERVER_APP_NODE_VERSION-darwin-$distribution_arch"
	cache_root="$BUILD_CACHE_DIR/code-server-node-runtime"
	extract_root="$cache_root/$package_name"
	tarball_path="$cache_root/$package_name.tar.xz"
	expected_sha256="$(code_server_node_distribution_sha256 "$distribution_arch")"
	node_bin="$extract_root/bin/node"

	# CDXC:CodeServerRuntime 2026-06-08-12:17: code-server owns Ghostex's app-bundled Node runtime. Cache the official per-architecture Node 22 distribution for build-time npm/node-gyp work, then stage the executable inside Web/code-server/lib/node so gxserver and code-server share one bundled Node instead of shipping duplicate runtimes.
	if [[ -x "$node_bin" ]] &&
		"$node_bin" -e "process.exit(process.versions.node === '$CODE_SERVER_APP_NODE_VERSION' ? 0 : 1)" >/dev/null 2>&1 &&
		binary_supports_macos_arch "$node_bin" "$GHOSTEX_MACOS_ARCH"; then
		printf '%s\n' "$node_bin"
		return 0
	fi

	mkdir -p "$cache_root"
	if [[ ! -f "$tarball_path" ]] || ! verify_sha256_file "$tarball_path" "$expected_sha256"; then
		echo "Downloading Node $CODE_SERVER_APP_NODE_VERSION for $GHOSTEX_MACOS_ARCH code-server runtime..." >&2
		curl -fsSL "$CODE_SERVER_NODE_DOWNLOAD_BASE_URL/$package_name.tar.xz" -o "$tarball_path"
	fi
	if ! verify_sha256_file "$tarball_path" "$expected_sha256"; then
		echo "Downloaded Node runtime checksum mismatch: $tarball_path" >&2
		exit 1
	fi

	rm -rf "$extract_root"
	tar -xJf "$tarball_path" -C "$cache_root"
	if [[ ! -x "$node_bin" ]]; then
		echo "Extracted Node runtime is missing executable: $node_bin" >&2
		exit 1
	fi
	if ! binary_supports_macos_arch "$node_bin" "$GHOSTEX_MACOS_ARCH"; then
		echo "Extracted Node runtime does not contain $GHOSTEX_MACOS_ARCH: $node_bin" >&2
		exit 1
	fi
	printf '%s\n' "$node_bin"
}

resolve_code_server_root() {
	local configured="${CODE_SERVER_ROOT:-${GHOSTEX_CODE_SERVER_ROOT:-}}"
	if [[ -n "$configured" ]]; then
		if [[ -f "$configured/package.json" ]]; then
			(cd "$configured" && pwd)
			return 0
		fi
		return 1
	fi
	if [[ -f "$REPO_ROOT/code-server/package.json" ]]; then
		(cd "$REPO_ROOT/code-server" && pwd)
		return 0
	fi
	return 1
}

code_server_ci_arch() {
	case "$GHOSTEX_MACOS_ARCH" in
		arm64)
			printf 'arm64\n'
			;;
		x86_64)
			printf 'amd64\n'
			;;
	esac
}

code_server_vscode_target() {
	case "$GHOSTEX_MACOS_ARCH" in
		arm64)
			printf 'darwin-arm64\n'
			;;
		x86_64)
			printf 'darwin-x64\n'
			;;
	esac
}

code_server_vscode_ripgrep_bin() {
	local vscode_root="$1"
	printf '%s/node_modules/@vscode/ripgrep/bin/rg\n' "$vscode_root"
}

code_server_vscode_payload_digest() {
	local vscode_target="$1"
	local node_identity="$2"
	local npm_version="$3"
	local package_version="$4"
	local commit="$5"
	fingerprint_inputs \
		--value "code-server-vscode-payload-v1" \
		--value "arch=$GHOSTEX_MACOS_ARCH" \
		--value "target=$vscode_target" \
		--value "node=$node_identity" \
		--value "npm=$npm_version" \
		--value "version=$package_version" \
		--value "commit=$commit" \
		--path "$CODE_SERVER_ROOT/ci/build/build-vscode.sh" \
		--path "$CODE_SERVER_ROOT/patches" \
		--path "$CODE_SERVER_ROOT/package.json" \
		--path "$CODE_SERVER_ROOT/package-lock.json" \
		--path "$CODE_SERVER_ROOT/.node-version" \
		--path "$CODE_SERVER_ROOT/lib/vscode/package.json" \
		--path "$CODE_SERVER_ROOT/lib/vscode/package-lock.json" \
		--path "$CODE_SERVER_ROOT/lib/vscode/product.json" \
		--path "$CODE_SERVER_ROOT/lib/vscode/build/gulpfile.reh.ts" \
		--path "$CODE_SERVER_ROOT/lib/vscode/build/lib/copilot.ts" \
		--path "$CODE_SERVER_ROOT/lib/vscode/remote/package.json" \
		--path "$CODE_SERVER_ROOT/lib/vscode/remote/package-lock.json"
}

code_server_release_version() {
	"$CODE_SERVER_NODE_BIN" -e "const fs=require('fs'); const pkg=JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); process.stdout.write(String(pkg.version || '0.0.0'));" "$CODE_SERVER_ROOT/package.json"
}

code_server_node_payload_digest() {
	fingerprint_inputs \
		--value "code-server-node-payload-v1" \
		--path "$CODE_SERVER_ROOT/ci/build/build-code-server.sh" \
		--path "$CODE_SERVER_ROOT/src/common" \
		--path "$CODE_SERVER_ROOT/src/node" \
		--path "$CODE_SERVER_ROOT/typings" \
		--path "$CODE_SERVER_ROOT/package.json" \
		--path "$CODE_SERVER_ROOT/package-lock.json" \
		--path "$CODE_SERVER_ROOT/.node-version" \
		--path "$CODE_SERVER_ROOT/tsconfig.json"
}

ensure_code_server_payload() {
	local vscode_target="$1"
	local vscode_release_root="$CODE_SERVER_ROOT/lib/vscode-reh-web-$vscode_target"
	local vscode_ripgrep_bin payload_digest payload_cache_key node_identity npm_version package_version commit node_payload_digest
	if [[ ! -f "$CODE_SERVER_ROOT/package.json" ]]; then
		echo "code-server source is missing: $CODE_SERVER_ROOT" >&2
		echo "Initialize the code-server submodule before building Ghostex." >&2
		exit 1
	fi
	if [[ ! -d "$CODE_SERVER_ROOT/node_modules" ]]; then
		echo "code-server node_modules are missing. Run: npm --prefix code-server install" >&2
		exit 1
	fi
	node_payload_digest="$(code_server_node_payload_digest)"
	if ! cache_matches "code-server-node-payload" "$node_payload_digest" "$CODE_SERVER_ROOT/out/node/entry.js"; then
		(
			cd "$CODE_SERVER_ROOT"
			env PATH="$CODE_SERVER_NODE_DIR:$PATH" "$CODE_SERVER_NPM_BIN" run build
		)
		write_cache_stamp "code-server-node-payload" "$node_payload_digest"
	fi
	if [[ ! -f "$CODE_SERVER_ROOT/lib/vscode/package.json" ]]; then
		echo "code-server VS Code submodule is missing. Run: git -C code-server submodule update --init lib/vscode" >&2
		exit 1
	fi
	if [[ ! -d "$CODE_SERVER_ROOT/lib/vscode/node_modules" ]]; then
		echo "code-server VS Code node_modules are missing. Run: npm --prefix code-server/lib/vscode install" >&2
		exit 1
	fi
	vscode_ripgrep_bin="$(code_server_vscode_ripgrep_bin "$vscode_release_root")"
	node_identity="$("$CODE_SERVER_NODE_BIN" -p 'process.version + ":" + process.versions.modules')"
	npm_version="$("$CODE_SERVER_NPM_BIN" --version 2>/dev/null || true)"
	package_version="$(code_server_release_version)"
	commit="$(git -C "$CODE_SERVER_ROOT" rev-parse HEAD 2>/dev/null || printf 'development')"
	payload_digest="$(code_server_vscode_payload_digest "$vscode_target" "$node_identity" "$npm_version" "$package_version" "$commit")"
	payload_cache_key="code-server-vscode-payload-$GHOSTEX_MACOS_ARCH"
	# CDXC:CodeServerRuntime 2026-06-09-17:06: Embedded VS Code search depends on @vscode/ripgrep/bin/rg. Rebuild the generated REH web payload when code-server packaging inputs change, server-main.js is missing, or ripgrep is missing/wrong-arch so `bun run start` and release builds cannot reuse a stale payload that opens but fails search.
	if ! cache_matches "$payload_cache_key" "$payload_digest" "$vscode_release_root/out/server-main.js" "$vscode_ripgrep_bin" ||
		! binary_supports_macos_arch "$vscode_ripgrep_bin" "$GHOSTEX_MACOS_ARCH"; then
		(
			cd "$CODE_SERVER_ROOT"
			env \
				PATH="$CODE_SERVER_NODE_DIR:$PATH" \
				OS=macos \
				ARCH="$(code_server_ci_arch)" \
				VSCODE_TARGET="$vscode_target" \
				VERSION="$(code_server_release_version)" \
				"$CODE_SERVER_NPM_BIN" run build:vscode
		)
	fi
	if [[ ! -f "$vscode_release_root/out/server-main.js" ]]; then
		echo "code-server VS Code release payload is missing: $vscode_release_root/out/server-main.js" >&2
		exit 1
	fi
	if [[ ! -f "$vscode_ripgrep_bin" ]]; then
		echo "code-server VS Code release payload is missing ripgrep: $vscode_ripgrep_bin" >&2
		exit 1
	fi
	if ! binary_supports_macos_arch "$vscode_ripgrep_bin" "$GHOSTEX_MACOS_ARCH"; then
		echo "code-server VS Code ripgrep binary does not contain $GHOSTEX_MACOS_ARCH: $vscode_ripgrep_bin" >&2
		exit 1
	fi
	write_cache_stamp "$payload_cache_key" "$payload_digest"
}

package_code_server_if_needed() {
	local target_dir="$WEB_DIR/code-server"
	local vscode_target package_digest node_identity npm_version vscode_release_root commit package_version expected_node_pty_prebuild
	vscode_target="$(code_server_vscode_target)"
	ensure_code_server_payload "$vscode_target"
	vscode_release_root="$CODE_SERVER_ROOT/lib/vscode-reh-web-$vscode_target"
	expected_node_pty_prebuild="$target_dir/lib/vscode/node_modules/node-pty/prebuilds/$(node_pty_prebuild_platform_dir)/pty.node"
	node_identity="$("$CODE_SERVER_NODE_BIN" -p 'process.version + ":" + process.versions.modules')"
	npm_version="$("$CODE_SERVER_NPM_BIN" --version 2>/dev/null || true)"
	package_version="$(code_server_release_version)"
	commit="$(git -C "$CODE_SERVER_ROOT" rev-parse HEAD 2>/dev/null || printf 'development')"
	package_digest="$(fingerprint_inputs \
		--value "code-server-package-v3" \
		--value "arch=$GHOSTEX_MACOS_ARCH" \
		--value "target=$vscode_target" \
		--value "node=$node_identity" \
		--value "npm=$npm_version" \
		--value "commit=$commit" \
		--value "entry=$(path_identity "$CODE_SERVER_ROOT/out/node/entry.js")" \
		--value "vscode=$(path_identity "$vscode_release_root/out/server-main.js")" \
		--value "ripgrep=$(path_identity "$(code_server_vscode_ripgrep_bin "$vscode_release_root")")" \
		--path "$CODE_SERVER_ROOT/ci/build/build-vscode.sh" \
		--path "$CODE_SERVER_ROOT/patches" \
		--path "$CODE_SERVER_ROOT/package.json" \
		--path "$CODE_SERVER_ROOT/package-lock.json" \
		--path "$CODE_SERVER_ROOT/.node-version" \
		--path "$CODE_SERVER_ROOT/src/browser")"
	# CDXC:CodeServerRuntime 2026-06-08-12:17: The app bundle must contain a self-contained code-server runtime at Web/code-server and the single shared Node executable at Web/code-server/lib/node. Missing code-server resources are build failures instead of installed-user Node prompts.
	if cache_matches "code-server-package-$GHOSTEX_MACOS_ARCH" "$package_digest" "$target_dir/out/node/entry.js" "$target_dir/lib/vscode/out/server-main.js" "$target_dir/lib/vscode/node_modules/@vscode/ripgrep/bin/rg" "$target_dir/lib/node" "$target_dir/node_modules" "$expected_node_pty_prebuild" &&
		node_pty_prebuilds_match_arch "$target_dir" &&
		binary_supports_macos_arch "$target_dir/lib/node" "$GHOSTEX_MACOS_ARCH" &&
		binary_supports_macos_arch "$target_dir/lib/vscode/node_modules/@vscode/ripgrep/bin/rg" "$GHOSTEX_MACOS_ARCH"; then
		# CDXC:CodeServerRuntime 2026-06-08-16:23: Web/code-server is a shared staging directory reused by arm64 and x86_64 release passes. A per-arch cache stamp is only valid when the staged Node executable still contains the requested CPU slice; otherwise restage the package so app validation uses the matching runtime.
		echo "code-server package is current; skipping package rebuild."
		return 0
	fi

	rm -rf "$target_dir"
	mkdir -p "$target_dir"
	rsync -a --delete "$CODE_SERVER_ROOT/out/" "$target_dir/out/"
	mkdir -p "$target_dir/src/browser"
	if [[ -d "$CODE_SERVER_ROOT/src/browser/media" ]]; then
		rsync -a --delete "$CODE_SERVER_ROOT/src/browser/media/" "$target_dir/src/browser/media/"
	fi
	if [[ -d "$CODE_SERVER_ROOT/src/browser/pages" ]]; then
		rsync -a --delete "$CODE_SERVER_ROOT/src/browser/pages/" "$target_dir/src/browser/pages/"
	fi
	for browser_asset in robots.txt security.txt; do
		if [[ -f "$CODE_SERVER_ROOT/src/browser/$browser_asset" ]]; then
			cp "$CODE_SERVER_ROOT/src/browser/$browser_asset" "$target_dir/src/browser/$browser_asset"
		fi
	done
	"$CODE_SERVER_NODE_BIN" -e "const fs=require('fs'); const src=JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); delete src.scripts; delete src.jest; delete src.devDependencies; src.version=process.argv[3]; src.commit=process.argv[4]; fs.writeFileSync(process.argv[2], JSON.stringify(src, null, 2) + '\n');" "$CODE_SERVER_ROOT/package.json" "$target_dir/package.json" "$package_version" "$commit"
	cp "$CODE_SERVER_ROOT/package-lock.json" "$target_dir/package-lock.json"
	if [[ -f "$CODE_SERVER_ROOT/.node-version" ]]; then
		cp "$CODE_SERVER_ROOT/.node-version" "$target_dir/.node-version"
	fi
	for root_asset in LICENSE README.md ThirdPartyNotices.txt; do
		if [[ -f "$CODE_SERVER_ROOT/$root_asset" ]]; then
			cp "$CODE_SERVER_ROOT/$root_asset" "$target_dir/$root_asset"
		fi
	done
	mkdir -p "$target_dir/bin"
	cp "$CODE_SERVER_ROOT/ci/build/code-server.sh" "$target_dir/bin/code-server"
	chmod 755 "$target_dir/bin/code-server"
	rsync -a --delete \
		--exclude '.cache/' \
		--exclude '.bin/' \
		"$CODE_SERVER_ROOT/node_modules/" "$target_dir/node_modules/"
	(
		cd "$target_dir"
		env PATH="$CODE_SERVER_NODE_DIR:$PATH" "$CODE_SERVER_NPM_BIN" prune --omit=dev --ignore-scripts --no-audit --no-fund
	)
	mkdir -p "$target_dir/lib"
	rsync -a --delete --exclude '/node' "$vscode_release_root/" "$target_dir/lib/vscode/"
	normalize_node_pty_prebuilds "$target_dir"
	prune_node_pty_prebuilds "$target_dir"
	cp "$CODE_SERVER_NODE_BIN" "$target_dir/lib/node"
	chmod 755 "$target_dir/lib/node"
	"$target_dir/lib/node" "$target_dir/out/node/entry.js" --version >/dev/null
	write_cache_stamp "code-server-package-$GHOSTEX_MACOS_ARCH" "$package_digest"
}

stage_shared_code_server_node_runtime() {
	local target_node="$WEB_DIR/code-server/lib/node"
	# CDXC:ContributorStart 2026-06-22-23:23: Optional Source panes must not remove the shared app-owned Node runtime. Native sidebar helpers and Portless still resolve Web/code-server/lib/node, so contributor builds without the code-server submodule stage only that executable and leave Source-specific files absent.
	if [[ "$APP_CAPABILITY_SOURCE_EDITOR" != "true" ]]; then
		rm -rf "$WEB_DIR/code-server"
	fi
	if [[ -x "$target_node" ]] && binary_supports_macos_arch "$target_node" "$GHOSTEX_MACOS_ARCH"; then
		APP_CAPABILITY_SHARED_NODE_RUNTIME=true
		return 0
	fi
	mkdir -p "$(dirname "$target_node")"
	cp "$CODE_SERVER_NODE_BIN" "$target_node"
	chmod 755 "$target_node"
	APP_CAPABILITY_SHARED_NODE_RUNTIME=true
}

code_server_component_version() {
	local resolved_version
	resolved_version="$(node "$REPO_ROOT/scripts/release-gpui/code-server-component-identity.mjs" --root "$CODE_SERVER_ROOT")"
	if [[ -n "${GHOSTEX_CODE_SERVER_COMPONENT_VERSION:-}" && "$GHOSTEX_CODE_SERVER_COMPONENT_VERSION" != "$resolved_version" ]]; then
		echo "Configured code-server component version does not match its Node payload identity: $GHOSTEX_CODE_SERVER_COMPONENT_VERSION != $resolved_version" >&2
		exit 1
	fi
	printf '%s\n' "$resolved_version"
}

published_code_server_component_asset() {
	local component_version component_tag asset_name sidecar_name published_asset_names
	component_version="$(code_server_component_version)"
	component_tag="code-server-$component_version"
	asset_name="code-server-$component_version-darwin-arm64.tar.gz"
	sidecar_name="$asset_name.sha256"
	if ! command -v gh >/dev/null 2>&1; then
		return 1
	fi
	published_asset_names="$(gh release view "$component_tag" --repo maddada/Ghostex --json assets --jq '.assets[].name' 2>/dev/null || true)"
	printf '%s\n' "$published_asset_names" | grep -Fxq "$asset_name" &&
		printf '%s\n' "$published_asset_names" | grep -Fxq "$sidecar_name"
}

stage_code_server_component_asset() {
	local component_version component_tag asset_dir asset_path asset_sidecar asset_sha256 component_manifest stage_root
	local reused_published_component=0
	component_version="$(code_server_component_version)"
	component_tag="code-server-$component_version"
	asset_dir="${GHOSTEX_ON_DEMAND_COMPONENT_ASSET_DIR:-$REPO_ROOT/build/on-demand-components/assets}"
	component_manifest="${GHOSTEX_ON_DEMAND_COMPONENTS_MANIFEST:-$REPO_ROOT/build/on-demand-components/components.json}"
	asset_path="$asset_dir/code-server-$component_version-darwin-arm64.tar.gz"
	asset_sidecar="$asset_path.sha256"
	if [[ "$GHOSTEX_MACOS_ARCH" != "arm64" ]]; then
		echo "On-demand code-server component packaging currently supports macOS arm64 only." >&2
		exit 1
	fi
	mkdir -p "$asset_dir"
	local linux_arch linux_archive linux_asset expected_linux_asset_name
	for linux_arch in x64 arm64; do
		if [[ "$linux_arch" == "x64" ]]; then
			linux_archive="${GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_X64_ARCHIVE:-}"
		else
			linux_archive="${GHOSTEX_ON_DEMAND_CODE_SERVER_LINUX_ARM64_ARCHIVE:-}"
		fi
		[[ -n "$linux_archive" ]] || {
			echo "macOS release preparation requires the Linux $linux_arch code-server component archive." >&2
			exit 1
		}
		[[ -f "$linux_archive" ]] || {
			echo "Linux code-server component archive is missing: $linux_archive" >&2
			exit 1
		}
		expected_linux_asset_name="code-server-$component_version-linux-$linux_arch.tar.gz"
		[[ "$(basename "$linux_archive")" == "$expected_linux_asset_name" ]] || {
			echo "Linux code-server component archive identity mismatch: expected $expected_linux_asset_name, got $(basename "$linux_archive")" >&2
			exit 1
		}
		node "$REPO_ROOT/scripts/release-gpui/verify-code-server-archive.mjs" \
			--archive "$linux_archive" \
			--version "$component_version" \
			--platform "linux-$linux_arch"
		linux_asset="$asset_dir/$expected_linux_asset_name"
		cp "$linux_archive" "$linux_asset"
		cp "$linux_archive.sha256" "$linux_asset.sha256"
	done
	if published_code_server_component_asset; then
		reused_published_component=1
		gh release download "$component_tag" \
			--repo maddada/Ghostex \
			--pattern "$(basename "$asset_path")" \
			--pattern "$(basename "$asset_sidecar")" \
			--dir "$asset_dir" \
			--clobber
	else
		for required_path in \
			"$WEB_DIR/code-server/out/node/entry.js" \
			"$WEB_DIR/code-server/out/node/routes/health.js" \
			"$WEB_DIR/code-server/lib/vscode/out/server-main.js" \
			"$WEB_DIR/code-server/lib/node"; do
			[[ -e "$required_path" ]] || {
				echo "Code-server component payload is missing $required_path" >&2
				exit 1
			}
		done
		/usr/bin/grep -Fq promptEditorIpcReady "$WEB_DIR/code-server/out/node/routes/health.js" || {
			echo "Code-server component payload lacks prompt-editor IPC readiness." >&2
			exit 1
		}
		stage_root="$(mktemp -d "$BUILD_CACHE_DIR/code-server-component-XXXXXX")"
		rsync -a --delete "$WEB_DIR/code-server/" "$stage_root/"
		"$REPO_ROOT/scripts/release-gpui/create-deterministic-tar.sh" "$stage_root" "$asset_path"
		rm -rf "$stage_root"
		asset_sha256="$(shasum -a 256 "$asset_path" | awk '{print $1}')"
		printf '%s  %s\n' "$asset_sha256" "$(basename "$asset_path")" >"$asset_sidecar"
	fi
	node "$REPO_ROOT/scripts/release-gpui/verify-code-server-archive.mjs" \
		--archive "$asset_path" \
		--version "$component_version" \
		--platform darwin-arm64
	if [[ "$reused_published_component" == "1" ]]; then
		echo "Reused verified published code-server component $component_tag."
	fi
	node "$REPO_ROOT/scripts/release-gpui/publish-component.mjs" \
		--metadata-only \
		--component code-server \
		--version "$component_version" \
		--asset-dir "$asset_dir" \
		--require-platforms darwin-arm64,linux-x64,linux-arm64 \
		--require-sha256-sidecars \
		--output "$component_manifest"
	echo "Prepared code-server component $component_version: $asset_path"
}

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

	# CDXC:PortlessPackaging 2026-06-22-22:26: Ghostex packages the published portless@0.14.0 CLI as Web/portless and runs it with the shared Web/code-server/lib/node runtime. Do not stage a second Node runtime; fail packaging if the installed package does not contain dist/cli.js.
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

macos_sdk_needs_infinity_fix() {
	local sdk="$1"
	[[ -f "$sdk/usr/include/math.h" ]] || return 1
	grep -q '__need_infinity_nan' "$sdk/usr/include/math.h" \
		&& ! grep -q 'Ghostex INFINITY fallback' "$sdk/usr/include/math.h"
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
 * Zig 0.15's bundled clang). Harmless when already defined. */
#ifndef INFINITY
#define INFINITY    HUGE_VALF
#endif
#ifndef NAN
#define NAN         __builtin_nanf("0x7fc00000")
#endif
MATH_EOF
	} > "$overlay_sdk/usr/include/math.h"
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
		# CDXC:LocalStartArchitecture 2026-06-08-08:42: zmx writes every macOS target to zmx/zig-out/bin/zmx, so an old per-arch cache stamp is not enough to prove the shared output still contains the requested CPU slice. Verify the Mach-O architecture before skipping or Ghostex can launch Intel zmx from an arm64 app.
		if binary_supports_macos_arch "$output_path" "$GHOSTEX_MACOS_ARCH"; then
			echo "zmx is current; skipping Zig build."
			return 0
		fi
		echo "zmx cache is stale for $GHOSTEX_MACOS_ARCH; rebuilding Zig artifact."
	fi

	(
		cd "$ZMX_ROOT"
		# CDXC:ZmxPersistence 2026-05-20-10:23: Zig 0.15.2 currently resolves the native build runner through the selected macOS 26 Xcode SDK on this machine, which can fail before zmx compilation starts. Scope the Command Line Tools developer dir to the zmx submodule build only; the zmx artifact itself is still built for the explicit deployment target above.
		ZMX_BUILD_ENV=(env -u LDFLAGS ZIG="$ZIG_BIN")
		if [[ -z "${ZMX_BUILD_DEVELOPER_DIR:-}" ]] \
			&& DEVELOPER_DIR=/Library/Developer/CommandLineTools /usr/bin/xcrun --sdk macosx --show-sdk-path >/dev/null 2>&1; then
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
			if [[ ! -f "$overlay_sdk/usr/include/math.h" ]] \
				|| [[ "$zmx_sdk/usr/include/math.h" -nt "$overlay_sdk/usr/include/math.h" ]]; then
				synthesize_macos_sdk_overlay "$zmx_sdk" "$overlay_sdk"
			fi
			shim_dir="$(mktemp -d "${TMPDIR:-/tmp}/ghostex-zmx-xcrun.XXXXXX")"
			trap 'rm -rf "$shim_dir"' EXIT
			cat > "$shim_dir/xcrun" <<XCRUN_EOF
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

build_tui_if_needed() {
	local output_path="$TUI_ROOT/target/$TUI_CARGO_TARGET/release/ghostex-tui"
	local cargo_version build_digest
	cargo_version="$("$TUI_CARGO_BIN" --version 2>/dev/null || true)"
	build_digest="$(fingerprint_inputs \
		--value "ghostex-tui-promoted-tui2-build-v1" \
		--value "target=$TUI_CARGO_TARGET" \
		--value "cargo=$cargo_version" \
		--value "zig=$ZIG_VERSION" \
		--path "$TUI_ROOT/src" \
		--path "$TUI_ROOT/Cargo.toml" \
		--path "$TUI_ROOT/Cargo.lock")"
	if cache_matches "ghostex-tui-$GHOSTEX_MACOS_ARCH" "$build_digest" "$output_path"; then
		echo "ghostex-tui is current; skipping Cargo build."
		return 0
	fi

	(
		TUI_BUILD_ENV=(env ZIG="$ZIG_BIN")
		tui_sdk="$(/usr/bin/xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)"
		if [[ -n "$tui_sdk" ]] && macos_sdk_needs_infinity_fix "$tui_sdk"; then
			overlay_sdk="$TUI_ROOT/.zig-cache/ghostex-sdk-overlay/$(basename "$tui_sdk")"
			if [[ ! -f "$overlay_sdk/usr/include/math.h" ]] \
				|| [[ "$tui_sdk/usr/include/math.h" -nt "$overlay_sdk/usr/include/math.h" ]]; then
				synthesize_macos_sdk_overlay "$tui_sdk" "$overlay_sdk"
			fi
			shim_dir="$(mktemp -d "${TMPDIR:-/tmp}/ghostex-tui-xcrun.XXXXXX")"
			trap 'rm -rf "$shim_dir"' EXIT
			cat > "$shim_dir/xcrun" <<XCRUN_EOF
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
			TUI_BUILD_ENV+=(PATH="$shim_dir:$PATH")
			echo "ghostex-tui build: using INFINITY-patched SDK overlay at $overlay_sdk"
		fi
		"${TUI_BUILD_ENV[@]}" "$TUI_CARGO_BIN" build --release --bin ghostex-tui --manifest-path "$TUI_ROOT/Cargo.toml" --target "$TUI_CARGO_TARGET"
	)
	write_cache_stamp "ghostex-tui-$GHOSTEX_MACOS_ARCH" "$build_digest"
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

build_gxserver_rust_if_needed() {
	local cargo_bin cargo_target output_path cli_output_path cargo_version build_digest
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
	cargo_version="$("$cargo_bin" --version 2>/dev/null || true)"
	build_digest="$(fingerprint_inputs \
		--value "gxserver-rs-build-v2" \
		--value "target=$cargo_target" \
		--value "cargo=$cargo_version" \
		--path "$GXSERVER_RS_ROOT/src" \
		--path "$GXSERVER_RS_ROOT/Cargo.toml" \
		--path "$GXSERVER_RS_ROOT/Cargo.lock")"
	if cache_matches "gxserver-rs-$GHOSTEX_MACOS_ARCH" "$build_digest" "$output_path" "$cli_output_path" &&
		binary_supports_macos_arch "$output_path" "$GHOSTEX_MACOS_ARCH" &&
		binary_supports_macos_arch "$cli_output_path" "$GHOSTEX_MACOS_ARCH"; then
		echo "Rust gxserver is current; skipping Cargo build." >&2
		GXSERVER_RUST_BIN="$output_path"
		return 0
	fi

	# CDXC:GxserverRustBuild 2026-06-24-20:22: Local start must fail before packaging when gxserver-rs no longer compiles. This function is called outside command substitution so `set -e` can abort on Cargo errors instead of stamping the current source digest and copying a stale daemon binary.
	"$cargo_bin" build --release --bins --manifest-path "$GXSERVER_RS_ROOT/Cargo.toml" --target "$cargo_target"
	if ! binary_supports_macos_arch "$output_path" "$GHOSTEX_MACOS_ARCH"; then
		echo "Rust gxserver binary does not contain $GHOSTEX_MACOS_ARCH: $output_path" >&2
		exit 1
	fi
	if ! binary_supports_macos_arch "$cli_output_path" "$GHOSTEX_MACOS_ARCH"; then
		echo "Rust ghostex CLI binary does not contain $GHOSTEX_MACOS_ARCH: $cli_output_path" >&2
		exit 1
	fi
	write_cache_stamp "gxserver-rs-$GHOSTEX_MACOS_ARCH" "$build_digest"
	GXSERVER_RUST_BIN="$output_path"
}

stage_beads_release_if_needed() {
	local output_path="$REPO_ROOT/build/$GHOSTEX_MACOS_ARCH/beads/bd"
	local release_arch build_digest
	case "$GHOSTEX_MACOS_ARCH" in
		arm64)
			release_arch="arm64"
			;;
		x86_64)
			echo "The pinned schema-v54 Beads artifact is published for macOS arm64 only; x86_64 packaging is unsupported." >&2
			exit 1
			;;
	esac
	if [[ -n "${GHOSTEX_BEADS_PREBUILT_BINARY:-}" ]]; then
		if [[ ! -x "$GHOSTEX_BEADS_PREBUILT_BINARY" ]]; then
			echo "Pinned native-CGO Beads binary is missing or not executable: $GHOSTEX_BEADS_PREBUILT_BINARY" >&2
			exit 1
		fi
		mkdir -p "$(dirname "$output_path")"
		cp "$GHOSTEX_BEADS_PREBUILT_BINARY" "$output_path"
		chmod 755 "$output_path"
		if ! binary_supports_macos_arch "$output_path" "$GHOSTEX_MACOS_ARCH"; then
			echo "Pinned native-CGO Beads binary does not contain the required $GHOSTEX_MACOS_ARCH Mach-O slice: $output_path" >&2
			exit 1
		fi
		return 0
	fi
	build_digest="$(fingerprint_inputs \
		--value "beads-schema54-672d942083a1-v1" \
		--value "target=darwin/$release_arch" \
		--path "$REPO_ROOT/scripts/beads-release.mjs" \
		--path "$REPO_ROOT/scripts/smoke-test-packaged-beads.mjs")"
	if cache_matches "beads-$GHOSTEX_MACOS_ARCH" "$build_digest" "$output_path"; then
		if binary_supports_macos_arch "$output_path" "$GHOSTEX_MACOS_ARCH" && \
			"$output_path" version 2>/dev/null | grep -Eq '^bd version 1\.1\.0 .*672d942'; then
			echo "Pinned schema-v54 Beads artifact is current; skipping download."
			return 0
		fi
		echo "Beads cache is stale for $GHOSTEX_MACOS_ARCH; restaging the schema-v54 artifact."
	fi

	mkdir -p "$(dirname "$output_path")"
	node "$REPO_ROOT/scripts/beads-release.mjs" \
		--platform darwin \
		--arch "$release_arch" \
		--output "$output_path"
	if ! binary_supports_macos_arch "$output_path" "$GHOSTEX_MACOS_ARCH"; then
		echo "Pinned Beads artifact does not contain the required $GHOSTEX_MACOS_ARCH Mach-O slice: $output_path" >&2
		exit 1
	fi
	write_cache_stamp "beads-$GHOSTEX_MACOS_ARCH" "$build_digest"
}

smoke_test_staged_beads() {
	local binary_path="$1"
	local host_arch
	host_arch="$(uname -m)"
	if [[ "$host_arch" == "$GHOSTEX_MACOS_ARCH" ]]; then
		node "$REPO_ROOT/scripts/smoke-test-packaged-beads.mjs" "$binary_path"
	elif [[ "${GHOSTEX_REQUIRE_BEADS_SMOKE:-0}" == "1" ]]; then
		echo "GHOSTEX_REQUIRE_BEADS_SMOKE=1 requires a native $GHOSTEX_MACOS_ARCH macOS runner; current host is $host_arch." >&2
		exit 1
	else
		echo "Skipping execution smoke for cross-architecture Beads artifact $GHOSTEX_MACOS_ARCH on $host_arch; checksum and Mach-O shape were verified."
	fi
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
	for binary_path in \
		"$WEB_DIR/bin/bd"; do
		if [[ -e "$binary_path" ]] && ! binary_supports_macos_arch "$binary_path" "$GHOSTEX_MACOS_ARCH"; then
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
	if [[ ! -f "$REPO_ROOT/shared/gxserver-protocol.ts" ]]; then
		echo "shared gxserver protocol source is missing: $REPO_ROOT/shared/gxserver-protocol.ts" >&2
		exit 1
	fi
	if [[ ! -f "$tsc_bin" ]]; then
		echo "TypeScript compiler is missing at $tsc_bin. Run bun install before packaging gxserver." >&2
		exit 1
	fi
	rm -rf "$protocol_stage_dir"
	mkdir -p "$protocol_stage_dir/src" "$protocol_stage_dir/types" "$target_dir/dist/protocol"
	cp "$REPO_ROOT/shared/gxserver-protocol.ts" "$protocol_stage_dir/src/index.ts"
	# CDXC:GxserverProtocolStaging 2026-08-21-12:10: shared/gxserver-protocol.ts pulls in
	# sibling shared modules (session-chat.ts, which now pulls session-chat-queue.ts).
	# Stage the whole relative-import closure instead of a hand-kept file list so adding a
	# shared module never breaks packaging with a TS2307 "cannot find module" failure.
	GXSERVER_PROTOCOL_SHARED_DIR="$REPO_ROOT/shared" \
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

The package includes Ghostex's pinned zmx artifact plus the checksum-verified schema-v54 Beads `bd` artifact in `bin/`. Project board operations require the bundled `bd`; shell-installed `bd` is intentionally ignored so Ghostex and agent workflows share one pinned Beads binary.
EOF
}

write_gxserver_package_build_identity() {
	local target_dir="$1"
	local package_version="$2"
	local beads_version
	beads_version="$("$WEB_DIR/bin/bd" version 2>/dev/null | sed -n 's/^bd version \([^ ]*\).*/\1/p')"
	GXSERVER_PACKAGE_BEADS_VERSION="$beads_version" \
		GXSERVER_PACKAGE_DIR="$target_dir" \
		GXSERVER_PACKAGE_VERSION="$package_version" \
		"$GXSERVER_NODE_BIN" <<'JS'
const { createHash } = require("node:crypto");
const { lstatSync, readFileSync, readdirSync, writeFileSync } = require("node:fs");
const { join, relative, sep } = require("node:path");

const targetDir = process.env.GXSERVER_PACKAGE_DIR;
const version = process.env.GXSERVER_PACKAGE_VERSION;
const beadsVersion = process.env.GXSERVER_PACKAGE_BEADS_VERSION;
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
		beadsVersion,
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
	# CDXC:GxserverRustPackaging 2026-06-22-16:17: Local and release macOS builds no longer keep the deleted gxserver/ TypeScript source tree. Assemble the Rust daemon package directly from gxserver-rs, shared/gxserver-protocol.ts, and app-owned tool binaries so `bun run start` never cds into gxserver/ for the default packaged daemon.
	# CDXC:ContributorStart 2026-06-22-23:23: zmx remains required. Beads is always staged from the checksum-pinned schema-compatible release artifact before this package is assembled.
	rm -rf "$package_dir"
	mkdir -p "$package_dir/bin"
	cp "$rust_bin" "$package_dir/bin/gxserver"
	# CDXC:GhostexRustCli 2026-07-13: the public ghostex/gx CLI is the native
	# Rust binary built alongside gxserver; stage it in the same package so
	# app bundles and PATH wrappers resolve one implementation.
	cp "${rust_bin%/*}/ghostex" "$package_dir/bin/ghostex"
	cp "$WEB_DIR/bin/zmx" "$package_dir/bin/zmx"
	chmod 755 "$package_dir/bin/gxserver" "$package_dir/bin/ghostex" "$package_dir/bin/zmx"
	if [[ -x "$WEB_DIR/bin/bd" ]]; then
		write_gxserver_shared_bd_launcher "$package_dir/bin/bd"
	fi
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
		"bin/zmx" \
		"bin/bd" \
		"bin/ghostex-tui"; do
		if [[ ! -e "$package_dir/$required_path" ]]; then
			echo "Remote gxserver $package_label package is missing required resource: $required_path" >&2
			return 1
		fi
	done
	for required_path in \
		"bin/gxserver" \
		"bin/zmx" \
		"bin/bd" \
		"bin/ghostex-tui"; do
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
	# CDXC:RemoteMachines 2026-06-23-09:46: macOS app bundles may stage Linux remote gxserver packages only from explicit prebuilt directories. Validate required gxserver/zmx/bd/Node/Portless/CLI resources and require Linux ELF payloads before copying to Web/gxserver-linux-* so the installer never uploads the host Darwin package to Ubuntu.
	#
	# CDXC:RemoteMachines 2026-06-23-10:07: The Ubuntu package builder writes build/remote-gxserver-linux/<arch>/package by default. Auto-stage that deterministic output when it exists so release/local app packaging can include the already-built Linux package without requiring another env var or rebuilding it in the macOS app pass.
	rm -rf "$target_dir"
	mkdir -p "$target_dir"
	rsync -a --delete "$source_dir"/ "$target_dir"/
}

stage_remote_gxserver_linux_packages_if_configured() {
	if [[ "$GHOSTEX_ON_DEMAND_ASSETS" == "1" ]]; then
		# CDXC:OnDemandAssets 2026-07-02-14:10: On-demand releases publish the Ubuntu packages as version-pinned GitHub release assets instead of embedding them in the app bundle. stage_on_demand_release_assets validates the same source packages and tars them; nothing is copied under Web/.
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
		echo "Build it with: scripts/build-remote-gxserver-linux-release.sh" >&2
		exit 1
	fi
	if ! validate_remote_gxserver_linux_package "$source_dir" "$package_label"; then
		exit 1
	fi
	printf '%s\n' "$source_dir"
}

write_on_demand_bd_launcher() {
	local launcher_path="$1"
	local version="$2"
	local asset_name="$3"
	local asset_sha="$4"
	# CDXC:OnDemandAssets 2026-07-02-14:10: This launcher replaces the large Beads binary in release bundles. It downloads the version-pinned bd asset from the app's own GitHub release on first Project board use, verifies the checksum baked in at build time (sealed by app codesigning), caches per app version, and execs the cached binary. The gxserver package's bin/bd launcher resolves here, so every bd consumer shares this one path.
	cat >"$launcher_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
GHOSTEX_BD_VERSION="$version"
GHOSTEX_BD_ASSET="$asset_name"
GHOSTEX_BD_SHA256="$asset_sha"
EOF
	cat >>"$launcher_path" <<'EOF'
if [[ -n "${GHOSTEX_ON_DEMAND_CACHE_DIR:-}" ]]; then
	CACHE_ROOT="$GHOSTEX_ON_DEMAND_CACHE_DIR"
else
	case "${GHOSTEX_HOME:-}" in
		/*) CACHE_ROOT="${GHOSTEX_HOME%/}/on-demand" ;;
		*)
			case "${XDG_DATA_HOME:-}" in
				/*) CACHE_ROOT="${XDG_DATA_HOME%/}/ghostex/on-demand" ;;
				*) CACHE_ROOT="$HOME/.local/share/ghostex/on-demand" ;;
			esac
			;;
	esac
fi
CACHE_DIR="$CACHE_ROOT/$GHOSTEX_BD_VERSION"
BD_BIN="$CACHE_DIR/bd"
DOWNLOAD_URL="${GHOSTEX_ON_DEMAND_BASE_URL:-https://github.com/maddada/Ghostex/releases/download}/v$GHOSTEX_BD_VERSION/$GHOSTEX_BD_ASSET"
if [[ ! -x "$BD_BIN" ]]; then
	mkdir -p "$CACHE_DIR"
	LOCK_DIR="$CACHE_DIR/.bd-download-lock"
	acquired=0
	for _ in $(seq 1 300); do
		if mkdir "$LOCK_DIR" 2>/dev/null; then
			acquired=1
			break
		fi
		if [[ -x "$BD_BIN" ]]; then
			break
		fi
		sleep 1
	done
	if [[ ! -x "$BD_BIN" ]]; then
		if [[ "$acquired" != "1" ]]; then
			rm -rf "$LOCK_DIR"
			mkdir -p "$LOCK_DIR"
			acquired=1
		fi
		echo "Ghostex: downloading the Project board component ($GHOSTEX_BD_ASSET) for first use..." >&2
		TMP_TAR="$CACHE_DIR/.bd-download-$$.tar.gz"
		TMP_EXTRACT="$CACHE_DIR/.bd-extract-$$"
		cleanup() {
			rm -rf "$TMP_TAR" "$TMP_EXTRACT"
			if [[ "$acquired" == "1" ]]; then
				rm -rf "$LOCK_DIR"
			fi
		}
		trap cleanup EXIT
		if ! /usr/bin/curl -fsSL --retry 2 -o "$TMP_TAR" "$DOWNLOAD_URL"; then
			echo "Ghostex: could not download $DOWNLOAD_URL. The Project board needs one download from github.com per app version." >&2
			exit 69
		fi
		echo "$GHOSTEX_BD_SHA256  $TMP_TAR" | /usr/bin/shasum -a 256 -c - >/dev/null
		rm -rf "$TMP_EXTRACT"
		mkdir -p "$TMP_EXTRACT"
		/usr/bin/tar -xzf "$TMP_TAR" -C "$TMP_EXTRACT"
		/usr/bin/xattr -d com.apple.quarantine "$TMP_EXTRACT/bd" 2>/dev/null || true
		chmod 755 "$TMP_EXTRACT/bd"
		mv -f "$TMP_EXTRACT/bd" "$BD_BIN"
		cleanup
		trap - EXIT
	elif [[ "$acquired" == "1" ]]; then
		rm -rf "$LOCK_DIR"
	fi
fi
exec "$BD_BIN" "$@"
EOF
	chmod 755 "$launcher_path"
}

stage_on_demand_release_assets() {
	local version asset_dir x64_source arm64_source bd_stage_dir
	local x64_sha arm64_sha bd_sha component_manifest
	local -a manifest_args
	if [[ "$GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES" != "1" ]]; then
		echo "GHOSTEX_ON_DEMAND_ASSETS=1 is a release-only mode and requires GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES=1." >&2
		exit 1
	fi
	if [[ "$GHOSTEX_MACOS_ARCH" != "arm64" ]]; then
		echo "GHOSTEX_ON_DEMAND_ASSETS=1 supports only arm64 release builds." >&2
		exit 1
	fi
	if [[ -z "${GHOSTEX_CODE_SIGN_IDENTITY:-}" ]]; then
		echo "GHOSTEX_ON_DEMAND_ASSETS=1 requires GHOSTEX_CODE_SIGN_IDENTITY so the downloadable bd binary is Developer ID signed before upload." >&2
		exit 1
	fi
	if [[ ! -x "$WEB_DIR/bin/bd" ]]; then
		echo "GHOSTEX_ON_DEMAND_ASSETS=1 requires the staged Beads release binary at Web/bin/bd before launcher replacement." >&2
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
		[[ -f "${GHOSTEX_ON_DEMAND_LINUX_X64_ARCHIVE:-}" ]] || { echo "GHOSTEX_ON_DEMAND_LINUX_X64_ARCHIVE is missing." >&2; exit 1; }
		[[ -f "${GHOSTEX_ON_DEMAND_LINUX_ARM64_ARCHIVE:-}" ]] || { echo "GHOSTEX_ON_DEMAND_LINUX_ARM64_ARCHIVE is missing." >&2; exit 1; }
		cp "$GHOSTEX_ON_DEMAND_LINUX_X64_ARCHIVE" "$asset_dir/gxserver-linux-x64.tar.gz"
		cp "$GHOSTEX_ON_DEMAND_LINUX_ARM64_ARCHIVE" "$asset_dir/gxserver-linux-arm64.tar.gz"
	else
		COPYFILE_DISABLE=1 /usr/bin/tar -czf "$asset_dir/gxserver-linux-x64.tar.gz" -C "$x64_source" .
		COPYFILE_DISABLE=1 /usr/bin/tar -czf "$asset_dir/gxserver-linux-arm64.tar.gz" -C "$arm64_source" .
	fi

	bd_stage_dir="$(mktemp -d /tmp/ghostex-bd-asset-XXXXXX)"
	cp "$WEB_DIR/bin/bd" "$bd_stage_dir/bd"
	chmod 755 "$bd_stage_dir/bd"
	# The bd binary leaves the codesigned app bundle, so it must carry its own
	# Developer ID signature for Gatekeeper-adjacent policy checks after the
	# launcher unpacks it outside the bundle.
	/usr/bin/codesign --force --options runtime "${GHOSTEX_CODE_SIGN_TIMESTAMP_FLAG:---timestamp}" --sign "$GHOSTEX_CODE_SIGN_IDENTITY" "$bd_stage_dir/bd"
	smoke_test_staged_beads "$bd_stage_dir/bd"
	COPYFILE_DISABLE=1 /usr/bin/tar -czf "$asset_dir/bd-darwin-arm64.tar.gz" -C "$bd_stage_dir" bd
	rm -rf "$bd_stage_dir"

	x64_sha="$(/usr/bin/shasum -a 256 "$asset_dir/gxserver-linux-x64.tar.gz" | awk '{print $1}')"
	arm64_sha="$(/usr/bin/shasum -a 256 "$asset_dir/gxserver-linux-arm64.tar.gz" | awk '{print $1}')"
	bd_sha="$(/usr/bin/shasum -a 256 "$asset_dir/bd-darwin-arm64.tar.gz" | awk '{print $1}')"

	GHOSTEX_ODA_VERSION="$version" \
		GHOSTEX_ODA_ASSET_DIR="$asset_dir" \
		GHOSTEX_ODA_X64_SHA="$x64_sha" \
		GHOSTEX_ODA_ARM64_SHA="$arm64_sha" \
		GHOSTEX_ODA_BD_SHA="$bd_sha" \
		node -e '
		const fs = require("fs");
		const path = require("path");
		const env = process.env;
		const assetDir = env.GHOSTEX_ODA_ASSET_DIR;
		const entries = [
			{ key: "gxserver-linux-x64", name: "gxserver-linux-x64.tar.gz", sha256: env.GHOSTEX_ODA_X64_SHA },
			{ key: "gxserver-linux-arm64", name: "gxserver-linux-arm64.tar.gz", sha256: env.GHOSTEX_ODA_ARM64_SHA },
			{ key: "bd-darwin-arm64", name: "bd-darwin-arm64.tar.gz", sha256: env.GHOSTEX_ODA_BD_SHA },
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
	node "$REPO_ROOT/scripts/release-gpui/on-demand-manifest.mjs" "${manifest_args[@]}"
	node "$REPO_ROOT/scripts/release-gpui/on-demand-manifest.mjs" validate-macos \
		--manifest "$WEB_DIR/on-demand-resources.json"

	write_on_demand_bd_launcher "$WEB_DIR/bin/bd" "$version" "bd-darwin-arm64.tar.gz" "$bd_sha"
	rm -rf "$WEB_DIR/gxserver-linux-x64" "$WEB_DIR/gxserver-linux-arm64"
	echo "On-demand release assets ready: x64=$x64_sha arm64=$arm64_sha bd=$bd_sha"
}

package_gxserver_if_needed() {
	local target_dir="$WEB_DIR/gxserver"
	local package_dir package_digest package_version rust_bin
	# GPUI bundles the native Rust gxserver package used by standalone installs.
	# TypeScript daemon packaging is intentionally unsupported.
	#
	# CDXC:LocalStartFast 2026-06-07-16:23: gxserver packaging should skip work when gxserver runtime sources, package metadata, packager code, bundled zmx/bd binaries, and generated protocol inputs are unchanged.
	#
	# CDXC:ProjectBoardBeads 2026-06-08-10:46: Package the full checksum-verified schema-v54 Beads CLI with gxserver so Project/Kanban opens without PATH setup. The active macOS release target is arm64 and receives its matching signed binary.
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
			--path "$REPO_ROOT/shared/gxserver-protocol.ts" \
			--path "$GXSERVER_RS_ROOT/src" \
			--path "$GXSERVER_RS_ROOT/Cargo.toml" \
			--path "$GXSERVER_RS_ROOT/Cargo.lock" \
			--path "$WEB_DIR/bin/zmx" \
			--path "$WEB_DIR/bin/bd")"
	local cache_outputs=("$target_dir/build-identity.json" "$target_dir/bin/gxserver" "$target_dir/dist/protocol/index.js" "$target_dir/dist/protocol/index.d.ts")
	cache_outputs+=("$target_dir/bin/ghostex")
	if cache_matches "gxserver-package-$GHOSTEX_MACOS_ARCH" "$package_digest" "${cache_outputs[@]}" &&
		gxserver_package_supports_macos_arch "$target_dir"; then
		# CDXC:GxserverPackaging 2026-06-08-16:23: Web/gxserver is also shared across dual-architecture release passes. Do not accept a cache hit unless the staged gxserver, zmx, and bd binaries match the requested architecture, or Intel and arm64 DMGs can silently inherit the previous pass's native artifacts.
		echo "gxserver package is current; skipping package rebuild."
		return 0
	fi

	echo "Packaging Rust gxserver with $rust_bin"
	package_gxserver_rust_package "$package_dir" "$rust_bin" "$package_version"
	rm -rf "$target_dir"
	cp -R "$package_dir" "$target_dir"
	if [[ -x "$WEB_DIR/bin/bd" ]]; then
		write_gxserver_shared_bd_launcher "$target_dir/bin/bd"
	else
		rm -f "$target_dir/bin/bd"
	fi
	write_cache_stamp "gxserver-package-$GHOSTEX_MACOS_ARCH" "$package_digest"
}

write_build_capabilities_manifest() {
	local notes_payload=""
	local note
	# CDXC:ContributorStart 2026-06-23-04:03: Local starts may have no skipped optional resources. macOS /bin/bash 3.2 treats an empty array expansion as unbound under `set -u`, so emit an empty notes payload without expanding the array when it has no entries.
	if (( ${#APP_OPTIONAL_RESOURCE_NOTES[@]} > 0 )); then
		for note in "${APP_OPTIONAL_RESOURCE_NOTES[@]}"; do
			notes_payload+="$note"$'\n'
		done
	fi
	GHOSTEX_CAP_SHARED_NODE_RUNTIME="$APP_CAPABILITY_SHARED_NODE_RUNTIME" \
		GHOSTEX_CAP_SOURCE_EDITOR="$APP_CAPABILITY_SOURCE_EDITOR" \
		GHOSTEX_CAP_TUI="$APP_CAPABILITY_TUI" \
		GHOSTEX_CAP_BEADS="$APP_CAPABILITY_BEADS" \
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
CDXC:ContributorStart 2026-06-22-23:23:
The app bundle needs a structured resource-capability manifest so local validation and Settings can distinguish intentionally omitted optional contributor modules from broken packaged resources. Keep the payload free of filesystem paths because persistent app diagnostics may include the same capability fields later.
*/
writeFileSync(
  capabilityPath,
  `${JSON.stringify({
    generatedBy: "prepare-macos-runtime.sh",
    optionalSubmodulesMayBeMissing: bool("GHOSTEX_CAP_ALLOW_MISSING_OPTIONAL"),
    resources: {
      beads: bool("GHOSTEX_CAP_BEADS"),
      sharedNodeRuntime: bool("GHOSTEX_CAP_SHARED_NODE_RUNTIME"),
      sourceEditor: bool("GHOSTEX_CAP_SOURCE_EDITOR"),
      tui: bool("GHOSTEX_CAP_TUI"),
      zmx: bool("GHOSTEX_CAP_ZMX"),
    },
    skippedOptionalResources: notes,
    version: 1,
  }, null, 2)}\n`,
  "utf8",
);
JS
}

# CDXC:CodeServerRuntime 2026-06-08-12:17: code-server owns the bundled Node runtime in the macOS app. Build code-server with Node 22 and stage that runtime inside Web/code-server/lib/node; explicit TypeScript gxserver packages reuse that runtime instead of shipping a duplicate Node.
CODE_SERVER_NODE_BIN="$(prepare_code_server_app_node_runtime)"
CODE_SERVER_NODE_DIR="$(cd "$(dirname "$CODE_SERVER_NODE_BIN")" && pwd)"
CODE_SERVER_NPM_BIN="$CODE_SERVER_NODE_DIR/npm"
if [[ ! -x "$CODE_SERVER_NPM_BIN" ]]; then
	echo "npm is required in the cached code-server Node distribution: $CODE_SERVER_NPM_BIN" >&2
	exit 1
fi
CODE_SERVER_ROOT="$(resolve_code_server_root || true)"
if [[ -z "$CODE_SERVER_ROOT" ]]; then
	if [[ "$CODE_SERVER_ROOT_EXPLICITLY_CONFIGURED" == "1" || "$GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES" == "0" ]]; then
		cat >&2 <<EOF
code-server source is required to package the embedded Source-tab runtime.

Set CODE_SERVER_ROOT or GHOSTEX_CODE_SERVER_ROOT to a code-server checkout, or place it at:
  $REPO_ROOT/code-server
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

# CDXC:NativeBuild 2026-05-29-11:24: `bun run start` builds zmx and its Ghostty Zig dependency, which require Zig 0.15.2. A global Homebrew `zig` upgrade to 0.16 breaks the build API, so the local native build must choose the compatible Zig binary deliberately instead of inheriting the first PATH entry.
ZIG_BIN="${ZIG:-}"
if [[ -z "$ZIG_BIN" && -x /opt/homebrew/opt/zig@0.15/bin/zig ]]; then
	ZIG_BIN=/opt/homebrew/opt/zig@0.15/bin/zig
elif [[ -z "$ZIG_BIN" ]]; then
	ZIG_BIN="$(command -v zig || true)"
fi
if [[ -z "$ZIG_BIN" ]]; then
	cat >&2 <<EOF
Zig 0.15.2 is required to build Ghostex's native zmx/Ghostty dependency.

Install it, then rerun this script:
  brew install zig@0.15
EOF
	exit 1
fi
ZIG_VERSION="$("$ZIG_BIN" version 2>/dev/null || true)"
if [[ "$ZIG_VERSION" != "0.15.2" ]]; then
	cat >&2 <<EOF
Zig 0.15.2 is required to build Ghostex's native zmx/Ghostty dependency.

Selected Zig:
  $ZIG_BIN
  version: ${ZIG_VERSION:-unknown}

Install Homebrew's compatible keg or set ZIG explicitly:
  brew install zig@0.15
  ZIG=/opt/homebrew/opt/zig@0.15/bin/zig bun run start
EOF
	exit 1
fi
export ZIG="$ZIG_BIN"

mkdir -p "$WEB_DIR"
rm -rf "$CLI_DIR"
mkdir -p "$CLI_DIR"

# CDXC:ZmxPersistence 2026-05-20-09:57: zmx pane refresh is now a zmx IPC feature, so Ghostex must bundle the pinned submodule binary instead of depending on whichever zmx happens to be on PATH. Build the submodule for the requested macOS architecture and copy it into app resources where TerminalWorkspaceView can launch it directly.
if [[ ! -f "$ZMX_ROOT/build.zig" ]]; then
	cat >&2 <<EOF
zmx source is missing:
  $ZMX_ROOT

Initialize submodules before building:
  git submodule update --init --recursive zmx
EOF
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
# CDXC:ContributorStart 2026-06-22-23:23: Optional contributor submodules should be packaged when present and strict, but absent optional checkouts should only disable their feature in local starts. Keep zmx above as the hard terminal/persistence dependency; gate TUI, Zehn, and Source independently so one missing feature cannot remove the rest of the app shell. Beads is staged independently from its verified official release archive.
case "$GHOSTEX_MACOS_ARCH" in
	arm64)
		TUI_CARGO_TARGET="aarch64-apple-darwin"
		;;
	x86_64)
		TUI_CARGO_TARGET="x86_64-apple-darwin"
		;;
esac
TUI_CARGO_BIN="${CARGO:-}"
if [[ -z "$TUI_CARGO_BIN" ]]; then
	TUI_CARGO_BIN="$(command -v cargo || true)"
fi
if [[ -f "$TUI_ROOT/Cargo.toml" ]]; then
	if [[ -z "$TUI_CARGO_BIN" ]]; then
		cat >&2 <<EOF
Cargo is required to build bundled ghostex-tui.

Install Rust, then rerun this script:
  rustup toolchain install stable
EOF
		exit 1
	fi
	build_tui_if_needed
	cp "$TUI_ROOT/target/$TUI_CARGO_TARGET/release/ghostex-tui" "$WEB_DIR/bin/ghostex-tui"
	chmod 755 "$WEB_DIR/bin/ghostex-tui"
	APP_CAPABILITY_TUI=true
elif [[ "$TUI_ROOT_EXPLICITLY_CONFIGURED" == "1" || "$GHOSTEX_ALLOW_MISSING_OPTIONAL_SUBMODULES" == "0" ]]; then
	cat >&2 <<EOF
Ghostex TUI source is missing:
  $TUI_ROOT

Initialize or provide the TUI source before building the app bundle.
EOF
	exit 1
else
	record_optional_resource_note "Ghostex TUI" "tui2 checkout was not found"
fi
stage_beads_release_if_needed
cp "$REPO_ROOT/build/$GHOSTEX_MACOS_ARCH/beads/bd" "$WEB_DIR/bin/bd"
chmod 755 "$WEB_DIR/bin/bd"
smoke_test_staged_beads "$WEB_DIR/bin/bd"
APP_CAPABILITY_BEADS=true
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
# CDXC:CliSessions 2026-05-10-03:28: Shells resolve the installed macOS
# executable as a terminal command. Bundle the native CLI in app resources
# so main.swift can proxy command argv before the AppKit app starts.
# CDXC:CliBranding 2026-05-26-15:11: Public CLI commands are now `ghostex`
# and `gx`; the bundled binary filename follows the long public CLI name while
# internal GHOSTEX_* environment names and storage paths remain implementation
# details. The macOS app bundle should ship executable `ghostex` and `gx`
# launchers automatically so Homebrew can install both public commands without
# asking users to add shell aliases by hand.
# CDXC:CliInstall 2026-06-07-13:53: The app CLI is not a web asset. Stage it under Contents/Resources/CLI so DMG and Homebrew installs can symlink public commands to one app-owned runtime while Web remains only the sidebar/runtime asset folder.
# CDXC:GhostexRustCli 2026-07-13: the public CLI is the native Rust `ghostex`
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
