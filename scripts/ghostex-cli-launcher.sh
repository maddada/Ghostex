#!/usr/bin/env bash
set -euo pipefail

# CDXC:CliBranding 2026-05-26-15:11: The installed app bundle exposes `ghostex` plus the shorter `gx` command. Keep both as thin launchers over the same bundled Node implementation so command behavior cannot drift by alias name.
# CDXC:CliEntrypoint 2026-05-30-21:37: Homebrew keeps $0 as the symlink path, so resolve links before locating the bundled Node CLI.
SOURCE="${BASH_SOURCE[0]}"
while [ -L "$SOURCE" ]; do
  DIR="$(cd -P "$(dirname "$SOURCE")" >/dev/null 2>&1 && pwd)"
  TARGET="$(readlink "$SOURCE")"
  case "$TARGET" in
    /*) SOURCE="$TARGET" ;;
    *) SOURCE="$DIR/$TARGET" ;;
  esac
done
SCRIPT_DIR="$(cd -P "$(dirname "$SOURCE")" >/dev/null 2>&1 && pwd)"
exec /usr/bin/env node "$SCRIPT_DIR/ghostex-cli.mjs" "$@"
