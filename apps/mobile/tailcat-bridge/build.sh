#!/bin/sh
# Builds the gomobile tailcat bridge and stages the artifacts into the mobile
# app's native module. Requires: Go, gomobile (go install
# golang.org/x/mobile/cmd/gomobile@<pin> golang.org/x/mobile/cmd/gobind@<pin>),
# an Android SDK with an NDK, and — for the iOS half only — Xcode.
#
# The bridge source is bridge.go in this directory; the mobile app consumes the
# artifacts via modules/ghostex-native (gradle libs/ dir on Android, podspec
# vendored framework on iOS).
#
# Usage: build.sh [--target android|ios|all]   (default: all)
#
# The Android release workflow calls `--target android` because its ubuntu
# runner has no Xcode and the iOS bind would fail there. `--target all` stays the
# default so local invocations keep producing both halves.
set -eu
cd "$(dirname "$0")"

TARGET=all
while [ "$#" -gt 0 ]; do
  case "$1" in
    --target)
      [ "$#" -ge 2 ] || { echo "--target requires a value" >&2; exit 2; }
      TARGET="$2"
      shift 2
      ;;
    --target=*)
      TARGET="${1#--target=}"
      shift
      ;;
    -h | --help)
      echo "usage: $0 [--target android|ios|all]"
      exit 0
      ;;
    *)
      echo "unknown argument: $1 (usage: $0 [--target android|ios|all])" >&2
      exit 2
      ;;
  esac
done

case "$TARGET" in
  android | ios | all) ;;
  *)
    echo "unknown --target $TARGET (expected android, ios, or all)" >&2
    exit 2
    ;;
esac

build_android=no
build_ios=no
case "$TARGET" in
  android) build_android=yes ;;
  ios) build_ios=yes ;;
  all)
    build_android=yes
    build_ios=yes
    ;;
esac

export PATH="$HOME/go/bin:$PATH"

APP_MODULE="../app/modules/ghostex-native"

mkdir -p build

if [ "$build_android" = yes ]; then
  : "${ANDROID_HOME:=/opt/homebrew/share/android-commandlinetools}"
  export ANDROID_HOME
  if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$(ls "$ANDROID_HOME/ndk" | sort -V | tail -1)"
  fi
  export ANDROID_NDK_HOME
  echo "== android aar (NDK: $ANDROID_NDK_HOME)"
  gomobile bind -target=android -androidapi 24 -javapkg dev.ghostex \
    -o build/tailcatbridge.aar .

  echo "== staging android into $APP_MODULE"
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
fi

if [ "$build_ios" = yes ]; then
  echo "== ios xcframework"
  gomobile bind -target=ios,iossimulator -o build/Tailcatbridge.xcframework .

  echo "== staging ios into $APP_MODULE"
  if [ -d "$APP_MODULE/ios/Vendor/Tailcatbridge.xcframework" ]; then
    rm -rf "$APP_MODULE/ios/Vendor/Tailcatbridge.xcframework"
  fi
  cp -R build/Tailcatbridge.xcframework "$APP_MODULE/ios/Vendor/Tailcatbridge.xcframework"
fi

echo "done"
