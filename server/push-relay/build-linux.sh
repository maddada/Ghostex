#!/usr/bin/env bash
# Builds ghostex-push-relay for Linux (static musl, x64 and arm64) from macOS or Linux, and packs each with
# its sha256 into dist/: ghostex-push-relay-<version>-linux-<arch>.tar.gz.
# Unlike the share relay it has C code (ring, under rustls, for TLS to Apple), so Zig compiles that part
# for the target; Rust's own rust-lld links. Set ZIG to pick a Zig binary (default: zig on PATH).
#
# Usage: server/push-relay/build-linux.sh [x64|arm64|all]   (default: all)
#        CARGO_TARGET_DIR is honoured.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
ARCH="${1:-all}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
SYSROOT="$(rustc --print sysroot)"
HOST="$(rustc -vV | sed -n 's/^host: //p')"
LLD="$SYSROOT/lib/rustlib/$HOST/bin/rust-lld"
[[ -x "$LLD" ]] || { echo "rust-lld not found at $LLD" >&2; exit 1; }
ZIG="${ZIG:-$(command -v zig || true)}"
[[ -n "$ZIG" ]] || { echo "zig not found: install it or set ZIG" >&2; exit 1; }
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
TOOLS="$TARGET_DIR/zig-wrappers"
mkdir -p dist "$TOOLS"
build() {
	local arch="$1" target env zig_target
	case "$arch" in
	x64) target=x86_64-unknown-linux-musl env=X86_64_UNKNOWN_LINUX_MUSL zig_target=x86_64-linux-musl ;;
	arm64) target=aarch64-unknown-linux-musl env=AARCH64_UNKNOWN_LINUX_MUSL zig_target=aarch64-linux-musl ;;
	*) echo "unknown arch $arch" >&2; exit 2 ;;
	esac
	rustup target list --installed | grep -Fxq "$target" || rustup target add "$target"
	# cc-rs adds its own --target=<rust triple>, which Zig cannot read: the wrapper drops it.
	cat > "$TOOLS/cc-$arch" <<WRAPPER
#!/bin/sh
for arg do
	shift
	case "\$arg" in --target=*) ;; *) set -- "\$@" "\$arg" ;; esac
done
exec "$ZIG" cc -target $zig_target "\$@"
WRAPPER
	printf '#!/bin/sh\nexec "%s" ar "$@"\n' "$ZIG" > "$TOOLS/ar"
	chmod +x "$TOOLS/cc-$arch" "$TOOLS/ar"
	local cc_var="CC_${target//-/_}" ar_var="AR_${target//-/_}"
	env "$cc_var=$PWD/$TOOLS/cc-$arch" "$ar_var=$PWD/$TOOLS/ar" \
		"CARGO_TARGET_${env}_LINKER=$LLD" "CARGO_TARGET_${env}_RUSTFLAGS=-C linker-flavor=ld.lld -C target-feature=+crt-static" \
		cargo build --release --target "$target"
	local name="ghostex-push-relay-$VERSION-linux-$arch"
	local stage="dist/$name"
	rm -rf "$stage" && mkdir -p "$stage"
	cp "$TARGET_DIR/$target/release/ghostex-push-relay" "$stage/"
	cp deploy/ghostex-push-relay.service deploy/Caddyfile deploy/apns.conf.example README.md "$stage/"
	COPYFILE_DISABLE=1 tar -C dist -czf "dist/$name.tar.gz" "$name"
	rm -rf "$stage"
	(cd dist && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256")
	echo "built dist/$name.tar.gz"
}
case "$ARCH" in
all) build x64; build arm64 ;;
*) build "$ARCH" ;;
esac
