#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GPUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$GPUI_DIR/../.." && pwd)"
BUILD_CACHE_DIR="${GHOSTEX_BUILD_CACHE_DIR:-$REPO_ROOT/build/${GHOSTEX_MACOS_ARCH:-$(uname -m)}/build-cache}"
source "$SCRIPT_DIR/build-cache.sh"

build_cef_sidebar_bundle_if_needed() {
	local bundle_digest
	local -a bundle_outputs

	# CDXC:Build 2026-09-05 WHY:
	# Generated CSS is an output, so hashing it as an input invalidates the cache after Tailwind changes it.
	# Hash the CEF entries, their shared imports and toolchain inputs so Rust-only edits reuse the web bundle.
	bundle_digest="$(fingerprint_inputs \
		--value "cef-sidebar-bundle-v2" \
		--exclude-path "$REPO_ROOT/packages/core-ui/styles/shadcn.generated.css" \
		--value "bun=$(bun --version 2>/dev/null || true)" \
		--path "$SCRIPT_DIR/build-macos-sidebar.sh" \
		--path "$GPUI_DIR/vite.config.ts" \
		--path "$GPUI_DIR/tsconfig.json" \
		--path "$GPUI_DIR/index.html" \
		--path "$GPUI_DIR/chat.html" \
		--path "$GPUI_DIR/find.html" \
		--path "$GPUI_DIR/kanban.html" \
		--path "$GPUI_DIR/manage.html" \
		--path "$GPUI_DIR/modal-host.html" \
		--path "$GPUI_DIR/titlebar-host.html" \
		--path "$GPUI_DIR/sidebar" \
		--path "$GPUI_DIR/views" \
		--path "$REPO_ROOT/packages/core-ui" \
		--path "$REPO_ROOT/packages/components" \
		--path "$REPO_ROOT/packages/shared" \
		--path "$REPO_ROOT/apps/web/src" \
		--path "$REPO_ROOT/tooling/shiki-classic-assets.mjs" \
		--path "$REPO_ROOT/package.json" \
		--path "$REPO_ROOT/bun.lock" \
		--path "$REPO_ROOT/tsconfig.json")"
	bundle_outputs=(
		"$REPO_ROOT/packages/core-ui/styles/shadcn.generated.css"
		"$GPUI_DIR/dist/sidebar/index.html"
		"$GPUI_DIR/dist/sidebar/chat.html"
		"$GPUI_DIR/dist/sidebar/find.html"
		"$GPUI_DIR/dist/sidebar/kanban.html"
		"$GPUI_DIR/dist/sidebar/manage.html"
		"$GPUI_DIR/dist/sidebar/modal-host.html"
		"$GPUI_DIR/dist/sidebar/titlebar-host.html"
		"$GPUI_DIR/dist/sidebar/monaco/vs/loader.js"
	)
	if cache_matches "cef-sidebar-bundle" "$bundle_digest" "${bundle_outputs[@]}"; then
		echo "CEF sidebar bundle is current; skipping web build."
		return 0
	fi
	(
		cd "$REPO_ROOT"
		bun run build:sidebar-css
		bunx vite build --config "$GPUI_DIR/vite.config.ts"
	)
	write_cache_stamp "cef-sidebar-bundle" "$bundle_digest"
}

build_cef_sidebar_bundle_if_needed
