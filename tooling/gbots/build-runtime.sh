#!/usr/bin/env bash
# Builds the GBots runtime: OpenMausBot's headless server at the commit pinned in upstream.json,
# with its source-available enterprise/ folder deleted and its product name replaced by GBots.
# Output: build/gbots/runtime/<runtimeVersion>/ (server bundle, NOTICE, LICENSE, gbots-runtime.json).
#
# Usage: tooling/gbots/build-runtime.sh [--source <git url or local clone>] [--clean]
#   --source  where to fetch the pinned commit from (default: the upstream in upstream.json)
#   --clean   start from a fresh source copy
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
pin="$root/tooling/gbots/upstream.json"
read_pin() { node -e "process.stdout.write(String(require(process.argv[1])[process.argv[2]]))" "$pin" "$1"; }
upstream="$(read_pin upstream)"
commit="$(read_pin commit)"
runtime_version="$(read_pin runtimeVersion)"
pnpm_version="$(read_pin pnpm)"
source="$upstream"
clean=0
while [ $# -gt 0 ]; do
  case "$1" in
    --source) source="$2"; shift 2 ;;
    --clean) clean=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

node_major="$(node -p 'process.versions.node.split(".")[0]')"
if [ "$node_major" -lt 24 ]; then
  echo "GBots runtime needs Node 24 or newer to build (found $(node --version))." >&2
  exit 1
fi

work="$root/build/gbots"
src="$work/src"
out="$work/runtime/$runtime_version"
mkdir -p "$work"
if [ "$clean" = 1 ]; then rm -rf "$src"; fi
if [ ! -d "$src/.git" ]; then
  rm -rf "$src"
  git init -q "$src"
  git -C "$src" remote add origin "$source"
fi
if [ "$(git -C "$src" rev-parse -q --verify HEAD 2>/dev/null || true)" != "$commit" ]; then
  echo "Fetching OpenMausBot $commit from $source"
  git -C "$src" fetch -q --depth 1 origin "$commit"
  git -C "$src" checkout -q --force "$commit"
  git -C "$src" clean -qfdx -e node_modules
else
  # Undo an earlier rename pass so it applies to pristine files.
  git -C "$src" checkout -q --force "$commit"
  git -C "$src" clean -qfdx -e node_modules
fi

# The enterprise layer is source-available, not Apache-2.0: the open edition is the tree without it.
rm -rf "$src/enterprise"

# OpenMausBot's name and mascot are trademarks, so the copy we ship says GBots wherever it names
# the product. Links to upstream (github.com, openmausbot.com) keep the real name so they still work;
# env names (OMB_*), file names and the lowercase command name are left alone.
node "$root/tooling/gbots/rename.mjs" "$src"

echo "Installing build dependencies (pnpm $pnpm_version, lifecycle scripts off)"
(cd "$src" && npx -y "pnpm@$pnpm_version" install --frozen-lockfile --ignore-scripts --config.confirmModulesPurge=false >/dev/null)
# esbuild's postinstall only checks its platform binary; tsc and esbuild run without any other script.
echo "Building the server bundle"
(cd "$src" && npx -y "pnpm@$pnpm_version" run build:server >/dev/null)

rm -rf "$out.tmp"
mkdir -p "$out.tmp"
cp -R "$src/dist-server" "$out.tmp/server"
rm -rf "$out.tmp/server/enterprise"
cp "$src/LICENSE" "$out.tmp/LICENSE"
cp "$src/NOTICE" "$out.tmp/NOTICE"
# The licences of the code the server adapts, and of the packages its bundles include.
if [ -d "$src/third_party" ]; then cp -R "$src/third_party" "$out.tmp/third_party"; fi
(cd "$src" && npx -y "pnpm@$pnpm_version" licenses list --prod --json >"$out.tmp/dependency-licenses.json")
cat > "$out.tmp/gbots-runtime.json" <<JSON
{
  "name": "gbots",
  "runtimeVersion": "$runtime_version",
  "upstream": "$upstream",
  "commit": "$commit",
  "entry": "server/openmausbot.js",
  "license": "Apache-2.0",
  "builtAt": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
JSON
rm -rf "$out"
mv "$out.tmp" "$out"
echo "$out"
