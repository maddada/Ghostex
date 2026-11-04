#!/usr/bin/env bash
# CDXC:Release 2026-10-31 WHY:
# The AppImage catalog starts every image on a clean Ubuntu 22.04 without
# network and fails it unless a window appears; 10.41.0 died first on a
# library the image did not carry. This starts the image the same way in a
# container (the oldest host the release targets, only the host libraries the
# AppImage excludelist promises, a virtual X display, no network) and waits for
# the first window, which offline is "Ghostex Setup" saying the browser
# runtime could not be downloaded, and checks the window is not blank. The
# catalog fails a window of 95% one colour only when it also shows almost no
# text; the offline window is about 94% one colour with two lines of text, and
# a window that drew nothing is about 100%, so this fails at 99%. Run it on the
# architecture of the image.
# SEE-ALSO: tooling/release-gpui/appimage-bundle-libraries.mjs (what the image bundles).
#
# Usage: linux-appimage-smoke.sh <AppImage> [screenshot.png]
set -euo pipefail

APPIMAGE="$(realpath "${1:?Usage: linux-appimage-smoke.sh <AppImage> [screenshot.png]}")"
SCREENSHOT="${2:-}"
SMOKE_IMAGE="ghostex-appimage-smoke:ubuntu-22.04"
command -v docker >/dev/null || {
	echo "docker is required for the AppImage smoke test" >&2
	exit 1
}

# No libxkbcommon, libxcb-xkb or Vulkan loader: the image must bring or not need them.
docker build --quiet --tag "$SMOKE_IMAGE" - >/dev/null <<'DOCKERFILE'
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends xvfb x11-utils imagemagick libgl1-mesa-dri libegl1 \
    libasound2 libfontconfig1 libfreetype6 fonts-dejavu-core \
 && rm -rf /var/lib/apt/lists/*
DOCKERFILE

# Under $TMPDIR so a Docker VM that shares only the home folder (Colima) sees it.
OUT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ghostex-appimage-smoke.XXXXXX")"
trap 'rm -rf "$OUT_DIR"' EXIT
set +e
docker run --rm --network none \
	-v "$APPIMAGE:/smoke/Ghostex.AppImage:ro" -v "$OUT_DIR:/out" \
	"$SMOKE_IMAGE" bash -c '
set -u
for library in libxkbcommon-x11.so.0 libxkbcommon.so.0 libxcb-xkb.so.1; do
	if ldconfig -p | grep -q "^\s*$library "; then
		echo "The smoke host is not clean: it has $library" >&2
		exit 2
	fi
done
Xvfb :99 -screen 0 1280x800x24 -nolisten tcp >/tmp/xvfb.log 2>&1 &
export DISPLAY=:99 HOME=/tmp/smoke-home
mkdir -p "$HOME"
for _ in $(seq 1 50); do xwininfo -root >/dev/null 2>&1 && break; sleep 0.1; done
/smoke/Ghostex.AppImage --appimage-extract-and-run >/tmp/ghostex.log 2>&1 &
APP_PID=$!
for _ in $(seq 1 90); do
	if ! kill -0 "$APP_PID" 2>/dev/null; then
		echo "Ghostex exited before showing a window:" >&2
		cat /tmp/ghostex.log >&2
		exit 1
	fi
	WINDOW="$(xwininfo -root -tree | grep -F "\"Ghostex Setup\"" | head -n 1)"
	if [ -n "$WINDOW" ] && xwininfo -id "$(echo "$WINDOW" | awk "{ print \$1 }")" | grep -q "Map State: IsViewable"; then
		# import reads the 32-bit window GPUI opens; the catalog captures it the same way.
		# Captured again until drawn: the text can take a few seconds on a slow host.
		for _ in $(seq 1 15); do
			sleep 2
			import -window "$(echo "$WINDOW" | awk "{ print \$1 }")" /out/screen.png
			SHARE="$(convert /out/screen.png -alpha off -depth 8 -format "%c" histogram:info:- | sort -rn |
				awk -v size="$(identify -format "%w %h" /out/screen.png)" "NR == 1 { top = \$1 + 0 } END { split(size, s, \" \"); print int(100 * top / (s[1] * s[2])) }")"
			[ "$SHARE" -lt 97 ] && break
		done
		echo "Window shown: $WINDOW (${SHARE}% one colour)"
		[ "$SHARE" -lt 99 ] || { echo "The window looks blank" >&2; exit 1; }
		test -x "$HOME/.local/share/ghostex/appimage/Ghostex" || { echo "AppRun did not copy the payload" >&2; exit 1; }
		exit 0
	fi
	sleep 1
done
echo "No Ghostex Setup window within 90 seconds:" >&2
cat /tmp/ghostex.log >&2
exit 1
'
STATUS=$?
set -e
if [[ -n "$SCREENSHOT" && -s "$OUT_DIR/screen.png" ]]; then
	cp "$OUT_DIR/screen.png" "$SCREENSHOT"
fi
exit "$STATUS"
