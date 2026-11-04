#!/usr/bin/env bash
# Sourced by the desktop build scripts; defines helpers only.

# CDXC:Build 2026-09-27 WHY:
# cef-dll-sys uses CEF_PATH/<CEF build>/ when that folder exists, and otherwise accepts any OLDER distribution that sits directly in CEF_PATH, so a cache kept across a CEF bump silently links the new bindings against the old CEF.
# A cache can also hold several versions (and architectures) side by side, so a bare `find` for the framework or libcef stages whichever one it meets first.
# Create the versioned folder before cargo runs and stage only the distribution inside it: exactly the CEF build the pinned cef-rs expects.
GHOSTEX_CEF_RS_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.dependencies/cef-rs" && pwd)"

# The CEF build the pinned bindings expect (`156.0.15`): the `+` build metadata
# of the cef-rs workspace version, which is what cef-dll-sys downloads.
ghostex_pinned_cef_build() {
	local build
	build="$(sed -n 's/^version = "[^"+]*+\([^"]*\)"$/\1/p' "$GHOSTEX_CEF_RS_ROOT/Cargo.toml" | head -n 1)"
	if [[ -z "$build" ]]; then
		echo "Could not read the pinned CEF build from $GHOSTEX_CEF_RS_ROOT/Cargo.toml" >&2
		return 1
	fi
	printf '%s\n' "$build"
}

# Creates and prints CEF_PATH/<pinned CEF build>, the folder cef-dll-sys
# downloads into and links against. Call it before cargo builds.
ghostex_prepare_versioned_cef_path() {
	local cef_path="$1" build
	build="$(ghostex_pinned_cef_build)" || return 1
	mkdir -p "$cef_path/$build"
	printf '%s\n' "$cef_path/$build"
}

# Prints the distribution folder for one Rust target inside the versioned
# folder, named the way download-cef names it (cef_<os>_<arch>).
ghostex_cef_distribution_dir() {
	local cef_path="$1" rust_target="$2" build os arch
	build="$(ghostex_pinned_cef_build)" || return 1
	case "$rust_target" in
	aarch64-apple-darwin) os=macos arch=aarch64 ;;
	x86_64-apple-darwin) os=macos arch=x86_64 ;;
	x86_64-pc-windows-msvc) os=windows arch=x86_64 ;;
	aarch64-pc-windows-msvc) os=windows arch=aarch64 ;;
	x86_64-unknown-linux-gnu) os=linux arch=x86_64 ;;
	aarch64-unknown-linux-gnu) os=linux arch=aarch64 ;;
	*)
		echo "No CEF distribution layout is known for Rust target $rust_target" >&2
		return 1
		;;
	esac
	printf '%s\n' "$cef_path/$build/cef_${os}_${arch}"
}
