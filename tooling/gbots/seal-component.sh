#!/usr/bin/env bash
# Records the published GBots component for one platform into a components manifest, right before
# a release build seals its on-demand-resources.json. Runs only when GHOSTEX_GBOTS_SEAL=1 (the
# release workflows set it), so local builds never wait on GitHub and ship without GBots.
#
# Usage: tooling/gbots/seal-component.sh <platform> <components.json>
# Env:   GHOSTEX_GBOTS_SEAL=1                 turn it on
#        GHOSTEX_GBOTS_WAIT_SECONDS (1800)    how long to wait for release-gpui-gbots.yml to publish
#        GHOSTEX_GBOTS_OPTIONAL=1             a build-only run: skip GBots when it is not published
set -euo pipefail

[ "${GHOSTEX_GBOTS_SEAL:-0}" = 1 ] || exit 0
platform="$1"
manifest="$2"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
version="$(node "$root/tooling/gbots/component-identity.mjs")"
tag="gbots-$version"
asset="gbots-$version-$platform.tar.gz"
repo="$(node "$root/tooling/release-gpui/components-repo.mjs")"
wait_seconds="${GHOSTEX_GBOTS_WAIT_SECONDS:-1800}"
deadline=$((SECONDS + wait_seconds))
while true; do
  if gh release view "$tag" --repo "$repo" --json assets --jq '.assets[].name' 2>/dev/null | grep -Fxq "$asset.sha256"; then
    break
  fi
  if ((SECONDS >= deadline)); then
    if [ "${GHOSTEX_GBOTS_OPTIONAL:-0}" = 1 ]; then
      echo "GBots $tag/$asset is not published; this build ships without GBots."
      exit 0
    fi
    echo "GBots component $tag/$asset was not published within ${wait_seconds}s (release-gpui-gbots.yml)." >&2
    exit 1
  fi
  sleep 20
done
asset_dir="$root/build/on-demand-components/gbots-$platform"
mkdir -p "$asset_dir"
node "$root/tooling/release-gpui/publish-component.mjs" \
  --reuse-published \
  --metadata-only \
  --require-sha256-sidecars \
  --component gbots \
  --version "$version" \
  --platform "$platform" \
  --asset-dir "$asset_dir" \
  --output "$manifest"
echo "Sealed GBots $version for $platform."
