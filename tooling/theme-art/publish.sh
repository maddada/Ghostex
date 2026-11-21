#!/usr/bin/env bash
# Publishes the theme artworks to the components repository's `theme-art` release, where gxserver
# downloads them from (`ARTWORK_RELEASE` in packages/settings-catalog/src/data/themes.rs, checked
# against the size and SHA-256 in `ARTWORKS` there).
#
#   tooling/theme-art/publish.sh            publish .dependencies/ghostex-components/theme-art/
#   tooling/theme-art/publish.sh --check    only check the folder against its manifest
#
# The source is the components repository's own `theme-art/` folder (the
# .dependencies/ghostex-components submodule, `git submodule update --init
# .dependencies/ghostex-components`): `<id>-<first 8 of its SHA-256>.webp` files and
# `manifest.json` (id, name, file, size, SHA-256). Every file is checked against the manifest, the
# release is created when it is missing, and only the files the release does not have yet are
# uploaded: a changed artwork is a new file name, so an asset installed apps know never changes.
#
# Adding an artwork: copy it into theme-art/ under its `<id>-<sha8>.webp` name, add its manifest
# entry, commit and push the submodule, run this, then add its `Artwork` line to `ARTWORKS`, its
# `WINDOW_ART_OPTIONS` entry and its bundled thumbnail in the Ghostex repository.
#
# Needs `gh` signed in with write access to the components repository (COMPONENTS_GITHUB_TOKEN in
# CI, tooling/release-gpui/components-repo-setup.md).
#
# CDXC:Theming 2026-11-21 DECISION:
# User (Q4365): "i want this ghostex-components thingy to be a sub repo under ghostex (under deps folder) like we did for ghostex-mobile exactly". The artworks live in the components repository's theme-art/ folder (the submodule) and this script publishes the release from it; the app's download and its size and hash check are unchanged.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
folder="$root/.dependencies/ghostex-components/theme-art"
tag="theme-art"
if [[ ! -f "$folder/manifest.json" ]]; then
  echo "No $folder/manifest.json. Check out the submodule: git -C \"$root\" submodule update --init .dependencies/ghostex-components" >&2
  exit 1
fi
repo="$(node -e "import('$root/tooling/release-gpui/components-repo.mjs').then(m => console.log(m.componentsGithubRepo()))")"

# Every file the manifest names is there, with its size and SHA-256, and nothing else is.
python3 - "$folder" <<'PY'
import hashlib, json, os, sys
folder = sys.argv[1]
manifest = json.load(open(os.path.join(folder, "manifest.json")))
named = set()
for artwork in manifest["artworks"]:
    path = os.path.join(folder, artwork["file"])
    data = open(path, "rb").read()
    digest = hashlib.sha256(data).hexdigest()
    if len(data) != artwork["sizeBytes"] or digest != artwork["sha256"]:
        sys.exit(f"{artwork['file']} does not match manifest.json")
    if artwork["file"] != f"{artwork['id']}-{digest[:8]}.webp":
        sys.exit(f"{artwork['file']} is not named <id>-<sha8>.webp")
    named.add(artwork["file"])
extra = sorted(f for f in os.listdir(folder) if f.endswith(".webp") and f not in named)
if extra:
    sys.exit("Not in manifest.json: " + ", ".join(extra))
print(f"{len(named)} artworks match manifest.json")
PY
if [[ "${1:-}" == "--check" ]]; then
  exit 0
fi

if ! gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
  gh release create "$tag" --repo "$repo" --target main --title "Theme artworks" \
    --notes "Artworks Ghostex downloads on demand for its themes (Settings > Themes: Art backgrounds and Backgrounds). Each file is <id>-<first 8 of its SHA-256>.webp; theme-art/manifest.json in this repository lists them. The app checks every download against the size and SHA-256 built into it."
fi
existing="$(gh release view "$tag" --repo "$repo" --json assets --jq '.assets[].name')"
missing=()
for file in "$folder"/*.webp; do
  name="$(basename "$file")"
  if ! grep -qxF "$name" <<<"$existing"; then
    missing+=("$file")
  fi
done
if [[ ${#missing[@]} -eq 0 ]]; then
  echo "The $tag release already has every artwork."
else
  gh release upload "$tag" --repo "$repo" "${missing[@]}"
  echo "Uploaded ${#missing[@]} artwork(s) to https://github.com/$repo/releases/tag/$tag"
fi
