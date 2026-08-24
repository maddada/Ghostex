#!/usr/bin/env bash
# Sync the vendored .dependencies/ghostty/ tree with upstream and re-apply the
# Ghostex patch series in .dependencies/ghostty-patches/. See
# .dependencies/ghostty-patches/README.md.
#
# Usage:
#   tooling/sync-ghostty.sh <upstream-ref>   # sync to ref (e.g. origin/main or a SHA)
#   tooling/sync-ghostty.sh --regen          # regenerate patches from the current tree
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GHOSTTY_DIR="$ROOT_DIR/.dependencies/ghostty"
PATCH_DIR="$ROOT_DIR/.dependencies/ghostty-patches"
UPSTREAM_URL="https://github.com/ghostty-org/ghostty.git"

# Per-patch file manifest. Keep in sync with .dependencies/ghostty-patches/README.md.
patch_files() {
	case "$1" in
	0001-build-lib-vt-shared-option-and-themes-install)
		echo "build.zig src/build/Config.zig src/build/GhosttyResources.zig"
		;;
	0002-build-xcframework-lazy-universal)
		echo "src/build/GhosttyXCFramework.zig"
		;;
	0003-build-metallib-developer-dir-override)
		echo "src/build/MetallibStep.zig"
		;;
	0004-app-debug-skip-slow-integrity-checks)
		echo "src/build_config.zig"
		;;
	0005-embed-config-string-apis)
		echo "src/config/CApi.zig include/ghostty.h"
		;;
	0006-mouse-cmd-click-encode-and-mod-dedupe)
		echo "src/input/mouse_encode.zig"
		;;
	0007-teardown-deadlock-hardening)
		echo "src/Surface.zig src/renderer/generic.zig src/termio/Exec.zig src/termio/Termio.zig src/termio/mailbox.zig src/termio/stream_handler.zig"
		;;
	*)
		echo "unknown patch: $1" >&2
		return 1
		;;
	esac
}

PATCH_NAMES=(
	0001-build-lib-vt-shared-option-and-themes-install
	0002-build-xcframework-lazy-universal
	0003-build-metallib-developer-dir-override
	0004-app-debug-skip-slow-integrity-checks
	0005-embed-config-string-apis
	0006-mouse-cmd-click-encode-and-mod-dedupe
	0007-teardown-deadlock-hardening
)

pinned_commit() {
	sed -n 's/^commit=//p' "$PATCH_DIR/UPSTREAM"
}

clone_upstream() {
	local workdir="$1"
	git clone --quiet --filter=blob:none "$UPSTREAM_URL" "$workdir/upstream"
}

regen() {
	local workdir
	workdir="$(mktemp -d)"
	trap 'rm -rf "$workdir"' EXIT
	local pin
	pin="$(pinned_commit)"
	echo "Regenerating patches against pinned upstream $pin"
	clone_upstream "$workdir"
	git -C "$workdir/upstream" checkout --quiet "$pin"
	for name in "${PATCH_NAMES[@]}"; do
		: >"$PATCH_DIR/$name.patch"
		for f in $(patch_files "$name"); do
			diff -u --label "a/$f" --label "b/$f" \
				"$workdir/upstream/$f" "$GHOSTTY_DIR/$f" \
				>>"$PATCH_DIR/$name.patch" || true
		done
		echo "  $name.patch ($(wc -l <"$PATCH_DIR/$name.patch" | tr -d ' ') lines)"
	done
	echo "Done. Review with: git diff .dependencies/ghostty-patches/"
}

sync() {
	local ref="$1"
	local workdir
	workdir="$(mktemp -d)"
	trap 'rm -rf "$workdir"' EXIT

	clone_upstream "$workdir"
	local sha
	sha="$(git -C "$workdir/upstream" rev-parse "$ref^{commit}")"
	echo "Syncing .dependencies/ghostty/ to upstream $sha"

	# Stage pristine upstream + patches in a temp tree first so a conflicting
	# patch aborts before the real tree is touched.
	mkdir "$workdir/tree"
	git -C "$workdir/upstream" archive "$sha" | tar -x -C "$workdir/tree"
	for name in "${PATCH_NAMES[@]}"; do
		if ! (cd "$workdir/tree" && patch -p1 --no-backup-if-mismatch -s <"$PATCH_DIR/$name.patch"); then
			echo "" >&2
			echo "PATCH FAILED: $name.patch" >&2
			echo "Rebase it by hand: apply the others, port the change onto the new" >&2
			echo "upstream code in .dependencies/ghostty/, then run: tooling/sync-ghostty.sh --regen" >&2
			exit 1
		fi
	done

	# Replace the vendored tree. Keep gitignored build/artifact dirs.
	rsync -a --delete \
		--exclude '/.zig-cache/' \
		--exclude '/zig-out/' \
		--exclude '/zig-pkg/' \
		--exclude '/macos/GhosttyKit.xcframework/' \
		"$workdir/tree/" "$GHOSTTY_DIR/"

	sed -i '' -e "s/^commit=.*/commit=$sha/" -e "s/^synced=.*/synced=$(date +%Y-%m-%d)/" \
		"$PATCH_DIR/UPSTREAM"

	cat <<EOF

Synced to $sha. Now verify (see .dependencies/ghostty-patches/README.md):
  1. cd .dependencies/ghostty && zig build test-lib-vt
  2. cd gpui && cargo check
  3. Re-audit gpui/src/ghostty_vt.rs + ghostty_kit.rs against .dependencies/ghostty/include/
     (implicit C enums renumber when upstream inserts entries!)
  4. cd .dependencies/ghostty && zig build -Demit-xcframework -Dxcframework-target=universal \\
       -Demit-macos-app=false -Doptimize=ReleaseSafe
EOF
}

case "${1:-}" in
--regen) regen ;;
"")
	echo "usage: $(basename "$0") <upstream-ref> | --regen" >&2
	exit 64
	;;
*) sync "$1" ;;
esac
