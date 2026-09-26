#!/bin/sh
# Builds the Rust chat core for the mobile app and stages it into the Expo module
# apps/mobile/app/modules/gx-chat-core, together with the UniFFI Swift and Kotlin bindings.
#
#   packages/gx-chat-mobile/build.sh [--target ios|android|all] [--debug]   (default: all, release)
#
# Outputs (all gitignored, reproducible from this crate):
#   ios/Vendor/GxChatMobile.xcframework        aarch64-apple-ios + aarch64-apple-ios-sim static libs
#   ios/Generated/gx_chat_mobile.swift         UniFFI Swift bindings
#   android/src/main/jniLibs/<abi>/libgx_chat_mobile.so   (GX_CHAT_ANDROID_ABIS, default arm64-v8a x86_64;
#                                                          releases build all four APK ABIs)
#   android/src/main/java/dev/ghostex/gxchatcore/uniffi/gx_chat_mobile.kt   UniFFI Kotlin bindings
#
# Needs: rustup (the crate pins 1.95.0 and its targets in rust-toolchain.toml), Xcode for iOS,
# cargo-ndk (`cargo install cargo-ndk`) and an Android NDK for Android. Run it once before
# `bunx expo run:ios` / `run:android`, and again after gx-chat-core changes.
set -eu
cd "$(dirname "$0")"
CRATE_DIR="$(pwd)"

TARGET=all
PROFILE=release
while [ "$#" -gt 0 ]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --target=*) TARGET="${1#--target=}"; shift ;;
    --debug) PROFILE=debug; shift ;;
    -h | --help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$TARGET" in
  ios | android | all) ;;
  *) echo "unknown --target $TARGET (expected ios, android or all)" >&2; exit 2 ;;
esac

CARGO_PROFILE_FLAG="--release"
[ "$PROFILE" = debug ] && CARGO_PROFILE_FLAG=""

MODULE_DIR="$CRATE_DIR/../../apps/mobile/app/modules/gx-chat-core"
[ -d "$MODULE_DIR" ] || { echo "Expo module not found at $MODULE_DIR (is the apps/mobile/app submodule checked out?)" >&2; exit 1; }

# The crate's rust-toolchain.toml selects the compiler; make sure its mobile targets exist.
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android x86_64-linux-android >/dev/null

# Bindings come from the host build of the same crate (UniFFI library mode reads the metadata
# compiled into the library), so they always match the scaffolding in the mobile libraries.
echo "== host library + bindgen"
cargo build $CARGO_PROFILE_FLAG --lib
cargo build --release --features bindgen --bin uniffi-bindgen
BINDGEN="$CRATE_DIR/target/release/uniffi-bindgen"
HOST_LIB="$CRATE_DIR/target/$PROFILE/libgx_chat_mobile.dylib"
[ -f "$HOST_LIB" ] || HOST_LIB="$CRATE_DIR/target/$PROFILE/libgx_chat_mobile.so"
BUILD_DIR="$CRATE_DIR/build"
rm -rf "$BUILD_DIR/swift" "$BUILD_DIR/kotlin"
mkdir -p "$BUILD_DIR"

if [ "$TARGET" = ios ] || [ "$TARGET" = all ]; then
  echo "== swift bindings"
  "$BINDGEN" generate --library "$HOST_LIB" --language swift --out-dir "$BUILD_DIR/swift"
  # Same floor as the ghostex-native pod, so the linker never sees objects newer than the app.
  export IPHONEOS_DEPLOYMENT_TARGET=16.4
  for triple in aarch64-apple-ios aarch64-apple-ios-sim; do
    echo "== $triple"
    cargo build $CARGO_PROFILE_FLAG --lib --target "$triple"
  done
  HEADERS="$BUILD_DIR/swift/headers"
  mkdir -p "$HEADERS"
  cp "$BUILD_DIR/swift/gx_chat_mobileFFI.h" "$HEADERS/"
  cp "$BUILD_DIR/swift/gx_chat_mobileFFI.modulemap" "$HEADERS/module.modulemap"
  XCFRAMEWORK="$MODULE_DIR/ios/Vendor/GxChatMobile.xcframework"
  rm -rf "$XCFRAMEWORK"
  mkdir -p "$MODULE_DIR/ios/Vendor" "$MODULE_DIR/ios/Generated"
  xcodebuild -create-xcframework \
    -library "target/aarch64-apple-ios/$PROFILE/libgx_chat_mobile.a" -headers "$HEADERS" \
    -library "target/aarch64-apple-ios-sim/$PROFILE/libgx_chat_mobile.a" -headers "$HEADERS" \
    -output "$XCFRAMEWORK" >/dev/null
  cp "$BUILD_DIR/swift/gx_chat_mobile.swift" "$MODULE_DIR/ios/Generated/gx_chat_mobile.swift"
  echo "   staged $(du -sh "$XCFRAMEWORK" | cut -f1) $XCFRAMEWORK"
fi

if [ "$TARGET" = android ] || [ "$TARGET" = all ]; then
  echo "== kotlin bindings"
  "$BINDGEN" generate --library "$HOST_LIB" --language kotlin --out-dir "$BUILD_DIR/kotlin" --no-format
  : "${ANDROID_HOME:=/opt/homebrew/share/android-commandlinetools}"
  export ANDROID_HOME
  if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$(ls "$ANDROID_HOME/ndk" | sort -V | tail -1)"
  fi
  export ANDROID_NDK_HOME
  ABIS="${GX_CHAT_ANDROID_ABIS:-arm64-v8a x86_64}"
  NDK_TARGETS=""
  for abi in $ABIS; do
    NDK_TARGETS="$NDK_TARGETS -t $abi"
    case "$abi" in
      arm64-v8a) rustup target add aarch64-linux-android >/dev/null ;;
      armeabi-v7a) rustup target add armv7-linux-androideabi >/dev/null ;;
      x86) rustup target add i686-linux-android >/dev/null ;;
      x86_64) rustup target add x86_64-linux-android >/dev/null ;;
      *) echo "unknown Android ABI: $abi" >&2; exit 2 ;;
    esac
  done
  echo "== android ($ABIS, NDK $ANDROID_NDK_HOME)"
  JNI_LIBS="$MODULE_DIR/android/src/main/jniLibs"
  rm -rf "$JNI_LIBS"
  # shellcheck disable=SC2086
  cargo ndk $NDK_TARGETS --platform 24 -o "$JNI_LIBS" build $CARGO_PROFILE_FLAG --lib
  KOTLIN_OUT="$MODULE_DIR/android/src/main/java/dev/ghostex/gxchatcore/uniffi"
  mkdir -p "$KOTLIN_OUT"
  cp "$BUILD_DIR/kotlin/dev/ghostex/gxchatcore/uniffi/gx_chat_mobile.kt" "$KOTLIN_OUT/gx_chat_mobile.kt"
  echo "   staged $(du -sh "$JNI_LIBS" | cut -f1) $JNI_LIBS"
fi
echo "== done"
