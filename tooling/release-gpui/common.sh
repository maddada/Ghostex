#!/usr/bin/env bash
set -euo pipefail

release_gpui_repo_root() {
	if [[ -n "${GHOSTEX_RELEASE_REPO_ROOT:-}" ]]; then
		cd "$GHOSTEX_RELEASE_REPO_ROOT" && pwd
		return
	fi
	cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd
}

release_gpui_require_version() {
	local version="${1:-}"
	if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
		echo "Version must be MAJOR.MINOR.PATCH, got: ${version:-<empty>}" >&2
		exit 2
	fi
}

release_gpui_build_number() {
	local version="$1"
	local major minor patch
	IFS=. read -r major minor patch <<<"$version"
	printf '%d\n' "$((10#$major * 10000 + 10#$minor * 100 + 10#$patch))"
}

release_gpui_default_output() {
	local repo_root="$1"
	local version="$2"
	local platform="$3"
	printf '%s/build/release-gpui/%s/%s\n' "$repo_root" "$version" "$platform"
}

release_gpui_prepare_output() {
	local repo_root="$1"
	local output="$2"
	case "$output" in
	"$repo_root"/build/release-gpui/*) ;;
	*)
		echo "Release output must stay under $repo_root/build/release-gpui: $output" >&2
		exit 2
		;;
	esac
	rm -rf "$output"
	mkdir -p "$output"
}

release_gpui_sha256() {
	local file="$1"
	if command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$file" | awk '{print $1}'
	else
		sha256sum "$file" | awk '{print $1}'
	fi
}

release_gpui_assert_dmg_budget() {
	local dmg="$1"
	local budget_bytes=$((300 * 1024 * 1024))
	local size_bytes
	size_bytes="$(stat -f '%z' "$dmg")"
	if ((size_bytes > budget_bytes)); then
		echo "Release DMG exceeds the 300 MiB bundle budget: $size_bytes bytes ($dmg)" >&2
		exit 1
	fi
	printf 'Release DMG size: %.1f MiB / 300.0 MiB budget\n' "$(awk -v bytes="$size_bytes" 'BEGIN { print bytes / 1048576 }')"
}

# CDXC:ReleaseChangeAwarePlanning 2026-08-13:
# Every producing job also emits the per-product provenance record next to its
# manifest, so the publisher can cross-check plan <-> manifest <-> provenance
# instead of trusting a hand-written duplicate. It runs only when the job was
# handed a resolved plan, and only for platforms the plan actually names, so
# intermediate manifests (macos-arm64-signed) and local runs are unaffected.
release_gpui_write_provenance() {
	local output="$1"
	if [[ -z "${GHOSTEX_RELEASE_PLAN:-}${GHOSTEX_RELEASE_PLAN_FILE:-}" ]]; then
		return 0
	fi
	local script_dir
	script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
	node "$script_dir/write-provenance.mjs" --dir "$output" --if-planned
}

release_gpui_write_manifest() {
	local output="$1"
	local platform="$2"
	local version="$3"
	shift 3
	local manifest="$output/manifest.json"
	RELEASE_GPUI_MANIFEST_PLATFORM="$platform" \
		RELEASE_GPUI_MANIFEST_VERSION="$version" \
		RELEASE_GPUI_MANIFEST_OUTPUT="$output" \
		RELEASE_GPUI_MANIFEST_SOURCE_SHA="${GHOSTEX_RELEASE_SOURCE_SHA:-}" \
		RELEASE_GPUI_MANIFEST_SOURCE_KIND="${GHOSTEX_RELEASE_SOURCE_KIND:-}" \
		RELEASE_GPUI_MANIFEST_APPLICATION_ID="${GHOSTEX_RELEASE_APPLICATION_ID:-}" \
		RELEASE_GPUI_MANIFEST_WORKFLOW_SHA="${GHOSTEX_RELEASE_WORKFLOW_SHA:-}" \
		RELEASE_GPUI_MANIFEST_WORKFLOW_RUN_ID="${GITHUB_RUN_ID:-}" \
		node - "$@" <<'JS'
const { createHash } = require("node:crypto");
const { readFileSync, statSync, writeFileSync } = require("node:fs");
const { basename, join } = require("node:path");

const output = process.env.RELEASE_GPUI_MANIFEST_OUTPUT;
const platform = process.env.RELEASE_GPUI_MANIFEST_PLATFORM;
const artifacts = process.argv.slice(2).map((file) => {
  const bytes = readFileSync(file);
  return {
    name: basename(file),
    sha256: createHash("sha256").update(bytes).digest("hex"),
    size: statSync(file).size,
  };
});
writeFileSync(join(output, "manifest.json"), `${JSON.stringify({
  artifacts,
  application_id: process.env.RELEASE_GPUI_MANIFEST_APPLICATION_ID || undefined,
  platform,
  schemaVersion: 1,
  source_kind: process.env.RELEASE_GPUI_MANIFEST_SOURCE_KIND || undefined,
  source_sha: process.env.RELEASE_GPUI_MANIFEST_SOURCE_SHA || undefined,
  version: process.env.RELEASE_GPUI_MANIFEST_VERSION,
  workflow_run_id: process.env.RELEASE_GPUI_MANIFEST_WORKFLOW_RUN_ID
    ? Number(process.env.RELEASE_GPUI_MANIFEST_WORKFLOW_RUN_ID)
    : undefined,
  workflow_sha: process.env.RELEASE_GPUI_MANIFEST_WORKFLOW_SHA || undefined,
}, null, 2)}\n`);
const architecture = {
  android: "universal",
  "gxserver-linux-x64": "x86_64",
  "gxserver-linux-arm64": "aarch64",
  "macos-arm64": "arm64",
  "macos-arm64-signed": "arm64",
}[platform] || "unknown";
const primary = artifacts.length === 1 ? artifacts[0] : {};
writeFileSync(join(output, "metadata.json"), `${JSON.stringify({
  architecture,
  artifacts,
  application_id: process.env.RELEASE_GPUI_MANIFEST_APPLICATION_ID || undefined,
  created_at: new Date().toISOString(),
  package: platform,
  schemaVersion: 1,
  source_kind: process.env.RELEASE_GPUI_MANIFEST_SOURCE_KIND || undefined,
  ...primary,
  source_sha: process.env.RELEASE_GPUI_MANIFEST_SOURCE_SHA || undefined,
  version: process.env.RELEASE_GPUI_MANIFEST_VERSION,
  workflow_run_id: process.env.RELEASE_GPUI_MANIFEST_WORKFLOW_RUN_ID
    ? Number(process.env.RELEASE_GPUI_MANIFEST_WORKFLOW_RUN_ID)
    : undefined,
  workflow_sha: process.env.RELEASE_GPUI_MANIFEST_WORKFLOW_SHA || undefined,
}, null, 2)}\n`);
JS
	release_gpui_write_provenance "$output"
}

release_gpui_require_command() {
	local command_name="$1"
	command -v "$command_name" >/dev/null 2>&1 || {
		echo "Required command is missing: $command_name" >&2
		exit 1
	}
}
