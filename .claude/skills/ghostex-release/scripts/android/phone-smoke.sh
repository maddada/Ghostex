#!/usr/bin/env bash
# Ghostex release smoke, phone part, on a headless Android emulator (Arch, KVM, no sudo).
#   phone-smoke.sh up                 boot the AVD headless (no window, no audio, swiftshader)
#   phone-smoke.sh install [vX.Y.Z|file.apk]  download ghostex-android.apk from that GitHub release (default: latest), or take a local APK, and adb install it
#   phone-smoke.sh launch             start the app
#   phone-smoke.sh shot <name>        full-size screenshot to $GX_SMOKE_DIR/<name>.png
#   phone-smoke.sh tap X Y | text 'abc' | key <KEYCODE> | ui    (input helpers; ui dumps the view tree)
#   phone-smoke.sh uninstall | down   remove the app | stop the emulator
#   phone-smoke.sh all [vX.Y.Z]       up + install + launch + shot 01-launch
# Run setup-sdk.sh once first. Screenshots go to $GX_SMOKE_DIR (default ~/.local/share/agent-work/Ghostex/smoke/phone-<date>).
set -euo pipefail
source "$(dirname "$0")/env.sh"
GX_SMOKE_DIR="${GX_SMOKE_DIR:-$HOME/.local/share/agent-work/Ghostex/smoke/phone-$(date +%Y%m%d)}"
APK_DIR="$GX_ANDROID_ROOT/apk"
PKG_FILE="$GX_ANDROID_ROOT/apk/package-name"
mkdir -p "$GX_SMOKE_DIR" "$APK_DIR"

booted() { [ "$(adb -s "$ANDROID_SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; }

up() {
  if adb devices | grep -q "^$ANDROID_SERIAL\s*device" && booted; then echo "already up: $ANDROID_SERIAL"; return; fi
  adb start-server >/dev/null
  nohup emulator -avd "$GX_AVD_NAME" -port "$GX_EMU_PORT" -no-window -no-audio -gpu swiftshader_indirect \
    -no-boot-anim -no-snapshot-save -camera-back none -camera-front none -accel on \
    > "$GX_ANDROID_ROOT/emulator.log" 2>&1 &
  echo $! > "$GX_ANDROID_ROOT/emulator.pid"
  adb -s "$ANDROID_SERIAL" wait-for-device
  for _ in $(seq 1 180); do booted && break; sleep 2; done
  booted || { echo "emulator did not boot; see $GX_ANDROID_ROOT/emulator.log" >&2; exit 1; }
  adb -s "$ANDROID_SERIAL" shell input keyevent KEYCODE_WAKEUP
  adb -s "$ANDROID_SERIAL" shell wm dismiss-keyguard 2>/dev/null || true
  # No animations: steadier screenshots.
  for s in window_animation_scale transition_animation_scale animator_duration_scale; do adb -s "$ANDROID_SERIAL" shell settings put global $s 0; done
  echo "up: $ANDROID_SERIAL abis=$(adb -s "$ANDROID_SERIAL" shell getprop ro.product.cpu.abilist | tr -d '\r')"
}

install() {
  local tag="${1:-}" apk="$APK_DIR/ghostex-android.apk"
  if [ -f "$tag" ]; then apk="$tag"   # a local APK, e.g. build-x86_64-apk.sh's
  else
    rm -f "$apk"
    if [ -n "$tag" ]; then gh release download "$tag" -R maddada/Ghostex -p ghostex-android.apk -D "$APK_DIR" --clobber
    else gh release download -R maddada/Ghostex -p ghostex-android.apk -D "$APK_DIR" --clobber; fi
  fi
  sha256sum "$apk"
  # A build signed with another key can't replace the installed one.
  adb -s "$ANDROID_SERIAL" uninstall io.ghostex >/dev/null 2>&1 || true
  local before after
  before="$(adb -s "$ANDROID_SERIAL" shell pm list packages -3 | tr -d '\r' | sort)"
  adb -s "$ANDROID_SERIAL" install -r -g "$apk"
  after="$(adb -s "$ANDROID_SERIAL" shell pm list packages -3 | tr -d '\r' | sort)"
  local pkg; pkg="$(comm -13 <(echo "$before") <(echo "$after") | sed 's/^package://' | head -1)"
  [ -n "$pkg" ] || pkg="$(echo "$after" | sed 's/^package://' | grep -i ghostex | head -1)"
  echo "$pkg" > "$PKG_FILE"; echo "installed: $pkg $(adb -s "$ANDROID_SERIAL" shell dumpsys package "$pkg" | grep -m1 versionName | tr -d '\r ')"
}

pkg() { cat "$PKG_FILE"; }
launch() { adb -s "$ANDROID_SERIAL" shell monkey -p "$(pkg)" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1; echo "launched $(pkg)"; }
shot() { local f="$GX_SMOKE_DIR/$1.png"; adb -s "$ANDROID_SERIAL" exec-out screencap -p > "$f"; echo "$f"; }
ui() { adb -s "$ANDROID_SERIAL" shell uiautomator dump /sdcard/ui.xml >/dev/null && adb -s "$ANDROID_SERIAL" exec-out cat /sdcard/ui.xml; }
uninstall() { adb -s "$ANDROID_SERIAL" uninstall "$(pkg)" && rm -f "$PKG_FILE"; }
down() {
  adb -s "$ANDROID_SERIAL" emu kill 2>/dev/null || true
  for _ in $(seq 1 30); do adb devices | grep -q "^$ANDROID_SERIAL" || break; sleep 1; done
  [ -f "$GX_ANDROID_ROOT/emulator.pid" ] && kill "$(cat "$GX_ANDROID_ROOT/emulator.pid")" 2>/dev/null || true
  rm -f "$GX_ANDROID_ROOT/emulator.pid"; adb kill-server 2>/dev/null || true; echo down
}

cmd="${1:-}"; shift || true
case "$cmd" in
  up) up ;; install) install "$@" ;; launch) launch ;; shot) shot "$1" ;; ui) ui ;;
  tap) adb -s "$ANDROID_SERIAL" shell input tap "$1" "$2" ;;
  text) adb -s "$ANDROID_SERIAL" shell input text "$(printf %s "$1" | sed 's/ /%s/g')" ;;
  key) adb -s "$ANDROID_SERIAL" shell input keyevent "$1" ;;
  uninstall) uninstall ;; down) down ;;
  all) up; install "$@"; launch; sleep 8; shot 01-launch ;;
  *) sed -n '2,11p' "$0"; exit 2 ;;
esac
