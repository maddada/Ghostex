#!/usr/bin/env bash
# CDXC:ReleaseTransientFailureClassification 2026-08-13:
# Bounded, classified retries for external dependency fetches.
#
# Only wrap operations whose failure mode is transport or availability: archive
# downloads, package-manager installs, `gh` calls. Never wrap cargo/zig/gradle
# compiles, vpk pack, codesign, or signtool — release 7.7 lost forty minutes to
# whole-job reruns caused by one closed HTTP connection, and the fix is to retry
# the fetch, not the build.
#
# A failure is retried only when tooling/release-gpui/failure-classification.mjs
# recognizes it as transient. Unclassified output is fatal by design.
#
# Usage:
#   tooling/release-gpui/retry.sh <attempts> <base-delay-seconds> -- <command> [args...]
#   source tooling/release-gpui/retry.sh && release_gpui_retry 4 5 -- <command> [args...]

release_gpui_retry() {
  local attempts="${1:?attempts}"
  local base_delay="${2:?base delay seconds}"
  shift 2
  if [[ "${1:-}" == "--" ]]; then
    shift
  fi
  if [[ $# -eq 0 ]]; then
    echo "release_gpui_retry: no command given" >&2
    return 2
  fi

  local script_dir classifier log status attempt delay jitter label classification classified
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  classifier="$script_dir/failure-classification.mjs"
  label="$1"
  log="$(mktemp "${TMPDIR:-/tmp}/ghostex-retry-XXXXXX")"

  for (( attempt = 1; attempt <= attempts; attempt++ )); do
    status=0
    (
      set -o pipefail
      "$@" 2>&1 | tee "$log"
    ) || status=$?
    if [[ $status -eq 0 ]]; then
      rm -f "$log"
      return 0
    fi

    if [[ $attempt -ge $attempts ]]; then
      echo "::error::$label failed after $attempt attempt(s)" >&2
      rm -f "$log"
      return "$status"
    fi

    classified=0
    classification="$(node "$classifier" "$log" 2>/dev/null)" || classified=$?
    if [[ $classified -ne 0 ]]; then
      echo "::error::$label failed with a non-retryable error (${classification:-unclassified}); not retrying" >&2
      rm -f "$log"
      return "$status"
    fi

    jitter=$(( RANDOM % 5 ))
    delay=$(( base_delay * (3 ** (attempt - 1)) + jitter ))
    echo "::notice::retry ${classification} attempt ${attempt}/${attempts} after ${delay}s ($label)"
    if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
      echo "- Retry \`$label\` attempt ${attempt}/${attempts} after ${delay}s — ${classification}" >>"$GITHUB_STEP_SUMMARY"
    fi
    sleep "$delay"
  done

  rm -f "$log"
  return 1
}

# Only run when executed, so the function can also be sourced by other scripts.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  set -euo pipefail
  release_gpui_retry "$@"
fi
