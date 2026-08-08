#!/usr/bin/env bash
set -euo pipefail

# CDXC:ReleaseAutomation 2026-07-02-14:10:
# The 5.4.0 release lost most of its controllable time rediscovering the macOS
# cross-build recipe for the Ubuntu remote gxserver packages (Zig CC/AR
# wrappers, Rust-style --target argument stripping, split Zig toolchains for
# zmx/tui versus zehn). This script owns that recipe so releases run one
# deterministic command instead of hand-typing environment variables.
#
# Usage:
#   scripts/build-remote-gxserver-linux-release.sh [--arch x64|arm64|all] [--check-only] [--force] [--allow-dirty]
#
# Freshness contract: a package is current when its build-identity.json records
# sourceRevision == HEAD and sourceDirty == false and every required runtime
# resource exists. Current packages are not rebuilt unless --force is passed.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${GHOSTEX_RELEASE_REPO_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
AUTOMATION_ROOT="${GHOSTEX_RELEASE_AUTOMATION_ROOT:-$REPO_ROOT}"
PACKAGE_ROOT="$REPO_ROOT/build/remote-gxserver-linux"

ARCH_FILTER="all"
CHECK_ONLY=0
FORCE=0
ALLOW_DIRTY=0

usage() {
	cat <<'EOF'
Usage: scripts/build-remote-gxserver-linux-release.sh [options]

Options:
  --arch x64|arm64|all  Build one or both Ubuntu package architectures (default: all).
  --check-only          Report package freshness against HEAD and exit 0 (fresh) or 1 (stale).
  --force               Rebuild even when packages already match HEAD.
  --allow-dirty         Build from a dirty worktree (the release script will reject the result).
  --help                Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--arch)
			ARCH_FILTER="${2:-}"
			shift 2
			;;
		--check-only)
			CHECK_ONLY=1
			shift
			;;
		--force)
			FORCE=1
			shift
			;;
		--allow-dirty)
			ALLOW_DIRTY=1
			shift
			;;
		--help | -h)
			usage
			exit 0
			;;
		*)
			echo "Unknown option: $1" >&2
			usage >&2
			exit 2
			;;
	esac
done

case "$ARCH_FILTER" in
	x64 | arm64 | all) ;;
	*)
		echo "Unsupported --arch value: $ARCH_FILTER (expected x64, arm64, or all)" >&2
		exit 2
		;;
esac

ARCHES=()
if [[ "$ARCH_FILTER" == "all" ]]; then
	ARCHES=(x64 arm64)
else
	ARCHES=("$ARCH_FILTER")
fi

HEAD_REVISION="$(git -C "$REPO_ROOT" rev-parse HEAD)"

elapsed_since() {
	local started="$1"
	local now
	now="$(date +%s)"
	local total=$((now - started))
	printf '%dm %ds' $((total / 60)) $((total % 60))
}

# Freshness uses the same required-resource list the release script enforces so
# the two checks cannot drift apart.
package_status() {
	local arch="$1"
	local package_dir="$PACKAGE_ROOT/$arch/package"
	GHOSTEX_RELEASE_MODULE="$AUTOMATION_ROOT/scripts/release-ghostex.mjs" \
		GHOSTEX_BEADS_RELEASE_MODULE="$REPO_ROOT/scripts/beads-release.mjs" \
		GHOSTEX_PACKAGE_DIR="$package_dir" \
		GHOSTEX_EXPECTED_REVISION="$HEAD_REVISION" \
		node --input-type=module --eval '
		import { readFileSync, existsSync } from "node:fs";
		import { pathToFileURL } from "node:url";
		const packageDir = process.env.GHOSTEX_PACKAGE_DIR;
		const expectedRevision = process.env.GHOSTEX_EXPECTED_REVISION;
		const { missingRemoteGxserverLinuxPackageResources } = await import(
			pathToFileURL(process.env.GHOSTEX_RELEASE_MODULE).href
		);
		const { BEADS_PACKAGE_ID } = await import(
			pathToFileURL(process.env.GHOSTEX_BEADS_RELEASE_MODULE).href
		);
		const identityPath = packageDir + "/build-identity.json";
		if (!existsSync(identityPath)) {
			console.log("missing build-identity.json");
			process.exit(1);
		}
		const missing = missingRemoteGxserverLinuxPackageResources(packageDir);
		if (missing.length > 0) {
			console.log("missing resources: " + missing.join(", "));
			process.exit(1);
		}
		const identity = JSON.parse(readFileSync(identityPath, "utf8"));
		if (identity.sourceDirty) {
			console.log("built from a dirty worktree");
			process.exit(1);
		}
		if (identity.sourceRevision !== expectedRevision) {
			console.log("built from " + identity.sourceRevision + " but HEAD is " + expectedRevision);
			process.exit(1);
		}
		if (identity.beadsVersion !== BEADS_PACKAGE_ID) {
			console.log("bundles Beads " + (identity.beadsVersion || "unknown") + " but expected " + BEADS_PACKAGE_ID);
			process.exit(1);
		}
		console.log("fresh (" + identity.sourceRevision + ")");
	'
}

report_freshness() {
	local stale=0
	local arch status
	for arch in "${ARCHES[@]}"; do
		if status="$(package_status "$arch")"; then
			echo "LINUX_${arch}: up to date - $status"
		else
			echo "LINUX_${arch}: stale - $status"
			stale=1
		fi
	done
	return "$stale"
}

if [[ "$CHECK_ONLY" == "1" ]]; then
	if report_freshness; then
		exit 0
	fi
	exit 1
fi

if [[ "$FORCE" != "1" ]] && report_freshness >/dev/null 2>&1; then
	report_freshness
	if [[ "${GHOSTEX_REQUIRE_BEADS_SMOKE:-0}" == "1" ]]; then
		host_arch="$(uname -m)"
		for arch in "${ARCHES[@]}"; do
			case "$arch:$host_arch" in
				x64:x86_64 | arm64:aarch64 | arm64:arm64) ;;
				*)
					echo "GHOSTEX_REQUIRE_BEADS_SMOKE=1 requires a native Linux $arch runner; current host is $(uname -s)/$host_arch." >&2
					exit 1
					;;
			esac
			node "$REPO_ROOT/scripts/smoke-test-packaged-beads.mjs" "$PACKAGE_ROOT/$arch/package/bin/bd"
		done
	fi
	echo "Remote Linux gxserver packages already match HEAD ($HEAD_REVISION); nothing to build."
	exit 0
fi

if [[ "$ALLOW_DIRTY" != "1" && -n "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=all)" ]]; then
	cat >&2 <<EOF
Worktree is dirty. Packages built now would record sourceDirty=true and the
release script would reject them. Commit or push release-bound work first, or
pass --allow-dirty for a non-release debugging build.
EOF
	exit 1
fi

zig_version_of() {
	"$1" version 2>/dev/null || true
}

# zmx and the TUI vendor tree require Zig 0.15.x while zehn requires Zig
# 0.16+. Resolve both toolchains explicitly instead of trusting PATH order.
resolve_zig_015() {
	local candidate
	for candidate in \
		"${ZMX_ZIG:-}" \
		"$HOME/.local/share/mise/installs/zig/0.15.2/bin/zig" \
		"$HOME/.local/share/mise/installs/zig/0.15"*/bin/zig \
		"$(command -v zig || true)"; do
		[[ -n "$candidate" && -x "$candidate" ]] || continue
		case "$(zig_version_of "$candidate")" in
			0.15.*)
				printf '%s\n' "$candidate"
				return 0
				;;
		esac
	done
	return 1
}

resolve_zig_016() {
	local candidate
	for candidate in \
		"${ZEHN_ZIG:-}" \
		/opt/homebrew/bin/zig \
		"$HOME/.local/share/mise/installs/zig/0.16"*/bin/zig \
		"$(command -v zig || true)"; do
		[[ -n "$candidate" && -x "$candidate" ]] || continue
		case "$(zig_version_of "$candidate")" in
			0.16.* | 0.17.* | 0.18.*)
				printf '%s\n' "$candidate"
				return 0
				;;
		esac
	done
	return 1
}

if ! ZIG_015="$(resolve_zig_015)"; then
	cat >&2 <<'EOF'
Could not find Zig 0.15.x for zmx/TUI cross builds.

Install it with mise, or point ZMX_ZIG at a Zig 0.15 binary:
  mise install zig@0.15.2
EOF
	exit 1
fi
if ! ZIG_016="$(resolve_zig_016)"; then
	cat >&2 <<'EOF'
Could not find Zig 0.16+ for zehn cross builds.

Install it with Homebrew, or point ZEHN_ZIG at a Zig 0.16 binary:
  brew install zig
EOF
	exit 1
fi

if command -v rustup >/dev/null 2>&1; then
	for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
		if ! rustup target list --installed 2>/dev/null | grep -Fxq "$target"; then
			cat >&2 <<EOF
Rust target $target is not installed.

Fix:
  rustup target add $target
EOF
			exit 1
		fi
	done
fi

WRAPPER_DIR="$(mktemp -d /tmp/ghostex-linux-cross-cc-XXXXXX)"
trap 'rm -rf "$WRAPPER_DIR"' EXIT

RUST_HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
RUST_LLD="$(rustc --print sysroot)/lib/rustlib/$RUST_HOST_TRIPLE/bin/rust-lld"
if [[ ! -x "$RUST_LLD" ]]; then
	echo "Rust LLD linker is missing: $RUST_LLD" >&2
	exit 1
fi

# Zig's CLI rejects the Rust-style --target=<triple> arguments that cargo and
# cc crates emit, and macOS `ar` produces unusable Linux static archives, so
# both roles run through Zig with the triple arguments stripped/translated.
write_cc_wrapper() {
	local wrapper_path="$1"
	local rust_triple="$2"
	local zig_triple="$3"
	cat >"$wrapper_path" <<EOF
#!/bin/bash
args=()
for arg in "\$@"; do
  case "\$arg" in
    --target=$rust_triple|-target|$rust_triple) ;;
    *) args+=("\$arg") ;;
  esac
done
exec "$ZIG_016" cc -target $zig_triple "\${args[@]}"
EOF
	chmod 755 "$wrapper_path"
}

# musl triples keep the Rust binaries fully static so remote hosts need no
# specific glibc/libstdc++ floor.
write_cc_wrapper "$WRAPPER_DIR/x86_64-linux-musl-cc" "x86_64-unknown-linux-musl" "x86_64-linux-musl"
write_cc_wrapper "$WRAPPER_DIR/aarch64-linux-musl-cc" "aarch64-unknown-linux-musl" "aarch64-linux-musl"

cat >"$WRAPPER_DIR/zig-ar" <<EOF
#!/bin/sh
exec "$ZIG_016" ar "\$@"
EOF
chmod 755 "$WRAPPER_DIR/zig-ar"

echo "Zig 0.15 (zmx/TUI): $ZIG_015 ($(zig_version_of "$ZIG_015"))"
echo "Zig 0.16 (zehn/cc): $ZIG_016 ($(zig_version_of "$ZIG_016"))"

build_arch() {
	local arch="$1"
	local started
	started="$(date +%s)"
	local cc_wrapper rust_triple env_suffix
	case "$arch" in
		x64)
			rust_triple="x86_64_unknown_linux_musl"
			cc_wrapper="$WRAPPER_DIR/x86_64-linux-musl-cc"
			env_suffix="X86_64_UNKNOWN_LINUX_MUSL"
			;;
		arm64)
			rust_triple="aarch64_unknown_linux_musl"
			cc_wrapper="$WRAPPER_DIR/aarch64-linux-musl-cc"
			env_suffix="AARCH64_UNKNOWN_LINUX_MUSL"
			;;
	esac

	echo "==> Building remote gxserver Linux $arch package"
	env \
		"CC_$rust_triple=$cc_wrapper" \
		"AR_$rust_triple=$WRAPPER_DIR/zig-ar" \
		"CARGO_TARGET_${env_suffix}_LINKER=$RUST_LLD" \
		"CARGO_TARGET_${env_suffix}_RUSTFLAGS=-C linker-flavor=ld.lld" \
		ZMX_ZIG="$ZIG_015" \
		TUI_ZIG="$ZIG_015" \
		ZEHN_ZIG="$ZIG_016" \
		node "$REPO_ROOT/gxserver-rs/package-remote-linux.mjs" --arch "$arch" --allow-cross

	if ! status="$(package_status "$arch")"; then
		echo "LINUX_$arch package failed validation after build: $status" >&2
		exit 1
	fi
	echo "==> LINUX_$arch package ready in $(elapsed_since "$started"): $status"
}

TOTAL_STARTED="$(date +%s)"
for arch in "${ARCHES[@]}"; do
	build_arch "$arch"
done

echo ""
echo "Remote Linux gxserver package fingerprints:"
for arch in "${ARCHES[@]}"; do
	identity="$PACKAGE_ROOT/$arch/package/build-identity.json"
	printf '  %s: %s\n' "$arch" "$(node -e 'const id=require(process.argv[1]); console.log(`${id.sourceRevision} dirty=${id.sourceDirty}`)' "$identity")"
done
echo "Total build time: $(elapsed_since "$TOTAL_STARTED")"
