#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# CDXC:GhostexTui 2026-07-01-02:10: Release verification must test the promoted GX 2 TUI source because `gx tui` and packaged `ghostex-tui` no longer launch the removed legacy `tui/` submodule.
tui_root="$repo_root/tui2"

# CDXC:ReleaseAutomation 2026-06-14-09:07: TUI release verification must not inherit Homebrew's unversioned Zig when it points at Zig 0.16. The vendored Ghostty VT build requires Zig 0.15.2, so choose the compatible keg before running Cargo tests.
zig_bin="${ZIG:-}"
if [[ -z "$zig_bin" && -x /opt/homebrew/opt/zig@0.15/bin/zig ]]; then
  zig_bin=/opt/homebrew/opt/zig@0.15/bin/zig
elif [[ -z "$zig_bin" ]]; then
  zig_bin="$(command -v zig || true)"
fi

if [[ -z "$zig_bin" ]]; then
  cat >&2 <<'EOF'
Zig 0.15.2 is required for Ghostex TUI tests.

Install the pinned Homebrew keg or set ZIG explicitly:
  brew install zig@0.15
  ZIG=/opt/homebrew/opt/zig@0.15/bin/zig scripts/ghostex-tui-test.sh
EOF
  exit 2
fi

zig_version="$("$zig_bin" version 2>/dev/null || true)"
if [[ "$zig_version" != "0.15.2" ]]; then
  cat >&2 <<EOF
Zig 0.15.2 is required for Ghostex TUI tests.

Selected Zig:
  $zig_bin
  version: ${zig_version:-unknown}

Install the compatible keg or set ZIG explicitly:
  brew install zig@0.15
  ZIG=/opt/homebrew/opt/zig@0.15/bin/zig scripts/ghostex-tui-test.sh
EOF
  exit 2
fi

cd "$tui_root"
env ZIG="$zig_bin" cargo test --bin ghostex-tui "$@"
