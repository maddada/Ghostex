#!/usr/bin/env bash
# Sourced by prepare-macos-runtime.sh after its runtime configuration is set.

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
	if [[ "$CODE_SERVER_APP_NODE_VERSION" == "24.18.1" ]]; then
		case "$distribution_arch" in
		arm64)
			printf '1d60b703fe5d7e7072489be8187f430f1a095a658c31e5e1e281331a5873fac3\n'
			return 0
			;;
		x64)
			printf 'f892c7895720f40d3750bde24f3554242d36f23602b5167b5b73ec4d13938aef\n'
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

	# CDXC:CodeEditor 2026-06-08-12:17: code-server owns Ghostex's app-bundled Node runtime. Cache the official per-architecture pinned Node distribution for build-time npm/node-gyp work, then stage the executable inside Web/code-server/lib/node so gxserver and code-server share one bundled Node instead of shipping duplicate runtimes.
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

# CDXC:RepoStructure 2026-08-25: The 2026-08-22 restructure moved the
# code-server, zmx and zehn gitlinks from the repository root into .dependencies/.
# Git cannot relocate a submodule working tree as part of a gitlink rename, so every
# checkout that had them initialized before that commit keeps the real tree at the old
# top-level path and receives an empty directory at the new one. That state is
# indistinguishable from "this contributor never initialized the optional submodule"
# unless the stranded signature is checked explicitly, and the contributor skip would
# otherwise package an app whose Code tab cannot start. These helpers identify the
# stranded signature so the packaging steps can fail with repair instructions, while
# still skipping for a checkout that genuinely has no working tree for the submodule:
# a real checkout at the old top-level path proves stranding, and for a checkout with
# no tree at either path, git's recorded core.worktree separates "initialized here"
# from "never initialized or deliberately `git submodule deinit`ed", which unsets it.
repo_git_common_dir() {
	local dir
	dir="$(cd "$REPO_ROOT" && git rev-parse --git-common-dir 2>/dev/null)" || return 1
	[[ -n "$dir" ]] || return 1
	(cd "$REPO_ROOT" && cd "$dir" && pwd)
}

submodule_git_module_dir() {
	# $1 = submodule name in .gitmodules (unchanged by the restructure).
	local common_dir
	common_dir="$(repo_git_common_dir)" || return 1
	[[ -d "$common_dir/modules/$1" ]] || return 1
	printf '%s\n' "$common_dir/modules/$1"
}

submodule_configured_worktree() {
	# $1 = submodule name. Prints the working tree git recorded when the submodule was
	# initialized here. `git submodule deinit` unsets core.worktree, so an unset value
	# means this checkout deliberately has no working tree for the submodule and the
	# optional-resource skip is the correct behaviour.
	local module_dir worktree resolved
	module_dir="$(submodule_git_module_dir "$1")" || return 1
	worktree="$(git config -f "$module_dir/config" --get core.worktree 2>/dev/null)" || return 1
	[[ -n "$worktree" ]] || return 1
	case "$worktree" in
	/*) resolved="$worktree" ;;
	*) resolved="$module_dir/$worktree" ;;
	esac
	if [[ -d "$resolved" ]]; then
		(cd "$resolved" && pwd)
		return 0
	fi
	printf '%s\n' "$resolved"
}

legacy_submodule_checkout() {
	# $1 = pre-restructure top-level directory, $2 = file that proves a real checkout.
	[[ -f "$REPO_ROOT/$1/$2" ]] || return 1
	(cd "$REPO_ROOT/$1" && pwd)
}

fail_if_code_server_checkout_is_stranded() {
	local expected_root legacy_root configured_worktree
	expected_root="$REPO_ROOT/.dependencies/code-server"
	legacy_root="$(legacy_submodule_checkout code-server package.json || true)"
	if [[ -z "$legacy_root" ]]; then
		configured_worktree="$(submodule_configured_worktree code-server || true)"
		# Never initialized here, or deliberately deinitialized: keep the contributor skip.
		[[ -n "$configured_worktree" ]] || return 0
		if [[ "$configured_worktree" == "$expected_root" ]]; then
			cat >&2 <<EOF
code-server source is missing:
  $expected_root

This checkout initialized the code-server submodule, so its working tree was
removed rather than never having been set up. Refusing to package an app whose
Code tab cannot start.

Restore the checkout:
  git -C $REPO_ROOT submodule update --init .dependencies/code-server

If you moved the tree elsewhere, point the build at it instead:
  GHOSTEX_CODE_SERVER_ROOT=/path/to/code-server bun run start
EOF
			exit 1
		fi
		legacy_root="$configured_worktree"
	fi
	cat >&2 <<EOF
code-server source is stranded outside .dependencies/code-server:
  $legacy_root

The 2026-08-22 restructure moved the submodule from code-server to
.dependencies/code-server. Git cannot move a submodule working tree with the
gitlink, so your checkout stayed where it was and .dependencies/code-server is
empty. Refusing to package an app whose Code tab cannot start.

Unblock this build without moving anything:
  GHOSTEX_CODE_SERVER_ROOT=$legacy_root bun run start

Repair the checkout (keeps node_modules and the built VS Code payload):
  cd $REPO_ROOT
  rmdir .dependencies/code-server
  mv $legacy_root .dependencies/code-server
  echo 'gitdir: ../../.git/modules/code-server' > .dependencies/code-server/.git
  git config -f .git/modules/code-server/config \\
    core.worktree ../../../.dependencies/code-server
  echo 'gitdir: ../../../../.git/modules/code-server/modules/lib/vscode' \\
    > .dependencies/code-server/lib/vscode/.git
  git config -f .git/modules/code-server/modules/lib/vscode/config \\
    core.worktree ../../../../../../.dependencies/code-server/lib/vscode

Then verify the repair:
  git -C .dependencies/code-server rev-parse HEAD
  git submodule status .dependencies/code-server
EOF
	exit 1
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
	if [[ -f "$REPO_ROOT/.dependencies/code-server/package.json" ]]; then
		(cd "$REPO_ROOT/.dependencies/code-server" && pwd)
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
		echo "code-server node_modules are missing. Run: npm --prefix \"$CODE_SERVER_ROOT\" install" >&2
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
		echo "code-server VS Code submodule is missing. Run: git -C \"$CODE_SERVER_ROOT\" submodule update --init lib/vscode" >&2
		exit 1
	fi
	if [[ ! -d "$CODE_SERVER_ROOT/lib/vscode/node_modules" ]]; then
		echo "code-server VS Code node_modules are missing. Run: npm --prefix \"$CODE_SERVER_ROOT/lib/vscode\" install" >&2
		exit 1
	fi
	vscode_ripgrep_bin="$(code_server_vscode_ripgrep_bin "$vscode_release_root")"
	node_identity="$("$CODE_SERVER_NODE_BIN" -p 'process.version + ":" + process.versions.modules')"
	npm_version="$("$CODE_SERVER_NPM_BIN" --version 2>/dev/null || true)"
	package_version="$(code_server_release_version)"
	commit="$(git -C "$CODE_SERVER_ROOT" rev-parse HEAD 2>/dev/null || printf 'development')"
	payload_digest="$(code_server_vscode_payload_digest "$vscode_target" "$node_identity" "$npm_version" "$package_version" "$commit")"
	payload_cache_key="code-server-vscode-payload-$GHOSTEX_MACOS_ARCH"
	# CDXC:CodeEditor 2026-06-09-17:06: Embedded VS Code search depends on @vscode/ripgrep/bin/rg. Rebuild the generated REH web payload when code-server packaging inputs change, server-main.js is missing, or ripgrep is missing/wrong-arch so `bun run start` and release builds cannot reuse a stale payload that opens but fails search.
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
	# CDXC:CodeEditor 2026-06-08-12:17: The app bundle must contain a self-contained code-server runtime at Web/code-server and the single shared Node executable at Web/code-server/lib/node. Missing code-server resources are build failures instead of installed-user Node prompts.
	if cache_matches "code-server-package-$GHOSTEX_MACOS_ARCH" "$package_digest" "$target_dir/out/node/entry.js" "$target_dir/lib/vscode/out/server-main.js" "$target_dir/lib/vscode/node_modules/@vscode/ripgrep/bin/rg" "$target_dir/lib/node" "$target_dir/node_modules" "$expected_node_pty_prebuild" &&
		node_pty_prebuilds_match_arch "$target_dir" &&
		binary_supports_macos_arch "$target_dir/lib/node" "$GHOSTEX_MACOS_ARCH" &&
		binary_supports_macos_arch "$target_dir/lib/vscode/node_modules/@vscode/ripgrep/bin/rg" "$GHOSTEX_MACOS_ARCH"; then
		# CDXC:CodeEditor 2026-06-08-16:23: Web/code-server is a shared staging directory reused by arm64 and x86_64 release passes. A per-arch cache stamp is only valid when the staged Node executable still contains the requested CPU slice; otherwise restage the package so app validation uses the matching runtime.
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
	# CDXC:Build 2026-06-22-23:23: Optional Source panes must not remove the shared app-owned Node runtime. Native sidebar helpers and Portless still resolve Web/code-server/lib/node, so contributor builds without the code-server submodule stage only that executable and leave Source-specific files absent.
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
	resolved_version="$(node "$REPO_ROOT/tooling/release-gpui/code-server-component-identity.mjs" --root "$CODE_SERVER_ROOT")"
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
		node "$REPO_ROOT/tooling/release-gpui/verify-code-server-archive.mjs" \
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
		"$REPO_ROOT/tooling/release-gpui/create-deterministic-tar.sh" "$stage_root" "$asset_path"
		rm -rf "$stage_root"
		asset_sha256="$(shasum -a 256 "$asset_path" | awk '{print $1}')"
		printf '%s  %s\n' "$asset_sha256" "$(basename "$asset_path")" >"$asset_sidecar"
	fi
	node "$REPO_ROOT/tooling/release-gpui/verify-code-server-archive.mjs" \
		--archive "$asset_path" \
		--version "$component_version" \
		--platform darwin-arm64
	if [[ "$reused_published_component" == "1" ]]; then
		echo "Reused verified published code-server component $component_tag."
	fi
	node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs" \
		--metadata-only \
		--component code-server \
		--version "$component_version" \
		--asset-dir "$asset_dir" \
		--require-platforms darwin-arm64,linux-x64,linux-arm64 \
		--require-sha256-sidecars \
		--output "$component_manifest"
	echo "Prepared code-server component $component_version: $asset_path"
}
