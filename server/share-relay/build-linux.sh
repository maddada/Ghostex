#!/usr/bin/env bash
# Builds ghostex-share-relay for Linux (static musl, x64 and arm64) from macOS or Linux, and packs
# each with its sha256 into dist/: ghostex-share-relay-<version>-linux-<arch>.tar.gz.
# The relay has no C dependencies, so Rust's own rust-lld links it; no Zig or cross image needed.
#
# The guest page (apps/gpui-web/www/dist, from `cargo xtask web-build`) is packed as one folder
# named by its page hash, ready to unpack into the relay's --pages folder, with its manifest
# beside it; the manifest is also copied to server/src/collab/guest_page.manifest, which is what
# a Ghostex release pins (commit it together with deploying the page).
#
# Usage: server/share-relay/build-linux.sh [x64|arm64|all|web]   (default: all)
#        CARGO_TARGET_DIR is honoured.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
ARCH="${1:-all}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
SYSROOT="$(rustc --print sysroot)"
HOST="$(rustc -vV | sed -n 's/^host: //p')"
LLD="$SYSROOT/lib/rustlib/$HOST/bin/rust-lld"
[[ -x "$LLD" ]] || { echo "rust-lld not found at $LLD" >&2; exit 1; }
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
mkdir -p dist
build() {
	local arch="$1" target env
	case "$arch" in
	x64) target=x86_64-unknown-linux-musl env=X86_64_UNKNOWN_LINUX_MUSL ;;
	arm64) target=aarch64-unknown-linux-musl env=AARCH64_UNKNOWN_LINUX_MUSL ;;
	*) echo "unknown arch $arch" >&2; exit 2 ;;
	esac
	rustup target list --installed | grep -Fxq "$target" || rustup target add "$target"
	env "CARGO_TARGET_${env}_LINKER=$LLD" "CARGO_TARGET_${env}_RUSTFLAGS=-C linker-flavor=ld.lld -C target-feature=+crt-static" \
		cargo build --release --target "$target"
	local name="ghostex-share-relay-$VERSION-linux-$arch"
	local stage="dist/$name"
	rm -rf "$stage" && mkdir -p "$stage"
	cp "$TARGET_DIR/$target/release/ghostex-share-relay" "$stage/"
	cp deploy/ghostex-share-relay.service deploy/Caddyfile README.md "$stage/"
	tar -C dist -czf "dist/$name.tar.gz" "$name"
	rm -rf "$stage"
	(cd dist && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256")
	echo "built dist/$name.tar.gz"
}
case "$ARCH" in
all) build x64; build arm64 ;;
web) ;;
*) build "$ARCH" ;;
esac
# The guest page, when the web build is there: `<page hash>/…`, where the page hash is the SHA-256
# of the manifest (every file's SHA-256 and path, sorted by path, dot files left out).
WEB="../../apps/gpui-web/www/dist"
if [[ -f "$WEB/index.html" ]]; then
	MANIFEST="dist/ghostex-share-relay-web-$VERSION.manifest"
	(cd "$WEB" && find . -type f -not -path '*/.*' | sed 's|^\./||' | LC_ALL=C sort | while IFS= read -r file; do shasum -a 256 "$file"; done) > "$MANIFEST"
	PAGE_HASH="$(shasum -a 256 < "$MANIFEST" | cut -d' ' -f1)"
	STAGE="dist/web-stage"
	rm -rf "$STAGE" && mkdir -p "$STAGE/$PAGE_HASH"
	(cd "$WEB" && find . -type f -not -path '*/.*' -print0 | cpio -0pdm --quiet "$OLDPWD/$STAGE/$PAGE_HASH")
	COPYFILE_DISABLE=1 tar -C "$STAGE" -czf "dist/ghostex-share-relay-web-$VERSION.tar.gz" "$PAGE_HASH"
	rm -rf "$STAGE"
	(cd dist && shasum -a 256 "ghostex-share-relay-web-$VERSION.tar.gz" > "ghostex-share-relay-web-$VERSION.tar.gz.sha256")
	cp "$MANIFEST" ../src/collab/guest_page.manifest
	echo "built dist/ghostex-share-relay-web-$VERSION.tar.gz: page $PAGE_HASH (id ${PAGE_HASH:0:16}); server/src/collab/guest_page.manifest updated"
else
	echo "no guest page at $WEB: run cargo xtask web-build first" >&2
fi
