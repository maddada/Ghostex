#!/usr/bin/env bash
set -euo pipefail

# Builds the two Rust binaries the macOS bundle ships: the app itself and the
# CEF helper. build-macos-app.sh runs this in the foreground, and
# `cargo xtask start` runs it in the background while
# prepare-macos-runtime.sh builds gxserver, then hands the result to
# build-macos-app.sh through GHOSTEX_GPUI_USE_PREBUILT_RUST=1. Keeping the
# cargo invocation in one file means both callers compile with identical
# flags and environment, so neither invalidates the other's artifacts.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GPUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# cef-dll-sys resolves its CEF distribution from CEF_PATH. The bundle packager
# stages the framework from this same cache, so the two must agree: both use
# the pinned build's versioned folder (see cef-distribution.sh).
export CEF_PATH="$GPUI_DIR/build/cef-cache"
# shellcheck source=cef-distribution.sh
source "$SCRIPT_DIR/cef-distribution.sh"
ghostex_prepare_versioned_cef_path "$CEF_PATH" >/dev/null

# CDXC:Build 2026-09-04 WHY:
# `--bins` also compiled the three smoke/demo binaries and the Windows
# installer on every macOS start, and terminal-element-demo re-linked whenever
# terminal_element.rs changed. The macOS bundle only ships these two.
cargo_args=(build --release --bin ghostex-gpui --bin ghostex-gpui-cef-helper)

# CDXC:Build 2026-09-04 WHY:
# The desktop crate is one 170k-line leaf crate, so every local start paid a
# full non-incremental release compile of it (~27s) even for a one-line edit.
# A per-package profile override turns incremental codegen on for that crate
# only: dependencies keep their profile hash and stay served by kache, and
# release packaging (no GHOSTEX_LOCAL_START) keeps the plain release profile.
# Do not move this into Cargo.toml or the release build inherits it.
if [[ "${GHOSTEX_LOCAL_START:-0}" == "1" ]]; then
	cargo_args+=(--config 'profile.release.package.ghostex-gpui.incremental=true')
	# CDXC:Build 2026-09-23 DECISION:
	# User chose opt-level 0 for the app crate on local starts, with `cargo xtask start --optimized` for performance work.
	# Measured on an M5 Pro: a one-line edit rebuilt in ~5s at opt-level 0, ~17s at 1 and ~20s at 3, and a cold compile used 71 vs 315 CPU-seconds.
	# Only this crate changes: GPUI, CEF, tree-sitter and every other dependency keep release optimization, and debug assertions and overflow checks stay off as in release.
	# SEE-ALSO: prepare-macos-runtime.sh applies the same rule to gxserver.
	if [[ "${GHOSTEX_START_OPTIMIZED:-0}" != "1" ]]; then
		cargo_args+=(--config 'profile.release.package.ghostex-gpui.opt-level=0')
	fi
fi

cd "$GPUI_DIR"
# The component signing key (release workflows set it on the steps that publish components) never
# reaches cargo, build scripts or the compiler wrapper.
unset GHOSTEX_COMPONENT_SIGNING_KEY
exec cargo "${cargo_args[@]}"
