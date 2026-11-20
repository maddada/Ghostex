#!/usr/bin/env bash
# Start (or stop) the persistent headless Chromium webdrive.mjs drives.
# WEBDRIVE_GL=swiftshader (default): WebGL2 on SwiftShader (WebGPU loses its device there).
# WEBDRIVE_GL=hw: the real GPU through ANGLE/Vulkan, WebGPU on.
#   webdrive-chrome.sh start <url> [WxH] | stop
set -euo pipefail
PROFILE="${WEBDRIVE_PROFILE:-$HOME/.local/share/agent-work/Ghostex/smoke/chrome-profile}"
case "${1:-}" in
  start) if [ "${WEBDRIVE_GL:-swiftshader}" = hw ]; then GL_FLAGS="--enable-unsafe-webgpu --enable-features=Vulkan --use-angle=vulkan --ignore-gpu-blocklist"
    else GL_FLAGS="--disable-features=WebGPU --use-angle=swiftshader --enable-unsafe-swiftshader"; fi
    rm -rf "$PROFILE"; mkdir -p "$PROFILE"; size="${3:-1440x900}"
    nohup chromium --headless=new --remote-debugging-port=0 --user-data-dir="$PROFILE" --window-size="${size/x/,}" \
      $GL_FLAGS --disable-extensions --no-first-run --no-default-browser-check \
      --disable-background-timer-throttling --disable-renderer-backgrounding "$2" > "$PROFILE.log" 2>&1 &
    echo $! > "$PROFILE.pid"; for _ in $(seq 1 50); do [ -s "$PROFILE/DevToolsActivePort" ] && break; sleep 0.2; done
    echo "chromium pid $(cat "$PROFILE.pid") devtools port $(head -1 "$PROFILE/DevToolsActivePort")" ;;
  stop) [ -f "$PROFILE.pid" ] && kill "$(cat "$PROFILE.pid")" 2>/dev/null || true; rm -f "$PROFILE.pid"; sleep 1; rm -rf "$PROFILE"; echo stopped ;;
  *) echo "usage: $0 start <url> [WxH] | stop"; exit 2 ;;
esac
