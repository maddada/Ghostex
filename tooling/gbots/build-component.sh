#!/usr/bin/env bash
# Builds one platform's GBots component archive: the runtime build-runtime.sh makes, the pinned
# Node 24 for that platform (checked against upstream.json), and gbots-runtime.json naming both.
# On macOS with --sign-identity the Node is signed with the hardened runtime and notarised
# (--notary-profile, an `xcrun notarytool store-credentials` profile), like the Code editor's node.
#
# Usage: tooling/gbots/build-component.sh --platform darwin-arm64|linux-x64|linux-arm64 --out <dir>
#        [--source <git url or clone>] [--sign-identity <identity>] [--notary-profile <name>]
# Prints the archive path. Writes <archive>.sha256 beside it.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
pin="$root/tooling/gbots/upstream.json"
platform=""
out=""
source_args=()
sign_identity=""
notary_profile=""
while [ $# -gt 0 ]; do
  case "$1" in
    --platform) platform="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --source) source_args=(--source "$2"); shift 2 ;;
    --sign-identity) sign_identity="$2"; shift 2 ;;
    --notary-profile) notary_profile="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$platform" ] && [ -n "$out" ] || { echo "--platform and --out are required" >&2; exit 2; }

node_version="$(node -e 'process.stdout.write(require(process.argv[1]).node.version)' "$pin")"
node_sha="$(node -e 'const p=require(process.argv[1]).node.platforms[process.argv[2]]; if(!p) process.exit(1); process.stdout.write(p)' "$pin" "$platform")" || {
  echo "upstream.json pins no Node for $platform" >&2
  exit 2
}
component_version="$(node "$root/tooling/gbots/component-identity.mjs")"
archive_name="gbots-$component_version-$platform.tar.gz"

runtime="$("$root/tooling/gbots/build-runtime.sh" ${source_args[@]+"${source_args[@]}"} | tail -n 1)"
[ -f "$runtime/gbots-runtime.json" ] || { echo "the runtime build printed no runtime" >&2; exit 1; }

work="$root/build/gbots/component/$platform"
stage="$work/stage"
rm -rf "$work"
mkdir -p "$stage" "$work/node-download"
cp -R "$runtime/." "$stage/"

node_tarball="node-v$node_version-$platform.tar.gz"
node_cache="$root/build/gbots/node-cache"
mkdir -p "$node_cache"
if [ ! -f "$node_cache/$node_tarball" ]; then
  curl -fsSL --retry 3 -o "$node_cache/$node_tarball.part" "https://nodejs.org/dist/v$node_version/$node_tarball"
  mv "$node_cache/$node_tarball.part" "$node_cache/$node_tarball"
fi
actual_sha="$(shasum -a 256 "$node_cache/$node_tarball" | awk '{print $1}')"
if [ "$actual_sha" != "$node_sha" ]; then
  rm -f "$node_cache/$node_tarball"
  echo "Node $node_version for $platform does not match its pinned checksum ($actual_sha)" >&2
  exit 1
fi
tar -xzf "$node_cache/$node_tarball" -C "$work/node-download"
node_root="$work/node-download/node-v$node_version-$platform"
mkdir -p "$stage/node/bin"
cp "$node_root/bin/node" "$stage/node/bin/node"
cp "$node_root/LICENSE" "$stage/node/LICENSE"
chmod 755 "$stage/node/bin/node"

node -e '
const fs = require("fs");
const [file, componentVersion, nodeVersion, platform] = process.argv.slice(1);
const manifest = JSON.parse(fs.readFileSync(file, "utf8"));
delete manifest.builtAt;
Object.assign(manifest, { componentVersion, platform, node: "node/bin/node", nodeVersion });
fs.writeFileSync(file, JSON.stringify(manifest, null, 2) + "\n");
' "$stage/gbots-runtime.json" "$component_version" "$node_version" "$platform"

if [[ "$platform" == darwin-* && -n "$sign_identity" ]]; then
  codesign --force --options runtime --timestamp \
    --entitlements "$root/tooling/gbots/node.entitlements" \
    --sign "$sign_identity" "$stage/node/bin/node"
  codesign --verify --strict --verbose=2 "$stage/node/bin/node"
  if [ -n "$notary_profile" ]; then
    ditto -c -k --keepParent "$stage/node/bin/node" "$work/node-notarize.zip"
    xcrun notarytool submit "$work/node-notarize.zip" --keychain-profile "$notary_profile" --wait
  fi
fi

# The archive runs on the platform it names; smoke-test it when that is this machine.
host_os="$(uname -s | tr '[:upper:]' '[:lower:]')"
host_arch="$(uname -m)"
case "$host_arch" in x86_64) host_arch=x64 ;; aarch64) host_arch=arm64 ;; esac
if [ "$host_os-$host_arch" = "$platform" ]; then
  "$stage/node/bin/node" "$stage/server/openmausbot.js" --help >/dev/null
fi

mkdir -p "$out"
"$root/tooling/release-gpui/create-deterministic-tar.sh" "$stage" "$out/$archive_name"
sha="$(shasum -a 256 "$out/$archive_name" | awk '{print $1}')"
printf '%s  %s\n' "$sha" "$archive_name" >"$out/$archive_name.sha256"
echo "$out/$archive_name"
