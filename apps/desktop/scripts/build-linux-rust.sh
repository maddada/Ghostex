#!/usr/bin/env bash
set -euo pipefail

# Builds the Rust binaries a Linux `cargo xtask start` stages: the app and its
# CEF helper (plus the CEF bootstrap when GHOSTEX_ON_DEMAND_ASSETS=1). The start
# runs this in the background while server/package-remote-linux.mjs builds
# gxserver, then hands the result to build-linux-app.sh through
# GHOSTEX_GPUI_USE_PREBUILT_RUST=1; build-linux-app.sh also runs it itself when
# GHOSTEX_LOCAL_START=1 without a prebuilt result. Release packaging never
# comes here: build-linux-app.sh keeps its own `cargo build --release --bins`
# (tooling/release-workflow-assertions.mjs pins that text).
#
# CDXC:Build 2026-11-14 WHY: Linux local starts compiled the desktop crate as a
# full opt-level 3 release with every demo binary (about 185 s for a one-line
# edit on a Ryzen 9 5900XT). macOS local starts already built only the shipped
# binaries at opt-level 0 with incremental codegen (build-macos-rust.sh,
# CDXC:Build 2026-09-23); the same flags take that edit to about 20 s here.
# SEE-ALSO: build-macos-rust.sh, server/package-remote-linux.mjs
# (--local-start), tooling/xtask/src/start/mod.rs (KACHE_INCREMENTAL_CRATES,
# without which kache strips the incremental flag).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GPUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Must match build-linux-app.sh: cef-dll-sys reruns its build script (and
# downloads CEF into OUT_DIR) whenever CEF_PATH differs between builds.
export CEF_PATH="${CEF_PATH:-$GPUI_DIR/build/cef-cache}"
# shellcheck source=cef-distribution.sh
source "$SCRIPT_DIR/cef-distribution.sh"
ghostex_prepare_versioned_cef_path "$CEF_PATH" >/dev/null

cargo_args=(build --release --bin ghostex-gpui --bin ghostex-gpui-cef-helper)
if [[ "${GHOSTEX_ON_DEMAND_ASSETS:-0}" == "1" ]]; then
	cargo_args+=(--bin ghostex-gpui-cef-bootstrap)
fi
cargo_args+=(--config 'profile.release.package.ghostex-gpui.incremental=true')
# CDXC:Diagnostics 2026-11-19 SEE-ALSO: the dev-build freeze detector (src/freeze_detector/heartbeat.rs); only local starts turn the feature on, never `cargo xtask build` or a release.
cargo_args+=(--features freeze-detector)
if [[ "${GHOSTEX_START_OPTIMIZED:-0}" != "1" ]]; then
	cargo_args+=(--config 'profile.release.package.ghostex-gpui.opt-level=0')
fi

cd "$GPUI_DIR"
# The component signing key never reaches cargo, build scripts or the compiler wrapper.
unset GHOSTEX_COMPONENT_SIGNING_KEY
exec cargo "${cargo_args[@]}"
