#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
SPARKLE_VERSION="2.9.1"
SPARKLE_SHA256="9fec2b888e6e2940b1bfbd5d3d010b9f67076b52170923549095cbb74132403b"
SPARKLE_ROOT="$REPO_ROOT/build/release-gpui/tooling/sparkle-$SPARKLE_VERSION"
ARCHIVE="$REPO_ROOT/build/release-gpui/tooling/Sparkle-$SPARKLE_VERSION.zip"

if [[ ! -x "$SPARKLE_ROOT/bin/generate_appcast" ]]; then
  mkdir -p "$(dirname "$ARCHIVE")"
  curl -fsSL \
    "https://github.com/sparkle-project/Sparkle/releases/download/$SPARKLE_VERSION/Sparkle-for-Swift-Package-Manager.zip" \
    -o "$ARCHIVE"
  actual="$(release_gpui_sha256 "$ARCHIVE")"
  if [[ "$actual" != "$SPARKLE_SHA256" ]]; then
    echo "Sparkle archive checksum mismatch: $actual" >&2
    exit 1
  fi
  rm -rf "$SPARKLE_ROOT"
  mkdir -p "$SPARKLE_ROOT"
  ditto -x -k "$ARCHIVE" "$SPARKLE_ROOT"
fi

printf '%s\n' "$SPARKLE_ROOT"
