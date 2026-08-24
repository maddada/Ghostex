#!/usr/bin/env bash
# CDXC:GPUIWindowsWslStart 2026-08-02:
# Build and stage the native Win32 GPUI app from a WSL-owned `bun run start`
# without routing any part of the workflow through PowerShell. Web resources
# build with Windows Bun so Vite reads the NTFS checkout natively instead of
# crawling thousands of modules through WSL's p9 mount. The pinned Windows
# Rust/MSVC/CMake/Ninja/Zig tools use the same interop path. Development keeps
# flat CEF layout; release staging emits the native component bootstrap and a
# sealed CEF component asset.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GPUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$GPUI_DIR/../.." && pwd)"
APP_NAME="Ghostex"
APP_DIR="$GPUI_DIR/build/windows/$APP_NAME"
RELEASE_ARCH="${GHOSTEX_WINDOWS_ARCH:-x64}"
ON_DEMAND_COMPONENTS="${GHOSTEX_ON_DEMAND_ASSETS:-0}"
RELEASE_VERSION="${GHOSTEX_GPUI_MARKETING_VERSION:-$(node -p "require('$REPO_ROOT/package.json').version")}"
CMD_EXE="/mnt/c/Windows/System32/cmd.exe"
ROBOCOPY_EXE="/mnt/c/Windows/System32/robocopy.exe"
WINDOWS_TAR_EXE="/mnt/c/Windows/System32/tar.exe"
WINDOWS_CERTUTIL_EXE="/mnt/c/Windows/System32/certutil.exe"

case "$RELEASE_ARCH" in
  x64)
    RUST_TARGET="x86_64-pc-windows-msvc"
    WSL_RUST_TARGET="x86_64-unknown-linux-musl"
    WSL_ZIG_TARGET="x86_64-linux-musl"
    WSL_RUST_ENV_SUFFIX="X86_64_UNKNOWN_LINUX_MUSL"
    VS_ARCH="x64"
    ;;
  arm64)
    RUST_TARGET="aarch64-pc-windows-msvc"
    WSL_RUST_TARGET="aarch64-unknown-linux-musl"
    WSL_ZIG_TARGET="aarch64-linux-musl"
    WSL_RUST_ENV_SUFFIX="AARCH64_UNKNOWN_LINUX_MUSL"
    VS_ARCH="arm64"
    ;;
  *)
    echo "GHOSTEX_WINDOWS_ARCH must be x64 or arm64, got $RELEASE_ARCH" >&2
    exit 1
    ;;
esac

if [[ ! -x "$CMD_EXE" || ! -x "$ROBOCOPY_EXE" || ! -x "$WINDOWS_TAR_EXE" || ! -x "$WINDOWS_CERTUTIL_EXE" ]] ||
  ! grep -qi microsoft /proc/sys/kernel/osrelease; then
  echo "build-windows-app-wsl.sh must run inside WSL2." >&2
  exit 1
fi

windows_robocopy() {
  local source_path="$1"
  local destination_path="$2"
  local source_windows destination_windows robocopy_status
  shift 2
  source_windows="$(wslpath -a -w "$source_path")"
  destination_windows="$(wslpath -a -w "$destination_path")"

  # Robocopy uses exit codes 0-7 for successful copy outcomes. Detach its output
  # from the terminal so Windows never starts a console cursor-position query;
  # the caller's phase messages provide the useful progress signal.
  set +e
  "$ROBOCOPY_EXE" "$source_windows" "$destination_windows" "$@" \
    /R:2 /W:1 /NFL /NDL /NJH /NJS /NP >/dev/null 2>&1
  robocopy_status="$?"
  set -e
  if ((robocopy_status > 7)); then
    echo "robocopy failed from $source_windows to $destination_windows (exit $robocopy_status)." >&2
    return "$robocopy_status"
  fi
}

windows_sha256() {
  local source_path="$1"
  local source_windows certutil_output certutil_status hash
  source_windows="$(wslpath -a -w "$source_path")"
  set +e
  certutil_output="$("$WINDOWS_CERTUTIL_EXE" -hashfile "$source_windows" SHA256 2>&1)"
  certutil_status="$?"
  set -e
  hash="$(printf '%s\n' "$certutil_output" | tr -d '\r ' | awk '/^[0-9a-fA-F]+$/ && length($0) == 64 { print tolower($0); exit }')"
  if [[ "$certutil_status" -ne 0 || -z "$hash" ]]; then
    echo "Could not calculate the Windows SHA-256 for $source_windows." >&2
    return 1
  fi
  printf '%s\n' "$hash"
}

report_build_phase() {
  local message="$1"
  if [[ -n "${GHOSTEX_WINDOWS_BUILD_PROGRESS_PATH:-}" && -w "$GHOSTEX_WINDOWS_BUILD_PROGRESS_PATH" ]]; then
    printf '    %s\n' "$message" >"$GHOSTEX_WINDOWS_BUILD_PROGRESS_PATH"
  else
    printf '%s\n' "$message"
  fi
}

WINDOWS_PROFILE_RAW="$($CMD_EXE /d /s /c "echo %USERPROFILE%" | tr -d '\r' | tail -n 1)"
WINDOWS_PROFILE="$(wslpath -a -u "$WINDOWS_PROFILE_RAW")"
WINDOWS_TOOLS_ROOT="${GHOSTEX_WINDOWS_TOOLS_ROOT:-$WINDOWS_PROFILE/apps/ghostex-build-tools}"

first_existing_file() {
  local candidate
  for candidate in "$@"; do
    if [[ -f "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

VS_DEV_CMD="$(first_existing_file \
  "${GHOSTEX_WINDOWS_VS_DEV_CMD:-}" \
  "$WINDOWS_PROFILE/apps/vs-buildtools/Common7/Tools/VsDevCmd.bat" \
  /mnt/c/Program\ Files/Microsoft\ Visual\ Studio/2022/*/Common7/Tools/VsDevCmd.bat \
  /mnt/c/Program\ Files\ \(x86\)/Microsoft\ Visual\ Studio/2022/*/Common7/Tools/VsDevCmd.bat \
  || true)"
WINDOWS_CARGO="$(first_existing_file \
  "${GHOSTEX_WINDOWS_CARGO:-}" \
  "$WINDOWS_PROFILE/.cargo/bin/cargo.exe" \
  || true)"
WINDOWS_RUSTUP="$(first_existing_file \
  "${GHOSTEX_WINDOWS_RUSTUP:-}" \
  "$WINDOWS_PROFILE/.cargo/bin/rustup.exe" \
  || true)"
WINDOWS_BUN="$(first_existing_file \
  "${GHOSTEX_WINDOWS_BUN:-}" \
  "$WINDOWS_PROFILE/.bun/bin/bun.exe" \
  || true)"
WINDOWS_CMAKE="$(first_existing_file \
  "${GHOSTEX_WINDOWS_CMAKE:-}" \
  "$WINDOWS_TOOLS_ROOT/cmake/cmake-4.4.2-windows-x86_64/bin/cmake.exe" \
  "/mnt/c/Program Files/CMake/bin/cmake.exe" \
  || true)"
WINDOWS_NINJA="$(first_existing_file \
  "${GHOSTEX_WINDOWS_NINJA:-}" \
  "$WINDOWS_TOOLS_ROOT/ninja/ninja.exe" \
  /mnt/c/Program\ Files/Microsoft\ Visual\ Studio/2022/*/Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja/ninja.exe \
  /mnt/c/Program\ Files\ \(x86\)/Microsoft\ Visual\ Studio/2022/*/Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja/ninja.exe \
  || true)"
WINDOWS_ZIG="$(first_existing_file \
  "${GHOSTEX_WINDOWS_ZIG:-}" \
  "$WINDOWS_TOOLS_ROOT/zig/zig-x86_64-windows-0.16.0/zig.exe" \
  "/mnt/c/tools/zig-x86_64-windows-0.16.0/zig.exe" \
  || true)"

for required_name in VS_DEV_CMD WINDOWS_CARGO WINDOWS_RUSTUP WINDOWS_BUN WINDOWS_CMAKE WINDOWS_NINJA WINDOWS_ZIG; do
  required_path="${!required_name:-}"
  if [[ -z "$required_path" || ! -f "$required_path" ]]; then
    echo "Required Windows build tool $required_name is unavailable." >&2
    exit 1
  fi
done

if [[ "$($WINDOWS_ZIG version | tr -d '\r')" != "0.16.0" ]]; then
  echo "Ghostex requires Windows Zig 0.16.0 at $WINDOWS_ZIG." >&2
  exit 1
fi

WSL_ZIG_VERSION="0.16.0"
case "$(uname -m)" in
  x86_64)
    WSL_ZIG_HOST_ARCH="x86_64"
    WSL_ZIG_SHA256="70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00"
    ;;
  aarch64 | arm64)
    WSL_ZIG_HOST_ARCH="aarch64"
    WSL_ZIG_SHA256="ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17"
    ;;
  *)
    echo "Unsupported WSL build host architecture: $(uname -m)" >&2
    exit 1
    ;;
esac
WSL_ZIG_ROOT="${GHOSTEX_WINDOWS_WSL_ZIG_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/ghostex/build-tools/zig-$WSL_ZIG_HOST_ARCH-linux-$WSL_ZIG_VERSION}"
WSL_ZIG="$WSL_ZIG_ROOT/zig"
if [[ ! -x "$WSL_ZIG" ]]; then
  wsl_zig_archive="$(mktemp "/tmp/ghostex-wsl-zig-$WSL_ZIG_VERSION.XXXXXX.tar.xz")"
  wsl_zig_cleanup_command="$(printf 'rm -f -- %q' "$wsl_zig_archive")"
  trap "$wsl_zig_cleanup_command" EXIT
  curl -fsSL --retry 3 \
    "https://ziglang.org/download/$WSL_ZIG_VERSION/zig-$WSL_ZIG_HOST_ARCH-linux-$WSL_ZIG_VERSION.tar.xz" \
    -o "$wsl_zig_archive"
  printf '%s  %s\n' "$WSL_ZIG_SHA256" "$wsl_zig_archive" | sha256sum -c -
  mkdir -p "$(dirname "$WSL_ZIG_ROOT")"
  tar -xJf "$wsl_zig_archive" -C "$(dirname "$WSL_ZIG_ROOT")"
  rm -f -- "$wsl_zig_archive"
  trap - EXIT
fi
if [[ "$($WSL_ZIG version)" != "$WSL_ZIG_VERSION" ]]; then
  echo "Ghostex requires WSL Zig $WSL_ZIG_VERSION at $WSL_ZIG." >&2
  exit 1
fi

CEF_CACHE="${GHOSTEX_WINDOWS_CEF_PATH:-$GPUI_DIR/build/cef-cache-windows}"
CARGO_OUTPUT_ROOT="${GHOSTEX_WINDOWS_CARGO_TARGET_DIR:-$GPUI_DIR/build/windows-target}"
WSL_GXSERVER_CARGO_OUTPUT_ROOT="${GHOSTEX_WINDOWS_WSL_GXSERVER_TARGET_DIR:-$GPUI_DIR/build/windows-wsl-gxserver-target}"
WSL_ZMX_CURRENT_PREFIX="$WSL_GXSERVER_CARGO_OUTPUT_ROOT/zmx-current"
WSL_ZMX_CACHE_DIR="${GHOSTEX_WINDOWS_WSL_ZMX_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/ghostex/zmx-build-cache/$WSL_ZIG_TARGET}"
ZIG_CACHE="${GHOSTEX_WINDOWS_ZIG_CACHE_DIR:-$GPUI_DIR/build/zig-global-cache-windows}"
mkdir -p "$CEF_CACHE" "$CARGO_OUTPUT_ROOT" "$WSL_GXSERVER_CARGO_OUTPUT_ROOT" "$WSL_ZMX_CURRENT_PREFIX" "$WSL_ZMX_CACHE_DIR" "$ZIG_CACHE"

# Windows Bun keeps the large Vite module graph on native NTFS. Running Linux
# Node against this /mnt/c checkout serializes more than ten thousand small-file
# reads through WSL's p9 client and makes the build appear hung for many minutes.
# Keep Windows Bun's output on a pipe so verbose WSL starts stream normally
# without Windows console cursor-position handshakes; pipefail preserves errors.
GPUI_DIR_WIN="$(wslpath -a -w "$GPUI_DIR")"
REPO_ROOT_WIN="$(wslpath -a -w "$REPO_ROOT")"
report_build_phase "Building sidebar CSS and CEF web assets..."
(
  cd "$REPO_ROOT"
  "$WINDOWS_BUN" run build:sidebar-css 2>&1 | cat
  "$REPO_ROOT/node_modules/.bin/vite.exe" build --config "$GPUI_DIR_WIN\\vite.config.ts" 2>&1 | cat
)

VS_DEV_CMD_WIN="$(wslpath -a -w "$VS_DEV_CMD")"
WINDOWS_RUSTUP_WIN="$(wslpath -a -w "$WINDOWS_RUSTUP")"
WINDOWS_CMAKE_DIR_WIN="$(wslpath -a -w "$(dirname "$WINDOWS_CMAKE")")"
WINDOWS_NINJA_DIR_WIN="$(wslpath -a -w "$(dirname "$WINDOWS_NINJA")")"
WINDOWS_CARGO_DIR_WIN="$(wslpath -a -w "$(dirname "$WINDOWS_CARGO")")"
# rustup installs cargo.exe as a symlink to rustup.exe. Converting the complete
# WSL path dereferences that symlink, so preserve the proxy basename explicitly.
WINDOWS_CARGO_WIN="$WINDOWS_CARGO_DIR_WIN\\$(basename "$WINDOWS_CARGO")"
WINDOWS_ZIG_WIN="$(wslpath -a -w "$WINDOWS_ZIG")"
CEF_CACHE_WIN="$(wslpath -a -w "$CEF_CACHE")"
CARGO_OUTPUT_ROOT_WIN="$(wslpath -a -w "$CARGO_OUTPUT_ROOT")"
ZIG_CACHE_WIN="$(wslpath -a -w "$ZIG_CACHE")"
WSL_GXSERVER_CARGO_OUTPUT_ROOT_WIN="$(wslpath -a -w "$WSL_GXSERVER_CARGO_OUTPUT_ROOT")"

# A generated batch file is the only reliable quoting boundary here. Passing a
# nested command string through WSL interop preserves literal `\"` characters,
# while separate `set` tokens lose cmd's protective quotes and append the space
# before `&&` to values such as CARGO_TARGET_DIR.
WINDOWS_BUILD_BATCH="$(mktemp "$GPUI_DIR/build/.windows-build.XXXXXX.cmd")"
WINDOWS_BUILD_BATCH_WIN="$(wslpath -a -w "$WINDOWS_BUILD_BATCH")"
cleanup_windows_build_batch() {
  rm -f -- "$WINDOWS_BUILD_BATCH"
}
trap cleanup_windows_build_batch EXIT
# Keep the native build independent of user PATH entries, which may point at
# disconnected or BitLocker-locked volumes. VsDevCmd adds the MSVC and SDK
# directories before the pinned Ghostex tools are prepended.
printf '%s\r\n' \
  '@echo off' \
  'set "PATH=%SystemRoot%\System32;%SystemRoot%;%SystemRoot%\System32\Wbem;%SystemRoot%\System32\WindowsPowerShell\v1.0"' \
  "set \"CEF_PATH=$CEF_CACHE_WIN\"" \
  "set \"CARGO_TARGET_DIR=$CARGO_OUTPUT_ROOT_WIN\"" \
  "set \"GHOSTEX_ZIG=$WINDOWS_ZIG_WIN\"" \
  "set \"ZIG_GLOBAL_CACHE_DIR=$ZIG_CACHE_WIN\"" \
  "call \"$VS_DEV_CMD_WIN\" -arch=$VS_ARCH -host_arch=x64 >nul" \
  'if errorlevel 1 exit /b %errorlevel%' \
  "set \"PATH=$WINDOWS_CARGO_DIR_WIN;$WINDOWS_CMAKE_DIR_WIN;$WINDOWS_NINJA_DIR_WIN;%PATH%\"" \
  "cd /d \"$GPUI_DIR_WIN\"" \
  'if errorlevel 1 exit /b %errorlevel%' \
  "\"$WINDOWS_RUSTUP_WIN\" target add $RUST_TARGET" \
  'if errorlevel 1 exit /b %errorlevel%' \
  "\"$WINDOWS_CARGO_WIN\" build --release --bin ghostex-gpui-cef-bootstrap --bin ghostex-gpui --bin ghostex-gpui-cef-helper --target $RUST_TARGET" \
  'exit /b %errorlevel%' \
  >"$WINDOWS_BUILD_BATCH"
report_build_phase "Building the native Windows GPUI shell..."
$CMD_EXE /d /c call "$WINDOWS_BUILD_BATCH_WIN" 2>&1 | cat
cleanup_windows_build_batch
trap - EXIT

build_current_wsl_gxserver() {
  # CDXC:GPUIWindowsWslRuntime 2026-08-03:
  # A WSL-owned Windows development build must package gxserver and zmx from
  # the same checkout as the native shell. Previously `bun run start` rebuilt
  # GPUI but retained runtime pieces from a cached Linux archive, so daemon and
  # terminal-title protocol fixes could be missing from the installed app.
  #
  # Windows Rust build scripts cannot execute WSL compilers directly. Cross-build
  # the static Linux daemon with the installed Windows Rust target, use the
  # pinned Windows Zig as the C compiler/archive tool for SQLite and ring, and
  # use Rust's own LLD for the final Linux link.
  local cross_dir cc_wrapper ar_wrapper build_batch
  local cc_wrapper_win ar_wrapper_win build_batch_win rust_sysroot_win rust_lld_win
  # Cargo fingerprints the configured C compiler path. Keep these wrappers at
  # stable paths so an unchanged incremental start does not rebuild ring and
  # SQLite merely because a fresh temporary directory name was generated.
  cross_dir="$WSL_GXSERVER_CARGO_OUTPUT_ROOT/cross-tools-$WSL_RUST_TARGET"
  mkdir -p "$cross_dir"
  cc_wrapper="$cross_dir/zig-cc.cmd"
  ar_wrapper="$cross_dir/zig-ar.cmd"
  build_batch="$cross_dir/build-gxserver.cmd"

  cat >"$cc_wrapper" <<EOF
@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "args="
:next
if "%~1"=="" goto run
if /I "%~1"=="--target=$WSL_RUST_TARGET" goto skip
if /I "%~1"=="-target" goto skip_pair
set args=%args% "%~1"
:skip
shift
goto next
:skip_pair
shift
shift
goto next
:run
"$WINDOWS_ZIG_WIN" cc -target $WSL_ZIG_TARGET -fno-sanitize=undefined %args%
EOF
  cat >"$ar_wrapper" <<EOF
@echo off
"$WINDOWS_ZIG_WIN" ar %*
EOF

  cc_wrapper_win="$(wslpath -a -w "$cc_wrapper")"
  ar_wrapper_win="$(wslpath -a -w "$ar_wrapper")"
  build_batch_win="$(wslpath -a -w "$build_batch")"
  rust_sysroot_win="$($WINDOWS_RUSTUP run stable rustc --print sysroot | tr -d '\r')"
  rust_lld_win="$rust_sysroot_win\\lib\\rustlib\\x86_64-pc-windows-msvc\\bin\\rust-lld.exe"
  if [[ ! -f "$(wslpath -a -u "$rust_lld_win")" ]]; then
    echo "Rust LLD is unavailable for the WSL gxserver cross-build: $rust_lld_win" >&2
    exit 1
  fi

  printf '%s\r\n' \
    '@echo off' \
    "set \"PATH=$WINDOWS_CARGO_DIR_WIN;%PATH%\"" \
    "set \"CC_${WSL_RUST_TARGET//-/_}=$cc_wrapper_win\"" \
    "set \"AR_${WSL_RUST_TARGET//-/_}=$ar_wrapper_win\"" \
    "set \"CARGO_TARGET_${WSL_RUST_ENV_SUFFIX}_LINKER=$rust_lld_win\"" \
    "set \"CARGO_TARGET_${WSL_RUST_ENV_SUFFIX}_RUSTFLAGS=-C linker-flavor=ld.lld\"" \
    "set \"CARGO_TARGET_DIR=$WSL_GXSERVER_CARGO_OUTPUT_ROOT_WIN\"" \
    "cd /d \"$REPO_ROOT_WIN\"" \
    'if errorlevel 1 exit /b %errorlevel%' \
    "\"$WINDOWS_RUSTUP_WIN\" target add $WSL_RUST_TARGET" \
    'if errorlevel 1 exit /b %errorlevel%' \
    "\"$WINDOWS_CARGO_WIN\" build --release --manifest-path \"$REPO_ROOT_WIN\\server\\Cargo.toml\" --target $WSL_RUST_TARGET --bin gxserver" \
    'exit /b %errorlevel%' \
    >"$build_batch"

  "$CMD_EXE" /d /c call "$build_batch_win" 2>&1 | cat

  WSL_GXSERVER_CURRENT_BIN="$WSL_GXSERVER_CARGO_OUTPUT_ROOT/$WSL_RUST_TARGET/release/gxserver"
  if [[ ! -x "$WSL_GXSERVER_CURRENT_BIN" ]]; then
    echo "The current-source WSL gxserver build is missing: $WSL_GXSERVER_CURRENT_BIN" >&2
    exit 1
  fi
  (
    cd "$REPO_ROOT/.dependencies/zmx"
    "$WSL_ZIG" build \
      --cache-dir "$WSL_ZMX_CACHE_DIR" \
      -Doptimize=ReleaseSafe \
      -Dtarget="$WSL_ZIG_TARGET" \
      --prefix "$WSL_ZMX_CURRENT_PREFIX"
  )
  WSL_ZMX_CURRENT_BIN="$WSL_ZMX_CURRENT_PREFIX/bin/zmx"
  if [[ ! -x "$WSL_ZMX_CURRENT_BIN" ]]; then
    echo "The current-source WSL zmx build is missing: $WSL_ZMX_CURRENT_BIN" >&2
    exit 1
  fi
}

report_build_phase "Building the bundled WSL gxserver and zmx runtime..."
build_current_wsl_gxserver

CEF_RELEASE="$(dirname "$(find "$CEF_CACHE" -type f -iname libcef.dll -print -quit)")"
if [[ -z "$CEF_RELEASE" || ! -f "$CEF_RELEASE/libcef.dll" ]]; then
  echo "cef-rs did not produce libcef.dll under $CEF_CACHE" >&2
  exit 1
fi
CEF_RESOURCES="$CEF_RELEASE"
if [[ ! -f "$CEF_RESOURCES/icudtl.dat" ]]; then
  CEF_RESOURCES="$(dirname "$CEF_RELEASE")/Resources"
fi
CEF_DISTRIBUTION_ROOT="$CEF_RELEASE"
if [[ ! -f "$CEF_DISTRIBUTION_ROOT/include/cef_version.h" ]]; then
  CEF_DISTRIBUTION_ROOT="$(dirname "$CEF_RELEASE")"
fi
CEF_VERSION_HEADER="$CEF_DISTRIBUTION_ROOT/include/cef_version.h"
if [[ ! -f "$CEF_VERSION_HEADER" ]]; then
  echo "Could not locate cef_version.h for $CEF_RELEASE" >&2
  exit 1
fi
CEF_COMPONENT_VERSION="$(sed -n 's/^#define CEF_VERSION "\([^"]*\)"$/\1/p' "$CEF_VERSION_HEADER" | head -n 1 | sed 's/[^A-Za-z0-9._-]/-/g')"
if [[ -z "$CEF_COMPONENT_VERSION" ]]; then
  echo "Could not resolve the CEF component version from $CEF_VERSION_HEADER" >&2
  exit 1
fi
if [[ ! -f "$CEF_RESOURCES/icudtl.dat" ]]; then
  echo "CEF resources with icudtl.dat were not found beside $CEF_RELEASE" >&2
  exit 1
fi

# The directory contains generated staging output only. Keep its inode stable
# so terminals whose cwd points here do not retain a deleted directory handle.
# These sources and the destination all live on NTFS. Native robocopy avoids
# making WSL's p9 client perform every individual CEF/Vite file operation.
report_build_phase "Staging the Windows app bundle with native file copies..."
mkdir -p "$APP_DIR"
EMPTY_STAGE_DIR="$(mktemp -d "$GPUI_DIR/build/.windows-empty-stage.XXXXXX")"
empty_stage_cleanup_command="$(printf 'rmdir -- %q 2>/dev/null || true' "$EMPTY_STAGE_DIR")"
trap "$empty_stage_cleanup_command" EXIT
windows_robocopy "$EMPTY_STAGE_DIR" "$APP_DIR" /MIR
rmdir -- "$EMPTY_STAGE_DIR"
trap - EXIT

RUST_RELEASE_DIR="$CARGO_OUTPUT_ROOT/$RUST_TARGET/release"
if [[ "$ON_DEMAND_COMPONENTS" == "1" ]]; then
  cp "$RUST_RELEASE_DIR/ghostex-gpui-cef-bootstrap.exe" "$APP_DIR/Ghostex.exe"
  cp "$RUST_RELEASE_DIR/ghostex-gpui.exe" "$APP_DIR/ghostex-gpui-runtime.exe"
else
  cp "$RUST_RELEASE_DIR/ghostex-gpui.exe" "$APP_DIR/Ghostex.exe"
fi
cp "$RUST_RELEASE_DIR/ghostex-gpui-cef-helper.exe" "$APP_DIR/"
LOCALES_DIR=""
for locale_candidate in "$CEF_RELEASE/locales" "$CEF_RESOURCES/locales"; do
  if [[ -d "$locale_candidate" ]]; then
    LOCALES_DIR="$locale_candidate"
    break
  fi
done
if [[ -z "$LOCALES_DIR" ]]; then
  echo "CEF locales were not found beside $CEF_RELEASE" >&2
  exit 1
fi
if [[ "$ON_DEMAND_COMPONENTS" != "1" ]]; then
  for source_root in "$CEF_RELEASE" "$CEF_RESOURCES"; do
    windows_robocopy "$source_root" "$APP_DIR" '*.dll' '*.pak' '*.dat' '*.bin'
  done
  for swiftshader_icd in "$CEF_RELEASE/vk_swiftshader_icd.json" "$CEF_RESOURCES/vk_swiftshader_icd.json"; do
    if [[ -f "$swiftshader_icd" ]]; then
      cp "$swiftshader_icd" "$APP_DIR/"
      break
    fi
  done
  windows_robocopy "$LOCALES_DIR" "$APP_DIR/locales" /MIR
fi
mkdir -p "$APP_DIR/dist"
windows_robocopy "$GPUI_DIR/dist/sidebar" "$APP_DIR/dist/sidebar" /MIR

COMPONENT_ROOT="${GHOSTEX_ON_DEMAND_COMPONENT_ROOT:-$REPO_ROOT/build/on-demand-components}"
COMPONENT_ASSET_DIR="${GHOSTEX_ON_DEMAND_COMPONENT_ASSET_DIR:-$COMPONENT_ROOT/assets}"
COMPONENT_MANIFEST="${GHOSTEX_ON_DEMAND_COMPONENTS_MANIFEST:-$COMPONENT_ROOT/components.json}"
if [[ "$ON_DEMAND_COMPONENTS" == "1" ]]; then
  CEF_STAGE="$(mktemp -d "$GPUI_DIR/build/cef-windows-component-XXXXXX")"
  CEF_ASSET="$COMPONENT_ASSET_DIR/cef-$CEF_COMPONENT_VERSION-windows-$RELEASE_ARCH.tar.gz"
  mkdir -p "$COMPONENT_ASSET_DIR"
  mkdir -p "$(dirname "$COMPONENT_MANIFEST")"
  printf '{"components":{}}\n' >"$COMPONENT_MANIFEST"
  for source_root in "$CEF_RELEASE" "$CEF_RESOURCES"; do
    find "$source_root" -maxdepth 1 -type f \
      \( -iname '*.dll' -o -iname '*.pak' -o -iname '*.dat' -o -iname '*.bin' \) \
      -exec cp -f -- {} "$CEF_STAGE/" \;
  done
  for swiftshader_icd in "$CEF_RELEASE/vk_swiftshader_icd.json" "$CEF_RESOURCES/vk_swiftshader_icd.json"; do
    if [[ -f "$swiftshader_icd" ]]; then
      cp "$swiftshader_icd" "$CEF_STAGE/"
      break
    fi
  done
  cp -R "$LOCALES_DIR" "$CEF_STAGE/locales"
  "$REPO_ROOT/tooling/release-gpui/create-deterministic-tar.sh" "$CEF_STAGE" "$CEF_ASSET" --windows-component
  rm -rf "$CEF_STAGE"
  node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs" \
    --metadata-only \
    --reuse-published \
    --component cef \
    --version "$CEF_COMPONENT_VERSION" \
    --platform "windows-$RELEASE_ARCH" \
    --asset-dir "$COMPONENT_ASSET_DIR" \
    --output "$COMPONENT_MANIFEST"
fi

stage_wsl_archive() {
  local source_archive="$1"
  local staged_name="$2"
  local staged_archive="$APP_DIR/resources/wsl/$staged_name"
  if [[ ! -f "$source_archive" ]]; then
    if [[ "${GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME:-1}" == "0" ]]; then
      return 0
    fi
    echo "Required WSL runtime archive is missing: $source_archive" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$staged_archive")"
  windows_robocopy "$(dirname "$source_archive")" "$(dirname "$staged_archive")" "$(basename "$source_archive")"
  if [[ "$(basename "$source_archive")" != "$staged_name" ]]; then
    mv -f -- "$(dirname "$staged_archive")/$(basename "$source_archive")" "$staged_archive"
  fi
  windows_sha256 "$source_archive" >"$staged_archive.sha256"
}

stage_verified_code_server_archive() {
  local source_archive="$1"
  local source_sidecar="$source_archive.sha256"
  local staged_dir="$APP_DIR/resources/wsl"
  mkdir -p "$staged_dir"
  windows_robocopy \
    "$(dirname "$source_archive")" \
    "$staged_dir" \
    "$(basename "$source_archive")" \
    "$(basename "$source_sidecar")"
}

stage_current_wsl_runtime_archive() {
  local source_archive="$1"
  local staged_name="$2"
  local staged_archive="$APP_DIR/resources/wsl/$staged_name"
  local package_dir staged_temp_archive package_cleanup_command
  local base_identity current_fingerprint package_version source_dirty source_revision
  if [[ ! -f "$source_archive" ]]; then
    if [[ "${GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME:-1}" == "0" ]]; then
      return 0
    fi
    echo "Required WSL runtime archive is missing: $source_archive" >&2
    exit 1
  fi
  # Extract and recompress on WSL's native filesystem. Doing this temporary
  # package work under /mnt/c turns every archive entry into a slow p9 call.
  package_dir="$(mktemp -d /tmp/ghostex-windows-wsl-package.XXXXXX)"
  staged_temp_archive="$(mktemp --suffix=.tar.gz /tmp/ghostex-windows-wsl-runtime.XXXXXX)"
  printf -v package_cleanup_command 'rm -rf -- %q; rm -f -- %q' \
    "$package_dir" "$staged_temp_archive"
  trap "$package_cleanup_command" EXIT

  tar -xzf "$source_archive" -C "$package_dir"
  if [[ ! -x "$package_dir/bin/gxserver" ]]; then
    echo "The base WSL runtime archive does not contain bin/gxserver." >&2
    exit 1
  fi
  if [[ -n "${GHOSTEX_WINDOWS_WSL_BEADS_BINARY:-}" ]]; then
    if [[ ! -x "$GHOSTEX_WINDOWS_WSL_BEADS_BINARY" ]]; then
      echo "Configured WSL Beads binary is not executable: $GHOSTEX_WINDOWS_WSL_BEADS_BINARY" >&2
      exit 1
    fi
    cp "$GHOSTEX_WINDOWS_WSL_BEADS_BINARY" "$package_dir/bin/bd"
    chmod 755 "$package_dir/bin/bd"
  elif [[ ! -x "$package_dir/bin/bd" ]]; then
    echo "The base WSL runtime archive does not contain executable bin/bd." >&2
    exit 1
  fi
  node "$REPO_ROOT/tooling/smoke-test-packaged-beads.mjs" "$package_dir/bin/bd"
  cp "$WSL_GXSERVER_CURRENT_BIN" "$package_dir/bin/gxserver"
  cp "$WSL_ZMX_CURRENT_BIN" "$package_dir/bin/zmx"
  chmod 755 "$package_dir/bin/gxserver"
  chmod 755 "$package_dir/bin/zmx"

  # The package identity participates in gxserver's restart decision. Keeping
  # the base archive's identity after replacing its daemon would let an older
  # running daemon look current and survive an app update. Seal the identity
  # from both the base package and the exact replacement binaries.
  base_identity="$(cat "$package_dir/build-identity.json" 2>/dev/null || true)"
  package_version="$(node -e 'const value=JSON.parse(process.argv[1]||"{}").packageVersion; process.stdout.write(typeof value==="string"&&value?value:"0.1.0")' "$base_identity")"
  current_fingerprint="sha256:$(
    {
      printf '%s\0' "$base_identity"
      sha256sum "$package_dir/bin/gxserver" "$package_dir/bin/zmx" "$package_dir/bin/bd"
    } | sha256sum | awk '{print $1}'
  )"
  source_revision="$(git -C "$REPO_ROOT" rev-parse HEAD)"
  source_dirty=false
  if ! git -C "$REPO_ROOT" diff --quiet --ignore-submodules -- ||
    ! git -C "$REPO_ROOT" diff --cached --quiet --ignore-submodules --; then
    source_dirty=true
  fi
  GHOSTEX_PACKAGE_IDENTITY_PATH="$package_dir/build-identity.json" \
    GHOSTEX_PACKAGE_VERSION="$package_version" \
    GHOSTEX_PACKAGE_FINGERPRINT="$current_fingerprint" \
    GHOSTEX_PACKAGE_SOURCE_DIRTY="$source_dirty" \
    GHOSTEX_PACKAGE_SOURCE_REVISION="$source_revision" \
    node -e 'const fs=require("node:fs"); const version=process.env.GHOSTEX_PACKAGE_VERSION; const fingerprint=process.env.GHOSTEX_PACKAGE_FINGERPRINT; fs.writeFileSync(process.env.GHOSTEX_PACKAGE_IDENTITY_PATH, JSON.stringify({buildIdentity:`gxserver:${version}:${fingerprint}`,fingerprint,packageVersion:version,sourceDirty:process.env.GHOSTEX_PACKAGE_SOURCE_DIRTY==="true",sourceRevision:process.env.GHOSTEX_PACKAGE_SOURCE_REVISION},null,2)+"\n")'

  mkdir -p "$(dirname "$staged_archive")"
  tar -czf "$staged_temp_archive" -C "$package_dir" .
  sha256sum "$staged_temp_archive" | awk '{print $1}' >"$staged_archive.sha256"
  cp -f -- "$staged_temp_archive" "$staged_archive"

  rm -rf -- "$package_dir"
  rm -f -- "$staged_temp_archive"
  trap - EXIT
}

WSL_GXSERVER_ARCHIVE="${GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE:-}"
WSL_CODE_SERVER_ARCHIVE="${GHOSTEX_WINDOWS_WSL_CODE_SERVER_ARCHIVE:-}"
CODE_SERVER_VERSION="$(node "$REPO_ROOT/tooling/release-gpui/code-server-component-identity.mjs" --root "$REPO_ROOT/.dependencies/code-server")"
if [[ -z "$CODE_SERVER_VERSION" ]]; then
  echo "Could not resolve the code-server component payload identity." >&2
  exit 1
fi
if [[ -n "${GHOSTEX_CODE_SERVER_COMPONENT_VERSION:-}" && "$GHOSTEX_CODE_SERVER_COMPONENT_VERSION" != "$CODE_SERVER_VERSION" ]]; then
  echo "Configured code-server component version does not match its Node payload identity." >&2
  exit 1
fi
CODE_SERVER_PLATFORM="linux-$RELEASE_ARCH"
CODE_SERVER_ARCHIVE_NAME="code-server-$CODE_SERVER_VERSION-$CODE_SERVER_PLATFORM.tar.gz"
if [[ -n "$WSL_CODE_SERVER_ARCHIVE" && -f "$WSL_CODE_SERVER_ARCHIVE" ]]; then
  if [[ "$(basename "$WSL_CODE_SERVER_ARCHIVE")" != "$CODE_SERVER_ARCHIVE_NAME" ]]; then
    echo "WSL code-server archive identity mismatch: expected $CODE_SERVER_ARCHIVE_NAME." >&2
    exit 1
  fi
  node "$REPO_ROOT/tooling/release-gpui/verify-code-server-archive.mjs" \
    --archive "$WSL_CODE_SERVER_ARCHIVE" \
    --version "$CODE_SERVER_VERSION" \
    --platform "$CODE_SERVER_PLATFORM"
elif [[ "$ON_DEMAND_COMPONENTS" == "1" || "${GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME:-1}" != "0" ]]; then
  echo "Required WSL Source archive is missing: $WSL_CODE_SERVER_ARCHIVE" >&2
  exit 1
fi
report_build_phase "Packaging the bundled WSL runtime archives..."
stage_current_wsl_runtime_archive "$WSL_GXSERVER_ARCHIVE" "gxserver-linux-$RELEASE_ARCH.tar.gz"
if [[ "$ON_DEMAND_COMPONENTS" == "1" ]]; then
  CODE_SERVER_STAGE="$(mktemp -d "$GPUI_DIR/build/code-server-windows-component-XXXXXX")"
  CODE_SERVER_ASSET="$COMPONENT_ASSET_DIR/code-server-$CODE_SERVER_VERSION-windows-$RELEASE_ARCH.tar.gz"
  cp "$WSL_CODE_SERVER_ARCHIVE" "$CODE_SERVER_STAGE/$CODE_SERVER_ARCHIVE_NAME"
  cp "$WSL_CODE_SERVER_ARCHIVE.sha256" "$CODE_SERVER_STAGE/$CODE_SERVER_ARCHIVE_NAME.sha256"
  "$REPO_ROOT/tooling/release-gpui/create-deterministic-tar.sh" "$CODE_SERVER_STAGE" "$CODE_SERVER_ASSET" --windows-component
  rm -rf "$CODE_SERVER_STAGE"
  CODE_SERVER_ASSET_SHA256="$(sha256sum "$CODE_SERVER_ASSET" | awk '{print $1}')"
  printf '%s  %s\n' "$CODE_SERVER_ASSET_SHA256" "$(basename "$CODE_SERVER_ASSET")" >"$CODE_SERVER_ASSET.sha256"
  node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs" \
    --metadata-only \
    --reuse-published \
    --component code-server \
    --version "$CODE_SERVER_VERSION" \
    --platform "windows-$RELEASE_ARCH" \
    --asset-dir "$COMPONENT_ASSET_DIR" \
    --require-sha256-sidecars \
    --output "$COMPONENT_MANIFEST"
  ON_DEMAND_BUILD_MANIFEST="$COMPONENT_ROOT/windows-$RELEASE_ARCH-assets.json"
  node -e 'const fs=require("node:fs");fs.writeFileSync(process.argv[1],JSON.stringify({assets:[],version:process.argv[2]},null,2)+"\n")' \
    "$ON_DEMAND_BUILD_MANIFEST" "$RELEASE_VERSION"
  mkdir -p "$APP_DIR/resources"
  node "$REPO_ROOT/tooling/release-gpui/on-demand-manifest.mjs" seal \
    --build-manifest "$ON_DEMAND_BUILD_MANIFEST" \
    --component-manifest "$COMPONENT_MANIFEST" \
    --output "$APP_DIR/resources/on-demand-resources.json" \
    --repo maddada/Ghostex
else
  stage_verified_code_server_archive "$WSL_CODE_SERVER_ARCHIVE"
fi

echo "Staged $APP_DIR"
