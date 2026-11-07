#!/usr/bin/env bash
# Builds ghostex-share-relay for Linux (static musl, x64 and arm64) from macOS or Linux, and packs
# each with its sha256 into dist/: ghostex-share-relay-<version>-linux-<arch>.tar.gz.
# The relay has no C dependencies, so Rust's own rust-lld links it; no Zig or cross image needed.
#
# Usage: server/share-relay/build-linux.sh [x64|arm64|all]   (default: all)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
ARCH="${1:-all}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
SYSROOT="$(rustc --print sysroot)"
HOST="$(rustc -vV | sed -n 's/^host: //p')"
LLD="$SYSROOT/lib/rustlib/$HOST/bin/rust-lld"
[[ -x "$LLD" ]] || { echo "rust-lld not found at $LLD" >&2; exit 1; }
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
	cp "target/$target/release/ghostex-share-relay" "$stage/"
	cp deploy/ghostex-share-relay.service deploy/Caddyfile README.md "$stage/"
	tar -C dist -czf "dist/$name.tar.gz" "$name"
	rm -rf "$stage"
	(cd dist && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256")
	echo "built dist/$name.tar.gz"
}
if [[ "$ARCH" == all ]]; then build x64; build arm64; else build "$ARCH"; fi
# The guest page, when the web build is there.
WEB="../../apps/gpui-web/www/dist"
if [[ -f "$WEB/index.html" ]]; then
	tar -C "$WEB" -czf "dist/ghostex-share-relay-web-$VERSION.tar.gz" .
	(cd dist && shasum -a 256 "ghostex-share-relay-web-$VERSION.tar.gz" > "ghostex-share-relay-web-$VERSION.tar.gz.sha256")
	echo "built dist/ghostex-share-relay-web-$VERSION.tar.gz"
else
	echo "no guest page at $WEB: run cargo xtask web-build first" >&2
fi
