#!/usr/bin/env bash
# One-time (idempotent) setup: cmdline-tools, platform-tools, emulator, an x86_64
# Google APIs system image and a headless AVD, all under ~/.local/share/agent-work/android.
set -euo pipefail
source "$(dirname "$0")/env.sh"
[ -r /dev/kvm ] && [ -w /dev/kvm ] || { echo "No rw access to /dev/kvm. Run: sudo usermod -aG kvm $USER (then log out/in)" >&2; exit 3; }
mkdir -p "$ANDROID_HOME" "$ANDROID_AVD_HOME" "$ANDROID_EMULATOR_HOME"
if [ ! -x "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" ]; then
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
  curl -fL --retry 3 -o "$tmp/clt.zip" https://dl.google.com/android/repository/commandlinetools-linux-13114758_latest.zip
  unzip -q "$tmp/clt.zip" -d "$tmp"
  mkdir -p "$ANDROID_HOME/cmdline-tools"; rm -rf "$ANDROID_HOME/cmdline-tools/latest"
  mv "$tmp/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
fi
yes | sdkmanager --licenses >/dev/null || true
sdkmanager --install "platform-tools" "emulator" "$GX_SYSTEM_IMAGE"
if ! avdmanager list avd -c | grep -qx "$GX_AVD_NAME"; then
  echo no | avdmanager create avd -n "$GX_AVD_NAME" -k "$GX_SYSTEM_IMAGE" -d pixel_7 --force
  cfg="$ANDROID_AVD_HOME/$GX_AVD_NAME.avd/config.ini"
  # Headless-friendly: software GPU, no audio/camera, enough RAM and disk for the app.
  for kv in "hw.gpu.enabled=yes" "hw.gpu.mode=swiftshader_indirect" "hw.ramSize=4096" "disk.dataPartition.size=6G" \
            "hw.audioInput=no" "hw.audioOutput=no" "hw.camera.back=none" "hw.camera.front=none" "hw.keyboard=yes" "fastboot.forceColdBoot=no"; do
    k="${kv%%=*}"; grep -v "^$k=" "$cfg" > "$cfg.new" || true; echo "$kv" >> "$cfg.new"; mv "$cfg.new" "$cfg"
  done
fi
echo "SDK: $ANDROID_HOME"; echo "AVD: $ANDROID_AVD_HOME/$GX_AVD_NAME.avd"
sdkmanager --list_installed 2>/dev/null | sed -n '1,20p'
