#!/usr/bin/env bash
# Publishes (or, for a build-only run, records) the CEF and Source components of a staged Linux
# release build, named by the build's sealed on-demand-resources.json. linux-stage.sh runs it after
# the build; a release that defers component signatures runs it as a step of its own, after the
# signing step, so the signing key never shares a step with the build (CDXC:Release 2026-11-14 in
# publish-component.mjs).
#
# Usage: linux-publish-components.sh <code-server component version> <code-server archive>
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
CODE_SERVER_COMPONENT_VERSION="${1:?Usage: linux-publish-components.sh <code-server component version> <code-server archive>}"
CODE_SERVER_ARCHIVE="${2:?Usage: linux-publish-components.sh <code-server component version> <code-server archive>}"
LINUX_ARCH="$(release_gpui_linux_arch)"
APP_DIR="$REPO_ROOT/apps/desktop/build/linux/Ghostex"
ON_DEMAND_MANIFEST="$APP_DIR/resources/on-demand-resources.json"
CEF_COMPONENT_VERSION="$(node -e '
const fs = require("node:fs");
const manifest = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const component = manifest.components?.cef;
const asset = component?.platforms?.[`linux-${process.argv[2]}`];
if (!component?.componentVersion || component.downloadTag !== `cef-${component.componentVersion}` || !/^[0-9a-f]{64}$/.test(asset?.sha256 ?? "")) process.exit(1);
process.stdout.write(component.componentVersion);
' "$ON_DEMAND_MANIFEST" "$LINUX_ARCH")" || {
	echo "Linux release build has an invalid sealed CEF component manifest" >&2
	exit 1
}
SEALED_CODE_SERVER_COMPONENT_VERSION="$(node -e '
const fs = require("node:fs");
const manifest = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const component = manifest.components?.["code-server"];
const asset = component?.platforms?.[`linux-${process.argv[3]}`];
const expectedName = `code-server-${process.argv[2]}-linux-${process.argv[3]}.tar.gz`;
if (component?.componentVersion !== process.argv[2] || component.downloadTag !== `code-server-${process.argv[2]}` || asset?.assetName !== expectedName || asset?.sha256SidecarName !== `${expectedName}.sha256` || !/^[0-9a-f]{64}$/.test(asset?.sha256 ?? "")) process.exit(1);
process.stdout.write(component.componentVersion);
' "$ON_DEMAND_MANIFEST" "$CODE_SERVER_COMPONENT_VERSION" "$LINUX_ARCH")" || {
	echo "Linux release build has an invalid sealed Source component manifest" >&2
	exit 1
}
# CDXC:Release 2026-10-19 WHY:
# A build-only release run (release-gpui.yml `build_only`) produces packages as
# Actions artifacts and publishes nothing, the components repository included:
# neither the CEF component nor the code-server component. Both calls below
# share one mode (COMPONENT_PUBLISH_MODE), so a build-only run only writes
# their sealed metadata and uploads no archive. The sealed manifest already
# records each archive's digest, so a tester serves the uploaded archives
# through GHOSTEX_ON_DEMAND_BASE_URL instead.
COMPONENT_PUBLISH_MODE=()
if [[ "${GHOSTEX_RELEASE_BUILD_ONLY:-0}" == "1" ]]; then
	COMPONENT_PUBLISH_MODE=(--metadata-only)
fi
node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs" \
	"${COMPONENT_PUBLISH_MODE[@]}" \
	--component cef \
	--version "$CEF_COMPONENT_VERSION" \
	--asset-dir "$REPO_ROOT/build/on-demand-components/assets" \
	--output "$REPO_ROOT/build/on-demand-components/components.json"
node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs" \
	"${COMPONENT_PUBLISH_MODE[@]}" \
	--require-sha256-sidecars \
	--component code-server \
	--version "$SEALED_CODE_SERVER_COMPONENT_VERSION" \
	--asset-dir "$(dirname "$CODE_SERVER_ARCHIVE")" \
	--output "$REPO_ROOT/build/on-demand-components/components.json"

