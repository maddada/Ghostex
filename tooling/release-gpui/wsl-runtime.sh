#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
VERSION="${1:-}"
ARCH="${2:-}"
LINUX_ARCHIVE="${3:-}"
OUTPUT="${4:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" "gxserver-wsl-windows-$ARCH")}"
release_gpui_require_version "$VERSION"
case "$ARCH" in
x64 | arm64) ;;
*)
	echo "WSL package architecture must be x64 or arm64, got: ${ARCH:-<empty>}" >&2
	exit 2
	;;
esac
release_gpui_require_command node
release_gpui_require_command zip
release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"

[[ -f "$LINUX_ARCHIVE" ]] || {
	echo "Linux gxserver runtime archive is missing: $LINUX_ARCHIVE" >&2
	exit 1
}
EXPECTED_NAME="gxserver-linux-$ARCH.tar.gz"
[[ "$(basename "$LINUX_ARCHIVE")" == "$EXPECTED_NAME" ]] || {
	echo "Expected $EXPECTED_NAME, got $(basename "$LINUX_ARCHIVE")" >&2
	exit 1
}

STAGE_ROOT="$REPO_ROOT/build/release-gpui/wsl-package-stage-$ARCH"
PACKAGE_NAME="gxserver-wsl-windows-$ARCH"
PACKAGE_ROOT="$STAGE_ROOT/$PACKAGE_NAME"
rm -rf "$STAGE_ROOT"
mkdir -p "$PACKAGE_ROOT"
trap 'rm -rf "$STAGE_ROOT"' EXIT

cp "$LINUX_ARCHIVE" "$PACKAGE_ROOT/$EXPECTED_NAME"
cp "$SCRIPT_DIR/install-gxserver-wsl.ps1" "$PACKAGE_ROOT/install-gxserver-wsl.ps1"
PAYLOAD_SHA="$(release_gpui_sha256 "$PACKAGE_ROOT/$EXPECTED_NAME")"
GHOSTEX_WSL_PACKAGE_ROOT="$PACKAGE_ROOT" \
	GHOSTEX_WSL_VERSION="$VERSION" \
	GHOSTEX_WSL_ARCH="$ARCH" \
	GHOSTEX_WSL_PAYLOAD_NAME="$EXPECTED_NAME" \
	GHOSTEX_WSL_PAYLOAD_SHA="$PAYLOAD_SHA" \
	node <<'JS'
const { statSync, writeFileSync } = require("node:fs");
const { join } = require("node:path");
const env = process.env;
const payload = join(env.GHOSTEX_WSL_PACKAGE_ROOT, env.GHOSTEX_WSL_PAYLOAD_NAME);
writeFileSync(join(env.GHOSTEX_WSL_PACKAGE_ROOT, "wsl-package.json"), `${JSON.stringify({
  schemaVersion: 1,
  version: env.GHOSTEX_WSL_VERSION,
  target: "wsl2",
  targetArch: env.GHOSTEX_WSL_ARCH,
  payload: {
    name: env.GHOSTEX_WSL_PAYLOAD_NAME,
    sha256: env.GHOSTEX_WSL_PAYLOAD_SHA,
    size: statSync(payload).size,
  },
}, null, 2)}\n`);
JS
cat >"$PACKAGE_ROOT/README.txt" <<EOF
Ghostex gxserver $VERSION for Windows WSL2 ($ARCH)

This is not a native Win32 gxserver or zmx build. It installs the matching
static Linux gxserver runtime into an initialized WSL2 distribution.

From PowerShell, run:
  .\\install-gxserver-wsl.ps1

You may select a distribution explicitly:
  .\\install-gxserver-wsl.ps1 -Distro Ubuntu
EOF

ARCHIVE="$OUTPUT/$PACKAGE_NAME.zip"
(
	cd "$STAGE_ROOT"
	zip -X -q -r "$ARCHIVE" "$PACKAGE_NAME"
)
release_gpui_write_manifest "$OUTPUT" "gxserver-wsl-windows-$ARCH" "$VERSION" "$ARCHIVE"
printf 'Built WSL gxserver %s release payload in %s\n' "$ARCH" "$OUTPUT"
