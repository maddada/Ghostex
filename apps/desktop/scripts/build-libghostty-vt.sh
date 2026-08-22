#!/usr/bin/env bash
set -euo pipefail

# CDXC:GPUILibghosttyVt 2026-07-03:
# Phase 1 of the GPUI cross-platform plan renders terminals as GPUI elements
# driven by libghostty-vt, so cargo builds must produce the static archive
# from the vendored Ghostty tree instead of depending on a manually built
# artifact. apps/desktop/build.rs invokes this script with an install prefix inside
# OUT_DIR and links {prefix}/lib/libghostty-vt.a directly, mirroring how the
# GhosttyKit archive is linked.
#
# The vendored Ghostty build pins Zig 0.16.x (build.zig.zon minimum_zig_version),
# while
# the machine default `zig` may be newer. Resolve a usable Zig explicitly:
# GHOSTEX_ZIG override first, then PATH, then mise installs. Do not silently
# build with a mismatched Zig; requireZig would fail anyway, so fail with a
# clear message instead.
#
# CDXC:iOSNativeTerminals 2026-05-22-11:17 (workaround retained from the
# now-archived iOS terminal build):
# Xcode 26's macOS SDK exposes libSystem as arm64e-only in the TBD stub,
# which Zig 0.15.x cannot use for native aarch64 links (the libghostty-vt
# shared library link fails with undefined libc symbols). Redirect only
# macosx SDK discovery to the newest Command Line Tools SDK that still
# exports arm64 while leaving other xcrun queries on the default toolchain.
#
# CDXC:GPUILibghosttyVt 2026-07-11:
# The redirect exists only for SDKs whose libSystem TBD lacks plain arm64
# exports. Newer toolchains (Xcode 27 beta) export arm64-macos again, and
# machines without Command Line Tools have no redirect target at all, so
# probe the default xcrun macOS SDK first and use it unmodified when it
# already exports arm64; only fall back to the CLT redirect when the
# default SDK cannot link arm64.

if [[ $# -ne 1 ]]; then
  echo "usage: $(basename "$0") <install-prefix>" >&2
  exit 64
fi
PREFIX="$1"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GHOSTTY_DIR="$ROOT_DIR/.dependencies/ghostty"
REQUIRED_ZIG_MINOR="0.16"

zig_matches() {
  local candidate="$1"
  [[ -x "$candidate" ]] || return 1
  local version
  version="$("$candidate" version 2>/dev/null)" || return 1
  [[ "$version" == "$REQUIRED_ZIG_MINOR".* ]]
}

find_zig() {
  if [[ -n "${GHOSTEX_ZIG:-}" ]]; then
    if zig_matches "$GHOSTEX_ZIG"; then
      printf '%s\n' "$GHOSTEX_ZIG"
      return 0
    fi
    echo "GHOSTEX_ZIG ($GHOSTEX_ZIG) is not a Zig $REQUIRED_ZIG_MINOR.x binary." >&2
    return 1
  fi

  local path_zig
  if path_zig="$(command -v zig 2>/dev/null)" && zig_matches "$path_zig"; then
    printf '%s\n' "$path_zig"
    return 0
  fi

  # Homebrew's zig@0.15 is preferred over the mise tarball install: on
  # macOS 27 (2026-07-11) the mise-installed 0.15.2 intermittently fails
  # executable links against the Xcode 27 SDK with every libSystem symbol
  # undefined, while the Homebrew build of the same version links cleanly.
  local candidate
  for candidate in /opt/homebrew/opt/zig@"$REQUIRED_ZIG_MINOR"/bin/zig \
    "$HOME/.local/share/mise/installs/zig/$REQUIRED_ZIG_MINOR".*/bin/zig \
    "$HOME/.local/share/mise/installs/zig/$REQUIRED_ZIG_MINOR".*/zig; do
    if zig_matches "$candidate"; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  echo "No Zig $REQUIRED_ZIG_MINOR.x found. Install one (e.g. 'mise install zig@0.16.0') or set GHOSTEX_ZIG to a Zig $REQUIRED_ZIG_MINOR.x binary." >&2
  return 1
}

ZIG="$(find_zig)"
GHOSTTY_APP_VERSION="$(
  sed -n -E 's/^[[:space:]]*\.version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$GHOSTTY_DIR/build.zig.zon" \
    | head -n 1
)"
if [[ -z "$GHOSTTY_APP_VERSION" ]]; then
  echo "Could not resolve Ghostty app version from $GHOSTTY_DIR/build.zig.zon." >&2
  exit 1
fi

cd "$GHOSTTY_DIR"

# The SDK is usable for Zig arm64 links only when the MAIN libSystem TBD
# document (the first `targets:` line) declares plain arm64-macos. Newer
# SDKs keep arm64-macos in later sub-documents while the main document is
# arm64e-only, so a whole-file grep reports false positives.
sdk_exports_arm64() {
  local tbd="$1/usr/lib/libSystem.tbd"
  [[ -f "$tbd" ]] || return 1
  sed -n '/^targets:/{p;q;}' "$tbd" | grep -Eq '(^|[ \[,])arm64-macos[] ,]'
}

# CDXC:GPUILibghosttyVt 2026-07-11:
# When no SDK on the machine can link plain arm64 (Xcode 26+/27 SDKs are
# arm64e-only in the main libSystem document and Command Line Tools may be
# absent), synthesize an overlay SDK next to the install prefix: symlink the
# default SDK and rewrite only libSystem.tbd so every targets list that
# declares arm64e-macos also declares arm64-macos. The dylib symbol tables
# are identical for arm64/arm64e, so the patched stub links correctly.
synthesize_arm64_sdk_overlay() {
  local source_sdk="$1"
  local overlay_sdk="$2"
  rm -rf "$overlay_sdk"
  mkdir -p "$overlay_sdk/usr/lib"
  local entry name
  for entry in "$source_sdk"/*; do
    name="$(basename "$entry")"
    [[ "$name" == "usr" ]] && continue
    ln -s "$entry" "$overlay_sdk/$name"
  done
  for entry in "$source_sdk"/usr/*; do
    name="$(basename "$entry")"
    [[ "$name" == "lib" ]] && continue
    ln -s "$entry" "$overlay_sdk/usr/$name"
  done
  for entry in "$source_sdk"/usr/lib/*; do
    name="$(basename "$entry")"
    [[ "$name" == "libSystem.tbd" ]] && continue
    ln -s "$entry" "$overlay_sdk/usr/lib/$name"
  done
  sed -E '/(^|[ \[,])arm64-macos[] ,]/! s/arm64e-macos/arm64-macos, arm64e-macos/g' \
    "$source_sdk/usr/lib/libSystem.tbd" > "$overlay_sdk/usr/lib/libSystem.tbd"
}

if [[ "$(uname)" == "Darwin" ]] && ! sdk_exports_arm64 "$(/usr/bin/xcrun --sdk macosx --show-sdk-path 2>/dev/null)"; then
  WRAPPER_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ghostex-xcrun.XXXXXX")"
  trap 'rm -rf "$WRAPPER_DIR"' EXIT
  MACOS_SDK=""
  if [[ -d /Library/Developer/CommandLineTools/SDKs ]]; then
    MACOS_SDK="$(
      find /Library/Developer/CommandLineTools/SDKs -maxdepth 1 -type d -name 'MacOSX*.sdk' 2>/dev/null \
        | while IFS= read -r sdk; do
            if sdk_exports_arm64 "$sdk"; then
              printf '%s\n' "$sdk"
            fi
          done \
        | sort -Vr \
        | head -n 1
    )"
  fi
  if [[ -z "$MACOS_SDK" ]]; then
    DEFAULT_SDK="$(/usr/bin/xcrun --sdk macosx --show-sdk-path 2>/dev/null)"
    if [[ -z "$DEFAULT_SDK" || ! -f "$DEFAULT_SDK/usr/lib/libSystem.tbd" ]]; then
      echo "No macOS SDK with arm64 libSystem exports was found, and no default macOS SDK is available to synthesize one from." >&2
      exit 1
    fi
    OVERLAY_SDK="${PREFIX}-sdk-overlay/$(basename "$DEFAULT_SDK")"
    if [[ ! -f "$OVERLAY_SDK/usr/lib/libSystem.tbd" ]] \
      || [[ "$DEFAULT_SDK/usr/lib/libSystem.tbd" -nt "$OVERLAY_SDK/usr/lib/libSystem.tbd" ]]; then
      synthesize_arm64_sdk_overlay "$DEFAULT_SDK" "$OVERLAY_SDK"
    fi
    if ! sdk_exports_arm64 "$OVERLAY_SDK"; then
      echo "Failed to synthesize an arm64-capable SDK overlay from $DEFAULT_SDK." >&2
      exit 1
    fi
    MACOS_SDK="$OVERLAY_SDK"
  fi
  cat > "$WRAPPER_DIR/xcrun" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if [[ "\${1:-}" == "--sdk" && "\${2:-}" == "macosx" && "\${3:-}" == "--show-sdk-path" ]]; then
  echo "$MACOS_SDK"
  exit 0
fi
/usr/bin/xcrun "\$@"
EOF
  chmod +x "$WRAPPER_DIR/xcrun"
  export PATH="$WRAPPER_DIR:$PATH"
fi

# Ghostex patch (2026-07-11): cargo links only the STATIC libghostty-vt
# archive; skip the shared dylib emit. Zig 0.15 cannot link macOS dylibs
# against the Xcode 26+/27 SDKs (bundled-libcxx / arm64e stub issues), so the
# unconditional dylib emit failed the whole cargo build on machines without
# an older Command Line Tools SDK even though the static archive builds fine.
exec "$ZIG" build \
  -Dversion-string="$GHOSTTY_APP_VERSION" \
  -Demit-lib-vt=true \
  -Demit-lib-vt-shared=false \
  -Demit-xcframework=false \
  -Doptimize=ReleaseSafe \
  --prefix "$PREFIX"
