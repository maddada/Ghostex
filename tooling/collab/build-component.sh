#!/usr/bin/env bash
# Builds one platform's Collab component archive: the `ghostex-collab` sidecar (server/collab) for
# that platform and `collab-component.json` naming it. Linux builds are static (musl), so one
# archive runs on every distribution gxserver runs on. On macOS with --sign-identity the binary is
# signed with the hardened runtime and notarised (--notary-profile, an `xcrun notarytool
# store-credentials` profile), like GBots' Node.
#
# Usage: tooling/collab/build-component.sh --platform <darwin-arm64|darwin-x64|linux-x64|linux-arm64|windows-x64|windows-arm64>
#        --out <dir> [--sign-identity <identity>] [--notary-profile <name>]
# Prints the archive path. Writes <archive>.sha256 beside it.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
platform=""
out=""
sign_identity=""
notary_profile=""
while [ $# -gt 0 ]; do
  case "$1" in
    --platform) platform="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --sign-identity) sign_identity="$2"; shift 2 ;;
    --notary-profile) notary_profile="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$platform" ] && [ -n "$out" ] || { echo "--platform and --out are required" >&2; exit 2; }

binary=ghostex-collab
tar_mode=()
case "$platform" in
  darwin-arm64) target=aarch64-apple-darwin ;;
  darwin-x64) target=x86_64-apple-darwin ;;
  linux-x64) target=x86_64-unknown-linux-musl ;;
  linux-arm64) target=aarch64-unknown-linux-musl ;;
  windows-x64) target=x86_64-pc-windows-msvc; binary=ghostex-collab.exe; tar_mode=(--windows-component) ;;
  windows-arm64) target=aarch64-pc-windows-msvc; binary=ghostex-collab.exe; tar_mode=(--windows-component) ;;
  *) echo "unknown platform: $platform" >&2; exit 2 ;;
esac

component_version="$(node "$root/tooling/collab/component-identity.mjs")"
archive_name="collab-$component_version-$platform.tar.gz"
protocol="$(sed -n 's/^pub const COLLAB_PROTOCOL_VERSION: u32 = \([0-9]*\);/\1/p' "$root/packages/gx-protocol/src/collab_component.rs")"
[ -n "$protocol" ] || { echo "no COLLAB_PROTOCOL_VERSION in collab_component.rs" >&2; exit 1; }

target_dir="${CARGO_TARGET_DIR:-$root/build/collab/target}"
(
  cd "$root/server/collab"
  rustup target list --installed | grep -Fxq "$target" || rustup target add "$target"
  case "$target" in
    *-linux-musl)
      # Static, with musl's own C compiler for ring's assembly (apt: musl-tools).
      export CC="${CC:-musl-gcc}"
      export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
      ;;
  esac
  CARGO_TARGET_DIR="$target_dir" cargo build --release --locked --target "$target"
)
built="$target_dir/$target/release/$binary"
[ -f "$built" ] || { echo "cargo built no $built" >&2; exit 1; }

work="$root/build/collab/component/$platform"
stage="$work/stage"
rm -rf "$work"
mkdir -p "$stage"
cp "$built" "$stage/$binary"
chmod 755 "$stage/$binary"
cp "$root/LICENSE" "$stage/LICENSE"
node -e '
const fs = require("fs");
const [file, componentVersion, platform, binary, protocol] = process.argv.slice(1);
fs.writeFileSync(file, JSON.stringify({ componentVersion, platform, binary, protocol: Number(protocol) }, null, 2) + "\n");
' "$stage/collab-component.json" "$component_version" "$platform" "$binary" "$protocol"

if [[ "$platform" == darwin-* && -n "$sign_identity" ]]; then
  codesign --force --options runtime --timestamp \
    --entitlements "$root/tooling/collab/collab.entitlements" \
    --sign "$sign_identity" "$stage/$binary"
  codesign --verify --strict --verbose=2 "$stage/$binary"
  if [ -n "$notary_profile" ]; then
    ditto -c -k --keepParent "$stage/$binary" "$work/collab-notarize.zip"
    xcrun notarytool submit "$work/collab-notarize.zip" --keychain-profile "$notary_profile" --wait
  fi
fi

# The archive runs on the platform it names; smoke-test it when that is this machine.
host_os="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$host_os" in mingw* | msys* | cygwin*) host_os=windows ;; esac
host_arch="$(uname -m)"
case "$host_arch" in x86_64 | amd64) host_arch=x64 ;; aarch64 | arm64) host_arch=arm64 ;; esac
if [ "$host_os-$host_arch" = "$platform" ]; then
  "$stage/$binary" --version
fi

mkdir -p "$out"
"$root/tooling/release-gpui/create-deterministic-tar.sh" "$stage" "$out/$archive_name" ${tar_mode[@]+"${tar_mode[@]}"}
if command -v sha256sum >/dev/null 2>&1; then
  sha="$(sha256sum "$out/$archive_name" | awk '{print $1}')"
else
  sha="$(shasum -a 256 "$out/$archive_name" | awk '{print $1}')"
fi
printf '%s  %s\n' "$sha" "$archive_name" >"$out/$archive_name.sha256"
echo "$out/$archive_name"
