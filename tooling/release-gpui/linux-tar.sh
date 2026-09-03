#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" linux-tar-x64)}"
release_gpui_require_version "$VERSION"
release_gpui_require_command tar
release_gpui_require_command zstd
TAR_VERSION="$(tar --version 2>&1)"
case "$TAR_VERSION" in
*"GNU tar"*) ;;
*)
	echo "The portable Linux tarball needs GNU tar (--mtime/--no-recursion); this host has: ${TAR_VERSION%%$'\n'*}" >&2
	exit 1
	;;
esac
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

# CDXC:Release 2026-08-13: shared staged root, see linux-deb.sh.
PACKAGE_ROOT="${GHOSTEX_LINUX_PACKAGE_ROOT:-$REPO_ROOT/build/release-gpui/linux-tar-package-root}"
if [[ "${GHOSTEX_LINUX_PACKAGE_ROOT_READY:-0}" != "1" ]]; then
	"$SCRIPT_DIR/linux-stage.sh" "$VERSION" "$PACKAGE_ROOT"
fi
[[ -d "$PACKAGE_ROOT/opt/ghostex" ]] || {
	echo "Linux package root is not staged: $PACKAGE_ROOT" >&2
	exit 1
}

# This archive is the staged package root and nothing else: a user installs it
# with `tar -xpf ghostex-<version>-linux-x64.tar.zst -C /`, and Debian control
# metadata is not part of a distro-agnostic payload. linux-deb.sh writes DEBIAN/
# into the shared root, so drop it here the same way linux-rpm.sh does instead of
# depending on the packaging order.
rm -rf "$PACKAGE_ROOT/DEBIAN"

TARBALL="$OUTPUT/ghostex-${VERSION}-linux-x64.tar.zst"
# Deliberately outside $OUTPUT: that directory is uploaded verbatim as the
# release-linux-tar-x64 artifact and must contain only the archive, the
# manifest, the metadata, and the provenance record.
FILE_LIST="$(mktemp)"
trap 'rm -f "$FILE_LIST"' EXIT

# Deterministic in the same spirit as create-deterministic-tar.sh: sorted member
# list, root:0 ownership, one normalized mtime. Two deliberate differences:
#
# 1. Directory entries are kept, because this archive is extracted onto a real
#    filesystem root rather than unpacked into an already-created staging tree.
# 2. mtimes are normalized through `tar --mtime` instead of `touch -h` on the
#    source, because linux-deb.sh and linux-rpm.sh package this same staged root
#    and must not observe a tree the tar builder rewrote underneath them.
#
# `-h` is deliberately absent: usr/bin/gx is a symlink to ghostex and must stay
# one in the archive. `--no-recursion` is required because the explicit member
# list already contains every directory.
(
	cd "$PACKAGE_ROOT"
	find . -mindepth 1 -print0 | LC_ALL=C sort -z >"$FILE_LIST"
	tar --format=gnu \
		--owner=0 --group=0 --numeric-owner \
		--mtime=@946684800 \
		--no-recursion --null --files-from "$FILE_LIST" -cf - |
		zstd -19 -T0 -q -f -o "$TARBALL" -
)

MEMBERS="$(zstd -dc "$TARBALL" | tar -tf -)"
for required in ./opt/ghostex/Ghostex ./usr/bin/ghostex ./usr/bin/gx \
	./usr/share/applications/ghostex.desktop ./usr/share/icons/hicolor/256x256/apps/ghostex.png; do
	grep -qxF "$required" <<<"$MEMBERS" || {
		echo "Linux tarball is missing $required" >&2
		exit 1
	}
done
if grep -q '^\./DEBIAN' <<<"$MEMBERS"; then
	echo "Linux tarball leaked DEBIAN/ control metadata" >&2
	exit 1
fi
# The wrapper in usr/bin/ghostex hardcodes /opt/ghostex, so the prefix has to
# survive extraction, and gx must arrive as a symlink rather than a second copy.
DETAILED="$(zstd -dc "$TARBALL" | tar -tvf -)"
grep -qE '^l.* \./usr/bin/gx -> ghostex$' <<<"$DETAILED" || {
	echo "Linux tarball did not preserve usr/bin/gx as a symlink" >&2
	exit 1
}

release_gpui_write_manifest "$OUTPUT" linux-tar-x64 "$VERSION" "$TARBALL"
printf 'Built portable Linux x64 tarball in %s\n' "$OUTPUT"
