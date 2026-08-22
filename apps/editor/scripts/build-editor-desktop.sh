#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

bun apps/editor/scripts/build-editor-web.mjs
cargo build --release --manifest-path apps/editor/desktop/Cargo.toml

DIST="$ROOT/apps/editor/dist/desktop"
mkdir -p "$DIST"

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    cp "$ROOT/apps/editor/desktop/target/release/ghostex-editor.exe" "$DIST/GhostexEditor.exe"
    ;;
  *)
    cp "$ROOT/apps/editor/desktop/target/release/ghostex-editor" "$DIST/ghostex-editor"
    ;;
esac

mkdir -p "$DIST/web"
cp -R "$ROOT/apps/editor/dist/web/." "$DIST/web/"
