#!/usr/bin/env bash
# Publishes theme artworks to the components repository's `theme-art` release, where gxserver
# downloads them from (`ARTWORK_RELEASE` in packages/settings-catalog/src/data/themes.rs).
#
#   tooling/theme-art/publish.sh <folder of <id>.webp files>
#
# Each file is uploaded as `<id>-<first 8 of its SHA-256>.webp`, the name the app asks for, so a
# changed artwork is a new asset and installed apps keep finding the one they were built with. The
# script prints the `Artwork { ... }` line each needs in `ARTWORKS` (and its `WINDOW_ART_OPTIONS`
# entry and bundled thumbnail are added by hand). Needs `gh` signed in with write access to the
# components repository (COMPONENTS_GITHUB_TOKEN in CI, tooling/release-gpui/components-repo-setup.md).
set -euo pipefail
folder="${1:?usage: publish.sh <folder of <id>.webp>}"
repo="$(node -e "import('$(dirname "$0")/../release-gpui/components-repo.mjs').then(m => console.log(m.componentsGithubRepo()))")"
tag="theme-art"
if ! gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
  gh release create "$tag" --repo "$repo" --title "Theme artworks" \
    --notes "Artworks Ghostex downloads on demand for its themes. Each file is <id>-<sha8>.webp." 
fi
staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT
for file in "$folder"/*.webp; do
  id="$(basename "$file" .webp)"
  sha="$(shasum -a 256 "$file" | cut -c1-64)"
  size="$(wc -c < "$file" | tr -d ' ')"
  cp "$file" "$staging/$id-${sha:0:8}.webp"
  echo "    Artwork { id: \"$id\", name: \"<name>\", sha256: \"$sha\", size_bytes: $size },"
done
gh release upload "$tag" --repo "$repo" --clobber "$staging"/*.webp
