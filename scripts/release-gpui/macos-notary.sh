#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
COMMAND="${1:-}"
VERSION="${2:-}"
DMG="${3:-}"
release_gpui_require_version "$VERSION"
[[ -f "$DMG" ]] || { echo "Signed DMG is missing: $DMG" >&2; exit 1; }

notary_args=()
if [[ -n "${GHOSTEX_NOTARY_KEY_PATH:-}" && -n "${GHOSTEX_NOTARY_KEY_ID:-}" && -n "${GHOSTEX_NOTARY_ISSUER_ID:-}" ]]; then
  notary_args+=(--key "$GHOSTEX_NOTARY_KEY_PATH" --key-id "$GHOSTEX_NOTARY_KEY_ID" --issuer "$GHOSTEX_NOTARY_ISSUER_ID")
elif [[ -n "${GHOSTEX_NOTARY_APPLE_ID:-}" && -n "${GHOSTEX_NOTARY_TEAM_ID:-}" && -n "${GHOSTEX_NOTARY_APP_PASSWORD:-}" ]]; then
  notary_args+=(--apple-id "$GHOSTEX_NOTARY_APPLE_ID" --team-id "$GHOSTEX_NOTARY_TEAM_ID" --password "$GHOSTEX_NOTARY_APP_PASSWORD")
elif [[ -n "${GHOSTEX_NOTARY_PROFILE:-}" ]]; then
  notary_args+=(--keychain-profile "$GHOSTEX_NOTARY_PROFILE")
else
  echo "Direct App Store Connect API-key, Apple ID, or keychain-profile notarization credentials are required" >&2
  exit 1
fi

case "$COMMAND" in
  submit)
    OUTPUT="${4:-}"
    [[ -n "$OUTPUT" ]] || { echo "Submission output path is required" >&2; exit 2; }
    mkdir -p "$(dirname "$OUTPUT")"
    xcrun notarytool submit "$DMG" "${notary_args[@]}" --no-wait --output-format json > "$OUTPUT"
    SUBMISSION_ID="$(plutil -extract id raw -o - "$OUTPUT")"
    [[ -n "$SUBMISSION_ID" ]] || { echo "notarytool did not return a submission ID" >&2; exit 1; }
    DMG_SHA256="$(release_gpui_sha256 "$DMG")"
    SUBMISSION_ID="$SUBMISSION_ID" DMG_SHA256="$DMG_SHA256" node - "$OUTPUT" <<'JS'
const { readFileSync, writeFileSync } = require("node:fs");
const file = process.argv[2];
const raw = JSON.parse(readFileSync(file, "utf8"));
writeFileSync(file, `${JSON.stringify({
  created_at: new Date().toISOString(),
  dmg_sha256: process.env.DMG_SHA256,
  status: "submitted",
  submission_id: process.env.SUBMISSION_ID,
  notarytool: raw,
}, null, 2)}\n`);
JS
    printf '%s\n' "$SUBMISSION_ID"
    ;;
  poll)
    SUBMISSION_ID="${4:-}"
    [[ -n "$SUBMISSION_ID" ]] || { echo "Existing submission ID is required" >&2; exit 2; }
    mkdir -p "$REPO_ROOT/build/release-gpui"
    attempts=0
    consecutive_poll_failures=0
    while (( attempts < 120 )); do
      attempts=$((attempts + 1))
      INFO_FILE="$(mktemp "$REPO_ROOT/build/release-gpui/notary-info-XXXXXX.json")"
      if ! xcrun notarytool info "$SUBMISSION_ID" "${notary_args[@]}" --output-format json > "$INFO_FILE"; then
        rm -f "$INFO_FILE"
        consecutive_poll_failures=$((consecutive_poll_failures + 1))
        if (( consecutive_poll_failures >= 10 )); then
          echo "Could not poll Apple submission $SUBMISSION_ID after $consecutive_poll_failures consecutive attempts" >&2
          exit 1
        fi
        poll_retry_delay=$((consecutive_poll_failures * 5))
        (( poll_retry_delay > 30 )) && poll_retry_delay=30
        echo "Apple notarization poll failed; retrying submission $SUBMISSION_ID in ${poll_retry_delay}s ($consecutive_poll_failures/10)" >&2
        sleep "$poll_retry_delay"
        continue
      fi
      consecutive_poll_failures=0
      STATUS="$(plutil -extract status raw -o - "$INFO_FILE")"
      rm -f "$INFO_FILE"
      case "$STATUS" in
        Accepted) break ;;
        Invalid|Rejected)
          xcrun notarytool log "$SUBMISSION_ID" "${notary_args[@]}" || true
          echo "Apple notarization ended with status $STATUS" >&2
          exit 1
          ;;
        In\ Progress|Submitted) sleep 30 ;;
        *) echo "Unexpected Apple notarization status: $STATUS" >&2; exit 1 ;;
      esac
    done
    [[ "${STATUS:-}" == "Accepted" ]] || { echo "Apple notarization is still in progress; retry only the poll/staple stage" >&2; exit 1; }
    xcrun stapler staple "$DMG"
    xcrun stapler validate "$DMG"
    ;;
  *)
    echo "Usage: macos-notary.sh submit <version> <dmg> <submission-json> | poll <version> <dmg> <submission-id>" >&2
    exit 2
    ;;
esac
