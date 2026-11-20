# Source me: Android SDK + AVD under agent-work (no sudo, separate from ~/Android/Sdk).
export GX_ANDROID_ROOT="$HOME/.local/share/agent-work/android"
export ANDROID_HOME="$GX_ANDROID_ROOT/sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_AVD_HOME="$GX_ANDROID_ROOT/avd"
export ANDROID_EMULATOR_HOME="$GX_ANDROID_ROOT/emulator-home"
# sdkmanager/avdmanager need Java 17+; mise has temurin-17 installed.
if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/java" ]; then
  for j in "$HOME"/.local/share/mise/installs/java/temurin-17*; do [ -x "$j/bin/java" ] && JAVA_HOME="$j"; done
  export JAVA_HOME
fi
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$JAVA_HOME/bin:$PATH"
export GX_AVD_NAME="${GX_AVD_NAME:-gx-smoke-api35}"
export GX_SYSTEM_IMAGE="${GX_SYSTEM_IMAGE:-system-images;android-35;google_apis;x86_64}"
# Own adb server port so we never collide with another agent's/user's adb on 5037.
export ANDROID_ADB_SERVER_PORT="${ANDROID_ADB_SERVER_PORT:-5047}"
export GX_EMU_PORT="${GX_EMU_PORT:-5682}"
export ANDROID_SERIAL="emulator-$GX_EMU_PORT"
