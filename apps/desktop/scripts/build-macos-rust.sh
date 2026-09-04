#!/usr/bin/env bash
set -euo pipefail

# Builds the two Rust binaries the macOS bundle ships: the app itself and the
# CEF helper. build-macos-app.sh runs this in the foreground, and
# tooling/start-gpui.mjs runs it in the background while
# prepare-macos-runtime.sh builds gxserver, then hands the result to
# build-macos-app.sh through GHOSTEX_GPUI_USE_PREBUILT_RUST=1. Keeping the
# cargo invocation in one file means both callers compile with identical
# flags and environment, so neither invalidates the other's artifacts.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GPUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# cef-dll-sys resolves its CEF distribution from CEF_PATH. The bundle packager
# stages the framework from this same cache, so the two must agree.
export CEF_PATH="$GPUI_DIR/build/cef-cache"

# CDXC:Build 2026-09-04 WHY:
# `--bins` also compiled the three smoke/demo binaries and the Windows
# installer on every macOS start, and terminal-element-demo re-linked whenever
# terminal_element.rs changed. The macOS bundle only ships these two.
cargo_args=(build --release --bin ghostex-gpui --bin ghostex-gpui-cef-helper)

# CDXC:Build 2026-09-04 WHY:
# The desktop crate is one 170k-line leaf crate, so every local start paid a
# full non-incremental release compile of it (~27s) even for a one-line edit.
# A per-package profile override turns incremental codegen on for that crate
# only: dependencies keep their profile hash and stay served by sccache, and
# release packaging (no GHOSTEX_LOCAL_START) keeps the plain release profile.
# Do not move this into Cargo.toml or the release build inherits it.
if [[ "${GHOSTEX_LOCAL_START:-0}" == "1" ]]; then
	cargo_args+=(--config 'profile.release.package.ghostex-gpui.incremental=true')
fi

cd "$GPUI_DIR"
exec cargo "${cargo_args[@]}"
