#!/usr/bin/env bash
# CDXC:ReleaseTransientFailureClassification 2026-08-13:
# Pre-fetch every Zig package dependency into ZIG_GLOBAL_CACHE_DIR before any
# compile starts.
#
# Release 7.7 failed twice, on both gxserver architectures, because
# `zigimg/archive/<sha>.tar.gz` returned `invalid HTTP response:
# HttpConnectionClosing` *in the middle of a build*. Retrying the build was the
# only available remedy and it threw away everything already compiled. Fetching
# first, with bounded classified retries, turns that class of failure into a
# ~10-second event at the start of the job.
#
# Integrity is not weakened: `zig build --fetch` verifies each package against
# the hash pinned in build.zig.zon and refuses a mismatch, and a hash mismatch is
# classified FATAL, so it can never be retried into acceptance.
#
# Usage: scripts/release-gpui/fetch-zig-deps.sh [root ...]      (default: ghostty zmx; each
# root is resolved under .dependencies/)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=scripts/release-gpui/retry.sh
source "$SCRIPT_DIR/retry.sh"

export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$REPO_ROOT/build/zig-global-cache}"
mkdir -p "$ZIG_GLOBAL_CACHE_DIR"

ROOTS=("$@")
if [[ ${#ROOTS[@]} -eq 0 ]]; then
  ROOTS=(ghostty zmx)
fi

zig_for_root() {
  printf '%s\n' "${GHOSTEX_ZIG:-${ZIG:-zig}}"
}

FETCHED=0
SKIPPED=()
for root in "${ROOTS[@]}"; do
  ROOT_DIR="$REPO_ROOT/.dependencies/$root"
  if [[ ! -f "$ROOT_DIR/build.zig.zon" ]]; then
    # Submodules are only checked out for the jobs that build them; a missing
    # root is not an error, it is a job that does not need those packages.
    SKIPPED+=("$root")
    continue
  fi
  ZIG_BIN="$(zig_for_root "$root")"
  if ! command -v "$ZIG_BIN" >/dev/null 2>&1 && [[ ! -x "$ZIG_BIN" ]]; then
    SKIPPED+=("$root (no zig)")
    continue
  fi
  echo "Pre-fetching Zig dependencies for $root with $ZIG_BIN"
  # Prefetching is a pure optimization: the compile fetches the same packages
  # with the same pinned hashes if this misses. Failing the job here would make
  # releases more fragile, not less, so a miss is a warning.
  if release_gpui_retry 4 5 -- "$ZIG_BIN" build --fetch --build-file "$ROOT_DIR/build.zig" --cache-dir "$ROOT_DIR/.zig-cache"; then
    FETCHED=$((FETCHED + 1))
  else
    echo "::warning::Zig dependency prefetch for $root did not complete; the build will fetch on demand"
    SKIPPED+=("$root (prefetch failed)")
  fi
done

echo "Pre-fetched Zig dependencies for $FETCHED root(s) into $ZIG_GLOBAL_CACHE_DIR"
if [[ ${#SKIPPED[@]} -gt 0 ]]; then
  echo "Skipped: ${SKIPPED[*]}"
fi
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  echo "- Zig dependency prefetch: ${FETCHED} root(s), skipped ${#SKIPPED[@]}" >>"$GITHUB_STEP_SUMMARY"
fi
