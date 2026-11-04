# shellcheck shell=bash
# Sourced by the build scripts that run Zig. Defines ghostex_find_zig.
#
# CDXC:Build 2026-10-26 WHY: every build path resolves Zig the same way, so a plain
# `cargo xtask start` finds the required Zig on a machine whose PATH `zig` is another
# version (Homebrew kept 0.16 while the repo moved to 0.17) and never builds with the
# wrong one. The required series is the vendored Ghostty's `.minimum_zig_version`
# (major.minor), so the next Zig move needs no edit here. Order: GHOSTEX_ZIG, then ZIG
# (either one, when set, must be the required series or the build fails), then PATH
# `zig`, Homebrew (zig@<series>, then zig), /usr/local, mise installs, ~/apps tarballs
# and the repo's build/toolchains download. No match fails with an install hint.
# Homebrew comes before mise on purpose: on macOS 27 (2026-07-11) the mise tarball
# build intermittently failed executable links against the Xcode 27 SDK with every
# libSystem symbol undefined, while Homebrew's build of the same version linked.
# SEE-ALSO: apps/desktop/build.rs (find_zig), tooling/xtask/src/util.rs (find_zig),
# server/package-remote-linux.mjs (resolveZigBinary).

ghostex_required_zig_series() {
	local repo_root="$1"
	sed -n -E 's/^[[:space:]]*\.minimum_zig_version[[:space:]]*=[[:space:]]*"([0-9]+\.[0-9]+)\..*/\1/p' \
		"$repo_root/.dependencies/ghostty/build.zig.zon" | head -n 1
}

ghostex_zig_is_series() {
	local candidate="$1" series="$2" version
	[[ -n "$candidate" && -x "$candidate" && ! -d "$candidate" ]] || return 1
	version="$("$candidate" version 2>/dev/null | tr -d '\r')" || return 1
	[[ "$version" == "$series".* ]]
}

# Prints the Zig binary to use, or explains what is missing on stderr and returns 1.
ghostex_find_zig() {
	local repo_root="$1" series
	series="$(ghostex_required_zig_series "$repo_root")"
	if [[ -z "$series" ]]; then
		echo "Could not read .minimum_zig_version from $repo_root/.dependencies/ghostty/build.zig.zon." >&2
		return 1
	fi
	local name value
	for name in GHOSTEX_ZIG ZIG; do
		value="${!name:-}"
		[[ -n "$value" ]] || continue
		if ghostex_zig_is_series "$value" "$series"; then
			printf '%s\n' "$value"
			return 0
		fi
		echo "$name=$value is not a Zig $series.x binary (it reports: $("$value" version 2>/dev/null || echo unavailable))." >&2
		echo "Point $name at Zig $series.x, or unset it so the build finds one." >&2
		return 1
	done
	local mise_data="${MISE_DATA_DIR:-$HOME/.local/share/mise}"
	local candidate
	for candidate in \
		"$(command -v zig 2>/dev/null || true)" \
		"/opt/homebrew/opt/zig@$series/bin/zig" \
		/opt/homebrew/bin/zig \
		"/usr/local/opt/zig@$series/bin/zig" \
		/usr/local/bin/zig \
		"$mise_data/installs/zig/$series".*/bin/zig \
		"$mise_data/installs/zig/$series".*/zig \
		"$HOME/apps/zig-"*"-$series".*/zig \
		"$repo_root/build/toolchains/zig-$series".*/zig \
		"$repo_root/build/toolchains/zig-$series".*/zig.exe; do
		if ghostex_zig_is_series "$candidate" "$series"; then
			printf '%s\n' "$candidate"
			return 0
		fi
	done
	cat >&2 <<EOF
Zig $series.x is required (the vendored Ghostty's minimum_zig_version), and none was found.
Checked GHOSTEX_ZIG, ZIG, PATH (zig: $(command -v zig 2>/dev/null || echo none)), Homebrew, mise and build/toolchains.
Install it with one of:
  mise install zig@$series.0
  brew install zig          # when Homebrew's zig is $series
or set GHOSTEX_ZIG to a Zig $series.x binary.
EOF
	return 1
}
