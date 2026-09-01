#!/bin/sh
# Builds the gomobile tailcat bridge and stages the artifacts into the mobile
# app's native module. Requires: Go, gomobile (go install
# golang.org/x/mobile/cmd/gomobile@latest golang.org/x/mobile/cmd/gobind@latest),
# Xcode, and an Android SDK with an NDK.
#
# The bridge source is bridge.go in this directory; the mobile app consumes the
# artifacts via modules/ghostex-native (gradle libs/ dir on Android, podspec
# vendored framework on iOS).
set -eu
cd "$(dirname "$0")"

export PATH="$HOME/go/bin:$PATH"
: "${ANDROID_HOME:=/opt/homebrew/share/android-commandlinetools}"
export ANDROID_HOME
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
  ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$(ls "$ANDROID_HOME/ndk" | sort -V | tail -1)"
fi
export ANDROID_NDK_HOME

APP_MODULE="../app/modules/ghostex-native"

mkdir -p build
echo "== android aar (NDK: $ANDROID_NDK_HOME)"
gomobile bind -target=android -androidapi 24 -javapkg dev.ghostex \
  -o build/tailcatbridge.aar .
echo "== ios xcframework"
gomobile bind -target=ios,iossimulator -o build/Tailcatbridge.xcframework .

echo "== staging into $APP_MODULE"
# AGP refuses direct local .aar dependencies inside a library module, so the
# aar is staged unpacked: classes.jar as a local jar dep (embedded into the
# module's AAR) and the per-ABI gojni shared objects as jniLibs.
UNPACK=build/aar-unpack
rm -rf "$UNPACK"
mkdir -p "$UNPACK"
(cd "$UNPACK" && unzip -o -q ../tailcatbridge.aar classes.jar 'jni/*')
mkdir -p "$APP_MODULE/android/libs"
cp "$UNPACK/classes.jar" "$APP_MODULE/android/libs/tailcatbridge.jar"
for abi in arm64-v8a armeabi-v7a x86 x86_64; do
  mkdir -p "$APP_MODULE/android/src/main/jniLibs/$abi"
  cp "$UNPACK/jni/$abi/libgojni.so" "$APP_MODULE/android/src/main/jniLibs/$abi/"
done
if [ -d "$APP_MODULE/ios/Vendor/Tailcatbridge.xcframework" ]; then
  rm -rf "$APP_MODULE/ios/Vendor/Tailcatbridge.xcframework"
fi
cp -R build/Tailcatbridge.xcframework "$APP_MODULE/ios/Vendor/Tailcatbridge.xcframework"
echo "done"
