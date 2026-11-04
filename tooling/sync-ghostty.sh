#!/usr/bin/env bash
# Sync the vendored .dependencies/ghostty/ tree with upstream and re-apply the
# Ghostex patch series in .dependencies/ghostty-patches/. See
# .dependencies/ghostty-patches/README.md.
#
# Usage:
#   tooling/sync-ghostty.sh <upstream-ref>   # sync to ref (e.g. origin/main or a SHA)
#   tooling/sync-ghostty.sh --regen          # regenerate patches from the current tree
#   tooling/sync-ghostty.sh --check-ignored [<upstream-ref>]
#                                            # only check that git would track every file
#                                            # of the patched tree at ref (default: pinned)
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
	0005-embed-config-string-apis)
		echo "src/config/CApi.zig include/ghostty.h"
		;;
	0006-mouse-cmd-click-encode-and-mod-dedupe)
		echo "src/input/mouse_encode.zig"
		;;
	0007-teardown-deadlock-hardening)
		echo "src/Surface.zig src/renderer/generic.zig src/termio/Exec.zig src/termio/Termio.zig src/termio/mailbox.zig src/termio/stream_handler.zig"
		;;
	0008-embed-custom-shader-msl-api)
		echo "src/apprt/embedded.zig"
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
	0005-embed-config-string-apis
	0006-mouse-cmd-click-encode-and-mod-dedupe
	0007-teardown-deadlock-hardening
	0008-embed-custom-shader-msl-api
)

pinned_commit() {
	sed -n 's/^commit=//p' "$PATCH_DIR/UPSTREAM"
}

clone_upstream() {
	local workdir="$1"
	git clone --quiet --filter=blob:none "$UPSTREAM_URL" "$workdir/upstream"
}

# Extract upstream at $2 into $1/tree and apply the patch series; exits on a
# conflicting patch so nothing outside the temp dir is touched.
stage_patched_tree() {
	local workdir="$1" sha="$2"
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
}

# CDXC:Build 2026-10-26 WHY: the root .gitignore's `build/` rule silently hid
# two new upstream files in the Zig 0.17 sync (src/build/GhosttyPkgConfig.zig,
# src/build/libghostty-vt.pc.in), so the commit lacked them and a fresh
# checkout's GhosttyKit build failed. Every file of the staged tree must be one
# git would add: a path that is ignored and not already tracked fails the sync
# before the vendored tree is replaced. Files already in the index are never
# reported, since git keeps tracking them whatever the ignore rules say.
check_tree_not_ignored() {
	local tree="$1" status=0 ignored
	ignored="$(
		cd "$tree" &&
			find . \( -type f -o -type l \) -print0 |
			perl -0pe 's{^\./}{.dependencies/ghostty/}' |
			git -C "$ROOT_DIR" check-ignore -z --stdin |
			tr '\0' '\n'
	)" || status=$?
	# check-ignore exits 1 when nothing matched; anything above that is an error.
	if ((status > 1)); then
		echo "git check-ignore failed (exit $status)." >&2
		exit 1
	fi
	if [[ -n "$ignored" ]]; then
		echo "" >&2
		echo "IGNORED FILES: git would not track these vendored Ghostty files:" >&2
		while IFS= read -r path; do printf '  %s\n' "$path"; done <<<"$ignored" >&2
		echo "Add !-exceptions for their folders to the root .gitignore, then sync again." >&2
		exit 1
	fi
	echo "Every vendored Ghostty file is trackable by git."
}

check_ignored() {
	local workdir
	workdir="$(mktemp -d)"
	local cleanup_command
	printf -v cleanup_command 'rm -rf -- %q' "$workdir"
	trap "$cleanup_command" EXIT
	clone_upstream "$workdir"
	local sha
	sha="$(git -C "$workdir/upstream" rev-parse "${1:-$(pinned_commit)}^{commit}")"
	echo "Checking the patched upstream $sha tree against .gitignore"
	stage_patched_tree "$workdir" "$sha"
	check_tree_not_ignored "$workdir/tree"
}

regen() {
	local workdir
	workdir="$(mktemp -d)"
	local cleanup_command
	printf -v cleanup_command 'rm -rf -- %q' "$workdir"
	trap "$cleanup_command" EXIT
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
	local cleanup_command
	printf -v cleanup_command 'rm -rf -- %q' "$workdir"
	trap "$cleanup_command" EXIT

	clone_upstream "$workdir"
	local sha
	sha="$(git -C "$workdir/upstream" rev-parse "$ref^{commit}")"
	echo "Syncing .dependencies/ghostty/ to upstream $sha"

	# Stage pristine upstream + patches in a temp tree first so a conflicting
	# patch aborts before the real tree is touched.
	stage_patched_tree "$workdir" "$sha"
	check_tree_not_ignored "$workdir/tree"

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
  2. cd apps/desktop && cargo check
  3. Re-audit apps/desktop/src/ghostty_vt/ffi.rs + ghostty_kit.rs against .dependencies/ghostty/include/
     (implicit C enums renumber when upstream inserts entries!)
  4. cd .dependencies/ghostty && zig build -Demit-xcframework -Dxcframework-target=universal \\
       -Demit-macos-app=false -Doptimize=ReleaseSafe
EOF
}

case "${1:-}" in
--regen) regen ;;
--check-ignored) check_ignored "${2:-}" ;;
"")
	echo "usage: $(basename "$0") <upstream-ref> | --regen | --check-ignored [<upstream-ref>]" >&2
	exit 64
	;;
*) sync "$1" ;;
esac
