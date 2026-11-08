#!/usr/bin/env bash
# Records the published Collab component for one platform into a components manifest, right before
# a release build seals its on-demand-resources.json. Runs only when GHOSTEX_COLLAB_SEAL=1 (the
# release workflows set it), so local builds never wait on GitHub and ship without Collab.
#
# Usage: tooling/collab/seal-component.sh <platform> <components.json>
# Env:   GHOSTEX_COLLAB_SEAL=1                 turn it on
#        GHOSTEX_COLLAB_WAIT_SECONDS (1800)    how long to wait for release-gpui-collab.yml to publish
#        GHOSTEX_COLLAB_OPTIONAL=1             a build-only run: skip Collab when it is not published
set -euo pipefail

[ "${GHOSTEX_COLLAB_SEAL:-0}" = 1 ] || exit 0
platform="$1"
manifest="$2"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
version="$(node "$root/tooling/collab/component-identity.mjs")"
tag="collab-$version"
asset="collab-$version-$platform.tar.gz"
repo="$(node "$root/tooling/release-gpui/components-repo.mjs")"
wait_seconds="${GHOSTEX_COLLAB_WAIT_SECONDS:-1800}"
deadline=$((SECONDS + wait_seconds))
while true; do
  if gh release view "$tag" --repo "$repo" --json assets --jq '.assets[].name' 2>/dev/null | grep -Fxq "$asset.sha256"; then
    break
  fi
  if ((SECONDS >= deadline)); then
    if [ "${GHOSTEX_COLLAB_OPTIONAL:-0}" = 1 ]; then
      echo "Collab $tag/$asset is not published; this build ships without Collab."
      exit 0
    fi
    echo "Collab component $tag/$asset was not published within ${wait_seconds}s (release-gpui-collab.yml)." >&2
    exit 1
  fi
  sleep 20
done
asset_dir="$root/build/on-demand-components/collab-$platform"
mkdir -p "$asset_dir"
node "$root/tooling/release-gpui/publish-component.mjs" \
  --reuse-published \
  --metadata-only \
  --require-sha256-sidecars \
  --component collab \
  --version "$version" \
  --platform "$platform" \
  --asset-dir "$asset_dir" \
  --output "$manifest"
echo "Sealed Collab $version for $platform."
