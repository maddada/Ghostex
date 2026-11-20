#!/usr/bin/env bash
# Build an x86_64 (emulator) release-mode APK of the phone app at the commit a Ghostex tag records,
# in a throwaway copy-on-write copy, signed with a throwaway key. Test fixture only: the published
# ghostex-android.apk is arm64-only and its Go bridge (libgojni.so) dies under the emulator's
# ARM translation (berberis: "Undefined instruction 0xd5380600", MRS ID_AA64ISAR0_EL1).
#   build-x86_64-apk.sh [vX.Y.Z]   -> $GX_ANDROID_ROOT/apk/ghostex-x86_64-<tag>.apk
set -euo pipefail
source "$(dirname "$0")/env.sh"
TAG="${1:-$(gh release view -R maddada/Ghostex --json tagName -q .tagName)}"
REPO="${GX_REPO:-$HOME/dev/ghostex}"
PHONE_SHA="$(git -C "$REPO" ls-tree "$TAG" apps/mobile/app | awk '{print $3}')"
WORK="$GX_ANDROID_ROOT/build/$TAG"
OUT="$GX_ANDROID_ROOT/apk/ghostex-x86_64-$TAG.apk"
echo "tag $TAG phone commit $PHONE_SHA -> $WORK"
rm -rf "$WORK"; mkdir -p "$WORK" "$GX_ANDROID_ROOT/apk"
# app.config.js hashes the main repo's tailcat bridge and runs `git ls-files` in both repos, so the
# build needs the main-repo layout: a --shared clone of the main repo at the tag (no change to the
# shared checkout or its .git), the phone repo cloned the same way at the tag's phone commit, then
# node_modules and the staged gomobile libs (untracked; the x86_64 libgojni.so may be older than
# the tag) reflink-copied in from the main checkout (btrfs).
git clone -q --shared --no-checkout "$REPO" "$WORK/repo"
git -C "$WORK/repo" -c advice.detachedHead=false checkout -q "$TAG"
rm -rf "$WORK/repo/apps/mobile/app"
git clone -q --shared --no-checkout "$REPO/.git/modules/mobile" "$WORK/repo/apps/mobile/app"
git -C "$WORK/repo/apps/mobile/app" -c advice.detachedHead=false checkout -q "$PHONE_SHA"
cp -a --reflink=auto "$REPO/apps/mobile/app/node_modules" "$WORK/repo/apps/mobile/app/node_modules"
JNI=modules/ghostex-native/android/src/main/jniLibs
mkdir -p "$WORK/repo/apps/mobile/app/$JNI"
cp -a --reflink=auto "$REPO/apps/mobile/app/$JNI/." "$WORK/repo/apps/mobile/app/$JNI/"
LIBS=modules/ghostex-native/android/libs
mkdir -p "$WORK/repo/apps/mobile/app/$LIBS"
cp -a --reflink=auto "$REPO/apps/mobile/app/$LIBS/." "$WORK/repo/apps/mobile/app/$LIBS/"
[ -s "$WORK/repo/apps/mobile/app/$LIBS/tailcatbridge.jar" ] && [ -s "$WORK/repo/apps/mobile/app/$JNI/x86_64/libgojni.so" ] || { echo "the gomobile bridge (tailcatbridge.jar + jniLibs/x86_64/libgojni.so) is not staged in $REPO/apps/mobile/app" >&2; exit 2; }
# wasm-opt (binaryen) without sudo: the upstream prebuilt Linux release, checksum-verified.
if ! command -v wasm-opt >/dev/null; then
  BIN_DIR="$GX_ANDROID_ROOT/tools/binaryen"
  if [ ! -x "$BIN_DIR/bin/wasm-opt" ]; then
    dl="$(mktemp -d)"; ( cd "$dl" && gh release download version_133 -R WebAssembly/binaryen -p 'binaryen-version_133-x86_64-linux.tar.gz*' && sha256sum -c binaryen-version_133-x86_64-linux.tar.gz.sha256 )
    mkdir -p "$BIN_DIR"; tar -xzf "$dl/binaryen-version_133-x86_64-linux.tar.gz" -C "$BIN_DIR" --strip-components=1; rm -rf "$dl"
  fi
  export PATH="$BIN_DIR/bin:$PATH"
fi
# The phone's chat core page is a gitignored build product: build it from the tag's sources.
(cd "$WORK/repo" && "${CARGO:-$HOME/.cargo/bin/cargo}" xtask build-mobile-chat-wasm)
ls -la "$WORK/repo/apps/mobile/app/assets/chat-core/chat-core.html"
KS="$GX_ANDROID_ROOT/throwaway-release.keystore"
[ -f "$KS" ] || keytool -genkeypair -v -keystore "$KS" -storepass smoketest -keypass smoketest -alias smoke \
  -keyalg RSA -keysize 2048 -validity 3650 -dname "CN=Ghostex smoke fixture" >/dev/null 2>&1
export ORG_GRADLE_PROJECT_ghostexReleaseStoreFile="$KS" ORG_GRADLE_PROJECT_ghostexReleaseStorePassword=smoketest \
  ORG_GRADLE_PROJECT_ghostexReleaseKeyAlias=smoke ORG_GRADLE_PROJECT_ghostexReleaseKeyPassword=smoketest
VERSION="${TAG#v}"
export GHOSTEX_RELEASE_VERSION="$VERSION" GHOSTEX_RELEASE_APPLICATION_ID=io.ghostex GHOSTEX_ANDROID_SELF_UPDATE=0 \
  GHOSTEX_RELEASE_SOURCE_KIND=react-native-mobile
export GRADLE_USER_HOME="$GX_ANDROID_ROOT/gradle-home"
cd "$WORK/repo/apps/mobile/app"
bunx expo prebuild --platform android --no-install
./android/gradlew --no-daemon -p android "-Dorg.gradle.jvmargs=-Xmx6g -XX:MaxMetaspaceSize=1g -Dfile.encoding=UTF-8" \
  -PreactNativeArchitectures=x86_64 :app:assembleRelease
cp android/app/build/outputs/apk/release/app-release.apk "$OUT"
echo "built $OUT"
